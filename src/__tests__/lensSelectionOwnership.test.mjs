import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);
const { create, act } = require('react-test-renderer');
const english = JSON.parse(await readFile(new URL('../i18n/locales/en.json', import.meta.url), 'utf8'));
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = {
  activeElement: { tagName: 'BODY' },
  addEventListener() {},
  removeEventListener() {},
};
globalThis.window = {
  addEventListener() {},
  removeEventListener() {},
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  clearTimeout,
};
globalThis.requestAnimationFrame = () => 1;
globalThis.cancelAnimationFrame = () => {};

function translate(key, values = {}) {
  const result = key.split('.').reduce((value, part) => value?.[part], english) ?? key;
  return String(result).replace(/{{(\w+)}}/g, (_, name) => String(values[name] ?? ''));
}

let mounted;
let calls = [];
let contextMenuItems = [];
let invocationQueues = new Map();
const defaultExif = {
  Make: 'Maker',
  LensModel: 'Lens 35',
  Model: 'Body 1',
  FocalLength: '35 mm',
  FNumber: '2',
  SubjectDistance: '2 m',
};
const usableParams = {
  k1: 0.12,
  k2: -0.01,
  k3: 0,
  model: 0,
  tca_vr: 1,
  tca_vb: 1,
  vig_k1: 0,
  vig_k2: 0,
  vig_k3: 0,
};
const oldParams = { ...usableParams, k1: 0.2 };

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function queueInvocation(name, value) {
  const queue = invocationQueues.get(name) ?? [];
  queue.push(value);
  invocationQueues.set(name, queue);
}

globalThis.__lensInvoke = (name, args) => {
  calls.push({ name, args });
  const queue = invocationQueues.get(name);
  if (queue?.length) return queue.shift();
  switch (name) {
    case 'get_lensfun_makers':
      return Promise.resolve(['Maker', 'Other Maker', 'Saved Maker']);
    case 'load_settings':
      return Promise.resolve({ myLenses: [{ maker: 'Saved Maker', model: 'Saved Lens' }] });
    case 'get_lensfun_lenses_for_maker':
      return Promise.resolve(args.maker === 'Maker' ? ['Lens 35', 'Lens 50'] : ['Saved Lens']);
    case 'autodetect_lens':
      return Promise.resolve(['Maker', 'Lens 35']);
    case 'get_lens_distortion_params':
      return Promise.resolve(usableParams);
    default:
      return Promise.resolve(null);
  }
};
globalThis.__lensShowContextMenu = (_x, _y, items) => {
  contextMenuItems = items;
};
globalThis.__lensToast = Object.fromEntries(['success', 'error', 'info'].map((kind) => [kind, () => {}]));

