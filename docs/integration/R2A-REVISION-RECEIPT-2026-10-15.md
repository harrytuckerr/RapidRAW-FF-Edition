# R2a revision — precise post-publication cleanup reporting (receipt)

- Date: 2026-10-15
- Implementer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Worktree: `rapidraw-fuji-team-20261015/uri-revision`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9` (verified).
- Repair input: uncommitted `file_management.rs` `dff9be3dec3f5376b74ecd4a61ca8bb6388c3d8aab158c1193a4efe8cb1f3188`, verified before editing. Frozen `uri-import` is untouched and still hashes to `dff9be3d…`.
- Review: `lead/docs/integration/R2A-INDEPENDENT-REVIEW-2026-10-15.md`, sha256 `e9eb2d6b71c81ec9a9fee292da092a17734d9a5b3ff0f17a5e49ce0eebc1eb96`. Only F1 (P2) is corrected.
- Writes:
  - `src-tauri/src/file_management.rs`
  - this receipt
  - private `/private/tmp/rapidraw-r2a-revision-20261015` (diffs only)
- **No cargo, compile, test or test-binary execution.** No shared target or cache access, no devices, no Android build, no other source, no dependency or lock change, no Git stage, commit or push.

## 1. Correction (production)

In `publish_import_bytes`:

- New `remove_temp` closure: calls `(ops.remove)(&temp_path)` directly.
  - `Ok` or `ErrorKind::NotFound` → clean (`None`).
  - **Any other error** → `Some(err)`. This includes PermissionDenied from an unsearchable directory, or an I/O error.
- **After a successful publish**, the `fs::symlink_metadata(&temp_path).is_ok()` gate is gone. Removal is always attempted directly, which closes the stat-versus-cleanup gap:
  - Rename path: the temp is already gone, so NotFound and no warning.
  - Hard-link fallback with removal succeeding: no warning.
  - Any other result: `Imported “name”, but the temporary file “.rapidraw-import-….tmp” could not be removed: <err>`.

  The result stays `Ok(PublishedImport)` with that warning. The complete final file is kept and never deleted. Nothing is turned into a "nothing imported" failure, so a retry cannot create a duplicate.
- **Failure cleanup** (`with_cleanup`) now uses the same `remove_temp`. An already-absent temp (NotFound) is no longer reported as a cleanup failure. Other removal errors are still appended **after** the primary error, which comes first.
- Unchanged:
  - validation and its ordering;
  - the same-buffer write (`write_all`/`flush`/`sync_all`);
  - no-clobber `persist_noclobber` publication and the pre-existing-target check;
  - no overwrite fallback;
  - the Android call site and `delete_after_import` skip;
  - local-path and desktop branches, events (R3), JNI, decoder, colour.

No new test seam was needed; the existing `ImportStagingOps` `persist` and `remove` hooks are used.

## 2. New regression tests (**written, NOT executed**)

All go through the actual `publish_import_bytes`.

| Test | Intent |
|---|---|
| `renamed_temporary_reports_not_found_as_clean` | Default real no-replace persist (rename) plus a recording `remove` that calls `fs::remove_file`. Expects exactly one removal attempt returning `NotFound`, `cleanup_warning == None`, only the final file present, and exact bytes |
| `unknown_temporary_status_after_hard_link_publication_is_a_warning` (`cfg(unix)`) | The `persist` adapter creates a real hard link, then sets the private library directory to `0o000` and returns Ok. This is F1's state: the temp's status can be neither read nor changed. Expects `Ok` (a complete published file is still a success), a warning starting `Imported “a.jpg”, but the temporary file “.rapidraw-import-`, final bytes equal the input, and the directory then holds the final file plus the reported temp. Permissions are restored right after the call and also by a `Drop` guard, so they are restored on panic too. The test assumes a non-root user, because root bypasses directory permissions |
| `already_missing_temporary_is_not_a_cleanup_failure_after_an_error` | Injected write failure; the `remove` adapter deletes the temp and then reports NotFound. Expects only the primary error `Could not write “a.jpg”: disk full (injected)` with no cleanup diagnostic, and an empty directory |
| `unsupported_publication_fails_without_final_or_temporary_file` | The `persist` adapter returns `ErrorKind::Unsupported`. Expects `Could not publish “a.jpg”: …`, no cleanup diagnostic, and an empty directory, so no final file and no temp |

Prior tests are kept unchanged and still cover:
- primary-error-first ordering (`cleanup_failure_is_reported_after_the_primary_error`);
- hard-link leftover removed or reported;
- the real publish-time collision;
- write failure;
- read-only destination;
- validation order;
- byte identity.

The module now has 15 test functions (13 default, plus 2 env-gated ignored controls).

Two closures returning `Err(..)` have explicit `-> std::io::Result<()>` annotations, so their types do not depend on inference from the later coercion. Compilation is still unverified.

## 3. Evidence boundary: new vs prior

- **This revision:** no build, no test run, no clippy. The new tests and the corrected source are **Not exercised**. Compilation is not proven.
- **Prior R2a worker results apply only to the previous source `dff9be3d…`, not to this revision.** Those were:
  - `file_management::tests`: 9 passed, 2 ignored;
  - 2 env controls passed;
  - default lib 159 passed, 14 ignored;
  - fmt and strict clippy exit 0.

  Logs are in `/private/tmp/rapidraw-r2a-20261015/logs/`.
- **Disclosure from the original R2a run.** The first build (`fm-tests-attempt1-compile-error.log`, sha256 `32ebe808…fa3c`, lines 2-6) ran the project build script. It downloaded the ONNX Runtime library (`libonnxruntime-macos-aarch64.dylib`, v1.22.0, from the project's Hugging Face URL) into `uri-import/src-tauri/resources/libonnxruntime.dylib`: 33,481,272 bytes, sha256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`. The file is git-ignored (`.gitignore:31`). It is a generated build-script artifact, not a tracked source or dependency change. The original R2a receipt did not mention it. This revision ran no build, so no such file exists in `uri-revision`.

