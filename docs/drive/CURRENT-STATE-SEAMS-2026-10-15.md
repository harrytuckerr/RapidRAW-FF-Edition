# Current-State Persistence Seams (GD0 map)

Date: 2026-10-15 · Base: detached `5ca276ac1dab210f11ee4cecbf904038516164f8` · Source-only, read-only.
Legend: **Pass** = verified by reading source. **Not exercised** = needs runtime, device, or provider. **Unknown** = not traced here.
Line numbers refer to the base commit. Paths are relative to the repo root.

## 1. Inventory

| Data | Current store (absolute location is device-specific unless noted) | Writer → reader | Portable? | Status |
|---|---|---|---|---|
| App settings | `<app_data_dir>/settings.json` (`app_settings.rs:726-737`) | `save_settings` (`:812`) ← `handleSettingsChange` (`src/store/useSettingsStore.ts:41-58`); `load_settings` (`:745`) | No. Mixes portable prefs with device paths and flags | Implemented |
| Edits (adjustments) | Sidecar `<source-dir>/<name>.rrdata`, or `<name>.<id>.rrdata` for virtual copies (`file_management.rs:397-426`). JSON `ImageMetadata` `{version, rating, flag, adjustments, tags, exif}` (`image_processing.rs:71-80`) | `save_metadata_and_update_thumbnail` (`file_management.rs:2567`, write `:2590`) ← `debouncedSave` 300 ms (`src/hooks/useEditorActions.ts:25-31`); `load_metadata` (`:3069`) ← `useImageLoader.ts:37` | Yes with the photo folder. Not with app data | Implemented |
| Virtual copy | Copies the parent `.rrdata` to the new `.<id>.rrdata` (`create_virtual_copy`, `:4633-4652`) | Frontend `CreateVirtualCopy` (`src/hooks/useAppContextMenus.ts:483`) | Same as edits | Implemented |
| Undo / redo history | `useEditorStore` in memory only, max 50 (`src/store/useEditorStore.ts:97`, `:146-151`); pushed via `debouncedSetHistory` 500 ms (`useEditorActions.ts:21-23`) | None | No | In-memory only |
| Checkpoints | None found. Only a log line mentioning "checkpoint" (`export_processing.rs:2086`) | None | n/a | Absent |
| Presets | `<app_data_dir>/presets/presets.json` (`file_management.rs:3086-3099`); tree of `PresetItem` (`:215-249`) | `save_presets` (`:3111`) ← `usePresets.ts` (11 call sites, 500 ms debounce, `:72-78`); `load_presets` (`:3101`) ← `:61` | Preset `adjustments` yes; file is device-local | Implemented |
| Preset export / import | `{creator, presets}` file written by `handle_export_presets_to_file` (`:3294-3306`); import by `parse_preset_file` (`:3161`), merge at `handle_import_presets_from_files` (`:3260`) | User-chosen file path | Yes (file) | Implemented |
| Profile opt-in in presets | Frontend sends `includeProfile` in the preset object (`src/hooks/usePresets.ts:126-130`). Profile value is copied into `adjustments.profile` (`:116-118`) | See Finding F1 | Value portable. Flag not persisted by Rust | Partial, see F1 |
| Profile selection in edits | `adjustments.profile = {base, look, amount}` (`src/utils/adjustments.ts:462-466`). `base` is 64-hex canonical ID (`:469-482`) | Same as edits | Yes if the same DCP bytes exist on the other device | Implemented |
| DCP profiles | Managed copies in `<app_data_dir>/profiles/dcp/*.dcp`, looks in `profiles/looks/*.xmp`, captured LUTs in `profiles/looks/captured/<uuid>.cube` (`dcp/registry.rs:523-535`, `dcp/capture.rs:89-91`) | `import_bytes` (`dcp/commands.rs:303-351`, direct `fs::write` `:327`, `:341`); `remove_profile` (`:384`) | Files yes. Paths no | Implemented |
| Profile identity | `ProfileId` = BLAKE3 of DCP bytes (`dcp/parser.rs:91`). Look `uuid` from XMP (`dcp/commands.rs:201-210`) | Used by `get_by_hex` (`dcp/registry.rs:324`) | Yes (content-addressed) | Implemented |
| Profile index | `<app_data_dir>/profiles/index.json`, version 1 (`dcp/registry.rs:39`, `:440-449`). Stores absolute `path`, id, size, mtime (`:383-400`) | `persist_index_after_change` best-effort (`dcp/commands.rs:559-567`); `discover_profiles` (`dcp/registry.rs:588`) | No (absolute paths). Rebuildable | Implemented |
| Adobe-discovered profiles | Read-only roots, platform-specific (`dcp/registry.rs:547-578`) | Scan only | No | Implemented |
| XMP sidecar sync | Reads `<source>.xmp` for rating, flag, label, tags (`file_management.rs:4753-4808`). Writes rating and tags back (`:4811`). Gated by `enable_xmp_sync`, `create_xmp_if_missing` | `load_metadata`, `update_metadata_for_paths` (`:2989`) | Yes beside the photo | Implemented. Crs develop settings not read (no `crs:` in `file_management.rs`) |
| EXIF cache | `<app_cache_dir>/exif/<blake3(folder)>.json` (`exif_processing.rs:70-105`), flushed every 3 s via tmp + rename | Internal | No, rebuildable | Implemented |
| Thumbnail cache | `app_cache_dir` (`file_management.rs:3393`, `:3644`) | Internal | No | Implemented |
| Albums | `<app_data_dir>/albums/` (`file_management.rs:852-860`, `save_albums` `:897`) | `save_albums` ← `useLibraryActions.ts:495`, `useAppContextMenus.ts` | No (device) | Implemented. File name not traced |
| Community presets | Fetched over HTTP (`lib.rs:1187`, `fetch_community_presets`). Saved into the `Community` folder of presets (`file_management.rs:3309-3363`) | `CommunityPage.tsx:187` | As presets | Implemented, network-dependent. Not exercised |
| Cloud account | Clerk plugin (`src/store/useCloudStore.ts:3`, `:54`, `:69`; registered `src-tauri/src/lib.rs:1934`). Only API call found: `GET ${CLOUD_API_BASE_URL}/usage` with bearer (`useCloudStore.ts:7`, `:82-89`) | Read-only usage | n/a | Usage only. No settings, edits, presets, or profiles sync found |
| Cloud inpainting | Hard-coded base `https://www.getrapidraw.com/api` (`inpainting.rs:538`) | Outbound | n/a | Implemented, network-dependent |
| AI connector | User address `ai_connector_address` (`app_settings.rs`, field alias `comfyuiAddress`). Bearer `token` parameter in `ai_connector.rs:82`, `:144`, `:211`. Origin of the value **Unknown** (not traced) | Outbound | No | Implemented |

