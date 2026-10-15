# R1 — Precise content errors at existing image-open boundaries (receipt)

- Date: 2026-10-15
- Implementer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Base: detached `8877d7eaa14442814912a199c8de9abdfba5c8b9` (worktree `rapidraw-fuji-team-20261015/raf-guards`). It is a docs-only successor of `4c1e965a`.
- Design: Astra `lead/docs/integration/ANDROID-RAF-REPAIR-REVIEW-2026-10-15.md`, sha256 `70c16a9879cdeb169160f738988dda527ece968bae1ca7879eb32436fa5d60b1`. Only R1 is implemented.
- Writes:
  - `src-tauri/src/formats.rs`
  - `src-tauri/src/image_loader.rs`
  - this receipt
  - private `/private/tmp/rapidraw-r1-20261015`
- Nothing else: no Git staging, commit or push; no lock, dependency, JNI, decoder, profile, import or UI changes; no devices.

## 1. Change

`formats.rs` (+132; 54 lines of code, the rest tests):
- Adds `ImageContentIssue { Empty, RafHasJpegSignature }` and the pure function `validate_image_content(path, bytes)`.
  - Empty content → `Empty`.
  - Case-insensitive extension `raf` with content starting `FF D8 FF` → `RafHasJpegSignature`.
  - Everything else → `Ok`. There is no FUJIFILM allowlist, no JPEG check for other RAW extensions, and no internal-marker scan. Passing is not a claim that the content is valid.
- Adds `ImageContentIssue::open_error_message(path)`. It names only the basename and the observed condition:
  - `“DSCF5389.RAF” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.`
  - `“name” is empty and cannot be opened.`

`image_loader.rs` (+276/−1; 24 lines of code, the rest tests):
- New private `check_image_content(bytes, path, &cancel)`:
  - Valid content → `Ok`. Nothing changes for valid content, including cancellation order and side effects.
  - Content issue with the request already cancelled → `"Load cancelled"`, which keeps the existing precedence.
  - Otherwise → the precise message.
- It is called first in `load_base_image_from_bytes`, before `persist_exif_if_missing`, Apple RAW 9 and rawler.
- It is also called first in `load_and_composite_with_profiles`, before the selected-profile branch, so a selected profile cannot bypass it.
- There is no rewrite of rawler error strings, no rename or delete, no JPEG substitution, and no profile fallback. The `.jpg` + selected-profile rejection (`Camera profiles require RAW input`) and the TIFF/DNG/embedded-preview fallbacks are unchanged.

Full diff: `/private/tmp/rapidraw-r1-20261015/r1-source.diff`, sha256 `2b35732d010f43d30bd539884be0efb93f0b5bcfe850091c5e080d1d6fd4a8d1`.

| File | Base sha256 | After sha256 |
|---|---|---|
| `src-tauri/src/formats.rs` | `8f30478cf51e1f800b97ab926c36ad42124a93b0501fb78a2556775a793ffb17` | `66134f6ce2183036fb35798cfb7eefc9d7ef512d72cbb2d873b762c9b4f2e5bf` |
| `src-tauri/src/image_loader.rs` | `9eb42a235162d4816bf7b7145a42cd2f7a6b1fc9e63dc12b717aa28ab262f156` | `fcf43150b241ee0cfdf0d2be1983ac91bfc837059b38dfdf91e1c11177f31cf5` |
| `src-tauri/Cargo.lock` | `7ddd491a…765def` | unchanged |

## 2. Toolchain, build and artifacts

