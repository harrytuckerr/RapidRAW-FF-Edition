// Run with: node --test src/__tests__/imageImport.test.mjs using the existing installed dependencies.
// Exercises the real useFileOperations/useTauriListeners hooks, the shared import
// coordinator and the real process store. Only the native transport (invoke/listen),
// toasts and unrelated platform modules are mocked.
import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { build } from 'esbuild';
import React from 'react';
const require = createRequire(import.meta.url);
const { create, act } = require('react-test-renderer');
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = { addEventListener() {}, removeEventListener() {} };
globalThis.window = {
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
};
globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 0);
globalThis.cancelAnimationFrame = (id) => clearTimeout(id);

// ---- Native transport mocks ------------------------------------------------
const transport = {
  invokeCalls: [],
  invokeImpl: null,
  handlers: new Map(), // event name -> Set<handler>
  listenMode: 'immediate', // 'immediate' | 'deferred'
  pendingListens: [],
  registrations: [],
  unlistenCalls: [],
  unlistenImpl: () => {},
};
function resetTransport() {
  transport.invokeCalls = [];
  transport.invokeImpl = () => Promise.resolve();
  transport.handlers = new Map();
  transport.listenMode = 'immediate';
  transport.pendingListens = [];
  transport.registrations = [];
  transport.unlistenCalls = [];
  transport.unlistenImpl = () => {};
}
globalThis.__importTestInvoke = (name, args) => {
  transport.invokeCalls.push({ name, args });
  return transport.invokeImpl(name, args);
};
globalThis.__importTestListen = (name, handler) => {
  const register = () => {
    if (!transport.handlers.has(name)) transport.handlers.set(name, new Set());
    transport.handlers.get(name).add(handler);
    transport.registrations.push({ name, handler });
    return () => {
      transport.unlistenCalls.push(name);
      const result = transport.unlistenImpl(name);
      transport.handlers.get(name)?.delete(handler);
      return result;
    };
  };
  if (transport.listenMode === 'immediate') return Promise.resolve(register());
  return new Promise((resolve, reject) => {
    transport.pendingListens.push({ name, resolve: () => resolve(register()), reject });
  });
};
const toasts = [];
globalThis.__importTestToast = {
  error: (message, options) => toasts.push({ kind: 'error', message, toastId: options?.toastId }),
  info: (message, options) => toasts.push({ kind: 'info', message, toastId: options?.toastId }),
  success: (message, options) => toasts.push({ kind: 'success', message, toastId: options?.toastId }),
  dismiss: () => {},
};