const S = '/Users/harrisontucker/.codex/worktrees/rapidraw-upstream-integration/RapidRaw';
const E = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const owned = new Set(['src/components/panel/right/CropPanel.tsx', 'src/hooks/useEditorActions.ts']);
const bundle = await build({
  absWorkingDir: S,
  jsx: 'automatic',
  stdin: {
    contents: `
      export { default as CropPanel } from './src/components/panel/right/CropPanel';
      export { useEditorStore } from './src/store/useEditorStore';
      export { useUIStore } from './src/store/useUIStore';
      export { useSettingsStore } from './src/store/useSettingsStore';
      export { useEditorActions, debouncedSetHistory, debouncedSave } from './src/hooks/useEditorActions';
      export { INITIAL_ADJUSTMENTS } from './src/utils/adjustments';
    `,
    resolveDir: S,
  },
  metafile: true,
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'crop-panel-native-boundaries',
      setup(buildApi) {
        buildApi.onResolve({ filter: /^\./ }, (args) => {
          if (args.path.endsWith('ContextMenuContext')) return;
          const importer = args.importer.startsWith(E + path.sep) ? S + args.importer.slice(E.length) : args.importer;
          const base = path.resolve(
            path.isAbsolute(importer) ? path.dirname(importer) : args.resolveDir || S,
            args.path,
          );
          for (const ext of ['', '.ts', '.tsx', '.js', '.mjs']) {
            const actual = base + ext;
            const rel = path.relative(S, actual);
            if (owned.has(rel)) return { path: path.join(E, rel) };
            if (args.importer.startsWith(E + path.sep) && existsSync(actual)) return { path: actual };
          }
        });
        buildApi.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/plugin-os|react-i18next|react-toastify|framer-motion)$|ContextMenuContext$/,
          },
          (args) => ({ path: args.path, namespace: 'lens-native-mock' }),
        );
        buildApi.onLoad({ filter: /.*/, namespace: 'lens-native-mock' }, ({ path }) => ({
          contents:
            path === '@tauri-apps/api/core'
              ? 'export const invoke = (...args) => globalThis.__lensInvoke(...args);'
              : path === '@tauri-apps/plugin-os'
                ? "export const platform = () => 'android';"
                : path === 'react-i18next'
                  ? 'export const useTranslation = () => ({ t: (...args) => globalThis.__lensTranslate(...args) }); export const Trans = ({ children }) => children;'
                  : path === 'react-toastify'
                    ? 'export const toast = globalThis.__lensToast;'
                    : path.endsWith('ContextMenuContext')
                      ? 'export const useContextMenu = () => ({ showContextMenu: (...args) => globalThis.__lensShowContextMenu(...args) });'
                      : 'export const motion = new Proxy({}, { get: (_, tag) => tag }); export const AnimatePresence = ({ children }) => children;',
        }));
      },
    },
  ],
});

for (const ownedPath of owned)
  assert.ok(
    Object.keys(bundle.metafile.inputs).some((p) => path.resolve(S, p) === path.join(E, ownedPath)),
    'actual current-port candidate bundled: ' + ownedPath,
  );
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  CropPanel,
  useEditorStore,
  useUIStore,
  useSettingsStore,
  useEditorActions,
  debouncedSetHistory,
  debouncedSave,
  INITIAL_ADJUSTMENTS,
} = mod.exports;

globalThis.__lensTranslate = translate;
let setAdjustments;

function ActionsProbe() {
  setAdjustments = useEditorActions().setAdjustments;
  return null;
}

function reset({ path = '/synthetic/lens-A.RAF', exif = defaultExif, lensMaker = 'Old Maker' } = {}) {
  calls = [];
  contextMenuItems = [];
  invocationQueues = new Map();
  setAdjustments = undefined;
  const adjustments = {
    ...INITIAL_ADJUSTMENTS,
    exposure: 0.75,
    aspectRatio: 1.5,
    lensCorrectionMode: 'manual',
    lensMaker,
    lensModel: 'Old Lens',
    lensDistortionParams: oldParams,
    lensDistortionAmount: 137,
  };
  useEditorStore.setState({
    selectedImage: {
      path,
      isReady: true,
      width: 3000,
      height: 2000,
      exif,
    },
    adjustments,
    history: [adjustments],
    historyIndex: 0,
    progressiveOpen: null,
    pendingOpenIntent: null,
    previewOverride: null,
    showOriginal: false,
  });
  const ui = useUIStore.getState();
  useUIStore.setState({
    ...ui,
    cropSectionsState: { ...ui.cropSectionsState, lens: true },
  });
  useSettingsStore.setState({ appSettings: null });
}

async function flushMicrotasks() {
  for (let index = 0; index < 6; index += 1) await Promise.resolve();
}

async function mountPanel() {
  await act(async () => {
    mounted = create(
      React.createElement(React.Fragment, null, React.createElement(CropPanel), React.createElement(ActionsProbe)),
    );
    await flushMicrotasks();
  });
  return mounted;
}

async function unmountPanel() {
  if (mounted) await act(async () => mounted.unmount());
  mounted = null;
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
}

afterEach(async () => {
  await unmountPanel();
});

const text = (node) =>
  (node.children ?? [])
    .map((child) => (typeof child === 'string' ? child : typeof child === 'number' ? String(child) : text(child)))
    .join('');

function findButton(root, label) {
  return root.root.findAllByType('button').find((node) => text(node) === label || node.props['aria-label'] === label);
}

