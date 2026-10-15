# PS1 — Preserve preset profile-capture metadata through native save and reopen (receipt)

- Date: 2026-10-15
- Implementer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Worktree: `rapidraw-fuji-team-20261015/preset-profile-roundtrip`, detached HEAD `5ca276ac1dab210f11ee4cecbf904038516164f8` (verified, clean at start)
- Owned writes:
  - `src-tauri/src/file_management.rs` (Preset field, community constructor, focused tests)
  - `src-tauri/src/preset_converter.rs` (constructor only)
  - this receipt
  - private `/private/tmp/rapidraw-ps1-20261015/`
- No frontend, renderer, profile, parser, settings, sidecar, shared-lib, lock, dependency, Git, device or cloud change.

## 1. Defect and reproduction

The frontend saves presets as `{…, includeProfile: boolean, adjustments: {…, profile}}`. Native `Preset` had no `includeProfile` field, and serde ignores unknown fields. So `save_presets` / `load_presets`, which use `Vec<PresetItem>`, and import/export (`PresetFile`) **silently dropped** the flag. On reopen it was absent, whether it had been true or false. Malformed values were silently accepted, then dropped.

The regression tests were written **before** the fix (module `file_management::ps1_preset_profile_tests`). They work purely at JSON level through the real types, so they compile unchanged before and after.

Pre-fix run: `cargo test --locked --lib ps1_preset_profile_tests -- --nocapture` → **exit 101: 2 passed, 3 failed**. Log `logs/ps1-before-fix.log`, sha256 `a1f4c38a5414f20996bead5747aa5905e07b225ed3535ecc0aff0927a6bd1e91`.
- `explicit_profile_capture_survives_save_and_reopen` FAILED: the reloaded preset lacked `"includeProfile": true`.
- `folder_tree_and_preset_file_keep_each_flag` FAILED: the flag was dropped from top-level and folder children.
- `malformed_flag_is_rejected_not_dropped` FAILED: `accepted includeProfile="yes"`.
- `legacy_preset_without_flag_stays_unchanged` and `converted_xmp_preset_does_not_claim_profile_capture` passed. These are the expected non-regression controls.

Pre-fix artifacts:
- test binary `before-fix-test-binary`, sha256 `76a7900b278ad7db4d9a71930d8424b7ed6d8acd629aeb450384fc90c29687da`;
- source with tests and no fix `before-fix-file_management.rs`, sha256 `8add9b7285484bd3a4b816c63b3b1ac8c252933ad2dc26770de31f55c6259e0a`;
- originals `base-file_management.rs` / `base-preset_converter.rs`.

## 2. Fix (minimal)

```rust
/// Whether the preset was saved with "Include Profile". Absent for presets
/// saved before this flag existed; their adjustments are left as stored.
#[serde(rename = "includeProfile", skip_serializing_if = "Option::is_none")]
pub include_profile: Option<bool>,
```

- Explicit `true`/`false` round-trip exactly. Absent or JSON `null` becomes `None` and stays absent on write. A non-boolean is a deserialization error.
- `adjustments` (a `serde_json::Value`) is untouched, including `profile.base` / `look` / `amount`.
- There is no legacy-intent migration and no change to apply or render semantics.
- The only two `Preset` struct literals in the crate (checked by grep) get `include_profile: None`:
  - `file_management.rs` `save_community_preset` (community presets carry no profile capture);
  - `preset_converter.rs` `convert_xmp_to_preset` (an ordinary XMP preset does not claim profile capture).

| File | Base sha256 | After sha256 |
|---|---|---|
| `src-tauri/src/file_management.rs` | `56ca21c1488f0e72fde52796e3b3fa97e1a3179f56d467cce599e615ad09d81e` | `e9fca39553855ce61d9456bfbc2b8953bd61bf982a835a8492c64cc033381235` |
| `src-tauri/src/preset_converter.rs` | `667b31a39b458ca055a1c62a0e7b574bb9e72f77fbc1c94d83e53397dd489644` | `6d35e0f63dd9f9053783a7b16357212105c28f2cca5ea936a18e914317012cdd` |
| `src-tauri/Cargo.lock` | `7ddd491a…765def` | unchanged |
| `src-tauri/resources/libonnxruntime.dylib` (ignored, pinned) | `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` | unchanged; no download |

Diff: `ps1.diff`, sha256 `1934694f251c57e49d056305f374d2e6f3a31c83a19c2b541c05dd341cf884cc`. +120 lines, of which production is 4 field lines, 2 constructor lines and a comment, and the rest is the test module. `git diff --check` is clean.

