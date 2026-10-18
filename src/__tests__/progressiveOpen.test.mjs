import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const bundle = await build({
  stdin: {
    contents: `export * from './src/utils/progressiveOpen';
      export { useEditorStore } from './src/store/useEditorStore';
      export { INITIAL_ADJUSTMENTS } from './src/utils/adjustments';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  acquireOpenFlight,
  clampEditorTransformForProgressiveFit,
  INITIAL_ADJUSTMENTS,
  isCurrentOpenKey,
  parseOpenKey,
  sameOpenKey,
  parseSourceRenderMode,
  sourceRenderPolicy,
  serializeOpenAdmission,
  useEditorStore,
  validatePlaceholderReady,
} = mod.exports;

const ticket = 'd8696d0c-a4c7-4fc8-bb2c-b442199e5771';
const key = { ticket, generation: '18446744073709551615' };

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function resetEditorStore() {
  useEditorStore.setState({
    selectedImage: null,
    progressiveOpen: null,
    pendingOpenIntent: null,
    adjustments: INITIAL_ADJUSTMENTS,
    history: [INITIAL_ADJUSTMENTS],
    historyIndex: 0,
    finalPreviewUrl: null,
    uncroppedAdjustedPreviewUrl: null,
    originalSize: { width: 0, height: 0 },
    previewSize: { width: 0, height: 0 },
  });
}

test('open keys are closed, canonical, full-string and u64-bounded without Number conversion', () => {
  assert.deepEqual(parseOpenKey(key), key);
  assert.equal(parseOpenKey({ ...key, extra: true }), null);
  assert.equal(parseOpenKey({ ...key, generation: '01' }), null);
  assert.equal(parseOpenKey({ ...key, generation: '18446744073709551616' }), null);
  assert.equal(parseOpenKey({ ...key, generation: '0\n' }), null);
  assert.equal(parseOpenKey({ ...key, ticket: ticket.toUpperCase() }), null);
  assert.equal(parseOpenKey({ ...key, ticket: `${ticket}\n` }), null);
  assert.equal(parseOpenKey({ ticket, generation: 9007199254740992 }), null);
  assert.equal(sameOpenKey(key, { ...key }), true);
  assert.equal(sameOpenKey(key, { ...key, generation: '1' }), false);
});

test('placeholder schema rejects unknown fields, malformed JPEG bytes and configured-fit or payload caps', () => {
  const valid = {
    schemaVersion: 1,
    openKey: { ticket, generation: '7' },
    width: 320,
    height: 200,
    mime: 'image/jpeg',
    dataBase64: '/9j/2Q==',
  };
  assert.deepEqual(validatePlaceholderReady(valid, 640), valid);
  assert.equal(validatePlaceholderReady({ ...valid, mystery: 1 }, 640), null);
  assert.equal(validatePlaceholderReady({ ...valid, mime: 'image/png' }, 640), null);
  assert.equal(validatePlaceholderReady({ ...valid, width: 641 }, 640), null);
  assert.equal(validatePlaceholderReady({ ...valid, dataBase64: '/9j/2Q==\n' }, 640), null);
  assert.equal(validatePlaceholderReady({ ...valid, openKey: { ...valid.openKey, generation: '7\n' } }, 640), null);
  const oversized = { ...valid, dataBase64: `/${'A'.repeat(2 * 1024 * 1024)}` };
  assert.equal(validatePlaceholderReady(oversized), null);
});

test('real store admits one metadata/history baseline and final load does not reinitialize it', () => {
  resetEditorStore();
  const store = useEditorStore;
  store.getState().prepareProgressiveIntent(ticket, '/A.raf');
  assert.equal(store.getState().selectedImage, null);
  assert.equal(store.getState().pendingOpenIntent.path, '/A.raf');
  assert.equal(store.getState().activateProgressiveIntent(ticket, '/A.raf', '', null), true);
  const loaded = { ...INITIAL_ADJUSTMENTS, exposure: 1.25, aspectRatio: null, crop: null };
  assert.equal(
    store
      .getState()
      .acceptProgressiveAdmission(ticket, '/A.raf', { ticket, generation: '1' }, { adjustments: loaded }, loaded),
    true,
  );
  const historyAtAdmission = store.getState().history;
  assert.equal(historyAtAdmission.length, 1);
  assert.equal(store.getState().adjustments.exposure, 1.25);
  assert.equal(
    store.getState().completeProgressiveLoad(
      ticket,
      '/A.raf',
      { ticket, generation: '1' },
      {
        width: 4000,
        height: 3000,
        previewSize: { width: 1920, height: 1440 },
        metadata: { adjustments: { exposure: -2 } },
        exif: { camera: 'fixture' },
        is_raw: true,
        as_shot_white_balance: null,
        renderMode: 'AccurateXtrans1Encoded',
      },
    ),
    true,
  );
  assert.equal(store.getState().selectedImage.isReady, true);
  assert.equal(store.getState().progressiveOpen.phase, 'Accurate');
  assert.equal(store.getState().adjustments.exposure, 1.25);
  assert.equal(store.getState().adjustments.aspectRatio, 4 / 3);
  assert.equal(store.getState().history, historyAtAdmission);
  assert.equal(isCurrentOpenKey({ ticket, generation: '1' }, store.getState(), '/A.raf', true), true);

  const activeDenoise = { ...store.getState().adjustments, aiDenoiseEnabled: true, aiDenoiseAmount: 25 };
  store.getState().setEditor({ adjustments: activeDenoise, aiDenoiseProgress: { '/A.raf': 0.4 } });
  store.getState().setEditor({ adjustments: { ...activeDenoise, exposure: 1.5 } });
  assert.equal(store.getState().aiDenoiseProgress['/A.raf'], 0.4, 'unrelated edits preserve active progress');

  store.getState().setEditor({ aiDenoiseProgress: { '/A.raf': 0.7 } });
  store.getState().setEditor({ adjustments: { ...activeDenoise, aiDenoiseAmount: 0 } });
  assert.equal(store.getState().aiDenoiseProgress['/A.raf'], undefined, 'zero amount clears progress');

  store.getState().setEditor({ adjustments: activeDenoise, aiDenoiseProgress: { '/A.raf': 0.6 } });
  store.getState().setEditor({ adjustments: { ...activeDenoise, aiDenoiseEnabled: false } });
  assert.equal(store.getState().aiDenoiseProgress['/A.raf'], undefined, 'disabling denoise clears progress');

  store.getState().setEditor({ adjustments: activeDenoise, aiDenoiseProgress: { '/A.raf': 0.8 } });
  store.getState().prepareProgressiveIntent('new-same-path-ticket', '/A.raf');
  assert.deepEqual(store.getState().aiDenoiseProgress, {}, 'a new open owner clears same-path progress');
});

test('real store fails closed for adjustments and history until the accurate keyed load', () => {
  resetEditorStore();
  const store = useEditorStore;
  const start = store.getState();
  store.getState().prepareProgressiveIntent(ticket, '/B.raf');
  store.getState().setEditor({ adjustments: { ...start.adjustments, exposure: 5 } });
  assert.equal(store.getState().adjustments.exposure, start.adjustments.exposure);
  assert.equal(store.getState().history, start.history);
  store.getState().failProgressiveIntent(ticket);

  store.getState().prepareProgressiveIntent(ticket, '/B.raf');
  store.getState().activateProgressiveIntent(ticket, '/B.raf', '');
  const initial = { ...INITIAL_ADJUSTMENTS, exposure: 0.75 };
  store.getState().acceptProgressiveAdmission(ticket, '/B.raf', { ticket, generation: '2' }, {}, initial);
  const admitted = store.getState();
  store.getState().claimProgressivePlaceholder(ticket, '/B.raf', { ticket, generation: '2' });
  store.getState().setProgressivePlaceholder(ticket, '/B.raf', { ticket, generation: '2' }, 'blob:fit-only');
  store.getState().undo();
  store.getState().redo();
  store.getState().goToHistoryIndex(0);
  store.getState().resetHistory(INITIAL_ADJUSTMENTS);
  store.getState().setEditor({ finalPreviewUrl: 'blob:stale', uncroppedAdjustedPreviewUrl: 'stale' });
  assert.equal(store.getState().adjustments.exposure, 0.75);
  assert.equal(store.getState().history, admitted.history);
  assert.equal(store.getState().finalPreviewUrl, admitted.finalPreviewUrl);
  assert.equal(store.getState().uncroppedAdjustedPreviewUrl, admitted.uncroppedAdjustedPreviewUrl);
  assert.equal(store.getState().progressiveOpen.phase, 'Refining');
});

test('same-ticket StrictMode leases share one admission; serialized cancellation finishes before the next admission', async () => {
  const gate = deferred();
  let runs = 0;
  const canceled = [];
  const run = async (control) => {
    runs++;
    await gate.promise;
    const admitted = { ticket, generation: '3' };
    if (control.isCancelled()) {
      canceled.push(admitted);
      control.markTerminal();
      return;
    }
    control.setKey(admitted);
    control.markTerminal();
  };
  const first = acquireOpenFlight(ticket, run, (openKey) => canceled.push(openKey));
  first.release();
  const second = acquireOpenFlight(ticket, run, (openKey) => canceled.push(openKey));
  await Promise.resolve();
  assert.equal(runs, 1);
  assert.equal(second.isCancelled(), false);
  gate.resolve();
  await second.promise;
  assert.equal(canceled.length, 0);
  second.release();

  const order = [];
  const barrier = deferred();
  const admissionA = serializeOpenAdmission(async () => {
    order.push('begin-A');
    await barrier.promise;
    order.push('cancel-A');
  });
  const admissionB = serializeOpenAdmission(async () => {
    order.push('begin-B');
  });
  await Promise.resolve();
  assert.deepEqual(order, ['begin-A']);
  barrier.resolve();
  await Promise.all([admissionA, admissionB]);
  assert.deepEqual(order, ['begin-A', 'cancel-A', 'begin-B']);
});

test('the Editor transform sink production clamp forces fit while progressive output is locked', () => {
  assert.deepEqual(clampEditorTransformForProgressiveFit(true, -140, 86, 7.5), { x: 0, y: 0, scale: 1 });
  assert.deepEqual(clampEditorTransformForProgressiveFit(false, -140, 86, 7.5), { x: -140, y: 86, scale: 7.5 });
});

test('flight lease cancellation is deferred, once-only, and disposes its registered placeholder', async () => {
  const gate = deferred();
  const openKey = { ticket, generation: '86' };
  const disposed = [];
  const nativeCancelled = [];
  const confirmedCancelled = [];
  let finish;
  const run = async (control) => {
    control.setKey(openKey);
    control.registerPlaceholderCleanup(() => disposed.push('owned-url'));
    await gate.promise;
    finish = () => control.markTerminal();
  };
  const first = acquireOpenFlight(
    ticket,
    run,
    (key) => nativeCancelled.push(key),
    (key) => confirmedCancelled.push(key),
  );
  await Promise.resolve();
  const second = acquireOpenFlight(
    ticket,
    run,
    (key) => nativeCancelled.push(key),
    (key) => confirmedCancelled.push(key),
  );
  first.release();
  await Promise.resolve();
  assert.deepEqual(disposed, []);
  assert.deepEqual(nativeCancelled, []);
  assert.deepEqual(confirmedCancelled, []);

  second.release();
  await Promise.resolve();
  assert.deepEqual(disposed, ['owned-url']);
  assert.deepEqual(nativeCancelled, [openKey]);
  assert.deepEqual(confirmedCancelled, [openKey]);
  gate.resolve();
  await second.promise;
  finish();
  assert.deepEqual(disposed, ['owned-url']);
  assert.deepEqual(nativeCancelled, [openKey]);
  assert.deepEqual(confirmedCancelled, [openKey]);
});

test('render-mode wire schema is closed and invalid store completions fail closed', () => {
  resetEditorStore();
  assert.equal(parseSourceRenderMode('AccurateXtrans1Encoded'), 'AccurateXtrans1Encoded');
  assert.equal(parseSourceRenderMode('Legacy'), 'Legacy');
  for (const invalid of [undefined, null, 1, {}, 'legacy', 'AccurateXtrans1Encoded\n', 'Preview']) {
    assert.equal(parseSourceRenderMode(invalid), null);
  }

  const store = useEditorStore;
  store.getState().prepareProgressiveIntent(ticket, '/mode.raf');
  store.getState().activateProgressiveIntent(ticket, '/mode.raf', 'thumb');
  store
    .getState()
    .acceptProgressiveAdmission(ticket, '/mode.raf', { ticket, generation: '77' }, {}, INITIAL_ADJUSTMENTS);
  const before = store.getState();
  const result = {
    width: 4000,
    height: 3000,
    previewSize: { width: 1920, height: 1440 },
    exif: null,
    is_raw: true,
    as_shot_white_balance: null,
  };
  for (const invalid of [undefined, null, 'preview', { value: 'Legacy' }]) {
    assert.equal(
      store
        .getState()
        .completeProgressiveLoad(ticket, '/mode.raf', { ticket, generation: '77' }, { ...result, renderMode: invalid }),
      false,
    );
    assert.equal(store.getState().selectedImage, before.selectedImage);
    assert.equal(store.getState().progressiveOpen, before.progressiveOpen);
    assert.equal(store.getState().history, before.history);
  }
  assert.equal(sourceRenderPolicy(before), 'locked');
});

test('Legacy handoff retires its keyed owner and preserves the one admission baseline', () => {
  resetEditorStore();
  const store = useEditorStore;
  store.getState().prepareProgressiveIntent(ticket, '/legacy.raf');
  store.getState().activateProgressiveIntent(ticket, '/legacy.raf', 'thumb');
  const initial = { ...INITIAL_ADJUSTMENTS, exposure: 0.65, aspectRatio: null, crop: null };
  const metadata = { adjustments: initial, fixture: 'kept' };
  const key = { ticket, generation: '78' };
  assert.equal(store.getState().acceptProgressiveAdmission(ticket, '/legacy.raf', key, metadata, initial), true);
  const admissionHistory = store.getState().history;
  assert.equal(
    store.getState().completeProgressiveLoad(
      ticket,
      '/legacy.raf',
      { ticket, generation: '77' },
      {
        width: 4000,
        height: 3000,
        previewSize: { width: 1920, height: 1440 },
        exif: null,
        is_raw: true,
        as_shot_white_balance: null,
        renderMode: 'Legacy',
      },
    ),
    false,
  );
  assert.equal(store.getState().progressiveOpen.phase, 'Pending');

  assert.equal(
    store.getState().completeProgressiveLoad(ticket, '/legacy.raf', key, {
      width: 4000,
      height: 3000,
      previewSize: { width: 1920, height: 1440 },
      exif: null,
      is_raw: true,
      as_shot_white_balance: null,
      renderMode: 'Legacy',
    }),
    true,
  );
  const after = store.getState();
  assert.equal(after.progressiveOpen, null);
  assert.equal(after.selectedImage.renderMode, 'Legacy');
  assert.equal(Object.hasOwn(after.selectedImage, 'openKey'), false);
  assert.equal(Object.hasOwn(after.selectedImage, 'openTicket'), false);
  assert.equal(after.selectedImage.isReady, true);
  assert.equal(after.selectedImage.metadata, metadata);
  assert.equal(after.adjustments.exposure, 0.65);
  assert.equal(after.adjustments.aspectRatio, 4 / 3);
  assert.equal(after.history, admissionHistory);
  assert.equal(after.historyIndex, 0);
  assert.equal(sourceRenderPolicy(after), 'legacy-native');
});
