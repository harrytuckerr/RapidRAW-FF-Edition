import assert from 'node:assert/strict';
import { after, afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import { build, stop as stopEsbuild } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);
const bundle = await build({
  stdin: {
    contents: `export { PointColourControls } from './src/components/adjustments/Color';
      export { pointColourV1 } from './src/utils/adjustments';
      export { pointColourOutputUv, usePointColourPicker } from './src/components/panel/editor/ImageCanvas';
      export { default as Slider } from './src/components/ui/Slider';
      export { debouncedSetHistory } from './src/hooks/useEditorActions';
      export { useImageLoader } from './src/hooks/useImageLoader';
      export { useAppNavigation } from './src/hooks/useAppNavigation';
      export { useTauriListeners } from './src/hooks/useTauriListeners';
      export { useAppContextMenus } from './src/hooks/useAppContextMenus';
      export { useImageProcessing } from './src/hooks/useImageProcessing';
      export { useEditorActions } from './src/hooks/useEditorActions';
      export { useAiMasking } from './src/hooks/useAiMasking';
      export { flushEditorSavesBeforeOpen } from './src/hooks/useEditorActions';
      export { default as ImageCanvas } from './src/components/panel/editor/ImageCanvas';
      export { default as Editor } from './src/components/panel/Editor';
      export { default as ExportPanel } from './src/components/panel/right/ExportPanel';
      export { useEditorStore } from './src/store/useEditorStore';
      export { useLibraryStore } from './src/store/useLibraryStore';
      export { useProcessStore } from './src/store/useProcessStore';
      export { useSettingsStore } from './src/store/useSettingsStore';
      export { useUIStore } from './src/store/useUIStore';
      export { globalImageCache } from './src/utils/ImageLRUCache';
      export { debouncedSave } from './src/hooks/useEditorActions';
      export { default as DetailsPanel } from './src/components/adjustments/Details';
      export { default as MasksPanel } from './src/components/panel/right/MasksPanel';
      export { Invokes } from './src/components/ui/AppProperties';
      export { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from './src/utils/adjustments';`,
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
            `export function useImageRenderSize() { return { width: 400, height: 300, scale: 0.1, offsetX: 40, offsetY: 10, containerWidth: 480, containerHeight: 320 }; }`,
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
            `export const useTranslation = () => ({ t: (key, options) => { globalThis.__progressiveTranslations?.push({ key, options }); return options?.defaultValue ?? key; }, i18n: { language: 'en' } });`,
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
  PointColourControls,
  pointColourV1,
  pointColourOutputUv,
  usePointColourPicker,
  Slider,
  debouncedSetHistory,
  debouncedSave,
  DetailsPanel,
  flushEditorSavesBeforeOpen,
  globalImageCache,
  INITIAL_ADJUSTMENTS,
  Invokes,
  MasksPanel,
  ImageCanvas,
  Editor,
  ExportPanel,
  normalizeLoadedAdjustments,
  useAppContextMenus,
  useAppNavigation,
  useAiMasking,
  useEditorActions,
  useEditorStore,
  useImageLoader,
  useImageProcessing,
  useLibraryStore,
  useProcessStore,
  useSettingsStore,
  useTauriListeners,
  useUIStore,
} = mod.exports;
const { create, act } = require('react-test-renderer');
const mountedRoots = new Set();
const originalCacheGet = globalImageCache.get;
const originalCacheSet = globalImageCache.set;

function trackedCreate(element, options) {
  const root = create(element, options);
  mountedRoots.add(root);
  return root;
}

async function trackedUnmount(root) {
  if (!mountedRoots.delete(root)) return;
  await act(async () => root.unmount());
}

afterEach(async () => {
  for (const root of [...mountedRoots]) await trackedUnmount(root);
  debouncedSave.cancel();
  debouncedSetHistory.cancel();
  globalImageCache.get = originalCacheGet;
  globalImageCache.set = originalCacheSet;
  globalImageCache.clear();
});

after(async () => {
  await stopEsbuild();
  if (originalProgressiveInitClerk) {
    Object.defineProperty(globalThis, '__progressiveInitClerk', originalProgressiveInitClerk);
  } else {
    delete globalThis.__progressiveInitClerk;
  }
  assert.equal(unexpectedClerkInitCalls, 0, 'external cloud-auth boundary must remain unused');
});

globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = {
  addEventListener() {},
  removeEventListener() {},
  documentElement: {},
  body: { appendChild() {}, removeChild() {} },
};
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
globalThis.getComputedStyle = () => ({
  getPropertyValue: (name) => (name === '--app-bg-primary' ? 'rgb(24, 24, 24)' : 'rgb(35, 35, 35)'),
});
globalThis.requestAnimationFrame = (callback) => setTimeout(() => callback(Date.now()), 0);
globalThis.cancelAnimationFrame = (handle) => clearTimeout(handle);
globalThis.ResizeObserver = class {
  observe() {}
  disconnect() {}
};
globalThis.__progressiveToast = { error() {}, dismiss() {}, success() {}, info() {} };
let shownContextMenu = null;
globalThis.__progressiveShowContextMenu = (...args) => {
  shownContextMenu = args;
};
let invokeHandler = async () => null;
const calls = [];
globalThis.__progressiveInvoke = (command, args) => {
  calls.push({ command, args });
  return invokeHandler(command, args);
};
const listeners = new Map();
let unlistenCount = 0;
const defaultListenHandler = async (name, callback) => {
  listeners.set(name, callback);
  return () => {
    unlistenCount++;
    if (listeners.get(name) === callback) listeners.delete(name);
  };
};
let listenHandler = defaultListenHandler;
globalThis.__progressiveListen = (...args) => listenHandler(...args);

const ticketA = '8b451702-eaf1-46d9-9d4f-1d741a1555ab';
const keyA = { ticket: ticketA, generation: '41' };

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flushMicrotasks(rounds = 12) {
  for (let index = 0; index < rounds; index++) await Promise.resolve();
}

function resetStores() {
  useEditorStore.setState({
    selectedImage: null,
    progressiveOpen: null,
    pendingOpenIntent: null,
    adjustments: INITIAL_ADJUSTMENTS,
    history: [INITIAL_ADJUSTMENTS],
    historyIndex: 0,
    finalPreviewUrl: null,
    uncroppedAdjustedPreviewUrl: null,
    histogram: null,
    waveform: null,
    originalSize: { width: 0, height: 0 },
    previewSize: { width: 0, height: 0 },
    hasRenderedFirstFrame: false,
  });
  useLibraryStore.setState({
    imageList: [],
    multiSelectedPaths: [],
    libraryActivePath: null,
    isViewLoading: false,
    currentFolderPath: null,
    imageRatings: {},
  });
  useProcessStore.setState({ thumbnails: {}, mediumThumbnails: {}, activeAiTasks: {} });
  useSettingsStore.setState({ appSettings: null, osPlatform: 'linux' });
  useUIStore.setState({ activeView: 'library' });
  calls.length = 0;
  shownContextMenu = null;
  listeners.clear();
  unlistenCount = 0;
  listenHandler = defaultListenHandler;
  invokeHandler = async () => null;
  debouncedSave.cancel();
}

function setAccurateImage(
  ticket,
  openKey,
  path,
  adjustments = INITIAL_ADJUSTMENTS,
  renderMode = 'AccurateXtrans1Encoded',
) {
  const store = useEditorStore.getState();
  store.prepareProgressiveIntent(ticket, path);
  store.activateProgressiveIntent(ticket, path, `thumb:${path}`);
  store.acceptProgressiveAdmission(ticket, path, openKey, { adjustments }, adjustments);
  store.completeProgressiveLoad(ticket, path, openKey, {
    width: 4000,
    height: 3000,
    previewSize: { width: 1920, height: 1440 },
    exif: null,
    is_raw: true,
    metadata: { adjustments: INITIAL_ADJUSTMENTS },
    as_shot_white_balance: null,
    renderMode,
  });
}

function useTestHooks({
  withLoader = false,
  withNavigation = false,
  withListeners = false,
  withProcessing = false,
  withAiMasking = false,
  withContextMenus = false,
} = {}) {
  const values = { nav: null, actions: null, process: null, refs: null, cachedRef: null };
  function Probe() {
    const cachedRef = React.useRef(null);
    values.cachedRef = cachedRef;
    const refsRef = React.useRef({
      transformWrapperRef: { current: null },
      preloadedDataRef: { current: null },
      cachedEditStateRef: cachedRef,
      selectedImagePathRef: { current: null },
      isBackendReadyRef: { current: true },
      latestRenderedJobIdRef: { current: 0 },
      previewJobIdRef: { current: 0 },
      currentResRef: { current: 0 },
      prevAdjustmentsRef: { current: null },
    });
    values.refs = refsRef.current;
    if (withLoader) useImageLoader(cachedRef);
    if (withNavigation) values.nav = useAppNavigation({ clearThumbnailQueue() {}, refs: refsRef.current });
    if (withListeners)
      values.listeners = useTauriListeners({
        refreshAllFolderTrees() {},
        handleSelectSubfolder() {},
        refreshImageList() {},
        markGenerated() {},
      });
    if (withProcessing)
      values.process = useImageProcessing(refsRef.current.transformWrapperRef, refsRef.current.prevAdjustmentsRef, {
        previewJobIdRef: refsRef.current.previewJobIdRef,
        latestRenderedJobIdRef: refsRef.current.latestRenderedJobIdRef,
        currentResRef: refsRef.current.currentResRef,
      });
    if (withAiMasking) values.ai = useAiMasking();
    if (withContextMenus)
      values.contextMenus = useAppContextMenus({
        handleImageSelect() {},
        handleBackToLibrary() {},
        handleRenameFiles() {},
        handleImportClick() {},
        async handleLibraryRefresh() {},
        async refreshAllFolderTrees() {},
        async refreshImageList() {},
        async executeDelete() {},
        handleDeleteRejected() {},
        async handleTogglePinFolder() {},
      });
    values.actions = useEditorActions();
    return null;
  }
  return { Probe, values };
}

function contextMenuEvent() {
  return { preventDefault() {}, stopPropagation() {}, clientX: 8, clientY: 12 };
}

function findContextMenuOption(options, label) {
  for (const option of options || []) {
    if (option.label === label) return option;
    const nested = findContextMenuOption(option.submenu, label);
    if (nested) return nested;
  }
  return null;
}

async function showEditorContextMenu(menus) {
  shownContextMenu = null;
  await act(async () => menus.handleEditorContextMenu(contextMenuEvent()));
  assert.ok(shownContextMenu, 'the real editor context-menu handler should open the visual boundary');
  return shownContextMenu[2];
}

async function showThumbnailContextMenu(menus, path) {
  shownContextMenu = null;
  await act(async () => menus.handleThumbnailContextMenu(contextMenuEvent(), path));
  assert.ok(shownContextMenu, 'the real thumbnail context-menu handler should open the visual boundary');
  return shownContextMenu[2];
}

async function mountContextMenus() {
  const { Probe, values } = useTestHooks({ withContextMenus: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  return { root, menus: values.contextMenus };
}

function editorResetConfirmation(options) {
  const reset = findContextMenuOption(options, 'contextMenus.editor.resetAdjustments');
  assert.ok(reset, 'the real editor menu should expose reset adjustments');
  const confirm = findContextMenuOption(reset.submenu, 'contextMenus.editor.confirmReset');
  assert.ok(confirm, 'the real editor menu should expose the destructive reset confirmation');
  return confirm;
}

function setEditorHistoryState(adjustments) {
  useEditorStore.setState({ adjustments, history: [INITIAL_ADJUSTMENTS, adjustments], historyIndex: 1 });
}

function preparePendingLoader(ticket, path) {
  const store = useEditorStore.getState();
  store.prepareProgressiveIntent(ticket, path);
  assert.equal(store.activateProgressiveIntent(ticket, path, `thumb:${path}`), true);
}

function successfulLoadResult(openKey, renderMode = 'AccurateXtrans1Encoded') {
  return {
    openKey,
    width: 4000,
    height: 3000,
    metadata: { adjustments: INITIAL_ADJUSTMENTS },
    exif: null,
    is_raw: true,
    as_shot_white_balance: null,
    renderMode,
  };
}

async function mountImageCanvas({
  path,
  setAdjustments,
  asShotWhiteBalance = { temperature: 6500, tint: 1 },
  appSettings = null,
  finalPreviewUrl = null,
  interactivePatch = null,
  hasRenderedFirstFrame = false,
  thumbnailUrl,
  zoom = 1,
}) {
  const selectedImage = useEditorStore.getState().selectedImage;
  useEditorStore.setState({
    selectedImage: { ...selectedImage, path, asShotWhiteBalance, ...(thumbnailUrl ? { thumbnailUrl } : {}) },
  });
  const props = {
    appSettings,
    activeAiPatchContainerId: null,
    activeAiSubMaskId: null,
    activeMaskContainerId: null,
    activeMaskId: null,
    adjustments: useEditorStore.getState().adjustments,
    brushSettings: null,
    crop: null,
    finalPreviewUrl,
    progressiveImageUrl: null,
    progressivePhase: 'Accurate',
    handleCropComplete() {},
    imageRenderSize: { width: 320, height: 240, offsetX: 0, offsetY: 0, scale: 1 },
    isAiEditing: false,
    isCropping: false,
    isMaskControlHovered: false,
    isMasking: false,
    isSliderDragging: false,
    isStraightenActive: false,
    isRotationActive: false,
    maskOverlayUrl: null,
    onGenerateAiMask() {},
    onQuickErase() {},
    onSelectAiSubMask() {},
    onSelectMask() {},
    onStraighten() {},
    selectedImage: useEditorStore.getState().selectedImage,
    setCrop() {},
    setIsMaskHovered() {},
    setIsMaskTouchInteracting() {},
    showOriginal: false,
    uncroppedAdjustedPreviewUrl: null,
    updateSubMask() {},
    isWbPickerActive: false,
    onWbPicked() {},
    setAdjustments,
    cursorStyle: 'default',
    transformState: { scale: zoom, positionX: 37, positionY: -21 },
    hasRenderedFirstFrame,
    interactivePatch,
  };
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(ImageCanvas, props));
    await flushMicrotasks(8);
  });
  const normalStages = root.root
    .findAllByType('mock-stage')
    .filter((node) => typeof node.props.onMouseEnter === 'function');
  assert.equal(normalStages.length, 0, 'no ordinary input Stage before picker activation in the clean fixture');
  return { root, props };
}

// Same boundary mocks and mounted production components as the existing consumer
// suite; no copied Point Colour reducer or colour algorithm.
const goodSample = { linearRgb: [0.5, 0.125, 0.0], hue: 30, saturation: 1, value: 0.73535698 };
function resetPoint() {
  resetStores();
  useEditorStore.setState({
    isPointColourPickerActive: false,
    isWbPickerActive: false,
    isRelightPickerActive: false,
    activeRelightLightId: null,
    isGuidedPerspectiveActive: false,
    isStraightenActive: false,
    isRotationActive: false,
    liveRotation: null,
    activeMaskContainerId: null,
    activeMaskId: null,
    activeAiPatchContainerId: null,
    activeAiSubMaskId: null,
    isMaskControlHovered: false,
    isGeneratingAiMask: false,
    isGeneratingAi: false,
    interactivePatch: null,
    isSliderDragging: false,
    previewOverride: null,
    showOriginal: false,
  });
}
async function mountActions() {
  let actions;
  function Actions() {
    actions = useEditorActions();
    return null;
  }
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Actions));
  });
  return {
    root,
    get actions() {
      return actions;
    },
  };
}
function pointer(zoom = 1, x = 80, y = 120) {
  const api = { getPointerPosition: () => ({ x: (160 + x) * zoom, y: (120 + y) * zoom }) };
  return { target: { getStage: () => api }, evt: { button: 0, cancelable: false, preventDefault() {} } };
}
function pointStage(canvas) {
  // The ordinary Stage has enter/leave handlers; the separate crop Stage does not.
  const stages = canvas.root.root
    .findAllByType('mock-stage')
    .filter((node) => typeof node.props.onMouseEnter === 'function');
  assert.equal(stages.length, 1, 'picker activation mounts exactly one ordinary input Stage');
  const stage = stages[0];
  for (const name of ['onMouseDown', 'onMouseMove', 'onMouseUp', 'onTouchStart', 'onTouchMove', 'onTouchEnd'])
    assert.equal(typeof stage.props[name], 'function', `${name} is bound on the ordinary Stage`);
  assert.equal(stage.props.onTouchStart, stage.props.onMouseDown);
  assert.equal(stage.props.onTouchMove, stage.props.onMouseMove);
  assert.equal(stage.props.onTouchEnd, stage.props.onMouseUp);
  return stage;
}
async function activatePoint(canvas) {
  let controls;
  await act(async () => {
    controls = trackedCreate(
      React.createElement(PointColourControls, {
        adjustments: useEditorStore.getState().adjustments,
        setAdjustments: canvas.props.setAdjustments,
      }),
    );
  });
  try {
    const pick = controls.root.findByProps({ 'aria-label': 'Pick a colour' });
    assert.equal(pick.props.disabled, false);
    await act(async () => pick.props.onClick());
    assert.equal(useEditorStore.getState().isPointColourPickerActive, true);
    pointStage(canvas);
  } finally {
    await trackedUnmount(controls);
  }
}

