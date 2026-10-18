import assert from 'node:assert/strict';
import { after, afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import { build, stop as stopEsbuild } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);

// Same native-boundary mocks as the existing Fold preview scheduling harness.
const bundle = await build({
  stdin: {
    contents: `export { useImageLoader, decodeInitialProfileDefault, adoptInitialProfileDefault } from './src/hooks/useImageLoader';
      export { useImageProcessing } from './src/hooks/useImageProcessing';
      export { debouncedSave } from './src/hooks/useEditorActions';
      export { useEditorStore } from './src/store/useEditorStore';
      export { useUIStore } from './src/store/useUIStore';
      export { useSettingsStore } from './src/store/useSettingsStore';
      export { useLibraryStore } from './src/store/useLibraryStore';
      export { Invokes, Panel } from './src/components/ui/AppProperties';
      export { INITIAL_ADJUSTMENTS } from './src/utils/adjustments';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'initial-default-native-boundaries',
      setup(builder) {
        const mocked = new Map([
          [
            '@tauri-apps/api/core',
            `export const invoke = (...args) => globalThis.__initialDefaultInvoke(...args); export const convertFileSrc = x => x;`,
          ],
          ['@tauri-apps/api/event', `export const listen = async () => () => {}; export const emit = async () => {};`],
          ['@tauri-apps/plugin-dialog', `export const open = async () => null; export const save = async () => null;`],
          ['@tauri-apps/api/path', `export const homeDir = async () => '/';`],
          ['@tauri-apps/api/window', `export const getCurrentWindow = () => ({});`],
          ['@tauri-apps/plugin-os', `export const platform = () => 'linux';`],
          ['react-toastify', `export const toast = new Proxy({}, { get: () => () => {} });`],
          ['react-i18next', `export const useTranslation = () => ({ t: key => key, i18n: { language: 'en' } });`],
        ]);
        builder.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/api\/event|@tauri-apps\/api\/window|@tauri-apps\/plugin-dialog|@tauri-apps\/api\/path|@tauri-apps\/plugin-os|react-toastify|react-i18next)$/,
          },
          (args) => ({ path: args.path, namespace: 'initial-default-mock' }),
        );
        builder.onLoad({ filter: /.*/, namespace: 'initial-default-mock' }, (args) => ({
          contents: mocked.get(args.path),
          loader: 'js',
        }));
        builder.onResolve({ filter: /\.css$/ }, (args) => ({ path: args.path, namespace: 'initial-default-css' }));
        builder.onLoad({ filter: /.*/, namespace: 'initial-default-css' }, () => ({
          contents: 'export default {}',
          loader: 'js',
        }));
      },
    },
  ],
});

const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  adoptInitialProfileDefault,
  debouncedSave,
  decodeInitialProfileDefault,
  INITIAL_ADJUSTMENTS,
  Invokes,
  Panel,
  useEditorStore,
  useImageLoader,
  useImageProcessing,
  useLibraryStore,
  useSettingsStore,
  useUIStore,
} = mod.exports;
const { create, act } = require('react-test-renderer');

globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.window = {
  devicePixelRatio: 1,
  innerWidth: 1024,
  innerHeight: 768,
  addEventListener() {},
  removeEventListener() {},
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  setTimeout,
  clearTimeout,
};
globalThis.document = {
  addEventListener() {},
  removeEventListener() {},
  documentElement: {},
  body: { appendChild() {}, removeChild() {} },
};

const BASE = '0123456789abcdef'.repeat(4);
const OTHER = 'f'.repeat(64);
const TICKET = '33333333-3333-4333-8333-333333333333';
const KEY = { ticket: TICKET, generation: '7' };
const PATH = '/fresh/DSCF0001.RAF';

const calls = [];
let invokeHandler = async () => null;
globalThis.__initialDefaultInvoke = (command, args) => {
  calls.push({ command, args });
  return invokeHandler(command, args);
};
const saves = () => calls.filter((call) => call.command === Invokes.SaveMetadataAndUpdateThumbnail);

function nativeOpen({ admissionAdjustments = null, initialProfileDefault } = {}) {
  return async (command, args) => {
    if (command === Invokes.BeginImageOpen) return { openKey: KEY, metadata: { adjustments: admissionAdjustments } };
    if (command === Invokes.LoadImage) {
      return {
        openKey: args.openKey,
        width: 4000,
        height: 3000,
        metadata: { adjustments: admissionAdjustments },
        exif: null,
        is_raw: true,
        as_shot_white_balance: null,
        renderMode: 'AccurateXtrans1Encoded',
        ...(initialProfileDefault === undefined ? {} : { initialProfileDefault }),
      };
    }
    if (command === Invokes.ApplyAdjustments) return new TextEncoder().encode('JPEG').buffer;
    return null;
  };
}

function resetStores() {
  useUIStore.setState({ activeView: 'editor', activePanel: Panel.Adjustments });
  useEditorStore.setState({
    selectedImage: null,
    progressiveOpen: null,
    pendingOpenIntent: null,
    adjustments: INITIAL_ADJUSTMENTS,
    history: [INITIAL_ADJUSTMENTS],
    historyIndex: 0,
    finalPreviewUrl: null,
    uncroppedAdjustedPreviewUrl: null,
    previewOverride: null,
    hasRenderedFirstFrame: false,
  });
  useLibraryStore.setState({ multiSelectedPaths: [] });
  useSettingsStore.setState({ appSettings: null });
  calls.length = 0;
  invokeHandler = async () => null;
  debouncedSave.cancel();
}

function pendingIntent() {
  const store = useEditorStore.getState();
  store.prepareProgressiveIntent(TICKET, PATH);
  assert.equal(store.activateProgressiveIntent(TICKET, PATH, `thumb:${PATH}`), true);
}

function admitted(baseline) {
  pendingIntent();
  assert.equal(useEditorStore.getState().acceptProgressiveAdmission(TICKET, PATH, KEY, null, baseline), true);
}

function Probe() {
  const cachedRef = React.useRef(null);
  const transformWrapperRef = React.useRef(null);
  const prevAdjustmentsRef = React.useRef(null);
  const previewJobIdRef = React.useRef(0);
  const latestRenderedJobIdRef = React.useRef(0);
  const currentResRef = React.useRef(1920);
  useImageLoader(cachedRef);
  useImageProcessing(transformWrapperRef, prevAdjustmentsRef, {
    previewJobIdRef,
    latestRenderedJobIdRef,
    currentResRef,
  });
  return null;
}

const roots = new Set();
async function mount() {
  let root;
  await act(async () => {
    root = create(React.createElement(Probe));
    for (let index = 0; index < 32; index++) await Promise.resolve();
  });
  roots.add(root);
  return root;
}

async function wait(ms) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

afterEach(async () => {
  for (const root of [...roots]) {
    roots.delete(root);
    await act(async () => root.unmount());
  }
  debouncedSave.cancel();
});

after(async () => {
  await stopEsbuild();
});

test('native provenance decodes only an exact lowercase {base} record', () => {
  assert.equal(decodeInitialProfileDefault({ base: BASE }), BASE);
  for (const invalid of [
    undefined,
    null,
    BASE,
    [BASE],
    {},
    { base: BASE.toUpperCase() },
    { base: BASE.slice(1) },
    { base: BASE, look: null },
    { base: null },
  ]) {
    assert.equal(decodeInitialProfileDefault(invalid), null, JSON.stringify(invalid));
  }
});

test('adoption replaces only the current untouched unselected admission baseline', () => {
  resetStores();
  admitted({ ...INITIAL_ADJUSTMENTS });
  assert.equal(adoptInitialProfileDefault(TICKET, PATH, { ...KEY, generation: '8' }, BASE), false, 'stale key');
  assert.equal(adoptInitialProfileDefault(TICKET, '/other.RAF', KEY, BASE), false, 'other path');
  assert.equal(adoptInitialProfileDefault(TICKET, PATH, KEY, BASE), true);
  const state = useEditorStore.getState();
  assert.equal(state.adjustments.profile.base, BASE);
  assert.equal(state.adjustments.profile.look, null);
  assert.equal(state.adjustments.profile.amount, 100);
  assert.equal(state.history.length, 1);
  assert.equal(state.history[0], state.adjustments);
  assert.equal(INITIAL_ADJUSTMENTS.profile.base, null, 'shared defaults are never mutated');

  for (const [label, baseline] of [
    ['saved base', { ...INITIAL_ADJUSTMENTS, profile: { base: OTHER, look: null, amount: 100 } }],
    ['explicit look', { ...INITIAL_ADJUSTMENTS, profile: { base: null, look: 'look-uuid', amount: 100 } }],
  ]) {
    resetStores();
    admitted(baseline);
    assert.equal(adoptInitialProfileDefault(TICKET, PATH, KEY, BASE), false, label);
    assert.equal(useEditorStore.getState().adjustments, baseline, label);
  }

  resetStores();
  admitted({ ...INITIAL_ADJUSTMENTS });
  useEditorStore.setState((s) => ({ history: [...s.history, { ...s.adjustments, exposure: 1 }] }));
  assert.equal(adoptInitialProfileDefault(TICKET, PATH, KEY, BASE), false, 'edited history');

  resetStores();
  admitted({ ...INITIAL_ADJUSTMENTS });
  assert.equal(
    useEditorStore.getState().completeProgressiveLoad(TICKET, PATH, KEY, {
      width: 4000,
      height: 3000,
      previewSize: { width: 1920, height: 1440 },
      exif: null,
      is_raw: true,
      as_shot_white_balance: null,
      renderMode: 'AccurateXtrans1Encoded',
    }),
    true,
  );
  assert.equal(adoptInitialProfileDefault(TICKET, PATH, KEY, BASE), false, 'already ready');
});

test('mounted fresh open shows the default as baseline, never saves on view, then persists on first edit', async () => {
  resetStores();
  pendingIntent();
  invokeHandler = nativeOpen({ initialProfileDefault: { base: BASE } });
  await mount();
  await wait(450);

  const state = useEditorStore.getState();
  assert.equal(state.selectedImage.isReady, true);
  assert.equal(state.adjustments.profile.base, BASE);
  assert.equal(state.history.length, 1);
  assert.equal(state.history[0].profile.base, BASE);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);
  assert.equal(saves().length, 0, 'viewing never writes a sidecar');

  await act(async () => {
    useEditorStore.getState().setEditor((s) => ({ adjustments: { ...s.adjustments, exposure: 0.5 } }));
  });
  await wait(450);
  assert.equal(saves().length, 1, 'first real edit uses the normal autosave');
  assert.equal(saves()[0].args.path, PATH);
  assert.equal(saves()[0].args.adjustments.profile.base, BASE);
  assert.equal(saves()[0].args.adjustments.exposure, 0.5);
});

test('mounted open without native provenance keeps the unselected baseline and existing save timing', async () => {
  resetStores();
  pendingIntent();
  invokeHandler = nativeOpen();
  await mount();
  await wait(450);
  assert.equal(useEditorStore.getState().selectedImage.isReady, true);
  assert.equal(useEditorStore.getState().adjustments.profile.base, null);
  assert.equal(saves().length, 0);

  await act(async () => {
    useEditorStore.getState().setEditor((s) => ({ adjustments: { ...s.adjustments, exposure: 0.5 } }));
  });
  await wait(450);
  assert.equal(saves().length, 1);
  assert.equal(saves()[0].args.adjustments.profile.base, null);
});
