# GD1b5-S3 — configured native verification and atomic session adoption

Design only, 2026-10-16 Melbourne, `gpt-6-astra`, task `01a14073-fa28-73c1-872e-a7a6f1612c16`. Read-only snapshot `drive-desktop-account` at `bd7f2d00a498315f48898c88ed99fbb225fa1113`. S1 is canonical/accepted with reported368Pass/14ignored/fmt/strict; S2 account7834f144 is a candidate undergoing independent validation. No S2 acceptance, new implementation or provider result is asserted here.

**Decision:** preserve legacy pending-only behavior; add an explicitly configured path. Before the one session.rs adoption writer, grant two small prerequisites serially: account lifetime/test support, then a pure readonly native registration catalog. No existing method or private field is assumed available when it is not.

## 1. Serial packages and exact ownership

| Package | Writable source, after a separate explicit grant | Implement and stop at |
|---|---|---|
| **A — candidate lifetime/test bridge** | Existing `src-tauri/src/drive_auth/account.rs` only: two metadata methods, one cfg(test) support module and narrow tests | Frozen diff; unchanged production HTTP/parser/candidate construction. Independent review/runtime before C. S2 must first pass its own gate. Do not touch the validator-owned child tests. |
| **B — native registration catalog** | NEW `src-tauri/src/drive_auth/registration.rs`, inline pure tests | Validated immutable native configuration and safe selection; no provider/runtime/I/O, no real registration values. Root alone exports module. |
| **C — configured session owner** | Existing `src-tauri/src/drive_auth/session.rs`, inline tests only; one writer | Explicit configured verification, first/same-account adoption, candidate-bound switch decision, connected expiry and same-lock cancellation. Preserve pending-only APIs/tests. Stop before browser/IPC/provider credential leases or application registration. |

Each package gets its own receipt/hash/review and an isolated snapshot. Root owns mod.rs/lib.rs/export/README/integration and all dependency/lock files. No helper edits hidden inside C; no new dependency. Browser handoff stays the existing native ticket. Secure store, refresh/revoke, cloud configuration/accounts, UI, queue jobs, transfer/lease access and sync remain separate grants.

## 2. A: minimal sealed-candidate prerequisites

Frozen account.rs:213–245 exposes only metadata and `matches_native_attempt(expected, now)`. That method intentionally includes the original admission deadline. It is correct **before adoption**, and wrong as an indefinite post-adoption lifetime test. Add exactly:

```rust
VerifiedAccountCandidate::credential_expires_at(&self) -> Instant;
VerifiedAccountCandidate::credentials_usable_at(&self, now: Instant) -> bool;
```

Implement as borrowed delegation to the retained input's existing ParsedTokenResponse::expires_at/is_usable_at. The boolean checks token validation time/expiry only; it does **not** check session generation, registration, admission deadline, remote revocation or Drive readiness. Document it as post-adoption metadata, never an alternative to matches_native_attempt at commit. No raw credential getter, mutation, into_parts, new constructor, Clone/Debug/serde or new proof type. Existing matches_native_attempt is unchanged.

Required narrow tests use candidates obtained through the actual private verifier transport: before admission deadline both checks can pass; after admission but before credential expiry, matches_native_attempt fails while credentials_usable_at can pass; at credential expiry both fail; a clock before token validation fails; the returned expiry equals the fixture's immutable parsed expiry. No extension/default lifetime is invented by these methods.

**Sibling-test seam is an explicit A change.** C cannot construct frozen sibling candidates or ExchangeResult. Add an account-local `#[cfg(test)] pub(in crate::drive_auth) mod session_test_support` exposing only:

- `FixtureSpec`: synthetic label A/B, ExchangeAttempt, approved actual scope set, issued_at and expires_in. Private-field constructor produces fixed synthetic access/refresh strings via the existing pure token parser; no caller token string, account proof or raw about JSON constructor.
- `async verify_fixture(spec, context, cancel, loopback: SocketAddr) -> Result<VerifiedAccountCandidate, AccountError>`: use existing private OwnedInput::Fixture, Target::Loopback validation and the **same verify_at/request/parser/final gate**. Fixed about path/fields remain unchanged; finite short test limits. Invalid fixture creation returns an existing typed parse/config error. No new production error/success stub.