test('pc02 mounted Point Colour maps composed output across zoom DPR crop and orientation', async () => {
  for (const zoom of [1, 2])
    for (const dpr of [1, 2])
      for (const keyed of [true, false]) {
        resetPoint();
        window.devicePixelRatio = dpr;
        const edit = {
          ...INITIAL_ADJUSTMENTS,
          orientationSteps: 1,
          flipHorizontal: true,
          crop: { x: 200, y: 100, width: 1000, height: 750 },
          transformVertical: 12,
        };
        setAccurateImage(ticketA, keyA, '/point.raf', edit);
        if (!keyed) {
          const { openTicket, openKey, ...legacy } = useEditorStore.getState().selectedImage;
          useEditorStore.setState({ selectedImage: { ...legacy, renderMode: 'Legacy' }, progressiveOpen: null });
        }
        invokeHandler = async (command) => {
          assert.equal(command, Invokes.SamplePointColour);
          return goodSample;
        };
        const hooks = await mountActions();
        const canvas = await mountImageCanvas({
          path: '/point.raf',
          setAdjustments: hooks.actions.setAdjustments,
          hasRenderedFirstFrame: true,
          finalPreviewUrl: 'preview:point',
          zoom,
        });
        await activatePoint(canvas);
        await act(async () => {
          pointStage(canvas).props.onMouseDown(pointer(zoom));
          await flushMicrotasks(20);
        });
        assert.equal(calls.length, 1);
        assert.deepEqual(calls[0].args.outputUv, [0.25, 0.5]);
        if (keyed) assert.deepEqual(calls[0].args.expectedOpenKey, keyA);
        else assert.equal(Object.hasOwn(calls[0].args, 'expectedOpenKey'), false);
        assert.equal(calls[0].args.jsAdjustments.orientationSteps, 1);
        assert.equal(useEditorStore.getState().adjustments.pointColour.centerHue, 30);
        assert.equal(useEditorStore.getState().adjustments.crop.x, 200);
        await trackedUnmount(canvas.root);
        await trackedUnmount(hooks.root);
        debouncedSetHistory.cancel();
        debouncedSave.cancel();
      }
  assert.equal(pointColourOutputUv({ x: 320, y: 20 }, 320, 240), null);
  assert.equal(pointColourOutputUv({ x: -1, y: 20 }, 320, 240), null);
  assert.equal(pointColourOutputUv({ x: Number.NaN, y: 20 }, 320, 240), null);
  assert.deepEqual(pointColourOutputUv({ x: 0, y: 0 }, 320, 240), [0, 0]);
});

