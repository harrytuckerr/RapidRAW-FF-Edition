import assert from 'node:assert/strict';
import { after, afterEach, test } from 'node:test';
import { createRequire } from 'node:module';
import { build, stop as stopEsbuild } from 'esbuild';
import React from 'react';

const require = createRequire(import.meta.url);

const bundle = await build({
  stdin: {
    contents: `export { useImageProcessing, getPreviewPhaseTimings } from './src/hooks/useImageProcessing';
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
      name: 'fold-preview-native-boundaries',
      setup(builder) {
        const mocked = new Map([
          [
            '@tauri-apps/api/core',
            `export const invoke = (...args) => globalThis.__foldInvoke(...args); export const convertFileSrc = x => x;`,
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
          (args) => ({ path: args.path, namespace: 'fold-mock' }),
        );
        builder.onLoad({ filter: /.*/, namespace: 'fold-mock' }, (args) => ({
          contents: mocked.get(args.path),
          loader: 'js',
        }));
        builder.onResolve({ filter: /\.css$/ }, (args) => ({ path: args.path, namespace: 'fold-mock-css' }));
        builder.onLoad({ filter: /.*/, namespace: 'fold-mock-css' }, () => ({
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
  getPreviewPhaseTimings,
  INITIAL_ADJUSTMENTS,
  Invokes,
  Panel,
  useEditorStore,
  useImageProcessing,
  useLibraryStore,
  useSettingsStore,
  useUIStore,
} = mod.exports;
const { create, act } = require('react-test-renderer');

// Timing is opt-in; console output from the opt-in path is silenced for the TAP run.
globalThis.__RAPIDRAW_PREVIEW_TIMING__ = true;
const originalConsoleDebug = console.debug;
console.debug = () => {};

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

// Both commands are deferred so each test controls completion order.
const uncroppedRequests = [];
const applyRequests = [];
globalThis.__foldInvoke = (command, args) => {
  if (command === Invokes.GenerateUncroppedPreview) {
    return new Promise((resolve, reject) => uncroppedRequests.push({ args, done: false, resolve, reject }));
  }
  if (command === Invokes.ApplyAdjustments) {
    return new Promise((resolve, reject) => applyRequests.push({ args, done: false, resolve, reject }));
  }
  return Promise.resolve(null);
};

const pendingOf = (list) => list.filter((request) => !request.done);
const settleNext = (list, value) => {
  const next = list.find((request) => !request.done);
  assert.ok(next, 'expected an in-flight request');
  next.done = true;
  next.resolve(value);
};
const jpegBytes = () => new TextEncoder().encode('JPEG-PREVIEW').buffer;

const roots = new Set();
let latestValue = null;
function Harness({ onValue }) {
  const transformWrapperRef = React.useRef(null);
  const prevAdjustmentsRef = React.useRef(null);
  const previewJobIdRef = React.useRef(0);
  const latestRenderedJobIdRef = React.useRef(0);
  const currentResRef = React.useRef(1920);
  onValue(
    useImageProcessing(transformWrapperRef, prevAdjustmentsRef, {
      previewJobIdRef,
      latestRenderedJobIdRef,
      currentResRef,
    }),
  );
  return null;
}

async function mount({ strict = false } = {}) {
  let root;
  const element = React.createElement(Harness, { onValue: (value) => (latestValue = value) });
  await act(async () => {
    root = create(strict ? React.createElement(React.StrictMode, null, element) : element);
  });
  roots.add(root);
  return root;
}

async function unmount(root) {
  if (!roots.delete(root)) return;
  await act(async () => root.unmount());
}

async function wait(ms) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

const image = (path) => ({ path, isReady: true, openTicket: undefined });
const adj = (exposure) => ({ ...INITIAL_ADJUSTMENTS, exposure });
const TICKET_A = '11111111-1111-4111-8111-111111111111';
const TICKET_B = '22222222-2222-4222-8222-222222222222';
const keyedImage = (path, ticket, generation) => ({
  path,
  isReady: true,
  renderMode: 'AccurateXtrans1Encoded',
  openTicket: ticket,
  openKey: { ticket, generation },
});
const progressiveFor = (path, ticket, generation) => ({
  ticket,
  path,
  openKey: { ticket, generation },
  phase: 'Accurate',
  saveReady: true,
  placeholderUrl: null,
  placeholderUsed: false,
  error: null,
});

function resetStores(path) {
  useUIStore.setState({ activeView: 'editor', activePanel: Panel.Crop });
  useEditorStore.setState({
    selectedImage: image(path),
    progressiveOpen: null,
    pendingOpenIntent: null,
    adjustments: adj(0),
    uncroppedAdjustedPreviewUrl: null,
    finalPreviewUrl: null,
    interactivePatch: null,
  });
  useLibraryStore.setState({ multiSelectedPaths: [] });
  useSettingsStore.setState({ appSettings: null });
}

function setKeyedSource(path, ticket, generation) {
  useEditorStore.setState({
    selectedImage: keyedImage(path, ticket, generation),
    progressiveOpen: progressiveFor(path, ticket, generation),
  });
}

afterEach(async () => {
  for (const root of [...roots]) await unmount(root);
  uncroppedRequests.length = 0;
  applyRequests.length = 0;
  latestValue = null;
});

after(async () => {
  console.debug = originalConsoleDebug;
  delete globalThis.__RAPIDRAW_PREVIEW_TIMING__;
  await stopEsbuild();
});

test('a burst keeps one uncropped request active and runs only the latest pending adjustment', async () => {
  resetStores('A');
  const root = await mount();
  await wait(5);
  assert.equal(uncroppedRequests.length, 1, 'leading request starts immediately');
  for (let exposure = 1; exposure <= 5; exposure += 1) {
    await act(async () => useEditorStore.setState({ adjustments: adj(exposure) }));
    await wait(3);
  }
  await wait(60);
  assert.equal(uncroppedRequests.length, 1, 'no overlapping request while one is in flight');
  await act(async () => settleNext(uncroppedRequests, 'data:A-1'));
  await wait(5);
  assert.equal(uncroppedRequests.length, 2, 'exactly one follow-up after the active request completes');
  assert.equal(uncroppedRequests[1].args.jsAdjustments.exposure, 5, 'the follow-up carries the latest adjustments');
  await act(async () => settleNext(uncroppedRequests, 'data:A-5'));
  await wait(5);
  assert.equal(uncroppedRequests.length, 2, 'intermediate adjustments are never requested');
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, 'data:A-5');
  await unmount(root);
});

test('legacy pending work for a previous image never runs for the selected image', async () => {
  resetStores('A');
  const root = await mount();
  await wait(5);
  await act(async () => useEditorStore.setState({ adjustments: adj(2) }));
  await wait(50);
  assert.equal(uncroppedRequests.length, 1, 'pending adjustment waits behind the active request');
  await act(async () => useEditorStore.setState({ selectedImage: image('B') }));
  await act(async () => settleNext(uncroppedRequests, 'data:stale-A'));
  await wait(5);
  assert.equal(uncroppedRequests.length, 1, 'pending work for image A is dropped for image B');
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null, 'stale completion never publishes');
  await act(async () => useEditorStore.setState({ selectedImage: image('A'), adjustments: adj(3) }));
  await wait(50);
  assert.equal(uncroppedRequests.length, 2, 'a fresh request is allowed after returning to A');
  await act(async () => settleNext(uncroppedRequests, 'data:A-3'));
  await wait(5);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, 'data:A-3');
  await unmount(root);
});

test('a replaced ticket on the same path drops a trailing call that was queued for the old ticket', async () => {
  setKeyedSource('A', TICKET_A, '1');
  resetStores('A');
  setKeyedSource('A', TICKET_A, '1');
  const root = await mount();
  await wait(5);
  assert.equal(uncroppedRequests.length, 1);
  assert.deepEqual(uncroppedRequests[0].args.expectedOpenKey, { ticket: TICKET_A, generation: '1' });

  // Queue a trailing call for ticket A1, then switch away and back on the same path before it fires.
  await act(async () => useEditorStore.setState({ adjustments: adj(9) }));
  await act(async () => useEditorStore.setState({ selectedImage: image('B'), progressiveOpen: null }));
  await act(async () => setKeyedSource('A', TICKET_B, '2'));
  await wait(60);
  assert.equal(uncroppedRequests.length, 1, 'the trailing call for the old ticket never dispatches');

  await act(async () => settleNext(uncroppedRequests, 'data:A1-stale'));
  await wait(5);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null, 'the old-ticket response never publishes');

  await act(async () => useEditorStore.setState({ adjustments: adj(10) }));
  await wait(5);
  assert.equal(uncroppedRequests.length, 2, 'a fresh adjustment on the replaced ticket dispatches');
  assert.deepEqual(uncroppedRequests[1].args.expectedOpenKey, { ticket: TICKET_B, generation: '2' });
  assert.equal(uncroppedRequests[1].args.jsAdjustments.exposure, 10);
  await act(async () => settleNext(uncroppedRequests, 'data:B2-10'));
  await wait(5);
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, 'data:B2-10');
  await unmount(root);
});

test('a pending request queued for a replaced ticket is dropped when the active request completes', async () => {
  resetStores('A');
  setKeyedSource('A', TICKET_A, '1');
  const root = await mount();
  await wait(5);
  await act(async () => useEditorStore.setState({ adjustments: adj(9) }));
  await wait(50);
  assert.equal(uncroppedRequests.length, 1, 'the queued request waits behind the active request');
  await act(async () => setKeyedSource('A', TICKET_B, '2'));
  await act(async () => settleNext(uncroppedRequests, 'data:A1-late'));
  await wait(5);
  assert.equal(uncroppedRequests.length, 1, 'the queued old-ticket request never dispatches after completion');
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null);
  await unmount(root);
});

test('unmount cancels pending uncropped work and drops a late completion, including under StrictMode', async () => {
  resetStores('A');
  const root = await mount({ strict: true });
  await wait(5);
  await act(async () => useEditorStore.setState({ adjustments: adj(7) }));
  await unmount(root);
  const before = uncroppedRequests.length;
  await act(async () => settleNext(uncroppedRequests, 'data:late'));
  await wait(60);
  assert.equal(uncroppedRequests.length, before, 'no follow-up starts after unmount');
  assert.equal(useEditorStore.getState().uncroppedAdjustedPreviewUrl, null, 'late completion is not published');
});

test('the apply callback keeps one active request, runs the latest pending adjustment, and records real job phases', async () => {
  resetStores('A');
  const root = await mount();
  await wait(5);
  const sampleMaxInFlight = () => pendingOf(applyRequests).length;
  let maxInFlight = 0;
  await act(async () => latestValue.applyAdjustments(adj(1), false));
  maxInFlight = Math.max(maxInFlight, sampleMaxInFlight());
  await act(async () => latestValue.applyAdjustments(adj(2), false));
  maxInFlight = Math.max(maxInFlight, sampleMaxInFlight());
  await act(async () => latestValue.applyAdjustments(adj(3), false));
  maxInFlight = Math.max(maxInFlight, sampleMaxInFlight());
  assert.equal(maxInFlight, 1, 'at most one ApplyAdjustments request is active');

  // Settle every in-flight apply request in order; the pipeline dispatches the latest pending one.
  while (pendingOf(applyRequests).length > 0) {
    await act(async () => settleNext(applyRequests, jpegBytes()));
    await wait(5);
  }
  const lastDispatched = applyRequests[applyRequests.length - 1];
  assert.equal(lastDispatched.args.jsAdjustments.exposure, 3, 'the latest apply adjustment is dispatched last');
  assert.equal(
    applyRequests.some((request) => request.args.jsAdjustments.exposure === 2),
    false,
    'the intermediate adjustment is never dispatched after being superseded',
  );

  const applyRecords = getPreviewPhaseTimings().filter((record) => record.kind === 'apply');
  assert.ok(applyRecords.length >= 2, 'apply job phases are recorded');
  const published = applyRecords.filter((record) => record.phase === 'published');
  assert.ok(published.length >= 1, 'a published apply phase is recorded');
  for (const record of applyRecords) {
    for (const key of ['seq', 'queueWaitMs', 'invokeMs']) assert.equal(typeof record[key], 'number', key);
    assert.ok(record.publishMs === null || typeof record.publishMs === 'number');
    assert.equal(JSON.stringify(record).includes('exposure'), false, 'records carry no adjustment JSON');
  }
  assert.ok(useEditorStore.getState().finalPreviewUrl?.startsWith('blob:'), 'the final apply frame is published');
  await unmount(root);
});

async function settleAllApply() {
  while (pendingOf(applyRequests).length > 0) {
    await act(async () => settleNext(applyRequests, jpegBytes()));
    await wait(5);
  }
}

// Observe actual admission: draining only current requests does not settle the
// hook's still-scheduled mount render. The deadline is a failure bound, not a sleep.
async function settleMountBaseline(native = false) {
  const before = getPreviewPhaseTimings().length;
  const deadline = performance.now() + 1000;
  while (applyRequests.length === 0 && performance.now() < deadline) await wait(5);
  assert.ok(performance.now() <= deadline, 'mount baseline admission exceeded its 1000ms deadline');
  assert.equal(applyRequests.length, 1, 'one mount baseline must be admitted within 1000ms');
  const baseline = applyRequests[0];
  assert.equal(baseline.args.jsAdjustments.exposure, 0, 'the admitted request is the actual mount baseline');
  assert.equal(baseline.done, false, 'the baseline is genuinely in flight before settlement');
  if (native) {
    await act(async () => settleNext(applyRequests, new TextEncoder().encode('WGPU_RENDER-frame').buffer));
    await wait(5);
  } else {
    await settleAllApply();
  }
  assert.equal(baseline.done, true, 'the actual mount baseline has settled');
  assert.equal(pendingOf(applyRequests).length, 0, 'no admitted baseline work remains');
  const records = getPreviewPhaseTimings().slice(before);
  assert.equal(records.length, 1, 'one actual mount baseline completion was observed');
  assert.equal(records[0].kind, 'apply');
  assert.equal(records[0].phase, native ? 'native_complete' : 'published');
  if (native) {
    assert.equal(useEditorStore.getState().finalPreviewUrl, null, 'a native baseline creates no encoded frame');
  } else {
    assert.ok(useEditorStore.getState().finalPreviewUrl?.startsWith('blob:'), 'the mount baseline actually published');
  }
}

function rejectNext(list, error) {
  const next = list.find((request) => !request.done);
  assert.ok(next, 'expected an in-flight request');
  next.done = true;
  next.reject(error);
}

test('a delayed apply rejection records a positive invoke duration and a failed phase', async () => {
  resetStores('A');
  const root = await mount();
  await settleMountBaseline();
  const before = getPreviewPhaseTimings().length;
  await act(async () => latestValue.applyAdjustments(adj(4), false));
  await wait(25);
  await act(async () => rejectNext(applyRequests, new Error('decoder failed')));
  await wait(5);
  const records = getPreviewPhaseTimings()
    .slice(before)
    .filter((record) => record.kind === 'apply');
  assert.equal(records.length, 1);
  assert.equal(records[0].phase, 'failed');
  assert.ok(records[0].invokeMs >= 15, `the delayed rejection has a real duration (got ${records[0].invokeMs})`);
  assert.equal(records[0].publishMs, null);
  await unmount(root);
});

test('a superseded apply rejection records a discarded phase while the latest request still runs', async () => {
  resetStores('A');
  const root = await mount();
  await settleMountBaseline();
  const before = getPreviewPhaseTimings().length;
  await act(async () => latestValue.applyAdjustments(adj(5), false));
  await wait(20);
  await act(async () => latestValue.applyAdjustments(adj(6), false));
  await wait(5);
  await act(async () => rejectNext(applyRequests, new Error('old decode failed')));
  await wait(5);
  await settleAllApply();
  const records = getPreviewPhaseTimings()
    .slice(before)
    .filter((record) => record.kind === 'apply');
  assert.deepEqual(
    records.map((record) => record.phase),
    ['discarded', 'published'],
    'a superseded rejection is discarded, not reported as a failure',
  );
  assert.ok(records[0].invokeMs >= 10, 'the discarded rejection still has a positive invoke duration');
  await unmount(root);
});

test('a native sentinel records native_complete and is not reported as a store publication', async () => {
  resetStores('A');
  const root = await mount();
  await settleMountBaseline(true);
  const before = getPreviewPhaseTimings().length;
  await act(async () => latestValue.applyAdjustments(adj(8), false));
  await wait(5);
  await act(async () => settleNext(applyRequests, new TextEncoder().encode('WGPU_RENDER-frame').buffer));
  await wait(5);
  const records = getPreviewPhaseTimings()
    .slice(before)
    .filter((record) => record.kind === 'apply');
  assert.equal(records.length, 1);
  assert.equal(records[0].phase, 'native_complete');
  assert.equal(records[0].publishMs, null);
  assert.equal(useEditorStore.getState().finalPreviewUrl, null, 'no encoded frame is published for the sentinel');
  await unmount(root);
});

test('uncropped timing records carry real job IDs, numbers, and no adjustment JSON', async () => {
  resetStores('A');
  const before = getPreviewPhaseTimings().length;
  const root = await mount();
  await wait(5);
  await act(async () => settleNext(uncroppedRequests, 'data:timing'));
  await wait(5);
  const records = getPreviewPhaseTimings()
    .slice(before)
    .filter((record) => record.kind === 'uncropped');
  assert.equal(records.length, 1);
  const [record] = records;
  assert.equal(record.phase, 'published');
  for (const key of ['seq', 'queueWaitMs', 'invokeMs', 'publishMs']) {
    assert.ok(record[key] === null || typeof record[key] === 'number', key);
  }
  assert.equal(JSON.stringify(record).includes('exposure'), false);
  await unmount(root);
});
