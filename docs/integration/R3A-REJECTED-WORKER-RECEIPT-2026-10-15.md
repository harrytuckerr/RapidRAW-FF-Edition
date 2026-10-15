# R3A — Native import terminal outcomes and exclusive-run guard (source-only receipt)

- Date: 2026-10-15
- Implementer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Worktree: `rapidraw-fuji-team-20261015/import-terminal`, detached HEAD `ea191124c9a52c761336a307fb1bcb09da4e84fc` (verified)
- Contract: `docs/integration/R3-TERMINAL-CONTRACT-2026-10-15.md`, sha256 `050005a661b88d7cec03252ae500970452b7cae599b409edc03507991aadab88` (verified, read completely). This implements the **native producer portion only**.
- Owned writes: `src-tauri/src/file_management.rs`, this receipt, and private `/private/tmp/rapidraw-r3a-20261015/`.
- **No cargo, build, test, cache, device, npm, `node_modules`, Git stage/commit/push.** Frontend files, `lib.rs`, locks and packages are untouched.

## 1. Files

| File | Before sha256 | After sha256 |
|---|---|---|
| `src-tauri/src/file_management.rs` | `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7` (R2a source base) | `0fa55c4ff247519e4e01962b918725b539cf0bfdc2d0fdd36ae16b1f9c08287d` |

Full diff: `/private/tmp/rapidraw-r3a-20261015/r3a-full.diff`, sha256 `e6d89648a52d97db72b70f07c94d0e2e81a2617a40b80006d2a80ce3b09ef0a0`. +913/−139. `git diff --check` clean. `rustfmt --edition 2024 --check src/file_management.rs` exit 0, run on the owned file only.

Diff hunks touch only three regions:
1. Line 12: `use std::sync::atomic::Ordering;` → `use std::sync::atomic::{AtomicBool, Ordering};`.
2. The former `import_files` command, base lines 3939–4096.
3. A new `#[cfg(test)] mod r3_terminal_tests` appended at the end.

Verified by script against the saved base copy (`base-file_management.rs`, sha256 = `e66dceb6…`):
- Base lines 1–3938 are identical except line 12. That covers all R2a helpers: `ImportStagingOps`, `validate_import_read`, `publish_import_bytes`, `import_content_bytes`.
- Everything from `pub fn generate_filename_from_template` to the base end is identical.

## 2. Native implementation (contract §"Native producer steps")

**Step 1, admission:**
- `validate_import_request` checks, before any event or work:
  - non-empty source list;
  - non-blank destination (`trim()`);
  - canonical run ID, meaning `Uuid::parse_str` succeeds and `hyphenated().to_string()` equals the input exactly, so lowercase hyphenated only (uppercase, simple and braced forms are rejected);
  - count ≤ `Number.MAX_SAFE_INTEGER`.
- `admit_import_run` validates, then calls `ImportRunGuard::try_acquire(&IMPORT_RUN_ACTIVE)`:
  - `static AtomicBool`, `compare_exchange(false, true, Acquire, Relaxed)`;
  - busy → `Err("An import is already running.")`, with no events and no writes;
  - `Drop` stores `false` with `Release`, which also runs on unwind.
- `import_files` gains the required `run_id: String` argument (JS `runId`). It keeps `sourcePaths`, `destinationFolder`, `settings` and `Result<(), String>`. The guard is moved into the `spawn_blocking` worker as `_guard` and dropped after `run_import_batch` returns, i.e. after the terminal emit attempt. Invoke resolution still means **queued only**.

**Steps 2–5, `run_import_batch(run_id, sources, item, emit) -> ImportCompletePayload`** is the real driver used by the command:
- Emits a typed `import-start` `{runId,total}` once.
- For each item:
  - It emits `import-progress` `{runId,total,current:i,path:label}`. Here `current` = completed attempts, and `path` is a bounded basename or `Item N`, never a raw URI.
  - It runs the **existing** item operation exactly once, then counts Ok → `imported += 1` or Err → `failed += 1`. There is no early return.
  - On Err, it emits `import-error` `{runId,total,failed:cumulative,errorMessage:detail}`.
