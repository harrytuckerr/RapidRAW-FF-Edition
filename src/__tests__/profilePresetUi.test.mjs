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
let coarsePointer = false;
globalThis.window = {
  matchMedia: () => ({ matches: coarsePointer, addEventListener() {}, removeEventListener() {} }),
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
  listError = null,
  dialogPending = null,
  modelOverride = null,
  onDialogOpen = null;
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
  onDialogOpen?.();
  if (dialogError) throw dialogError;
  if (dialogPending) return dialogPending;
  return dialogResult;
};
globalThis.__ui1Toast = Object.fromEntries(
  ['success', 'error', 'info'].map((kind) => [kind, (text) => toasts.push({ kind, text })]),
);
globalThis.__ui1Invoke = async (name, args) => {
  calls.push({ name, args });
  if (name === 'list_profiles_for_image') {
    if (listError) throw listError;
    return modelOverride || browserModel;
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
    export { default as ProfilePanel, ProfilesWorkspacePanel } from './src/components/panel/right/ProfilePanel';
    export { default as ControlsPanel } from './src/components/panel/right/ControlsPanel';
    export { default as PanelSwitcher, MobilePanelSwitcher } from './src/components/panel/PanelSwitcher';
    export { useSettingsStore } from './src/store/useSettingsStore';
    export { reconcileWorkspace } from './src/store/useUIStore';
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
        // Unrelated adjustment views are outside the dedicated Profiles route.
        b.onResolve({ filter: /(?:adjustments\/(?:Basic|Curves|Color|Details|Effects)|AdjustmentSectionsSubMenu|Waveform)$/ },
          (a) => ({ path: a.path, namespace: 'profile-nav-unrelated' }));
        b.onLoad({ filter: /.*/, namespace: 'profile-nav-unrelated' }, () => ({ contents: 'export default function UnrelatedView(){return null;}' }));
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
        export const useDroppable=()=>({setNodeRef(){},isOver:false}); export const useDndMonitor=()=>{};`
                          : `export const motion=new Proxy({}, {get:(_,tag)=>tag}); export const AnimatePresence=({children})=>children; export const LayoutGroup=({children})=>children;`,
        }));
      },
    },
  ],
});
const mod = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, mod, mod.exports);
const {
  ProfilePanel,
  ProfilesWorkspacePanel,
  ControlsPanel,
  PanelSwitcher,
  MobilePanelSwitcher,
  useSettingsStore,
  reconcileWorkspace,
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
  coarsePointer = false;
  dialogResult = null;
  dialogError = null;
  listError = null;
  dialogPending = null;
  modelOverride = null;
  onDialogOpen = null;
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
async function mount(kind = 'profile', createNodeMock) {
  function ActualPanel() {
    const adjustments = useEditorStore((s) => s.adjustments);
    const { setAdjustments } = useEditorActions();
    return kind === 'profile'
      ? React.createElement(ProfilePanel, { adjustments, setAdjustments, imagePath: '/synthetic/camera.RAF' })
      : React.createElement(PresetsPanel, { onNavigateToCommunity() {} });
  }
  await act(async () => {
    mounted = create(React.createElement(ActualPanel), { createNodeMock });
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
const importSurface = (root) => root.root.findByProps({ 'data-profile-import-menu': true });
async function importControl(root, key) {
  if (!button(root, translate(`profiles.${key}`))) {
    await click(button(root, translate('profiles.importMenu')));
  }
  return button(root, translate(`profiles.${key}`));
}
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
    const control = await importControl(root, key);
    assert.match(importSurface(root).props.className, /min-h-\[44px\]/);
    assert.match(importSurface(root).props.className, /focus-visible/);
    dialogResult = ext === 'dcp' ? null : [];
    await click(control);
    assert.deepEqual(dialogs.at(-1).filters[0].extensions, [ext]);
    assert.equal((await importControl(root, key)).props.disabled, false);
  }
  assert.equal(calls.filter((x) => x.name === 'import_profiles').length, 0);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('base import calls real profile hook and refreshes the active image without selecting a replacement', async () => {
  reset();
  dialogResult = ['/synthetic/base.dcp'];
  const root = await mount();
  await click(await importControl(root, 'importBase'));
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
  await click(await importControl(root, 'importLook'));
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
  await click(await importControl(root, 'importLook'));
  assert.ok(toasts.some((x) => x.kind === 'error' && x.text.includes('Synthetic access denied')));
  assert.equal((await importControl(root, 'importBase')).props.disabled, false);
  assert.equal((await importControl(root, 'importLook')).props.disabled, false);
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
  await click(await importControl(root, 'importBase'));
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

test('compact import menu exposes exactly two actions only after opening, without selecting a profile', async () => {
  for (const mobile of [false, true]) {
    reset();
    coarsePointer = mobile;
    const root = await mount();
    assert.ok(button(root, translate('profiles.importMenu')));
    assert.equal(button(root, translate('profiles.importBase')), undefined);
    assert.equal(button(root, translate('profiles.importLook')), undefined);
    assert.equal(root.root.findAllByProps({ role: 'option' }).length, 0);
    await click(button(root, translate('profiles.importMenu')));
    assert.deepEqual(root.root.findAllByProps({ role: 'option' }).map(text), [
      translate('profiles.importBase'),
      translate('profiles.importLook'),
    ]);
    assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
    assert.equal(dialogs.length, 0);
    await act(async () => root.unmount());
    mounted = null;
  }
});

test('same-frame repeated import callbacks start one picker, close popup before dispatch and stay disabled until completion', async () => {
  reset();
  let finish;
  dialogPending = new Promise((resolve) => {
    finish = resolve;
  });
  const root = await mount();
  const control = await importControl(root, 'importBase');
  onDialogOpen = () => {
    assert.equal(button(root, translate('profiles.importMenu')).props.disabled, true);
    assert.equal(root.root.findAllByProps({ role: 'option' }).length, 0);
  };
  await act(async () => {
    control.props.onClick();
    control.props.onClick();
  });
  assert.equal(dialogs.length, 1);
  assert.equal(importSurface(root).props['aria-busy'], true);
  assert.equal(button(root, translate('profiles.importMenu')).props.disabled, true);
  assert.equal(button(root, translate('profiles.importMenu')).props['aria-expanded'], false);
  assert.equal(root.root.findAllByProps({ role: 'option' }).length, 0);
  let prevented = 0,
    stopped = 0;
  importSurface(root).props.onKeyDownCapture({
    key: 'd',
    preventDefault() {
      prevented++;
    },
    stopPropagation() {
      stopped++;
    },
  });
  assert.equal(prevented, 1);
  assert.equal(stopped, 1);
  let tabPrevented = false;
  importSurface(root).props.onKeyDownCapture({
    key: 'Tab',
    preventDefault() {
      tabPrevented = true;
    },
    stopPropagation() {},
  });
  assert.equal(tabPrevented, false);
  await act(async () => {
    finish(null);
    await dialogPending;
  });
  assert.equal(button(root, translate('profiles.importMenu')).props.disabled, false);
  assert.equal(calls.filter((x) => x.name === 'import_profiles').length, 0);
  assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
});

test('partial base and Look imports preserve original diagnostics, exact filters and one refresh per request', async () => {
  for (const [kind, ext] of [
    ['importBase', 'dcp'],
    ['importLook', 'xmp'],
  ]) {
    reset();
    dialogResult = [`/synthetic/good.${ext}`, `/synthetic/bad.${ext}`];
    imported = { imported: 1, failed: [{ path: dialogResult[1], reason: 'Synthetic original parser diagnostic' }] };
    const root = await mount();
    await click(await importControl(root, kind));
    assert.deepEqual(dialogs[0].filters[0].extensions, [ext]);
    assert.equal(calls.filter((x) => x.name === 'import_profiles').length, 1);
    assert.equal(calls.filter((x) => x.name === 'list_profiles_for_image').length, 2);
    assert.ok(toasts.some((x) => x.kind === 'success'));
    const failure = toasts.find((x) => x.kind === 'error');
    assert.ok(failure.text.includes('Synthetic original parser diagnostic'));
    assert.equal(failure.text.includes(translate('profiles.presetImportHint')), ext === 'xmp');
    assert.deepEqual(useEditorStore.getState().adjustments.profile, profile);
    await act(async () => root.unmount());
    mounted = null;
  }
});

test('import menu capture and real Dropdown handlers support arrows, Space, Escape and restored focus', async () => {
  reset();
  let root;
  const focus = [];
  const optionNodes = [0, 1].map((index) => ({
    focus() {
      document.activeElement = this;
      focus.push(index);
    },
  }));
  const trigger = {
    getAttribute: (key) => String(button(root, translate('profiles.importMenu')).props[key]),
    click: () => button(root, translate('profiles.importMenu')).props.onClick(),
    focus() {
      document.activeElement = this;
      focus.push('trigger');
    },
  };
  const surface = {
    querySelector: () => trigger,
    querySelectorAll: () => root.root.findAllByProps({ role: 'option' }).map((_, index) => optionNodes[index]),
  };
  root = await mount('profile', (element) => (element.props['data-profile-import-menu'] ? surface : null));
  const key = (value) => ({ key: value, currentTarget: surface, preventDefault() {}, stopPropagation() {} });
  await act(async () => importSurface(root).props.onKeyDownCapture(key('ArrowDown')));
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 5));
  });
  assert.equal(focus.at(-1), 0);
  importSurface(root).props.onKeyDownCapture(key('ArrowDown'));
  assert.equal(focus.at(-1), 1);
  let stopped = false,
    prevented = false;
  importSurface(root).props.onKeyDownCapture({
    ...key(' '),
    stopPropagation() {
      stopped = true;
    },
    preventDefault() {
      prevented = true;
    },
  });
  assert.equal(stopped, true);
  assert.equal(prevented, false); // native button Space activation remains available
  const dropdown = root.root.find((node) => typeof node.type === 'function' && node.type.name === 'Dropdown');
  await act(async () => {
    importSurface(root).props.onKeyDownCapture(key('Escape'));
    dropdown
      .findAllByType('div')
      .find((node) => node.props.onKeyDown)
      .props.onKeyDown(key('Escape'));
  });
  assert.equal(focus.at(-1), 'trigger');
  assert.equal(button(root, translate('profiles.importMenu')).props['aria-expanded'], false);
  await click(await importControl(root, 'importLook'));
  assert.equal(focus.at(-1), 'trigger');
  assert.equal(button(root, translate('profiles.importMenu')).props.disabled, false);
});

test('profile browser preserves native ranked sequence across mixed groups and search without changing pairing', async () => {
  reset();
  const ranked = [
    { uuid: 'ccd', name: 'CCD example', group: 'Shared' },
    { uuid: 'fuji', name: 'Fuji example', group: 'Film' },
    { uuid: 'generic', name: 'Generic example', group: 'Shared' },
  ].map((entry) => ({ ...entry, requiredBaseProfile: 'Cobalt Modular', pairing: { state: 'satisfied', base } }));
  modelOverride = { ...browserModel, looks: ranked };
  const root = await mount();
  const profileTrigger = root.root
    .findAllByType('button')
    .find((node) => node.props['aria-haspopup'] === 'listbox' && text(node) !== translate('profiles.importMenu'));
  await click(profileTrigger);
  const names = () =>
    root.root
      .findAllByProps({ role: 'option' })
      .map(text)
      .filter((name) => ranked.some((entry) => entry.name === name));
  assert.deepEqual(names(), ['CCD example', 'Fuji example', 'Generic example']);
  const search = root.root
    .findAllByType('input')
    .find((node) => node.props.placeholder === translate('profiles.searchPlaceholder'));
  assert.ok(search);
  await act(async () => search.props.onChange({ target: { value: 'example' } }));
  assert.deepEqual(names(), ['CCD example', 'Fuji example', 'Generic example']);
  await act(async () => search.props.onChange({ target: { value: 'Shared' } }));
  assert.deepEqual(names(), ['CCD example', 'Generic example']);
  await click(root.root.findAllByProps({ role: 'option' }).find((node) => text(node) === 'Generic example'));
  assert.deepEqual(useEditorStore.getState().adjustments.profile, { ...profile, look: 'generic' });
});


test('dedicated Profiles navigation mounts existing selectors once and keeps source/history guards', async () => {
  reset();
  const previousUi = useUIStore.getState();
  const previousSettings = useSettingsStore.getState().appSettings;
  const initial = reconcileWorkspace(undefined, false);
  assert.deepEqual(initial.panelLayout.rightTop.slice(0, 3), [Panel.Adjustments, Panel.Profiles, Panel.Crop]);
  const saved = {
    ...initial,
    panelLayout: { ...initial.panelLayout, rightTop: initial.panelLayout.rightTop.filter((id) => id !== Panel.Profiles) },
  };
  const restored = reconcileWorkspace(saved, false);
  assert.equal(restored.panelLayout.rightTop.filter((id) => id === Panel.Profiles).length, 1);
  assert.equal(restored.activePanels.rightTop, Panel.Adjustments);
  assert.deepEqual(restored.panelLayout.rightTop.filter((id) => id !== Panel.Profiles), saved.panelLayout.rightTop);
  useUIStore.setState(restored);
  useSettingsStore.setState({ appSettings: {
    ...previousSettings,
    adjustmentLayout: { hiddenSections: ['basic', 'curves', 'color', 'details', 'effects'] },
  } });
  const source = useEditorStore.getState().selectedImage;
  useEditorStore.setState({ pendingOpenIntent: null, progressiveOpen: null, isWaveformVisible: false,
    history: [useEditorStore.getState().adjustments], historyIndex: 0 });
  // App's actual registration must point at the same connected component mounted below.
  assert.match(await readFile('src/App.tsx', 'utf8'), /case Panel\.Profiles:\s*return <ProfilesWorkspacePanel \/>;/);
  function WorkspaceBoundary() {
    const active = useUIStore((state) => state.activePanels.rightTop);
    const select = useUIStore((state) => state.setPanel);
    return React.createElement(React.Fragment, null,
      React.createElement(PanelSwitcher, { region: 'rightTop', side: 'right', placement: 'right' }),
      React.createElement(MobilePanelSwitcher, { activePanel: active, onPanelSelect: select }),
      active === Panel.Profiles ? React.createElement(ProfilesWorkspacePanel) : React.createElement(ControlsPanel));
  }
  try {
    await act(async () => { mounted = create(React.createElement(WorkspaceBoundary)); });
    const root = mounted;
    assert.equal(root.root.findAllByType(ProfilePanel).length, 0, 'Adjustments does not duplicate Profiles');
    await click(root.root.findByProps({ 'data-tab-id': 'layout-tab-profiles' }));
    assert.equal(useUIStore.getState().activePanels.rightTop, Panel.Profiles);
    assert.equal(root.root.findAllByType(ProfilePanel).length, 1);
    assert.equal(root.root.findByType(ProfilePanel).props.imagePath, source.path);
    assert.equal(useEditorStore.getState().selectedImage, source);
    assert.ok(calls.some((call) => call.name === 'list_profiles_for_image' && call.args.path === source.path));
    const selector = () => root.root.findAllByType('button').find((node) =>
      node.props['aria-haspopup'] === 'listbox' && text(node) !== translate('profiles.importMenu'));
    await click(selector());
    await click(root.root.findAllByProps({ role: 'option' }).find((node) => text(node) === 'Cobalt Modular'));
    assert.deepEqual(useEditorStore.getState().adjustments.profile, { base, look: null, amount: 100 });
    await click(selector());
    await click(root.root.findAllByProps({ role: 'option' }).find((node) => text(node) === 'Synthetic neutral'));
    await intensity(root, 42);
    await act(async () => debouncedSetHistory.flush());
    assert.deepEqual(useEditorStore.getState().history.at(-1).profile, { base, look: 'look-one', amount: 42 });
    const accepted = useEditorStore.getState().adjustments;
    await act(async () => useEditorStore.setState({ pendingOpenIntent: { ticket: 'pending', path: '/synthetic/next.RAF' } }));
    await act(async () => root.root.findByProps({ type: 'range' }).props.onChange({ target: { value: '77' } }));
    assert.equal(useEditorStore.getState().adjustments, accepted, 'existing pending-source mutation guard still blocks');
    await act(async () => useEditorStore.setState({ pendingOpenIntent: null }));
    await click(root.root.findByProps({ 'data-tab-id': 'layout-tab-adjustments' }));
    assert.equal(root.root.findAllByType(ProfilePanel).length, 0);
    await click(root.root.findByProps({ 'aria-label': translate('profiles.title') }));
    assert.equal(useUIStore.getState().activePanel, Panel.Profiles, 'folded navigation uses existing store selection');
    assert.equal(root.root.findAllByType(ProfilePanel).length, 1);
    await act(async () => useEditorStore.setState({ selectedImage: { path: '/synthetic/second.RAF', isReady: true } }));
    assert.equal(root.root.findByType(ProfilePanel).props.imagePath, '/synthetic/second.RAF');
    assert.ok(calls.some((call) => call.name === 'list_profiles_for_image' && call.args.path === '/synthetic/second.RAF'));
    await act(async () => useEditorStore.setState({ selectedImage: null }));
    assert.equal(root.root.findAllByType(ProfilePanel).length, 0, 'no source means no editable profile selectors');
  } finally {
    if (mounted) await act(async () => mounted.unmount());
    mounted = null;
    debouncedSetHistory.cancel();
    useUIStore.setState(previousUi);
    useSettingsStore.setState({ appSettings: previousSettings });
    useEditorStore.setState({ pendingOpenIntent: null, progressiveOpen: null });
  }
});