## 2. Atomicity and versioning (source facts)

| Store | Version field | Write mode | Read failure behaviour |
|---|---|---|---|
| `settings.json` | None | Direct `fs::write` (`app_settings.rs:805`, `:815`) | `unwrap_or_default` (`:750`). Silent defaults with no backup. The next `save_settings` replaces the file |
| `*.rrdata` | `version: u32`, not checked on read (`image_processing.rs:72`) | Direct `fs::write` (`file_management.rs:2590`, `:109`, `:513`, `:3006`, `:3080`); `load_sidecar` heals bloated EXIF by rewriting (`exif_processing.rs:246`) | `load_sidecar` returns `ImageMetadata::default()` on parse failure (`exif_processing.rs:233`). The next edit or rating save writes that default over the original (Finding F2) |
| `presets.json` | None | Direct write (`file_management.rs:3111-3114`) | Load returns `Err` (`:3101-3108`). Frontend sets `[]` in memory (`usePresets.ts:58-66`). Saves happen only from mutators, not on load |
| `profiles/index.json` | `version: 1` checked (`dcp/registry.rs:434`) | Direct write, best-effort (`:440-449`, `dcp/commands.rs:559-567`) | Version mismatch forces a re-parse (`:434`) |
| DCP copies | Content-addressed name, not versioned | Validated by parse before copy (`dcp/commands.rs:320-327`), then direct write | Invalid files rejected |
| EXIF cache | None | tmp + rename, no fsync (`exif_processing.rs:98-103`) | Parse failure gives empty cache (`:122-123`) |

