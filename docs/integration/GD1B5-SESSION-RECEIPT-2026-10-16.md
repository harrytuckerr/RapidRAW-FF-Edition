# GD1b5-S1 — native session owner, source-only receipt

Frozen 2026-10-16 Melbourne by **gpt-6-astra** (live Team roster verified), task `01a14034-3201-78d0-907a-91c310827c60`. Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-session`; clean detached input `c5e1bdcf76de64bad1b87bcaa9e2526c95457efa`, following accepted source commit `cad750c5be0d07f801f8070df76133100d687aa5`.

**Delivery: ready for independent source review and separately authorized registration/validation. ZERO tests executed.** This module is unregistered; this receipt does not accept compilation, runtime correctness, Google authorization, account identity, browser launch or Drive sync.

## Exact owned output and custody

Only two new repository paths are delivered:

- `src-tauri/src/drive_auth/session.rs`: 65348 bytes; SHA-256 `f49ac5ed30090288199f72cc2c3659499e9ae4dcede5d461bf538c6a58e8a6bc`. One module with 21 inline test functions (11 synchronous, 10 async).
- This new `docs/integration/GD1B5-SESSION-RECEIPT-2026-10-16.md`; final size/hash in private `delivery.json` to avoid a self-hash.

Private evidence only: `/private/tmp/rapidraw-gd1b5-session-implementation-20261016/`. `session-source.patch` SHA-256 `b72300b04d1e31af9df54a1dab9081e933999b732143744d0c413c489306afb1` (67408 bytes), an exact new-file diff from `/dev/null`. No module export, source registration or existing-file change.

Dispatch ledger `/private/tmp/rapidraw-gd1b5-session-dispatch-20261016/ownership-and-inputs.json` was verified as `c5f9713cfc15e4b37dfaefc8d96c240cc3e6b37c74fdee017dbe2c85d12f847c`. All **614 tracked input byte lengths and SHA-256 values** matched before and after authoring. `protected-before.json` and `protected-after.json` are byte-identical, each 94,419 bytes, SHA-256 `edad9f2d89e21518f51a14651f98114d612c9b3e3544f727d6e9c6b62c20138e`. Final handoff rechecks these same inputs and exact two-path untracked inventory. Existing source/tests/docs/README/Cargo/locks stayed protected.

Full approved contract `11950b2f77d6e4d5174c8023a74912b706c39ec03a1b93e40f3019115d0716f1` and lead review `3e842938847da9f27fe724a8f1c5282a17db603138cf11356454af79693380fe` were read. Full current canonical report `167f42a72196430084684ed108c7c988b70da5ae812a1a763d01b4e5deefdc22` records **339 previous tests passed / 14 ignored plus fmt/strict lint**. That report supersedes the design's historical candidate-pending wording. Those previous tests do **not** validate session.rs.

Actual frozen dependency hashes (current canonical bytes, not historical candidate hashes):

| Protected dependency | SHA-256 |
|---|---|
| `src-tauri/src/drive_auth/pkce.rs` | `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8` |
| `src-tauri/src/drive_auth/loopback.rs` | `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41` |
| `src-tauri/src/drive_auth/token_response.rs` | `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886` |
| `src-tauri/src/drive_auth/exchange.rs` | `f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408` |
| `src-tauri/src/drive_auth/mod.rs` | `bc7e526d8ec1082ce55e51c28a3f60ef4e1cf0519d7577daa1178df055e6ef3d` |
| `src-tauri/src/lib.rs` | `8c71f74aec49a6635a5ab2f959472adeea78e0b48eaabaf9b22c2f0f679e4963` |
| `src-tauri/Cargo.toml` | `454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2` |
| `src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

## Implemented boundary and review map

`NativeStart` (line 29) accepts bounded native registration configuration: client ID matches the frozen helper's <=256-byte installed-client syntax; registration ID is 1–128 ASCII identifier bytes; configured project number is 1–32 canonical positive decimal digits; scopes are one or two unique ApprovedScope values. Configuration is not authenticated membership. There is no caller URL/endpoint/account/token field.