## 4. Commands run (all exit 0)

```
git -C uri-revision rev-parse HEAD ; git status --short ; shasum -a 256 src-tauri/src/file_management.rs
shasum -a 256 lead/docs/integration/R2A-INDEPENDENT-REVIEW-2026-10-15.md
rustfmt --edition 2024 src-tauri/src/file_management.rs
rustfmt --edition 2024 --check src-tauri/src/file_management.rs
git diff --check
git diff > /private/tmp/rapidraw-r2a-revision-20261015/r2a-revision-full.diff
diff -u uri-import/src-tauri/src/file_management.rs uri-revision/src-tauri/src/file_management.rs > …/r2a-revision-incremental.diff   (exit 1 = differences, expected)
```

There is no `rustfmt.toml` in the repo; edition 2024 matches `Cargo.toml`.

## 5. Hashes

| Artifact | sha256 |
|---|---|
| `uri-revision/src-tauri/src/file_management.rs` (revised) | `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7` |
| Repair input (`uri-import`, frozen, unchanged) | `dff9be3dec3f5376b74ecd4a61ca8bb6388c3d8aab158c1193a4efe8cb1f3188` |
| Full diff vs HEAD (`r2a-revision-full.diff`, +720/−41) | `c1e84f20e638688b954545d3e1000a7916cc7bdbc81bbf2d1e33b8fb6b0b8f1d` |
| Incremental diff vs `dff9be3d` (`r2a-revision-incremental.diff`, 175 lines, 3 hunks) | `f60132b91a353c6aac0be74455f513662de4b6561891306fd81c2d7bddab24d6` |
| `formats.rs`, `Cargo.lock` | unchanged (`66134f6c…`, `7ddd491a…`) |

## 6. Gates

| Item | Status |
|---|---|
| F1 control-flow fix (direct removal; only Ok or NotFound means clean; warning otherwise; final kept; Ok with warning) | Pass (static, this edit) |
| Primary-error-first ordering kept; NotFound not reported on failure cleanup | Pass (static) |
| rustfmt check, `git diff --check` | Pass |
| Compilation, the 4 new tests, the full file_management and lib suites, clippy | **Not exercised**; awaits a separate validation assignment |
| Android cfg compile, Fold filesystem, provider/JNI, UI | Not exercised |
| R3 false Success, R2b local path and sidecars | Not fixed (out of scope) |