- `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1 (797e8a9bc 2026-08-05)`, aarch64-apple-darwin.
- `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target` (exclusive use). Env: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false CARGO_INCREMENTAL=0`.
- No clean and no dependency update. Two logged compilation invocations: formats tests finished in 2m36s; image-loader tests rebuilt RapidRAW in 1m05s; later control/full-suite runs reused the result. The logs do not establish why the rebuild occurred. The first build script downloaded and verified ONNX Runtime to ignored `src-tauri/resources/libonnxruntime.dylib`; this generated resource is not a source or dependency-lock change.
- Test binary: `target/debug/deps/rapidraw_lib-b8c68747df43d7c8`, sha256 `126499362bfedc7ba1490f0ed6650c84dfa9b5dd11b45360d0a9a1ee4a1a532a`.
- Target is 3.3 GiB. Free space was 36 GiB before and after, above the 6 GiB reserve.
- Fixtures: `python3 -I scripts/integration-fixtures.py --output-dir /private/tmp/rapidraw-r1-20261015/fixtures` (script used read-only). `synthetic-camera.dng` sha256 `b2116c8ba983cca8a183a904fb12217307f2c6c9aa0fa76d7be02f30be20b53f`.
- Not built: Android, GPU, or other platforms.

## 3. Commands and results (run in `src-tauri`)

| Command | Exit | Result | Log sha256 (`/private/tmp/rapidraw-r1-20261015/logs/`) |
|---|---|---|---|
| `cargo fmt --all -- --check` | 0 | clean | `fmt.log` `e38302ae…48bf` |
| `cargo test --locked --lib formats::tests -- --nocapture` | 0 | **5 passed**, 0 failed, 0 ignored | `formats-tests.log` `3d932eba…6bb2` |
| `cargo test --locked --lib image_loader::tests -- --nocapture` | 0 | **6 passed**, 0 failed, 3 ignored (R1 env controls) | `image-loader-tests.log` `1078ad20…16e6` |
| `RAPIDRAW_R1_FIXTURE_DIR=… RAPIDRAW_R1_NATIVE_RAF=… RAPIDRAW_R1_FAILED_PAYLOAD=… cargo test --locked --lib image_loader::tests -- --ignored --nocapture` | 0 | **3 passed**, 0 failed; 159 filtered out (only the R1 controls ran) | `image-loader-ignored-tests.log` `e32855c08fcd81bab7246006e9bdb7c5ffec3c4477b6d7487321d506d1a703e2` |
| `cargo test --locked --lib` | 0 | **150 passed**, 0 failed, 12 ignored (3 R1 + 9 pre-existing; none invoked) | `lib-suite.log` `284d14e1e6fcd20b16ec090fac590456a6f2cdf81cca970761c0dd3dd39aba22` |

The `--ignored` run was filtered to `image_loader::tests`, so no vendor, Adobe, upstream or GPU acceptance tests ran. If an R1 control's env var is missing, the test panics (`set RAPIDRAW_R1_…`); it does not pass as a skip. No compiler warnings point at either changed file.

## 4. What the tests exercise

| Test | Through | Evidence |
|---|---|---|
| `formats::tests` (5) | pure validator and messages | Empty for all names; `.RAF/.raf/.RaF/.rAf` + JPEG signature; `.jpg/.JPEG/.mrw/.MRW/.dng/x.raf.jpg/raf` + JPEG → Ok; TIFF with a JPEG at a nonzero offset, `FUJI`, short or unknown data under `.RAF` → Ok (no allowlist, no marker scan); message is basename only, no directory |
| `misnamed_jpeg_raf_is_rejected_before_any_decoder` | `load_base_image_from_bytes` (Apple RAW off **and on**) and `load_and_composite_with_profiles` (no profile and Look selected) | A real encoded 32×24 JPEG (image crate encoder), named `.RAF/.raf/.RaF`, produces the exact message |
| `same_jpeg_named_jpg_opens_and_keeps_profile_raw_requirement` | both entry points | The same bytes as `.jpg` open at 32×24 (Apple RAW off and on, and via the wrapper). `.jpg` + selected profile still gives `Camera profiles require RAW input` |
| `empty_content_is_reported_at_both_entry_points` | both entry points, `.RAF/.jpg/.dng`, Apple RAW off/on, with and without a profile | Empty message for each |
| `cancelled_request_still_reports_cancellation` | both entry points | A misnamed JPEG or empty input with a cancelled token gives `Load cancelled` |
| `jpeg_signature_under_other_raw_names_still_reaches_rawler` | `load_base_image_from_bytes`, `x.MRW` | Not caught by the guard; rawler's `No decoder found` (the synthetic JPEG has no Minolta EXIF), so the route is still eligible |
| `embedded_jpeg_preview_fallback_still_runs_under_raf_name` | `load_base_image_from_bytes`, `.RAF` and `.dng` | A TIFF (unknown Make, JPEG via tags 513/514) is rejected by rawler, and the existing preview fallback returns 32×24 |
| `synthetic_cfa_dng_decodes_identically_including_raf_alias` (env) | guarded open vs direct `raw_processing::develop_raw_image` | The script's CFA DNG decodes to 64×64 with **exact per-element f32 value equality** under `.dng` and `.RAF` names. Rawler stays authoritative and there is no Fuji-magic allowlist |
| `native_raf_control_decodes_identically_through_guard` (env) | same comparison | X-Pro2 `DSCF1893.RAF` (sha256 `e736b4af…19b1`, read-only) gives exact per-element f32 value equality (4000×6000 after orientation) under `.RAF` and `.raf` |
| `reported_failed_payload_gets_precise_error_and_opens_as_jpg` (env) | `load_base_image_from_bytes` | The user's private payload (sha256 `82bd1e04…c958c6`, read-only, not copied) named `DSCF5389.RAF` gives the exact precise message. Named `.jpg`, it opens at 7752×5178. Only dimensions were printed |

Five helper/message tests supplement six consumer tests and three opt-in controls; acceptance does not rely on helper tests alone. Each consumer test goes through the real open or profile function. No short fake `FUJIFILM` header is counted as a native decode.

Before vs after for native decode: the guarded output has equal dimensions and exact per-element f32 value equality with the unguarded `develop_raw_image` output on the same build. Numeric equality does not distinguish signed-zero bits. This is a code-level value equivalence. A separate pre-change binary was not built.

## 5. Gates

| Item | Status |
|---|---|
| fmt, formats (5), image_loader (6 + 3 env controls), default lib suite (150) | Pass (code tests, macOS host) |
| Misnamed JPEG `.RAF` rejected before EXIF persistence, Apple RAW and rawler, at both boundaries | Pass (code tests) |
| `.jpg` open, `.jpg`+profile rejection, TIFF preview fallback, synthetic DNG including `.RAF` alias, `.MRW` route | Pass (code tests) |
| X-Pro2 native RAF unchanged through guard | Pass (host, exact f32 value equality) |
| Native X-T5 RAF decode | Not exercised (no genuine X-T5 bytes) |
| Native Minolta JFIF RAW | Not exercised (no fixture; route eligibility shown with a synthetic JPEG) |
| Android build, device open of the staged DSCF5389.RAF, UI toast text | Not exercised (R1 is host-only; needs the device operator) |
| Profile, fidelity and performance acceptance | Not exercised (out of scope) |
| Import staging, transactional write, terminal import status (R2/R3) | Not exercised (not this task) |
| X-T5 RAW fails on Fold | User-reported |

Known limits (unchanged, per design):
- Direct CLI and late `lib.rs::profile_base_image` callers keep the existing pipeline errors.
- The thumbnail path in `file_management.rs:1464` calls `safe_embedded_preview_fallback` directly. It returns `None` for a bare JPEG; no new message is claimed there.
- Cached-file replacement races are not addressed.

Lead import note: corrected receipt prose after independent review (helper-test coverage, two compilation invocations/generated ONNX artifact, numeric equality versus bit identity). Accepted source hashes are unchanged; the original worker receipt hash remains recorded in the independent review. Combined-source rebuild and Android verification remain separate gates.