`SessionController` (line 184) is a non-Clone owner. Tasks retain a separate Arc<Inner>, so task/ticket references cannot prevent owner Drop. One mutex holds the checked u64 generation, current UUID/phase, registration, stop signal, actual listener cancellation, private pending result and one last terminal record. Public snapshots contain only bounded status/UUID/native-counter/granted-subset metadata. No secret-bearing type implements Debug, Display, Clone or Serialize; no controller token getter, verified constructor or Connected variant exists. Public error/status enums are static and carry no raw provider/OS strings.

`begin` (line 258) reserves before spawning. A cancellation-on-drop reservation guard protects a dropped begin future. The production reservation (line 451) rejects Busy without mutating the incumbent; UUID entropy is obtained outside the commit lock. Generation exhaustion and mutex poison close admission, discard pending secrets and signal cancellation. Poison access thereafter is limited to fail-closed cleanup/inspection, never adoption or restored service.

`bind_worker` (line 787) uses actual LoopbackSession::bind/cancellation/authorization_url_for_browser/wait_on_worker. Binding and pumping share one blocking worker off the async/UI thread. The wrapper publishes readiness immediately before entering the actual pump; late binding cannot attach to a stale/cancelled/replacement slot. It uses the bound native helper URL, not a supplied URL or fabricated readiness API.

The native BrowserLaunchTicket (line 382) offers a checked borrowed URL and one consuming `finish(HandedOff|Failed) -> Result<(), TicketError>`. URL access and acknowledgment recheck identity, phase and deadlines under the mutex. Drop is failure, never handoff success. A callback can complete while acknowledgment is outstanding; at most one owned worker result is retained and no code exchange starts before a positive acknowledgment. A stale or late ticket cannot resurrect an attempt. This module does not open a browser.

`run_attempt` (line 885) calls the real fixed-endpoint exchange exactly once with the owned grant, matching UUID/generation and owned oneshot cancellation. The exchange future is not detached or retried. `adopt_pending` (line 626) checks current nonterminal phase/generation, actual monotonic time under the lock, original deadline, returned attempt binding, token expiry and explicit approved scope subset before retaining the whole ExchangeResult privately. Success is **CompletedPendingVerification / AccountVerificationPending**, never identity, connected session or ready sync. The native registration stays attached for a future separately granted verifier integration.

300s absolute admission lifetime, 5s startup deadline and 10s ticket-ack deadline are immutable/capped; stage changes cannot renew the absolute deadline. Exchange retains its own 30s cap and outer cancellation/deadline. Pending retention is min(token expiry, original admission deadline); the actual retention watchdog (line 987) and snapshot expiry check discard expired material. Authorization terminal outcome is recorded once. Later pending discard/expiry changes retention/phase but does not replace CompletedPendingVerification with a second authorization outcome.

`supervise` (line 851) catches orchestration unwind, signals actual listener cancellation and joins the owned blocking handle. Draining also retains the single outstanding ticket acknowledgment receiver: even after a timeout/cancel, a stalled native launcher must finish/drop its ticket before the slot is reusable. Both resources are bounded to this one slot. No lock is held across socket binding, awaits, exchange or joins. `shutdown` closes admission and waits outside the mutex. Drop signals closure synchronously; the existing supervisor owns eventual cleanup. Started spawn_blocking is **not** aborted or represented as immediately released. An unreturning OS call/held external ticket can keep Draining/Busy and shutdown pending; it cannot accumulate further attempts. Runtime/process teardown remains outside a hard cleanup guarantee.

## Definitions versus execution

All 21 functions in the inline tests are **DEFINED, NOT RUN**. `checks.json` contains their exact names; table-driven cases and race alternatives are not added to the function count. Test definitions exercise:

