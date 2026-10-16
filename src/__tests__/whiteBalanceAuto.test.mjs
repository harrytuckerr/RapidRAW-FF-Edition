// Exercises the real hook, Zustand editor store and ColorPanel native select. Only
// native invoke, translation, animation, and unrelated visual controls are mocked.
import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { build } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);
const { create, act } = require('react-test-renderer');
const english = JSON.parse(await readFile('src/i18n/locales/en.json', 'utf8'));
const translate = (key) => key.split('.').reduce((value, part) => value?.[part], english) ?? key;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = {
  addEventListener() {},
  removeEventListener() {},
  documentElement: { style: { setProperty() {} } },
};
globalThis.window = {
  addEventListener() {},
  removeEventListener() {},
  clearTimeout,
  setTimeout,
};
globalThis.requestAnimationFrame = (callback) => setTimeout(() => callback(0), 0);
globalThis.cancelAnimationFrame = clearTimeout;
globalThis.ResizeObserver = class {
  observe() {}
  disconnect() {}
};

let pendingInvokes = [];
let toasts = [];
globalThis.__wbInvoke = (name, args) => {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  pendingInvokes.push({ name, args, promise, resolve, reject });
  return promise;
};
globalThis.__wbToast = { error: (message) => toasts.push(message), success() {}, info() {} };
globalThis.__wbTranslate = translate;

const bundle = await build({
  stdin: {
    contents: `
      export { default as ColorPanel } from './src/components/adjustments/Color';
      export { useEditorActions, debouncedSetHistory } from './src/hooks/useEditorActions';
      export { useEditorStore } from './src/store/useEditorStore';
      export { INITIAL_ADJUSTMENTS } from './src/utils/adjustments';
    `,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'wb-test-boundaries',
      setup(buildApi) {
        buildApi.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/plugin-os|react-i18next|react-toastify|framer-motion|\.\.\/ui\/(Slider|ColorWheel))$/,
          },
          (args) => ({ path: args.path, namespace: 'wb-test-mock' }),
        );
        buildApi.onLoad({ filter: /.*/, namespace: 'wb-test-mock' }, ({ path }) => ({
          contents:
            path === '@tauri-apps/api/core'
              ? 'export const invoke=(...args)=>globalThis.__wbInvoke(...args);'
              : path === '@tauri-apps/plugin-os'
                ? "export const platform=()=>'android';"
                : path === 'react-i18next'
                  ? 'export const useTranslation=()=>({t:(key)=>globalThis.__wbTranslate(key)});'
                  : path === 'react-toastify'
                    ? 'export const toast=globalThis.__wbToast;'
                    : path === 'framer-motion'
                      ? 'export const motion=new Proxy({}, {get:(_,tag)=>tag}); export const AnimatePresence=({children})=>children;'
                      : 'export default function VisualControl(){return null;} ',
        }));
      },
    },
  ],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const { ColorPanel, useEditorActions, useEditorStore, INITIAL_ADJUSTMENTS, debouncedSetHistory } = mod.exports;

const sample = { r: 0.3, g: 0.31, b: 0.29, count: 120, temperature: 5600, tint: 4 };
const image = (path) => ({ path, isReady: true, asShotWhiteBalance: { temperature: 5000, tint: 0 } });
let mounted = [];
let hookHandlers = [];

function reset() {
  pendingInvokes = [];
  toasts = [];
  hookHandlers = [];
  const initial = {
    ...INITIAL_ADJUSTMENTS,
    exposure: 0.75,
    profile: { base: 'camera-base', look: 'camera-look', amount: 42 },
  };
  useEditorStore.setState({
    selectedImage: image('/synthetic/a.RAF'),
    adjustments: initial,
    history: [initial],
    historyIndex: 0,
  });
}

function HookHarness() {
  hookHandlers.push(useEditorActions().handleAutoWhiteBalance);
  return null;
}

function ColorHarness({ isForMask = false }) {
  const adjustments = useEditorStore((state) => state.adjustments);
  const { setAdjustments, handleAutoWhiteBalance } = useEditorActions();
  return React.createElement(ColorPanel, {
    adjustments,
    appSettings: null,
    asShotWhiteBalance: { temperature: 5000, tint: 0 },
    isForMask,
    onAutoWhiteBalance: handleAutoWhiteBalance,
    setAdjustments,
  });
}