The only temp-file-plus-rename write found in the persistence path is the EXIF cache. The AI model download uses rename too (`ai_processing.rs:348-355`).

## 3. Findings for the architect (source facts, not decisions)

- **F1. `includeProfile` is not persisted by Rust.** The frontend sends `includeProfile` (`usePresets.ts:126-130`). The Rust `Preset` struct has no matching field (`file_management.rs:215-226`; `include_profile` appears nowhere in `src-tauri/src`). Serde drops unknown fields on the next `save_presets` round-trip. `existingPreset.includeProfile` checks (`usePresets.ts:273-288`, `:375`) then read `false` after reload. **Not exercised** at runtime.
- **F2. Corrupt `.rrdata` is replaced by defaults on the next save.** `load_sidecar` silently falls back to defaults (`exif_processing.rs:233`). `save_metadata_and_update_thumbnail` writes `metadata` built on that fallback (`file_management.rs:2578-2590`). The original file has no backup. **Not exercised** at runtime.
- **F3. Corrupt `settings.json` is replaced on the next settings save.** `load_settings` uses `unwrap_or_default` (`app_settings.rs:750`), and `save_settings` overwrites (`:815`). The UI then holds defaults. On Linux, the migration flag (`:766-772`) also rewrites the file on load. **Not exercised.**
- **F4. Edits are stored beside the original, so they are only as portable as the photo folder.** Virtual copies use the `?vc=` suffix (`file_management.rs:397-426`). Android `content://` sources are imported (copied) before editing (`file_management.rs:4318-4330`). Whether sidecars for copied content are written beside the copy was **not traced**.
- **F5. Settings mix portable and device-specific data.** Examples: `root_folders`, `last_root_path`, `pinned_folders`, `folder_icons` keys, `last_folder_state` (absolute paths); `processing_backend`, `use_wgpu_renderer`, `linux_gpu_optimization`, `raw_preprocessing_*` (device-specific). A sync design needs a split. The map does not decide where the split goes.
- **F6. Profile base IDs are content-addressed, but the index stores absolute paths.** `adjustments.profile.base` (BLAKE3 hex) resolves on any device that has the same DCP bytes (`dcp/parser.rs:91`). `index.json` (`registry.rs:383-400`) is not portable and is rebuildable.
- **F7. No checkpoint or version-history feature exists.** Only in-memory undo / redo (50 entries).
- **F8. No settings, edits, presets, or profiles sync client exists in this repo.** Search for `getrapidraw.com/api`, `sync` with cloud/remote, and per-data endpoints found only `/usage` (`useCloudStore.ts:88`) and the inpainting base URL (`inpainting.rs:538`). Clerk auth exists. Do not assume Google sign-in works through Clerk. That is an unverified provider question.
- **F9. No credential store.** No keyring, keychain, or secret-store crate in `src-tauri/Cargo.toml`. Token values were not read or printed. Origin of the AI connector bearer token is **Unknown**.

## 4. Consumers and file ownership (for the architect)

Hot files on the persistence path, by line count and write sites:
- `src-tauri/src/file_management.rs` (6,124 lines; 24 direct `fs::write` calls; presets, sidecars, albums, XMP sync)
- `src-tauri/src/app_settings.rs` (settings I/O, migrations)
- `src-tauri/src/exif_processing.rs` (sidecar load, EXIF cache)
- `src-tauri/src/dcp/registry.rs`, `src-tauri/src/dcp/commands.rs` (profiles)
- `src/store/useSettingsStore.ts`, `src/hooks/usePresets.ts`, `src/hooks/useEditorActions.ts`, `src/hooks/useImageLoader.ts`

The UI1 six-file list was **not provided** to this map, so collisions were **not** checked against it. The lead should intersect it with the list above before assigning Astra.

## 5. Not exercised

- Runtime round-trip of `includeProfile` (F1) and corrupt-file behaviour (F2, F3).
- Android: settings path, `.library` root (`android_integration.rs:608-650`), and sidecar placement for `content://` sources (F4).
- Cloud: `/usage` response, Clerk session, and any provider behaviour.
- Scale and performance of the EXIF cache and `index.json`.
- Whether the `Community` preset fetch works offline.
