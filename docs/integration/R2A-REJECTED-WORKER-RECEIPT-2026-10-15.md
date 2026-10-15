# R2a — Bounded Android content-URI import staging (receipt)

- Date: 2026-10-15
- Implementer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Base: `8a4f02bcec8a3edb567979462cbf022adde6f8b9` (full HEAD verified, clean at start; R1 committed). Worktree `rapidraw-fuji-team-20261015/uri-import`.
- Design: `docs/integration/ANDROID-RAF-REPAIR-REVIEW-2026-10-15.md` (Astra), section R2, URI part only. Uses the R1 helper `formats::{validate_image_content, ImageContentIssue}`; `formats.rs` itself is unchanged (`66134f6c…`).
- Writes:
  - `src-tauri/src/file_management.rs`
  - this receipt
  - private `/private/tmp/rapidraw-r2a-20261015`
- Nothing else: no Git staging, commit or push; no lock or dependency change; no device commands; no Android, GPU or all-platform build.

## 1. Change (`file_management.rs`, +597/−41)

The Android `content://` branch of `import_files` now does only this:
1. Resolve the display name, as before.
2. `read_android_content_uri`.
3. Hand off to `import_content_bytes`.
4. Keep the `delete_after_import` URI skip log exactly as before.

Everything else is in new non-`cfg` helpers in the same file. They are host-testable and carry `#[cfg_attr(not(target_os = "android"), allow(dead_code))]`.

- **`validate_import_read(display_name, read, source_label)`.** Runs on the **original resolved display name** and the **bytes actually read**.
  - Read `Err`: `Could not read “name”: <underlying error>`. The content-URI text is replaced by "the selected item"; it is never reported as empty.
  - `Ok(empty)`: `No image data was read for “name”. Select the original file and try again.`
  - `.raf` (any case) with a leading `FF D8 FF`: the R1 message, `“name” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.`
  - Anything else: accepted unchanged and left to the decoders. There is no FUJIFILM allowlist, no check of other RAW extensions, no marker scan, and no rename or reinterpretation.
- **`import_content_bytes(...)`.** This is the former branch body, moved, with the order changed:
  1. Validate (above).
  2. Get the creation date (EXIF).
  3. Apply organise-by-date.
  4. `create_dir_all`.
  5. Apply the filename template.
  6. Publish.

  The EXIF date function is injected (`&exif_processing::get_creation_date_from_bytes` on Android). That function is `#[cfg(target_os = "android")]` in `exif_processing.rs`, which is outside my ownership, so tests pass a recording stub. Validation therefore runs before any EXIF parsing, template or folder creation, and before the template can rename the file.