Give its types/functions/constructors `pub(in crate::drive_auth)` visibility as well (nested `pub(super)` functions would expose them only to account.rs, not sibling session.rs). This bridge is absent from production, scoped to drive_auth siblings during tests, and is not a public candidate factory. A candidate still requires a real synthetic HTTP response. The session tests own bounded ephemeral servers and hard-coded tokenA→permissionA/tokenB→permissionB responses/captures; the bridge does not infer the expected account. No Google call, production endpoint override, sibling private-field bypass or modification to current independent validator tests.

## 3. B: readonly native configuration, not provider-attested mapping

Scoped source search found NativeStart's caller-supplied syntax and S2 context checks, **no authoritative registration catalog** in drive_auth/lib/app_settings. Do not promote a NativeStart string into trusted configuration merely by naming it trusted.

Proposed new B API (names are new, not existing):

```rust
NativeRegistration::from_native_configuration(
    id: &str, desktop_client_id: &str, project_number: &str,
    allowed: &[ApprovedScope],
) -> Result<NativeRegistration, RegistrationError>;
NativeRegistrationCatalog::from_native_records(
    records: Vec<NativeRegistration>,
) -> Result<NativeRegistrationCatalog, RegistrationError>;
RegistrationSelection::new(id: &str, requested: &[ApprovedScope])
    -> Result<RegistrationSelection, RegistrationError>;
NativeRegistrationCatalog::resolve(&self, selection: &RegistrationSelection)
    -> Result<&NativeRegistration, RegistrationError>;
```

All fields private. Exact borrowed record getters: id(), desktop_client_id(), project_number() -> &str, and allowed_scopes() -> &[ApprovedScope]; selection exposes id()/requested_scopes() only. Retain the validated canonical project string for S2 context construction. No mutation/serde/filesystem/environment loader. Catalog0–8 entries; zero is explicitly unconfigured. IDs1–128 ASCII alnum/`._-`, installed-client syntax same as S1/S2 <=256 bytes, project canonical positive u64, allowed/requested sets unique1–2 AppData/File. Reject duplicate registration IDs and duplicate client IDs (especially conflicting projects), unknown selection and requests outside the record's allowed set. Configured allowed scopes are product policy, **not actual grants**. No scope accumulation, project-prefix guessing, upstream sample client, arbitrary URL or permissive fallback. Resolve an exact ID, not prefix/fuzzy/default selection.

Provenance policy: later root application setup passes owner-reviewed native build configuration records with a recorded source/review hash. Until a separate registration/operational grant supplies actual public client/project mappings, the application catalog remains empty and Connect stays unavailable. No real values are invented in B/C. Do not load configuration from ordinary app settings, sidecars, renderer input, environment variables or an arbitrary file path in this slice. A future IPC may pass only a registration selector and capability choice; native code resolves client/project and exact scopes. Client/project membership is a trusted setup assumption, not proven by string validation, about.get, an email or unknown id_token. No credential, billing or project creation request now.

Catalog is immutable for the controller's lifetime. There is **no hot reload**. A future configuration replacement must close/shutdown the old owner (including Draining fence) before registering a new owner; it must not activate a parallel owner while cleanup remains outstanding. Re-check the selected catalog record against the attempt's retained registration at verification extraction and adoption anyway. A mismatch is RegistrationMismatch, never silent retargeting. Pure B tests cover empty/missing catalog, duplicate/conflicting records, overflow/project canonicalization, exact selection, approved capability subsets and no change to an existing immutable record.

## 4. C: explicit APIs and compatibility

Preserve `SessionController::new()` and `begin(NativeStart)` exactly as **pending-only** behavior: no account request, no Connected state and no silent conversion of unconfigured attempts. Raw begin remains pending-only even on a configured controller; it cannot adopt/replace a connected owner. All paths share the one occupied-slot/Busy fence.

New native APIs:

```rust
SessionController::with_catalog(catalog: NativeRegistrationCatalog) -> Self;
async SessionController::begin_configured(
    &self, selection: RegistrationSelection,
) -> Result<BrowserLaunchTicket, StartError>;
SessionController::connection_snapshot(&self)
    -> Result<ConnectionSnapshot, ControllerError>;
SessionController::decide_switch(
    &self, attempt_id: Uuid, generation: u64,
    decision_id: Uuid, decision: SwitchDecision, // Accept or Decline
) -> Result<SwitchDisposition, ControllerError>;
```

No renderer/IPC registration in C. Empty catalog or use of begin_configured on new() -> NotConfigured before socket binding/admission; unknown selection -> RegistrationUnavailable; Busy does not mutate the incumbent/connected owner. with_catalog(empty) remains unconfigured and starts no expiry watcher. The new constructor for a nonempty catalog requires the native runtime just as existing spawn-based work does; no fictional try_spawn API. A failed runtime setup must not return an owner advertising availability.