const bundle = await build({
  stdin: {
    contents: `export { useFileOperations } from './src/hooks/useFileOperations';
      export { useTauriListeners } from './src/hooks/useTauriListeners';
      export { useProcessStore } from './src/store/useProcessStore';
      export { useLibraryStore } from './src/store/useLibraryStore';
      export * from './src/utils/imageImport';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'mock-native-boundaries',
      setup(b) {
        b.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/api\/event|@tauri-apps\/plugin-dialog|@tauri-apps\/plugin-os|react-toastify|react-i18next)$/,
          },
          (a) => ({ path: a.path, namespace: 'mock' }),
        );
        b.onLoad({ filter: /.*/, namespace: 'mock' }, (a) => ({
          contents:
            a.path === '@tauri-apps/api/core'
              ? 'export const invoke = (...a) => globalThis.__importTestInvoke(...a); export const convertFileSrc = (p) => p;'
              : a.path === '@tauri-apps/api/event'
                ? 'export const listen = (...a) => globalThis.__importTestListen(...a);'
                : a.path === '@tauri-apps/plugin-dialog'
                  ? 'export const open = async () => null; export const save = async () => null;'
                  : a.path === '@tauri-apps/plugin-os'
                    ? "export const platform = () => 'macos';"
                    : a.path === 'react-toastify'
                      ? 'export const toast = globalThis.__importTestToast;'
                      : 'const t = (key) => key; export const useTranslation = () => ({ t });',
        }));
      },
    },
  ],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  useFileOperations,
  useTauriListeners,
  useProcessStore,
  useLibraryStore,
  IMPORT_ALREADY_RUNNING_MESSAGE,
  IMPORT_NOT_READY_MESSAGE,
  IMPORT_NO_FILES_MESSAGE,
  IMPORT_NO_DESTINATION_MESSAGE,
  IMPORT_UNKNOWN_RESULT_MESSAGE,
} = mod.exports;

const unhandled = [];
process.on('unhandledRejection', (reason) => unhandled.push(reason));

// ---- Harness ---------------------------------------------------------------
const RUN_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const FOREIGN_ID = '0d6f1c52-7a3e-4b8f-9c21-5e4d3b2a1f00';
const settings = { filenameTemplate: '{original_filename}', organizeByDate: false };

async function mount(render, { strict = false } = {}) {
  let value, root;
  function Probe() {
    value = render();
    return null;
  }
  const element = strict
    ? React.createElement(React.StrictMode, null, React.createElement(Probe))
    : React.createElement(Probe);
  await act(async () => {
    root = create(element);
  });
  return {
    get value() {
      return value;
    },
    async update(nextRender) {
      render = nextRender;
      await act(async () => root.update(React.cloneElement(element)));
    },
    async close() {
      await act(async () => root.unmount());
    },
  };
}

function makeRefreshSpies() {
  const calls = { trees: 0, subfolder: [] };
  return {
    calls,
    props: {
      refreshAllFolderTrees: () => {
        calls.trees += 1;
      },
      handleSelectSubfolder: (path, isNewRoot) => {
        calls.subfolder.push([path, isNewRoot]);
      },
      refreshImageList: () => {},
      markGenerated: () => {},
    },
  };
}

async function mountListeners(props, options) {
  return mount(() => useTauriListeners(props), options);
}

async function mountFileOps() {
  const noop = async () => {};
  return mount(() =>
    useFileOperations(
      noop,
      noop,
      () => {},
      () => {},
      [],
    ),
  );
}

/** Dispatches through a registered listener; throws if none is registered. */
async function emit(name, payload) {
  const handlers = transport.handlers.get(name);
  if (!handlers || handlers.size === 0) throw new Error(`no listener registered for ${name}`);
  await act(async () => {
    for (const handler of [...handlers]) handler({ event: name, payload });
  });
}

const importState = () => useProcessStore.getState().importState;
const importInvokes = () => transport.invokeCalls.filter((c) => c.name === 'import_files');
const lastRunId = () => importInvokes().at(-1).args.runId;
const errorToasts = () => toasts.filter((t) => t.kind === 'error');

async function start(fileOps, sources = ['/a/1.RAF'], destination = '/lib') {
  await act(async () => {
    await fileOps.value.startImportFiles(sources, destination, settings);
  });
}

async function setup() {
  resetTransport();
  toasts.length = 0;
  unhandled.length = 0;
  useLibraryStore.getState().setLibrary({ currentFolderPath: '/lib' });
  await act(async () => {
    useProcessStore.setState({
      importState: { errorMessage: '', path: '', progress: { current: 0, total: 0 }, status: 'idle' },
    });
  });
  const spies = makeRefreshSpies();
  const listeners = await mountListeners(spies.props);
  const fileOps = await mountFileOps();
  return {
    spies,
    listeners,
    fileOps,
    async close() {
      await fileOps.close();
      await listeners.close();
    },
  };
}

const complete = (runId, total, imported, failed, errorMessage) => ({ runId, total, imported, failed, errorMessage });

// ---- Tests -------------------------------------------------------------------

test('failed batch ends in one Error with an actionable toast and one refresh; invoke resolution alone stays Importing', async () => {
  const h = await setup();
  await start(h.fileOps, ['/a/missing.RAF']);

  assert.equal(importInvokes().length, 1);
  const { args } = importInvokes()[0];
  assert.match(args.runId, RUN_ID_RE);
  assert.deepEqual(
    { sourcePaths: args.sourcePaths, destinationFolder: args.destinationFolder, settings: args.settings },
    { sourcePaths: ['/a/missing.RAF'], destinationFolder: '/lib', settings },
  );
  // The invoke promise has resolved: that only means "queued".
  assert.equal(importState().status, 'importing');

  const runId = args.runId;
  await emit('import-start', { runId, total: 1 });
  await emit('import-progress', { runId, total: 1, current: 0, path: 'missing.RAF' });
  await emit('import-error', {
    runId,
    total: 1,
    failed: 1,
    errorMessage: 'missing.RAF: Source file not found: missing.RAF',
  });
  assert.equal(importState().status, 'importing');
  assert.equal(importState().errorMessage, 'missing.RAF: Source file not found: missing.RAF');
  assert.equal(errorToasts().length, 0, 'item errors do not toast');

  await emit('import-progress', { runId, total: 1, current: 1, path: '' });
  const summary = '0 of 1 imported; 1 failed: missing.RAF: Source file not found: missing.RAF';
  await emit('import-complete', complete(runId, 1, 0, 1, summary));

  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, summary);
  assert.deepEqual(importState().progress, { current: 1, total: 1 });
  assert.deepEqual(errorToasts(), [{ kind: 'error', message: summary, toastId: `import-${runId}` }]);
  assert.equal(h.spies.calls.trees, 1);
  assert.deepEqual(h.spies.calls.subfolder, [['/lib', false]]);
  await h.close();
});

test('terminal table: all-good is the only Success; mixed and all-failed are Error', async () => {
  const rows = [
    { name: 'all good', total: 2, imported: 2, failed: 0, message: '', status: 'success', toast: false },
    {
      name: 'mixed',
      total: 2,
      imported: 1,
      failed: 1,
      message: '1 of 2 imported; 1 failed: b.RAF: Source file not found: b.RAF',
      status: 'error',
      toast: true,
    },
    {
      name: 'all failed',
      total: 2,
      imported: 0,
      failed: 2,
      message: '0 of 2 imported; 2 failed: a.RAF: x; b.RAF: y',
      status: 'error',
      toast: true,
    },
  ];
  for (const row of rows) {
    const h = await setup();
    await start(h.fileOps, ['/a/a.RAF', '/a/b.RAF']);
    const runId = lastRunId();
    await emit('import-start', { runId, total: row.total });
    await emit('import-complete', complete(runId, row.total, row.imported, row.failed, row.message));
    assert.equal(importState().status, row.status, row.name);
    assert.equal(importState().errorMessage, row.message, row.name);
    assert.equal(errorToasts().length, row.toast ? 1 : 0, row.name);
    assert.equal(h.spies.calls.trees, 1, `${row.name}: refresh once, including all-failed`);
    await h.close();
  }
});

test('malformed or unattributable terminals never produce Success', async () => {
  const oversize = 'é'.repeat(1025);
  // [label, payload builder, expectation]: 'ignored' keeps Importing; 'failClosed' retires as unknown result.
  const rows = [
    ['null payload', () => null, 'ignored'],
    ['unit payload', () => undefined, 'ignored'],
    ['string payload', () => 'done', 'ignored'],
    ['array payload', () => [], 'ignored'],
    ['missing runId', () => ({ total: 1, imported: 1, failed: 0, errorMessage: '' }), 'ignored'],
    ['non-canonical runId', (id) => ({ ...complete(id.toUpperCase(), 1, 1, 0, '') }), 'ignored'],
    ['foreign runId', () => complete(FOREIGN_ID, 1, 1, 0, ''), 'ignored'],
    ['nonplain matching object', (id) => Object.assign(new Date(0), complete(id, 1, 1, 0, '')), 'failClosed'],
    ['missing imported', (id) => ({ runId: id, total: 1, failed: 0, errorMessage: '' }), 'failClosed'],
    ['unsafe count', (id) => complete(id, 1, Number.MAX_SAFE_INTEGER + 1, 0, ''), 'failClosed'],
    ['zero counts', (id) => complete(id, 0, 0, 0, ''), 'failClosed'],
    ['total differs from reservation', (id) => complete(id, 2, 2, 0, ''), 'failClosed'],
    ['counts do not add up', (id) => complete(id, 1, 1, 1, 'x'), 'failClosed'],
    ['NaN', (id) => complete(id, 1, Number.NaN, 0, ''), 'failClosed'],
    ['Infinity', (id) => complete(id, 1, Infinity, 0, ''), 'failClosed'],
    ['fraction', (id) => complete(id, 1, 0.5, 0.5, ''), 'failClosed'],
    ['negative', (id) => complete(id, 1, 2, -1, ''), 'failClosed'],
    ['string count', (id) => complete(id, '1', 1, 0, ''), 'failClosed'],
    ['non-string message', (id) => complete(id, 1, 0, 1, 42), 'failClosed'],
    ['oversize message', (id) => complete(id, 1, 0, 1, oversize), 'failClosed'],
    ['blank message with failure', (id) => complete(id, 1, 0, 1, '   '), 'failClosed'],
    ['failed 0 with message', (id) => complete(id, 1, 1, 0, 'unexpected'), 'failClosed'],
  ];
  for (const [label, build, expectation] of rows) {
    const h = await setup();
    await start(h.fileOps);
    const runId = lastRunId();
    await emit('import-complete', build(runId));
    if (expectation === 'ignored') {
      assert.equal(importState().status, 'importing', label);
      assert.equal(h.spies.calls.trees, 0, label);
      // A valid terminal still completes the run afterwards.
      await emit('import-complete', complete(runId, 1, 1, 0, ''));
      assert.equal(importState().status, 'success', label);
    } else {
      assert.equal(importState().status, 'error', label);
      assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE, label);
      assert.equal(errorToasts().length, 1, label);
      assert.equal(h.spies.calls.trees, 1, `${label}: conservative refresh once`);
      // The retired run cannot be completed afterwards.
      await emit('import-complete', complete(runId, 1, 1, 0, ''));
      assert.equal(importState().status, 'error', label);
      assert.equal(h.spies.calls.trees, 1, label);
    }
    await h.close();
  }

  // Failed count below a previously reported cumulative item error.
  const h = await setup();
  await start(h.fileOps, ['/a/1.RAF', '/a/2.RAF']);
  const runId = lastRunId();
  await emit('import-error', { runId, total: 2, failed: 2, errorMessage: 'b: x' });
  await emit('import-complete', complete(runId, 2, 1, 1, '1 of 2 imported; 1 failed: a: x'));
  assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE);
  await h.close();

  // The largest valid message (1,024 Unicode scalars, astral characters) is accepted unchanged.
  const h2 = await setup();
  await start(h2.fileOps);
  const longest = '📷'.repeat(1024);
  await emit('import-complete', complete(lastRunId(), 1, 0, 1, longest));
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, longest);
  await h2.close();
});

test('malformed matching item events fail closed once; progress never regresses', async () => {
  const h = await setup();
  await start(h.fileOps, ['/a/1.RAF', '/a/2.RAF', '/a/3.RAF']);
  const runId = lastRunId();
  await emit('import-progress', { runId, total: 3, current: 2, path: '2.RAF' });
  await emit('import-progress', { runId, total: 3, current: 1, path: '1.RAF' });
  assert.deepEqual(importState().progress, { current: 2, total: 3 });
  assert.equal(importState().path, '2.RAF');
  await emit('import-start', { runId, total: 3 });
  assert.deepEqual(importState().progress, { current: 2, total: 3 }, 'start never resets an observed run');

  await emit('import-progress', { runId, total: 3, current: 4, path: 'x' });
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE);
  assert.equal(h.spies.calls.trees, 1);
  await emit('import-progress', { runId, total: 3, current: 3, path: '' });
  await emit('import-complete', complete(runId, 3, 3, 0, ''));
  assert.equal(importState().status, 'error', 'retired run cannot reacquire');
  assert.equal(errorToasts().length, 1);
  assert.equal(h.spies.calls.trees, 1);
  await h.close();
});

test('overlapping requests from two mounted callers never invoke twice or clobber the active run', async () => {
  const h = await setup();
  const second = await mountFileOps();
  const deferred = [];
  transport.invokeImpl = () => new Promise((resolve, reject) => deferred.push({ resolve, reject }));

  let pendingA;
  await act(async () => {
    pendingA = h.fileOps.value.startImportFiles(['/a/1.RAF'], '/lib', settings);
  });
  const runA = lastRunId();
  await emit('import-progress', { runId: runA, total: 1, current: 0, path: '1.RAF' });
  const before = { ...importState() };

  await start(second, ['/b/2.RAF']);
  assert.equal(importInvokes().length, 1, 'no second native invoke');
  assert.deepEqual(importState(), before, 'incumbent state unchanged');
  assert.deepEqual(toasts.at(-1), {
    kind: 'error',
    message: IMPORT_ALREADY_RUNNING_MESSAGE,
    toastId: 'import-already-running',
  });

  // Invalid local requests during a run also leave it alone.
  await start(second, [], '/lib');
  await start(second, ['/b/2.RAF'], '  ');
  assert.equal(importInvokes().length, 1);
  assert.deepEqual(importState(), before);

  await emit('import-complete', complete(runA, 1, 1, 0, ''));
  assert.equal(importState().status, 'success');

  // B is admitted after A's terminal; then A's delayed rejection must not affect B.
  let pendingB;
  await act(async () => {
    pendingB = second.value.startImportFiles(['/b/2.RAF'], '/lib', settings);
  });
  const runB = lastRunId();
  assert.notEqual(runB, runA);
  await act(async () => {
    deferred[0].reject('late failure from A');
    await pendingA;
  });
  assert.equal(importState().status, 'importing');
  assert.equal(importState().errorMessage, '');

  // B's own native rejection (e.g. backend busy) retires only B.
  await act(async () => {
    deferred[1].reject('An import is already running.');
    await pendingB;
  });
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, 'Failed to start import: An import is already running.');
  await emit('import-complete', complete(runB, 1, 1, 0, ''));
  assert.equal(importState().status, 'error', 'a retired token cannot be completed');

  await second.close();
  await h.close();
});

test('a terminal received before the invoke settles stays terminal', async () => {
  const h = await setup();
  const deferred = [];
  transport.invokeImpl = () => new Promise((resolve, reject) => deferred.push({ resolve, reject }));
  let pending;
  await act(async () => {
    pending = h.fileOps.value.startImportFiles(['/a/1.RAF'], '/lib', settings);
  });
  const runId = lastRunId();
  await emit('import-complete', complete(runId, 1, 1, 0, ''));
  assert.equal(importState().status, 'success');
  await act(async () => {
    deferred[0].reject('rejected after completion');
    await pending;
  });
  assert.equal(importState().status, 'success');
  assert.equal(errorToasts().length, 0);
  await h.close();
});

test('duplicate and late events from a finished run cannot alter the next run', async () => {
  const h = await setup();
  await start(h.fileOps);
  const runA = lastRunId();
  await emit('import-error', { runId: runA, total: 1, failed: 1, errorMessage: 'a: x' });
  await emit('import-complete', complete(runA, 1, 0, 1, '0 of 1 imported; 1 failed: a: x'));
  await emit('import-complete', complete(runA, 1, 0, 1, '0 of 1 imported; 1 failed: a: x'));
  assert.equal(errorToasts().length, 1, 'duplicate terminal toasts once');
  assert.equal(h.spies.calls.trees, 1, 'duplicate terminal refreshes once');

  await start(h.fileOps, ['/b/1.RAF', '/b/2.RAF']);
  const runB = lastRunId();
  await emit('import-progress', { runId: runB, total: 2, current: 1, path: '1.RAF' });
  for (const [name, payload] of [
    ['import-start', { runId: runA, total: 1 }],
    ['import-progress', { runId: runA, total: 1, current: 0, path: 'late' }],
    ['import-error', { runId: runA, total: 1, failed: 1, errorMessage: 'late' }],
    ['import-complete', complete(runA, 1, 1, 0, '')],
  ]) {
    await emit(name, payload);
  }
  assert.equal(importState().status, 'importing');
  assert.equal(importState().path, '1.RAF');
  assert.deepEqual(importState().progress, { current: 1, total: 2 });

  await emit('import-complete', complete(runB, 2, 2, 0, ''));
  assert.equal(importState().status, 'success');
  await emit('import-progress', { runId: runB, total: 2, current: 0, path: 'after' });
  assert.equal(importState().path, '', 'late progress after terminal is inert');
  assert.equal(h.spies.calls.trees, 2);
  await h.close();
});

test('item errors keep a long run Importing past the reset timer; the final timer cannot reset the next run', async () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const h = await setup();
    await start(h.fileOps, ['/a/1.RAF', '/a/2.RAF']);
    const runA = lastRunId();
    await emit('import-error', { runId: runA, total: 2, failed: 1, errorMessage: '1.RAF: x' });
    await act(async () => mock.timers.tick(6000));
    assert.equal(importState().status, 'importing');
    assert.equal(importState().errorMessage, '1.RAF: x');

    await emit('import-complete', complete(runA, 2, 1, 1, '1 of 2 imported; 1 failed: 1.RAF: x'));
    assert.equal(importState().status, 'error');
    await act(async () => mock.timers.tick(4000));
    await start(h.fileOps, ['/b/1.RAF']);
    await act(async () => mock.timers.tick(2000));
    assert.equal(importState().status, 'importing', 'old terminal timer was cancelled by the new run');
    await emit('import-complete', complete(lastRunId(), 1, 1, 0, ''));
    await act(async () => mock.timers.tick(5000));
    assert.equal(importState().status, 'idle', 'presentation reset still happens after a terminal');
    await h.close();
  } finally {
    mock.timers.reset();
  }
});

test('imports are refused until all four listeners register; a failed registration disposes every handle', async () => {
  resetTransport();
  toasts.length = 0;
  unhandled.length = 0;
  transport.listenMode = 'deferred';
  const spies = makeRefreshSpies();
  const listeners = await mountListeners(spies.props);
  const fileOps = await mountFileOps();
  const imports = transport.pendingListens.filter((p) => p.name.startsWith('import-'));
  assert.equal(imports.length, 4);

  await start(fileOps);
  assert.equal(importInvokes().length, 0, 'no invoke before readiness');
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, IMPORT_NOT_READY_MESSAGE);

  await act(async () => imports.slice(0, 3).forEach((p) => p.resolve()));
  await start(fileOps);
  assert.equal(importInvokes().length, 0, 'three of four is not ready');

  await act(async () => imports[3].resolve());
  await start(fileOps);
  assert.equal(importInvokes().length, 1, 'admitted once all four are registered');
  await emit('import-complete', complete(lastRunId(), 1, 1, 0, ''));
  await fileOps.close();
  await listeners.close();

  // Second mount: one registration rejects, the others resolve late.
  resetTransport();
  transport.listenMode = 'deferred';
  const listeners2 = await mountListeners(spies.props);
  const fileOps2 = await mountFileOps();
  const imports2 = transport.pendingListens.filter((p) => p.name.startsWith('import-'));
  await act(async () => imports2[0].resolve());
  await act(async () => imports2[1].reject(new Error('registration failed')));
  await act(async () => [2, 3].forEach((i) => imports2[i].resolve()));
  for (const name of ['import-start', 'import-progress', 'import-error', 'import-complete']) {
    assert.equal(transport.handlers.get(name)?.size ?? 0, 0, `${name} handle disposed`);
  }
  await start(fileOps2);
  assert.equal(importInvokes().length, 0, 'no admission after a failed registration');
  assert.equal(unhandled.length, 0, 'no unhandled rejection');
  await fileOps2.close();
  await listeners2.close();
});

test('unmount retires an active run as unknown, StrictMode remount works, and an old cleanup cannot invalidate a newer generation', async () => {
  const h = await setup();
  await start(h.fileOps);
  const runId = lastRunId();
  await h.listeners.close();
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE);
  assert.equal(transport.handlers.get('import-complete')?.size ?? 0, 0, 'old callbacks removed');

  // A new generation cannot revive the lost run.
  const spies = makeRefreshSpies();
  const strict = await mountListeners(spies.props, { strict: true });
  await emit('import-complete', complete(runId, 1, 1, 0, ''));
  assert.equal(importState().status, 'error');
  assert.equal(transport.handlers.get('import-complete').size, 1, 'StrictMode leaves exactly one live listener');

  await start(h.fileOps);
  const strictRun = lastRunId();
  await emit('import-complete', complete(strictRun, 1, 1, 0, ''));
  assert.equal(importState().status, 'success');
  assert.equal(spies.calls.trees, 1);

  // Two listener mounts: closing the older one leaves the newer generation ready.
  const newer = await mountListeners(makeRefreshSpies().props);
  await strict.close();
  await start(h.fileOps);
  assert.equal(importInvokes().length, 3);
  await emit('import-complete', complete(lastRunId(), 1, 1, 0, ''));
  assert.equal(importState().status, 'success');
  await newer.close();
  await h.fileOps.close();
});

test('refresh failures do not change terminal accounting or duplicate the toast', async () => {
  resetTransport();
  toasts.length = 0;
  unhandled.length = 0;
  useLibraryStore.getState().setLibrary({ currentFolderPath: '/lib' });
  const listeners = await mountListeners({
    refreshAllFolderTrees: () => {
      throw new Error('tree refresh threw');
    },
    handleSelectSubfolder: () => Promise.reject(new Error('subfolder refresh rejected')),
    refreshImageList: () => {},
    markGenerated: () => {},
  });
  const fileOps = await mountFileOps();
  const errors = [];
  const originalError = console.error;
  console.error = (...args) => errors.push(args);
  try {
    await start(fileOps);
    const runId = lastRunId();
    await emit('import-complete', complete(runId, 1, 0, 1, '0 of 1 imported; 1 failed: 1.RAF: x'));
    await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
    assert.equal(importState().status, 'error');
    assert.equal(importState().errorMessage, '0 of 1 imported; 1 failed: 1.RAF: x');
    assert.deepEqual(importState().progress, { current: 1, total: 1 });
    assert.equal(errorToasts().length, 1);
    assert.equal(unhandled.length, 0);
    assert.equal(errors.filter((e) => String(e[0]).includes('Folder refresh after import failed')).length, 2);
  } finally {
    console.error = originalError;
  }
  await fileOps.close();
  await listeners.close();
});

test('invalid local requests are explicit errors when idle and never invoke', async () => {
  const h = await setup();
  await start(h.fileOps, [], '/lib');
  assert.equal(importState().status, 'error');
  assert.equal(importState().errorMessage, IMPORT_NO_FILES_MESSAGE);
  await start(h.fileOps, ['/a/1.RAF'], '');
  assert.equal(importState().errorMessage, IMPORT_NO_DESTINATION_MESSAGE);
  assert.equal(importInvokes().length, 0);
  await h.close();
});

test('replacing listeners during an active run retires it; old callbacks and old invocation rejection stay inert', async () => {
  const h = await setup();
  let rejectA, pendingA;
  transport.invokeImpl = () =>
    new Promise((_, reject) => {
      rejectA = reject;
    });
  await act(async () => {
    pendingA = h.fileOps.value.startImportFiles(['/a.RAF'], '/lib', settings);
  });
  const runA = lastRunId();
  const oldCallbacks = transport.registrations.filter(({ name }) => name.startsWith('import-'));
  const newerSpies = makeRefreshSpies();
  const newer = await mountListeners(newerSpies.props);
  assert.equal(importState().status, 'error', 'superseded active generation must retire as unknown');
  assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE);
  assert.equal(errorToasts().length, 1);
  assert.equal(h.spies.calls.trees + newerSpies.calls.trees, 0, 'listener loss is not a verified terminal');

  transport.invokeImpl = () => Promise.resolve();
  await start(h.fileOps);
  const runB = lastRunId();
  assert.notEqual(runB, runA);
  const before = { ...importState() };
  await act(async () => {
    for (const { name, handler } of oldCallbacks) {
      handler({ payload: name === 'import-complete' ? complete(runB, 1, 1, 0, '') : { runId: runB } });
    }
    rejectA('old generation rejected');
    await pendingA;
  });
  assert.deepEqual(importState(), before, 'old callbacks cannot act even using the new run ID');
  await h.listeners.close();
  assert.deepEqual(importState(), before, 'old cleanup cannot retire the new generation');
  await emit('import-complete', complete(runB, 1, 1, 0, ''));
  assert.equal(importState().status, 'success');
  assert.equal(newerSpies.calls.trees, 1);
  await newer.close();
  await h.fileOps.close();
});

test('unmount before deferred registration disposes late handles and cannot ready a newer generation', async () => {
  resetTransport();
  toasts.length = 0;
  transport.listenMode = 'deferred';
  const old = await mountListeners(makeRefreshSpies().props);
  const oldPending = [...transport.pendingListens];
  await old.close();
  transport.pendingListens = [];
  const currentSpies = makeRefreshSpies();
  const current = await mountListeners(currentSpies.props);
  const currentPending = [...transport.pendingListens];
  const fileOps = await mountFileOps();
  await act(async () => oldPending.forEach((p) => p.resolve()));
  await start(fileOps);
  assert.equal(importInvokes().length, 0, 'late old registration cannot mark the new generation ready');
  assert.equal(importState().errorMessage, IMPORT_NOT_READY_MESSAGE);
  for (const name of ['import-start', 'import-progress', 'import-error', 'import-complete']) {
    assert.equal(transport.handlers.get(name)?.size ?? 0, 0);
    assert.equal(transport.unlistenCalls.filter((n) => n === name).length, 1);
  }
  await act(async () => currentPending.forEach((p) => p.resolve()));
  await start(fileOps);
  await emit('import-complete', complete(lastRunId(), 1, 1, 0, ''));
  assert.equal(currentSpies.calls.trees, 1);
  await fileOps.close();
  await current.close();
  assert.equal(
    [...transport.handlers.values()].reduce((n, handlers) => n + handlers.size, 0),
    0,
  );
});

test('import unlisten throws/rejections are isolated and do not prevent retirement or other handle cleanup', async () => {
  const h = await setup();
  await start(h.fileOps);
  const oldCallbacks = transport.registrations.filter(({ name }) => name.startsWith('import-'));
  const errors = [];
  const originalError = console.error;
  console.error = (...args) => errors.push(args);
  transport.unlistenImpl = (name) => {
    if (name === 'import-start') throw new Error('unlisten threw');
    if (name === 'import-progress') return Promise.reject(new Error('unlisten rejected'));
  };
  try {
    await h.listeners.close();
    await act(async () => {
      await Promise.resolve();
    });
    assert.equal(importState().status, 'error');
    assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE);
    for (const name of ['import-start', 'import-progress', 'import-error', 'import-complete']) {
      assert.equal(transport.unlistenCalls.filter((n) => n === name).length, 1);
    }
    await act(async () =>
      oldCallbacks.forEach(({ handler }) => handler({ payload: complete(lastRunId(), 1, 1, 0, '') })),
    );
    assert.equal(importState().status, 'error');
    assert.equal(errorToasts().length, 1);
    assert.equal(unhandled.length, 0);
    assert.equal(errors.filter((e) => String(e[0]).includes('import listener')).length, 2);
  } finally {
    console.error = originalError;
    transport.unlistenImpl = () => {};
    await h.fileOps.close();
  }
});

test('terminal effects use latest callback refs and retire before a refresh starts another run', async () => {
  const h = await setup();
  await start(h.fileOps);
  const runA = lastRunId();
  const latest = makeRefreshSpies();
  let pendingB;
  await h.listeners.update(() =>
    useTauriListeners({
      ...latest.props,
      refreshAllFolderTrees: () => {
        latest.calls.trees += 1;
        pendingB = h.fileOps.value.startImportFiles(['/b.RAF'], '/next', settings);
      },
    }),
  );
  await emit('import-complete', complete(runA, 1, 1, 0, ''));
  await act(async () => {
    await pendingB;
  });
  assert.equal(h.spies.calls.trees, 0, 'stale render callbacks not used');
  assert.equal(latest.calls.trees, 1);
  assert.deepEqual(latest.calls.subfolder, [['/lib', false]]);
  assert.equal(importInvokes().length, 2, 'terminal retired before refresh callback requested next run');
  assert.equal(importState().status, 'importing');
  assert.notEqual(lastRunId(), runA);
  await h.listeners.update(() => useTauriListeners(latest.props));
  await emit('import-complete', complete(lastRunId(), 1, 1, 0, ''));
  assert.equal(importState().status, 'success');
  assert.equal(latest.calls.trees, 2);
  await h.close();
});

test('matching malformed start/progress/item-error table fails closed; valid boundaries and duplicate errors remain safe', async () => {
  const rows = [
    ['import-start', { total: 0 }],
    ['import-start', { total: Number.MAX_SAFE_INTEGER + 1 }],
    ['import-progress', { current: -1, path: 'x' }],
    ['import-progress', { current: 0.5, path: 'x' }],
    ['import-progress', { current: NaN, path: 'x' }],
    ['import-progress', { current: Infinity, path: 'x' }],
    ['import-progress', { current: 0, path: null }],
    ['import-progress', { current: 0, path: '📷'.repeat(257) }],
    ['import-error', { failed: 0, errorMessage: 'x' }],
    ['import-error', { failed: 2, errorMessage: 'x' }],
    ['import-error', { failed: -1, errorMessage: 'x' }],
    ['import-error', { failed: 0.5, errorMessage: 'x' }],
    ['import-error', { failed: NaN, errorMessage: 'x' }],
    ['import-error', { failed: Infinity, errorMessage: 'x' }],
    ['import-error', { failed: 1, errorMessage: '' }],
    ['import-error', { failed: 1, errorMessage: ' \t\n' }],
    ['import-error', { failed: 1, errorMessage: null }],
    ['import-error', { failed: 1, errorMessage: '📷'.repeat(1025) }],
  ];
  for (const [name, fields] of rows) {
    const h = await setup();
    await start(h.fileOps);
    const id = lastRunId();
    await emit(name, { runId: id, total: 1, ...fields });
    assert.equal(importState().errorMessage, IMPORT_UNKNOWN_RESULT_MESSAGE, `${name} ${JSON.stringify(fields)}`);
    assert.equal(h.spies.calls.trees, 1);
    await emit(name, { runId: id, total: 1, ...fields });
    assert.equal(errorToasts().length, 1);
    assert.equal(h.spies.calls.trees, 1);
    await h.close();
  }
  const h = await setup();
  await start(h.fileOps, ['/a.RAF', '/b.RAF']);
  const id = lastRunId();
  await emit('import-progress', { runId: id, total: 2, current: 1, path: '📷'.repeat(256) });
  assert.equal(importState().path, '📷'.repeat(256));
  await emit('import-error', { runId: id, total: 2, failed: 2, errorMessage: 'both failed' });
  await emit('import-error', { runId: id, total: 2, failed: 1, errorMessage: 'earlier failure' });
  await emit('import-error', { runId: id, total: 2, failed: 2, errorMessage: 'both failed' });
  await emit('import-start', { runId: id, total: 2 });
  assert.equal(importState().status, 'importing');
  assert.equal(importState().errorMessage, 'both failed');
  assert.equal(errorToasts().length, 0);
  await emit('import-complete', complete(id, 2, 0, 2, '0 of 2 imported; 2 failed: both failed'));
  assert.equal(importState().status, 'error');
  assert.equal(h.spies.calls.trees, 1);
  await h.close();
});

test('invalid source entries are rejected locally rather than sending malformed native arguments', async () => {
  const h = await setup();
  for (const sources of [null, 'a.RAF', [null], [42], [''], ['  '], Array(1)]) {
    await start(h.fileOps, sources);
    assert.equal(importState().status, 'error');
    assert.equal(importInvokes().length, 0);
    assert.ok(importState().errorMessage.trim());
  }
  await h.close();
});