async function click(node) {
  assert.ok(node, 'expected the actual mounted control');
  await act(async () => {
    node.props.onClick?.();
    await flushMicrotasks();
  });
}

async function selectDropdown(root, placeholder, optionLabel) {
  await click(findButton(root, translate(placeholder)));
  const option = root.root.findAllByProps({ role: 'option' }).find((node) => text(node) === optionLabel);
  await click(option);
}

function currentAdjustments() {
  return useEditorStore.getState().adjustments;
}

function statusText(root) {
  return root.root.findAllByProps({ role: 'status' }).map(text).join(' ');
}

let keyCounter = 0;
async function navigate(imagePath, exif = defaultExif) {
  let key;
  await act(async () => {
    const store = useEditorStore.getState();
    const adjustments = store.adjustments;
    const ticket = '12345678-1234-1234-1234-' + String(++keyCounter).padStart(12, '0');
    key = { ticket, generation: String(keyCounter) };
    store.prepareProgressiveIntent(ticket, imagePath);
    assert.equal(store.activateProgressiveIntent(ticket, imagePath, 'data:image/jpeg;base64,AA=='), true);
    assert.equal(store.acceptProgressiveAdmission(ticket, imagePath, key, {}, adjustments), true);
    assert.equal(
      store.completeProgressiveLoad(ticket, imagePath, key, {
        width: 3000,
        height: 2000,
        exif,
        is_raw: true,
        renderMode: 'AccurateXtrans1Encoded',
      }),
      true,
    );
    await flushMicrotasks();
  });
  return key;
}

async function flushHistory() {
  await act(async () => debouncedSetHistory.flush());
}

test('lens_auto_supported_result_commits_once_and_undo_restores_previous_lens', async () => {
  reset();
  const root = await mountPanel();
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => flushMicrotasks());

  assert.equal(currentAdjustments().lensCorrectionMode, 'auto');
  assert.equal(currentAdjustments().lensMaker, 'Maker');
  assert.equal(currentAdjustments().lensModel, 'Lens 35');
  assert.equal(currentAdjustments().exposure, 0.75);
  assert.match(statusText(root), /Maker - Lens 35/);
  assert.deepEqual(calls.find((call) => call.name === 'autodetect_lens').args, {
    maker: 'Maker',
    model: 'Lens 35',
    cameraModel: 'Body 1',
  });
  assert.deepEqual(calls.find((call) => call.name === 'get_lens_distortion_params').args, {
    maker: 'Maker',
    model: 'Lens 35',
    focalLength: 35,
    aperture: 2,
    distance: 2,
  });

  await flushHistory();
  assert.equal(useEditorStore.getState().history.length, 2);
  await act(async () => useEditorStore.getState().undo());
  assert.equal(currentAdjustments().lensCorrectionMode, 'manual');
  assert.equal(currentAdjustments().lensModel, 'Old Lens');
});

