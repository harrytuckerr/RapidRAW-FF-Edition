# GD1b5-S3C4 independent runtime validation

**Executed gates Pass:** 86 tests passed, 0 failed, 0 ignored; standalone/final formatting and strict all-target Clippy passed. **One test evidence gap remains:** root's review found an intervening reap/expiry/notification path in the earlier-replacement timer case. These passes do not establish that case's claimed isolation of acceptance-triggered watcher rescheduling. No production defect is established by that finding; root review/canonical acceptance remains separate.

Actual reviewer/new-probe author: `gpt-6.1-sol`, slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`; C4 production author: Astra/`gpt-6-astra`. Review snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-switch-decision-review`, detached `47a1a46b2fec95826ffe5e71e39b4b5889211919`. All 679 original files were frozen read-only. Only the new kept independent child and this report were added; no production or old test/oracle edits.

## Actual gates and unique results

Cwd `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-switch-decision-review/src-tauri`; pinned toolchain directory `/Users/harrisontucker/.rustup/toolchains/1.98-aarch64-apple-darwin/bin`. `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1 (797e8a9bc 2026-08-05)`; host `aarch64-apple-darwin`. Exact absolute argv, UTC, durations, environment overrides, full stdout/stderr and fresh preflights are preserved privately in `commands.json`, `environment.json` and `preflights.json`.

| Command (exact arguments) | Exit | Seconds |
| --- | ---: | ---: |
| cargo test --locked --offline --lib drive_auth::session -- --test-threads=1 | 0 | 88.955842 |
| cargo fmt --all --check | 0 | 0.794090 |
| cargo clippy --locked --offline --all-targets -- -D warnings | 0 | 18.459090 |

The sole focused command selected **86 unique definitions**, all executed once: 53 inline (including author C4 tests), 24 prior independent children (8 each), and 9 new independent groups. Actual summary: `ok. 86 passed; 0 failed; 0 ignored; 0 measured; 378 filtered out; finished in 5.14s`. No prior historical or overlapping pass counts are added. `results-by-origin.json` contains every actual test name/result and disjoint origin. No full-library/CLI/integration/ignored rerun occurred.

Overrides: `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`, `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`, both `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`, both profile `INCREMENTAL=false`, `CARGO_INCREMENTAL=0`, `CARGO_NET_OFFLINE=true`, `GIT_OPTIONAL_LOCKS=0`, `PYTHONDONTWRITEBYTECODE=1`, private `TMPDIR=/private/tmp/rapidraw-gd1b5-switch-decision-validation-20261016/tmp/`, `TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin`, `ORT_SKIP_DOWNLOAD=1`. No unrelated inherited environment dump. Every gate had a fresh no-competing-compiler and ≥6GiB check; actual free-byte records are retained.

