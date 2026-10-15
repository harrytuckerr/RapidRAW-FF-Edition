# GD1b5-S2 — authenticated account candidate, before adoption

Design-only, 2026-10-16 Melbourne, actual `gpt-6-astra`; task `01a1404e-e04f-7053-9435-583b6862c66c`. Snapshot `drive-desktop-session`, detached `c5e1bdcf76de64bad1b87bcaa9e2526c95457efa`. S1 `session.rs` **f49ac5ed** remains a candidate pending final validation custody/root acceptance. During this design, the leader reported independent focused29Pass (21 author +8 independent), fmt and strict all-target Clippy Pass with source unchanged. That is attributed status, not a rerun or S1 acceptance by this author. This document neither accepts S1 nor implements S2/S3.

## 1. Next single-file grant

Assign NEW `src-tauri/src/drive_auth/account.rs` with inline tests, plus a separately authorized receipt. Implement one native authenticated request and bounded response parser. Root alone registers the module. No changes to session/exchange/parser/PKCE/model/mod/lib/Cargo/locks/README; no browser, IPC, refresh, storage, job retargeting, renderer or generic HTTP framework. Existing reqwest0.13.4, Tokio, serde/serde_json and model syntax types suffice. Stop if a protected API/dependency change becomes necessary.

The only production success constructor is the fixed authenticated HTTP operation. It consumes and retains the **same owned ExchangeResult** used for the request, with its original correlation and native registration context. It returns an unactivated `VerifiedAccountCandidate`, never ConnectedSessionOnly or Drive readiness. No raw-JSON parser may return this candidate; no public/default/deserializable/fake verified constructor, token setter, `into_parts` or token-replacement API.