test('lens_auto_missing_metadata_no_match_null_calibration_and_transport_error_are_distinct', async () => {
  reset({ exif: {} });
  let root = await mountPanel();
  const beforeMissing = currentAdjustments();
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  assert.match(statusText(root), /Camera or lens metadata is missing/);
  assert.deepEqual(currentAdjustments(), beforeMissing);
  assert.equal(
    calls.some((call) => call.name === 'autodetect_lens'),
    false,
  );

  await unmountPanel();
  reset();
  const noMatch = deferred();
  queueInvocation('autodetect_lens', noMatch.promise);
  root = await mountPanel();
  const beforeNoMatch = currentAdjustments();
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  noMatch.resolve(null);
  await act(async () => flushMicrotasks());
  assert.match(statusText(root), /Lens profile not found/);
  assert.deepEqual(currentAdjustments(), beforeNoMatch);

  const nullCalibration = deferred();
  queueInvocation('get_lens_distortion_params', nullCalibration.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  nullCalibration.resolve(null);
  await act(async () => flushMicrotasks());
  assert.match(statusText(root), /no usable correction data/i);
  assert.deepEqual(currentAdjustments(), beforeNoMatch);

  const transportFailure = deferred();
  queueInvocation('autodetect_lens', transportFailure.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  transportFailure.reject(new Error('transport offline'));
  await act(async () => flushMicrotasks());
  assert.match(statusText(root), /Could not load lens correction data/);
  assert.deepEqual(currentAdjustments(), beforeNoMatch);
});

test('lens_auto_A_to_B_and_A_B_A_navigation_discards_old_results', async () => {
  reset();
  const firstMatch = deferred();
  queueInvocation('autodetect_lens', firstMatch.promise);
  const root = await mountPanel();
  const beforeA = currentAdjustments();
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await navigate('/synthetic/lens-B.RAF', { ...defaultExif, LensModel: 'Lens B', Model: 'Body B' });
  const beforeB = currentAdjustments();
  firstMatch.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.deepEqual(currentAdjustments(), beforeB);
  assert.equal(calls.filter((call) => call.name === 'get_lens_distortion_params').length, 0);

  await navigate('/synthetic/lens-A.RAF', defaultExif);
  const secondMatch = deferred();
  queueInvocation('autodetect_lens', secondMatch.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await navigate('/synthetic/lens-B.RAF', { ...defaultExif, LensModel: 'Lens B', Model: 'Body B' });
  await navigate('/synthetic/lens-A.RAF', defaultExif);
  const beforeABA = currentAdjustments();
  secondMatch.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.deepEqual(currentAdjustments(), beforeABA);
  assert.deepEqual(beforeABA, beforeA);
});

test('lens_auto_same_source_manual_edit_and_lens_reset_win_over_pending_result', async () => {
  reset();
  const root = await mountPanel();
  const manualMatch = deferred();
  queueInvocation('autodetect_lens', manualMatch.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => setAdjustments({ lensMaker: 'Manual Maker', lensModel: 'Manual Choice' }));
  manualMatch.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensMaker, 'Manual Maker');
  assert.equal(currentAdjustments().lensModel, 'Manual Choice');
  assert.doesNotMatch(statusText(root), /Detecting EXIF/i);

  const resetMatch = deferred();
  queueInvocation('autodetect_lens', resetMatch.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  const target = root.root
    .findAll((node) => node && typeof node.props?.onContextMenu === 'function')
    .find(
      (node) =>
        node.props.className?.includes('bg-surface') && text(node).includes(translate('modals.lensCorrection.title')),
    );
  assert.ok(target, 'actual lens section context menu is mounted');
  await act(async () => {
    target.props.onContextMenu({ preventDefault() {}, stopPropagation() {}, clientX: 0, clientY: 0 });
  });
  assert.equal(contextMenuItems.length, 1);
  await act(async () => contextMenuItems[0].onClick());
  const afterReset = currentAdjustments();
  resetMatch.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensCorrectionMode, INITIAL_ADJUSTMENTS.lensCorrectionMode);
  assert.equal(currentAdjustments().lensMaker, INITIAL_ADJUSTMENTS.lensMaker);
  assert.equal(currentAdjustments().lensModel, INITIAL_ADJUSTMENTS.lensModel);
  assert.deepEqual(currentAdjustments(), afterReset);
});

test('lens_strict_intent_rejects_stale_revision_before_preview_or_history_mutation', async () => {
  reset();
  await mountPanel();
  const store = useEditorStore.getState();
  const fence = {
    path: store.selectedImage.path,
    source: store.selectedImage,
    adjustments: store.adjustments,
    isCurrent: () => useEditorStore.getState().selectedImage === store.selectedImage,
  };
  assert.ok(fence);
  await act(async () => setAdjustments({ exposure: 1.75 }));
  const previewOverride = { exposure: 2.25 };
  useEditorStore.setState({ showOriginal: true, previewOverride });
  const beforeAdjustments = currentAdjustments();
  const beforeHistory = useEditorStore.getState().history;
  const beforeHistoryIndex = useEditorStore.getState().historyIndex;

  let accepted;
  await act(async () => {
    accepted = setAdjustments(
      { lensMaker: 'Stale Maker', lensModel: 'Stale Lens', lensDistortionParams: usableParams },
      fence,
      { strictManualIntent: true },
    );
  });

  assert.equal(accepted, false);
  assert.equal(useEditorStore.getState().showOriginal, true);
  assert.equal(useEditorStore.getState().previewOverride, previewOverride);
  assert.equal(currentAdjustments(), beforeAdjustments);
  assert.equal(useEditorStore.getState().history, beforeHistory);
  assert.equal(useEditorStore.getState().historyIndex, beforeHistoryIndex);

  assert.equal(setAdjustments({ exposure: 1.25 }), undefined);
});

test('lens_manual_model_newer_choice_wins_and_success_uses_real_history', async () => {
  reset({ lensMaker: 'Maker' });
  const root = await mountPanel();
  await act(async () => flushMicrotasks());
  const firstParams = deferred();
  const secondParams = deferred();
  queueInvocation('get_lens_distortion_params', firstParams.promise);
  queueInvocation('get_lens_distortion_params', secondParams.promise);
  await selectDropdown(root, 'modals.lensCorrection.selectLensModel', 'Lens 35');
  await selectDropdown(root, 'modals.lensCorrection.selectLensModel', 'Lens 50');
  secondParams.resolve({ ...usableParams, k1: 0.4 });
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensModel, 'Lens 50');
  firstParams.resolve({ ...usableParams, k1: 0.2 });
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensModel, 'Lens 50');
  assert.equal(currentAdjustments().lensDistortionParams.k1, 0.4);
  await flushHistory();
  assert.equal(useEditorStore.getState().history.length, 2);
  await act(async () => useEditorStore.getState().undo());
  assert.equal(currentAdjustments().lensModel, 'Old Lens');
});

