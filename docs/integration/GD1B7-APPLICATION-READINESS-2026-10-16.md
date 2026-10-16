# GD1b7 — native application owner and token-free facade

Design only, gpt-6-astra, 2026-10-16; canonical `e79bdc0f096df05e821e8988803daf3332f43af8`. Browser integration records 502Pass/14ignored/fmt/strict; those are prior native/synthetic gates. I authored browser/session components: this is application-readiness mapping, not independent review of them. No real Google activation exists.

## Two serial packages

**A: NEW `src-tauri/src/drive_auth/application.rs` only**, plus assigned receipt/tests. Implement a real `DriveApplication` owner, strict request codecs, bounded response mapping, configured begin→browser handoff, cancellation/decision/disconnect and native shutdown. No Tauri registration yet; this package has meaningful native behavior and serialization tests, not placeholder wrappers. Existing serde/JSON/UUID/runtime suffice. Root later exports it in mod.rs.

**B: one coherent root integration grant:** NEW `drive_auth/commands.rs` wrappers; existing lib.rs management/invoke/exit hooks; mod.rs; capabilities/default.json; narrowly authorised SettingsPanel.tsx restart call. Manage one `Arc<DriveApplication>` before `.build`, so it exists before setup-created webviews (Tauri app.rs:2688). No per-command/window controller. Root must integrate exit/restart changes with command registration; exposing configured begin before cleanup wiring is unacceptable. No UI feature implementation or configuration activation belongs to A/B.

## Native configuration and ownership

`DriveApplication::from_native_records(Vec<NativeRegistration>) -> Result<Self,FacadeError>` builds the existing immutable catalog, retaining only ≤8 selector IDs/allowed capabilities for options. Default production records are **empty**; constructor remains task-free (`session.rs:396–420`). Valid begin against empty catalog returns NotConfigured before reservation/socket/browser. Owner-approved native build configuration and its review/hash must later establish client/project provenance; syntax and about.get cannot attest that mapping. No env/settings/Clerk fallback, guessed ID, renderer client/project/endpoint/browser override or configuration reload. Scope choice is explicit `stateLibrary`→appdata, `photos`→file, or both in a fresh transaction; actual grants never accumulate by assumption.

A holds one non-Clone SessionController and a bounded closing gate; no retained task handle/cycle or second credential owner. `begin` awaits `begin_configured(selection)`, then moves that exact ticket into `browser::hand_off`. Recheck closing after ticket delivery and drop it on closure. It returns only HandedOff, **not** authenticated/connected success. Ticket key is private: do not invent a correlated begin receipt from a racy snapshot. Native future Drop before ticket return cancels the reservation; after dispatch browser retains its actual cleanup fence. Tauri `respond_async` spawns independently (ipc/mod.rs:329): abandoning a JS promise is not native cancellation. Provide explicit cancel; duplicate begin remains Busy without overwriting incumbent/status. No automatic retry.

## Bounded wire contract

CamelCase, schemaVersion=1, closed enums, canonical lowercase UUIDs, u64 generation/epoch/project values as canonical decimal strings. Use an outer invoke argument `{request: JSON.stringify(dto)}` capped at4KiB, parsed with strict serde struct decoding/end-of-input; reject duplicate/unknown fields, versions, malformed IDs, counts and scopes with static InvalidRequest. This avoids promising duplicate-key rejection after Tauri has already materialized an ordinary JSON object. Core IPC allocation is not bounded by this adapter check. Responses ≤4KiB; no personal DTO Debug, native-error formatting, URLs/state/code/verifier/tokens, events or logging. Poll authoritative getters.

Proposed commands (all prefixed `drive_auth_`):

