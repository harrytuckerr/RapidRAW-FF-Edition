# GD0 State Map Receipt

Date: 2026-10-15
Task: GD0 map — current settings, edits, preset and profile persistence seams
Executor: Claude Code - Haiku (claude-haiku-5-5, as reported by the agent environment; configuration not inspected)
Worktree: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-state-map`
Base: detached `5ca276ac1dab210f11ee4cecbf904038516164f8` (`docs: record successful main promotion and verified preview packages [skip ci]`); clean before this task

## Deliverables (new files only)

| File | SHA256 |
|---|---|
| `docs/drive/CURRENT-STATE-SEAMS-2026-10-15.md` | `af74957f1e50b32a186ad986dbc255c50ddfbb74474a05e83a98edc190cfe5b5` |
| `docs/integration/GD0-STATE-MAP-RECEIPT-2026-10-15.md` | Recorded in the team message, not inside this file |

`git status --porcelain` shows only the new `docs/drive/` directory and the receipt. No tracked file changed. No commit, build, test, npm, native, cache, device, UI, account, network, or cloud action was run.

## Source inputs (read-only; SHA256 of the base file contents)

| File | SHA256 |
|---|---|
| `src/store/useSettingsStore.ts` | `9e5b295cfea558565bc9b55be918c5f71ef4469d8a9de2b8d7fac80fc37468e2` |
| `src/store/useEditorStore.ts` | `a5c6a40142143bb34b409f78a3bb2a6d1a643382320b755e7abbe81b715f629e` |
| `src/hooks/usePresets.ts` | `8867497e65c5f3ad3505ddd95ce7fd245bef77690a7813219dea7d24c5fe7f93` |
| `src/hooks/useProfiles.ts` | `a7750146c9f54fad9874341675b0d236f14ed55af2bfe5eac0ee2029062dad22` |
| `src/hooks/useImageLoader.ts` | `69e0aeed347a364dfb59557c41cad4ee4a64e52f5c18bb8492598ab10d63b4ef` |
| `src/hooks/useEditorActions.ts` | `44fdc699e14cb87f12b87c0be69a28bfc4110383cdc3b5a01157a380a0abcffe` |
| `src/utils/adjustments.ts` | `520b1172c22792f0e1cc1037c2332b5e343bbafaf438cc4f22340650adb17283` |
| `src/store/useCloudStore.ts` | `6dd911ef513e5d4ece75c2ff2f37c5b032e278eb5cdde7273a011e89ca803a5a` |
| `src-tauri/src/app_settings.rs` | `cb468be204900650452c2ec40fa4fe16ab6952373a71dbc3d32b33f745aa0f91` |
| `src-tauri/src/file_management.rs` | `56ca21c1488f0e72fde52796e3b3fa97e1a3179f56d467cce599e615ad09d81e` |
| `src-tauri/src/exif_processing.rs` | `51eb6e7658862d6b2ffd4526ce1b7fd18e3d4185c6c3dc1ac6cc6b676a6d7455` |
| `src-tauri/src/image_processing.rs` | `55194396fb1ed7bca0e38bb1d138a4d758dae29be83c9760bb40044b4dbc274e` |
| `src-tauri/src/dcp/registry.rs` | `e5e702dafe96bf0d351ebbb14c5be22ae7b8d2f5517f04e7d0300e5b7ab41ae4` |
| `src-tauri/src/dcp/commands.rs` | `fa19b95007fba60ec6822290f6dac644c18602af0fef0352cb7c9e30c1aa74a5` |
| `src-tauri/src/dcp/parser.rs` | `ef2a381587b49167b5853019548666a28c965daceb41134658390a428cab8e2d` |
| `src-tauri/src/dcp/capture.rs` | `8303350b355d95dc16fecaeca0be4535201623870b93268154ec805a23106450` |
| `src-tauri/src/android_integration.rs` | `7ff6a04dbb9af36890b101c35b64aae31f389e64d214259d4ff67b365ed5e9d6` |
| `src-tauri/src/tagging.rs` | `f8eeb304e4d69e8f9003840d70da18059bb50905a444e16e58f8cbbbddf0052d` |
| `src-tauri/src/lib.rs` | `9a1ec07f6517260dd7c186e9ec0be3df0dde73bf2f904a33e60de18383615870` |
| `src-tauri/src/ai_connector.rs` | `f90878f5c1392b50f99cc29ceae562d13287f377c619118ec8d4e97cdcdf30fa` |
| `src-tauri/Cargo.toml` | `454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2` |
| `src-tauri/capabilities/default.json` | `7da702bc090b288d4e55cda5b0a6b4806d511711b080b5eb53f4e29f531b4d4f` |
| `src-tauri/capabilities/desktop.json` | `c7aabe18a59fc24f37679458710354ca0b22b6bc9625efb44b56f519613ed0bd` |

## Commands run (read-only)

```text
cd /Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-state-map
git rev-parse HEAD                              # 5ca276ac1dab210f11ee4cecbf904038516164f8
git symbolic-ref -q HEAD || echo DETACHED       # DETACHED
git status --porcelain                          # clean before writing
find src-tauri/src -name '*.rs' | xargs wc -l   # size survey
grep -rn -E '<pattern>' src src-tauri/src   # persistence, invoke, cloud, HTTP, plugin, and credential names (values not printed)
sed -n <range>       # reads of cited functions
shasum -a 256 <files>
wc -l <files>
```

Credential checks printed only names and line numbers. No token, key, or secret value was read or printed.

## Evidence summary

| Check | Result |
|---|---|
| Worktree at frozen base, detached, clean | Pass |
| Settings store: `settings.json`, whole-object save, no version, direct write | Pass (source) |
| Edit store: `.rrdata` beside source, debounced 300 ms, direct write | Pass (source) |
| Undo / redo: in-memory only, 50 entries | Pass (source) |
| Checkpoints feature | Pass (source): absent |
| Presets: `presets/presets.json`, direct write, load error surfaced | Pass (source) |
| `includeProfile` survives Rust round-trip | Pass (source): **not persisted** (Finding F1). Runtime **Not exercised** |
| Corrupt `.rrdata` / `settings.json` silently defaulted, then overwritten on next save | Pass (source) (F2, F3). Runtime **Not exercised** |
| DCP managed paths, index version 1, BLAKE3 identity | Pass (source) |
| `profile.base` canonical 64-hex ID | Pass (source) |
| Cloud API surface: Clerk auth plus `/usage` only | Pass (source). Provider behaviour **Not exercised** |
| Cloud settings, edits, presets, or profiles sync | Pass (source): no client found (F8) |
| Credential storage (keyring / keychain) | Pass (source): none in `Cargo.toml`. AI bearer token origin **Unknown** |
| Android library root and `content://` sidecar placement | Source read, **Not exercised** (no device) |
| Settings and preset persistence on Android | **Unknown** (not traced separately) |
| Adobe-discovered profile roots | Pass (source), read-only |
| Runtime save/load round-trip on any platform | **Not exercised** |

## Notes and limits

- The UI1 six-file list was not provided, so the file-collision check against UI1 was **not performed**. The lead must intersect the hot-file list in section 4 of the seams doc with the UI1 list.
- Line numbers are from the base commit. They will drift if the base changes.
- Everything labelled "source" is a reading of code, not a runtime result.

## Next-consumer readiness

- **Architect (GD1 design)**: Ready. The seams doc has the inventory, atomicity table, and findings F1–F9. Decisions on portable/device split, sync scope, and atomic writes are not made here.
- **Implementation (Astra or others)**: Not assigned by this task. Blocked until the architect's design and the UI1 collision check are complete.
- **Cloud sync of settings / edits / presets / profiles**: Not ready. No client, endpoint, or auth-to-sync contract exists in the repo. Clerk login is not confirmed to work for sync.
- **Device and provider proofs (Android, Clerk, `/usage`)**: Not exercised. Needs a device or account, which this task did not touch.
