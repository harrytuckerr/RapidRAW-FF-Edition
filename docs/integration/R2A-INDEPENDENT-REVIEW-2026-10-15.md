# R2a independent URI staging review — 15 October 2026

**Verdict: Revise for one bounded cleanup-reporting defect (P2).** The URI-only validation, same-buffer staging and no-clobber publication design otherwise matches the assigned slice. A post-publication metadata error is incorrectly treated as proof that the temporary path is absent. Fix that branch and refresh its focused evidence before accepting R2a. No R2b/local-path or R3/event implementation is requested here.

Reviewer: Codex CLI - Astra, actual model `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/uri-import`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9`. Only this new report is written in `raf-review/docs/integration/`. Read-only review: no source edits, Cargo/build/test-binary execution, filesystem failure experiment, cache mutation, private photo reading/rendering or device operation.

## Frozen input identity

All assigned hashes independently recomputed. Full receipt and full patch read locally. Current `git diff -- src-tauri/src/file_management.rs` byte-equals the preserved diff. Only file_management.rs is tracked-modified; receipt is untracked.

| Input | SHA-256 |
|---|---|
| `uri-import/src-tauri/src/file_management.rs` | `dff9be3dec3f5376b74ecd4a61ca8bb6388c3d8aab158c1193a4efe8cb1f3188` |
| `uri-import/docs/integration/R2A-URI-IMPORT-RECEIPT-2026-10-15.md` | `38529accc831cc3ffad016e2151600e5ce8150d9ebabd079326469578a15892b` |
| `/private/tmp/rapidraw-r2a-20261015/r2a-source.diff` | `0a101f18ec938c3c924ae124dbad94c623cdb14f06a9e911b96103788506a646` |
| `/private/tmp/rapidraw-r2a-20261015/rapidraw-r2a-test-binary` | `1f736a0627c7796cb7395cbdaec69d24640cd743a5e17310d61a4b9e5937cab2` |
| R1 `formats.rs`, unchanged | `66134f6ce2183036fb35798cfb7eefc9d7ef512d72cbb2d873b762c9b4f2e5bf` |
| Cargo.lock, unchanged | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

The preserved binary was hashed only. This does not constitute a new build or independently reproduce its source provenance.

## F1 — P2: uncertain temporary-file status silently becomes “no cleanup warning”

**Location:** modified `src-tauri/src/file_management.rs:3861–3871`, especially `fs::symlink_metadata(&temp_path).is_ok()` at3862.

**Reproducer/state:** successful hard-link publication leaves both final and temporary names, as tempfile explicitly permits. Directory-search permission is then lost, or the filesystem returns an I/O error when checking the temporary name. `symlink_metadata` returns `PermissionDenied`/another non-NotFound error; `.is_ok()` is false, so no removal is attempted and `cleanup_warning=None`. `publish_import_bytes` returns published success without disclosing that temporary cleanup is unverified. The hidden file can remain. The same ambiguous stat error after rename also prevents knowing whether cleanup is needed; absence has not been proved.

This is **static control-flow evidence**, not an executed filesystem reproduction. A deterministic worker test can use the existing persist seam to create a real hard link, then temporarily deny search permission on its private test directory before returning success; restore permissions safely after the helper returns. Alternatively add a minimal metadata-error seam. Current test `hard_link_fallback_leftover_is_removed_or_reported` at4808–4833 leaves metadata readable and therefore cannot catch the defect.

**Consequence:** violates the promised cleanup-warning contract; hidden orphaned photo bytes/storage can be left without a diagnostic. It does not show replacement of the final target, corruption of final bytes, or loss of the original.

**Smallest correction:** after successful publication, attempt removal of the owned temporary path directly; `Ok` and **only `ErrorKind::NotFound`** mean already clean, any other error returns `Some(cleanup_warning)`. This avoids the stat/cleanup check gap. An explicit three-way metadata match is also acceptable: present→remove, NotFound→none, every other stat failure→warning. Preserve the successfully published final file and successful publication result with warning; do not convert it into a retryable “nothing imported” failure or delete the final file. Use the same NotFound handling where appropriate in the failure-cleanup helper to avoid falsely calling an already absent temporary a cleanup failure. No overwrite fallback.

Required regression: complete final bytes survive, original/pre-existing targets remain untouched, unknown/denied temporary status yields a warning, successful cleanup and rename-path NotFound yield none. Keep the existing primary-error-first behavior on write/publish failure. Do not broaden to UI events or other import routes in this correction.

## Accepted source behavior and exact boundaries

Line numbers below are in modified `uri-import/src-tauri/src/file_management.rs`.

- **Original-name validation precedes side effects.** `validate_import_read:3775–3797` distinguishes real read Err from successful empty bytes, applies the existing R1 narrow RAF/JPEG check and returns the same Vec. `import_content_bytes:3896` runs before EXIF/date at3898, directory creation at3911, template at3914 and publication at3927. Original resolved display name is checked before output naming. No camera-prefix allowlist, non-RAF JPEG rejection, conversion or reinterpretation.
- **Read errors remain errors.** Android caller at3955–3966 passes the `read_android_content_uri` Result into the helper. A successful zero-length read is a separate empty-content failure. Exact occurrences of the supplied URI are redacted in that helper's underlying read error; this is literal replacement, not a general URI-redaction guarantee for every outer log or display-name resolution failure. JNI/provider behavior is unchanged and untested here.
- **Same validated data is written.** The validated Vec is held locally and borrowed as an immutable slice through EXIF and publication. There is no reopening the provider or rereading a potentially different source before the write. No URI-source delete is added; the existing skip at3968–3972 remains.
- **Staging and pre-publication errors.** A unique hidden `.rapidraw-import-*.tmp` is created in the destination directory. `write_all`, `flush`, then `sync_all` at3724–3728 return errors through the same `written` branch at3844–3847. File handle is dropped before explicit removal. Failure cleanup removes the recorded temporary path only and appends removal errors after the primary error; it never removes a collision destination or original.
- **Publication is authoritative for collision safety.** `symlink_metadata(dest)` at3819 recognizes existing files/symlinks; its other errors are currently ignored as a preflight hint. This is not proof of absence, but `persist_noclobber` still makes the final no-replace decision, so that preflight does not introduce an overwrite path. Reporting non-NotFound preflight errors explicitly would be clearer, but is not a separate demonstrated no-clobber defect.
- **Local/desktop and terminal-event scope preserved.** Independent text comparison from `let (source_path, source_sidecar)` through the end of `import_files` is identical to HEAD. The other image/thumbnail code before the new helpers is unchanged apart from imports. Android local paths, sidecars/deletion and R3's false Success remain expressly pending; their existing problems are not grounds to expand R2a now.
- **Hidden leftovers remain hidden.** Existing `is_supported_image_file` rejects leading-dot names, and `.tmp` is not an image extension. This prevents gallery classification, not a guarantee of cleanup or durable publication.

## Installed tempfile3.27.0 and ownership analysis

Read local source at `<HOME>/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tempfile-3.27.0/src/file/`:

- `imp/unix.rs:94–134`: Android/macOS/Linux no-replace publication first uses `renameat_with(...,NOREPLACE)`. NOSYS/INVAL fall back to `hard_link`; other errors return directly. Hard-link creation refuses an existing destination. Its subsequent unlink error is deliberately ignored. There is no replace-rename fallback in the reviewed wrapper or this `overwrite=false` implementation.
- `mod.rs:857–865,299–313`: initial `NamedTempFile.keep()` transfers file/path and disables automatic path cleanup. Unix `imp::keep` at137–138 always returns Ok; Windows-specific keep failure is not an Android-production path in this slice.
- `mod.rs:368–385`: `TempPath::try_from_path` reconstructs RAII ownership of a path; it does not verify file existence. On persist success, `mod.rs:260–267` disarms that owner. On failure, the returned error retains the TempPath; the wrapper at3741–3744 calls `keep()` to disarm it so caller cleanup can report its own errors. For Android/macOS Unix semantics this avoids silent double cleanup. No source deletion is transferred to that owner.
- Explicit keep means a panic/process death between creation and normal cleanup can leave a hidden temporary. File `sync_all` occurs **before** publication; there is no directory fsync after publication. Thus no-clobber/completed-byte publication is the supported claim, **not** guaranteed persistence across power loss, filesystem failure or crash recovery. No directory-sync redesign is requested in this bounded review.

If Fold storage does not support the attempted no-replace primitive/fallback, the actual helper returns a publication error and attempts temp cleanup; there is no fallback that overwrites. Host APFS success does not prove Android/FUSE behavior. The Android callsite still needs the pinned-wrapper build and actual device filesystem/provider gate. A failure message with retained primary error is appropriate; speculative filesystem workarounds are outside this review.

## Test and receipt evidence

**Reviewer executed no tests or failure injection.** Existing logs were read and rehashed, not regenerated. The recorded worker tests call real `import_content_bytes`/`publish_import_bytes`. Injected date counting and individual write/persist/remove hooks are legitimate seam tests; they do not establish actual ContentResolver, Android EXIF, Fold filesystem or UI acceptance.

| Worker run inspected | Result / limitation |
|---|---|
| `cargo test --locked --lib file_management::tests -- --nocapture` | 9 passed,2 ignored. Covers actual helper validation/order, byte equality, existing target, real publish-time collision, injected partial-write/removal errors, simulated link-without-unlink and non-writable host directory. |
| Filtered R2a `--ignored --nocapture` with explicit env inputs | 2 passed. Synthetic DNG (including RAF alias) and X-Pro2 input/output/source SHA equality recorded. Private files were not opened by reviewer. |
| `cargo test --locked --lib` | 159 passed,14 ignored; no claim ignored controls all passed. |
| Format and plain/strict Clippy | Logs exit0; strict command includes `-D warnings`. Cargo build-script status warnings are distinguishable from Rust diagnostics. |
| Earlier attempt | Preserved E0425 log accurately disclosed; injecting the Android-only date function resolved host compilation. |

Missing coverage relevant to the bounded repair: ambiguous post-publish stat errors (F1); separate flush/sync-failure and unsupported-persist-error cases are not executed in the current tests. Their source `?` paths look correct, but do not label them runtime-proven. During the repair the worker can use small deterministic injected errors through the existing helper seams; no full-filesystem exhaustion or large build is needed for these cases. The real collision test uses default `persist_without_replacing`; the hard-link-leftover test deliberately substitutes the persist step and does not prove the OS fallback was selected.

Verified log hashes, `/private/tmp/rapidraw-r2a-20261015/logs/`:

- `fm-tests.log`: `3f1e8b91bffbf667aae6e6357db508069c74f3497a19d3598a065180b4037493`
- `fm-ignored-tests.log`: `23939b6440dad7f1e75f093976f046a279ccb5f730ce6a9bf02c10157c4ca1f1`
- `lib-suite.log`: `acc8f1f4b5d00f2f5ad976b523654697bcb592289c4407ec0fb1da7f87020c2f`
- `clippy-strict.log`: `7549aa26c62875c6608de06e6daca0bc1279bee0ab18c1a354416674e1126f84`
- `fm-tests-attempt1-compile-error.log`: `32ebe8087d2db2fbbe6d68b15538b9d29c8c3adeea102b8c9f999da2a7f6fa3c`

The receipt otherwise labels host versus Android/provider evidence candidly and explicitly retains R3's false Success. Its unconditional claim that post-success leftover cleanup is either removed or reported needs qualification until F1 is fixed. Initial compile log also records the existing build script downloading its ignored ONNX runtime into `src-tauri/resources/libonnxruntime.dylib`; disclose that generated artifact when describing writes, as in prior receipts. This is a receipt clarification, not a tracked dependency/source mutation finding.

## Receipt and disposition

**Fail — static:** P2 cleanup uncertainty silently omitted. **Pass — static:** frozen identity/scope, validation order, buffer identity, read-error separation, no-replace publication and ordinary ownership/error paths; unchanged local/desktop/event body; `git diff --check`. **Pass — recorded worker executions:**9 default+2controls,159 default library and lint/format as above. **Not exercised by reviewer:** every test/build, file write/publish/failure experiment, Android cfg compilation, Fold/provider/UI, private photo bytes or source/provider original preservation. **User-reported:** no new user evidence adopted.

Read-only commands: `git rev-parse HEAD`, `git status --short`, `git diff`, `git show HEAD:...`, `git diff --check`, Python/subprocess text/hash comparisons (no file writes), `nl`/`rg`/`cat`/`tail`, `shasum -a 256`. No shared build-cache access/mutation or test-binary execution. Only this report is a new persistent artifact; its SHA is in the delivery message.

Next consumer: lead assigns a narrow same-file correction and focused regression, then freezes a new source/diff/receipt for re-review. The current source is not accepted as fully satisfying R2a cleanup reporting. Android build/provider/Fold gates remain required after source acceptance. Stop here; no R2b/R3 scope expansion or source editing by reviewer.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `e9eb2d6b71c81ec9a9fee292da092a17734d9a5b3ff0f17a5e49ce0eebc1eb96` (14388 bytes).