Google documents about.get as a GET with an empty request body and required fields projection; both approved appdata/file scopes are accepted. About.user is the authenticated user. User.permissionId is the user's identifier in Permission resources; email can be absent. Thus this request can associate those bearer credentials with a Drive permission identity. It does **not** verify the configured client-to-project mapping, an email login, token audience independently, cross-device grant sharing, or workspace readiness. The configured project number remains the application's namespace choice. [about.get](https://developers.google.com/workspace/drive/api/reference/rest/v3/about/get), [About](https://developers.google.com/workspace/drive/api/reference/rest/v3/about), [User](https://developers.google.com/workspace/drive/api/reference/rest/v3/User), checked 2026-10-16.

## 2. Narrow proposed native API and provenance boundary

All following names are **proposed S2 APIs**, not existing methods:

```rust
// pub(super), private fields, no Debug/Display/Clone/Serialize/Deserialize.
VerificationContext::from_native_attempt(
    registration_id: &str,
    client_id: &str,
    project_number: &str,
    requested: &[ApprovedScope],
    expected_attempt: &ExchangeAttempt,
    admission_deadline: Instant,
) -> Result<VerificationContext, AccountError>;

async fn verify_account(
    exchange: ExchangeResult,
    context: VerificationContext,
    cancel: oneshot::Receiver<()>,
) -> Result<VerifiedAccountCandidate, AccountError>;

// Candidate borrows only bounded native metadata; it exposes no credentials.
VerifiedAccountCandidate::account_key(&self) -> &AccountKey;
VerifiedAccountCandidate::granted_scopes(&self) -> &[ApprovedScope];
VerifiedAccountCandidate::missing_requested_scopes(&self) -> &[ApprovedScope];
VerifiedAccountCandidate::display(&self) -> &AccountDisplay;
VerifiedAccountCandidate::matches_native_attempt(
    &self, expected: &VerificationContext, now: Instant
) -> bool;
```

Keep candidate/context/display/quota private-field and nonserializable/non-Debug/non-Clone owners; AccountError is a static Copy/Debug enum with bounded field/reason discriminants, no strings. `AccountDisplay` offers borrowed optional displayName/email and optional quota metadata, never identity decisions. `matches_native_attempt` checks exact UUID/generation, registration ID/client/project, requested scope-set equality, original deadline equality, now before that deadline and actual token usability. It is a comparison aid, **not** access to live controller state; S3 must call it under its own commit lock with newly read trusted state. No secret accessor is needed for S2 or storing the whole candidate in S3. Future provider access requires a separately reviewed bound lease seam, not splitting proof and tokens now.

Context construction validates configuration only. Use S1's client-ID and registration-ID bounds and unique nonempty approved request set. Parse project number as canonical **positive u64** and construct `UnsignedDecimal::new`; do not truncate/hash/map an overflowing string. Concrete seam: S1 accepts up to32 decimal digits, while model::UnsignedDecimal is u64 (model.rs:117–150). S2 therefore returns `InvalidRegistration(ProjectNumber)` before dispatch for an unrepresentable S1 value; S3/configuration selection must surface this before login where possible. No S1/model edit is authorized here. Reject nil expected UUID or zero generation, expired context and a result whose ID/generation differs from context.

**Trust cannot be manufactured from these inputs.** ExchangeResult (exchange.rs:38–48) has only private tokens plus attempt ID/generation; it carries no client ID/project/requested-set provenance. Context's native caller must take these from the same admission record, after checking that registration ID maps to the configured exact client/project record. This constructor cannot discover a dishonest or mistaken native mapping. No project/client prefix inference, token decoding or additional token-info request. `ProjectMappingMismatch` is an S3/configuration error; do not pretend about.get can detect it. S2's `AttemptMismatch` detects a different result ID/generation, not an undisclosed change inside the trusted native configuration source.

Before dispatch, require result grants to be a nonempty, unique subset of context.requested (only AppData/File). Preserve the exact returned subset. Missing requested scopes become a bounded `missing_requested_scopes` result, not fabricated grants and not an identity failure if at least one approved scope remains. S3 can disable the ungranted capability or require a fresh explicit authorization for a user action; no union with prior scopes or broader fallback. No OAuth grant automatically enrolls a sync category. The dated desktop scope addendum remains authoritative.

## 3. Fixed transport and deadlines

One GET to **`https://www.googleapis.com/drive/v3/about`**, exactly one query pair:

`fields=user(permissionId,displayName,emailAddress),storageQuota`

Construct the fixed reqwest::Url and append this constant through `query_pairs_mut()`. **Do not use RequestBuilder::query:** pinned reqwest0.13.4 request.rs:356 gates it behind the undeclared `query` feature. No URL argument, base URL setting, environment override, client injection, redirect target, thumbnail request, body, scope parameter, API key or client secret. Header-only Bearer authentication; Accept application/json and Accept-Encoding identity. No body/header/query logging or raw request/response Debug. [RFC6750 §2.1](https://www.rfc-editor.org/rfc/rfc6750#section-2.1).

Treat the access token as opaque. At the HTTP boundary require 1–8192 bytes and RFC6750 b64token syntax (one or more ASCII alnum/`-._~+/`, then only optional trailing `=`). Current parser intentionally accepts a wider visible-ASCII opaque token repertoire; an unsupported header credential is `InvalidBearerCredential`, not normalized/encoded/reinterpreted. Build a bounded HeaderValue containing `Bearer ` plus those bytes, mark it sensitive, and set Authorization once. reqwest's pinned `bearer_auth` also marks it sensitive (request.rs:274–280); explicit HeaderValue construction avoids an open-ended Display input. Sensitivity is a defense, not permission to log the request. Never put refresh tokens in this request.

Use a **fresh** client, exact established exchange policy: verified rustls, minimum TLS1.2, https_only(true), HTTP1 only, pool_max_idle_per_host(0), redirect Policy::none(), retry::never(), referer(false), no_proxy(), all four no_gzip/no_brotli/no_deflate/no_zstd, connection_verbose(false), no cookies, custom trust roots or certificate/hostname bypass. Direct-only choice means proxy-required environments fail visibly; no silent proxy fallback. Pinned client defaults otherwise enable system proxies and redirect/retry policies (client.rs:309–311; redirect.rs:160–164). Existing exchange.rs:214–238 demonstrates the available APIs. No dependency/features change is needed.

Overall deadline is `min(entry.checked_add(30s), context.admission_deadline, tokens.expires_at())`, never renewed. Connect/read bounds5s, client total30s; outer absolute timer covers header/body/parse/final return and caps all phases. If is_usable_at(now) is false, now>=expires_at maps to TokenExpired; otherwise fail as ClockRegression without inventing a validated_at getter. Check cancellation (sender send **or drop**), matching correlation and token usability before dispatch, after response/chunks/parsing, and immediately before constructing the candidate; cancellation/expired token never produces success. Yield between chunks so buffered progress cannot starve cancellation. The owner awaits/drops the one request future; no task list or retry loop. A result returned just before a cancellation remains subject to S3's atomic rejection. No hard abort of synchronous platform work or remote rollback guarantee.

Response limits: at most64 header entries and16KiB checked aggregate header-name/value bytes; critical header singleton checks. On status200 require application/json with no parameter or only charset=utf-8 (case-insensitive tokens; quoted UTF-8 accepted); Content-Encoding absent or identity only; reject duplicate/unsupported encodings. Content-Length optional nonempty ASCII digits (surrounding HTTP space/tab trimmed; leading zeros allowed), parsed with checked arithmetic, <=65,536; absence/chunked is allowed. Stream Response::chunk() into one checked <=64KiB body buffer, reject oversize before append; verify an advertised length equals final byte count. HTTP framing/truncation errors are transport failures. Singleton checks apply to headers exposed by reqwest; its HTTP stack may reject or normalize wire framing first. Do not claim a raw-wire parser audit. No `.json()`/unbounded `.bytes()`. These are application acceptance/retained-body limits, **not** an HTTP parser's preallocation or total-RAM cap. reqwest may have already allocated headers/a chunk before the adapter checks them.

Check cancellation/expiry, then header-resource limits, then status. Non-200 drops response without consuming/printing its body. No Location, WWW-Authenticate error description, response error URI, photoLink or other remote URL is followed. For200 only, enforce remaining content headers and parse the bounded body. Redirect rejection is intentional, including same-origin and307/308; the fixed target never forwards its bearer header. [RFC9110 §§15.4,15.5.2–4](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.4).

## 4. Bounded private parser and account key

Private `parse_about(&[u8]) -> Result<AboutFields, AccountError>` returns data, never proof. Use serde Visitors for root/user/storageQuota and IgnoredAny for extensions, not a serde_json::Value tree. A linear preflight bounds body64KiB, nesting16 and4096 structural punctuation units (container openings plus commas/colons outside strings, as the existing token parser does); serde remains authoritative for syntax/UTF-8/trailing content. At most32 decoded keys each in root/user/quota, keys<=128 UTF-8 bytes; reject duplicates in those three maps, including escaped aliases. Unknown well-formed fields are skipped under the same body/depth/work limits. No claim to reject duplicates inside entirely ignored extension objects. No disabled serde recursion limit/new feature. Non-JSON, NaN/Infinity syntax, root arrays/scalars, malformed/trailing bytes fail without retaining parser diagnostics.

| Field | Exact v1 rule |
|---|---|
| root.user | Required non-null object; missing/null -> MissingUser, other type -> MalformedResponse. No alternate root-level permission ID. |
| user.permissionId | Required nonempty string, unchanged bytes, no trimming/case folding. Pass existing ProviderId::try_from: <=256 ASCII alnum/underscore/hyphen. Missing/null -> MissingPermissionId; wrong type/empty/unsupported syntax -> InvalidPermissionId. This alphabet is the current conservative model policy, **not** a Google-documented exhaustive alphabet; unexpected legitimate syntax needs separate review, not guessed widening. |
| displayName | Optional; absent/null/blank =>None. Supplied nonblank string <=256 Unicode scalars and1024 UTF-8 bytes, no control characters. Wrong type/oversize/control -> InvalidDisplayMetadata. Do not normalize identity from it or render it as HTML. |
| emailAddress | Optional; absent/null/blank =>None. Supplied nonblank string <=320 UTF-8 bytes, no controls; bounded display text, not an email-validation/authentication algorithm. Wrong type/oversize/control -> InvalidDisplayMetadata. |
| storageQuota | Absent/null =>Unknown. Present must be an object. Its limit/usage/usageInDrive/usageInDriveTrash are optional/null or canonical nonnegative decimal strings <=i64::MAX; wrong type/negative/overflow =>InvalidQuota. Retain exact integers, no floating conversion. |
| quota.limit absent | With a present quota object, represent NoReportedLimit; Google documents omission for unlimited storage. Do not infer unlimited from an entirely absent quota object, or quota correctness from identity verification. Other usage fields may also be absent. |
| root.error | Presence on a200 response fails as ProviderErrorPayload, including simultaneous user; never surface its content. |

For a supplied displayName/email string, check byte/scalar/control limits before treating whitespace-only text as absent; oversized blank strings and control-only strings do not bypass validation.

Build AccountKey only inside successful request completion from configured positive UnsignedDecimal and parsed ProviderId. AccountKey itself is cloneable/serializable syntax (model.rs:213); obtaining/copying it is not obtaining a VerifiedAccountCandidate. Optional display/quota fields do not participate in account equality. Quota can concern an organization's pooled storage; it is not a per-user storage entitlement or a prerequisite for identity. [About quota fields](https://developers.google.com/workspace/drive/api/reference/rest/v3/about), [User optional email](https://developers.google.com/workspace/drive/api/reference/rest/v3/User).

## 5. Static failure categories and partial capability result

| Observation | Bounded result / owner |
|---|---|
| Invalid registration syntax/u64 project, invalid requested set, unusable header token | S2 InvalidRegistration / InvalidRequestedScopes / InvalidBearerCredential; no request |
| Context ID/generation differs from owned result | S2 AttemptMismatch; no request |
| Empty/duplicate/unrequested actual scope set | S2 InvalidGrantedScopes; no request |
| Requested capability absent but another approved grant remains | Successful account candidate may carry MissingRequestedCapability via its missing-requested list; that feature remains disabled, no union |
| Signal/drop cancellation, expired token, deadline/clock anomaly | S2 Cancelled / TokenExpired / Deadline / ClockRegression; no candidate |
| TLS/DNS/connect/read/framing, unsupported headers/encoding, byte/depth/work bound | S2 Transport / HeaderLimit / InvalidHeaders / ContentEncoding / BodyLimit / ParseLimit; static diagnostics only |
| 401 | Unauthorized: credentials were not accepted; not a definitive revocation/refresh diagnosis |
| 403 | Forbidden: do not assume scope alone is the cause; no request for broad access |
| 3xx,429,5xx,other non200 | Redirect / RateLimited / Server / UnexpectedStatus; no retry in this component |
| Missing user/permission ID, malformed known data,200 error payload | Distinct MissingUser / MissingPermissionId / InvalidPermissionId / MalformedResponse / InvalidDisplayMetadata / InvalidQuota / ProviderErrorPayload |
| Native registration changed/misconfigured, or candidate registration differs at commit | **S3** ProjectMappingMismatch/RegistrationMismatch; provider does not prove mapping |
| Session generation changed after dispatch/result, closed/cancelled/replaced slot | **S3** StaleAttempt; drop candidate atomically without replacing a session |

Failure consumes and drops this new verification input; it neither alters a prior connected session nor initiates refresh/login/revocation. No error variant carries raw token/body/header/URL/email/permissionId or third-party error text. Account verification does not justify displaying raw HTTP error pages. 401/403 have different meanings, and403 can occur for reasons unrelated to credentials. [RFC9110 §§15.5.2,15.5.4](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.5.2), [RFC6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1).

## 6. Exact later S3 seam — not authorized in S2

Frozen S1 has private NativeStart fields (:29), Slot.registration/pending (:160), Pending::Exchange (:190), adopt_pending (:626), and no pending-take API. Its authorization `live` predicate (:714) requires terminal=None; pending authorization already has CompletedPendingVerification. Its `run_attempt` (:885) enters retain_pending (:987), then supervise (:851) unconditionally reaps the slot. Therefore **adding an external getter/take while leaving that watchdog/supervisor unchanged is incorrect**: it could reap a verification or adopted session.

A separate **session.rs-only S3 ownership grant**, after S1/S2 gates, must:

1. Under the existing mutex, check the same current key, pending phase, completed-pending authorization outcome, deadline and original token usability; verify the slot's registration trio against the authoritative native configured registration chosen at admission. Construct VerificationContext from these private fields and current key. Move the exact Pending::Exchange out once, mark VerifyingAccount and retain the occupied slot/stop receiver/original deadline. No public NativeStart getter or public token-take command is needed; this code resides inside session.rs. If the trusted registration catalog is not yet available, stop at an explicit integration prerequisite rather than call syntactic strings authenticated configuration.
2. Replace the pending-retention continuation in the **same supervisor** with the bounded verification continuation. Await verify_account outside the mutex, bridge existing stop-watch to an owned cancellation sender (or drop the owned verifier future on stop), and preserve resource fencing. Do not leave a second pending watchdog able to reap the slot concurrently. The original deadline continues to govern verification/adoption.
3. Add a specific verification-commit predicate (do not reuse authorization `live`). Under the same lock require current identity/generation, expected verifying phase, nonclosed/uncancelled state, exact current registration binding and `candidate.matches_native_attempt(expected, now)`. Reject stale/mismatch/expiry by dropping the **whole** candidate. Authorization's once-only CompletedPendingVerification record remains unchanged; verification/adoption is separately reported.
4. A later ConnectedSessionOnly record must be a separate private State field owning the whole candidate, not Slot.pending that supervise::reaped deletes. Move it atomically only after all checks; resource cleanup may then release the attempt slot. Existing valid session survives failed reconnect; a different account requires explicit candidate-bound switch intent. Reduced grant sets replace rather than union capabilities. Disconnect/switch invalidates under this same lock; no local edit/profile/library mutation or remote revoke cleanup. Connected-token expiry/access leases need their own explicit S3 plan; this document grants no persistent/refresh implementation.

S2 can be independently implemented/tested without these changes, but cannot be wired to the app without that producer/consumer grant. Root owns registration/mod/lib and any future trusted configuration selection. No hidden exchange metadata accessor, no model rewrite, no callback into arbitrary caller code under a lock.

## 7. Independent validation and stop points

S2 author defines tests, with execution only under a separate grant. Required fixtures use hard-coded synthetic expectations; no real Google account:

- Private cfg(test) localhost endpoint plus short limits, same request builder/reader/parser/final checks; production entry accepts no endpoint/client/trust override or environment switch. Capture exact GET/path/one decoded fields pair, empty body, only Authorization containing the synthetic token, sensitive header marking, no secret in URL/errors. Capture redirects at a second listener and prove **zero** forwarded requests, including307/308; drop/stall to prove no automatic replay.
- Actual socket success with tokenA -> permissionA and tokenB -> permissionB; candidate retains the very owned input used on that request, no replacement/into-parts API. Swap contexts/IDs/generations: reject before dispatch; mismatched configured project at S3 commit: reject there. A synthetic server claiming permissionB for tokenA is merely a server fixture, not a client-detectable authenticity oracle; do not invent a test claiming S2 can detect a dishonest trusted provider.
- Valid no-email/no-quota/unlimited-quota, partial granted subset and missing-capability list. Missing/null/wrong-type user/ID; escaped duplicate user/permissionId/known fields; root error+user; overlong/controls; integer bounds and null policy; unknown nested extensions; bad UTF-8/trailing/oversized/deep/work-heavy JSON.
-401/403/429/5xx and HTML bodies; missing/duplicate Content-Type, encoding/length mismatch, chunk overflow/truncation, headers over cap, total/read stalls and drips; sender send/drop before/mid/final completion; token already expired or expiring during stream/parse; original admission deadline earlier than30s; late S3 generation invalidation in its later real-mutex suite. Error redaction assertions must use synthetic canary values.
- Bounded ephemeral servers with stop flags, finite I/O and joined cleanup even on panic. No detached unbounded tasks, proxy environment mutation or actual Google traffic.

**Test construction seam:** sibling account.rs cannot construct frozen ExchangeResult's private fields. Do not add a helper constructor or invoke production Google exchange. A private account-local `OwnedInput` enum may wrap the real ExchangeResult in production and, **only under cfg(test)**, hold synthetic ParsedTokenResponse + ExchangeAttempt created with the existing public pure token parser. The same private transport execution takes this owner; the candidate is still constructed only after the actual synthetic socket response. This tests transport/binding logic, not provider identity. No fixture variant/constructor exists in production, and no parser directly creates a candidate. Independent validators may use a separately granted child test module to reach this seam; no dependency/helper edit is required.

After source review and root-only module export, future commands from src-tauri with the explicitly assigned pinned1.98/offline/no-debug/no-incremental cache: `cargo test --locked --offline --lib drive_auth::account`; combined drive_auth tests; `cargo fmt --all -- --check`; `cargo clippy --locked --offline --all-targets -- -D warnings`. No such command ran for this design. S1 final acceptance remains pending; S2 does not self-accept or approve S1 authored code. Later designated-account tests must verify the owner-approved registration/project mapping, fresh appdata/file/both grants, actual TLS/provider permission IDs, two-account switching and fresh cross-client behavior. No credentials/registration/account changes are requested now.

## 8. Frozen evidence and receipt

Read-only input checks cover all614 tracked files plus frozen S1 source/receipt (616 files). Full exact hashes and pinned registry source hashes are in private `/private/tmp/rapidraw-gd1b5-account-contract-20261016/inputs.json`; before/after records and final document hash are in delivery.json. Key inputs:

| File | SHA-256 |
|---|---|
| session.rs | f49ac5ed30090288199f72cc2c3659499e9ae4dcede5d461bf538c6a58e8a6bc |
| S1 receipt | 2410f2493390ec9ad820312dc43d0778d91be220c3938159adf093b44a28e351 |
| exchange.rs | f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408 |
| token_response.rs | 7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886 |
| pkce.rs | 85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8 |
| drive/model.rs | f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0 |
| drive_auth/mod.rs | bc7e526d8ec1082ce55e51c28a3f60ef4e1cf0519d7577daa1178df055e6ef3d |
| Cargo.toml | 454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2 |
| Cargo.lock | 7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def |
| Approved S1 contract / lead review | 11950b2f77d6e4d5174c8023a74912b706c39ec03a1b93e40f3019115d0716f1 / 3e842938847da9f27fe724a8f1c5282a17db603138cf11356454af79693380fe |

Pass: frozen source/API/dependency inspection and current primary documentation review. Not exercised: S2 code, compile/lint/tests/sockets, OS browser, authenticated HTTP, credentials, accounts, provider acceptance, adoption, devices or sync. Existing files remain immutable. User documentation initially timed out and a subsequent direct fetch returned the field definitions; no authenticated API was called. API/layout limits above are chosen adapter policy, not measured provider compatibility or hard resource/security guarantees. Only this new design document and its private evidence were written; stop for root review before any implementation.
