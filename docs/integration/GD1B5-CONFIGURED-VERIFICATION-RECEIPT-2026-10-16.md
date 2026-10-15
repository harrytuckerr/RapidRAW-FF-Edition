# GD1b5-S3C2 — configured verification without adoption

Source-only delivery, 2026-10-16 Melbourne. Actual model `gpt-6-astra`; task `01a140cb-1924-7da1-b7ed-bb071652171e`. Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-configured-verification`, detached base `0131d650706ee1a227f168015c07778d08623cb8`.

**Frozen for independent review; compilation/runtime/acceptance Not exercised.** C2 performs native configured verification and immediately discards a successful sealed candidate. `VerifiedNotAdopted` never means Connected, sign-in success, permission for jobs or Drive readiness. No application command, browser launch or catalog configuration is installed.

## Authority and ownership

Read full root BRIEF.md, approved adoption/slicing contracts and root reviews, accepted B/C1 source/reviews and canonical validation, current account lifetime/fixture bridge, catalog APIs, S1 production and permanent regressions. Root's canonical422Pass/0Fail/14ignored/fmt/strict concerns the baseline only; no such result is attributed to changed C2 source. Current API/dependency hashes are in private dependencies.json. The preimplementation root known answers `210b99d15bc9caa5b63be75c220d20b10453a737d9e5d459bb507e88a14a0320` were read and copied unchanged.

Owned changes: existing `src-tauri/src/drive_auth/session.rs` and this new receipt only. Private evidence: `/private/tmp/rapidraw-gd1b5-configured-verification-implementation-20261016`. Root alone handles exports/README/integration. No account/registration/helper/child/Cargo/lock changes, Git mutations, build/cache/socket/network/provider/device/browser/IPC activity, or downstream implementation.

## Implemented native boundary

Added native `with_catalog(NativeRegistrationCatalog)`, async `begin_configured(RegistrationSelection)` and `verification_snapshot()`. The constructor stores immutable native configuration but starts no task or watcher and needs no runtime until begin. Empty/missing catalog yields `StartError::Registration(NotConfigured)` before reservation/binding; unknown/disallowed selections produce B's bounded typed categories. Client/project/requested scopes are copied only from exact catalog resolution and the validated selection, never caller URLs or prefix guesses. Configuration presence is not attested provider mapping.

`new()` and raw `begin(NativeStart)` remain pending-only, including on configured owners. Both paths use the same reserve/BeginReservation/supervise/ticket/listener cleanup ownership. Valid concurrent starts share Busy/Draining. Legacy validation and pending APIs remain intact. No hot reload, connection epoch or retained connected owner.

The configured Slot retains the original RegistrationSelection alongside its native request; Work carries only the selected mode. After the actual exchange, run_attempt delegates once to the configured continuation in the **same supervisor**. Under the existing mutex, start_verification validates current Exchanging UUID/generation, unexpired admission, original configured selection/mapping, owned result identity, local token usability and exact actual granted subset. It builds S2 context from current retained data, records the existing CompletedPendingVerification authorization outcome once, and enters VerifyingAccount. There is no public take/getter or pending watchdog on this path.

The owned result then enters actual `verify_account` outside the lock. Existing stop and original admission deadline bound it; S2's own request/token limits still apply. No new lifetime, retry, scope union, alternate production endpoint or spawned verifier. Configured continuation owns its future; cancellation signals and drops it cooperatively. The original supervisor still joins the blocking worker and any outstanding ticket acknowledgment before reaping.

Completion re-locks, validates current key/mode/phase/authorization/verification state and unchanged catalog binding, rebuilds context and reads the clock after that construction. Admission and credential usability are checked, followed by `candidate.matches_native_attempt`. A stale key cannot affect a replacement. A current running attempt with a broken mode/mapping finishes with BindingMismatch instead of silently publishing success. The entire candidate is explicitly dropped **before** publishing VerifiedNotAdopted; it is never placed in Slot/State or exposed through a constructor/parts/getter.

Separate bounded verification metadata contains configured-presence, attempt/generation, phase and one last static terminal record. It carries no personal account data, tokens, URL or actual-grant lease. Debug/Copy apply only to these nonpersonal metadata/enums; secret owners remain non-Debug/non-Clone/nonserializable. Outcomes are VerifiedNotAdopted, Cancelled, TimedOut, Closed, WorkerFailure, BindingMismatch or Failed(static AccountError). No provider text is surfaced.

Verification-aware terminate/fault handling records a terminal only after verification began and never overwrites completed authorization. Cancel winning the mutex prevents success; success winning leaves stale cancel inert. Close, owner Drop, poison, counter overflow and supervisor-reported unwind drain without retaining a proof. Snapshot expiry enforces the original deadline too. There is no lock across await, no started-worker abort claim, no zeroization claim and no new secret storage/watch task.

## Test definitions — ZERO executed

Added12 definitions (11 async +1 synchronous) under the owned configured_tests module. Existing21 authored tests and8 permanent independent child tests retain exact bytes/expectations; no mechanical old-fixture edits were needed. The original child registration footer is exact.

| New definition | Intended evidence, pending separate execution |
| --- | --- |
| public_configured_admission_rejects_locally_without_reserving | Empty/new/unknown/disallowed errors before reservation; presence remains metadata. |
| configured_and_raw_public_begin_share_real_ticket_cleanup_fence | Actual configured listener/ticket, Busy across both begins, held-ticket shutdown fence; no positive code callback. |
| raw_on_configured_owner_keeps_legacy_pending_continuation | Same legacy continuation/watchdog and no verification terminal. |
| actual_a_b_and_partial_grants_are_verified_then_discarded | Real A/B localhost requests through A bridge; exact synthetic bearer, smaller actual File grant, once-success and no retained pending owner. |
| extraction_rejects_wrong_identity_scope_and_retained_mapping | Wrong key/generation, empty/duplicate scopes and changed registration/client/project/request/mode reject before verification begins. |
| completion_rebuilds_current_mapping_before_discard_commit | Actual returned candidate rejects altered mapping/request or removed configured selection. |
| cancellation_and_real_candidate_commit_serialize_and_late_results_are_inert | Both lock orders and barrier race; once-terminal and stale error cannot overwrite next attempt. |
| foreign_and_late_real_candidates_cannot_rebind_or_overwrite | Candidate from another actual verifier request cannot be committed to a different attempt; late actual candidate cannot rewrite replacement authorization. |
| original_and_token_expiry_do_not_renew_at_completion | Exact original/token boundaries, expired preflight input, unchanged authorization. |
| stalled_real_transport_cancel_close_and_owner_drop_preserve_authorization | Cancellation/close/Drop during captured stalled HTTP, actual continuation exits, original success retained, Draining fence. |
| real_transport_deadline_preserves_one_authorization_result | Stalled HTTP capped by short admission or shorter token expiry; no deadline renewal. |
| post_authorization_worker_fault_poison_and_counter_overflow_fail_closed | Actual candidate plus production fault reducers; stale result discarded, original authorization preserved, stop/fence asserted. |

No fake verified result or sibling private-field access. A private cfg(test) VerificationInput variant owns A's restricted FixtureSpec, a synthetic preflight Pending marker and loopback address; production has only original ExchangeResult. The marker supplies known synthetic header fields for the **same production preflight predicates**, not account proof. FixtureSpec still uses A's pure token parser, and A's actual HTTP verifier must produce every successful candidate. The spec is boxed only inside the test variant to keep its size bounded relative to the production enum variant.

Synthetic post-exchange controls exercise real mutex/phase/continuation/commit paths but do not prove a full code-exchange-to-Google chain. Reducer fixtures that manually reap launch no listener/supervisor. Worker-failure tests call the existing production failure reducer; existing S1 tests cover actual caught worker panic/cleanup. New peer definitions use one ephemeral loopback connection,16KiB request cap,3s worker budget, stop-aware finite I/O and joined Drop, including panic cleanup. They have **not run**. No Google call or positive Code handoff exists in these new tests.

## Preservation and checks

Baseline session.rs65,639B SHA256 `eba4929001cd3c04aab9c6142b6151a70f899debb773e8b5ac0a4305299b87e0`.

Candidate112,397B SHA256 `1b77db23a04d51eddb9b1daff104e447dc0bbe6e803a611eb25a9f34fd7020fe`.

Patch53,410B SHA256 `bc72891178d15912a8206982e4209ece1b6aa0330c00c31873ab35c3e128539c`.

All654 inputs were rehashed from root manifest `a0c309c4affcdc15591518f3dc3e31d909184cdd9ff547668bde8d99ba053d8e` before editing. Only session.rs differs; the other653 inputs, tracked inventory, detached HEAD and index remain exact. Private manifests record each path/size/hash. Existing test module/footer direct byte comparisons pass. Functions supervise, bind_worker, join_worker, receive_ack, continue_pending, retain_pending, expire_pending, current, live, advance and fault also remain byte-identical. Their existing callees now include the explicitly reviewed verification-aware termination path; byte preservation is not a claim that new behavior has runtime acceptance.

Pass: standalone pinned rustfmt write/check with `--edition 2024 --config skip_children=true` on only session.rs; whitespace/final-newline checks; input/test/footer/function preservation. Formatter1.9.0-stable (48a229ceae) selected with RUSTUP_TOOLCHAIN=1.98/RUSTUP_AUTO_INSTALL=0. Exact check argv/UTC/exits in checks.json `968fd8fef99060a1fd93f8a0921507f1c630463f04689ab7aa9dfbc11b2e0ad8`; preservation.json `a000f63626d20b2d167d5446e28d9806526a6bda0f478424cd11ec3ca2fbd1d2`. No compiler/type/lint result follows from formatting.

Not exercised: Cargo/rustc/compile/tests/Clippy/cache, sockets or cleanup runtime, Google/provider/account/registration provenance, browser/IPC/UI/Android/device, canonical integration or product acceptance. New tests executed0; existing tests rerun0. Cache never acquired/accessed.

## Review handoff

Root static review, then a separately granted independent source/runtime owner. Suggested future pinned/offline commands from src-tauri under that owner's cache grant: `cargo test --locked --offline --lib drive_auth::session -- --test-threads=1`, relevant account/registration regressions, fmt and strict all-target Clippy, then root combined native validation. Do not treat historical422 or these41 session test definitions as passed C2 evidence. Concrete failures preserve this frozen delivery and require a bounded revision grant.

B catalog changes, C3 retention/watchers/adoption, C4 switching, configuration loading, credential leases/store/refresh, browser/UI/IPC/navigation and app activation remain excluded. No credentials request. Stop at frozen source/receipt for independent review.
