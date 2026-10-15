# R2a revision — validation receipt

- Date: 2026-10-15
- Validator: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Worktree: `rapidraw-fuji-team-20261015/uri-revision`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9` (verified)
- Input source: `src-tauri/src/file_management.rs` `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7`, verified before and after. **No source change was needed or made.**
- Source-only receipt: `R2A-REVISION-RECEIPT-2026-10-15.md` `35c81f2a64bc18aa8fb947e6e439fd5f4fd9a70610b463e481373fb4bf91a646`. Kept unchanged as the historical, unexecuted phase.
- Unchanged: `formats.rs` `66134f6ce2183036fb35798cfb7eefc9d7ef512d72cbb2d873b762c9b4f2e5bf` and `Cargo.lock` `7ddd491a…765def`.
- Writes:
  - this receipt
  - private `/private/tmp/rapidraw-r2a-validation-20261015/`, holding logs and the preserved binary
  - the build-script artifact disclosed in §3
- No cargo clean, dependency refresh, GPU, Android or all-features build. No device action, Git staging or commit.

## 1. Environment

- `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1 (797e8a9bc 2026-08-05)`, aarch64-apple-darwin. Run inside `src-tauri`.
- uid 501, non-root, as the permission fixture requires. 36 GiB free, above the 6 GiB reserve.
- Exclusive `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`.
- Env: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false CARGO_INCREMENTAL=0`. Recorded in `logs/env.txt` `87dc3552…dee59`.

## 2. Commands and results

| # | Command | Exit | Actual result | Log sha256 (`/private/tmp/rapidraw-r2a-validation-20261015/logs/`) |
|---|---|---|---|---|
| 1 | `cargo test --locked --lib file_management::tests -- --nocapture` | 0 | **13 passed**, 0 failed, **2 ignored** (162 filtered out), matching the expected 13+2 | `fm-tests.log` `b0397300aae386aebab777569a9b7b4f8e978b00cd6893b36a2f29985149b472` |
| 2 | `RAPIDRAW_R2A_FIXTURE_DIR=/private/tmp/rapidraw-r2a-20261015/fixtures RAPIDRAW_R2A_NATIVE_RAF=…/211_FUJI/DSCF1893.RAF cargo test --locked --lib file_management::tests -- --ignored --nocapture` | 0 | **2 passed**, 0 failed (175 filtered out; only the R2a controls ran) | `fm-ignored-tests.log` `62acf85340c2e0db61a3aadfd71a9b630987e971ed7afbd478f93439bebf57ae` |
| 3 | `cargo test --locked --lib` | 0 | **163 passed**, 0 failed, 14 ignored (none invoked). That is 159 before plus the 4 new tests | `lib-suite.log` `3e5bf1662f87891e4a9af39dfe8c9ac0125218f991e3967492dd08d10ff807b9` |
| 4 | `cargo fmt --all -- --check` | 0 | clean | `fmt.log` `e38302ae…48bf` |
| 5 | `cargo clippy --locked --all-targets -- -D warnings` | 0 | no diagnostics. Fresh 17:48 check metadata for lib, lib-test, `validate_dcp` bin and bin-test (artifact mtimes confirm a real recheck) | `clippy-strict-all-targets.log` `0f9ebfb0dcb6455690335ff3ee1714f39baef5f10b58f218b91788e317c4a940` |

Control inputs, read-only:
- Original synthetic fixture `synthetic-camera.dng` `b2116c8b…20b53f`, from the original R2a private fixture dir.
- Genuine X-Pro2 `DSCF1893.RAF` `e736b4af4d80fa12418f92c018ac99b5c5c96d8e772ce3a34fc2cb9c2a4619b1`.
- Imported outputs: `synthetic-camera.dng` and `synthetic-camera.RAF` both `b2116c8b…`; `DSCF1893.RAF` `e736b4af…`. All byte-identical, and the sources were unchanged.