test('lens_saved_lens_list_and_params_are_owned_by_latest_selection', async () => {
  reset();
  const root = await mountPanel();
  await act(async () => flushMicrotasks());
  const firstList = deferred();
  const secondList = deferred();
  const secondParams = deferred();
  queueInvocation('get_lensfun_lenses_for_maker', firstList.promise);
  queueInvocation('get_lensfun_lenses_for_maker', secondList.promise);
  queueInvocation('get_lens_distortion_params', secondParams.promise);

  await selectDropdown(root, 'modals.lensCorrection.chooseSavedLens', 'Saved Maker - Saved Lens');
  await selectDropdown(root, 'modals.lensCorrection.chooseSavedLens', 'Saved Maker - Saved Lens');
  secondList.resolve(['Only latest lens']);
  await act(async () => flushMicrotasks());
  secondParams.resolve(usableParams);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensMaker, 'Saved Maker');
  assert.equal(currentAdjustments().lensModel, 'Saved Lens');

  firstList.resolve(['Stale lens list']);
  await act(async () => flushMicrotasks());
  await click(findButton(root, translate('modals.lensCorrection.selectLensModel')));
  const optionLabels = root.root.findAllByProps({ role: 'option' }).map(text);
  assert.deepEqual(optionLabels, ['Only latest lens']);
  assert.equal(calls.filter((call) => call.name === 'get_lens_distortion_params').length, 1);
});

test('lens_repeated_request_cancels_old_success_timer_and_unmount_blocks_completion', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  reset();
  const root = await mountPanel();
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => flushMicrotasks());
  assert.match(statusText(root), /Maker - Lens 35/);

  const nextMatch = deferred();
  queueInvocation('autodetect_lens', nextMatch.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => t.mock.timers.tick(500));
  assert.match(statusText(root), /Detecting EXIF/);

  nextMatch.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensCorrectionMode, 'auto');
  assert.equal(currentAdjustments().lensModel, 'Lens 35');
  assert.match(statusText(root), /Maker - Lens 35/);

  const beforeUnmount = currentAdjustments();
  const unmountMatch = deferred();
  queueInvocation('autodetect_lens', unmountMatch.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await unmountPanel();
  unmountMatch.resolve(['Maker', 'Lens 50']);
  await act(async () => flushMicrotasks());
  assert.deepEqual(currentAdjustments(), beforeUnmount);
  assert.equal(calls.filter((call) => call.name === 'get_lens_distortion_params').length, 2);
});