Keep AuthSnapshot's existing authorization meaning. Add a separate native-only ConnectionSnapshot: configured/not-configured status; current attempt verification phase; connected status (Disconnected/ConnectedSessionOnly/Expired/Closed/Fault); bounded account key/actual scopes if relevant; at most one pending SwitchProposal; one last verification/adoption terminal record. SwitchProposal copies only bounded account-key/name/email/actual-scope metadata plus its decision UUID/deadline; display fields do not choose identity. No Debug/serde implementation for these new personal-metadata snapshots. No tokens, URLs, raw provider strings or general event history. No serializer/IPC schema now. ConnectedSessionOnly means the sealed candidate was committed and is locally usable; it is not cloud readiness or permission for queued jobs.

## 5. Ownership and exact transitions under one mutex

Retain non-Clone SessionController; tasks hold Arc<Inner>, never the owner. Inner has an immutable optional catalog; the **existing State mutex** protects everything mutable. Add only bounded owners/records:

- Slot mode PendingOnly/Configured and the selected record binding; expected connection epoch at admission; verification phase/outcome; at most one switch candidate/decision token.
- `connected: Option<ConnectedOwner>` owning the **whole** VerifiedAccountCandidate, a connection UUID, checked connection_epoch and local retention deadline. It is not in Slot.pending and survives attempt reaping.
- One `connection_identity` stamp (AccountKey + registration/connection epoch) retained after credential expiry for reconnect comparison, with no tokens. Cleared on explicit disconnect. No growing account history.
- Checked connection_epoch, one last verification terminal record, and one connected-expiry watcher-running flag. Existing attempt generation remains its separate correlation counter.

| Trigger | Required commit behavior |
|---|---|
| Raw begin/code exchange succeeds | Existing pending outcome and retention behavior only. Never enters verifier. |
| Configured exchange succeeds | Record existing once-only CompletedPendingVerification; mark verification started. In the **same supervisor**, acquire lock, require current UUID/generation, configured mode, correct phase, that completed authorization outcome, unchanged catalog mapping and valid original deadline/tokens; move exact Pending::Exchange out once, build S2 VerificationContext from the same Slot fields/key, enter VerifyingAccount. |
| Verification running | Await real verify_account outside lock, using the existing stop/deadline cancellation path. No second verifier task or surviving pending watchdog. Original300s admission deadline still caps verification and later switch confirmation; S2's30s request cap cannot renew it. |
| Verification returns | Re-lock; re-read real monotonic time and current configuration. Require same key/generation/mode, VerifyingAccount, no verification terminal, same connection_epoch baseline, no close/cancel, original deadline not reached and candidate.matches_native_attempt against context rebuilt from **current retained/native state**. Failure drops the whole candidate. |
| No previous connection identity, or same AccountKey | Checked next connection_epoch first; then atomically move candidate into connected, replace any old connected owner, record new identity/epoch and actual granted subset. Requested/prior grants are never unioned. Record verification outcome Adopted and mark attempt Draining; supervisor reaps only Slot after owned cleanup. |
| Different AccountKey (including changed project namespace) | Retain candidate only in Slot as AwaitingSwitch. Keep incumbent usable. Generate one decision UUID outside the lock and publish it only after a guarded commit; bind it to current attempt/generation, candidate key, selected registration, baseline connection_epoch and confirmation deadline. Do not auto-switch because the browser chose another account. |
| Explicit Accept decision | Under same mutex recheck every binding, current epoch, deadline and candidate usability; atomically replace connected/identity and increment connection_epoch. Record Adopted once. No external call or callback under the lock. |
| Decline/cancel/verification failure/timeout | Drop only this attempt's result/candidate, record its verification outcome once if verification began, retain the prior still-usable connected owner/identity. Drain actual attempt resources before new admission. |
| Explicit invalidate(Disconnect or SwitchAccount), shutdown/Drop | Invalidate attempt generation and connection epoch, signal current work, discard pending/switch/connected secrets and clear connection identity. No revoke, local-edit deletion, job retarget or automatic login. SwitchAccount invalidation is disconnect semantics, **not** a reusable approval to adopt any future account. |

The new verification commit predicate must NOT reuse S1 `live()`: that requires authorization terminal=None, whereas pending success is already terminal. Do not reset the old authorization terminal merely to make live() pass. `run_attempt` currently ends at retain_pending, and `supervise` always reaps Slot: only PendingOnly keeps that continuation. Configured mode replaces it with one owned verification/decision continuation; no public token getter/take. Candidate moved into connected is excluded from reaped/terminate-attempt cleanup; explicit disconnect/owner faults clear it separately.

