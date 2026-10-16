// Defined under a source-only grant. Later: node --test src/__tests__/applicationLifecycle.test.mjs
// Executes the actual scoped production callback bodies, transpiled with the
// existing esbuild dependency. This is functional caller routing, NOT a full
// React hook/component, Tauri IPC, OS termination or mobile-device test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { transform } from 'esbuild';

const settingsUrl = new URL('../components/panel/SettingsPanel.tsx', import.meta.url);
const externalUrl = new URL('../hooks/useExternalEditSession.ts', import.meta.url);
async function productionCallback(url, start, end, name, environment) {
  const source = await readFile(url, 'utf8');
  const begin = source.indexOf(start);
  assert.ok(begin >= 0, 'production callback must exist');
  assert.equal(source.indexOf(start, begin + start.length), -1, 'unique callback');
  const finish = source.indexOf(end, begin);
  assert.ok(finish > begin, 'production callback terminator must exist');
  const { code } = await transform(source.slice(begin, finish), { loader: 'ts', format: 'cjs' });
  return new Function(...Object.keys(environment), `${code}; return ${name};`)(...Object.values(environment));
}
function settings(environment) {
  return productionCallback(
    settingsUrl,
    '  const handleSaveAndRelaunch = async () => {',
    '\n\n  const handleProviderChange',
    'handleSaveAndRelaunch',
    environment,
  );
}
function external(environment) {
  return productionCallback(
    externalUrl,
    '      const requestExit = async () => {',
    '\n      void requestExit();',
    'requestExit',
    environment,
  );
}
const desktop = ['macos', 'windows', 'linux'];
const mobile = ['android', 'ios'];

test('real Settings callback saves then routes all five supported platforms', async () => {
  for (const osPlatform of [...desktop, ...mobile]) {
    const calls = [];
    const callback = await settings({
      osPlatform,
      appSettings: { theme: 'dark', value: 1 },
      processingSettings: { value: 2 },
      onSettingsChange: async (value) => calls.push(['save', value]),
      invoke: async (...args) => calls.push(['invoke', ...args]),
      relaunch: async () => calls.push(['plugin']),
    });
    await callback();
    const terminal = desktop.includes(osPlatform)
      ? ['invoke', 'app_restart', { request: '{"schemaVersion":1}' }]
      : ['plugin'];
    assert.deepEqual(calls, [['save', { theme: 'dark', value: 2 }], terminal]);
  }
});

test('real Settings callback fails closed on unknown platforms or rejected work', async () => {
  for (const [osPlatform, failure] of [
    ['', 'none'],
    ['unknown', 'none'],
    ['macos', 'invoke'],
    ['linux', 'save'],
    ['android', 'plugin'],
  ]) {
    const calls = [];
    const callback = await settings({
      osPlatform,
      appSettings: {},
      processingSettings: {},
      onSettingsChange: async () => {
        calls.push('save');
        if (failure === 'save') throw new Error('synthetic');
      },
      invoke: async () => {
        calls.push('invoke');
        if (failure === 'invoke') throw new Error('synthetic');
      },
      relaunch: async () => {
        calls.push('plugin');
        if (failure === 'plugin') throw new Error('synthetic');
      },
    });
    await assert.doesNotReject(callback());
    assert.deepEqual(
      calls,
      failure === 'invoke' ? ['save', 'invoke'] : failure === 'plugin' ? ['save', 'plugin'] : ['save'],
    );
  }
});

test('real external-edit callback routes exit0 without changing finishing state on success', async () => {
  for (const osPlatform of [...desktop, ...mobile]) {
    const calls = [];
    const callback = await external({
      osPlatform,
      invoke: async (...args) => calls.push(['invoke', ...args]),
      exit: async (code) => calls.push(['plugin', code]),
      setIsFinishing: (value) => calls.push(['finishing', value]),
    });
    await callback();
    assert.deepEqual(calls, [
      desktop.includes(osPlatform)
        ? ['invoke', 'app_exit', { request: '{"schemaVersion":1,"code":0}' }]
        : ['plugin', 0],
    ]);
  }
});

test('real external-edit callback handles unknown/rejected routes without fallback or unhandled rejection', async () => {
  for (const osPlatform of ['', 'other', 'windows', 'ios']) {
    const calls = [];
    const callback = await external({
      osPlatform,
      invoke: async () => {
        calls.push('invoke');
        throw new Error('synthetic');
      },
      exit: async () => {
        calls.push('plugin');
        throw new Error('synthetic');
      },
      setIsFinishing: (value) => calls.push(['finishing', value]),
    });
    await assert.doesNotReject(callback());
    assert.deepEqual(
      calls,
      osPlatform === 'windows'
        ? ['invoke', ['finishing', false]]
        : osPlatform === 'ios'
          ? ['plugin', ['finishing', false]]
          : [['finishing', false]],
    );
  }
});