- Then it emits a final progress `{current:total, path:""}` and exactly one `import-complete` `{runId,total,imported,failed,errorMessage}` built from the counted outcomes.
- Counts are `u64`. `total` is bounded at admission, and `imported + failed == total` by construction.
- The emitter (`emit_import_event`) maps typed payloads (`#[serde(rename_all = "camelCase")]`) to the four existing event names. An emit `Err` is logged with `log::warn!` and never changes outcomes, and later emits, including the terminal, are still attempted.

**Display safety:**
- `import_item_label` uses, in order: the URI display name the item route already resolved; else the local basename (the `?vc=` suffix is stripped as `parse_virtual_path` does); else `Item N` for content URIs or unusable names.
- `import_failure_detail` builds `label: reason`. It replaces the exact source string with the label, redacts any remaining `content://` token (ASCII case-insensitive) to `[content URI]`, collapses control-character runs to one space, and bounds the result to 256 Unicode scalars.
- `import_summary`:
  - empty when `failed == 0`;
  - otherwise `"{imported} of {total} imported; {failed} failed: d1; d2; d3"`, plus ` +N further failures` when more than 3 failed;
  - bounded to 1,024 scalars.
- Only the first three details are kept. Each is bounded before it is stored.

**Item operation (moved, not rewritten):**
- The former closure body is now `fn import_one_item(source_path_str, i, total_files, destination_folder, settings, display_name)`.
- The one added line is `*display_name = Some(resolved_name.clone());`, placed after the existing Android `resolve_android_content_uri_name(...)?`. It reuses the already resolved name; there is no second provider read.
- The fn carries `#[cfg_attr(not(target_os = "android"), allow(unused_variables))]`, because `display_name` is only written on Android.
- `import_item_operation(destination, settings, total)` returns the closure used by both the command and the tests. It calls `import_one_item` and keeps the existing `eprintln!("Failed to import {}: {}", …)` diagnostic unchanged; diagnostic logging cleanup is out of scope.

**Moved-code proof:** the base closure body (`base-item-closure-body.rs`, sha256 `661784af…9176`) and the new fn body (`new-item-fn-body.rs`, sha256 `72944b88…84e0`), with the added `display_name` line removed:
- They are **token-identical**: whitespace-insensitive comparison is True.
- Line diff after the 12-space dedent shows exactly one difference. rustfmt joined the five-line `#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]` attribute onto one line at the shallower indent. That is formatting only.
- R2a staging, local copy/sidecar/`.rrexif`/delete semantics and the Android delete-skip are therefore unchanged.

## 3. Test definitions: `mod r3_terminal_tests` (13 test fns, **NOT EXECUTED**)

All tests use the real `run_import_batch`. Batch tests use the real `import_item_operation` / `import_one_item` on local temp files (`tempfile`), with `delete_after_import = false`. Guard tests use a leaked test-local `AtomicBool`, never the global.