`decide_switch` is a one-shot decision on the exact presented candidate, not an account-key-only approval. Confirmation deadline is min(original admission deadline, credential expiry, proposal time+60s); no extension on repeated calls. Unknown/stale IDs, repeated decisions and late confirmations are inert typed Stale/AlreadyResolved dispositions. A changed connection_epoch rejects the proposal. Credential expiry of the incumbent drops only its tokens and marks Expired, retaining its identity/epoch: a same-account reconnect still works, while a different candidate still requires the explicit decision. This avoids treating expiry as a first-connection shortcut. Explicit disconnect clears that identity; a subsequent new connection is a new user-requested connection, not silent replacement of an active session.

Cancel-versus-adopt is serialized: cancel first invalidates the attempt and preserves incumbent; adopt first records success, and stale attempt-cancel does not undo the newly connected owner. Only explicit disconnect/close can then remove it. Failed reconnect does not invalidate a still-usable session just because attempt generation advances; connection lifetime/epoch is independent. No provider credential lease exists in this package, so no task may infer permission to resume/retarget Drive jobs.

## 6. Lifetime, notifications and cleanup

**Connected lifetime policy:** after guarded adoption, retain until `min(candidate.credential_expires_at(), adopted_at.checked_add(1h))`. The1h cap is a conservative v1 in-memory retention policy, not a claim about Google's usual token lifetime. It never extends provider expiry and does not refresh. Before adoption/switch acceptance, original admission deadline and matches_native_attempt remain mandatory. **After adoption**, use the new credentials_usable_at plus local retention deadline, never the expired300s admission comparison. Any clock/lifetime inconsistency fails closed; no new secret access is needed.

Configured controllers own exactly one lightweight connected-expiry watcher in addition to the at-most-one attempt supervisor. It uses existing Tauri async spawn/Tokio watch/sleep primitives; it watches **connected ownership only**, never Slot/pending/verification. It reads a connection UUID/epoch/deadline under the mutex, releases the lock, awaits change/expiry, then rechecks the same identity before dropping expired credentials. A delayed timer for sessionA cannot clear sessionB. Snapshot and every adoption/decision entry perform the same expiry check; scheduler delay cannot advertise an expired session as usable. Keep a bounded expired identity stamp, no expired credential.

The watcher holds Inner only. Close/Drop clears secrets and wakes it; a guarded finally/unwind path marks it stopped and notifies shutdown. Shutdown waits outside the mutex for both existing attempt cleanup and watcher completion. Watcher failure/poison/counter overflow closes admission, signals cancellation and clears all retained credential owners; never recovers a poisoned state into service or wraps counters. A future runtime teardown can still prevent scheduled cleanup, and started blocking listener work remains cooperative/non-abortable. The original Draining fence and outstanding browser-ticket acknowledgment ownership are preserved. No unbounded task list, per-session expiry-task accumulation or lock held over await/browser/network/join.

Two bounded terminal channels:

- Existing last_authorization stays immutable once set for its attempt (including CompletedPendingVerification). A later verification failure is not rewritten as an OAuth denial.
- New last_verification records exactly one terminal outcome for a configured attempt **after verification begins**: Adopted, Declined, Cancelled, Expired, RegistrationMismatch, VerificationFailed(static AccountError) or WorkerFailure. AwaitingSwitch is a nonterminal stage, not successful connection. An authorization failure before verification starts produces no duplicate verification failure notification.

Connected expiry/disconnect changes connection lifecycle status, not either old attempt terminal record. A stale completion/decision returns an inert disposition and must not overwrite the last terminal record of a newer attempt; terminal setters always require the matching owned slot. Notifications are private wake hints; snapshots under the one mutex are authoritative. No extra toast or frontend event framework is part of C.

## 7. Meaningful C tests without forgeable proof

C tests exercise the **actual production reserve/extract/verify-result/decision/adopt/expire/cancel reducers and mutex**, not a second miniature state machine. Keep existing21 author S1 tests and independent regressions intact; future integration must preserve raw begin behavior.