// Current-source tests below use actual progressive intent/admission/completion APIs.
test('lens_current_pending_intent_blocks_start_and_pending_clear_cannot_revive_old_request', async () => {
  reset();
  const root = await mountPanel();
  const before = currentAdjustments();
  await act(async () => useEditorStore.getState().prepareProgressiveIntent('pending-one', '/synthetic/next.RAF'));
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  assert.equal(calls.filter((c) => c.name === 'autodetect_lens').length, 0);
  assert.equal(currentAdjustments(), before);
  await act(async () => useEditorStore.getState().failProgressiveIntent('pending-one'));
  const pending = deferred();
  queueInvocation('autodetect_lens', pending.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => {
    useEditorStore.getState().prepareProgressiveIntent('pending-two', before.path || '/synthetic/lens-A.RAF');
    useEditorStore.getState().failProgressiveIntent('pending-two');
  });
  pending.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments(), before);
  assert.equal(calls.filter((c) => c.name === 'get_lens_distortion_params').length, 0);
  assert.doesNotMatch(statusText(root), /Maker - Lens 35|Detecting EXIF/);
});

test('lens_current_native_key_valid_then_same_path_generation_ABA_and_missing_key_refuse', async () => {
  reset();
  const root = await mountPanel();
  const first = await navigate('/synthetic/lens-A.RAF');
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  assert.equal(currentAdjustments().lensModel, 'Lens 35');
  await flushHistory();
  const pending = deferred();
  queueInvocation('autodetect_lens', pending.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  const second = await navigate('/synthetic/lens-A.RAF');
  assert.notDeepEqual(first, second);
  const unchanged = currentAdjustments();
  const paramsCalls = calls.filter((c) => c.name === 'get_lens_distortion_params').length;
  pending.resolve(['Other Maker', 'Foreign Lens']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments(), unchanged);
  assert.equal(calls.filter((c) => c.name === 'get_lens_distortion_params').length, paramsCalls);
  const count = calls.filter((c) => c.name === 'autodetect_lens').length;
  await act(async () => {
    const state = useEditorStore.getState();
    useEditorStore.setState({
      progressiveOpen: { ...state.progressiveOpen, openKey: { ...second, generation: '999' } },
    });
  });
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  assert.equal(
    calls.filter((c) => c.name === 'autodetect_lens').length,
    count,
    'mismatched native key must not downgrade to legacy',
  );
  assert.equal(currentAdjustments(), unchanged);
});

test('lens_current_atomic_pending_guard_preserves_original_preview_history_and_rejects_commit', async () => {
  reset();
  await mountPanel();
  const current = useEditorStore.getState();
  const source = current.selectedImage;
  const adjustment = current.adjustments;
  const intent = { path: source.path, source, adjustments: adjustment, isCurrent: () => true };
  const originalSet = current.setEditor;
  await act(async () =>
    useEditorStore.setState({
      setEditor: (updater) => {
        useEditorStore.getState().prepareProgressiveIntent('atomic-pending', '/synthetic/next.RAF');
        originalSet(updater);
      },
    }),
  );
  const override = { ...adjustment, exposure: 3 };
  await act(async () => useEditorStore.setState({ showOriginal: true, previewOverride: override }));
  const history = useEditorStore.getState().history;
  let accepted;
  try {
    await act(async () => {
      accepted = setAdjustments({ lensModel: 'Stale Lens' }, intent, { strictManualIntent: true });
    });
  } finally {
    await act(async () => useEditorStore.setState({ setEditor: originalSet }));
  }
  assert.equal(accepted, false);
  assert.equal(currentAdjustments(), adjustment);
  assert.equal(useEditorStore.getState().showOriginal, true);
  assert.equal(useEditorStore.getState().previewOverride, override);
  await flushHistory();
  assert.equal(useEditorStore.getState().history, history);
});

test('lens_saved_failed_calibration_preserves_old_mode_and_model_list_without_stuck_loading', async () => {
  reset({ lensMaker: 'Maker' });
  const root = await mountPanel();
  const before = currentAdjustments();
  queueInvocation('get_lensfun_lenses_for_maker', Promise.resolve(['Uncommitted Saved Lens']));
  queueInvocation('get_lens_distortion_params', Promise.resolve(null));
  await selectDropdown(root, 'modals.lensCorrection.chooseSavedLens', 'Saved Maker - Saved Lens');
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments(), before);
  assert.match(statusText(root), /no usable correction data/i);
  assert.doesNotMatch(statusText(root), /Loading lens/);
  await click(findButton(root, translate('modals.lensCorrection.selectLensModel')));
  assert.deepEqual(root.root.findAllByProps({ role: 'option' }).map(text), ['Lens 35', 'Lens 50']);
});

