// Run with npm run test:profiles after npm ci.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { build } from 'esbuild';
import React from 'react';
import ts from 'typescript';
const require = createRequire(import.meta.url);
const { create, act } = require('react-test-renderer');
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = { addEventListener() {}, removeEventListener() {} };
globalThis.window = {
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
};
let invoke = async () => [];
const previewToasts = [];
globalThis.__profileTestToast = {
  error: (message, options) => previewToasts.push({ kind: 'error', message, ...options }),
  dismiss: (id) => previewToasts.push({ kind: 'dismiss', id }),
};
globalThis.__profileTestInvoke = (...args) => invoke(...args);
const bundle = await build({
  stdin: {
    contents: `export { useProfiles } from './src/hooks/useProfiles';
      export { usePresets } from './src/hooks/usePresets';
      export { useImageProcessing, normalizePreviewResponse } from './src/hooks/useImageProcessing';
      export { useEditorActions } from './src/hooks/useEditorActions';
      export { useEditorStore } from './src/store/useEditorStore';
      export { useSettingsStore } from './src/store/useSettingsStore';
      export { useUIStore } from './src/store/useUIStore';
      export { default as Browser } from './src/components/ui/ProfileBrowser';
      export { default as Configure } from './src/components/modals/ConfigurePresetModal';
      export * from './src/utils/adjustments';`,
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
          { filter: /^(@tauri-apps\/api\/core|@tauri-apps\/plugin-os|react-toastify|react-i18next|framer-motion)$/ },
          (a) => ({
            path: a.path,
            namespace: 'mock',
          }),
        );
        b.onLoad({ filter: /.*/, namespace: 'mock' }, (a) => ({
          contents:
            a.path === 'react-toastify'
              ? 'export const toast = globalThis.__profileTestToast;'
              : a.path === '@tauri-apps/plugin-os'
                ? "export const platform = () => 'android';"
                : a.path.includes('tauri')
                  ? 'export const invoke = (...args) => globalThis.__profileTestInvoke(...args);'
                  : a.path === 'react-i18next'
                    ? 'const t = key => key; export const useTranslation = () => ({t});'
                    : `import React from 'react'; export const motion = new Proxy({}, { get: (_, tag) => tag });
       export const AnimatePresence = ({children}) => children;`,
        }));
      },
    },
  ],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  usePresets,
  normalizeProfileBaseId,
  normalizeProfileBaseInAdjustments,
  normalizePreviewResponse,
  useImageProcessing,
  useEditorActions,
  useEditorStore,
  useSettingsStore,
  useUIStore,
  useProfiles,
  Browser,
  Configure,
  INITIAL_ADJUSTMENTS,
  normalizeLoadedAdjustments,
  getVisibleAdjustmentSections,
  withAdjustmentLayout,
  COPYABLE_ADJUSTMENT_KEYS,
  ADJUSTMENT_SECTIONS,
} = mod.exports;
const profile = { base: 'camera-dcp', look: 'look-uuid', amount: 37 };
async function hook(render, props) {
  let value, root;
  function Probe(p) {
    value = render(p);
    return null;
  }
  await act(async () => {
    root = create(React.createElement(Probe, props));
  });
  return {
    get value() {
      return value;
    },
    async update(p) {
      await act(async () => root.update(React.createElement(Probe, p)));
    },
    async close() {
      await act(async () => root.unmount());
    },
  };
}

