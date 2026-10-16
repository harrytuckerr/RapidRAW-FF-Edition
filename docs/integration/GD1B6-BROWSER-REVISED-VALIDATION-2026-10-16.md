# GD1b6 revised browser validation

Pass for this bounded new gate. Codex CLI - Sol, actual roster model `gpt-6.1-sol`, 2026-10-16T01:05:11.432162+00:00. Detached `6932d49ba6f989baf5b9ca2ae450b4846fec6512`, snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-browser-observer-review`. Author Astra; prior independent production review and probes Sol; observer correction Sol, independently source-reviewed by root. Root canonical acceptance remains separate.

| Gate | Exit | Duration (s) | Result |
| --- | ---: | ---: | --- |
| `cargo test --locked --offline --lib drive_auth::browser -- --nocapture --test-threads=1` | 0 | 121.252049 | Pass |
| `cargo fmt --all --check` | 0 | 1.195241 | Pass |
| `cargo clippy --locked --offline --all-targets -- -D warnings` | 0 | 24.475911 | Pass |

The new focused run has 23 completed rows (14 authored, 9 independent). Its actual harness summary is `[['ok', '23', '0', '0', '0', '464', '31.60s']]`; Cargo exit `0`. Full 23-name equality, unique completed outcomes and successful summary are checked independently in `focused-results.json`; a selection banner alone is insufficient. Format/strict checks run only after complete focused Pass. No repeat, repair, formatter write, suppression or oracle/source edit occurred.

Rust/Cargo1.98.1, locked/offline; full DEV/TEST DEBUG=0 and INCREMENTAL=false, CARGO_INCREMENTAL=0, RUSTUP_TOOLCHAIN=1.98/AUTO_INSTALL=0, private TMPDIR, TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin, ORT_SKIP_DOWNLOAD=1 and GIT_OPTIONAL_LOCKS=0. Exact argv, resolved tool, environment overrides, UTCs, durations, exits and all raw/sanitized logs are preserved privately. No sensitive inherited environment dump. Known 33481272-byte ORT SHA256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` was hash-verified/read-copied only if absent; download skipped. Fresh >=12GiB/full-width PID/parent preflight completed and was inspected before each separate gate launch; mailbox reads were separate completed calls.

All 698 original input hashes/exact product inventory and detached HEAD/index/pointer match before/after. Browser 40838 B SHA256 `5277c7e4fc176b22e7941d229818b82477c736cfc7da800b24b1e1e1cc9e7e31` retains the entire 40752-byte author prefix plus exact86-byte footer. Child 32108 B SHA256 `6344645bcae7878c62322e7d6a3bc816498ff5da84b77a630419b16f1d5491c6`, module/Cargo/oracles and every other input remain unchanged. This report alone adds product path699; ignored ORT/generated schemas are separately inventoried. Both the 78-record interrupted seal and 34-record source seal rehash unchanged.

The original command remains Interrupted/-2:23selected,22started,21complete Pass observations,one started/incomplete,one unstarted,no summary. No old observation or prior C4 result is added to this new23-definition result. Controlled worker-unwind stderr remains intact (2 panic-start lines); it is not silently deleted. There was no runtime restart or routine signal in this task.

The executed binary is preserved from the actual `Running unittests` path, not an old fingerprint:

- `/private/tmp/rapidraw-gd1b6-browser-revised-validation-20261016/executed-rapidraw_lib-c8f324ca959c2d5c`: 41480480 B, SHA256 `75f079716d8e078fd395d12753bdfa0061ab59aef58542a46f44c18b50ca7f74`.

Public-ticket/drop and actual owned synthetic-child cleanup, canonical URL/command-shape validation, pure completion-CAS and bounded localhost denial behavior are the exercised layers. The observer checks a fresh launcher PID/expected Rust parent/zombie non-reap predicate through its own owned bounded-query worker; production remains sole launcher reaper. Exceptional native acquisition/file-I/O/kill/wait scheduling can exceed the async timeout, so no hard OS cleanup/abort schedule is proved. OS default-handler invocation, browser readiness/login, Google/provider/DNS/full positive OAuth/Code-success, account/configuration, UI/IPC/mobile/device/storage/refresh/transfer/sync and canonical full suite remain **Not exercised**. Public default hand_off is unpolled; only frozen synthetic plans execute.

Cache explicitly released at `2026-10-16T01:04:52.277660+00:00`; robust bounded `ps -ww -axo pid=,ppid=,command=` classified no surviving compiler/test/owned fixture, 16219783168 free bytes. This is a point-in-time process observation, not a hard cleanup guarantee. No more native commands after release. Full receipt/698-input custody, logs, binary, exact cases, histories and sealed manifest: `/private/tmp/rapidraw-gd1b6-browser-revised-validation-20261016`. Frozen for root review; no downstream activity.

| Actual completed test | Result |
| --- | --- |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_caller_future_drop_keeps_started_child_and_shutdown_fenced` | Pass |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_controller_cancel_and_original_ten_second_deadline_hold_child` | Pass |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_generated_bytes_real_null_child_failures_and_no_replay` | Pass |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_local_denial_precedes_but_cannot_bypass_child_acknowledgment` | Pass |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_unpolled_stale_and_predispatch_cancellation_never_spawn` | Pass |
| `drive_auth::browser::browser_independent_tests::actual_processes::independent_worker_unwind_and_exited_child_unknown_wait_keep_ticket` | Pass |
| `drive_auth::browser::browser_independent_tests::independent_canonical_request_bounds_and_adversarial_aliases` | Pass |
| `drive_auth::browser::browser_independent_tests::independent_real_completion_cas_and_drop_have_only_fixed_winners` | Pass |
| `drive_auth::browser::browser_independent_tests::independent_selected_default_shape_is_built_but_never_invoked` | Pass |
| `drive_auth::browser::tests::caller_drop_and_completion_have_one_atomic_winner` | Pass |
| `drive_auth::browser::tests::destination_aliases_duplicates_unknowns_and_lossy_encoding_fail` | Pass |
| `drive_auth::browser::tests::dropping_unpolled_public_future_does_not_dispatch` | Pass |
| `drive_auth::browser::tests::field_bounds_and_canonical_base64_do_not_admit_variants` | Pass |
| `drive_auth::browser::tests::generated_subsets_scope_order_and_exact_encoding_are_accepted` | Pass |
| `drive_auth::browser::tests::private_synthetic_plan_carries_original_bytes_without_dispatch` | Pass |
| `drive_auth::browser::tests::processes::actual_handoff_deadline_does_not_release_a_started_child` | Pass |
| `drive_auth::browser::tests::processes::caller_drop_before_dispatch_releases_failure_without_starting_child` | Pass |
| `drive_auth::browser::tests::processes::dropped_caller_and_controller_cancel_keep_actual_child_shutdown_fence` | Pass |
| `drive_auth::browser::tests::processes::earlier_denial_callback_waits_for_real_launcher_acknowledgment` | Pass |
| `drive_auth::browser::tests::processes::panic_and_unknown_wait_keep_ticket_until_real_child_reap` | Pass |
| `drive_auth::browser::tests::processes::real_selected_child_success_nonzero_and_spawn_error_are_distinct` | Pass |
| `drive_auth::browser::tests::processes::stale_and_real_handoff_expiry_prevent_any_dispatch` | Pass |
| `drive_auth::browser::tests::selected_recipe_is_exactly_first_and_never_a_program_override` | Pass |

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `01c80f573a81cd3f07928e0b0d5c23a1f1b781fc35adeae92235ebdd68637c70` (7419 bytes).
