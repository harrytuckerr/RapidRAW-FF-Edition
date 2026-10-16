// The real PresetsPanel import callback; only dialog, transport, translation and
// UI notification/state sinks are supplied. No picker/device/native code runs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { transform } from 'esbuild';

const source = await readFile('src/components/panel/right/PresetsPanel.tsx', 'utf8');
const start = source.indexOf('  const handleImportPresets = async () => {');
const end = source.indexOf('\n\n  const handleExport =', start);
assert.ok(start >= 0 && end > start, 'actual production callback exists');
assert.equal(source.indexOf('  const handleImportPresets = async () => {', start + 1), -1);
const { code } = await transform(source.slice(start, end), { loader: 'ts', format: 'cjs' });
const english = JSON.parse(await readFile('src/i18n/locales/en.json', 'utf8'));
const t = (key, values = {}) =>
  key
    .split('.')
    .reduce((value, part) => value?.[part], english)
    .replace(/{{(\w+)}}/g, (_, name) => String(values[name]));
const uri = 'content://synthetic.documents/document/42?grant=TEST_ONLY_PRIVATE';

function actualHandler({ selected = null, result = { imported: 1, presets: [], failures: [] }, fail } = {}) {
  const calls = [],
    notifications = [],
    refresh = [];
  const environment = {
    t,
    openDialog: async (options) => {
      calls.push(['dialog', options]);
      if (fail === 'dialog') throw new Error(`Synthetic picker rejection ${uri}`);
      return selected;
    },
    importPresetsFromFiles: async (paths) => {
      calls.push(['import', paths]);
      if (fail === 'ipc') throw new Error(`Synthetic IPC rejection ${uri}`);
      return result;
    },
    setFolderPreviewsGenerated: (value) => refresh.push(['folders', [...value]]),
    setPreviews: (value) => refresh.push(['previews', value]),
    toast: Object.fromEntries(
      ['success', 'warn', 'error', 'info'].map((kind) => [kind, (text) => notifications.push({ kind, text })]),
    ),
  };
  const run = new Function(...Object.keys(environment), `${code}; return handleImportPresets;`)(
    ...Object.values(environment),
  );
  return { run, calls, notifications, refresh };
}

test('real preset import handler treats null and empty selections as cancellation without import or refresh', async () => {
  for (const selected of [null, []]) {
    const h = actualHandler({ selected });
    await h.run();
    assert.equal(h.calls.length, 1);
    assert.deepEqual(h.refresh, []);
    assert.deepEqual(h.notifications, [{ kind: 'info', text: t('editor.presets.importOutcomeCancelled') }]);
  }
});

test('real preset import handler forwards SAF strings and arrays unchanged and refreshes on success', async () => {
  for (const selected of [uri, [uri, '/synthetic/Desktop.XMP']]) {
    const h = actualHandler({ selected, result: { imported: 2, presets: [], failures: [] } });
    await h.run();
    const options = h.calls[0][1];
    assert.equal(options.multiple, true);
    assert.deepEqual(
      options.filters.map((filter) => filter.extensions),
      [['rrpreset', 'xmp', 'lrtemplate'], ['rrpreset'], ['xmp', 'lrtemplate']],
    );
    assert.deepEqual(h.calls[1], ['import', Array.isArray(selected) ? selected : [selected]]);
    assert.deepEqual(h.refresh, [
      ['folders', []],
      ['previews', {}],
    ]);
    assert.deepEqual(h.notifications, [
      { kind: 'success', text: t('editor.presets.importOutcomeSuccess', { count: 2 }) },
    ]);
  }
});

test('real preset import handler reports partial import count and at most three native failure details', async () => {
  const failures = Array.from({ length: 4 }, (_, i) => ({
    fileName: `Item ${i + 1}`,
    error: `Item ${i + 1}: synthetic failure ${i + 1}`,
  }));
  const before = structuredClone(failures);
  const h = actualHandler({ selected: [uri], result: { imported: 3, presets: [], failures } });
  await h.run();
  assert.deepEqual(h.notifications, [
    {
      kind: 'warn',
      text: `${t('editor.presets.importOutcomePartial', { count: 3, failed: 4 })}\n${failures
        .slice(0, 3)
        .map((failure) => failure.error)
        .join('\n')}`,
    },
  ]);
  assert.ok(!h.notifications[0].text.includes('synthetic failure 4'));
  assert.deepEqual(failures, before);
  assert.deepEqual(h.refresh, [
    ['folders', []],
    ['previews', {}],
  ]);
});

test('real preset import handler distinguishes total failure from a valid file with no presets', async () => {
  const failure = {
    fileName: 'Look.xmp',
    error: 'Look.xmp: Use Camera Profile > Import Look (.xmp) with its required base DCP.',
  };
  const failed = actualHandler({ selected: uri, result: { imported: 0, presets: [], failures: [failure] } });
  await failed.run();
  assert.deepEqual(failed.notifications, [
    { kind: 'error', text: `${t('editor.presets.importOutcomeFailed', { count: 0, failed: 1 })}\n${failure.error}` },
  ]);
  const empty = actualHandler({ selected: uri, result: { imported: 0, presets: [], failures: [] } });
  await empty.run();
  assert.deepEqual(empty.notifications, [{ kind: 'info', text: t('editor.presets.importOutcomeEmpty') }]);
  assert.deepEqual(empty.refresh, [
    ['folders', []],
    ['previews', {}],
  ]);
});

test('real preset import handler hides raw picker and IPC grant errors and does not refresh on rejection', async () => {
  for (const fail of ['dialog', 'ipc']) {
    const h = actualHandler({ selected: uri, fail });
    await assert.doesNotReject(h.run());
    assert.deepEqual(h.notifications, [{ kind: 'error', text: t('editor.presets.importOutcomeError') }]);
    assert.deepEqual(h.refresh, []);
    assert.ok(!JSON.stringify(h.notifications).includes('content://'));
    assert.ok(!JSON.stringify(h.notifications).includes('TEST_ONLY_PRIVATE'));
  }
});

test('all fifteen locales define outcome text and preserve the exact count placeholders', async () => {
  const expected = {
    importOutcomeCancelled: [],
    importOutcomeSuccess: ['count'],
    importOutcomePartial: ['count', 'failed'],
    importOutcomeFailed: ['failed'],
    importOutcomeEmpty: [],
    importOutcomeError: [],
  };
  const files = (await readdir('src/i18n/locales')).filter((name) => name.endsWith('.json'));
  assert.equal(files.length, 15);
  for (const file of files) {
    const locale = JSON.parse(await readFile(`src/i18n/locales/${file}`, 'utf8'));
    for (const [key, placeholders] of Object.entries(expected)) {
      const value = locale.editor.presets[key];
      assert.equal(typeof value, 'string', `${file}:${key}`);
      assert.ok(value.trim(), `${file}:${key}`);
      assert.deepEqual(
        [...value.matchAll(/{{(\w+)}}/g)].map((match) => match[1]).sort(),
        placeholders,
        `${file}:${key}`,
      );
    }
  }
});