test('preset opt-in survives configure, duplicate, overwrite, folder storage and tool conversion', async () => {
  invoke = async () => [];
  const h = await hook(() => usePresets({ ...INITIAL_ADJUSTMENTS, profile }));
  let plain, opted, folder;
  await act(async () => {
    plain = h.value.addPreset('plain');
  });
  assert.equal(plain.adjustments.profile, undefined);
  await act(async () => {
    opted = h.value.configurePreset(plain.id, 'opted', false, false, 'style', true);
  });
  assert.deepEqual(opted.adjustments.profile, profile);
  await act(async () => {
    h.value.duplicatePreset(opted.id);
  });
  const duplicate = h.value.presets.find((p) => p.preset?.id !== opted.id).preset;
  assert.equal(duplicate.includeProfile, true);
  await act(async () => {
    h.value.overwritePreset(duplicate.id);
  });
  assert.deepEqual(h.value.presets.find((p) => p.preset?.id === duplicate.id).preset.adjustments.profile, profile);
  await act(async () => {
    h.value.addFolder('folder');
  });
  folder = h.value.presets.find((p) => p.folder).folder;
  await act(async () => {
    opted = h.value.addPreset('nested', folder.id, false, false, 'tool', true);
  });
  await act(async () => {
    h.value.overwritePreset(opted.id);
  });
  assert.deepEqual(
    h.value.presets.find((p) => p.folder?.id === folder.id).folder.children[0].adjustments.profile,
    profile,
  );
  await act(async () => {
    opted = h.value.configurePreset(opted.id, 'nested', false, false, 'tool', false);
  });
  assert.equal(opted.adjustments.profile, undefined);
  await h.close();
});

test('profile loading rejects stale responses and refreshes the current camera after import', async () => {
  const pending = [];
  invoke = (name, args) =>
    name === 'list_profiles_for_image'
      ? new Promise((resolve) => pending.push({ path: args.path, resolve }))
      : Promise.resolve({ imported: 1, failed: [] });
  const h = await hook((p) => useProfiles(p.path), { path: 'A' });
  await h.update({ path: 'B' });
  const model = (camera) => ({ camera, cameraProfiles: [], looks: [] });
  await act(async () => pending[1].resolve(model('B')));
  await act(async () => pending[0].resolve(model('A')));
  assert.equal(h.value.browserModel.camera, 'B');
  let importing;
  await act(async () => {
    importing = h.value.importProfiles(['new.dcp']);
  });
  assert.equal(pending[2].path, 'B');
  await act(async () => {
    pending[2].resolve(model('B-imported'));
    await importing;
  });
  assert.equal(h.value.browserModel.camera, 'B-imported');
  await h.update({ path: 'C' });
  assert.equal(h.value.browserModel, null);
  await h.update({ path: null });
  await act(async () => pending[3].resolve(model('C')));
  assert.equal(h.value.browserModel, null);
  await h.close();
});

test('unpaired Looks stay visibly disabled and cannot be selected by keyboard or click', async () => {
  let selected = 0,
    root;
  const model = {
    camera: 'Camera',
    cameraProfiles: [],
    looks: [
      {
        uuid: 'missing',
        name: 'Missing',
        group: 'Looks',
        pairing: { state: 'missing_base', required_name: 'Base', camera: 'Camera', installed_for_camera: [] },
      },
      { uuid: 'available', name: 'Available', group: 'Looks', pairing: { state: 'satisfied', base: 'dcp' } },
    ],
  };
  await act(async () => {
    root = create(
      React.createElement(Browser, {
        model,
        activeBase: null,
        activeLook: null,
        onSelectBase() {},
        onSelectLook() {
          selected++;
        },
        onClear() {},
      }),
    );
  });
  await act(async () => root.root.findByProps({ 'aria-haspopup': 'listbox' }).props.onClick());
  const missing = root.root.findByProps({ 'aria-disabled': true });
  assert.equal(missing.props.disabled, true);
  assert.match(missing.props.className, /opacity-40/);
  assert.match(missing.props['aria-label'], /requires the base profile/);
  await act(async () => missing.props.onClick());
  const container = root.root.findAll((n) => typeof n.type === 'string' && typeof n.props.onKeyDown === 'function')[0];
  await act(async () => container.props.onKeyDown({ key: 'ArrowDown', preventDefault() {} }));
  await act(async () => container.props.onKeyDown({ key: 'Enter', preventDefault() {} }));
  assert.equal(selected, 0);
  await act(async () => root.root.findByProps({ 'aria-disabled': false }).props.onClick());
  assert.equal(selected, 1);
  await act(async () => root.unmount());
});