A private cfg(test) verification-input variant may hold A's FixtureSpec and loopback address. Production carries only ExchangeResult. A shared verification continuation dispatches the production variant to verify_account and the test variant to A's verify_fixture; both return the same sealed candidate from the actual bounded HTTP implementation. Tests install this synthetic *input*, not a verified output, after reaching the real pending transition through private existing-style test controls. Admission uses B's real catalog/selection. No caller-visible endpoint, arbitrary future/callback injection or candidate constructor. Existing S1 loopback/denial tests continue to exercise real startup, tickets and cleanup; the seam does not claim a complete browser/code-exchange/Google chain.

Required independent rows:

- new()+raw begin remains pending-only; configured raw begin also remains pending-only. Empty/unknown/disallowed selections fail before binding; two admissions preserve Busy/incumbent and immutable catalog. Misconfigured project/client selection fails locally without inventing provider proof.
- Actual synthetic about transport produces A/B sealed candidates and captured exact token binding. First adoption persists after Slot reaping. Same-account reconnect atomically replaces credentials/scopes; failed/cancelled reconnect preserves the old valid owner. Partial new grants remove former capabilities rather than union them.
- Barrier races: verification result versus cancel/disconnect/Drop; same-lock configuration mismatch injection; stale generation, replaced epoch, poisoned mutex and both checked counters exhausted. No obsolete result can replace a session.
- Different-account candidate remains AwaitingSwitch while old session works; exact decision Accept/Decline; wrong attempt/generation/decision, replay, changed epoch and timeout are inert. Expired incumbent retains identity so the proposed new account cannot bypass confirmation. Explicit disconnect clears both credentials and identity.
- Admission deadline versus token lifetime: result after original300s is rejected; a candidate adopted before that boundary remains usable past it until token/local-cap expiry. Use A's metadata methods, with pure future-Instant comparisons and bounded timer fixtures; no waiting300s/1h in tests. Expiry race cannot commit an already-expired candidate; old timer cannot clear a newer connection.
- Watcher starts once, no owner-reference cycle, pending-only owner starts none; shutdown waits for actual cleanup/watcher exit outside lock, Drop signals both, panic/failure clears secrets and releases only legitimately reaped resources. Terminal authorization/verification records occur once; post-adoption expiry does not rewrite them.

All servers use finite synthetic requests, stop flags, bounded I/O and joined Drop. No realGoogle and no production test override. Freeze source/probes/custody, independently inspect race assertions, then run only with explicit isolated-cache/socket authority. Future commands from src-tauri: scoped `cargo test --locked --offline --lib drive_auth::account`, `drive_auth::registration`, `drive_auth::session`; combined native suite, `cargo fmt --all -- --check`, strict `cargo clippy --locked --offline --all-targets -- -D warnings`. Root alone performs canonical integration; fixture passes do not establish provider or product acceptance.

## 8. Evidence and stop point

This design read actual S1 current source, full S1 contract11950b2f and approved account contract8afb6aa7 §6; the current frozen S2 methods were inspected directly. Pinned Tauri2.12.1 spawn/JoinHandle and Tokio1.53.1 watch APIs are already used by S1; no new runtime method is invented. Scoped registration search covers drive_auth/lib/app_settings; it is not a claim to have scanned every unrelated application integration. No new provider claim was needed beyond the approved account contract's cited Google about/User/RFC evidence; no account/network request or credential inquiry occurred in this task.

| Dependency | SHA-256 |
|---|---|
| session.rs | 17ed9fec0d126895db683ae89fa9e056673e0fbae0dcc2119129369a1b2d6638 |
| account.rs candidate | 7834f144d3ca806dbdf669b15f62e714fa5b80d9d360ce331550534d441921f7 |
| exchange.rs | f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408 |
| token_response.rs | 7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886 |
| model.rs | f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0 |
| drive_auth/mod.rs | 5c59c0add8d68c464828e7719821ab3dd1b40296f29968abf0175178e7df4301 |
| Cargo.toml / Cargo.lock | 454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2 / 7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def |

Private `/private/tmp/rapidraw-gd1b5-adoption-contract-20261016/inputs.json` records exact source/contract/pinned-registry hashes. Before/after custody covers all624 tracked inputs plus frozen account source/receipt (626 files). delivery.json records this document's exact hash, checks and evidence. Only this new document/private evidence are writable in the present task.

Pass: source/API mapping and bounded design/custody checks. Not exercised: any A/B/C implementation, build/test/cache/socket/browser/account/provider/device operation, canonical S2 acceptance, real configuration provenance, adoption or sync. S2 remains independently pending; implementation must rebase to its accepted bytes after review. **Stop at this contract for root review.** The smallest next assignment is A only, after S2's gate; no simultaneous helper/session rewrite or downstream work is authorized by this document.