- **`publish_import_bytes(dest_dir, file_name, bytes, ops)`.** Publishes the same immutable validated buffer:
  1. If the destination already exists (`symlink_metadata`), fail with the existing message `File already exists at destination: …`.
  2. Create a hidden temporary file in the same directory: `tempfile::Builder` with prefix `.rapidraw-import-` and suffix `.tmp`, then `keep()` so cleanup is explicit. On failure: `Could not stage “name”: …`.
  3. `write_all`, `flush`, `sync_all`. On failure: `Could not write “name”: …`.
  4. `TempPath::try_from_path(temp)?.persist_noclobber(dest)`. An `AlreadyExists` error, including a collision created after staging, gives the existing "File already exists" message. Any other error gives `Could not publish “name”: …`.
  5. On any failure: remove only the owned temporary file. If that removal fails, the primary error is kept first and `The temporary file “.rapidraw-import-….tmp” could not be removed: <err>` is appended (basename only). No final file is written, and a pre-existing target or the source is never touched.
  6. After success: if the temporary name still exists (tempfile's hard-link fallback), remove it. If that fails, the import still counts as published (the final file is complete) and a warning `Imported “name”, but the temporary file “…” could not be removed: …` is returned and logged with `log::warn!`.
- There is **no** `rename` or `fs::write` fallback anywhere. The replaceable steps (`write`, `before_publish`, `persist`, `remove`) live in `ImportStagingOps`. Production uses `ImportStagingOps::default()`; tests replace single steps.

Exact no-clobber behaviour of installed tempfile 3.27.0 (`src/file/imp/unix.rs:94-134`):
- On android, linux and macOS it first calls `renameat_with(NOREPLACE)`, which is atomic and never replaces.
- On `NOSYS` or `INVAL` it falls back to `hard_link`, which fails if the target exists, and then `unlink(temp)` with **errors ignored**. That is why the step-6 check exists. On this path the target is never replaced, but a crash between link and unlink can leave the hidden temporary file.
- A hidden leftover (leading `.`, `.tmp` extension) is excluded by `is_supported_image_file`, so it is never listed as an image. A test asserts this.
- If the Fold's storage supports neither NOREPLACE rename nor hard links (possible on FUSE or sdcardfs), publication **fails** with `Could not publish “name”: …` and the temporary file is removed. It never overwrites. **This host run does not prove Fold filesystem behaviour.**

Explicitly not changed, per ownership:
- The Android local-path branch and all desktop behaviour.
- Sidecar and deletion semantics (R2b).
- `formats.rs`, `image_loader.rs`, JNI and `android_integration.rs`, locks, `lib.rs`.
- **Terminal events (R3).** The backend still emits `import-error`, then an unconditional `import-complete`, and the frontend shows Success. That false Success **persists until R3**.
- The existing `eprintln!("Failed to import {source}: {e}")` in the loop still prints the URI (R3 territory).

Full diff: `/private/tmp/rapidraw-r2a-20261015/r2a-source.diff`, sha256 `0a101f18ec938c3c924ae124dbad94c623cdb14f06a9e911b96103788506a646`.

| File | Base sha256 | After sha256 |
|---|---|---|
| `src-tauri/src/file_management.rs` | `227a82b96e02a4304c909214660791b4ab5fea9de496c58f3780a25a0f7c886f` | `dff9be3dec3f5376b74ecd4a61ca8bb6388c3d8aab158c1193a4efe8cb1f3188` |
| `src-tauri/src/formats.rs` (R1 canonical) | `66134f6ce2183036fb35798cfb7eefc9d7ef512d72cbb2d873b762c9b4f2e5bf` | unchanged |
| `src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` | unchanged |

## 2. Toolchain, build and artifacts

- rustc 1.98.1 (48a229cea 2026-09-01), cargo 1.98.1, aarch64-apple-darwin. Run inside `src-tauri`.
- Env: `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target` (exclusive) plus `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false CARGO_INCREMENTAL=0`.
- No clean and no dependency change. Target is 3.3 GiB; 37 GiB free before and after.
- Test binary: `target/debug/deps/rapidraw_lib-b8c68747df43d7c8`, sha256 `1f736a0627c7796cb7395cbdaec69d24640cd743a5e17310d61a4b9e5937cab2`.
- Fixtures: `python3 -I scripts/integration-fixtures.py --output-dir /private/tmp/rapidraw-r2a-20261015/fixtures` (script used read-only). `synthetic-camera.dng` sha256 `b2116c8b…20b53f`.
- First test attempt: compile error E0425. `exif_processing::get_creation_date_from_bytes` is Android-only, so I fixed it by injecting the date function. The same attempt raised a deprecation warning for `TempPath::from_path`, fixed by switching to `try_from_path`. Log kept: `fm-tests-attempt1-compile-error.log`.

## 3. Commands and results

| Command | Exit | Result | Log sha256 (`/private/tmp/rapidraw-r2a-20261015/logs/`) |
|---|---|---|---|
| `cargo fmt --all -- --check` | 0 | clean | `fmt.log` `e38302ae…48bf` |
| `cargo test --locked --lib file_management::tests -- --nocapture` | 0 | **9 passed**, 0 failed, 2 ignored (R2a env controls); no compiler warnings | `fm-tests.log` `3f1e8b91bffbf667aae6e6357db508069c74f3497a19d3598a065180b4037493` |
| `RAPIDRAW_R2A_FIXTURE_DIR=… RAPIDRAW_R2A_NATIVE_RAF=… cargo test --locked --lib file_management::tests -- --ignored --nocapture` | 0 | **2 passed**; 171 filtered out (only the R2a controls ran; they panic if the env var is missing) | `fm-ignored-tests.log` `23939b6440dad7f1e75f093976f046a279ccb5f730ce6a9bf02c10157c4ca1f1` |
| `cargo test --locked --lib` | 0 | **159 passed**, 0 failed, 14 ignored (12 pre-existing incl. R1 + 2 R2a; none invoked) | `lib-suite.log` `acc8f1f4b5d00f2f5ad976b523654697bcb592289c4407ec0fb1da7f87020c2f` |
| `cargo clippy --locked --lib --tests` | 0 | no warnings | `clippy.log` `6ade3351…208a` |
| `cargo clippy --locked --lib --tests -- -D warnings` | 0 | no warnings; fresh lib and test check artifacts at 17:27 | `clippy-strict.log` `7549aa26c62875c6608de06e6daca0bc1279bee0ab18c1a354416674e1126f84` |

## 4. What the tests exercise

All tests go through the real `import_content_bytes` (validate → date → folders → template → publish) or `publish_import_bytes`. None of them only checks the prefix helper.

| Test | Evidence |
|---|---|
| `jpeg_named_raf_is_not_imported_in_any_case_or_template` | A real encoded 32×24 JPEG named `.RAF/.raf/.RaF`, with the `{original_filename}` template and with a `renamed_{sequence}` + organise-by-date template, gives the exact R1 message. The EXIF date stub was **never called**, the library folder was **never created**, and the tree holds no files, temporary or final |
| `same_jpeg_named_jpg_is_imported_byte_identical` | The same bytes as `.jpg`, both flat and organise-by-date, import byte-identical. The only file present is the final one, with no leftover temp |
| `empty_read_and_read_failure_are_distinct_and_write_nothing` | `Ok(empty)` gives the empty message. A read `Err` containing the URI gives `Could not read …: … the selected item`, with no `content://`. No date call and no folder |
| `existing_target_is_never_replaced` | A pre-existing `DSCF5389.jpg` keeps its original bytes; the error is "File already exists"; no temp is left |
| `collision_created_after_staging_is_not_replaced` | A `before_publish` hook creates the target after the temp is written. Real `persist_noclobber` refuses it, the target keeps the racing content, and the temp is removed |
| `write_failure_leaves_no_final_or_temporary_file` | A write fails after half the bytes. Exact error; the directory is empty |
| `cleanup_failure_is_reported_after_the_primary_error` | Write failure plus removal failure. The message is the primary error followed by the temp-basename diagnostic. The one leftover is hidden, `!is_supported_image_file`, and there is no final file |
| `hard_link_fallback_leftover_is_removed_or_reported` | A simulated link-without-unlink persist: the leftover is removed, leaving only the final file. With removal failing, the result is Ok with a precise `cleanup_warning`. Final bytes are exact |
| `unwritable_destination_fails_without_output` (unix) | A 0o555 directory gives `Could not stage “a.jpg”: …`. Write was never attempted and nothing was created |
| `synthetic_dng_imports_byte_identical_including_raf_alias` (env) | The synthetic CFA DNG as `.dng` and as `.RAF` imports with sha256 `b2116c8b…20b53f` both times, and the source is unchanged. There is no FUJIFILM allowlist |
| `native_raf_imports_byte_identical` (env) | The genuine X-Pro2 `DSCF1893.RAF` (read-only) imports with sha256 `e736b4af4d80fa12418f92c018ac99b5c5c96d8e772ce3a34fc2cb9c2a4619b1`, identical; the source is unchanged |

## 5. Gates

| Item | Status |
|---|---|
| fmt; file_management::tests 9 + 2 env controls; default lib 159; clippy plain and `-D warnings` | Pass (macOS host code tests) |
| Validation before EXIF, template and folder creation; RAF/JPEG and empty rejected with nothing written; read errors distinct and URI-redacted | Pass (host, through the actual helper) |
| No-clobber publish incl. publish-time race; write/stage failures leave no final or temp; cleanup failure reported after the primary error | Pass (host, injected adapters + real tempfile persist) |
| Genuine X-Pro2 RAF and synthetic DNG (incl. `.RAF` alias) imported byte-identical | Pass (host) |
| `#[cfg(target_os = "android")]` branch compiled | **Not exercised.** No Android build in R2a. The branch is 22 lines of calls into host-tested helpers, but this needs the pinned Android build-wrapper gate |
| Fold filesystem: NOREPLACE rename, hard-link availability, sync behaviour on `Android/media/<pkg>/.library` | Not exercised (host APFS only) |
| Real ContentResolver/provider reads (JNI) and picker | Not exercised; the host seam is not provider or UI acceptance |
| Native X-T5 RAF; Minolta RAW | Not exercised |
| Terminal import status / false Success (R3); Android local-path branch, sidecars, deletion (R2b) | Not exercised; explicitly **not fixed** |
| X-T5 RAW fails on Fold | User-reported |

Cache: `/private/tmp/rapidraw-integration-20261015/target` is released when this receipt is delivered.