test('normalization, section resets, copy settings and reordered layouts preserve profile boundaries', () => {
  assert.deepEqual(normalizeLoadedAdjustments({ profile }).profile, profile);
  assert.equal(normalizeLoadedAdjustments({ profile: { ...profile, amount: 0 } }).profile.amount, 0);
  assert.equal(COPYABLE_ADJUSTMENT_KEYS.includes('profile'), false);
  for (const keys of Object.values(ADJUSTMENT_SECTIONS)) assert.equal(keys.includes('profile'), false);
  assert.deepEqual(
    getVisibleAdjustmentSections({ sectionOrder: ['color', 'basic', 'color', 'obsolete'], hiddenSections: ['curves'] }),
    ['color', 'basic', 'details', 'effects'],
  );
  const settings = {
    copyPasteSettings: { includedAdjustments: ['exposure'] },
    adjustmentLayout: { hiddenSections: ['curves'] },
  };
  assert.deepEqual(
    withAdjustmentLayout(settings, { sectionOrder: ['color'] }).copyPasteSettings,
    settings.copyPasteSettings,
  );
});

test('profile checkbox is included when saving without editing another field', async () => {
  let root, saved;
  const onSave = (...args) => {
    saved = args;
  };
  await act(async () => {
    root = create(
      React.createElement(Configure, {
        isOpen: true,
        onSave,
        onClose() {},
        initialPreset: { name: 'Test', adjustments: {} },
      }),
    );
  });
  const switchNode = root.root.find(
    (n) => typeof n.type === 'function' && n.props.label === 'modals.configurePreset.includeProfile',
  );
  await act(async () => switchNode.props.onChange(true));
  const input = root.root.findByProps({ type: 'text' });
  await act(async () => input.props.onKeyDown({ key: 'Enter' }));
  assert.equal(saved[4], true);
  await act(async () => root.unmount());
});

test('zero preset intensity resets only keys present in the preset', async () => {
  // Compile the actual interpolation function without mounting the whole editor.
  const source = await readFile('src/components/panel/right/PresetsPanel.tsx', 'utf8');
  const ast = ts.createSourceFile('PresetsPanel.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const declaration = ast.statements.find(
    (node) =>
      ts.isVariableStatement(node) &&
      node.declarationList.declarations.some((d) => d.name.getText(ast) === 'mixAdjustments'),
  );
  const js = ts.transpile(declaration.getText(ast), { target: ts.ScriptTarget.ES2022 });
  const mix = new Function('INITIAL_ADJUSTMENTS', js + '; return mixAdjustments;')(INITIAL_ADJUSTMENTS);
  const mixed = mix({ exposure: 2 }, 0);
  assert.deepEqual(Object.keys(mixed), ['exposure']);
  assert.deepEqual({ ...INITIAL_ADJUSTMENTS, profile, ...mixed }.profile, profile);
});

test('explicit empty profile capture survives style-to-tool conversion', async () => {
  invoke = async () => [];
  const h = await hook(() => usePresets(INITIAL_ADJUSTMENTS));
  let preset;
  await act(async () => {
    preset = h.value.addPreset('clear profile', null, false, false, 'style', true);
  });
  await act(async () => {
    preset = h.value.configurePreset(preset.id, preset.name, false, false, 'tool', true);
  });
  assert.deepEqual(preset.adjustments.profile, INITIAL_ADJUSTMENTS.profile);
  await h.close();
});