| Test | Asserts |
|---|---|
| `single_failure_ends_with_an_error_terminal` | Missing source gives the exact event sequence Start → Progress(0, "missing.RAF") → ItemError(1, "missing.RAF: Source file not found: missing.RAF") → Progress(1, "") → Complete(1, 0, 1, "0 of 1 imported; 1 failed: …"). |
| `mixed_batch_counts_each_outcome_once_and_keeps_the_good_copy` | Real file plus missing file gives (2, 1, 1) and the exact summary. One Start, one ItemError, one Complete (last); progress currents [0, 1, 2]. Copied file and source byte-identical. |
| `all_good_batch_completes_with_an_empty_message` | (2, 2, 0, ""), no ItemError, copies exact, sources still present. |
| `existing_destination_is_a_counted_failure_and_left_unchanged` | Collision counts as a failure with detail `a.JPG: File already exists at destination…`; existing bytes are kept. |
| `many_failures_are_summarised_with_bounded_safe_details` | Five failures: a content URI on host (label `Item 1`), a control-character name, a 300-char name, `fourth.RAF`, `fifth.RAF`. Summary names 3 details and ends ` +2 further failures`, ≤ 1,024 scalars, no control characters. Every ItemError is ≤ 256 scalars. No event JSON contains `content://`. Cumulative failed is [1..5]. |
| `resolved_uri_name_labels_failures_and_uri_text_is_redacted` | Item closure sets a resolved name. Detail uses it, the exact URI is replaced by the name, another `CONTENT://` token is redacted, and the pre-attempt progress path is `Item 1`. This exercises the injected outcome seam, not a real provider. |
| `wire_payloads_use_the_contract_field_names` | serde keys are exactly `runId,total` / `current,path,runId,total` / `errorMessage,failed,runId,total` / `errorMessage,failed,imported,runId,total`. |
| `invalid_requests_are_rejected_without_claiming_the_run` | Empty list, blank destination, and empty / invalid / uppercase / simple / braced run IDs all give Err with the flag still free. A valid request claims the flag; dropping the guard releases it. |
| `counts_beyond_javascript_safe_integers_are_rejected` | `MAX_SAFE` accepted, `MAX_SAFE + 1` rejected (where `usize` allows). |
| `busy_run_rejects_admission_until_the_worker_releases` | Guard moved into a worker thread running the real driver with a blocking item. A second admission returns exactly `"An import is already running."`. After release and join, the flag is free and admission succeeds. |
| `concurrent_admissions_accept_exactly_one` | 8 threads behind a Barrier: exactly 1 acquisition, then released. |
| `guard_is_released_when_the_worker_panics` | `catch_unwind` around a held guard and an injected panic; the flag is released and reacquirable. Expected panic text appears on stderr. |
| `display_text_helpers_bound_and_redact` | Redaction, control-character collapsing, Unicode-scalar truncation, `?vc=` basename, URI → `Item N`, `/x/..` → `Item N`. |

Static review notes (unexecuted):
- Two closures returning `Err`/`Ok(())` have explicit `-> Result<(), String>` annotations so they don't depend on later coercion inference.
- I avoided a fixture collision: "bad name.RAF" contains `e.RAF`, so the absent-detail names are `fourth.RAF` and `fifth.RAF`.

**Future commands**, from `src-tauri` in an assigned native cache window, after a source review:

```sh
cargo test --offline --locked --lib r3_terminal_tests -- --nocapture        # expect 13 tests
cargo test --offline --locked --lib file_management::tests -- --nocapture   # R2a: expect 13 passed + 2 ignored
cargo test --offline --locked --lib
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
```

A filter that matches zero tests is not acceptance.

## 4. Integration boundary and mismatches

- **Must not be integrated alone.** The current frontend (`useFileOperations.ts` `startImportFiles`) invokes `import_files` without `runId`. Tauri would reject that call for the missing required argument. The current listeners also expect the old untagged and unit events. R3B (frontend coordinator and hooks) must land atomically with this, as the contract requires.
- `lib.rs` needs no change: the command registration is unchanged and the new argument is handled by Tauri's command macro.
- No contract item required an excluded file or API. There is no mismatch to report.
- Not added, per contract: cancellation, task framework, durable journal, crash/panic completion guarantee, cross-reload reconciliation.

## 5. Gates

| Item | Status |
|---|---|
| Native admission, guard, typed run-tagged events, outcome accounting, bounded safe details/summary, in the real command path | Pass (static source, this edit) |
| Moved item operation token-identical; R2a helpers and all non-command code byte-identical | Pass (scripted comparison) |
| rustfmt check on owned file; `git diff --check` | Pass |
| Compilation, the 13 `r3_terminal_tests`, R2a `file_management::tests`, lib suite, clippy | **Not exercised** (source-only; no cargo) |
| Frontend R3B, real Tauri transport, Android/provider, R2b local-path safety | Not exercised; out of scope |