test('pc03 mounted Point Colour rejects deferred edits ABA cancellation errors and extra clicks', async () => {
  for (const mode of ['edit', 'profile', 'settings', 'pending', 'ABA', 'cancel', 'unmount', 'error', 'neutral']) {
    resetPoint();
    setAccurateImage(ticketA, keyA, '/A.raf');
    const pending = deferred();
    invokeHandler = () => pending.promise;
    const hooks = await mountActions();
    const canvas = await mountImageCanvas({
      path: '/A.raf',
      setAdjustments: hooks.actions.setAdjustments,
      hasRenderedFirstFrame: true,
      finalPreviewUrl: 'preview:A',
    });
    await activatePoint(canvas);
    await act(async () => {
      pointStage(canvas).props.onMouseDown(pointer());
      pointStage(canvas).props.onMouseDown(pointer());
      await flushMicrotasks();
    });
    assert.equal(calls.length, 1, 'one active click; no pending hover queue');
    await act(async () => {
      if (mode === 'edit') hooks.actions.setAdjustments({ exposure: 0.75 });
      if (mode === 'settings') useSettingsStore.getState().setAppSettings({ baselineExposure: 1 });
      if (mode === 'pending')
        useEditorStore.getState().prepareProgressiveIntent('bb451702-eaf1-46d9-9d4f-1d741a1555ab', '/next.raf');
      if (mode === 'profile')
        hooks.actions.setAdjustments({ profile: { base: 'replacement', look: null, amount: 100 } });
      if (mode === 'ABA') {
        const b = '9b451702-eaf1-46d9-9d4f-1d741a1555ab';
        setAccurateImage(b, { ticket: b, generation: '42' }, '/B.raf');
        const a = 'ab451702-eaf1-46d9-9d4f-1d741a1555ab';
        setAccurateImage(a, { ticket: a, generation: '43' }, '/A.raf');
      }
      if (mode === 'cancel') useEditorStore.getState().setEditor({ isPointColourPickerActive: false });
    });
    if (mode === 'unmount') await trackedUnmount(canvas.root);
    const before = useEditorStore.getState().adjustments;
    await act(async () => {
      if (mode === 'error') pending.reject(new Error('actual sampler error'));
      else pending.resolve(mode === 'neutral' ? { ...goodSample, saturation: 0 } : goodSample);
      await flushMicrotasks(20);
    });
    assert.equal(useEditorStore.getState().adjustments, before);
    assert.equal(useEditorStore.getState().adjustments.pointColour, null);
    if (mode === 'error' || mode === 'neutral')
      assert.ok(canvas.root.root.findAllByProps({ role: 'alert' }).length > 0);
    await trackedUnmount(canvas.root);
    await trackedUnmount(hooks.root);
    debouncedSetHistory.cancel();
    debouncedSave.cancel();
  }
});