for (const environment of [
  { name: 'coarse pointer', width: 1024, coarse: true, inline: true },
  { name: 'narrow viewport', width: 412, coarse: false, inline: true },
  { name: 'desktop keyboard', width: 1024, coarse: false, inline: false },
]) {
  test(`profile browser focus, scrolling and selection on ${environment.name}`, async () => {
    const previousWindow = globalThis.window;
    let focusCount = 0;
    let selectedBase = null;
    let selectedLook = null;
    let listener;
    let removedListener;
    const media = {
      matches: environment.width <= 767 || environment.coarse,
      addEventListener(event, callback) {
        assert.equal(event, 'change');
        listener = callback;
      },
      removeEventListener(event, callback) {
        assert.equal(event, 'change');
        removedListener = callback;
      },
    };
    globalThis.window = {
      matchMedia(query) {
        assert.equal(query, '(max-width: 767px), (pointer: coarse)');
        return media;
      },
    };
    let root;
    try {
      await act(async () => {
        root = create(
          React.createElement(Browser, {
            model: {
              camera: 'Camera',
              cameraProfiles: [{ profileName: 'Base', entries: [{ id: 'dcp', profileName: 'Base' }] }],
              looks: [
                {
                  uuid: 'missing',
                  name: 'Missing',
                  group: 'Looks',
                  pairing: {
                    state: 'missing_base',
                    required_name: 'Other Base',
                    camera: 'Camera',
                    installed_for_camera: ['Base'],
                  },
                },
                { uuid: 'ready', name: 'Ready', group: 'Looks', pairing: { state: 'satisfied', base: 'dcp' } },
              ],
            },
            activeBase: null,
            activeLook: null,
            onSelectBase(id) {
              selectedBase = id;
            },
            onSelectLook(id, base) {
              selectedLook = [id, base];
            },
            onClear() {},
          }),
          {
            createNodeMock(element) {
              if (element.type === 'input')
                return {
                  focus() {
                    focusCount++;
                  },
                };
              return null;
            },
          },
        );
      });
      const trigger = () => root.root.findByProps({ 'aria-haspopup': 'listbox' });
      const keydown = (key) =>
        root.root
          .findAll((n) => typeof n.type === 'string' && typeof n.props.onKeyDown === 'function')[0]
          .props.onKeyDown({ key, preventDefault() {}, stopPropagation() {} });
      await act(async () => {
        environment.inline ? trigger().props.onClick() : keydown('Enter');
      });
      assert.equal(focusCount, environment.inline ? 0 : 1);
      const list = root.root.findByProps({ role: 'listbox' });
      assert.equal(list.parent.props.className.split(' ').includes('absolute'), !environment.inline);
      assert.equal(list.props.className.includes('overflow-hidden'), !environment.inline);
      assert.equal(list.props.className.includes('max-h-80'), !environment.inline);
      const missing = root.root.findByProps({ 'aria-disabled': true });
      assert.equal(missing.props.disabled, true);
      assert.match(missing.props['aria-label'], /requires the base profile 'Other Base'/);
      if (environment.inline) {
        assert.ok(
          root.root.findAll((n) => typeof n.type === 'string' && n.props.children === missing.props['aria-label'])
            .length,
        );
      }
      await act(async () => missing.props.onClick());
      assert.equal(selectedLook, null);
      assert.equal(trigger().props['aria-expanded'], true);
      if (environment.inline) {
        await act(async () => root.root.findAllByProps({ role: 'option' })[0].props.onClick());
      } else {
        await act(async () => keydown('ArrowDown'));
        await act(async () => keydown('Enter'));
      }
      assert.equal(selectedBase, 'dcp');
      assert.equal(trigger().props['aria-expanded'], false);
      await act(async () => trigger().props.onClick());
      await act(async () => root.root.findAllByProps({ role: 'option' })[2].props.onClick());
      assert.deepEqual(selectedLook, ['ready', 'dcp']);
      // Responsive changes also apply while the browser is mounted.
      await act(async () => {
        media.matches = !media.matches;
        listener();
      });
      await act(async () => trigger().props.onClick());
      const resizedList = root.root.findByProps({ role: 'listbox' });
      assert.equal(resizedList.parent.props.className.split(' ').includes('absolute'), environment.inline);
    } finally {
      if (root) await act(async () => root.unmount());
      globalThis.window = previousWindow;
    }
    assert.equal(removedListener, listener);
  });
}

