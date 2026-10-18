import assert from 'node:assert/strict';
import { after, afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import { build, stop as stopEsbuild } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);
const bundle = await build({
  stdin: {
    contents: `export { default as Editor } from './src/components/panel/Editor';
      export { AutoStraightenControl } from './src/components/panel/right/CropPanel';
      export { useEditorActions, useAutoStraighten, debouncedSetHistory, debouncedSave, straightenedAdjustments, autoStraightenUnavailable } from './src/hooks/useEditorActions';
      export { useImageProcessing } from './src/hooks/useImageProcessing';
      export { useEditorStore } from './src/store/useEditorStore';
      export { useSettingsStore } from './src/store/useSettingsStore';
      export { useUIStore } from './src/store/useUIStore';
      export { useLibraryStore } from './src/store/useLibraryStore';
      export { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from './src/utils/adjustments';
      export { calculateAutoCropForRotation } from './src/utils/cropUtils';
      export { globalImageCache } from './src/utils/ImageLRUCache';
      export { Invokes, Panel } from './src/components/ui/AppProperties';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'progressive-test-native-boundaries',
      setup(builder) {
        const mocked = new Map([
          [
            '@tauri-apps/api/core',
            `export const invoke = (...args) => globalThis.__progressiveInvoke(...args); export const convertFileSrc = x => x;`,
          ],
          [
            '@tauri-apps/api/event',
            `export const listen = (...args) => globalThis.__progressiveListen(...args); export const emit = async (...args) => globalThis.__progressiveEmit?.(...args);`,
          ],
          [
            '@tauri-apps/plugin-dialog',
            `export const open = async (...args) => globalThis.__progressiveDialogOpen?.(...args) ?? null; export const save = async (...args) => globalThis.__progressiveDialogSave?.(...args) ?? null;`,
          ],
          [
            'progressive-editor-image-render-size',
            `const size = { width: 400, height: 300, scale: 0.1, offsetX: 40, offsetY: 10, containerWidth: 480, containerHeight: 320 }; export function useImageRenderSize() { return size; }`,
          ],
          [
            'auto-editor-canvas-boundary',
            `import React from 'react'; export default function CanvasBoundary(props) { return React.createElement('editor-canvas-boundary', props); }`,
          ],
          [
            'auto-editor-toolbar-boundary',
            `import React from 'react'; export default function ToolbarBoundary(props) { return React.createElement('editor-toolbar-boundary', props); }`,
          ],
          ['@tauri-apps/api/path', `export const homeDir = async () => '/';`],
          [
            '@tauri-apps/api/window',
            `export const getCurrentWindow = () => ({ setDecorations: async () => {}, minimize: async () => {}, toggleMaximize: async () => {}, close: async () => {} });`,
          ],
          ['@tauri-apps/plugin-os', `export const platform = () => 'linux';`],
          [
            '@tauri-apps/plugin-http',
            `export const fetch = async () => { throw new Error('cloud HTTP transport is outside this consumer test'); };`,
          ],
          ['tauri-plugin-clerk', `export const initClerk = (...args) => globalThis.__progressiveInitClerk(...args);`],
          [
            'react-toastify',
            `export const toast = new Proxy({}, { get: (_, key) => (...args) => globalThis.__progressiveToast?.[key]?.(...args) });`,
          ],
          [
            'react-i18next',
            `export const useTranslation = () => ({ t: (key, options) => { globalThis.__progressiveTranslations?.push({ key, options }); return options?.defaultValue ?? key; }, i18n: { language: 'en' } }); export const Trans = ({ children }) => children;`,
          ],
          [
            'react-image-crop',
            `import React from 'react'; export default function ReactCrop({ children }) { return React.createElement('mock-crop', null, children); } export const centerCrop = x => x; export const makeAspectCrop = x => x;`,
          ],
          [
            'react-konva',
            `import React from 'react'; const shape = name => function MockShape({ children, ...props }) { return React.createElement(name, props, children); }; export const Stage = shape('mock-stage'); export const Layer = shape('mock-layer'); export const Ellipse = shape('mock-ellipse'); export const Line = shape('mock-line'); export const Transformer = shape('mock-transformer'); export const Group = shape('mock-group'); export const Circle = shape('mock-circle'); export const Rect = shape('mock-rect'); export const Arrow = shape('mock-arrow');`,
          ],
          [
            '../context/ContextMenuContext',
            `export const useContextMenu = () => ({ showContextMenu: (...args) => globalThis.__progressiveShowContextMenu?.(...args) });`,
          ],
          [
            'framer-motion',
            `import React from 'react'; const wrap = tag => ({ children, ...props }) => React.createElement(tag, Object.fromEntries(Object.entries(props).filter(([key]) => ['className','style','id','role','title','onClick','onMouseEnter','onMouseLeave','disabled','tabIndex','aria-label','aria-live'].includes(key))), children); const Reorder = Object.assign(wrap('div'), { Item: wrap('div') }); export const motion = new Proxy({}, { get: () => wrap('div') }); export const AnimatePresence = ({ children }) => children; export const LayoutGroup = ({ children }) => children; export { Reorder }; export const DragControls = {}; export const useDragControls = () => ({ start() {} });`,
          ],
        ]);
        builder.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/api\/event|@tauri-apps\/api\/window|@tauri-apps\/plugin-dialog|@tauri-apps\/api\/path|@tauri-apps\/plugin-os|@tauri-apps\/plugin-http|tauri-plugin-clerk|react-toastify|react-i18next|react-image-crop|react-konva|framer-motion)$/,
          },
          (args) => ({ path: args.path, namespace: 'progressive-mock' }),
        );
        builder.onResolve({ filter: /^\.\.\/context\/ContextMenuContext$/ }, (args) => ({
          path: args.path,
          namespace: 'progressive-mock',
        }));
        builder.onResolve({ filter: /^\.\.\/\.\.\/hooks\/useImageRenderSize$/ }, (args) =>
          args.importer.endsWith('/src/components/panel/Editor.tsx')
            ? { path: 'progressive-editor-image-render-size', namespace: 'progressive-mock' }
            : undefined,
        );
        builder.onResolve({ filter: /^\.\/editor\/(ImageCanvas|EditorToolbar)$/ }, (args) =>
          args.importer.endsWith('/src/components/panel/Editor.tsx')
            ? {
                path: args.path.endsWith('/ImageCanvas')
                  ? 'auto-editor-canvas-boundary'
                  : 'auto-editor-toolbar-boundary',
                namespace: 'progressive-mock',
              }
            : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'progressive-mock' }, (args) => ({
          contents: mocked.get(args.path),
          loader: 'js',
        }));
        builder.onResolve({ filter: /\.css$/ }, (args) => ({ path: args.path, namespace: 'progressive-mock-css' }));
        builder.onLoad({ filter: /.*/, namespace: 'progressive-mock-css' }, () => ({
          contents: 'export default {}',
          loader: 'js',
        }));
      },
    },
  ],
});
const mod = { exports: {} };
const originalProgressiveInitClerk = Object.getOwnPropertyDescriptor(globalThis, '__progressiveInitClerk');
let unexpectedClerkInitCalls = 0;
globalThis.__progressiveInitClerk = (...args) => {
  unexpectedClerkInitCalls += 1;
  throw new Error(`Unexpected Clerk auth boundary invocation (${args.length} arguments)`);
};
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  Editor,
  AutoStraightenControl,
  useEditorActions,
  useAutoStraighten,
  debouncedSetHistory,
  debouncedSave,
  straightenedAdjustments,
  autoStraightenUnavailable,
  useImageProcessing,
  useEditorStore,
  useSettingsStore,
  useUIStore,
  useLibraryStore,
  INITIAL_ADJUSTMENTS,
  normalizeLoadedAdjustments,
  calculateAutoCropForRotation,
  globalImageCache,
  Invokes,
  Panel,
} = mod.exports;
const { create, act } = require('react-test-renderer');
const roots = new Set();
const calls = [],
  draws = [],
  images = [],
  revoked = [];
const originalEditorGlobals = Object.fromEntries(
  ['getComputedStyle', 'ResizeObserver', 'requestAnimationFrame', 'cancelAnimationFrame'].map((name) => [
    name,
    Object.getOwnPropertyDescriptor(globalThis, name),
  ]),
);
const editorAnimationFrames = new Set();
globalThis.getComputedStyle = () => ({ getPropertyValue: () => '' });
globalThis.ResizeObserver = class {
  observe() {}
  disconnect() {}
};
globalThis.requestAnimationFrame = (callback) => {
  const handle = setTimeout(() => {
    editorAnimationFrames.delete(handle);
    callback(performance.now());
  }, 1);
  editorAnimationFrames.add(handle);
  return handle;
};
globalThis.cancelAnimationFrame = (handle) => {
  clearTimeout(handle);
  editorAnimationFrames.delete(handle);
};
const originalImage = globalThis.Image;
const originalDocument = globalThis.document;
const originalWindow = globalThis.window;
const originalCreateUrl = URL.createObjectURL,
  originalRevokeUrl = URL.revokeObjectURL;
let serial = 0,
  decodeNext = null,
  estimator = async () => ({ correctionDegrees: -5, confidence: 1 });
let applyHandler = async () => new Uint8Array([1, 2, 3]).buffer;
let actions, processing;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.__progressiveInvoke = async (command, args) => {
  calls.push({ command, args });
  if (command === Invokes.ApplyAdjustments) return applyHandler(args);
  if (command === Invokes.CalculatePreviewAutoStraighten) return estimator(args);
  if (command === Invokes.SaveMetadataAndUpdateThumbnail) return null;
  throw new Error(`Unexpected native operation: ${command}`);
};
globalThis.Image = class {
  naturalWidth = 1024;
  naturalHeight = 768;
  src = '';
  constructor() {
    images.push(this);
  }
  decode() {
    const next = decodeNext;
    decodeNext = null;
    return next ? next() : Promise.resolve();
  }
};
globalThis.document = {
  documentElement: { style: { setProperty() {} } },
  addEventListener() {},
  removeEventListener() {},
  createElement(name) {
    assert.equal(name, 'canvas');
    return {
      width: 0,
      height: 0,
      getContext(kind) {
        assert.equal(kind, '2d');
        return {
          drawImage(_image, x, y, w, h) {
            draws.push({ x, y, w, h });
          },
          getImageData(x, y, w, h) {
            return { data: new Uint8ClampedArray(w * h * 4).fill(255) };
          },
        };
      },
    };
  },
};
globalThis.window = {
  devicePixelRatio: 1,
  innerWidth: 1024,
  innerHeight: 768,
  addEventListener() {},
  removeEventListener() {},
  setTimeout,
  clearTimeout,
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
};
URL.createObjectURL = () => `blob:auto-${++serial}`;
URL.revokeObjectURL = (url) => revoked.push(url);
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
async function tick() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 5));
  });
}
async function until(predicate, label = 'condition') {
  const deadline = Date.now() + 1500;
  while (!predicate() && Date.now() < deadline) await tick();
  assert.ok(predicate(), `${label} did not settle within1500ms`);
}
async function unmount(root) {
  if (roots.delete(root)) await act(async () => root.unmount());
}
function reset(edit = INITIAL_ADJUSTMENTS, keyed = true) {
  debouncedSave.cancel();
  debouncedSetHistory.cancel();
  calls.length = draws.length = images.length = revoked.length = 0;
  estimator = async () => ({ correctionDegrees: -5, confidence: 1 });
  applyHandler = async () => new Uint8Array([1, 2, 3]).buffer;
  decodeNext = null;
  useEditorStore.setState({
    selectedImage: null,
    pendingOpenIntent: null,
    progressiveOpen: null,
    adjustments: edit,
    history: [edit],
    historyIndex: 0,
    finalPreviewUrl: null,
    finalPreviewProof: null,
    autoStraightenBusy: false,
    uncroppedAdjustedPreviewUrl: null,
    interactivePatch: null,
    previewOverride: null,
    showOriginal: false,
    isSliderDragging: false,
    isRotationActive: false,
    isStraightenActive: false,
    isGuidedPerspectiveActive: false,
    liveRotation: null,
    isWbPickerActive: false,
    isPointColourPickerActive: false,
    isRelightPickerActive: false,
    displaySize: { width: 0, height: 0 },
    baseRenderSize: { width: 0, height: 0 },
    originalSize: { width: 400, height: 300 },
    hasRenderedFirstFrame: true,
    patchesSentToBackend: new Set(),
    isWaveformVisible: false,
  });
  useSettingsStore.setState({ appSettings: null, osPlatform: 'android' });
  useLibraryStore.setState({ multiSelectedPaths: [] });
  useUIStore.setState({ activeView: 'editor', activePanel: Panel.Adjustments });
  if (keyed) load('10000000-0000-4000-8000-000000000001', '1', '/A.raf', edit);
  else
    useEditorStore.setState({
      selectedImage: { path: '/A.raf', width: 400, height: 300, isReady: true, isRaw: true, renderMode: 'Legacy' },
    });
}
function load(ticket, generation, path, edit = INITIAL_ADJUSTMENTS) {
  const s = useEditorStore.getState(),
    openKey = { ticket, generation };
  s.prepareProgressiveIntent(ticket, path);
  s.activateProgressiveIntent(ticket, path, 'thumb:unused');
  assert.equal(s.acceptProgressiveAdmission(ticket, path, openKey, { adjustments: edit }, edit), true);
  assert.equal(
    s.completeProgressiveLoad(ticket, path, openKey, {
      width: 400,
      height: 300,
      previewSize: { width: 400, height: 300 },
      metadata: { adjustments: edit },
      is_raw: true,
      exif: null,
      as_shot_white_balance: null,
      renderMode: 'AccurateXtrans1Encoded',
    }),
    true,
  );
}
async function mount() {
  function Probe() {
    actions = useEditorActions();
    const prev = React.useRef(null),
      transform = React.useRef(null);
    const refs = React.useRef({
      previewJobIdRef: { current: 0 },
      latestRenderedJobIdRef: { current: 0 },
      currentResRef: { current: 0 },
    });
    processing = useImageProcessing(transform, prev, refs.current);
    return React.createElement(AutoStraightenControl);
  }
  let root;
  await act(async () => {
    root = create(React.createElement(Probe));
    roots.add(root);
  });
  await until(() => !!useEditorStore.getState().finalPreviewProof, 'real final preview publication');
  return root;
}
const button = (root) => root.root.findByProps({ 'aria-label': 'Auto straighten' });
async function click(root) {
  assert.equal(button(root).props.disabled, false);
  await act(async () => button(root).props.onClick());
}
const nativeCalls = () => calls.filter((c) => c.command === Invokes.CalculatePreviewAutoStraighten);
async function done(root) {
  await until(() => !root.root.findAllByProps({ 'aria-label': 'Cancel auto straighten' }).length, 'auto completion');
}
afterEach(async () => {
  for (const r of [...roots]) await unmount(r);
  for (const handle of editorAnimationFrames) clearTimeout(handle);
  editorAnimationFrames.clear();
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
  globalImageCache.clear();
  decodeNext = null;
});
after(async () => {
  await stopEsbuild();
  for (const [name, descriptor] of Object.entries(originalEditorGlobals)) {
    if (descriptor) Object.defineProperty(globalThis, name, descriptor);
    else delete globalThis[name];
  }
  if (originalProgressiveInitClerk)
    Object.defineProperty(globalThis, '__progressiveInitClerk', originalProgressiveInitClerk);
  else delete globalThis.__progressiveInitClerk;
  assert.equal(unexpectedClerkInitCalls, 0);
  globalThis.Image = originalImage;
  globalThis.document = originalDocument;
  globalThis.window = originalWindow;
  URL.createObjectURL = originalCreateUrl;
  URL.revokeObjectURL = originalRevokeUrl;
});

test('as02_actual_controls_use_existing_rotation_crop_transaction', async () => {
  const edit = {
    ...INITIAL_ADJUSTMENTS,
    rotation: 7,
    orientationSteps: 1,
    flipHorizontal: true,
    aspectRatio: 4 / 3,
    crop: { x: 40, y: 20, width: 240, height: 180 },
    profile: { base: '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', look: null, amount: 100 },
    pointColour: { v: 9, kept: 'future' },
  };
  reset(edit);
  const root = await mount();
  const expectedCrop = calculateAutoCropForRotation(400, 300, 1, 4 / 3, 2, edit.crop, -5);
  await click(root);
  await done(root);
  const value = useEditorStore.getState().adjustments;
  assert.equal(value.rotation, 2);
  assert.deepEqual(value.crop, expectedCrop);
  assert.equal(value.orientationSteps, 1);
  assert.equal(value.flipHorizontal, true);
  assert.equal(value.profile, edit.profile);
  assert.equal(value.pointColour, edit.pointColour);
  assert.deepEqual(value, straightenedAdjustments(edit, { width: 400, height: 300 }, -5));
  assert.equal(nativeCalls().length, 1);
  assert.deepEqual(nativeCalls()[0].args.expectedOpenKey, {
    ticket: '10000000-0000-4000-8000-000000000001',
    generation: '1',
  });
  assert.deepEqual(draws, [{ x: 0, y: 0, w: 512, h: 384 }]);
});

test('as04_deferred_source_edit_settings_cancel_and_aba_reject', async () => {
  for (const keyed of [true, false])
    for (const mode of [
      'edit',
      'profile',
      'settings',
      'preview',
      'pending',
      'ABA',
      'navigate',
      'cancel',
      'unmount',
      'error',
      'none',
      'decode-stale',
      'decode-error',
    ]) {
      reset(INITIAL_ADJUSTMENTS, keyed);
      const root = await mount();
      const pending = deferred();
      estimator = () => pending.promise;
      if (mode === 'decode-stale' || mode === 'decode-error') decodeNext = () => pending.promise;
      await click(root);
      if (!mode.startsWith('decode-')) await until(() => nativeCalls().length === 1, 'native estimate admission');
      await act(async () => button(root).props.onClick());
      assert.equal(nativeCalls().length, mode.startsWith('decode-') ? 0 : 1, 'no second operation or hover queue');
      await act(async () => {
        if (mode === 'edit' || mode === 'decode-stale') actions.setAdjustments({ exposure: 0.5 });
        if (mode === 'profile') actions.setAdjustments({ profile: { base: 'changed', look: null, amount: 100 } });
        if (mode === 'settings') useSettingsStore.getState().setAppSettings({ baselineExposure: 1 });
        if (mode === 'preview') processing.applyAdjustments(useEditorStore.getState().adjustments, false);
        if (mode === 'pending')
          useEditorStore.getState().prepareProgressiveIntent('20000000-0000-4000-8000-000000000002', '/B.raf');
        if (mode === 'ABA') {
          load('20000000-0000-4000-8000-000000000002', '2', '/B.raf');
          load('30000000-0000-4000-8000-000000000003', '3', '/A.raf');
        }
        if (mode === 'navigate') useUIStore.getState().setUI({ activeView: 'library' });
        if (mode === 'cancel') root.root.findByProps({ 'aria-label': 'Cancel auto straighten' }).props.onClick();
      });
      let reopened = null;
      if (mode === 'unmount') {
        await unmount(root);
        assert.equal(
          useEditorStore.getState().autoStraightenBusy,
          true,
          'unmounted work retains admission until settlement',
        );
        reopened = await mount();
        assert.equal(button(reopened).props.disabled, true, 'remount cannot overlap unfinished analysis');
      }
      const before = useEditorStore.getState().adjustments;
      await act(async () => {
        if (mode === 'error' || mode === 'decode-error') pending.reject(new Error('estimator rejected'));
        else pending.resolve(mode === 'none' ? null : { correctionDegrees: -5, confidence: 1 });
      });
      if (mode !== 'unmount') await done(root);
      else await until(() => !useEditorStore.getState().autoStraightenBusy, 'unmounted work releases admission');
      assert.equal(useEditorStore.getState().adjustments, before, mode);
      if (mode.startsWith('decode-'))
        assert.equal(nativeCalls().length, 0, 'stale decoder completion never invokes native');
      if (mode === 'error' || mode === 'decode-error')
        assert.match(root.root.findByProps({ role: 'status' }).children.join(''), /estimator rejected/);
      if (mode === 'none') assert.match(root.root.findByProps({ role: 'status' }).children.join(''), /No clear level/);
      await unmount(root);
      if (reopened) await unmount(reopened);
      debouncedSetHistory.cancel();
      debouncedSave.cancel();
    }
});

test('as05_single_history_undo_redo_and_saved_fields', async () => {
  reset();
  const root = await mount();
  await act(async () => actions.setAdjustments({ exposure: 0.75 }));
  await until(() => !!useEditorStore.getState().finalPreviewProof, 'preview after pending manual edit');
  const manual = useEditorStore.getState().adjustments;
  assert.equal(useEditorStore.getState().history.length, 1, 'manual history still pending');
  await click(root);
  await done(root);
  const automatic = useEditorStore.getState().adjustments;
  assert.equal(useEditorStore.getState().history.length, 3);
  assert.equal(useEditorStore.getState().history[1], manual);
  assert.equal(useEditorStore.getState().history[2], automatic);
  assert.equal(automatic.rotation, -5);
  assert.equal(automatic.exposure, 0.75);
  await until(
    () => calls.some((c) => c.command === Invokes.SaveMetadataAndUpdateThumbnail && c.args.adjustments === automatic),
    'ordinary autosave',
  );
  const saved = calls.find(
    (c) => c.command === Invokes.SaveMetadataAndUpdateThumbnail && c.args.adjustments === automatic,
  ).args.adjustments;
  assert.equal(Object.hasOwn(saved, 'finalPreviewProof'), false);
  assert.deepEqual(normalizeLoadedAdjustments(saved).crop, automatic.crop);
  assert.equal(normalizeLoadedAdjustments(saved).rotation, -5);
  await act(async () => useEditorStore.getState().undo());
  assert.equal(useEditorStore.getState().adjustments, manual);
  await act(async () => useEditorStore.getState().redo());
  assert.equal(useEditorStore.getState().adjustments, automatic);
  await until(() => !!useEditorStore.getState().finalPreviewProof, 'preview after redo');
  const history = useEditorStore.getState().history;
  estimator = async () => ({ correctionDegrees: 0, confidence: 1 });
  await click(root);
  await done(root);
  assert.equal(useEditorStore.getState().history, history);
  assert.equal(useEditorStore.getState().adjustments, automatic);
});

test('as06_actual_preview_publication_and_bounded_capture_only', async () => {
  reset(INITIAL_ADJUSTMENTS, false);
  const root = await mount();
  const proof = useEditorStore.getState().finalPreviewProof;
  assert.equal(proof.source, useEditorStore.getState().selectedImage);
  assert.equal(proof.adjustments, useEditorStore.getState().adjustments);
  assert.equal(proof.settings, useSettingsStore.getState().appSettings);
  assert.equal(proof.url, useEditorStore.getState().finalPreviewUrl);
  assert.equal(proof.isCurrent(), true);
  const before = calls.length;
  await click(root);
  await done(root);
  const estimateCall = calls.slice(before).find((c) => c.command === Invokes.CalculatePreviewAutoStraighten);
  assert.ok(estimateCall);
  assert.equal(estimateCall.args.rgba.length, 512 * 384 * 4);
  assert.ok(estimateCall.args.rgba.length <= 1024 * 1024);
  assert.equal(Object.hasOwn(estimateCall.args, 'expectedOpenKey'), false);
  assert.ok(
    calls.every((c) =>
      [Invokes.ApplyAdjustments, Invokes.CalculatePreviewAutoStraighten, Invokes.SaveMetadataAndUpdateThumbnail].includes(
        c.command,
      ),
    ),
  );
  assert.equal(proof.isCurrent(), false, 'accepted edit invalidates original proof');
  await until(() => !!useEditorStore.getState().finalPreviewProof, 'new final frame');
  const pending = deferred();
  applyHandler = () => pending.promise;
  await act(async () => processing.applyAdjustments(useEditorStore.getState().adjustments, true));
  assert.equal(useEditorStore.getState().finalPreviewProof, null, 'interactive admission invalidates old final proof');
  const roi = new Uint8Array(25);
  new Uint32Array(roi.buffer, 0, 6).set([0, 0, 100, 75, 200, 150]);
  await act(async () => pending.resolve(roi.buffer));
  await tick();
  assert.equal(useEditorStore.getState().finalPreviewProof, null);
  assert.equal(button(root).props.disabled, true);
  applyHandler = async () => new TextEncoder().encode('WGPU_RENDER-frame').buffer;
  await act(async () => processing.applyAdjustments(useEditorStore.getState().adjustments, false));
  await tick();
  assert.equal(useEditorStore.getState().finalPreviewProof, null, 'native sentinel is not an encoded proof');
  applyHandler = async () => new Uint8Array([1, 2, 3]).buffer;
  await act(async () => processing.applyAdjustments(useEditorStore.getState().adjustments, false));
  await until(() => !!useEditorStore.getState().finalPreviewProof, 'encoded recovery');
  await act(async () => actions.setAdjustments({ transformVertical: 2 }));
  assert.match(autoStraightenUnavailable(), /perspective/);
  await unmount(root);
  assert.equal(useEditorStore.getState().finalPreviewProof, null);
});

test('as07_actual_editor_manual_history_then_auto_and_source_lifetime', async () => {
  reset({ ...INITIAL_ADJUSTMENTS, aspectRatio: 4 / 3, exposure: 0.25 });
  const control = await mount();
  async function mountActualEditor() {
    let root;
    await act(async () => {
      root = create(
        React.createElement(
          React.StrictMode,
          null,
          React.createElement(Editor, {
            onBackToLibrary() {},
            onContextMenu() {},
            transformWrapperRef: { current: null },
          }),
        ),
      );
      roots.add(root);
    });
    return root;
  }
  let editor = await mountActualEditor();
  const canvas = () => editor.root.findByType('editor-canvas-boundary');
  const toolbar = () => editor.root.findByType('editor-toolbar-boundary');
  const original = useEditorStore.getState().adjustments;
  assert.deepEqual(useEditorStore.getState().history, [original]);
  assert.equal(typeof canvas().props.onStraighten, 'function');
  // This callback is created by the real Editor, not the Probe hook's setter.
  await act(async () => canvas().props.onStraighten(2));
  const manual = useEditorStore.getState().adjustments;
  assert.deepEqual(manual, straightenedAdjustments(original, useEditorStore.getState().selectedImage, 2));
  await until(() => useEditorStore.getState().finalPreviewProof?.adjustments === manual, 'actual Editor B preview');
  assert.deepEqual(useEditorStore.getState().history, [original], 'B must still be inside the real 500ms debounce');
  await click(control);
  await done(control);
  const automatic = useEditorStore.getState().adjustments;
  assert.equal(automatic.rotation, -3);
  assert.equal(automatic.exposure, 0.25);
  assert.equal(nativeCalls().length, 1);
  assert.deepEqual(useEditorStore.getState().history, [original, manual, automatic]);
  await act(async () => toolbar().props.onUndo());
  assert.equal(useEditorStore.getState().adjustments, manual);
  await act(async () => toolbar().props.onRedo());
  assert.equal(useEditorStore.getState().adjustments, automatic);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 550));
  });
  assert.deepEqual(useEditorStore.getState().history, [original, manual, automatic], 'no late/duplicate B');

  // Ordinary manual editing still commits on the existing timer without Auto.
  await act(async () => canvas().props.setAdjustments({ exposure: 0.5 }));
  const ordinary = useEditorStore.getState().adjustments;
  assert.deepEqual(useEditorStore.getState().history, [original, manual, automatic]);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 550));
  });
  assert.deepEqual(useEditorStore.getState().history, [original, manual, automatic, ordinary]);

  // Same path with a new source/OpenKey must not inherit an old delayed edit.
  await act(async () => canvas().props.setAdjustments({ exposure: 0.75 }));
  const stale = useEditorStore.getState().adjustments;
  const oldSource = useEditorStore.getState().selectedImage;
  await act(async () => load('20000000-0000-4000-8000-000000000002', '2', '/A.raf', original));
  assert.notEqual(useEditorStore.getState().selectedImage, oldSource);
  const reopenedHistory = useEditorStore.getState().history;
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 550));
  });
  assert.equal(useEditorStore.getState().history, reopenedHistory);
  assert.ok(!useEditorStore.getState().history.includes(stale));

  // The shared timer outlives the view; its source fence also covers unmount.
  await act(async () => canvas().props.setAdjustments({ exposure: 1 }));
  const staleUnmounted = useEditorStore.getState().adjustments;
  await unmount(editor);
  await act(async () => load('30000000-0000-4000-8000-000000000003', '3', '/B.raf', original));
  const replacementHistory = useEditorStore.getState().history;
  editor = await mountActualEditor();
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 550));
  });
  assert.equal(useEditorStore.getState().history, replacementHistory);
  assert.ok(!useEditorStore.getState().history.includes(staleUnmounted));
  await act(async () => canvas().props.setAdjustments({ exposure: 1.25 }));
  const currentManual = useEditorStore.getState().adjustments;
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 550));
  });
  assert.equal(useEditorStore.getState().history.at(-1), currentManual);
  assert.equal(useEditorStore.getState().history.length, replacementHistory.length + 1);
});


test('as09_app_auto_hook_ignores_unrelated_writes_and_observes_busy_availability', async () => {
  reset();
  let renders = 0;
  let observed;
  function Probe() {
    observed = useAutoStraighten();
    renders += 1;
    return null;
  }
  let root;
  await act(async () => {
    root = create(React.createElement(Probe));
    roots.add(root);
  });
  assert.match(observed.unavailable, /complete current preview/);
  const initialRenders = renders;
  await act(async () => {
    useEditorStore.setState({ zoom: 2 });
    useEditorStore.setState({ displaySize: { width: 480, height: 320 } });
    useEditorStore.setState({ aiDenoiseProgress: { '/A.raf': 0.5 } });
  });
  assert.equal(renders, initialRenders, 'unrelated editor writes do not rerender the App-level hook');
  assert.equal(observed.busy, false);
  await act(async () => useEditorStore.setState({ autoStraightenBusy: true }));
  assert.equal(observed.busy, true);
  assert.ok(renders > initialRenders);
  const busyRenders = renders;
  await act(async () => useEditorStore.setState({ showOriginal: true }));
  assert.match(observed.unavailable, /current adjustment/);
  assert.ok(renders > busyRenders);
  const unavailableRenders = renders;
  await act(async () => useUIStore.setState({ activeView: 'library' }));
  assert.match(observed.unavailable, /Open the photo in the editor/);
  assert.ok(renders > unavailableRenders, 'view changes still refresh availability');
  await unmount(root);
});