async function mount(element) {
  let root;
  await act(async () => {
    root = create(element);
  });
  mounted.push(root);
  return root;
}

function createDeferredRequest(handler, message = 'Auto failed') {
  let promise;
  act(() => {
    promise = handler(message);
  });
  return promise;
}

async function resolveRequest(request, response) {
  await act(async () => {
    request.resolve(response);
    await request.promise;
  });
}

function renderedText(node) {
  if (node === null || node === undefined) return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  return (node.children || []).map(renderedText).join('');
}

async function choose(root, label) {
  const select = root.root.findByType('select');
  const option = select.findAllByType('option').find((node) => renderedText(node) === label);
  assert.ok(option, `native select option ${label} exists`);
  await act(async () => select.props.onChange({ currentTarget: { value: option.props.value } }));
}

afterEach(async () => {
  for (const root of mounted.splice(0)) await act(async () => root.unmount());
  debouncedSetHistory.cancel();
});

test('real Auto hook sends full-frame UV corners and merges WB into concurrent unrelated edits', async () => {
  reset();
  await mount(React.createElement(HookHarness));
  const requestPromise = createDeferredRequest(hookHandlers[0]);
  const request = pendingInvokes[0];
  assert.equal(request.name, 'sample_white_balance');
  assert.deepEqual(request.args, {
    expectedPath: '/synthetic/a.RAF',
    corners: [
      { x: 0, y: 0 },
      { x: 1, y: 0 },
      { x: 1, y: 1 },
      { x: 0, y: 1 },
    ],
  });

  const current = useEditorStore.getState().adjustments;
  useEditorStore.setState({ adjustments: { ...current, exposure: 2.25, profile: { ...current.profile, amount: 88 } } });
  await resolveRequest(request, sample);
  await requestPromise;
  const updated = useEditorStore.getState().adjustments;
  assert.deepEqual(updated.whiteBalance, { temperature: sample.temperature, tint: sample.tint });
  assert.equal(updated.temperature, 0);
  assert.equal(updated.tint, 0);
  assert.equal(updated.exposure, 2.25);
  assert.deepEqual(updated.profile, { base: 'camera-base', look: 'camera-look', amount: 88 });
});

test('real Auto hook refuses ready images without a nonempty string source path', async () => {
  for (const path of [undefined, null, '', 42]) {
    reset();
    useEditorStore.setState({ selectedImage: image(path) });
    await mount(React.createElement(HookHarness));
    const before = useEditorStore.getState().adjustments;
    await createDeferredRequest(hookHandlers.at(-1));
    assert.equal(pendingInvokes.length, 0);
    assert.equal(useEditorStore.getState().adjustments, before);
    assert.deepEqual(toasts, []);
    await act(async () => mounted.pop().unmount());
  }
});

test('real Auto hook rejects each zero RGB channel, zero/fractional count, and native failure without WB changes', async () => {
  for (const invalid of [
    { ...sample, r: 0 },
    { ...sample, g: 0 },
    { ...sample, b: 0 },
    { ...sample, count: 0 },
    { ...sample, count: 1.5 },
  ]) {
    reset();
    await mount(React.createElement(HookHarness));
    const before = useEditorStore.getState().adjustments;
    const promise = createDeferredRequest(hookHandlers.at(-1));
    await resolveRequest(pendingInvokes.at(-1), invalid);
    await promise;
    assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, before.whiteBalance);
    assert.deepEqual(toasts, ['Auto failed']);
    await act(async () => mounted.pop().unmount());
  }

  reset();
  await mount(React.createElement(HookHarness));
  const before = useEditorStore.getState().adjustments.whiteBalance;
  const promise = createDeferredRequest(hookHandlers[0]);
  const request = pendingInvokes[0];
  await act(async () => {
    request.reject(new Error('native unavailable'));
    await promise;
  });
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, before);
  assert.deepEqual(toasts, ['Auto failed']);
});