const legacyBase = [74, 36, 24, ...Array.from({ length: 29 }, (_, i) => i)];
const canonicalBase = legacyBase.map((byte) => byte.toString(16).padStart(2, '0')).join('');

test('strict profile IDs normalize legacy serde bytes and hex without mutating source', () => {
  assert.equal(normalizeProfileBaseId(legacyBase), canonicalBase);
  assert.equal(normalizeProfileBaseId(canonicalBase.toUpperCase()), canonicalBase);
  assert.equal(normalizeProfileBaseId(null), null);
  const loaded = { profile: { base: legacyBase, look: 'look-uuid', amount: 0 }, exposure: 2 };
  assert.deepEqual(normalizeLoadedAdjustments(loaded).profile, { base: canonicalBase, look: 'look-uuid', amount: 0 });
  assert.equal(loaded.profile.base, legacyBase);
  const partial = normalizeProfileBaseInAdjustments({ profile: loaded.profile });
  assert.deepEqual(Object.keys(partial), ['profile']);
  for (const malformed of [
    '',
    'bad',
    'g'.repeat(64),
    ' ' + canonicalBase,
    [],
    Array(31).fill(0),
    Array(33).fill(0),
    [...legacyBase.slice(0, 31), 256],
    [...legacyBase.slice(0, 31), -1],
    [...legacyBase.slice(0, 31), 0.5],
    [...legacyBase.slice(0, 31), '1'],
    Array(32),
    {},
    true,
    42,
  ]) {
    assert.throws(() => normalizeProfileBaseId(malformed), /Invalid profile.base/);
    const invalid = { profile: { base: malformed, look: 'look-uuid', amount: 100 } };
    assert.equal(normalizeProfileBaseInAdjustments(invalid), invalid);
    assert.deepEqual(normalizeLoadedAdjustments(invalid).profile.base, malformed);
  }
});

test('loaded and imported presets normalize nested legacy IDs without adding uncaptured settings', async () => {
  const fixtures = [
    { preset: { id: 'p', name: 'Legacy', adjustments: { profile: { base: legacyBase, look: 'look', amount: 100 } } } },
    {
      folder: {
        id: 'f',
        name: 'Folder',
        children: [
          { id: 'child', adjustments: { profile: { base: canonicalBase.toUpperCase(), look: null, amount: 0 } } },
        ],
      },
    },
    { preset: { id: 'invalid', adjustments: { profile: { base: [1, 2], look: 'look', amount: 100 } } } },
  ];
  invoke = async (command) =>
    command === 'handle_import_presets_from_files'
      ? { presets: structuredClone(fixtures), failures: [] }
      : structuredClone(fixtures);
  const h = await hook(() => usePresets(INITIAL_ADJUSTMENTS));
  const check = () => {
    assert.equal(h.value.presets[0].preset.adjustments.profile.base, canonicalBase);
    assert.equal(h.value.presets[1].folder.children[0].adjustments.profile.base, canonicalBase);
    assert.deepEqual(Object.keys(h.value.presets[0].preset.adjustments), ['profile']);
    assert.deepEqual(h.value.presets[2].preset.adjustments.profile.base, [1, 2]);
  };
  try {
    check();
    await act(async () => h.value.importPresetsFromFile('legacy.json'));
    check();
    await act(async () => h.value.importLegacyPresetsFromFile('legacy.xmp'));
    check();
    await act(async () => h.value.importPresetsFromFiles(['legacy.json']));
    check();
  } finally {
    await h.close();
  }
});