test('lens_manual_edit_and_undo_round_trip_does_not_revive_inflight_auto', async () => {
  reset();
  const root = await mountPanel();
  const before = currentAdjustments();
  const pending = deferred();
  queueInvocation('autodetect_lens', pending.promise);
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  await act(async () => {
    setAdjustments({ exposure: 1.25 });
    useEditorStore.getState().resetHistory(before);
  });
  assert.equal(currentAdjustments(), before);
  pending.resolve(['Maker', 'Lens 35']);
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments(), before);
  assert.equal(calls.filter((c) => c.name === 'get_lens_distortion_params').length, 0);
});

test('lens_same_maker_reselection_reloads_cleared_model_list', async () => {
  reset({ lensMaker: 'Maker' });
  const root = await mountPanel();
  const before = calls.filter((c) => c.name === 'get_lensfun_lenses_for_maker').length;
  await click(findButton(root, 'Maker'));
  await click(root.root.findAllByProps({ role: 'option' }).find((node) => text(node) === 'Maker'));
  await act(async () => flushMicrotasks());
  assert.equal(currentAdjustments().lensMaker, 'Maker');
  assert.equal(currentAdjustments().lensModel, null);
  assert.equal(calls.filter((c) => c.name === 'get_lensfun_lenses_for_maker').length, before + 1);
  assert.doesNotMatch(statusText(root), /Loading lens/);
  await click(findButton(root, translate('modals.lensCorrection.selectLensModel')));
  assert.deepEqual(root.root.findAllByProps({ role: 'option' }).map(text), ['Lens 35', 'Lens 50']);
});

test('lens_accepted_history_timer_cannot_append_into_a_new_native_source', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  reset();
  const root = await mountPanel();
  await navigate('/synthetic/lens-A.RAF');
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  assert.equal(currentAdjustments().lensModel, 'Lens 35');
  await navigate('/synthetic/lens-B.RAF');
  const history = useEditorStore.getState().history;
  await act(async () => t.mock.timers.tick(500));
  assert.equal(useEditorStore.getState().history, history);
  assert.equal(history.length, 1);
});

test('lens_actual_completion_retires_status_when_pending_begins_inside_atomic_commit', async () => {
  reset();
  const root = await mountPanel();
  const originalSet = useEditorStore.getState().setEditor;
  let armed = false;
  await act(async () =>
    useEditorStore.setState({
      setEditor: (updater) => {
        if (armed) useEditorStore.getState().prepareProgressiveIntent('commit-pending', '/synthetic/next.RAF');
        originalSet(updater);
      },
    }),
  );
  const pending = deferred();
  queueInvocation('get_lens_distortion_params', pending.promise);
  const before = currentAdjustments();
  const history = useEditorStore.getState().history;
  await click(findButton(root, translate('modals.lensCorrection.modeAuto')));
  armed = true;
  try {
    pending.resolve(usableParams);
    await act(async () => flushMicrotasks());
  } finally {
    armed = false;
    await act(async () => useEditorStore.setState({ setEditor: originalSet }));
  }
  assert.equal(currentAdjustments(), before);
  assert.equal(useEditorStore.getState().history, history);
  assert.doesNotMatch(statusText(root), /Detecting EXIF|Maker - Lens 35/);
});
