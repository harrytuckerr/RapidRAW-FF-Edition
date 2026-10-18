import assert from 'node:assert/strict';
import { after, afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import { build, stop as stopEsbuild } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);
const bundle = await build({
  stdin: {
    contents: `export { useImageLoader } from './src/hooks/useImageLoader';
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
            `export const useTranslation = () => ({ t: (key, options) => { globalThis.__progressiveTranslations?.push({ key, options }); return key; }, i18n: { language: 'en' } });`,
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
    isWbPickerActive: true,
    onWbPicked() {},
    setAdjustments,
    cursorStyle: 'default',
    transformState: { scale: 1, positionX: 0, positionY: 0 },
    hasRenderedFirstFrame,
    interactivePatch,
  };
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(ImageCanvas, props));
    await flushMicrotasks(8);
  });
  const stage = root.root
    .findAllByType('mock-stage')
    .find(
      (candidate) =>
        typeof candidate.props.onMouseMove === 'function' && typeof candidate.props.onMouseDown === 'function',
    );
  assert.ok(stage, 'the mounted ImageCanvas should expose its actual white-balance pointer stage');
  return { root, stage, props };
}

test('mounted loader waits for tracked save, installs listener before keyed admission, and bypasses cached adjustment restoration', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useEditorStore.getState().setEditor({ adjustments: { ...useEditorStore.getState().adjustments, exposure: 1.2 } });
  useLibraryStore
    .getState()
    .setLibrary({ imageList: [{ path: '/A.raf' }, { path: '/B.raf' }], libraryActivePath: '/A.raf' });
  useProcessStore.getState().setProcess({ mediumThumbnails: { '/B.raf': 'thumb:B' } });

  const cacheCalls = { get: 0, set: 0 };
  const originalGet = globalImageCache.get;
  const originalSet = globalImageCache.set;
  globalImageCache.get = function (...args) {
    cacheCalls.get++;
    return originalGet.apply(this, args);
  };
  globalImageCache.set = function (...args) {
    cacheCalls.set++;
    return originalSet.apply(this, args);
  };
  globalImageCache.clear();

  const saveGate = deferred();
  const loadGate = deferred();
  let openedKey;
  invokeHandler = (command, args) => {
    if (command === 'cancel_image_open') return Promise.resolve('AlreadyFinished');
    if (command === 'save_metadata_and_update_thumbnail') return saveGate.promise;
    if (command === 'begin_image_open') {
      openedKey = { ticket: args.ticket, generation: '42' };
      return Promise.resolve({
        openKey: openedKey,
        metadata: { adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 2.3 } },
      });
    }
    if (command === 'load_image') return loadGate.promise;
    throw new Error(`Unexpected command ${command}`);
  };

  const saveState = useEditorStore.getState().adjustments;
  debouncedSave('/A.raf', saveState);
  const { Probe, values } = useTestHooks({ withLoader: true, withNavigation: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  values.cachedRef.current = { adjustments: saveState, selectedImage: useEditorStore.getState().selectedImage };

  let selecting;
  await act(async () => {
    selecting = values.nav.handleImageSelect('/B.raf');
    await flushMicrotasks(8);
  });
  assert.equal(useEditorStore.getState().pendingOpenIntent?.path, '/B.raf');
  assert.equal(
    calls.some((call) => call.command === 'begin_image_open'),
    false,
  );
  assert.equal(listeners.has('xtrans-placeholder-ready'), false);
  assert.equal(calls.filter((call) => call.command === 'save_metadata_and_update_thumbnail').length, 1);

  saveGate.resolve();
  await act(async () => {
    await selecting;
    await flushMicrotasks(20);
  });
  assert.equal(listeners.has('xtrans-placeholder-ready'), true);
  const commandNames = calls.map((call) => call.command);
  assert.ok(commandNames.indexOf('save_metadata_and_update_thumbnail') < commandNames.indexOf('begin_image_open'));
  assert.ok(commandNames.indexOf('begin_image_open') < commandNames.indexOf('load_image'));
  assert.equal(
    calls.some((call) => call.command === 'load_metadata'),
    false,
  );
  assert.equal(
    calls.some((call) => call.command === 'clear_session_caches'),
    false,
  );
  assert.equal(cacheCalls.get, 0);
  assert.equal(cacheCalls.set, 0);
  assert.equal(useEditorStore.getState().adjustments.exposure, 2.3);
  const admissionHistory = useEditorStore.getState().history;

  const revoked = [];
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  let urlNumber = 0;
  URL.createObjectURL = () => `blob:progressive-${++urlNumber}`;
  URL.revokeObjectURL = (url) => revoked.push(url);
  const originalImage = globalThis.Image;
  globalThis.Image = class {
    naturalWidth = 320;
    naturalHeight = 200;
    set src(value) {
      this.source = value;
    }
    async decode() {}
  };
  try {
    const placeholderEvent = {
      payload: {
        schemaVersion: 1,
        openKey: openedKey,
        width: 320,
        height: 200,
        mime: 'image/jpeg',
        dataBase64: '/9j/2Q==',
      },
    };
    await act(async () => {
      listeners.get('xtrans-placeholder-ready')(placeholderEvent);
      listeners.get('xtrans-placeholder-ready')(placeholderEvent);
      await flushMicrotasks(8);
    });
    assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Refining');
    assert.equal(useEditorStore.getState().selectedImage.isReady, false);
    assert.equal(useEditorStore.getState().history, admissionHistory);
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);

    loadGate.resolve({
      openKey: openedKey,
      width: 4000,
      height: 3000,
      metadata: { adjustments: { exposure: -8 } },
      exif: null,
      is_raw: true,
      as_shot_white_balance: null,
      renderMode: 'AccurateXtrans1Encoded',
    });
    await act(async () => {
      await flushMicrotasks(16);
    });
    assert.equal(useEditorStore.getState().selectedImage.isReady, true);
    assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Accurate');
    assert.equal(useEditorStore.getState().progressiveOpen.placeholderUrl, null);
    assert.equal(useEditorStore.getState().adjustments.exposure, 2.3);
    assert.equal(useEditorStore.getState().history, admissionHistory);
    assert.deepEqual(revoked, ['blob:progressive-1']);
    assert.equal(listeners.has('xtrans-placeholder-ready'), false);
    assert.equal(urlNumber, 1);
  } finally {
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    globalThis.Image = originalImage;
    await trackedUnmount(root);
    assert.equal(
      calls.some((call) => call.command === 'cancel_image_open' && call.args?.openKey?.ticket === openedKey.ticket),
      false,
    );
    globalImageCache.get = originalGet;
    globalImageCache.set = originalSet;
    globalImageCache.clear();
  }
});

test('legacy unkeyed accurate source retains its existing cache handoff while keyed targets are never restored from it', async () => {
  resetStores();
  useEditorStore.setState({
    selectedImage: {
      path: '/legacy.raf',
      isReady: true,
      width: 1000,
      height: 800,
      isRaw: true,
      exif: null,
      metadata: null,
      thumbnailUrl: 'thumb:legacy',
    },
    progressiveOpen: null,
    pendingOpenIntent: null,
  });
  useLibraryStore
    .getState()
    .setLibrary({ imageList: [{ path: '/legacy.raf' }, { path: '/fresh.raf' }], libraryActivePath: '/legacy.raf' });
  const { Probe, values } = useTestHooks({ withNavigation: true });
  const cacheCalls = [];
  const originalSet = globalImageCache.set;
  const originalGet = globalImageCache.get;
  globalImageCache.set = function (path, entry) {
    cacheCalls.push({ kind: 'set', path });
    return originalSet.call(this, path, entry);
  };
  globalImageCache.get = function (path) {
    cacheCalls.push({ kind: 'get', path });
    return originalGet.call(this, path);
  };
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  values.cachedRef.current = {
    adjustments: INITIAL_ADJUSTMENTS,
    histogram: null,
    waveform: null,
    finalPreviewUrl: null,
    uncroppedPreviewUrl: null,
    selectedImage: useEditorStore.getState().selectedImage,
    originalSize: { width: 1000, height: 800 },
    previewSize: { width: 1000, height: 800 },
  };
  invokeHandler = async () => null;
  await act(async () => {
    await values.nav.handleImageSelect('/fresh.raf');
  });
  assert.deepEqual(
    cacheCalls.filter((call) => call.path === '/legacy.raf'),
    [{ kind: 'set', path: '/legacy.raf' }],
  );
  assert.equal(
    cacheCalls.some((call) => call.kind === 'get' && call.path === '/fresh.raf'),
    false,
  );
  assert.equal(useEditorStore.getState().selectedImage.openTicket !== undefined, true);
  await trackedUnmount(root);
});

test('SavePending failure leaves the prior accurate editor intact and never admits the requested target', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useEditorStore.getState().setEditor({ adjustments: { ...useEditorStore.getState().adjustments, exposure: 1.4 } });
  useLibraryStore
    .getState()
    .setLibrary({ imageList: [{ path: '/A.raf' }, { path: '/B.raf' }], libraryActivePath: '/A.raf' });
  const saveGate = deferred();
  invokeHandler = (command) => {
    if (command === 'cancel_image_open') return Promise.resolve('AlreadyFinished');
    if (command === 'save_metadata_and_update_thumbnail') return saveGate.promise;
    throw new Error(`Unexpected command ${command}`);
  };
  debouncedSave('/A.raf', useEditorStore.getState().adjustments);
  const { Probe, values } = useTestHooks({ withLoader: true, withNavigation: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  let selecting;
  await act(async () => {
    selecting = values.nav.handleImageSelect('/B.raf');
    await flushMicrotasks(8);
  });
  saveGate.reject(new Error('disk full'));
  await act(async () => {
    await selecting;
    await flushMicrotasks(8);
  });
  assert.equal(useEditorStore.getState().pendingOpenIntent, null);
  assert.equal(useEditorStore.getState().selectedImage.path, '/A.raf');
  assert.equal(useEditorStore.getState().selectedImage.isReady, true);
  assert.equal(useEditorStore.getState().adjustments.exposure, 1.4);
  assert.equal(
    calls.some((call) => call.command === 'begin_image_open'),
    false,
  );

  invokeHandler = async () => undefined;
  debouncedSave('/A.raf', useEditorStore.getState().adjustments);
  assert.equal(await flushEditorSavesBeforeOpen(), true);
  await trackedUnmount(root);
});

test('navigation unmount while waiting for a target save cannot activate or publish the pending selection later', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useLibraryStore
    .getState()
    .setLibrary({ imageList: [{ path: '/A.raf' }, { path: '/B.raf' }], libraryActivePath: '/A.raf' });
  const saveGate = deferred();
  invokeHandler = (command) => {
    if (command === 'cancel_image_open') return Promise.resolve('AlreadyFinished');
    if (command === 'save_metadata_and_update_thumbnail') return saveGate.promise;
    throw new Error(`Unexpected command ${command}`);
  };
  debouncedSave('/A.raf', useEditorStore.getState().adjustments);
  const { Probe, values } = useTestHooks({ withNavigation: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  let selecting;
  await act(async () => {
    selecting = values.nav.handleImageSelect('/B.raf');
    await flushMicrotasks(8);
  });
  await trackedUnmount(root);
  const libraryPathAfterUnmount = useLibraryStore.getState().libraryActivePath;
  invokeHandler = async () => undefined;
  saveGate.resolve();
  await selecting;
  await flushMicrotasks(8);
  assert.equal(useEditorStore.getState().pendingOpenIntent, null);
  assert.equal(useEditorStore.getState().selectedImage.path, '/A.raf');
  assert.equal(
    calls.some((call) => call.command === 'begin_image_open'),
    false,
  );
  assert.equal(useLibraryStore.getState().libraryActivePath, libraryPathAfterUnmount);
  debouncedSave('/A.raf', useEditorStore.getState().adjustments);
  assert.equal(await flushEditorSavesBeforeOpen(), true);
});

test('unmount during deferred native admission cancels the exact returned key before load_image', async () => {
  resetStores();
  const lateTicket = '27c0d2ce-e03b-4fcb-96a1-5eb9a18e5b3a';
  const lateKey = { ticket: lateTicket, generation: '88' };
  useEditorStore.getState().prepareProgressiveIntent(lateTicket, '/late.raf');
  useEditorStore.getState().activateProgressiveIntent(lateTicket, '/late.raf', '');
  const beginGate = deferred();
  invokeHandler = (command) => {
    if (command === 'begin_image_open') return beginGate.promise;
    if (command === 'cancel_image_open') return Promise.resolve('Cancelled');
    return Promise.resolve(null);
  };
  const { Probe } = useTestHooks({ withLoader: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(React.StrictMode, null, React.createElement(Probe)));
    await flushMicrotasks(8);
  });
  assert.equal(calls.filter((call) => call.command === 'begin_image_open').length, 1);
  assert.equal(listeners.has('xtrans-placeholder-ready'), true);
  await trackedUnmount(root);
  await flushMicrotasks(3);
  beginGate.resolve({ openKey: lateKey, metadata: { adjustments: INITIAL_ADJUSTMENTS } });
  await flushMicrotasks(20);
  assert.equal(
    calls.some((call) => call.command === 'load_image'),
    false,
  );
  const cancellation = calls.find((call) => call.command === 'cancel_image_open');
  assert.deepEqual(cancellation.args.openKey, lateKey);
  assert.equal(unlistenCount, 1);
});

test('late listener setup after unmount is unlistened and never starts native admission', async () => {
  resetStores();
  const lateTicket = '6b1d1e7a-4b0f-4eaa-b6c1-b0434c8920d1';
  useEditorStore.getState().prepareProgressiveIntent(lateTicket, '/late-listener.raf');
  useEditorStore.getState().activateProgressiveIntent(lateTicket, '/late-listener.raf', '');
  const listenerGate = deferred();
  listenHandler = (name, callback) => {
    listeners.set(name, callback);
    return listenerGate.promise;
  };
  invokeHandler = async () => {
    throw new Error('begin_image_open must not run after unmount');
  };
  const { Probe } = useTestHooks({ withLoader: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(8);
  });
  assert.equal(listeners.has('xtrans-placeholder-ready'), true);
  await trackedUnmount(root);
  await flushMicrotasks(4);
  listenerGate.resolve(() => {
    unlistenCount++;
    listeners.clear();
  });
  await flushMicrotasks(12);
  assert.equal(
    calls.some((call) => call.command === 'begin_image_open'),
    false,
  );
  assert.equal(unlistenCount, 1);
});

test('listener setup failure skips only the placeholder and still completes the accurate keyed load', async () => {
  resetStores();
  const openTicket = 'e53246d1-6e20-4e1c-8a03-1fe9d3f60e41';
  useEditorStore.getState().prepareProgressiveIntent(openTicket, '/listener-failure.raf');
  useEditorStore.getState().activateProgressiveIntent(openTicket, '/listener-failure.raf', '');
  listenHandler = async () => {
    throw new Error('listener unavailable');
  };
  invokeHandler = async (command, args) => {
    if (command === 'begin_image_open')
      return { openKey: { ticket: args.ticket, generation: '91' }, metadata: { adjustments: INITIAL_ADJUSTMENTS } };
    if (command === 'load_image')
      return {
        openKey: args.openKey,
        width: 1600,
        height: 1200,
        metadata: {},
        exif: null,
        is_raw: true,
        as_shot_white_balance: null,
        renderMode: 'AccurateXtrans1Encoded',
      };
    throw new Error(`Unexpected command ${command}`);
  };
  const { Probe } = useTestHooks({ withLoader: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(20);
  });
  assert.equal(useEditorStore.getState().selectedImage.isReady, true);
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Accurate');
  assert.equal(
    calls.some((call) => call.command === 'begin_image_open'),
    true,
  );
  assert.equal(
    calls.some((call) => call.command === 'load_image'),
    true,
  );
  assert.equal(listeners.has('xtrans-placeholder-ready'), false);
  await trackedUnmount(root);
});

test('actual Tauri listener consumers ignore pending, old, missing and same-path stale keys', async () => {
  resetStores();
  const { Probe, values } = useTestHooks({ withListeners: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks();
  });
  const store = useEditorStore.getState();
  await act(async () => {
    store.prepareProgressiveIntent(ticketA, '/same.raf');
    store.activateProgressiveIntent(ticketA, '/same.raf', 'thumb');
    store.acceptProgressiveAdmission(ticketA, '/same.raf', keyA, {}, INITIAL_ADJUSTMENTS);
  });
  assert.equal(listeners.has('analytics-update'), true);

  await act(async () => {
    listeners.get('analytics-update')({
      payload: { path: '/same.raf', openKey: keyA, histogram: { a: 1 }, waveform: { a: 2 } },
    });
    listeners.get('wgpu-frame-ready')({ payload: { path: '/same.raf', openKey: keyA } });
    listeners.get('preview-update-uncropped')({ payload: 'data:image/jpeg;base64,old' });
  });
  assert.equal(useEditorStore.getState().histogram, null);
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, false);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null);

  await act(async () => {
    assert.equal(
      store.completeProgressiveLoad(ticketA, '/same.raf', keyA, {
        width: 1000,
        height: 800,
        previewSize: { width: 1000, height: 800 },
        exif: null,
        is_raw: true,
        metadata: {},
        as_shot_white_balance: null,
        renderMode: 'AccurateXtrans1Encoded',
      }),
      true,
    );
  });
  await act(async () => {
    listeners.get('analytics-update')({
      payload: { path: '/same.raf', openKey: { ticket: ticketA, generation: '40' }, histogram: { old: 1 } },
    });
    listeners.get('analytics-update')({ payload: { path: '/same.raf', histogram: { missingKey: 1 } } });
    listeners.get('wgpu-frame-ready')({
      payload: { path: '/same.raf', openKey: { ticket: ticketA, generation: '40' } },
    });
  });
  assert.equal(useEditorStore.getState().histogram, null);
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, false);
  await act(async () => {
    listeners.get('analytics-update')({
      payload: { path: '/same.raf', openKey: keyA, histogram: { current: 1 }, waveform: { current: 2 } },
    });
    listeners.get('wgpu-frame-ready')({ payload: { path: '/same.raf', openKey: keyA } });
    listeners.get('preview-update-uncropped')({ payload: 'data:image/jpeg;base64,keyless' });
  });
  assert.deepEqual(useEditorStore.getState().histogram, { current: 1 });
  // An encoded keyed source remains on CPU display even when a native frame event matches its OpenKey.
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, false);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null);
  await act(async () => {
    useEditorStore.setState({
      selectedImage: {
        path: '/legacy.raf',
        isReady: true,
        width: 1000,
        height: 800,
        isRaw: false,
        exif: null,
        thumbnailUrl: '',
      },
      progressiveOpen: null,
      pendingOpenIntent: null,
      histogram: null,
      hasRenderedFirstFrame: false,
    });
    listeners.get('analytics-update')({ payload: { path: '/legacy.raf', histogram: { legacy: 1 } } });
    listeners.get('wgpu-frame-ready')({ payload: { path: '/legacy.raf' } });
    listeners.get('preview-update-uncropped')({ payload: 'data:image/jpeg;base64,legacy' });
  });
  assert.deepEqual(useEditorStore.getState().histogram, { legacy: 1 });
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, true);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, 'data:image/jpeg;base64,legacy');
  await trackedUnmount(root);
});

test('mounted Details used by global and mask sections disables real slider callbacks while keyed data is not ready', async () => {
  resetStores();
  useEditorStore.getState().prepareProgressiveIntent(ticketA, '/pending.raf');
  useEditorStore.getState().activateProgressiveIntent(ticketA, '/pending.raf', 'thumb');
  useEditorStore.getState().acceptProgressiveAdmission(ticketA, '/pending.raf', keyA, {}, INITIAL_ADJUSTMENTS);
  useEditorStore.getState().claimProgressivePlaceholder(ticketA, '/pending.raf', keyA);
  useEditorStore.getState().setProgressivePlaceholder(ticketA, '/pending.raf', keyA, 'blob:fit');
  let actions;
  function Probe() {
    actions = useEditorActions();
    const adjustments = useEditorStore((state) => state.adjustments);
    return React.createElement(DetailsPanel, {
      adjustments,
      setAdjustments: actions.setAdjustments,
      appSettings: null,
      isForMask: true,
    });
  }
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  const sharpness = root.root.find((node) => node.props.label === 'adjustments.details.sharpness');
  assert.equal(sharpness.props.disabled, true);
  await act(async () => sharpness.props.onChange({ target: { value: '45' } }));
  assert.equal(useEditorStore.getState().adjustments.sharpness, INITIAL_ADJUSTMENTS.sharpness);
  assert.equal(useEditorStore.getState().historyIndex, 0);
  await trackedUnmount(root);
});

test('mounted MasksPanel disables its real controls during a deferred open intent', async () => {
  resetStores();
  invokeHandler = async () => [];
  useEditorStore.getState().prepareProgressiveIntent(ticketA, '/pending.raf');
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(MasksPanel));
  });
  const panelControls = root.root.findByType('fieldset');
  assert.equal(panelControls.props.disabled, true);
  await trackedUnmount(root);
});

test('actual editor renderer rejects an A response after same-path A/B/A navigation and keeps the WGPU sentinel branch', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/same.raf');
  useUIStore.setState({ activeView: 'library' });
  useEditorStore.setState({ isSliderDragging: true });
  let previewGate = deferred();
  let decodeGate = deferred();
  const createdUrls = [];
  const revokedUrls = [];
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  const originalImage = globalThis.Image;
  URL.createObjectURL = () => {
    const value = `blob:preview-${createdUrls.length + 1}`;
    createdUrls.push(value);
    return value;
  };
  URL.revokeObjectURL = (value) => revokedUrls.push(value);
  globalThis.Image = class {
    set src(value) {
      this.source = value;
    }
    decode() {
      return decodeGate.promise;
    }
  };
  const { Probe, values } = useTestHooks({ withProcessing: true });
  let root;
  try {
    await act(async () => {
      root = trackedCreate(React.createElement(Probe));
      await flushMicrotasks();
    });
    invokeHandler = (command) => {
      if (command === 'apply_adjustments') return previewGate.promise;
      throw new Error(`Unexpected renderer command ${command}`);
    };
    await act(async () => {
      values.process.executeApplyAdjustments(useEditorStore.getState().adjustments);
      await flushMicrotasks(8);
    });
    const request = calls.find((call) => call.command === 'apply_adjustments');
    assert.deepEqual(request.args.expectedOpenKey, keyA);

    await act(async () => {
      previewGate.resolve(new Uint8Array([255, 216, 255, 217]).buffer);
      await flushMicrotasks(8);
    });
    assert.equal(createdUrls.length, 1);

    const ticketForB = 'ba001d9f-18e1-44a5-812a-5cde8845e2bb';
    const keyForB = { ticket: ticketForB, generation: '44' };
    await act(async () => setAccurateImage(ticketForB, keyForB, '/other.raf'));
    const ticketForA2 = '267e12fe-2a0a-476e-8c52-c64a9dfe5178';
    const keyForA2 = { ticket: ticketForA2, generation: '45' };
    await act(async () => {
      setAccurateImage(ticketForA2, keyForA2, '/same.raf');
      decodeGate.resolve();
      await flushMicrotasks(12);
    });
    assert.equal(useEditorStore.getState().selectedImage.path, '/same.raf');
    assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyForA2);
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);
    assert.deepEqual(revokedUrls, ['blob:preview-1']);

    previewGate = deferred();
    previewGate.resolve(new TextEncoder().encode('WGPU_RENDER').buffer);
    invokeHandler = (command) => (command === 'apply_adjustments' ? previewGate.promise : Promise.resolve(null));
    await act(async () => {
      values.process.executeApplyAdjustments(useEditorStore.getState().adjustments);
      await flushMicrotasks(12);
    });
    assert.equal(createdUrls.length, 1);
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);
  } finally {
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    globalThis.Image = originalImage;
    if (root) await trackedUnmount(root);
  }
});

test('actual AI-depth consumer forwards the accurate key and ignores a same-path stale completion', async () => {
  resetStores();
  const originalAdjustments = {
    ...INITIAL_ADJUSTMENTS,
    masks: [{ id: 'container-1', subMasks: [{ id: 'mask-1', parameters: { feather: 5 } }] }],
  };
  setAccurateImage(ticketA, keyA, '/same.raf', originalAdjustments);
  const aiGate = deferred();
  invokeHandler = (command) => {
    if (command === 'generate_ai_depth_mask') return aiGate.promise;
    return Promise.resolve(null);
  };
  const { Probe, values } = useTestHooks({ withAiMasking: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  let generating;
  await act(async () => {
    generating = values.ai.handleGenerateAiDepthMask('mask-1', { minDepth: 10, maxDepth: 90 });
    await flushMicrotasks(8);
  });
  const request = calls.find((call) => call.command === 'generate_ai_depth_mask');
  assert.deepEqual(request.args.expectedOpenKey, keyA);

  const ticketForB = 'ba001d9f-18e1-44a5-812a-5cde8845e2bb';
  const keyForB = { ticket: ticketForB, generation: '44' };
  await act(async () => setAccurateImage(ticketForB, keyForB, '/other.raf', originalAdjustments));
  const ticketForA2 = '267e12fe-2a0a-476e-8c52-c64a9dfe5178';
  const keyForA2 = { ticket: ticketForA2, generation: '45' };
  await act(async () => setAccurateImage(ticketForA2, keyForA2, '/same.raf', originalAdjustments));
  aiGate.resolve({ feather: 88 });
  await act(async () => {
    await generating;
    await flushMicrotasks(8);
  });
  assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyForA2);
  assert.equal(useEditorStore.getState().adjustments.masks[0].subMasks[0].parameters.feather, 5);
  await trackedUnmount(root);
});

test('mounted context menu reset is inert for pending, refining, failed, and cancelled source owners', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  const { root, menus } = await mountContextMenus();
  const ticketB = 'cf25f4ce-7753-4e77-9b0e-fc2295ca7bb5';
  const keyB = { ticket: ticketB, generation: '42' };

  for (const phase of ['Pending', 'Refining', 'Failed', 'Cancelled']) {
    const selectedImage = {
      ...useEditorStore.getState().selectedImage,
      path: '/B.raf',
      openTicket: ticketB,
      openKey: keyB,
      isReady: phase === 'Failed' || phase === 'Cancelled',
    };
    useEditorStore.setState({
      selectedImage,
      pendingOpenIntent: phase === 'Pending' ? { ticket: ticketB, path: '/B.raf' } : null,
      progressiveOpen: {
        ticket: ticketB,
        path: '/B.raf',
        openKey: phase === 'Pending' ? null : keyB,
        phase,
        saveReady: true,
        placeholderUrl: null,
        placeholderUsed: phase === 'Refining',
        error: phase === 'Failed' ? 'load failed' : null,
      },
    });
    const before = useEditorStore.getState();
    const options = await showEditorContextMenu(menus);
    const confirm = editorResetConfirmation(options);
    await act(async () => confirm.onClick());
    const after = useEditorStore.getState();
    assert.equal(after.adjustments, before.adjustments, `${phase} must not replace adjustments`);
    assert.equal(after.history, before.history, `${phase} must not replace history`);
    assert.equal(after.historyIndex, before.historyIndex, `${phase} must not move history`);
  }

  await trackedUnmount(root);
});

test('mounted context reset commits only the captured accurate key and preserves accurate legacy unkeyed behavior', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  const current = useEditorStore.getState();
  const prior = { ...current.adjustments, exposure: 1.3, aiPatches: [{ id: 'old-patch' }] };
  setEditorHistoryState(prior);
  const { root, menus } = await mountContextMenus();

  let options = await showEditorContextMenu(menus);
  await act(async () => editorResetConfirmation(options).onClick());
  const reset = { ...INITIAL_ADJUSTMENTS, aspectRatio: 4 / 3, aiPatches: [] };
  assert.deepEqual(useEditorStore.getState().adjustments, reset);
  assert.deepEqual(useEditorStore.getState().history, [reset]);
  assert.equal(useEditorStore.getState().historyIndex, 0);

  const keyedImage = useEditorStore.getState().selectedImage;
  const legacyImage = { ...keyedImage };
  delete legacyImage.openTicket;
  delete legacyImage.openKey;
  useEditorStore.setState({ selectedImage: legacyImage, progressiveOpen: null, pendingOpenIntent: null });
  const legacyPrior = { ...INITIAL_ADJUSTMENTS, exposure: -0.8, aiPatches: [{ id: 'legacy-patch' }] };
  setEditorHistoryState(legacyPrior);
  options = await showEditorContextMenu(menus);
  await act(async () => editorResetConfirmation(options).onClick());
  assert.deepEqual(useEditorStore.getState().adjustments, reset);
  assert.deepEqual(useEditorStore.getState().history, [reset]);
  assert.equal(useEditorStore.getState().historyIndex, 0);
  await trackedUnmount(root);
});

test('stale reset confirmation cannot affect B or a same-path A/B/A replacement', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  const { root, menus } = await mountContextMenus();
  let options = await showEditorContextMenu(menus);
  const oldAConfirmation = editorResetConfirmation(options);

  const ticketB = '0bb573fb-f66f-42ee-b4a1-349265b2c85c';
  const keyB = { ticket: ticketB, generation: '42' };
  const bAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: 0.9 };
  setAccurateImage(ticketB, keyB, '/B.raf', bAdjustments);
  setEditorHistoryState(bAdjustments);
  const beforeB = useEditorStore.getState();
  await act(async () => oldAConfirmation.onClick());
  assert.equal(useEditorStore.getState().adjustments, beforeB.adjustments);
  assert.equal(useEditorStore.getState().history, beforeB.history);

  const ticketA2 = 'fc581245-cfba-47df-85a7-5a77026c2ec3';
  const keyA2 = { ticket: ticketA2, generation: '43' };
  setAccurateImage(ticketA2, keyA2, '/A.raf', { ...INITIAL_ADJUSTMENTS, exposure: 0.4 });
  options = await showEditorContextMenu(menus);
  const staleA2Confirmation = editorResetConfirmation(options);
  const ticketB2 = '624a80fe-5c67-4577-8f43-7dfce86bc80f';
  const keyB2 = { ticket: ticketB2, generation: '44' };
  setAccurateImage(ticketB2, keyB2, '/B.raf', { ...INITIAL_ADJUSTMENTS, exposure: 1.1 });
  const ticketA3 = '8a9271d4-4af8-4930-9ec1-6482c8c7c88f';
  const keyA3 = { ticket: ticketA3, generation: '45' };
  setAccurateImage(ticketA3, keyA3, '/A.raf', { ...INITIAL_ADJUSTMENTS, exposure: -0.5 });
  setEditorHistoryState(useEditorStore.getState().adjustments);
  const beforeA3 = useEditorStore.getState();
  await act(async () => staleA2Confirmation.onClick());
  assert.equal(useEditorStore.getState().selectedImage.path, '/A.raf');
  assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyA3);
  assert.equal(useEditorStore.getState().adjustments, beforeA3.adjustments);
  assert.equal(useEditorStore.getState().history, beforeA3.history);
  await trackedUnmount(root);
});

test('thumbnail batch metadata refresh commits once only for the same accurate editor owner', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'library' });
  useLibraryStore.setState({
    multiSelectedPaths: ['/A.raf'],
    libraryActivePath: null,
    imageList: [{ path: '/A.raf' }],
  });
  const prior = { ...INITIAL_ADJUSTMENTS, exposure: 0.6 };
  setEditorHistoryState(prior);
  const nativeAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: 1.7 };
  invokeHandler = async (command, args) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return true;
    if (command === Invokes.LoadMetadata && args.path === '/A.raf')
      return { adjustments: nativeAdjustments, is_null: false };
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/A.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  assert.ok(auto, 'the real thumbnail context menu should expose its batch callback');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(16);
  });

  const batch = calls.find((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths);
  assert.deepEqual(batch.args.paths, ['/A.raf']);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 1);
  const normalized = normalizeLoadedAdjustments(nativeAdjustments);
  assert.deepEqual(useEditorStore.getState().adjustments, normalized);
  assert.deepEqual(useEditorStore.getState().history, [normalized]);
  assert.equal(useEditorStore.getState().historyIndex, 0);
  await trackedUnmount(root);
});

test('thumbnail batch skips stale selected-source metadata after navigation while unrelated library refresh continues', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'library' });
  useLibraryStore.setState({
    multiSelectedPaths: ['/A.raf', '/other.raf'],
    libraryActivePath: '/other.raf',
    imageList: [{ path: '/A.raf' }, { path: '/other.raf' }],
  });
  const selectedMetadata = deferred();
  const unrelatedLibraryAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: 2.2 };
  invokeHandler = (command, args) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return Promise.resolve(true);
    if (command === Invokes.LoadMetadata && args.path === '/A.raf') return selectedMetadata.promise;
    if (command === Invokes.LoadMetadata && args.path === '/other.raf')
      return Promise.resolve({ adjustments: unrelatedLibraryAdjustments, is_null: false });
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/A.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(12);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata)[0].args.path, '/A.raf');

  const ticketB = '9c1556b6-2525-48b6-9097-8c54d43dbe8f';
  const keyB = { ticket: ticketB, generation: '46' };
  const bAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: -1.3 };
  setAccurateImage(ticketB, keyB, '/B.raf', bAdjustments);
  setEditorHistoryState(bAdjustments);
  const beforeB = useEditorStore.getState();
  await act(async () => {
    selectedMetadata.resolve({ adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 3.4 }, is_null: false });
    await flushMicrotasks(16);
  });
  assert.equal(useEditorStore.getState().adjustments, beforeB.adjustments);
  assert.equal(useEditorStore.getState().history, beforeB.history);
  assert.equal(useEditorStore.getState().historyIndex, beforeB.historyIndex);
  assert.equal(useLibraryStore.getState().libraryActiveAdjustments.exposure, unrelatedLibraryAdjustments.exposure);
  assert.deepEqual(
    calls.filter((call) => call.command === Invokes.LoadMetadata).map((call) => call.args.path),
    ['/A.raf', '/other.raf'],
  );
  await trackedUnmount(root);
});

test('thumbnail batch error preserves editor state and does not request stale metadata', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'library' });
  useLibraryStore.setState({ multiSelectedPaths: ['/A.raf'], libraryActivePath: null });
  const prior = { ...INITIAL_ADJUSTMENTS, exposure: 0.3 };
  setEditorHistoryState(prior);
  invokeHandler = (command) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return Promise.reject(new Error('batch failed'));
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/A.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(16);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths).length, 1);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 0);
  assert.equal(useEditorStore.getState().adjustments, prior);
  assert.equal(useEditorStore.getState().history[1], prior);
  assert.equal(useEditorStore.getState().historyIndex, 1);
  await trackedUnmount(root);
});

test('pending open through the editor filmstrip preserves unrelated batch targets without a second editor initialization', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'editor' });
  useLibraryStore.setState({
    multiSelectedPaths: ['/A.raf', '/B.raf', '/C.raf'],
    imageList: [{ path: '/A.raf' }, { path: '/B.raf' }, { path: '/C.raf' }],
    libraryActivePath: null,
  });
  const before = useEditorStore.getState();
  const ticketB = '71ad8467-c696-4286-8aec-23dd7682ce77';
  useEditorStore.getState().prepareProgressiveIntent(ticketB, '/B.raf');
  invokeHandler = async (command) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return true;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/B.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(16);
  });
  const batch = calls.find((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths);
  // The current pending target B is now excluded from this editor-side batch; unrelated C still runs.
  assert.deepEqual(batch.args.paths, ['/C.raf']);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 0);
  assert.equal(useEditorStore.getState().adjustments, before.adjustments);
  assert.equal(useEditorStore.getState().history, before.history);
  assert.equal(useEditorStore.getState().pendingOpenIntent.path, '/B.raf');

  const ticketB2 = '12ec6383-b157-4e8d-aaef-667014a5de85';
  const keyB2 = { ticket: ticketB2, generation: '47' };
  useEditorStore.getState().prepareProgressiveIntent(ticketB2, '/B.raf');
  useEditorStore.getState().activateProgressiveIntent(ticketB2, '/B.raf', 'thumb:B');
  useEditorStore.setState((state) => ({
    progressiveOpen: { ...state.progressiveOpen, phase: 'Refining', openKey: keyB2, placeholderUsed: true },
    selectedImage: { ...state.selectedImage, openKey: keyB2, isReady: false },
  }));
  useLibraryStore.setState({ multiSelectedPaths: ['/B.raf', '/C.raf'] });
  calls.length = 0;
  const optionsAfterActivation = await showThumbnailContextMenu(menus, '/B.raf');
  const autoAfterActivation = findContextMenuOption(optionsAfterActivation, 'contextMenus.thumbnail.autoAdjust');
  await act(async () => {
    autoAfterActivation.onClick();
    await flushMicrotasks(16);
  });
  const activeBatch = calls.find((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths);
  assert.deepEqual(activeBatch.args.paths, ['/C.raf']);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 0);
  assert.equal(useEditorStore.getState().selectedImage.path, '/B.raf');
  assert.equal(useEditorStore.getState().adjustments.exposure, INITIAL_ADJUSTMENTS.exposure);
  await trackedUnmount(root);
});

test('navigation before thumbnail batch metadata admission prevents stale metadata lookup', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'library' });
  useLibraryStore.setState({ multiSelectedPaths: ['/A.raf', '/other.raf'], libraryActivePath: null });
  const batchGate = deferred();
  invokeHandler = (command) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return batchGate.promise;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/A.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(8);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths).length, 1);

  const ticketB = '649ab773-58f3-43af-8ce8-a1ff92ed9983';
  const keyB = { ticket: ticketB, generation: '48' };
  setAccurateImage(ticketB, keyB, '/B.raf');
  await act(async () => {
    batchGate.resolve(true);
    await flushMicrotasks(16);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 0);
  assert.equal(useEditorStore.getState().selectedImage.path, '/B.raf');
  await trackedUnmount(root);
});

test('actual loader setup-cleanup-setup reacquires the same flight and completes the accurate load', async () => {
  resetStores();
  const ticket = '41093cf2-8ea5-42bb-a135-c0d41e4689bc';
  const openKey = { ticket, generation: '71' };
  preparePendingLoader(ticket, '/strict-effects.raf');
  const loadGate = deferred();
  invokeHandler = (command) => {
    if (command === Invokes.BeginImageOpen)
      return Promise.resolve({ openKey, metadata: { adjustments: INITIAL_ADJUSTMENTS } });
    if (command === Invokes.LoadImage) return loadGate.promise;
    if (command === Invokes.CancelImageOpen) return Promise.resolve('Cancelled');
    throw new Error(`Unexpected invocation ${command}`);
  };

  const { Probe } = useTestHooks({ withLoader: true });
  let firstRoot;
  let secondRoot;
  await act(async () => {
    firstRoot = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(16);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.BeginImageOpen).length, 1);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);

  act(() => {
    mountedRoots.delete(firstRoot);
    firstRoot.unmount();
    secondRoot = trackedCreate(React.createElement(Probe));
  });
  await act(async () => {
    await flushMicrotasks(16);
  });

  assert.equal(calls.filter((call) => call.command === Invokes.BeginImageOpen).length, 1);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);
  assert.equal(calls.filter((call) => call.command === Invokes.CancelImageOpen).length, 0);
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Pending');
  loadGate.resolve(successfulLoadResult(openKey));
  await act(async () => {
    await flushMicrotasks(20);
  });
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Accurate');
  assert.equal(useEditorStore.getState().selectedImage.isReady, true);
  await trackedUnmount(secondRoot);
});

test('one loader subscriber may leave while the shared flight continues; the last release cancels once', async () => {
  resetStores();
  const ticket = 'a870a5e6-8db3-44af-b97e-e0c4059e6b35';
  const openKey = { ticket, generation: '72' };
  preparePendingLoader(ticket, '/shared-flight.raf');
  const loadGate = deferred();
  invokeHandler = (command) => {
    if (command === Invokes.BeginImageOpen)
      return Promise.resolve({ openKey, metadata: { adjustments: INITIAL_ADJUSTMENTS } });
    if (command === Invokes.LoadImage) return loadGate.promise;
    if (command === Invokes.CancelImageOpen) return Promise.resolve('Cancelled');
    throw new Error(`Unexpected invocation ${command}`);
  };

  const first = useTestHooks({ withLoader: true });
  const second = useTestHooks({ withLoader: true });
  let firstRoot;
  let secondRoot;
  await act(async () => {
    firstRoot = trackedCreate(React.createElement(first.Probe));
    await flushMicrotasks(16);
    secondRoot = trackedCreate(React.createElement(second.Probe));
    await flushMicrotasks(8);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.BeginImageOpen).length, 1);
  assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);

  await trackedUnmount(firstRoot);
  await act(async () => {
    await flushMicrotasks(8);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.CancelImageOpen).length, 0);
  assert.notEqual(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');

  await trackedUnmount(secondRoot);
  await act(async () => {
    await flushMicrotasks(12);
  });
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
  assert.equal(
    calls.filter((call) => call.command === Invokes.CancelImageOpen && call.args.openKey.ticket === ticket).length,
    1,
  );
  loadGate.resolve(successfulLoadResult(openKey));
  await act(async () => {
    await flushMicrotasks(16);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.CancelImageOpen).length, 1);
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
});

test('loader revokes an unresolved placeholder on unmount, accurate completion, and load error', async () => {
  resetStores();
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  const originalImage = globalThis.Image;
  const revoked = [];
  const images = [];
  let urlNumber = 0;
  let decodeGate;
  URL.createObjectURL = () => `blob:owned-progressive-${++urlNumber}`;
  URL.revokeObjectURL = (url) => revoked.push(url);
  globalThis.Image = class {
    constructor() {
      this.naturalWidth = 320;
      this.naturalHeight = 200;
      this.source = null;
      this.onload = null;
      this.onerror = null;
      this.onabort = null;
      this.removed = [];
      images.push(this);
    }
    set src(value) {
      this.source = value;
    }
    removeAttribute(name) {
      this.removed.push(name);
      if (name === 'src') this.source = null;
    }
    decode() {
      return decodeGate.promise;
    }
  };

  const runCase = async (terminal, suffix, generation) => {
    resetStores();
    const ticket = `91c9f6dd-0000-4000-8000-${suffix}`;
    const openKey = { ticket, generation: String(generation) };
    preparePendingLoader(ticket, `/decode-${terminal}.raf`);
    const loadGate = deferred();
    decodeGate = deferred();
    const cancelCallsBefore = calls.length;
    const revokedBefore = revoked.length;
    const imageCountBefore = images.length;
    const urlBefore = urlNumber;
    invokeHandler = (command) => {
      if (command === Invokes.BeginImageOpen)
        return Promise.resolve({ openKey, metadata: { adjustments: INITIAL_ADJUSTMENTS } });
      if (command === Invokes.LoadImage) return loadGate.promise;
      if (command === Invokes.CancelImageOpen) return Promise.resolve('Cancelled');
      throw new Error(`Unexpected invocation ${command}`);
    };
    const { Probe } = useTestHooks({ withLoader: true });
    let root;
    await act(async () => {
      root = trackedCreate(React.createElement(Probe));
      await flushMicrotasks(16);
    });
    const event = {
      payload: {
        schemaVersion: 1,
        openKey,
        width: 320,
        height: 200,
        mime: 'image/jpeg',
        dataBase64: '/9j/2Q==',
      },
    };
    const onPlaceholder = listeners.get('xtrans-placeholder-ready');
    assert.equal(typeof onPlaceholder, 'function');
    await act(async () => {
      onPlaceholder(event);
      onPlaceholder(event);
      await flushMicrotasks(8);
    });
    assert.equal(urlNumber, urlBefore + 1, 'the flight may own only one placeholder URL');
    const image = images[imageCountBefore];
    assert.ok(image);
    assert.equal(image.source, `blob:owned-progressive-${urlNumber}`);

    if (terminal === 'unmount') {
      await trackedUnmount(root);
      root = null;
      await act(async () => {
        await flushMicrotasks(12);
      });
      assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
    } else if (terminal === 'accurate') {
      loadGate.resolve(successfulLoadResult(openKey));
      await act(async () => {
        await flushMicrotasks(16);
      });
      assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Accurate');
    } else {
      loadGate.reject(new Error('expected fixture load error'));
      await act(async () => {
        await flushMicrotasks(16);
      });
      assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Failed');
    }

    assert.deepEqual(revoked.slice(revokedBefore), [`blob:owned-progressive-${urlNumber}`]);
    assert.deepEqual(image.removed, ['src']);
    assert.equal(image.source, null);
    assert.equal(image.onload, null);
    assert.equal(image.onerror, null);
    assert.equal(image.onabort, null);
    decodeGate.resolve();
    await act(async () => {
      await flushMicrotasks(16);
    });
    assert.equal(revoked.length - revokedBefore, 1, 'late decode completion cannot revoke twice or publish');
    assert.equal(useEditorStore.getState().progressiveOpen.placeholderUrl, null);
    if (terminal === 'unmount') {
      loadGate.resolve(successfulLoadResult(openKey));
      await act(async () => {
        await flushMicrotasks(16);
      });
      assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
      assert.equal(calls.slice(cancelCallsBefore).filter((call) => call.command === Invokes.CancelImageOpen).length, 1);
    } else {
      await trackedUnmount(root);
    }
  };

  try {
    await runCase('unmount', '000000000001', 73);
    await runCase('accurate', '000000000002', 74);
    await runCase('error', '000000000003', 75);
  } finally {
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    globalThis.Image = originalImage;
  }
});

test('a remounted loader cannot revive a genuinely cancelled owner', async () => {
  resetStores();
  const ticket = 'b3f96b6a-b15c-42a7-8bd1-433493a6643d';
  preparePendingLoader(ticket, '/cancelled-owner.raf');
  assert.equal(useEditorStore.getState().cancelProgressiveOpen(ticket), true);
  invokeHandler = async () => {
    throw new Error('a cancelled owner must not invoke native loading');
  };
  const { Probe } = useTestHooks({ withLoader: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(12);
  });
  assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
  assert.equal(calls.length, 0);
  assert.equal(listeners.has('xtrans-placeholder-ready'), false);
  await trackedUnmount(root);
});

test('actual auto-adjust action sends the captured key only for keyed editor requests', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  invokeHandler = async (command) => {
    if (command === Invokes.CalculateAutoAdjustments) return { exposure: 0.7 };
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks();
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  await act(async () => {
    await values.actions.handleAutoAdjustments();
  });
  const keyedCall = calls.find((call) => call.command === Invokes.CalculateAutoAdjustments);
  assert.deepEqual(keyedCall.args.expectedOpenKey, keyA);

  calls.length = 0;
  const current = useEditorStore.getState().selectedImage;
  useEditorStore.setState({
    progressiveOpen: null,
    pendingOpenIntent: null,
    selectedImage: {
      ...current,
      path: '/legacy.tif',
      openTicket: undefined,
      openKey: undefined,
      renderMode: undefined,
      isReady: true,
    },
  });
  await act(async () => {
    await values.actions.handleAutoAdjustments();
  });
  const legacyCall = calls.find((call) => call.command === Invokes.CalculateAutoAdjustments);
  assert.ok(legacyCall);
  assert.equal(Object.hasOwn(legacyCall.args, 'expectedOpenKey'), false);
  await trackedUnmount(root);
});

test('mounted ImageCanvas binds both white-balance request paths to the accurate OpenKey', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  invokeHandler = async (command) => {
    if (command === Invokes.SampleWhiteBalance) return { r: 0.9, g: 0.9, b: 0.9, temperature: 5600, tint: 1, count: 8 };
    throw new Error(`Unexpected invocation ${command}`);
  };
  const hooks = useTestHooks();
  let hooksRoot;
  await act(async () => {
    hooksRoot = trackedCreate(React.createElement(hooks.Probe));
  });
  const canvas = await mountImageCanvas({ path: '/A.raf', setAdjustments: hooks.values.actions.setAdjustments });
  const stageApi = { getPointerPosition: () => ({ x: 240, y: 190 }), setPointersPositions() {} };
  const pointer = { target: { getStage: () => stageApi }, evt: { button: 0, cancelable: false, preventDefault() {} } };

  await act(async () => {
    canvas.stage.props.onMouseMove(pointer);
    await flushMicrotasks(16);
  });
  await act(async () => {
    canvas.stage.props.onMouseDown(pointer);
    canvas.stage.props.onMouseUp();
    await flushMicrotasks(16);
  });

  const samples = calls.filter((call) => call.command === Invokes.SampleWhiteBalance);
  assert.equal(samples.length, 2);
  assert.deepEqual(
    samples.map((call) => call.args.expectedOpenKey),
    [keyA, keyA],
  );
  await trackedUnmount(canvas.root);
  await trackedUnmount(hooksRoot);
});

test('mounted ImageCanvas keeps legacy white-balance sample arguments unkeyed', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  const current = useEditorStore.getState().selectedImage;
  useEditorStore.setState({
    progressiveOpen: null,
    pendingOpenIntent: null,
    selectedImage: {
      ...current,
      path: '/legacy.tif',
      openTicket: undefined,
      openKey: undefined,
      renderMode: undefined,
      isReady: true,
    },
  });
  invokeHandler = async (command) => {
    if (command === Invokes.SampleWhiteBalance) return { r: 0.9, g: 0.9, b: 0.9, temperature: 5600, tint: 1, count: 8 };
    throw new Error(`Unexpected invocation ${command}`);
  };
  const hooks = useTestHooks();
  let hooksRoot;
  await act(async () => {
    hooksRoot = trackedCreate(React.createElement(hooks.Probe));
  });
  const canvas = await mountImageCanvas({ path: '/legacy.tif', setAdjustments: hooks.values.actions.setAdjustments });
  const stageApi = { getPointerPosition: () => ({ x: 240, y: 190 }), setPointersPositions() {} };
  const pointer = { target: { getStage: () => stageApi }, evt: { button: 0, cancelable: false, preventDefault() {} } };
  await act(async () => {
    canvas.stage.props.onMouseMove(pointer);
    await flushMicrotasks(16);
  });
  await act(async () => {
    canvas.stage.props.onMouseDown(pointer);
    canvas.stage.props.onMouseUp();
    await flushMicrotasks(16);
  });
  const samples = calls.filter((call) => call.command === Invokes.SampleWhiteBalance);
  assert.equal(samples.length, 2);
  assert.ok(samples.every((call) => !Object.hasOwn(call.args, 'expectedOpenKey')));
  await trackedUnmount(canvas.root);
  await trackedUnmount(hooksRoot);
});

test('deferred Android LUT content-name cannot mutate after A-to-B navigation', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf', { ...INITIAL_ADJUSTMENTS, exposure: 0.2 });
  useSettingsStore.setState({ osPlatform: 'android' });
  const nameGate = deferred();
  invokeHandler = (command) => {
    if (command === 'load_and_parse_lut') return Promise.resolve({ size: 123 });
    if (command === 'resolve_android_content_uri_name') return nameGate.promise;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks();
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  let selection;
  await act(async () => {
    selection = values.actions.handleLutSelect('content://lut/opaque-id');
    await flushMicrotasks(8);
  });
  assert.equal(calls.filter((call) => call.command === 'resolve_android_content_uri_name').length, 1);
  const bAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: -1.2 };
  setAccurateImage(
    '1a14a001-2d63-49ad-96a2-2ac20e6c89f8',
    { ticket: '1a14a001-2d63-49ad-96a2-2ac20e6c89f8', generation: '81' },
    '/B.raf',
    bAdjustments,
  );
  setEditorHistoryState(bAdjustments);
  const before = useEditorStore.getState();
  nameGate.resolve('late-name.cube');
  await act(async () => {
    await selection;
    await flushMicrotasks(8);
  });
  assert.equal(useEditorStore.getState().adjustments, before.adjustments);
  assert.equal(useEditorStore.getState().history, before.history);
  assert.equal(useEditorStore.getState().adjustments.lutPath, before.adjustments.lutPath);
  await trackedUnmount(root);
});

test('deferred Android LUT content-name cannot mutate after same-path A/B/A navigation', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf', { ...INITIAL_ADJUSTMENTS, exposure: 0.3 });
  useSettingsStore.setState({ osPlatform: 'android' });
  const nameGate = deferred();
  invokeHandler = (command) => {
    if (command === 'load_and_parse_lut') return Promise.resolve({ size: 321 });
    if (command === 'resolve_android_content_uri_name') return nameGate.promise;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks();
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  let selection;
  await act(async () => {
    selection = values.actions.handleLutSelect('content://lut/aba-id');
    await flushMicrotasks(8);
  });
  const middleTicket = '2a14a002-2d63-49ad-96a2-2ac20e6c89f8';
  setAccurateImage(middleTicket, { ticket: middleTicket, generation: '82' }, '/B.raf');
  const finalTicket = '3a14a003-2d63-49ad-96a2-2ac20e6c89f8';
  const finalKey = { ticket: finalTicket, generation: '83' };
  const finalAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: -0.8 };
  setAccurateImage(finalTicket, finalKey, '/A.raf', finalAdjustments);
  setEditorHistoryState(finalAdjustments);
  const before = useEditorStore.getState();
  nameGate.resolve('late-aba.cube');
  await act(async () => {
    await selection;
    await flushMicrotasks(8);
  });
  assert.equal(useEditorStore.getState().selectedImage.path, '/A.raf');
  assert.deepEqual(useEditorStore.getState().selectedImage.openKey, finalKey);
  assert.equal(useEditorStore.getState().adjustments, before.adjustments);
  assert.equal(useEditorStore.getState().history, before.history);
  await trackedUnmount(root);
});

test('pending intent excludes its path from reset paste and auto-lens batches while unrelated C still runs', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useEditorStore.getState().prepareProgressiveIntent('4a14a004-2d63-49ad-96a2-2ac20e6c89f8', '/B.raf');
  useLibraryStore.setState({ multiSelectedPaths: ['/B.raf', '/C.raf'], libraryActivePath: null });
  useEditorStore.setState({ copiedAdjustments: { exposure: 0.6 } });
  useSettingsStore.setState({
    appSettings: { copyPasteSettings: { mode: 'replace', includedAdjustments: ['exposure'] } },
  });
  invokeHandler = async (command) => {
    if (command === Invokes.ResetAdjustmentsForPaths || command === Invokes.ApplyAdjustmentsToPaths)
      return Promise.resolve(true);
    if (command === 'apply_auto_lens_correction_to_paths') return true;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks();
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });

  const cases = [
    {
      command: Invokes.ResetAdjustmentsForPaths,
      run: () => values.actions.handleResetAdjustments(['/B.raf', '/C.raf']),
    },
    {
      command: Invokes.ApplyAdjustmentsToPaths,
      run: () => values.actions.handlePasteAdjustments(['/B.raf', '/C.raf']),
    },
    {
      command: 'apply_auto_lens_correction_to_paths',
      run: () => values.actions.handleAutoLensCorrection(['/B.raf', '/C.raf']),
    },
  ];
  for (const item of cases) {
    calls.length = 0;
    await act(async () => {
      item.run();
      await flushMicrotasks(12);
    });
    const batch = calls.find((call) => call.command === item.command);
    assert.ok(batch, `${item.command} should preserve the unrelated target`);
    assert.deepEqual(batch.args.paths, ['/C.raf']);
    calls.length = 0;
    await act(async () => {
      if (item.command === Invokes.ResetAdjustmentsForPaths) values.actions.handleResetAdjustments(['/B.raf']);
      if (item.command === Invokes.ApplyAdjustmentsToPaths) values.actions.handlePasteAdjustments(['/B.raf']);
      if (item.command === 'apply_auto_lens_correction_to_paths') values.actions.handleAutoLensCorrection(['/B.raf']);
      await flushMicrotasks(12);
    });
    assert.equal(calls.filter((call) => call.command === item.command).length, 0, 'B-only pending batches are inert');
  }
  await trackedUnmount(root);
});

test('refining selected B is excluded from batches while accurate B remains eligible', async () => {
  resetStores();
  const ticketB = '5a14a005-2d63-49ad-96a2-2ac20e6c89f8';
  const keyB = { ticket: ticketB, generation: '84' };
  preparePendingLoader(ticketB, '/B.raf');
  const store = useEditorStore.getState();
  assert.equal(store.acceptProgressiveAdmission(ticketB, '/B.raf', keyB, {}, INITIAL_ADJUSTMENTS), true);
  assert.equal(store.claimProgressivePlaceholder(ticketB, '/B.raf', keyB), true);
  assert.equal(store.setProgressivePlaceholder(ticketB, '/B.raf', keyB, 'blob:refining-test'), true);
  useLibraryStore.setState({ multiSelectedPaths: ['/B.raf', '/C.raf'], libraryActivePath: null });
  useEditorStore.setState({ copiedAdjustments: { exposure: 0.4 } });
  useSettingsStore.setState({
    appSettings: { copyPasteSettings: { mode: 'replace', includedAdjustments: ['exposure'] } },
  });
  invokeHandler = async (command) => {
    if (
      command === Invokes.ResetAdjustmentsForPaths ||
      command === Invokes.ApplyAdjustmentsToPaths ||
      command === 'apply_auto_lens_correction_to_paths'
    )
      return true;
    if (command === Invokes.LoadMetadata || command === 'load_metadata') return { adjustments: null, is_null: true };
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks();
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  const cases = [
    { command: Invokes.ResetAdjustmentsForPaths, run: (paths) => values.actions.handleResetAdjustments(paths) },
    { command: Invokes.ApplyAdjustmentsToPaths, run: (paths) => values.actions.handlePasteAdjustments(paths) },
    { command: 'apply_auto_lens_correction_to_paths', run: (paths) => values.actions.handleAutoLensCorrection(paths) },
  ];
  for (const item of cases) {
    calls.length = 0;
    await act(async () => {
      item.run(['/B.raf']);
      await flushMicrotasks(8);
    });
    assert.equal(calls.filter((call) => call.command === item.command).length, 0);
    calls.length = 0;
    await act(async () => {
      item.run(['/B.raf', '/C.raf']);
      await flushMicrotasks(12);
    });
    const refiningBatch = calls.find((call) => call.command === item.command);
    assert.deepEqual(refiningBatch.args.paths, ['/C.raf']);
  }

  const accurateAdjustments = { ...INITIAL_ADJUSTMENTS, exposure: -0.3 };
  setAccurateImage(ticketB, keyB, '/B.raf', accurateAdjustments);
  for (const item of cases) {
    calls.length = 0;
    await act(async () => {
      item.run(['/B.raf', '/C.raf']);
      await flushMicrotasks(12);
    });
    const accurateBatch = calls.find((call) => call.command === item.command);
    assert.deepEqual(accurateBatch.args.paths, ['/B.raf', '/C.raf']);
  }
  await trackedUnmount(root);
});

test('thumbnail callback reads the current pending target and leaves B metadata and history untouched', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/A.raf');
  useUIStore.setState({ activeView: 'editor' });
  useLibraryStore.setState({
    multiSelectedPaths: ['/A.raf', '/B.raf', '/C.raf'],
    imageList: [{ path: '/A.raf' }, { path: '/B.raf' }, { path: '/C.raf' }],
    libraryActivePath: '/B.raf',
  });
  const batchGate = deferred();
  invokeHandler = (command) => {
    if (command === Invokes.ApplyAutoAdjustmentsToPaths) return batchGate.promise;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { root, menus } = await mountContextMenus();
  const options = await showThumbnailContextMenu(menus, '/B.raf');
  const auto = findContextMenuOption(options, 'contextMenus.thumbnail.autoAdjust');
  assert.ok(auto);

  const pendingTicket = '6a14a006-2d63-49ad-96a2-2ac20e6c89f8';
  useEditorStore.getState().prepareProgressiveIntent(pendingTicket, '/B.raf');
  await act(async () => {
    auto.onClick();
    await flushMicrotasks(8);
  });
  const batch = calls.find((call) => call.command === Invokes.ApplyAutoAdjustmentsToPaths);
  assert.deepEqual(batch.args.paths, ['/C.raf']);

  useEditorStore.getState().activateProgressiveIntent(pendingTicket, '/B.raf', 'thumb:B');
  const keyB = { ticket: pendingTicket, generation: '85' };
  useEditorStore.setState((state) => ({
    progressiveOpen: { ...state.progressiveOpen, openKey: keyB, phase: 'Refining', placeholderUsed: true },
    selectedImage: { ...state.selectedImage, openKey: keyB, isReady: false },
    adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 2.7 },
    history: [INITIAL_ADJUSTMENTS, { ...INITIAL_ADJUSTMENTS, exposure: 2.7 }],
    historyIndex: 1,
  }));
  const beforeB = useEditorStore.getState();
  await act(async () => {
    batchGate.resolve(true);
    await flushMicrotasks(16);
  });
  assert.equal(calls.filter((call) => call.command === Invokes.LoadMetadata).length, 0);
  assert.equal(useEditorStore.getState().adjustments, beforeB.adjustments);
  assert.equal(useEditorStore.getState().history, beforeB.history);
  assert.equal(useEditorStore.getState().historyIndex, beforeB.historyIndex);
  assert.equal(useEditorStore.getState().selectedImage.path, '/B.raf');
  await trackedUnmount(root);
});

test('mounted keyed Legacy completion retires the reservation and resumes unkeyed native consumers', async () => {
  resetStores();
  const ticket = '7a14a007-2d63-49ad-96a2-2ac20e6c89f8';
  const path = '/legacy-handoff.raf';
  const key = { ticket, generation: '91' };
  preparePendingLoader(ticket, path);
  invokeHandler = async (command, args) => {
    if (command === Invokes.BeginImageOpen) return { openKey: key, metadata: { adjustments: INITIAL_ADJUSTMENTS } };
    if (command === Invokes.LoadImage) return successfulLoadResult(args.openKey, 'Legacy');
    if (command === Invokes.CancelImageOpen) return null;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe, values } = useTestHooks({ withLoader: true, withListeners: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(32);
  });

  const state = useEditorStore.getState();
  assert.equal(state.progressiveOpen, null);
  assert.equal(state.selectedImage.path, path);
  assert.equal(state.selectedImage.renderMode, 'Legacy');
  assert.equal(state.selectedImage.isReady, true);
  assert.equal(Object.hasOwn(state.selectedImage, 'openKey'), false);
  assert.equal(Object.hasOwn(state.selectedImage, 'openTicket'), false);

  await act(async () => {
    state.setEditor({ finalPreviewUrl: 'data:image/jpeg;base64,legacy' });
    await flushMicrotasks();
  });
  assert.equal(values.cachedRef.current.selectedImage.renderMode, 'Legacy');
  assert.equal(values.cachedRef.current.selectedImage.openTicket, undefined);

  await act(async () => {
    listeners.get('wgpu-frame-ready')({ payload: { path } });
    listeners.get('preview-update-uncropped')({ payload: 'data:image/jpeg;base64,uncropped' });
  });
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, true);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, 'data:image/jpeg;base64,uncropped');
  assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);
  await trackedUnmount(root);
});

test('mounted loader rejects a missing or unknown final renderMode without retiring its keyed owner', async () => {
  for (const [index, invalidMode] of [
    [0, undefined],
    [1, 'FutureMode'],
  ]) {
    resetStores();
    const ticket = `8a14a008-2d63-49ad-96a2-2ac20e6c89f${index}`;
    const path = `/invalid-mode-${index}.raf`;
    const key = { ticket, generation: String(92 + index) };
    preparePendingLoader(ticket, path);
    invokeHandler = async (command, args) => {
      if (command === Invokes.BeginImageOpen) return { openKey: key, metadata: { adjustments: INITIAL_ADJUSTMENTS } };
      if (command === Invokes.LoadImage) {
        const result = successfulLoadResult(args.openKey);
        if (invalidMode === undefined) delete result.renderMode;
        else result.renderMode = invalidMode;
        return result;
      }
      if (command === Invokes.CancelImageOpen) return null;
      throw new Error(`Unexpected invocation ${command}`);
    };
    const { Probe } = useTestHooks({ withLoader: true });
    let root;
    await act(async () => {
      root = trackedCreate(React.createElement(Probe));
      await flushMicrotasks(32);
    });
    const state = useEditorStore.getState();
    assert.equal(state.progressiveOpen.phase, 'Failed');
    assert.equal(state.selectedImage.isReady, false);
    assert.deepEqual(state.selectedImage.openKey, key);
    assert.equal(state.selectedImage.openTicket, ticket);
    assert.equal(state.selectedImage.renderMode, undefined);
    assert.equal(calls.filter((call) => call.command === Invokes.LoadImage).length, 1);
    await trackedUnmount(root);
  }
});

test('a stale keyed Legacy response cannot retire the later same-path A/B/A owner', async () => {
  resetStores();
  const ticketA1 = '9a14a009-2d63-49ad-96a2-2ac20e6c89f0';
  const keyA1 = { ticket: ticketA1, generation: '94' };
  preparePendingLoader(ticketA1, '/same.raf');
  const loadGate = deferred();
  invokeHandler = async (command, args) => {
    if (command === Invokes.BeginImageOpen) return { openKey: keyA1, metadata: { adjustments: INITIAL_ADJUSTMENTS } };
    if (command === Invokes.LoadImage) return loadGate.promise;
    if (command === Invokes.CancelImageOpen) return null;
    throw new Error(`Unexpected invocation ${command}`);
  };
  const { Probe } = useTestHooks({ withLoader: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(32);
  });
  assert.equal(
    calls.some((call) => call.command === Invokes.LoadImage),
    true,
  );

  const ticketB = 'aa14a00a-2d63-49ad-96a2-2ac20e6c89f1';
  const keyB = { ticket: ticketB, generation: '95' };
  const ticketA2 = 'ba14a00b-2d63-49ad-96a2-2ac20e6c89f2';
  const keyA2 = { ticket: ticketA2, generation: '96' };
  await act(async () => {
    setAccurateImage(ticketB, keyB, '/other.raf');
    setAccurateImage(ticketA2, keyA2, '/same.raf');
  });
  loadGate.resolve(successfulLoadResult(keyA1, 'Legacy'));
  await act(async () => {
    await flushMicrotasks(24);
  });
  const current = useEditorStore.getState();
  assert.equal(current.selectedImage.path, '/same.raf');
  assert.deepEqual(current.selectedImage.openKey, keyA2);
  assert.equal(current.selectedImage.openTicket, ticketA2);
  assert.equal(current.selectedImage.renderMode, 'AccurateXtrans1Encoded');
  assert.equal(current.progressiveOpen.phase, 'Accurate');
  await trackedUnmount(root);
});

test('a keyed encoded owner ignores native frames and keeps canvas base fade and patch visible', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/encoded.raf');
  useSettingsStore.setState({ appSettings: { useWgpuRenderer: true } });
  const { Probe } = useTestHooks({ withListeners: true });
  let listenerRoot;
  await act(async () => {
    listenerRoot = trackedCreate(React.createElement(Probe));
    await flushMicrotasks(8);
  });
  await act(async () => {
    listeners.get('wgpu-frame-ready')({ payload: { path: '/encoded.raf', openKey: keyA } });
  });
  assert.equal(useEditorStore.getState().hasRenderedFirstFrame, false);
  await trackedUnmount(listenerRoot);

  useEditorStore.setState({ hasRenderedFirstFrame: true });
  const canvas = await mountImageCanvas({
    path: '/encoded.raf',
    setAdjustments() {},
    appSettings: { useWgpuRenderer: true },
    thumbnailUrl: 'data:image/jpeg;base64,encoded-old',
    hasRenderedFirstFrame: true,
  });
  const updatedProps = {
    ...canvas.props,
    finalPreviewUrl: 'data:image/jpeg;base64,encoded-new',
    interactivePatch: { url: 'blob:encoded-patch', normX: 0.1, normY: 0.2, normW: 0.3, normH: 0.4 },
  };
  await act(async () => {
    canvas.root.update(React.createElement(ImageCanvas, updatedProps));
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
  const visibleSources = canvas.root.root.findAll((node) => node.type === 'image').map((node) => node.props.href);
  assert.ok(visibleSources.includes('data:image/jpeg;base64,encoded-old'), 'the encoded base remains visible');
  assert.ok(visibleSources.includes('data:image/jpeg;base64,encoded-new'), 'the encoded fade completes visibly');
  assert.ok(visibleSources.includes('blob:encoded-patch'), 'the encoded interactive patch remains visible');
  await trackedUnmount(canvas.root);
});

test('mounted Editor transform callbacks recheck live render policy across both native transform branches', async () => {
  resetStores();
  useSettingsStore.setState({ appSettings: { useWgpuRenderer: true, editorPreviewResolution: 1024 } });
  useUIStore.setState({ activeView: 'editor', activePanel: 'crop' });
  useLibraryStore.setState({ isViewLoading: false });
  const legacyTicket = 'ca14a00c-2d63-49ad-96a2-2ac20e6c89f3';
  const legacyKey = { ticket: legacyTicket, generation: '98' };
  setAccurateImage(legacyTicket, legacyKey, '/legacy-visible.raf', INITIAL_ADJUSTMENTS, 'Legacy');
  useEditorStore.getState().setEditor({ hasRenderedFirstFrame: true });

  const firstTransform = deferred();
  let transformCount = 0;
  invokeHandler = (command) => {
    if (command === Invokes.UpdateWgpuTransform) {
      transformCount += 1;
      return transformCount === 1 ? firstTransform.promise : Promise.resolve(null);
    }
    return Promise.resolve(null);
  };
  const node = () => ({
    getBoundingClientRect: () => ({ left: 20, top: 30, width: 480, height: 320, right: 500, bottom: 350 }),
    clientWidth: 480,
    clientHeight: 320,
    addEventListener() {},
    removeEventListener() {},
    setPointerCapture() {},
    releasePointerCapture() {},
    focus() {},
    blur() {},
    style: {},
  });
  const props = {
    onBackToLibrary() {},
    onContextMenu() {},
    transformWrapperRef: { current: { resetTransform() {}, setTransform() {} } },
  };
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Editor, props), { createNodeMock: node });
    await flushMicrotasks(16);
  });
  const transforms = () => calls.filter((call) => call.command === Invokes.UpdateWgpuTransform);
  assert.equal(transforms().length, 1, 'the valid Legacy owner uses the visible transform branch');
  assert.notEqual(transforms()[0].args.payload.x, -999999);

  const encodedTicket = 'da14a00d-2d63-49ad-96a2-2ac20e6c89f4';
  await act(async () => {
    setAccurateImage(encodedTicket, { ticket: encodedTicket, generation: '99' }, '/encoded.raf');
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
  firstTransform.resolve(null);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
  assert.equal(transforms().length, 1, 'the delayed visible callback cannot issue a hidden transform for encoded mode');

  const legacyAgainTicket = 'ea14a00e-2d63-49ad-96a2-2ac20e6c89f5';
  await act(async () => {
    setAccurateImage(
      legacyAgainTicket,
      { ticket: legacyAgainTicket, generation: '100' },
      '/legacy-hidden.raf',
      INITIAL_ADJUSTMENTS,
      'Legacy',
    );
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
  assert.equal(transforms().length, 2, 'the valid Legacy owner retains the hidden transform branch');
  assert.equal(transforms()[1].args.payload.x, -999999);

  const pendingTicket = 'fa14a00f-2d63-49ad-96a2-2ac20e6c89f6';
  await act(async () => {
    useEditorStore.getState().prepareProgressiveIntent(pendingTicket, '/pending.raf');
    useEditorStore.getState().activateProgressiveIntent(pendingTicket, '/pending.raf', 'thumb:pending');
    useEditorStore.setState({ hasRenderedFirstFrame: true });
    useSettingsStore.setState({
      appSettings: { useWgpuRenderer: true, editorPreviewResolution: 1024, editorNeutralGreyBg: true },
    });
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
  assert.equal(
    transforms().length,
    2,
    'Pending suppresses both visible and hidden native transforms despite stale flags',
  );
  await trackedUnmount(root);
});

test('mounted export estimate forwards the current key and drops stale replies while explicit files stay independent', async () => {
  resetStores();
  const stableAdjustments = { ...INITIAL_ADJUSTMENTS, aspectRatio: 4 / 3, exposure: 0 };
  setAccurateImage(ticketA, keyA, '/A.raf', stableAdjustments);
  useUIStore.setState({
    activePanels: { leftTop: 'export', leftBottom: null, rightTop: 'adjustments', rightBottom: null },
  });
  let estimateCount = 0;
  const staleGate = deferred();
  invokeHandler = (command) => {
    if (command !== Invokes.EstimateExportSizes) throw new Error(`Unexpected invocation ${command}`);
    estimateCount += 1;
    return estimateCount === 1 ? Promise.resolve(1111) : staleGate.promise;
  };
  const translationCalls = [];
  globalThis.__progressiveTranslations = translationCalls;
  const editorPanelProps = {
    exportState: { status: 'success', progress: { current: 0, total: 0 }, errorMessage: '' },
    multiSelectedPaths: [],
    selectedImage: useEditorStore.getState().selectedImage,
    setExportState() {},
    appSettings: null,
    onSettingsChange() {},
    rootPaths: [],
    isVisible: true,
  };
  let root;
  try {
    await act(async () => {
      root = trackedCreate(React.createElement(ExportPanel, editorPanelProps));
      await flushMicrotasks(8);
    });
    await act(async () => new Promise((resolve) => setTimeout(resolve, 540)));
    const firstEstimate = calls.find((call) => call.command === Invokes.EstimateExportSizes);
    assert.ok(firstEstimate);
    assert.deepEqual(firstEstimate.args.expectedOpenKey, keyA);
    assert.equal(firstEstimate.args.currentEditPath, '/A.raf');
    assert.ok(translationCalls.some((item) => item.key === 'export.status.estimatedSize'));

    translationCalls.length = 0;
    await act(async () => {
      useEditorStore.getState().setEditor({ adjustments: { ...stableAdjustments, exposure: 0.25 } });
    });
    await act(async () => new Promise((resolve) => setTimeout(resolve, 540)));
    const estimates = calls.filter((call) => call.command === Invokes.EstimateExportSizes);
    assert.equal(estimates.length, 2);
    assert.deepEqual(estimates[1].args.expectedOpenKey, keyA);

    const ticketA2 = '0a14a010-2d63-49ad-96a2-2ac20e6c89f7';
    const keyA2 = { ticket: ticketA2, generation: '101' };
    await act(async () => {
      useEditorStore.setState((state) => ({
        progressiveOpen: { ...state.progressiveOpen, ticket: ticketA2, openKey: keyA2 },
        selectedImage: { ...state.selectedImage, openTicket: ticketA2, openKey: keyA2 },
      }));
      await flushMicrotasks();
    });
    const estimatedSizeTranslationsBeforeStaleReply = translationCalls.filter(
      (item) => item.key === 'export.status.estimatedSize',
    ).length;
    staleGate.resolve(2222);
    await act(async () => {
      await flushMicrotasks(20);
    });
    assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyA2);
    assert.equal(
      translationCalls.filter((item) => item.key === 'export.status.estimatedSize').length,
      estimatedSizeTranslationsBeforeStaleReply,
    );
    await trackedUnmount(root);
  } finally {
    if (root) await trackedUnmount(root);
    delete globalThis.__progressiveTranslations;
  }

  resetStores();
  calls.length = 0;
  const pendingTicket = '1a14a011-2d63-49ad-96a2-2ac20e6c89f8';
  preparePendingLoader(pendingTicket, '/pending.raf');
  useLibraryStore.setState({ multiSelectedPaths: [] });
  useUIStore.setState({
    activePanels: { leftTop: 'export', leftBottom: null, rightTop: 'adjustments', rightBottom: null },
  });
  invokeHandler = async (command) => {
    if (command === Invokes.EstimateExportSizes) return 256;
    throw new Error(`Unexpected invocation ${command}`);
  };
  let pendingCurrentEditRoot;
  await act(async () => {
    pendingCurrentEditRoot = trackedCreate(
      React.createElement(ExportPanel, {
        ...editorPanelProps,
        multiSelectedPaths: [],
        selectedImage: useEditorStore.getState().selectedImage,
      }),
    );
    await flushMicrotasks(8);
  });
  await act(async () => new Promise((resolve) => setTimeout(resolve, 540)));
  assert.equal(calls.filter((call) => call.command === Invokes.EstimateExportSizes).length, 0);
  await trackedUnmount(pendingCurrentEditRoot);

  useLibraryStore.setState({ multiSelectedPaths: ['/C.raf'] });
  useUIStore.setState({
    activePanels: { leftTop: 'export', leftBottom: null, rightTop: 'adjustments', rightBottom: null },
  });
  invokeHandler = async (command) => {
    if (command === Invokes.EstimateExportSizes) return 512;
    throw new Error(`Unexpected invocation ${command}`);
  };
  let explicitRoot;
  await act(async () => {
    explicitRoot = trackedCreate(
      React.createElement(ExportPanel, {
        ...editorPanelProps,
        multiSelectedPaths: ['/C.raf'],
        selectedImage: useEditorStore.getState().selectedImage,
        onClose() {},
      }),
    );
    await flushMicrotasks(8);
  });
  await act(async () => new Promise((resolve) => setTimeout(resolve, 540)));
  const explicitEstimate = calls.find((call) => call.command === Invokes.EstimateExportSizes);
  assert.ok(explicitEstimate);
  assert.deepEqual(explicitEstimate.args.paths, ['/C.raf']);
  assert.equal(explicitEstimate.args.currentEditPath, null);
  assert.equal(Object.hasOwn(explicitEstimate.args, 'expectedOpenKey'), false);
  await trackedUnmount(explicitRoot);
});

function inlineConsumerAdjustments() {
  const subMask = (id) => ({
    id,
    type: 'brush',
    visible: true,
    parameters: { mask_data_base64: `snake:${id}`, maskDataBase64: `camel:${id}` },
  });
  return {
    ...INITIAL_ADJUSTMENTS,
    aspectRatio: 4 / 3,
    aiPatches: [
      {
        id: 'inline-patch',
        visible: true,
        isLoading: false,
        patchData: 'inline-patch-rgb',
        subMasks: [subMask('inline-patch-mask')],
      },
    ],
    masks: [{ id: 'inline-container', visible: true, subMasks: [subMask('inline-container-mask')] }],
  };
}

async function mountInlineConsumerEditor(onCommit = () => {}) {
  useUIStore.setState({ activeView: 'editor', activePanel: 'adjustments' });
  useEditorStore.setState({ activeMaskContainerId: null, activeAiPatchContainerId: null });
  const node = () => ({
    getBoundingClientRect: () => ({ left: 20, top: 30, width: 480, height: 320, right: 500, bottom: 350 }),
    clientWidth: 480,
    clientHeight: 320,
    addEventListener() {},
    removeEventListener() {},
    setPointerCapture() {},
    releasePointerCapture() {},
    focus() {},
    blur() {},
    style: {},
  });
  let root;
  await act(async () => {
    root = trackedCreate(
      React.createElement(
        React.Profiler,
        { id: 'inline-consumer', onRender: () => root && onCommit(root) },
        React.createElement(Editor, {
          onBackToLibrary() {},
          onContextMenu() {},
          transformWrapperRef: { current: { resetTransform() {}, setTransform() {} } },
        }),
      ),
      { createNodeMock: node },
    );
    await flushMicrotasks(16);
  });
  return root;
}

test('mounted adjustment requests retain repeated keyed inline bytes while legacy sends then strips the same IDs', async () => {
  const originalImage = globalThis.Image;
  const originalCreate = URL.createObjectURL;
  URL.createObjectURL = () => 'data:image/jpeg;base64,inline-fixture';
  globalThis.Image = class {
    decode() {
      return Promise.resolve();
    }
  };
  try {
    for (const keyed of [true, false]) {
      resetStores();
      const adjustments = inlineConsumerAdjustments();
      const original = structuredClone(adjustments);
      setAccurateImage(ticketA, keyA, '/inline.raf', adjustments);
      if (!keyed) {
        useEditorStore.setState((state) => ({
          selectedImage: { ...state.selectedImage, openTicket: undefined, openKey: undefined, renderMode: 'Legacy' },
          progressiveOpen: null,
        }));
      }
      const ids = ['inline-patch', 'inline-patch-mask', 'inline-container-mask'];
      const sent = new Set(keyed ? ids : []);
      useEditorStore.setState({ patchesSentToBackend: sent, isSliderDragging: true });
      useSettingsStore.setState({ appSettings: { enableLivePreviews: false } });
      invokeHandler = async (command) => {
        assert.equal(command, Invokes.ApplyAdjustments);
        return new Uint8Array([255, 216, 255, 217]).buffer;
      };
      const { Probe, values } = useTestHooks({ withProcessing: true });
      let root;
      await act(async () => {
        root = trackedCreate(React.createElement(Probe));
      });
      for (let repeat = 0; repeat < 2; repeat++) {
        await act(async () => {
          await values.process.executeApplyAdjustments(adjustments);
        });
        const request = calls.filter((call) => call.command === Invokes.ApplyAdjustments).at(-1);
        const expected = structuredClone(original);
        if (!keyed && repeat === 1) {
          expected.aiPatches[0].patchData = null;
          for (const subMask of [expected.aiPatches[0].subMasks[0], expected.masks[0].subMasks[0]]) {
            subMask.parameters.mask_data_base64 = null;
            subMask.parameters.maskDataBase64 = null;
          }
        }
        assert.deepEqual(request.args.jsAdjustments, expected);
        assert.equal(Object.hasOwn(request.args, 'expectedOpenKey'), keyed);
        if (keyed) assert.deepEqual(request.args.expectedOpenKey, keyA);
        assert.deepEqual([...sent].sort(), [...ids].sort(), 'legacy first success still records all newly sent IDs');
        assert.deepEqual(adjustments, original, 'IPC shaping cannot mutate store-owned inline data');
        assert.equal(useEditorStore.getState().adjustments, adjustments);
      }
      assert.equal(calls.filter((call) => call.command === Invokes.ApplyAdjustments).length, 2);
      await trackedUnmount(root);
    }
  } finally {
    URL.createObjectURL = originalCreate;
    globalThis.Image = originalImage;
  }
});

test('mounted inline adjustment requests keep source and browser decode fences through same-path replacement', async () => {
  resetStores();
  const adjustments = inlineConsumerAdjustments();
  const original = structuredClone(adjustments);
  setAccurateImage(ticketA, keyA, '/same-inline.raf', adjustments);
  useEditorStore.setState({ patchesSentToBackend: new Set(['inline-patch', 'inline-patch-mask']), isSliderDragging: true });
  useSettingsStore.setState({ appSettings: { enableLivePreviews: false } });
  const ipc = deferred();
  const decoding = deferred();
  const originalImage = globalThis.Image;
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  const revoked = [];
  let decodeCount = 0;
  globalThis.Image = class {
    decode() {
      decodeCount++;
      return decoding.promise;
    }
  };
  URL.createObjectURL = () => 'blob:inline-stale';
  URL.revokeObjectURL = (url) => revoked.push(url);
  const { Probe, values } = useTestHooks({ withProcessing: true });
  let root;
  let request;
  try {
    invokeHandler = (command) => {
      assert.equal(command, Invokes.ApplyAdjustments);
      return ipc.promise;
    };
    await act(async () => {
      root = trackedCreate(React.createElement(Probe));
    });
    await act(async () => {
      request = values.process.executeApplyAdjustments(adjustments);
      await flushMicrotasks();
      ipc.resolve(new Uint8Array([255, 216, 255, 217]).buffer);
      await flushMicrotasks();
    });
    assert.equal(decodeCount, 1);
    assert.deepEqual(calls.find((call) => call.command === Invokes.ApplyAdjustments).args.jsAdjustments, original);
    const ticketB = 'ba001d9f-18e1-44a5-812a-5cde8845e2bb';
    const ticketA2 = '267e12fe-2a0a-476e-8c52-c64a9dfe5178';
    const keyA2 = { ticket: ticketA2, generation: '145' };
    await act(async () => {
      setAccurateImage(ticketB, { ticket: ticketB, generation: '144' }, '/other-inline.raf', adjustments);
      setAccurateImage(ticketA2, keyA2, '/same-inline.raf', adjustments);
      decoding.resolve();
      await request;
    });
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);
    assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyA2);
    assert.deepEqual(revoked, ['blob:inline-stale']);
    assert.deepEqual(adjustments, original);
  } finally {
    if (root) await trackedUnmount(root);
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    globalThis.Image = originalImage;
  }
});

test('mounted Editor preserves standalone and adjustment mask bytes on every keyed request but strips known legacy masks', async () => {
  for (const keyed of [true, false]) {
    resetStores();
    const adjustments = inlineConsumerAdjustments();
    const original = structuredClone(adjustments);
    setAccurateImage(ticketA, keyA, '/inline-overlay.raf', adjustments);
    if (!keyed) {
      useEditorStore.setState((state) => ({
        selectedImage: { ...state.selectedImage, openTicket: undefined, openKey: undefined, renderMode: 'Legacy' },
        progressiveOpen: null,
      }));
    }
    const sent = new Set(['inline-patch', 'inline-patch-mask', 'inline-container-mask', 'standalone-mask']);
    useEditorStore.setState({ patchesSentToBackend: sent });
    const maskDef = {
      id: 'standalone-container',
      visible: true,
      adjustments: {},
      subMasks: [
        {
          id: 'standalone-mask',
          parameters: { mask_data_base64: 'standalone:snake', maskDataBase64: 'standalone:camel' },
        },
      ],
    };
    const originalMask = structuredClone(maskDef);
    invokeHandler = async (command) => {
      assert.equal(command, Invokes.GenerateMaskOverlay);
      return null;
    };
    const root = await mountInlineConsumerEditor();
    for (let repeat = 0; repeat < 2; repeat++) {
      await act(async () => {
        root.root.findByType(ImageCanvas.type).props.onLiveMaskPreview(maskDef);
        await flushMicrotasks(16);
      });
      const request = calls.filter((call) => call.command === Invokes.GenerateMaskOverlay).at(-1);
      assert.ok(request, 'the actual mounted Editor callback must issue the overlay request');
      const expectedAdjustments = structuredClone(original);
      const expectedMask = structuredClone(originalMask);
      if (!keyed) {
        for (const subMask of [expectedAdjustments.aiPatches[0].subMasks[0], expectedAdjustments.masks[0].subMasks[0], expectedMask.subMasks[0]]) {
          subMask.parameters.mask_data_base64 = null;
          subMask.parameters.maskDataBase64 = null;
        }
      }
      assert.deepEqual(request.args.jsAdjustments, expectedAdjustments);
      assert.deepEqual(request.args.maskDef, expectedMask);
      assert.equal(Object.hasOwn(request.args, 'expectedOpenKey'), keyed);
      if (keyed) assert.deepEqual(request.args.expectedOpenKey, keyA);
      assert.deepEqual(adjustments, original);
      assert.deepEqual(maskDef, originalMask);
      assert.equal(useEditorStore.getState().adjustments, adjustments);
      assert.deepEqual([...sent], ['inline-patch', 'inline-patch-mask', 'inline-container-mask', 'standalone-mask']);
    }
    assert.equal(calls.filter((call) => call.command === Invokes.GenerateMaskOverlay).length, 2);
    await trackedUnmount(root);
  }
});

test('mounted standalone inline mask rejects late IPC and late decode after source replacement or cancellation', async () => {
  const originalImage = globalThis.Image;
  try {
    for (const boundary of ['IPC', 'decode']) {
      resetStores();
      const adjustments = inlineConsumerAdjustments();
      setAccurateImage(ticketA, keyA, '/same-overlay.raf', adjustments);
      useEditorStore.setState({ patchesSentToBackend: new Set(['inline-patch-mask', 'standalone-mask']) });
      const ipc = deferred();
      const decoding = deferred();
      let decodeCount = 0;
      const published = [];
      globalThis.Image = class {
        decode() {
          decodeCount++;
          return decoding.promise;
        }
      };
      invokeHandler = (command) => {
        assert.equal(command, Invokes.GenerateMaskOverlay);
        return ipc.promise;
      };
      const root = await mountInlineConsumerEditor((mounted) => {
        published.push(mounted.root.findByType(ImageCanvas.type).props.maskOverlayUrl);
      });
      const maskDef = { id: 'standalone', visible: true, adjustments: {}, subMasks: [
        { id: 'standalone-mask', parameters: { mask_data_base64: 'kept:snake', maskDataBase64: 'kept:camel' } },
      ] };
      await act(async () => {
        root.root.findByType(ImageCanvas.type).props.onLiveMaskPreview(maskDef);
        await flushMicrotasks();
        if (boundary === 'decode') {
          ipc.resolve('data:image/png;base64,stale-inline');
          await flushMicrotasks();
        }
      });
      const overlay = calls.find((call) => call.command === Invokes.GenerateMaskOverlay);
      assert.deepEqual(overlay.args.maskDef, maskDef);
      assert.deepEqual(overlay.args.jsAdjustments, adjustments);
      assert.deepEqual(overlay.args.expectedOpenKey, keyA);
      assert.equal(decodeCount, boundary === 'decode' ? 1 : 0);
      const ticketA2 = '267e12fe-2a0a-476e-8c52-c64a9dfe5178';
      const keyA2 = { ticket: ticketA2, generation: '146' };
      await act(async () => {
        if (boundary === 'IPC') {
          setAccurateImage(ticketA2, keyA2, '/same-overlay.raf', adjustments);
          ipc.resolve('data:image/png;base64,stale-inline');
        } else {
          const ticketB = 'ba001d9f-18e1-44a5-812a-5cde8845e2bb';
          const store = useEditorStore.getState();
          store.prepareProgressiveIntent(ticketB, '/cancelled-overlay.raf');
          store.activateProgressiveIntent(ticketB, '/cancelled-overlay.raf', 'thumb:cancelled');
          assert.equal(store.cancelProgressiveOpen(ticketB), true);
        }
        decoding.resolve();
        await flushMicrotasks(24);
      });
      assert.equal(decodeCount, boundary === 'decode' ? 1 : 0, 'stale IPC must not enter browser decode');
      assert.equal(published.includes('data:image/png;base64,stale-inline'), false, 'no stale overlay may be committed even transiently');
      assert.equal(root.root.findByType(ImageCanvas.type).props.maskOverlayUrl, null);
      if (boundary === 'IPC') assert.deepEqual(useEditorStore.getState().selectedImage.openKey, keyA2);
      else assert.equal(useEditorStore.getState().progressiveOpen.phase, 'Cancelled');
      await trackedUnmount(root);
    }
  } finally {
    globalThis.Image = originalImage;
  }
});

async function mountDialogExport({ directory = false, paths = [], library = false } = {}) {
  useUIStore.setState({
    activePanels: { leftTop: 'export', leftBottom: null, rightTop: 'adjustments', rightBottom: null },
  });
  const exportStates = [];
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(ExportPanel, {
      exportState: { status: 'success', progress: { current: 0, total: 0 }, errorMessage: '' },
      multiSelectedPaths: paths,
      selectedImage: useEditorStore.getState().selectedImage,
      setExportState: (state) => exportStates.push(state),
      appSettings: null,
      onSettingsChange() {},
      rootPaths: ['/'],
      isVisible: true,
      ...(library ? { onClose() {} } : {}),
    }));
    await flushMicrotasks();
  });
  if (directory) {
    const advanced = root.root.findByProps({
      className: 'w-full flex items-center justify-between p-3.5 hover:bg-card-active transition-colors',
    });
    await act(async () => advanced.props.onClick());
    const preserveFolders = root.root.findByProps({ label: 'export.advanced.preserveFolders' });
    await act(async () => preserveFolders.props.onChange(true));
  }
  const exportButton = () => root.root.findAllByType('button').find(
    (button) => button.props.className?.includes('group rounded-md h-11 w-full'),
  );
  assert.ok(exportButton(), 'the mounted ExportPanel must expose its actual export button');
  return { root, exportStates, exportButton };
}

test('mounted current-edit export captures its accurate key and inline inputs across file and directory dialogs', async () => {
  try {
    for (const directory of [false, true]) {
      resetStores();
      const adjustments = { ...inlineConsumerAdjustments(), exposure: 0.75 };
      const original = structuredClone(adjustments);
      setAccurateImage(ticketA, keyA, '/export-source.raf', adjustments);
      const output = deferred();
      const dialogs = [];
      const dialog = (kind, options) => {
        dialogs.push({ kind, options });
        return output.promise;
      };
      globalThis.__progressiveDialogSave = (options) => dialog('file', options);
      globalThis.__progressiveDialogOpen = (options) => dialog('directory', options);
      invokeHandler = async (command) => {
        if (command === Invokes.EstimateExportSizes) return 64;
        assert.equal(command, Invokes.ExportImages);
        return null;
      };
      const mounted = await mountDialogExport({ directory });
      let exporting;
      await act(async () => {
        exporting = mounted.exportButton().props.onClick();
        await flushMicrotasks();
      });
      assert.equal(dialogs.length, 1);
      assert.equal(dialogs[0].kind, directory ? 'directory' : 'file');
      assert.equal(calls.some((call) => call.command === Invokes.ExportImages), false);
      await act(async () => {
        useUIStore.setState({ isInstantTransition: true });
        output.resolve(directory ? '/exports' : '/exports/result.jpeg');
        await exporting;
      });
      const exported = calls.filter((call) => call.command === Invokes.ExportImages);
      assert.equal(exported.length, 1);
      assert.deepEqual(exported[0].args.expectedOpenKey, keyA);
      assert.notEqual(exported[0].args.expectedOpenKey, keyA, 'the pending dialog cannot retain a mutable key alias');
      assert.deepEqual(exported[0].args.paths, ['/export-source.raf']);
      assert.equal(exported[0].args.currentEditPath, '/export-source.raf');
      assert.deepEqual(exported[0].args.currentEditAdjustments, original);
      assert.notEqual(exported[0].args.currentEditAdjustments, adjustments, 'the exported inputs are the captured clone');
      assert.equal(exported[0].args.isExplicitFilePath, !directory);
      assert.equal(exported[0].args.exportSettings.preserveFolders, directory);
      assert.deepEqual(adjustments, original);
      assert.equal(useEditorStore.getState().adjustments, adjustments);
      assert.equal(mounted.exportStates.length, 1);
      await trackedUnmount(mounted.root);
    }
  } finally {
    delete globalThis.__progressiveDialogSave;
    delete globalThis.__progressiveDialogOpen;
  }
});

test('mounted export refuses source switches same-path ABA edits and missing keyed ownership across output dialogs', async () => {
  try {
    for (const [index, change] of ['source-switch', 'same-path-ABA', 'edited-inputs', 'pending-intent', 'missing-key'].entries()) {
      resetStores();
      const adjustments = { ...inlineConsumerAdjustments(), exposure: 0.25 };
      const original = structuredClone(adjustments);
      setAccurateImage(ticketA, keyA, '/export-same.raf', adjustments);
      if (change === 'missing-key') {
        useEditorStore.setState((state) => ({ selectedImage: { ...state.selectedImage, openKey: undefined } }));
      }
      const output = deferred();
      let dialogCount = 0;
      const dialog = () => {
        dialogCount++;
        return output.promise;
      };
      globalThis.__progressiveDialogSave = dialog;
      globalThis.__progressiveDialogOpen = dialog;
      invokeHandler = async (command) => {
        if (command === Invokes.EstimateExportSizes) return 64;
        throw new Error(`Unexpected export invocation after stale ownership: ${command}`);
      };
      const mounted = await mountDialogExport({ directory: index % 2 === 1 });
      let exporting;
      await act(async () => {
        exporting = mounted.exportButton().props.onClick();
        await flushMicrotasks();
      });
      assert.equal(dialogCount, change === 'missing-key' ? 0 : 1);
      const ticketB = 'ba001d9f-18e1-44a5-812a-5cde8845e2bb';
      const ticketA2 = '267e12fe-2a0a-476e-8c52-c64a9dfe5178';
      await act(async () => {
        if (change === 'source-switch' || change === 'same-path-ABA') {
          setAccurateImage(ticketB, { ticket: ticketB, generation: '150' }, '/export-other.raf', adjustments);
          if (change === 'same-path-ABA') {
            setAccurateImage(ticketA2, { ticket: ticketA2, generation: '151' }, '/export-same.raf', adjustments);
          }
        } else if (change === 'edited-inputs') {
          useEditorStore.getState().setEditor({ adjustments: { ...adjustments, exposure: 1.5 } });
        } else if (change === 'pending-intent') {
          useEditorStore.getState().prepareProgressiveIntent(ticketB, '/export-pending.raf');
        }
        output.resolve('/exports/stale-result.jpeg');
        await exporting;
      });
      assert.equal(calls.filter((call) => call.command === Invokes.ExportImages).length, 0);
      assert.equal(mounted.exportStates.length, 0, 'refusal precedes exporting feedback and cannot silently downgrade');
      assert.deepEqual(adjustments, original);
      await trackedUnmount(mounted.root);
    }
  } finally {
    delete globalThis.__progressiveDialogSave;
    delete globalThis.__progressiveDialogOpen;
  }
});

test('mounted export keeps legacy current-edit arguments unkeyed and explicit batch files independent of the editor', async () => {
  try {
    for (const explicit of [false, true]) {
      resetStores();
      const adjustments = { ...INITIAL_ADJUSTMENTS, aspectRatio: 4 / 3, exposure: -0.5 };
      setAccurateImage(ticketA, keyA, '/legacy-export.raf', adjustments);
      if (!explicit) {
        useEditorStore.setState((state) => ({
          selectedImage: { ...state.selectedImage, openTicket: undefined, openKey: undefined, renderMode: 'Legacy' },
          progressiveOpen: null,
        }));
      }
      const output = deferred();
      globalThis.__progressiveDialogSave = () => output.promise;
      globalThis.__progressiveDialogOpen = () => output.promise;
      invokeHandler = async (command) => {
        if (command === Invokes.EstimateExportSizes) return 64;
        assert.equal(command, Invokes.ExportImages);
        return null;
      };
      const paths = explicit ? ['/independent-C.raf', '/independent-D.raf'] : [];
      const mounted = await mountDialogExport({ paths, library: explicit });
      let exporting;
      await act(async () => {
        exporting = mounted.exportButton().props.onClick();
        await flushMicrotasks();
        if (explicit) {
          useEditorStore.getState().prepareProgressiveIntent('ba001d9f-18e1-44a5-812a-5cde8845e2bb', '/pending.raf');
        }
        output.resolve(explicit ? '/exports' : '/exports/legacy.jpeg');
        await exporting;
      });
      const exported = calls.filter((call) => call.command === Invokes.ExportImages);
      assert.equal(exported.length, 1);
      assert.equal(Object.hasOwn(exported[0].args, 'expectedOpenKey'), false);
      assert.deepEqual(exported[0].args.paths, explicit ? paths : ['/legacy-export.raf']);
      assert.equal(exported[0].args.currentEditPath, explicit ? null : '/legacy-export.raf');
      assert.deepEqual(exported[0].args.currentEditAdjustments, explicit ? null : adjustments);
      await trackedUnmount(mounted.root);
    }
  } finally {
    delete globalThis.__progressiveDialogSave;
    delete globalThis.__progressiveDialogOpen;
  }
});

function queuePreviewResponse(interactive = false) {
  if (!interactive) return new Uint8Array([255, 216, 255, 217]).buffer;
  const buffer = new ArrayBuffer(28);
  const view = new DataView(buffer);
  view.setUint32(0, 0, true);
  view.setUint32(4, 0, true);
  view.setUint32(8, 100, true);
  view.setUint32(12, 80, true);
  view.setUint32(16, 400, true);
  view.setUint32(20, 300, true);
  new Uint8Array(buffer).set([255, 216, 255, 217], 24);
  return buffer;
}

function installQueuePreviewImage() {
  const originalImage = globalThis.Image;
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  const revoked = [];
  let nextUrl = 0;
  globalThis.Image = class {
    decode() {
      return Promise.resolve();
    }
  };
  URL.createObjectURL = () => `blob:queued-preview-${++nextUrl}`;
  URL.revokeObjectURL = (url) => revoked.push(url);
  return {
    revoked,
    restore() {
      globalThis.Image = originalImage;
      URL.createObjectURL = originalCreate;
      URL.revokeObjectURL = originalRevoke;
    },
  };
}

function previewAdjustments(exposure) {
  return { ...INITIAL_ADJUSTMENTS, exposure };
}

async function mountProcessingHook() {
  const { Probe, values } = useTestHooks({ withProcessing: true });
  let root;
  await act(async () => {
    root = trackedCreate(React.createElement(Probe));
  });
  return { root, process: values.process, refs: values.refs };
}

test('preview queue serializes dragging updates and coalesces them into the final release', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-release.raf');
  useEditorStore.setState({ isSliderDragging: true });
  const browser = installQueuePreviewImage();
  const firstInvoke = deferred();
  let active = 0;
  let maxActive = 0;
  invokeHandler = (command, args) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    active++;
    maxActive = Math.max(maxActive, active);
    const response = calls.length === 1 ? firstInvoke.promise : Promise.resolve(queuePreviewResponse(false));
    return response.finally(() => active--);
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    let initial;
    let release;
    await act(async () => {
      initial = mounted.process.executeApplyAdjustments(previewAdjustments(0.1), true, 512);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.2), true, 640);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.4), true, 768);
      mounted.process.applyAdjustments(previewAdjustments(0.7), true, 896);
      release = mounted.process.applyAdjustments(previewAdjustments(1.25), false, 2048);
      assert.equal(calls.length, 1, 'the final request waits for the active interactive invoke');
      firstInvoke.resolve(queuePreviewResponse(true));
      await flushMicrotasks(24);
    });
    await initial;
    await release;
    await flushMicrotasks();
    assert.equal(calls.length, 2, 'superseded intermediate drags never invoke native rendering');
    assert.equal(maxActive, 1, 'only one ApplyAdjustments invoke may be active');
    assert.equal(calls[0].args.isInteractive, true);
    assert.equal(calls[1].args.isInteractive, false);
    assert.equal(calls[1].args.targetResolution, 2048);
    assert.equal(calls[1].args.jsAdjustments.exposure, 1.25);
    assert.equal(useEditorStore.getState().finalPreviewUrl, 'blob:queued-preview-2');
    await trackedUnmount(root);
    root = null;
  } finally {
    if (root) await trackedUnmount(root);
    browser.restore();
  }
});

test('preview queue keeps the newest request when a queued final is followed by newer drag and final intents', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-latest.raf');
  useEditorStore.setState({ isSliderDragging: true });
  const browser = installQueuePreviewImage();
  const firstInvoke = deferred();
  invokeHandler = (command, args) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    if (calls.length === 1) return firstInvoke.promise;
    return Promise.resolve(queuePreviewResponse(args.isInteractive));
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    let first;
    let latest;
    await act(async () => {
      first = mounted.process.executeApplyAdjustments(previewAdjustments(-0.1), true, 512);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.5), false, 1200);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.8), true, 1536);
      latest = mounted.process.executeApplyAdjustments(previewAdjustments(1.6), false, 3072);
      firstInvoke.resolve(queuePreviewResponse(true));
      await flushMicrotasks(24);
    });
    await Promise.all([first, latest]);
    assert.equal(calls.length, 2);
    assert.equal(calls[1].args.jsAdjustments.exposure, 1.6);
    assert.equal(calls[1].args.isInteractive, false);
    assert.equal(calls[1].args.targetResolution, 3072);
    assert.equal(useEditorStore.getState().finalPreviewUrl, 'blob:queued-preview-2');
    await trackedUnmount(root);
    root = null;
  } finally {
    if (root) await trackedUnmount(root);
    browser.restore();
  }
});

test('preview queue drops stale A work after source switch and same-path A/B/A replacement', async () => {
  for (const replacement of ['B', 'A2']) {
    resetStores();
    setAccurateImage(ticketA, keyA, '/queue-owner.raf');
    useEditorStore.setState({ isSliderDragging: true });
    const browser = installQueuePreviewImage();
    const firstInvoke = deferred();
    const staleToasts = [];
    const originalToast = globalThis.__progressiveToast.error;
    globalThis.__progressiveToast.error = (...args) => staleToasts.push(args);
    invokeHandler = (command) => {
      assert.equal(command, Invokes.ApplyAdjustments);
      return firstInvoke.promise;
    };
    let root;
    try {
      const mounted = await mountProcessingHook();
      root = mounted.root;
      let first;
      let queued;
      await act(async () => {
        first = mounted.process.executeApplyAdjustments(previewAdjustments(0.1), true, 512);
        queued = mounted.process.executeApplyAdjustments(previewAdjustments(0.9), false, 2048);
        if (replacement === 'B') {
          setAccurateImage('ba001d9f-18e1-44a5-812a-5cde8845e2bb', { ticket: 'ba001d9f-18e1-44a5-812a-5cde8845e2bb', generation: '42' }, '/queue-B.raf');
        } else {
          setAccurateImage('ba001d9f-18e1-44a5-812a-5cde8845e2bb', { ticket: 'ba001d9f-18e1-44a5-812a-5cde8845e2bb', generation: '42' }, '/queue-B.raf');
          setAccurateImage('267e12fe-2a0a-476e-8c52-c64a9dfe5178', { ticket: '267e12fe-2a0a-476e-8c52-c64a9dfe5178', generation: '43' }, '/queue-owner.raf');
        }
        firstInvoke.reject(new Error('Request cancelled'));
        await flushMicrotasks(24);
      });
      await Promise.all([first, queued]);
      assert.equal(calls.length, 1, `${replacement} replacement must not receive queued A adjustments`);
      assert.deepEqual(staleToasts, [], 'a stale source cancellation cannot publish an error for the replacement');
      assert.equal(useEditorStore.getState().finalPreviewUrl, null);
      await trackedUnmount(root);
      root = null;
    } finally {
      if (root) await trackedUnmount(root);
      globalThis.__progressiveToast.error = originalToast;
      browser.restore();
    }
  }
});

test('preview queue discards pending work on unmount without a late invoke or error toast', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-unmount.raf');
  useEditorStore.setState({ isSliderDragging: true });
  const browser = installQueuePreviewImage();
  const firstInvoke = deferred();
  const staleToasts = [];
  const originalToast = globalThis.__progressiveToast.error;
  globalThis.__progressiveToast.error = (...args) => staleToasts.push(args);
  invokeHandler = (command) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    return firstInvoke.promise;
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    let first;
    let queued;
    await act(async () => {
      first = mounted.process.executeApplyAdjustments(previewAdjustments(0.1), true, 512);
      queued = mounted.process.executeApplyAdjustments(previewAdjustments(1.0), false, 2048);
    });
    await trackedUnmount(root);
    root = null;
    firstInvoke.reject(new Error('Request cancelled'));
    await Promise.all([first, queued]);
    await flushMicrotasks();
    assert.equal(calls.length, 1, 'unmount releases the pending slot without dispatch');
    assert.deepEqual(staleToasts, []);
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);
  } finally {
    if (root) await trackedUnmount(root);
    globalThis.__progressiveToast.error = originalToast;
    browser.restore();
  }
});

test('preview queue keeps a real latest Request cancelled failure visible', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-failure.raf');
  useEditorStore.setState({ isSliderDragging: true });
  const failures = [];
  const originalToast = globalThis.__progressiveToast.error;
  globalThis.__progressiveToast.error = (...args) => failures.push(args);
  invokeHandler = async (command) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    throw new Error('Request cancelled');
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    await act(async () => {
      await mounted.process.executeApplyAdjustments(previewAdjustments(0.6), false, 2048);
    });
    assert.equal(calls.length, 1);
    assert.equal(failures.length, 1);
    assert.equal(failures[0][0], 'Request cancelled');
    assert.equal(failures[0][1].toastId, 'preview-render-error');
    await trackedUnmount(root);
    root = null;
  } finally {
    if (root) await trackedUnmount(root);
    globalThis.__progressiveToast.error = originalToast;
  }
});

test('continuous dragging displays completed same-source intermediate frames before the latest release', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-continuous.raf');
  useEditorStore.setState({ isSliderDragging: true });
  const browser = installQueuePreviewImage();
  const firstInvoke = deferred();
  const finalInvoke = deferred();
  let active = 0;
  let maxActive = 0;
  invokeHandler = (command, args) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    active++;
    maxActive = Math.max(maxActive, active);
    const response = calls.length === 1 ? firstInvoke.promise : finalInvoke.promise;
    return response.finally(() => active--);
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    let first;
    let final;
    await act(async () => {
      first = mounted.process.executeApplyAdjustments(previewAdjustments(0.1), true, 512);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.2), true, 640);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.4), true, 768);
      mounted.process.executeApplyAdjustments(previewAdjustments(0.8), true, 1024);
      final = mounted.process.executeApplyAdjustments(previewAdjustments(1.3), false, 3072);
      firstInvoke.resolve(queuePreviewResponse(true));
      await flushMicrotasks(24);
    });

    assert.equal(calls.length, 2, 'the pending final request starts only after the first response and decode finish');
    assert.equal(maxActive, 1);
    assert.equal(useEditorStore.getState().interactivePatch?.url, 'blob:queued-preview-1');
    assert.equal(useEditorStore.getState().finalPreviewUrl, null);
    assert.equal(calls[1].args.jsAdjustments.exposure, 1.3);
    assert.equal(calls[1].args.isInteractive, false);
    assert.equal(calls[1].args.targetResolution, 3072);

    finalInvoke.resolve(queuePreviewResponse(false));
    await act(async () => {
      await Promise.all([first, final]);
      await flushMicrotasks();
    });
    assert.equal(useEditorStore.getState().finalPreviewUrl, 'blob:queued-preview-2');
    assert.equal(useEditorStore.getState().interactivePatch, null);
    await trackedUnmount(root);
    root = null;
  } finally {
    if (root) await trackedUnmount(root);
    browser.restore();
  }
});

test('queued preview dispatch uses the current viewport and ROI after an in-flight request', async () => {
  resetStores();
  setAccurateImage(ticketA, keyA, '/queue-viewport.raf');
  useEditorStore.setState({
    isSliderDragging: true,
    baseRenderSize: { width: 100, height: 100, offsetX: 0, offsetY: 0, containerWidth: 100, containerHeight: 100 },
  });
  const browser = installQueuePreviewImage();
  const firstInvoke = deferred();
  const finalInvoke = deferred();
  let active = 0;
  let maxActive = 0;
  invokeHandler = (command, args) => {
    assert.equal(command, Invokes.ApplyAdjustments);
    active++;
    maxActive = Math.max(maxActive, active);
    const response = calls.length === 1 ? firstInvoke.promise : finalInvoke.promise;
    return response.finally(() => active--);
  };
  let root;
  try {
    const mounted = await mountProcessingHook();
    root = mounted.root;
    mounted.refs.transformWrapperRef.current = {
      instance: { transformState: { scale: 2, positionX: -50, positionY: -50 } },
    };
    let first;
    let latest;
    await act(async () => {
      first = mounted.process.executeApplyAdjustments(previewAdjustments(0.1), true, 512);
      latest = mounted.process.executeApplyAdjustments(previewAdjustments(1.4), false, 3072);
    });
    await act(async () => {
      mounted.refs.transformWrapperRef.current.instance.transformState = {
        scale: 2,
        positionX: -100,
        positionY: -100,
      };
      useEditorStore.setState({
        baseRenderSize: { width: 200, height: 200, offsetX: 0, offsetY: 0, containerWidth: 200, containerHeight: 200 },
      });
    });
    assert.equal(useEditorStore.getState().baseRenderSize.width, 200, 'the new viewport has rendered before the request drains');
    await act(async () => {
      firstInvoke.resolve(queuePreviewResponse(true));
      await flushMicrotasks(24);
    });

    assert.equal(calls.length, 2);
    assert.deepEqual(calls[0].args.roi.map((value) => Number(value.toFixed(2))), [0.23, 0.23, 0.54, 0.54]);
    assert.deepEqual(calls[1].args.roi.map((value) => Number(value.toFixed(2))), [0.24, 0.24, 0.52, 0.52]);
    assert.equal(calls[1].args.jsAdjustments.exposure, 1.4);
    assert.equal(calls[1].args.isInteractive, false);
    assert.equal(calls[1].args.targetResolution, 3072);
    assert.equal(maxActive, 1);
    assert.equal(useEditorStore.getState().interactivePatch?.url, 'blob:queued-preview-1');

    finalInvoke.resolve(queuePreviewResponse(false));
    await act(async () => {
      await Promise.all([first, latest]);
      await flushMicrotasks();
    });
    assert.equal(useEditorStore.getState().finalPreviewUrl, 'blob:queued-preview-2');
    await trackedUnmount(root);
    root = null;
  } finally {
    if (root) await trackedUnmount(root);
    browser.restore();
  }
});