test('preview binary normalization handles ArrayBuffer, Tauri fallback arrays and sliced Uint8Array', () => {
  const bytes = [255, 216, 1, 255, 217];
  const buffer = new Uint8Array(bytes).buffer;
  assert.equal(normalizePreviewResponse(buffer), buffer);
  assert.deepEqual([...new Uint8Array(normalizePreviewResponse(bytes))], bytes);
  const padded = new Uint8Array([99, ...bytes, 88]);
  assert.deepEqual([...new Uint8Array(normalizePreviewResponse(padded.subarray(1, -1)))], bytes);
  for (const value of [null, {}, 'bytes', [-1], [256], [0.5], ['1'], Array(1)]) {
    assert.throws(() => normalizePreviewResponse(value), /Invalid preview response/);
  }
});

async function renderingHook(ready = false) {
  useEditorStore.getState().setEditor({
    selectedImage: { path: 'A', isReady: ready },
    adjustments: structuredClone(INITIAL_ADJUSTMENTS),
    previewOverride: null,
    showOriginal: false,
    finalPreviewUrl: 'old-preview',
    interactivePatch: null,
    isSliderDragging: false,
    patchesSentToBackend: new Set(),
  });
  useSettingsStore.getState().setAppSettings({ enableZoomHifi: false });
  useUIStore.getState().setUI({ activeView: 'editor' });
  previewToasts.length = 0;
  const refs = {
    previewJobIdRef: { current: 0 },
    latestRenderedJobIdRef: { current: 0 },
    currentResRef: { current: 0 },
  };
  const transform = { current: null },
    previous = { current: null };
  const h = await hook(() => ({ ...useImageProcessing(transform, previous, refs), ...useEditorActions() }));
  return h;
}

test('profile updates trigger IPC with canonical profile and exit original comparison', async () => {
  const requests = [];
  invoke = async (command, args) => {
    if (command === 'apply_adjustments') {
      requests.push(args);
      return [255, 216, 255, 217];
    }
    return [];
  };
  const h = await renderingHook(true);
  try {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 80));
    });
    requests.length = 0;
    await act(async () => {
      useEditorStore
        .getState()
        .setEditor({ showOriginal: true, previewOverride: structuredClone(INITIAL_ADJUSTMENTS) });
      h.value.setAdjustments((previous) => ({
        ...previous,
        profile: { base: legacyBase, look: 'look-uuid', amount: 100 },
      }));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 80));
    });
    assert.equal(requests.length, 1);
    assert.deepEqual(requests[0].jsAdjustments.profile, { base: canonicalBase, look: 'look-uuid', amount: 100 });
    assert.equal(useEditorStore.getState().showOriginal, false);
    assert.equal(useEditorStore.getState().previewOverride, null);
    assert.match(useEditorStore.getState().finalPreviewUrl, /^blob:/);
  } finally {
    await h.close();
  }
});

