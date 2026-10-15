# GD1b5-S3C: four serial slices

Design only; 2026-10-16; gpt-6-astra; base `5a2208c0184657e1d2a364ad46a6cddcb5f43290`.

**C1 continuation → C2 nonretaining verification → C3 connected ownership → C4 switching.** C2 first verifies; C3 first adopts. Root must approve temporary VerifiedNotAdopted. No app/browser/IPC/UI registration.

Each slice owns ONLY `src-tauri/src/drive_auth/session.rs` regions/tests below +receipt. One serial writer; no new module. Other files protected; root owns probes/exports/integration.

## Dependencies and policies

S1/S2 accepted; A `6b0ab79e…` runtime pending; B plan `038709cf…` not API. C1 independent; C2 requires canonical A+B/frozen interfaces. No helper edits; catalog empty until approved configuration.

Parent proposals: connected=min(token expiry, adoption+1h); confirmation=min(admission, token expiry, proposal+60s). **Root freezes values at C3/C4 dispatch.** No changed/default policy or Google guarantee; checked arithmetic fails closed.

## C1: existing pending continuation extraction

Regions: run_attempt tail(baseline976–984), retain_pending(987), adjacent helper/tests. Private `async continue_pending(&Inner, Key, Pending, &mut watch::Receiver<bool>) -> Result<(), AuthorizationOutcome>` contains the existing adopt_pending→retain_pending sequence. Real caller passes Pending::Exchange(value); supervise catch/unwind and worker/ticket ownership unchanged. No unused future scaffolding.

Stop: unchanged pending-only behavior. Existing regressions +actual-helper Pending markers: key/scopes, stop/expiry, terminal/reaping. Real listener/ticket tests retained; markers prove no identity.

## C2: configured verification, discard successful candidate

Regions: Slot/Inner/status(160–232), constructor/begin/snapshot(239–303), reserve(451), commits(626), finish/terminate/fault(537,704–749), continuation(851–1000), tests; startup/ticket/listener unchanged.

Add native with_catalog/begin_configured(selection), bounded verification snapshot/terminal using B. Raw new()/begin stays pending-only on any owner; one Slot/Busy fence. After configured exchange, under the existing mutex validate Exchanging key/generation/mode, selected immutable registration, result identity/scopes/expiry and original deadline. Record CompletedPendingVerification once; enter VerifyingAccount and move the exact ExchangeResult into the same supervisor. No take/watchdog; legacy uses C1.

Build VerificationContext from retained native fields. Await verify_account outside lock, capped by original300s/stop. Completion rechecks current key/generation, mode/phase, completed authorization, no verification terminal, unchanged mapping, fresh deadline and candidate.matches_native_attempt against current context. **Do not reuse live() requiring terminal=None or reset authorization terminal.**

Success records VerifiedNotAdopted, drops candidate; no connected/history/watcher. Failure/cancel/unwind records verification terminal once if started, preserves last_authorization and drains before reaping; extend finish accordingly.

Tests use production guards/continuation/mutex: cfg(test) FixtureSpec+SocketAddr invokes A verify_fixture; production only ExchangeResult. Joined localhost A/B peers capture bearer; no fake proof. Cover configured admission/Busy, raw begin, scope/binding errors, cancel/deadline/close/late completion/config mismatch, poison/overflow and separate once-only terminals. Synthetic seam is not full-chain proof.

Stop: verifier, **no retained credentials/Connected**. If rejected, merge C2+C3 under larger grant; no half-retaining stop.

## C3: first/same-account ownership with full lifecycle

Regions: State/Inner/snapshots, configured commit, reserve/cancel/cancel_key/invalidate/close/fault/reaped, expiry/watch, shutdown/Drop/tests. No switch storage.