test('pc04 actual controls retain unsupported payloads and use normal history reset and save', async () => {
  resetPoint();
  setAccurateImage(ticketA, keyA, '/controls.raf');
  const future = { v: 9, saved: 'must remain', hue: 17 };
  assert.deepEqual(normalizeLoadedAdjustments({ ...INITIAL_ADJUSTMENTS, pointColour: future }).pointColour, future);
  assert.equal(pointColourV1(future), null);
  assert.equal(pointColourV1({ v: 1, centerHue: 360, widthDegrees: 40, hue: 0, saturation: 0, luminance: 0 }), null);
  assert.equal(pointColourV1({ v: 1, centerHue: 30, widthDegrees: 40, hue: NaN, saturation: 0, luminance: 0 }), null);
  const point = { v: 1, centerHue: 30, widthDegrees: 40, hue: 0, saturation: 0, luminance: 0 };
  // f32 spacing below 360 is 2^-15; its exact halfway value rounds to even 360.
  for (const centerHue of [359, 359.999969482421875, 359.99998]) {
    const supported = { ...point, centerHue };
    assert.equal(pointColourV1(supported), supported, 'supported hue is borrowed without normalization');
  }
  for (const centerHue of [359.9999847412109375, 359.99999]) {
    const unsupported = { ...point, centerHue };
    assert.equal(pointColourV1(unsupported), null, 'native f32 rounding to 360 is unsupported');
    assert.deepEqual(
      normalizeLoadedAdjustments({ ...INITIAL_ADJUSTMENTS, pointColour: unsupported }).pointColour,
      unsupported,
      'unsupported saved hue is preserved, not normalized or discarded',
    );
    assert.equal(unsupported.centerHue, centerHue);
  }
  useEditorStore.setState({
    adjustments: { ...INITIAL_ADJUSTMENTS, pointColour: point },
    history: [{ ...INITIAL_ADJUSTMENTS, pointColour: point }],
    historyIndex: 0,
  });
  let actions;
  function Controls() {
    actions = useEditorActions();
    const adjustments = useEditorStore((state) => state.adjustments);
    return React.createElement(PointColourControls, {
      adjustments,
      setAdjustments: actions.setAdjustments,
      appSettings: null,
    });
  }
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Controls));
  });
  const sliders = root.root.findAllByType(Slider);
  assert.equal(sliders.length, 4);
  await act(async () => {
    sliders.find((s) => s.props.label === 'Hue').props.onChange({ target: { value: '25' } });
    debouncedSetHistory.flush();
  });
  assert.equal(useEditorStore.getState().adjustments.pointColour.hue, 25);
  await act(async () => useEditorStore.getState().undo());
  assert.equal(useEditorStore.getState().adjustments.pointColour.hue, 0);
  await act(async () => useEditorStore.getState().redo());
  assert.equal(useEditorStore.getState().adjustments.pointColour.hue, 25);
  invokeHandler = async () => null;
  const saved = useEditorStore.getState().adjustments;
  debouncedSave('/controls.raf', saved);
  await debouncedSave.flush();
  assert.equal(calls.at(-1).command, Invokes.SaveMetadataAndUpdateThumbnail);
  assert.deepEqual(calls.at(-1).args.adjustments.pointColour, { ...point, hue: 25 });
  await act(async () =>
    root.root
      .findAllByType('button')
      .find((b) => b.children.includes('Remove Point Colour'))
      .props.onClick(),
  );
  assert.equal(useEditorStore.getState().adjustments.pointColour, null);
  assert.equal(useEditorStore.getState().adjustments.profile, saved.profile);
  await act(async () => useEditorStore.getState().setEditor({ isWbPickerActive: true }));
  await act(async () => root.root.findByProps({ 'aria-label': 'Pick a colour' }).props.onClick());
  assert.equal(useEditorStore.getState().isWbPickerActive, false);
  assert.equal(useEditorStore.getState().isPointColourPickerActive, true);
  await act(async () => useEditorStore.getState().setEditor({ isRelightPickerActive: true }));
  assert.equal(useEditorStore.getState().isPointColourPickerActive, false);
  await trackedUnmount(root);
});

