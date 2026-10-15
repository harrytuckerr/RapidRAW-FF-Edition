// Actual shared panels/hooks/stores; only native transport, dialogs and unrelated
// drag/context-menu/animation infrastructure are mocked. No renderer or app is run.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { build } from 'esbuild';
import React from 'react';
const require = createRequire(import.meta.url);
const { create, act } = require('react-test-renderer');
const english = JSON.parse(await readFile('src/i18n/locales/en.json', 'utf8'));
function translate(key, values = {}) {
  const lookup = (path) => path.split('.').reduce((value, part) => value?.[part], english);
  const value = lookup(key) ?? lookup(`${key}_${values.count === 1 ? 'one' : 'other'}`) ?? key;
  return String(value).replace(/{{(\w+)}}/g, (_, name) => String(values[name] ?? ''));
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.document = { addEventListener() {}, removeEventListener() {} };
globalThis.window = {
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  addEventListener() {},
  removeEventListener() {},
  clearTimeout,
};
let frameTime = 0;
globalThis.requestAnimationFrame = (fn) => setTimeout(() => fn((frameTime += 400)), 0);
globalThis.cancelAnimationFrame = clearTimeout;
globalThis.__ui1Translate = translate;
let dialogResult = null,
  dialogError = null,
  imported = { imported: 1, failed: [] },
  listError = null;
let presets = [],
  calls = [],
  dialogs = [],
  toasts = [];
const base = 'a'.repeat(64),
  capturedBase = 'b'.repeat(64);
const profile = { base, look: 'look-one', amount: 37 };
const browserModel = {
  camera: 'Synthetic Camera',
  cameraProfiles: [{ profileName: 'Cobalt Modular', entries: [{ id: base, profileName: 'Cobalt Modular' }] }],
  looks: [{ uuid: 'look-one', name: 'Synthetic neutral', group: 'Image Looks', pairing: { state: 'satisfied', base } }],
};
globalThis.__ui1Open = async (options) => {
  dialogs.push(options);
  if (dialogError) throw dialogError;
  return dialogResult;
};
globalThis.__ui1Toast = Object.fromEntries(
  ['success', 'error', 'info'].map((kind) => [kind, (text) => toasts.push({ kind, text })]),
);
globalThis.__ui1Invoke = async (name, args) => {
  calls.push({ name, args });
  if (name === 'list_profiles_for_image') {
    if (listError) throw listError;
    return browserModel;
  }
  if (name === 'import_profiles') return imported;
  if (name === 'load_presets') return presets;
  if (name === 'generate_preset_preview') throw new Error('No original image loaded');
  if (name === 'handle_import_presets_from_files') return { presets, failures: [] };
  return [];
};
const bundle = await build({
  stdin: {
    contents: `
    export { default as ProfilePanel } from './src/components/panel/right/ProfilePanel';
    export { default as PresetsPanel } from './src/components/panel/right/PresetsPanel';
    export { useEditorStore } from './src/store/useEditorStore';
    export { useUIStore } from './src/store/useUIStore';
    export { useEditorActions, debouncedSetHistory, debouncedSave } from './src/hooks/useEditorActions';
    export { INITIAL_ADJUSTMENTS } from './src/utils/adjustments';
    export { Panel } from './src/components/ui/AppProperties';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['react', 'react/jsx-runtime'],
  plugins: [
    {
      name: 'mock-panel-boundaries',
      setup(b) {
        b.onResolve(
          {
            filter:
              /^(@tauri-apps\/api\/core|@tauri-apps\/plugin-dialog|@tauri-apps\/plugin-os|react-i18next|react-toastify|framer-motion|@dnd-kit\/core)$|ContextMenuContext$/,
          },
          (a) => ({ path: a.path, namespace: 'ui1-mock' }),
        );
        b.onLoad({ filter: /.*/, namespace: 'ui1-mock' }, ({ path }) => ({
          contents:
            path === '@tauri-apps/api/core'
              ? 'export const invoke = (...a) => globalThis.__ui1Invoke(...a);'
              : path.endsWith('plugin-dialog')
                ? 'export const open = (...a) => globalThis.__ui1Open(...a); export const save = async () => null;'
                : path.endsWith('plugin-os')
                  ? "export const platform = () => 'android';"
                  : path === 'react-i18next'
                    ? 'const t=(...a)=>globalThis.__ui1Translate(...a); export const useTranslation=()=>({t});'
                    : path === 'react-toastify'
                      ? 'export const toast=globalThis.__ui1Toast;'
                      : path.endsWith('ContextMenuContext')
                        ? 'export const useContextMenu=()=>({showContextMenu(){}});'
                        : path === '@dnd-kit/core'
                          ? `export const DndContext=({children})=>children; export const DragOverlay=({children})=>children;
        export const PointerSensor=()=>{}; export const useSensor=()=>null; export const useSensors=()=>[];
        export const useDraggable=()=>({attributes:{},listeners:{},setNodeRef(){},isDragging:false});
        export const useDroppable=()=>({setNodeRef(){},isOver:false});`
                          : `export const motion=new Proxy({}, {get:(_,tag)=>tag}); export const AnimatePresence=({children})=>children;`,
        }));
      },
    },
  ],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  ProfilePanel,
  PresetsPanel,
  useEditorStore,
  useUIStore,
  useEditorActions,
  INITIAL_ADJUSTMENTS,
  Panel,
  debouncedSave,
  debouncedSetHistory,
} = mod.exports;
let mounted;
function reset(items = []) {
  dialogResult = null;
  dialogError = null;
  listError = null;
  imported = { imported: 1, failed: [] };
  presets = items;
  calls = [];
  dialogs = [];
  toasts.length = 0;
  useEditorStore.setState({
    selectedImage: { path: '/synthetic/camera.RAF', isReady: true },
    adjustments: { ...INITIAL_ADJUSTMENTS, profile: { ...profile } },
  });
}
async function mount(kind = 'profile') {
  function ActualPanel() {
    const adjustments = useEditorStore((s) => s.adjustments);
    const { setAdjustments } = useEditorActions();
    return kind === 'profile'
      ? React.createElement(ProfilePanel, { adjustments, setAdjustments, imagePath: '/synthetic/camera.RAF' })
      : React.createElement(PresetsPanel, { onNavigateToCommunity() {} });
  }
  await act(async () => {
    mounted = create(React.createElement(ActualPanel));
  });
  return mounted;
}
const text = (node) => node.children.map((x) => (typeof x === 'string' ? x : text(x))).join('');
const button = (root, label) =>
  root.root.findAllByType('button').find((n) => text(n) === label || n.props['aria-label'] === label);
const click = async (node) => {
  assert.ok(node, 'actual button exists');
  await act(async () => node.props.onClick());
};
const item = (root, id) =>
  root.root.find(
    (n) => typeof n.type === 'function' && n.type.name === 'DraggablePresetItem' && n.props.preset.id === id,
  );
async function apply(root, id) {
  const div = item(root, id)
    .findAllByType('div')
    .find((n) => n.props.onClick);
  await act(async () => div.props.onClick());
}
async function intensity(root, amount) {
  const range = root.root.findByProps({ type: 'range' });
  await act(async () => range.props.onChange({ target: { value: String(amount) } }));
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  assert.equal(root.root.findByProps({ type: 'range' }).props.value, amount);
}
afterEach(async () => {
  if (mounted) await act(async () => mounted.unmount());
  mounted = null;
  debouncedSave.cancel();
  debouncedSetHistory.cancel();
});

test('separate visible base/Look actions pass exact dialog filters and cancel without importing', async () => {
  reset();
  const root = await mount();
  for (const [key, ext] of [
    ['importBase', 'dcp'],
    ['importLook', 'xmp'],
  ]) {
    const control = button(root, translate(`profiles.${key}`));
    assert.match(control.props.className, /min-h-\[44px\]/);
    assert.match(control.props.className, /focus-visible/);
    dialogResult = ext === 'dcp' ? null : [];
    await click(control);
    assert.deepEqual(dialogs.at(-1).filters[0].extensions, [ext]);
    assert.equal(control.props.disabled, false);
  }
  assert.equal(calls.filter((x) => x.name === 'import_profiles').length, 0);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('base import calls real profile hook and refreshes the active image without selecting a replacement', async () => {
  reset();
  dialogResult = ['/synthetic/base.dcp'];
  const root = await mount();
  await click(button(root, translate('profiles.importBase')));
  assert.deepEqual(calls.find((x) => x.name === 'import_profiles').args, { sourcePaths: dialogResult });
  assert.equal(calls.filter((x) => x.name === 'list_profiles_for_image').length, 2);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
  assert.ok(toasts.some((x) => x.kind === 'success'));
});

test('ordinary XMP in Look destination retains original rejection and guides to existing Presets panel', async () => {
  reset();
  dialogResult = ['/synthetic/Exposure.xmp'];
  const diagnostic = "invalid Look 'Exposure.xmp': missing crs:CameraProfile";
  imported = { imported: 0, failed: [{ path: dialogResult[0], reason: diagnostic }] };
  const root = await mount();
  await click(button(root, translate('profiles.importLook')));
  assert.ok(
    toasts.some(
      (x) =>
        x.kind === 'error' && x.text.includes(diagnostic) && x.text.includes(translate('profiles.presetImportHint')),
    ),
  );
  assert.ok(root.root.findByProps({ role: 'status' }));
  assert.equal(
    calls.some((x) => x.name === 'handle_import_presets_from_files'),
    false,
  );
  await click(button(root, translate('profiles.goToPresets')));
  assert.equal(useUIStore.getState().activePanel, Panel.Presets);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('dialog rejection remains an error, re-enables both actions and never substitutes profiles', async () => {
  reset();
  dialogError = new Error('Synthetic access denied');
  const root = await mount();
  await click(button(root, translate('profiles.importLook')));
  assert.ok(toasts.some((x) => x.kind === 'error' && x.text.includes('Synthetic access denied')));
  assert.equal(button(root, translate('profiles.importBase')).props.disabled, false);
  assert.equal(button(root, translate('profiles.importLook')).props.disabled, false);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('current base and optional Look identity stay visible; Look slider changes only its amount', async () => {
  reset();
  const root = await mount();
  const labels = root.root.findAllByType('dd').map(text);
  assert.deepEqual(labels, ['Cobalt Modular', 'Synthetic neutral · 37%']);
  await intensity(root, 200);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, { ...profile, amount: 200 });
  await click(button(root, translate('profiles.resetTooltip')));
  assert.deepEqual(useEditorStore.getState().adjustments.profile, { base: null, look: null, amount: 100 });
  assert.ok(root.root.findAllByType('dd').map(text).includes(translate('profiles.noBase')));
});

test('profile list failure is visible with diagnostics and does not erase selection', async () => {
  reset();
  listError = new Error('Synthetic list unavailable');
  const root = await mount();
  assert.match(text(root.root.findByProps({ role: 'alert' })), /Synthetic list unavailable/);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('preset Import presets button uses adjustment formats and actual preset import hook', async () => {
  reset();
  const root = await mount('presets');
  for (const cancelled of [null, []]) {
    dialogResult = cancelled;
    await click(button(root, translate('editor.presets.importAction')));
    assert.equal(
      calls.some((x) => x.name === 'handle_import_presets_from_files'),
      false,
    );
  }
  dialogResult = ['/synthetic/Exposure.xmp'];
  await click(button(root, translate('editor.presets.importAction')));
  assert.deepEqual(dialogs.at(-1).filters[0].extensions, ['rrpreset', 'xmp', 'lrtemplate']);
  assert.deepEqual(calls.find((x) => x.name === 'handle_import_presets_from_files').args, { filePaths: dialogResult });
  assert.equal(
    calls.some((x) => x.name === 'import_profiles'),
    false,
  );
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('ordinary preset apply and 0/50/100/200 intensity change adjustments while preserving exact base/Look/amount', async () => {
  reset([{ preset: { id: 'ordinary', name: 'Exposure and colour', adjustments: { exposure: 2, saturation: 20 } } }]);
  const root = await mount('presets');
  await apply(root, 'ordinary');
  assert.equal(useEditorStore.getState().adjustments.exposure, 2);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
  for (const [amount, exposure, saturation] of [
    [0, 0, 0],
    [50, 1, 10],
    [100, 2, 20],
    [200, 4, 40],
  ]) {
    await intensity(root, amount);
    const a = useEditorStore.getState().adjustments;
    assert.equal(a.exposure, exposure);
    assert.equal(a.saturation, saturation);
    assert.deepEqual(a.profile, profile);
  }
});

test('a deliberately captured profile applies intact and preset intensity never scales its selected Look amount', async () => {
  const captured = { base: capturedBase, look: 'captured-look', amount: 121 };
  reset([
    {
      preset: {
        id: 'captured',
        name: 'Deliberate capture',
        includeProfile: true,
        adjustments: { exposure: 2, profile: captured },
      },
    },
  ]);
  const root = await mount('presets');
  await apply(root, 'captured');
  assert.ok(text(item(root, 'captured')).includes(translate('editor.presets.includesProfile')));
  for (const amount of [0, 50, 200]) {
    await intensity(root, amount);
    assert.deepEqual(useEditorStore.getState().adjustments.profile, captured);
  }
});

test('actual save-preset modal defaults profile capture off and preserves the explicit opt-in through save', async () => {
  reset();
  const root = await mount('presets');
  await click(button(root, translate('editor.presets.tooltips.saveNew')));
  const checkbox = root.root.find(
    (n) => typeof n.type === 'function' && n.props.label === translate('modals.configurePreset.includeProfile'),
  );
  assert.equal(checkbox.props.checked, false);
  await act(async () => checkbox.props.onChange(true));
  const name = root.root.findByProps({ placeholder: translate('modals.configurePreset.placeholder') });
  await act(async () => name.props.onChange({ target: { value: 'Captured foundation' } }));
  await act(async () => name.props.onKeyDown({ key: 'Enter' }));
  const saved = root.root
    .findAll((n) => typeof n.type === 'function' && n.type.name === 'DraggablePresetItem')
    .map((n) => n.props.preset)
    .find((p) => p.name === 'Captured foundation');
  assert.equal(saved.includeProfile, true);
  assert.deepEqual(saved.adjustments.profile, profile);
});

test('turning off an ordinary preset keeps a camera profile changed after that preset was applied', async () => {
  reset([{ preset: { id: 'ordinary', name: 'Exposure', adjustments: { exposure: 2 } } }]);
  const root = await mount('presets');
  await apply(root, 'ordinary');
  const replacement = { base: capturedBase, look: null, amount: 0 };
  await act(async () => {
    useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, profile: replacement } });
  });
  await apply(root, 'ordinary');
  assert.deepEqual(useEditorStore.getState().adjustments.profile, replacement);
  assert.equal(useEditorStore.getState().adjustments.exposure, INITIAL_ADJUSTMENTS.exposure);
});

test('base DCP rejection retains diagnostics without suggesting the adjustment preset importer', async () => {
  reset();
  dialogResult = ['/synthetic/base.dcp'];
  const diagnostic = "invalid DCP 'base.dcp': synthetic parser failure";
  imported = { imported: 0, failed: [{ path: dialogResult[0], reason: diagnostic }] };
  const root = await mount();
  await click(button(root, translate('profiles.importBase')));
  assert.deepEqual(dialogs.at(-1).filters[0].extensions, ['dcp']);
  assert.ok(toasts.some((x) => x.kind === 'error' && x.text.includes(diagnostic)));
  assert.equal(
    toasts.some((x) => x.text.includes(translate('profiles.presetImportHint'))),
    false,
  );
  assert.equal(root.root.findAllByProps({ role: 'status' }).length, 0);
  assert.equal(button(root, translate('profiles.goToPresets')), undefined);
  assert.equal(
    calls.some((x) => x.name === 'handle_import_presets_from_files'),
    false,
  );
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});