test('direct-store WB changes and real undo A-to-B-to-A invalidate a pending Auto result', async () => {
  reset();
  await mount(React.createElement(HookHarness));
  const before = useEditorStore.getState().adjustments;
  const promise = createDeferredRequest(hookHandlers[0]);
  useEditorStore.setState({ adjustments: { ...before, temperature: 24 } });
  useEditorStore.setState({ adjustments: before });
  await resolveRequest(pendingInvokes[0], sample);
  await promise;
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, before.whiteBalance);

  const undoBase = useEditorStore.getState().adjustments;
  const undoChanged = { ...undoBase, tint: 11 };
  useEditorStore.setState({ adjustments: undoBase, history: [undoBase, undoChanged, undoBase], historyIndex: 2 });
  const second = createDeferredRequest(hookHandlers[0]);
  useEditorStore.getState().undo();
  useEditorStore.getState().undo();
  await resolveRequest(pendingInvokes.at(-1), sample);
  await second;
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, undoBase.whiteBalance);
});

test('source A-to-B-to-A and a newer Auto after hook remount prevent the older result from winning', async () => {
  reset();
  const oldRoot = await mount(React.createElement(HookHarness));
  const oldHandler = hookHandlers[0];
  const sourceStalePromise = createDeferredRequest(oldHandler);
  const sourceStaleRequest = pendingInvokes[0];
  const originalImage = useEditorStore.getState().selectedImage;
  useEditorStore.setState({ selectedImage: image('/synthetic/b.RAF') });
  useEditorStore.setState({ selectedImage: originalImage });
  const startingWhiteBalance = useEditorStore.getState().adjustments.whiteBalance;
  await resolveRequest(sourceStaleRequest, { ...sample, temperature: 7300 });
  await sourceStalePromise;
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, startingWhiteBalance);

  const oldPromise = createDeferredRequest(oldHandler);
  const oldRequest = pendingInvokes.at(-1);
  await act(async () => oldRoot.unmount());
  mounted = mounted.filter((root) => root !== oldRoot);

  await mount(React.createElement(HookHarness));
  const newPromise = createDeferredRequest(hookHandlers.at(-1));
  const newRequest = pendingInvokes.at(-1);
  await resolveRequest(oldRequest, { ...sample, temperature: 7300 });
  await oldPromise;
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, startingWhiteBalance);
  await resolveRequest(newRequest, { ...sample, temperature: 6200 });
  await newPromise;
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, { temperature: 6200, tint: sample.tint });
});

test('real ColorPanel native select applies As Shot/Kelvin without touching profile or exposure; Auto dispatches action', async () => {
  reset();
  const root = await mount(React.createElement(ColorHarness));
  const select = root.root.findByType('select');
  assert.equal(select.props['aria-label'], translate('adjustments.color.whiteBalance'));
  assert.match(select.props.className, /min-h-11/);
  assert.match(select.props.className, /focus-visible/);
  const original = useEditorStore.getState().adjustments;
  await choose(root, 'As Shot');
  let updated = useEditorStore.getState().adjustments;
  assert.equal(updated.whiteBalance, null);
  assert.equal(updated.temperature, 0);
  assert.equal(updated.tint, 0);
  assert.equal(updated.exposure, original.exposure);
  assert.deepEqual(updated.profile, original.profile);

  await choose(root, 'Shade · 7500 K');
  updated = useEditorStore.getState().adjustments;
  assert.deepEqual(updated.whiteBalance, { temperature: 7500, tint: 0 });
  assert.equal(updated.exposure, original.exposure);
  assert.deepEqual(updated.profile, original.profile);

  await choose(root, 'Auto');
  assert.equal(pendingInvokes.at(-1).name, 'sample_white_balance');
  assert.deepEqual(pendingInvokes.at(-1).args.corners, [
    { x: 0, y: 0 },
    { x: 1, y: 0 },
    { x: 1, y: 1 },
    { x: 0, y: 1 },
  ]);
  await resolveRequest(pendingInvokes.at(-1), sample);
});

test('real mask ColorPanel omits global preset/Auto select', async () => {
  reset();
  let autoCalled = false;
  function MaskHarness() {
    const adjustments = useEditorStore((state) => state.adjustments);
    const { setAdjustments } = useEditorActions();
    return React.createElement(ColorPanel, {
      adjustments,
      appSettings: null,
      asShotWhiteBalance: { temperature: 5000, tint: 0 },
      isForMask: true,
      onAutoWhiteBalance: async () => {
        autoCalled = true;
      },
      setAdjustments,
    });
  }
  const root = await mount(React.createElement(MaskHarness));
  assert.equal(root.root.findAllByType('select').length, 0);
  assert.equal(autoCalled, false);
  assert.deepEqual(useEditorStore.getState().adjustments.whiteBalance, null);
});