test('current render errors toast, superseded/stale jobs stay quiet, only latest success dismisses', async () => {
  const pending = [];
  invoke = (command, args) =>
    command === 'apply_adjustments'
      ? new Promise((resolve, reject) => pending.push({ resolve, reject, args }))
      : Promise.resolve([]);
  const h = await renderingHook();
  const adjustments = { ...INITIAL_ADJUSTMENTS, profile: { base: canonicalBase, look: 'look', amount: 100 } };
  const start = () => h.value.executeApplyAdjustments(adjustments);
  const settle = async (index, method, value, request) => {
    await act(async () => {
      pending[index][method](value);
      await request;
    });
  };
  try {
    let request = start();
    await settle(0, 'reject', 'Profile render failed', request);
    assert.deepEqual(previewToasts.at(-1), {
      kind: 'error',
      message: 'Profile render failed',
      toastId: 'preview-render-error',
    });
    assert.equal(useEditorStore.getState().finalPreviewUrl, 'old-preview');
    for (const reason of ['Preview superseded', 'Superseded or worker failed']) {
      const count = previewToasts.length,
        index = pending.length;
      request = start();
      await settle(index, 'reject', reason, request);
      assert.equal(previewToasts.length, count);
    }
    let index = pending.length;
    request = start();
    await settle(index, 'reject', 'Preview worker stopped before returning a result', request);
    assert.match(previewToasts.at(-1).message, /Preview worker stopped/);
    const oldIndex = pending.length,
      old = start(),
      newIndex = pending.length,
      newest = start();
    await settle(newIndex, 'reject', 'Newest failed', newest);
    const count = previewToasts.length;
    await settle(oldIndex, 'resolve', [255, 216, 255, 217], old);
    assert.equal(previewToasts.length, count);
    index = pending.length;
    request = start();
    await settle(index, 'resolve', [255, 216, 255, 217], request);
    assert.deepEqual(previewToasts.at(-1), { kind: 'dismiss', id: 'preview-render-error' });
    index = pending.length;
    request = start();
    await act(async () => useEditorStore.getState().setEditor({ selectedImage: { path: 'B', isReady: false } }));
    const before = previewToasts.length;
    await settle(index, 'reject', 'Old image failed', request);
    assert.equal(previewToasts.length, before);
    const finalUrl = useEditorStore.getState().finalPreviewUrl;
    await act(async () =>
      h.value.executeApplyAdjustments({ ...adjustments, profile: { ...adjustments.profile, base: [1, 2] } }),
    );
    assert.match(previewToasts.at(-1).message, /Invalid profile.base/);
    assert.equal(useEditorStore.getState().finalPreviewUrl, finalUrl);
    index = pending.length;
    request = start();
    await settle(index, 'resolve', {}, request);
    assert.match(previewToasts.at(-1).message, /Invalid preview response/);
    // A late failure must not clear the interactive patch installed by newer work.
    const staleIndex = pending.length,
      staleRequest = start();
    const latestIndex = pending.length,
      latestRequest = start();
    const patch = { url: 'newer-patch', normX: 0, normY: 0, normW: 1, normH: 1 };
    await act(async () => useEditorStore.getState().setEditor({ interactivePatch: patch }));
    const toastCount = previewToasts.length;
    await settle(staleIndex, 'reject', 'Obsolete failed', staleRequest);
    assert.equal(useEditorStore.getState().interactivePatch, patch);
    assert.equal(previewToasts.length, toastCount);
    await settle(latestIndex, 'reject', 'Preview superseded', latestRequest);
    assert.equal(useEditorStore.getState().interactivePatch, patch);
    assert.equal(previewToasts.length, toastCount);
    const switchedIndex = pending.length,
      switchedRequest = start();
    await act(async () =>
      useEditorStore.getState().setEditor({ selectedImage: { path: 'C', isReady: false }, interactivePatch: patch }),
    );
    await settle(switchedIndex, 'reject', 'Previous image failed', switchedRequest);
    assert.equal(useEditorStore.getState().interactivePatch, patch);
    assert.equal(previewToasts.length, toastCount);
  } finally {
    await h.close();
  }
});

test('interactive preview decodes ROI headers identically from binary and JSON byte replies', async () => {
  const bytes = new Uint8Array(28);
  const header = new DataView(bytes.buffer);
  [10, 20, 30, 40, 100, 200].forEach((value, i) => header.setUint32(i * 4, value, true));
  bytes.set([255, 216, 255, 217], 24);
  let response = bytes.buffer;
  invoke = async (command) => (command === 'apply_adjustments' ? response : []);
  const h = await renderingHook();
  try {
    for (const shape of [bytes.buffer, Array.from(bytes)]) {
      response = shape;
      await act(async () => h.value.executeApplyAdjustments(INITIAL_ADJUSTMENTS, true));
      const patch = useEditorStore.getState().interactivePatch;
      assert.deepEqual([patch.normX, patch.normY, patch.normW, patch.normH], [0.1, 0.1, 0.3, 0.2]);
      assert.deepEqual([...new Uint8Array(await (await fetch(patch.url)).arrayBuffer())], [255, 216, 255, 217]);
    }
    response = [];
    await act(async () => h.value.executeApplyAdjustments(INITIAL_ADJUSTMENTS));
    assert.match(previewToasts.at(-1).message, /no image bytes/);
  } finally {
    await h.close();
  }
});