The ignored ORT was read-copied from the canonical snapshot, 33481272 bytes/SHA256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`; both source/destination hashes verified. Build log confirms existing valid runtime/download skipped. No network download, resolve, dependency edit or clean.

## New fixed cases and qualification

All nine sources and fixed expectations were formatted/frozen **before any Cargo command**. Child 42589 bytes/SHA256 `f1a24b616719d7cc5edbb36719813a8de97f51751d7968b61ef85fbe2b221ca4`; fixed-oracles SHA256 `8c7d10ecc49d91e07fb241f9138ea66b08c5d574036e23b870f13f707d93dd62`; 680-path pre-execution freeze `5c461ac700153a80d0eb5e6a8f42b018300dfb18fb059989520437062e59492b`. `fixed-oracles.json` maps each group to root's 12 preimplementation known answers (root JSON copied unchanged). Native candidate evidence uses the existing restricted bridge, pure parser once at original issue time and actual own bounded localhost HTTP responses; no proof factory, fake Running, author Peer/helper reuse or positive Code handoff.

| New group | Fixed behavior tested (with qualification below) |
| --- | --- |
| exact_proposal_partial_accept_and_late_sealed_completion | AwaitingSwitch is nonterminal; exact metadata, immutable history, wrong tuples Stale; late actual sealed completion/error inert; accept adopts exact File subset in project202 once; consumed replay AlreadyResolved, after reap Stale. |
| decline_cancel_markers_and_new_attempt_custody | Decline or cancel preserves whole incumbent; exact consumed marker remains bounded to slot; after reap old decisions/errors cannot alter newer attempt/auth/history. |
| original_three_caps_equality_and_backwards_clock | Original admission300s / immutable credential40s / proposed+60s minima exact; binding valid1ns before, acceptance at equality forbidden TimedOut and AlreadyResolved; repeats immutable; backwards proposed clock TimedOut. Future-clock reducer tests, not timers. |
| binding_corruption_never_retargets_consent | Eight independent key/epoch/project/client/id/selection/requested/stamp corruptions reject exact consent Stale+BindingMismatch, preserve incumbent, replay AlreadyResolved. |
| continuation_immediate_decision_and_actual_pending_timer | Actual continue_configured HTTP-seals B and remains pending/Busy; immediate exact Decline cannot lose wake; real100ms confirmation ends TimedOut with raw-state absence observed before getter; one watcher; actual continuation joined. |
| accepted_earlier_expiry_reschedules_actual_watcher | Accepted replacement has earlier100ms connected deadline; real watcher reschedules from incumbent900s, raw-state credentials clear while new stamp/epoch remain, without getter; one watcher. |
| serialized_consent_cancel_disconnect_and_real_barrier_race | Accept/cancel both orders and actual barrier permit only (AdoptedSessionOnly,AlreadyTerminal) or (AlreadyResolved,Cancelled). Explicit disconnect clears either winner and prevents resurrection. |
| expired_stamp_and_drop_orders_hold_actual_cleanup_fence | Expired stamp still requires proposal; accept/Drop both orders clear all owners/proposal before actual held watcher finalization; no new admission; exact decision Closed; released gate finalizes and last inner reference drops. |
| overflow_poison_and_actual_watcher_fault_clear_both_owners | Checked generation/connection overflow, poison and actual watcher panic globally clear both owners/proposal without wrap; auth remains immutable, verification Closed once, typed fault persists. |

**Root-attributed evidence qualification**, Team message `01a141dd-b5b0-7e71-93de-7c516e3406ee`: in `independent_accepted_earlier_expiry_reschedules_actual_watcher`, `owner.inner.reaped(work.key)` occurs before the raw connected-none observation. Production `reaped()` itself calls `expire_connection(now)` and sends `inner.changed`; it can lazily expire if delayed, and can independently reschedule the watcher even if acceptance notification regressed. Therefore this passing test does **not** isolate the acceptance notification or prove expiry solely from that notification without an intervening expiry/notification path. The child comment and pre-execution oracle state that intended property too strongly. They remain frozen unchanged; the qualification is recorded separately in `root-qualification.json`. The source appears to send the shared notification correctly; no product bug is shown. A minimal test-ordering correction requires root's separate grant. Other eight groups had no blocking finding in root's source review. These run results are preserved literally, not relabeled as corrected evidence.

Future-clock cases inspect real reducers/binding with the original 300s admission, immutable token expiry and exact proposed+60s; they are not real elapsed-time boundary tests. The separate 100ms pending-confirmation test drives and actually joins `continue_configured` and observes raw draining/proposal absence before getters. Drop/cleanup exercises the actual connected watcher and held finalization gate. Reducer-created slots have no listener/supervisor and are manually reaped only as synthetic reservations; that is not full positive OAuth/supervisor cleanup evidence. Existing actual worker/ticket fences execute through the retained regressions. The fixture HTTP label/project mapping is synthetic custody, not provider identity/project attestation.

## Frozen artifacts and custody

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| Kept new child | 42589 | `f1a24b616719d7cc5edbb36719813a8de97f51751d7968b61ef85fbe2b221ca4` |
| Actual executed/preserved native binary | 41190880 | `1a63043bc8a92720d765e223178724fb5db00e34b79006e485870bb4a4afbc59` |
| Immutable wired session | 209473 | `ef4726e1b53c8cf59d94ad4d0e9d86e58597bf827c8bd1bc08e26076545773fa` |
| Entire immutable author prefix | 209371 | `525be1112c4273988beee762458a603ef8094d05a38cc0d06e9721b65af041ce` |

Input authority: `runtime-inputs.json`, 113079 bytes/SHA256 `04238b22c53e1d4416474c89f3be1fe454b718dc1b64eb9f6a267618fdeaf31f`. Full before/after hashes prove all **679 existing inputs** unchanged, exact HEAD/index/pointer and 102-byte root footer retained, including migrated C3 child04bf5d97, C2 child31750921 and S1 child7129423f. Frozen new child/oracles match before/after. Product inventory is **681** (679+child+report); ignored ORT and four generated schema files are separately inventoried, with schema copies retained. No source/test removal or Git process/mutation.

Private evidence `/private/tmp/rapidraw-gd1b5-switch-decision-validation-20261016` contains full/sanitized logs, actual binary, source/patch, commands/environment/preflights/toolchain, complete input inventories, fixed/root oracles, root qualification, ignored schema copies, runtime-reuse record, machine receipt, delivery and sealed artifact manifest. Final report/receipt/seal hashes are sent through Team after sealing; no self-referential hash.

Cache **explicitly released 2026-10-15T23:21:11.375643+00:00**, no cargo/rustc/clippy processes, 16274579456 bytes free (≥6GiB). Root-owned lease file unchanged. No more build/test commands in this task.

**Pass:** actual 86/0/0 focused run, formatting, strict all-target lint, immutable-input/oracle custody. **Fail:** no executed gate failure; the isolated acceptance-notification evidence gap remains open. **Not exercised:** canonical full suite, Google/provider identity or full authorization/exchange/account chain, production registration/configuration, browser/IPC/Android/device/storage/refresh/lease/transfer/sync/product acceptance. Historical C3 failures remain literal and immutable. Source/evidence frozen for root review; no downstream implementation or publication authority.