## 3. Tests (5 fns, real `PresetItem` / `PresetFile` types)

| Test | Asserts |
|---|---|
| `explicit_profile_capture_survives_save_and_reopen` | For both `true` and `false`: deserialize `Vec<PresetItem>` → `to_string_pretty` → deserialize → serialize gives JSON identical to the input, including the flag and `profile {base, look, amount 37}`. |
| `legacy_preset_without_flag_stays_unchanged` | A legacy preset (no flag, with `adjustments.profile`) round-trips identically. No `includeProfile` key is added, and the profile values are unchanged. |
| `folder_tree_and_preset_file_keep_each_flag` | Top-level true plus folder children true/false/absent round-trip identically through `Vec<PresetItem>` and through `PresetFile` (import/export). |
| `malformed_flag_is_rejected_not_dropped` | `"yes"`, `1`, `{}` and `[true]` are rejected with a deserialize error. `null` is treated as absent. |
| `converted_xmp_preset_does_not_claim_profile_capture` | A real `convert_xmp_to_preset` output serializes without `includeProfile` or `adjustments.profile`. |

## 4. Commands and results (after fix)

Run inside `src-tauri`. Toolchain: rustc/cargo 1.98.1, uid 501. Environment: exclusive `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`, `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true` (`logs/env.txt` `1a5db84b…a9b155`). Free disk was 27 GiB before and 26 GiB after.

| Command | Exit | Result | Log sha256 |
|---|---|---|---|
| `cargo test --locked --lib ps1_preset_profile_tests -- --nocapture` | 0 | **5 passed**, 0 failed | `ps1-after-fix.log` `a20471d3b17da42f2f3086b1555b61dd4b89cbde5587d1e82c09cbda065da052` |
| `cargo test --locked --lib file_management:: -- --nocapture` | 0 | **37 passed**, 2 ignored: `tests` 13 + 2 ignored, `r3_terminal_tests` 18, `ps1_preset_profile_tests` 5 | `fm-tests.log` `08cec2964a81f408fed6ea2eb23b3b28f66cf0ecb1a7afc46f1b8786cdf6c253` |
| `cargo test --locked --lib` | 0 | **213 passed**, 0 failed, 14 ignored (none invoked) | `lib-suite.log` `df08f6f4411ad1c0d3c7732421ec368cfbf91479192b98abef5669cf87d435a1` |
| `cargo fmt --all -- --check` | 0 | clean | `fmt.log` `e38302ae…48bf` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 | no diagnostics; fresh 22:16 check metadata for lib, lib-test, `validate_dcp` | `clippy.log` `35a129253bc9d869a3262ff0800b14b0e309e0f69d57cb5f36890d7ffcd45b0a` |

Post-fix test binary: `after-fix-test-binary`, sha256 `74a4e784a1baa23adb4a63090614147a3b112359088428da3bb9e297ca3ef4c9`. The build script reported the pinned ONNX runtime as already valid, so nothing was downloaded. There were no unrelated lint fixes.

## 5. Remaining gaps (reported, not changed)

- **Legacy configuration gap.** Presets saved before this fix lost their flag on save, so they reload with `includeProfile` absent. Their `adjustments.profile` may or may not reflect an opt-in, and there is no reliable way to recover the original intent. Per scope, there is no speculative migration: such presets keep exactly their stored adjustments, and the frontend's handling of an absent flag decides the UI default. This needs a product decision, possibly frontend (UI1/Sol).
- **Malformed-file load behaviour (pre-existing, frontend).** `load_presets` now returns `Err` for a non-boolean `includeProfile`, as it already does for any malformed field, and the file on disk is not modified. However, `usePresets.ts:63-65` responds to a load error with `setPresets([])`, and a later debounced save could overwrite the file. The backend never writes non-boolean values; this is a pre-existing frontend risk outside PS1 ownership.
- Not exercised: the real Tauri IPC round-trip from the UI, Android, and the frontend preset apply semantics.

## 6. Gates

| Item | Status |
|---|---|
| Reproduced flag drop and malformed acceptance before the fix | Pass (observed Fail, retained log and binary) |
| `includeProfile` true/false preserved; legacy absent stays absent; adjustments including profile unchanged; malformed rejected; constructors None | Pass (host tests) |
| Existing `file_management` tests, full lib suite, fmt, strict clippy | Pass |
| Frontend/IPC/device round-trip | Not exercised |

Cache `/private/tmp/rapidraw-integration-20261015/target` is **released** when this receipt is delivered.