- The production mutex/reducers with simultaneous admissions, Busy preservation, stale cancel/ready/completion, once-only terminal records, reservation-drop invalidation, counter exhaustion both idle and active, and poison fail-closed behavior.
- Barrier races between pending completion and cancel/disconnect/switch; current ID/generation and original deadline preservation; wrong/expired/missing/unrequested grants; partial scopes and pending retention without a second terminal result.
- Actual listener/pump definitions for callback-before-ack followed by Failed (synthetic code never positively acknowledged), incorrect state followed by acknowledged denial, unknown extension handling through the unchanged helper, ticket drop/late acknowledgment, actual late-bound cancellation, owner Drop and socket closure after shutdown.
- A private cfg(test) bounded startup gate for dropping the actual begin future and forcing a startup timeout while the actual blocking handle remains owned. The gate self-expires after 2s; localhost connect/read/write fixtures use 1s bounds and cleanup waits use 3s. A private shorter Work startup deadline is test-only; production constants are fixed.
- The actual pending-retention helper and blocking JoinHandle error path. A private cfg(test) Pending marker targets the same commit reducer without inventing an ExchangeResult/account proof constructor or a production endpoint override.

These definitions distinguish structural concurrency tests and synthetic localhost fixtures from real token issuance/account/browser acceptance. No test sends a positively acknowledged code to Google. There is no public or environment-controlled test transport, fake verified state, scope broadening, helper change or success stub.

## Checks actually executed

Host recorded as `macOS-26.5.2-arm64-arm-64bit`. Source authoring used standalone rustfmt only. Initial default formatter reports `rustfmt 1.9.0-stable (8bab26f4f6 2026-07-14)`; additionally the **pinned** `RUSTUP_TOOLCHAIN=1.98` formatter reports `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)` and the same file passes its check. No rustc/Cargo/test executable was invoked.

| Check | Observed result |
|---|---|
| HEAD, initial clean status, ledger and all614 before/after hashes | Pass; exact input base and protected bytes |
| Standalone `rustfmt --edition 2024 --config skip_children=true --check src-tauri/src/drive_auth/session.rs`, default and pinned1.98 | Pass; both exit0, empty diagnostics |
| `git diff --check` | Pass; exit0; tracked files unchanged (this alone does not cover an untracked source) |
| `git diff --no-index --check /dev/null src-tauri/src/drive_auth/session.rs` | Empty whitespace diagnostics; exit1 because a new-file diff exists; not reported as exit0 |
| New-file patch capture | Exit1 as expected for differences; exact source SHA above |
| Compile, focused tests, full suite, strict Clippy | Not exercised, source-only grant; **0 executed tests** |
| Local sockets, HTTP, browser, account/provider, secure storage, UI/IPC, device, sync | Not exercised |

`checks.json` SHA-256 `6057a442b9d7c9f9cd19f687ef34d710c6ffb7675749902d3cc74dda370d6d53` records exact final check argv/cwd/exits and per-command logs. `pinned-format-checks.json` adds the explicit toolchain override and logs; `source-anchors.json` maps review locations. These are captured final checks, not an invented complete terminal transcript of authoring reads/edits. Pass here is custody/formatting evidence only. No observed runtime failure can be asserted because runtime was not exercised.

No Cargo/build/shared-cache/node/dependency/provider/socket/browser/device operation, Git mutation, registration, existing-source edit or README change occurred. Native build-cache ownership was neither acquired nor used. Ordinary drop is not guaranteed secret zeroization; an OS/browser may complete after local cancellation, and remote issuance/revocation/rollback is not claimed.

## Next consumer and stop point

Root independently reviews this exact source, then alone registers the new module in an isolated review checkout. Under an explicit cache/runtime grant, from `src-tauri`, pinned Rust1.98, locked/offline/no-debug/no-incremental/private TMPDIR:

```sh
cargo test --locked --offline --lib drive_auth::session
cargo test --locked --offline --lib drive_auth
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

These commands are future acceptance work, **not run here**; reviewer must freeze/register source and separately authorize synthetic loopback execution first. Independent adversarial review should inspect generation/terminal commit ordering, ticket resource fencing, retained joins, panic/poison behavior, no-lock-across-await, and concurrency tests; authored fixture definitions do not self-approve those behaviors. Current base339Pass does not waive failures in this new module.

After S1 acceptance, account verification remains a separate grant using authenticated fixed about.get and the configured project mapping; browser adapter/registration/IPC and later adoption remain separate serial owners. No downstream implementation has started. Source and receipt stop here for independent review.