test('pc07 real store identity no-ops stay silent while edits and picker transitions publish', () => {
  resetPoint();
  setAccurateImage(ticketA, keyA, '/store-noop.raf');
  const admitted = useEditorStore.getState();
  assert.equal(admitted.selectedImage.isReady, true);
  assert.equal(admitted.progressiveOpen.phase, 'Accurate');
  assert.equal(admitted.pendingOpenIntent, null);
  let notifications = 0;
  const unsubscribe = useEditorStore.subscribe(() => {
    notifications += 1;
  });
  const checkIdentityNoop = (label) => {
    const before = useEditorStore.getState();
    const count = notifications;
    before.setEditor((state) => state);
    const after = useEditorStore.getState();
    assert.deepEqual(
      {
        stateSame: Object.is(after, before),
        notifications: notifications - count,
        adjustmentsSame: Object.is(after.adjustments, before.adjustments),
        historySame: Object.is(after.history, before.history),
      },
      {
        stateSame: true,
        notifications: 0,
        adjustmentsSame: true,
        historySame: true,
      },
      label,
    );
  };
  try {
    checkIdentityNoop('ready source updater identity');
    const edited = { ...admitted.adjustments, exposure: 0.625 };
    admitted.setEditor({ adjustments: edited });
    assert.notEqual(useEditorStore.getState(), admitted, 'real edits still replace state');
    assert.equal(useEditorStore.getState().adjustments, edited);
    assert.equal(useEditorStore.getState().history, admitted.history);
    assert.equal(notifications, 1, 'one real edit publishes one subscription notification');
    checkIdentityNoop('after a real edit');
    for (const mode of ['point', 'white balance', 'relight']) {
      const before = useEditorStore.getState();
      const count = notifications;
      before.setEditor({
        isPointColourPickerActive: mode === 'point',
        isWbPickerActive: mode === 'white balance',
        isRelightPickerActive: mode === 'relight',
      });
      const after = useEditorStore.getState();
      assert.notEqual(after, before, `${mode}: transition replaces state`);
      assert.equal(notifications, count + 1, `${mode}: transition notifies once`);
      assert.equal(after.isPointColourPickerActive, mode === 'point');
      assert.equal(after.isWbPickerActive, mode === 'white balance');
      assert.equal(after.isRelightPickerActive, mode === 'relight');
      assert.equal(after.adjustments, edited, 'picker transitions do not edit pixels');
      assert.equal(after.history, admitted.history, 'picker transitions preserve history');
      checkIdentityNoop(`${mode}: updater identity`);
    }
  } finally {
    unsubscribe();
  }
});