| Suffix | Request → response/native mapping |
|---|---|
| options | version → configured, registrations[{id≤128 ASCII, allowedCapabilities≤2}] |
| begin | version, registrationId, capabilities[1–2 unique] → HandedOff or typed error |
| status | version → controllerGeneration, attemptId?, phase, lastAuthorization?, unverifiedGrantedCapabilities; snapshot() |
| verification_status | version → configured, controllerGeneration, attemptId?, phase, lastTerminal?; verification_snapshot() |
| connection | version → status, connectionId?, epoch, accountKey?, grantedCapabilities; connection_snapshot() |
| proposal | version → null or exact attemptId/generation/decisionId, candidateAccount/incumbentAccount, registrationId, actualCapabilities, baselineConnectionEpoch, expiresInMs; switch_proposal() |
| cancel | version, attemptId → Cancelled/NotCurrent/AlreadyTerminal; cancel(id) |
| decide | version, attemptId/generation/decisionId, Accept/Decline → AdoptedSessionOnly/Declined/Stale/AlreadyResolved; decide_switch(...) |
| disconnect | version → OwnershipInvalidated acknowledgment; invalidate(Disconnect), not cleanup completion or local-file deletion/revoke |

AccountKey is projectNumber plus bounded permissionId≤256 ASCII, never email. Proposal expiry is floor/clamped `deadline−Instant::now()` milliseconds, maximum60000; zero becomes no actionable proposal. Reads never extend deadlines. ConnectionSnapshot exposes no retention deadline: omit a connection countdown. Separate getter responses are separately observed, not an atomic combined snapshot. `controllerGeneration` is not necessarily the retained Slot's attempt generation; only proposal/terminal records carry that tuple.

Decide echoes only the immutable decision tuple. Candidate/account/registration/actual-grant/baseline-epoch binding remains inside the controller's single commit lock; never precheck a snapshot then claim atomicity, or accept account-key-only consent. Cancel's actual API accepts UUID only; do not pretend it checks a supplied generation.

FacadeError codes: InvalidRequest, NotConfigured, UnknownRegistration, DisallowedCapabilities, Busy, Closing, Closed, Stale, TimedOut, Cancelled, LaunchFailed, WorkerFailure, AttemptEnded, ControllerFault. StartError::Terminated maps to AttemptEnded. Map other native errors explicitly, never stringify them. Terminal enums mirror fixed native semantics; Failed(AccountError) becomes bounded verificationFailed with cause authRequired/accessDenied/unavailable/invalidResponse/bindingMismatch/setupFailed/cancelled/timedOut. No raw provider/native description.

## Mandatory application-lifecycle correction

`lib.rs:2545–2560` currently prevents exit then immediately `_exit`/`process::exit`, bypassing cleanup. B must replace that normal path: synchronously mark closing once and invalidate(Disconnect), prevent repeated exit requests, then await controller.shutdown off the event thread. Preserve requested exit code. Allow final exit after shutdown Ok or its post-fence Poisoned/CounterExhausted/WatcherFailure result (:634–656); Closed/channel failure is not cleanup proof. Preserve the fault outcome separately. No lock across awaits. A stuck launcher keeps Draining; crashes/SIGABRT handler(:126–138), forced termination and runtime failure cannot promise graceful cleanup.

Restart is a second blocker: `prevent_exit` ignores restart (Tauri app.rs:90). SettingsPanel.tsx:763 calls plugin relaunch; process:default grants restart. B replaces that call with a main-window-only native drain-before-request_restart command, and removes direct process restart permission while preserving exit. Headless exit callers(:2113/:2117) retain codes and use the same gate. Ready state prevents recursive exit handling. Tasks may hold managed Arc while draining, but owner stores no task/AppHandle cycle.

Current app commands lack an AppManifest ACL. B must require injected main-window identity on every wrapper and preserve Tauri's remote-origin denial (webview/mod.rs:2080); do not invent allow-command permissions. Adding a partial AppManifest would gate unrelated commands globally and needs separate permission migration. Legacy raw begin remains absent from IPC.

## Gates and stop

A tests strict codecs/precision/redaction, empty config/no work, invalid selectors/scopes, actual native Busy/cancel/drop, stale mappings and local-edit independence. Private synthetic ticket-consumer seams may test failure/denial plumbing; no sealed proof fabrication or OS-success claim. Existing C4 tests remain the native switch/expiry oracle; literal DTO fixtures test serialization only. B separately tests actual main/non-main/remote IPC, one owner, repeated exit/restart, draining/fault cleanup and no generated URL crossing IPC. Browser/provider/full positive OAuth require later operational grants. Freeze on API mismatch; no helper repair now. Source mapping/custody Pass; implementation/runtime/provider Not exercised. Storage/refresh/leases/jobs/transfers/sync/Android/configuration remain excluded. Stop for root review.