Add native connection_snapshot; State.connected owns whole sealed candidate, UUID/checked connection epoch/local deadline outside Slot. Retain one token-free identity stamp after expiry. Admission captures expected epoch. Adoption rechecks epoch, C2 predicate and candidate usability under the SAME mutex. First identity or same AccountKey adopts; actual grants replace, never union. Failed reconnect preserves valid incumbent; reaped clears Slot only. Different identity (also after incumbent expiry) drops candidate and records temporary **SwitchRequired**, not success/decline. C4 later replaces that branch.

Cancel-first rejects commit; adopt-first makes stale attempt-cancel inert. Explicit Disconnect/SwitchAccount invalidation clears credentials/identity and advances both relevant counters; failed reconnect does not. Authorization and verification records remain separate/once-only. Snapshots: no secrets/Debug/serde. No token/parts/proof constructor.

Indivisible lifecycle: one expiry watcher holds Inner, not controller; UUID/epoch guards old timers. Snapshot and mutation entry enforce expiry too. After adoption use A credential metadata, not300s admission comparison. No lock across await. Close/Drop/fault clears secrets and signals attempt/watcher; shutdown waits outside lock for Slot cleanup AND watcher exit. Startup/finally/unwind failure cannot leave usable credentials or advertise availability. Poison/overflow fail closed. Preserve external-ticket and blocking-worker Draining fences; no abort/hard-release claim or per-session watcher accumulation.

A transport tests: adoption survives reaping, same-account/partial replacement, failed reconnect, different identity rejection, expired identity, admission-vs-token lifetime. Barriers cover cancel/adopt, disconnect/Drop, old timer/new session, stale epoch/config, poison/both counters, watcher single startup/failure/exit and shutdown. Explicit Instants/short timers; pending-only starts no watcher.

**Storage/lifecycle cannot be split here.** Reslice if too large; no retention half, allow attributes, dummy callers or toy reducers.

## C4: exact candidate-bound switch decision

Regions: different-identity commit, Slot proposal, snapshots, decide_switch, supervisor wait, termination/expiry/tests. No new watcher.

Replace SwitchRequired with one nonterminal AwaitingSwitch candidate in Slot. Generate decision UUID outside lock; guarded publication binds attempt/generation, candidate key, registration, baseline epoch and approved capped deadline. Incumbent remains separately managed. Native `decide_switch(attempt_id, generation, decision_id, Accept|Decline)` rechecks all bindings/admission/usability under the mutex; Accept uses C3 atomic replacement. Decline/timeout/cancel drops proposal only. Await in original supervisor; no await under lock. Replay/stale decision cannot rewrite newer terminal. Disconnect destroys both; invalidate(SwitchAccount) is disconnect, not reusable consent.

A/B tests: exact consent; wrong/replayed IDs, epoch/expiry, expired incumbent, decline/cancel/reaping; Accept-versus-cancel/disconnect/Drop, once-terminal/unwind. Reuse C3 lifecycle tests. Native switching only; no provider/product acceptance.

## Gates and evidence

Each slice: frozen diff/receipt, independent static/focused runtime/fmt/strict review, root combined integration, next isolated rebase. Failure freezes rejected bytes/logs; stop import pending bounded correction. Future src-tauri commands under root cache grant: `cargo test --locked --offline --lib drive_auth::session -- --test-threads=1` (+account/registration); `cargo fmt --all -- --check`; `cargo clippy --locked --offline --all-targets -- -D warnings`. Reducers do not replace transport/combined checks.

Read S1 production/contracts/review, A/B. Reuse existing Mutex/Tauri spawn/spawn_blocking/Tokio oneshot/watch/select/sleep; no new API/provider claim. Private `/private/tmp/rapidraw-gd1b5-adoption-slices-20261016` contains exact dependency hashes,634-input pre/post inventory/custody, document SHA and receipt. Detached HEAD/index read directly; no Git execution.

Pass: design/custody. Not exercised: implementation/build/test/cache/socket/provider/browser/device, A acceptance/B implementation, configuration/adoption/Drive. Stop for review; no implementation or credentials request.