The test binary was built from `e66dceb6`, not reused from the prior R2a run: Cargo rebuilt it in run 1. Path `target/debug/deps/rapidraw_lib-b8c68747df43d7c8`, sha256 `6f8876f92bf6024a783e55247bd65445fb5221ba8a6b16bf3089a76cf20c5513` (the prior R2a binary was `1f736a06…`). It is preserved as `/private/tmp/rapidraw-r2a-validation-20261015/rapidraw-r2a-revision-test-binary`, same hash, and was unchanged after runs 2–5.

## 3. Build-script artifact disclosure

Run 1 ran the project build script, which downloaded ONNX Runtime v1.22.0 for macos-aarch64. Log lines 2–6 show the source URL `https://huggingface.co/CyberTimon/RapidRAW-Models/resolve/main/onnxruntimes-v1.22.0/libonnxruntime-macos-aarch64.dylib?download=true`. It went into `uri-revision/src-tauri/resources/libonnxruntime.dylib`:
- 33,481,272 bytes, sha256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`, identical to the earlier `uri-import` artifact;
- git-ignored (`!!`, `.gitignore:31`);
- not a tracked source or dependency change.

Later runs logged "already exists and is valid. Skipping download". There were no build failures in this validation.

## 4. What the new tests actually exercise

All of these call the real `publish_import_bytes`.

| Test | Real filesystem behaviour vs injection |
|---|---|
| `renamed_temporary_reports_not_found_as_clean` | **Real**: production no-replace persist (`persist_noclobber` on APFS) and real `fs::remove_file`. The only seam is a recorder wrapped around real removal. It observed exactly one removal returning `NotFound` and no warning, i.e. the temporary name was absent after publication. This does **not** prove which OS publication primitive ran (NOREPLACE rename, or hard link plus a successful unlink) |
| `unknown_temporary_status_after_hard_link_publication_is_a_warning` | **Real** hard link and a **real** `0o000` permission change on a private dir. The real `fs::remove_file` failed with a permission error. The result was Ok with the warning, final bytes intact, and final + temp present. **Injected**: the hard-link publication itself, through the `persist` seam. The OS fallback selection is not exercised |
| `already_missing_temporary_is_not_a_cleanup_failure_after_an_error` | **Injected**: the write failure, and a remove adapter that really deletes the temp and then returns NotFound. It shows NotFound gets no cleanup diagnostic after a primary error, and the directory ends up empty |
| `unsupported_publication_fails_without_final_or_temporary_file` | **Injected** `ErrorKind::Unsupported` from the `persist` seam; real temp cleanup. It shows `Could not publish …`, no final file and no temp |

The retained prior tests also passed on this source:
- primary-error-first cleanup ordering;
- real publish-time collision via the production persist;
- write failure;
- read-only destination;
- validation order and EXIF-stub non-call;
- byte identity.

## 5. Gates

| Item | Status |
|---|---|
| Revised source compiles; file_management 13 + 2 controls; lib 163; fmt; strict all-targets clippy | **Pass** (macOS host) |
| F1: unknown or denied temp status after a hard-link publication gives Ok with a warning, final kept | **Pass** (real permission failure, injected hard-link publication). Ok-with-warning keeps the published file; no universal retry/duplicate guarantee is claimed |
| Rename path NotFound gives no warning; NotFound after a primary error is not a cleanup failure; unsupported persist gives a clean failure | **Pass** (as scoped in §4) |
| Android `cfg` branch compile (pinned wrapper) | Not exercised |
| tempfile OS hard-link fallback actually selected; Fold/FUSE rename/link/sync behaviour | Not exercised |
| ContentResolver/JNI provider reads, picker, UI | Not exercised |
| R2b (Android local path, sidecars, deletion); R3 (terminal events, false Success) | Not exercised; not fixed |
| Native X-T5, Minolta | Not exercised |

Source is frozen at `e66dceb6`. The cache at `/private/tmp/rapidraw-integration-20261015/target` is released when this receipt is delivered.

Lead import note: expanded the incorrectly suffixed abbreviated formats.rs hash to its verified full value. Original worker validation receipt230d7de3 remains frozen in uri-revision; no source or executed-result change.
