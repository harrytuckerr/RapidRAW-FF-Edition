# GD1b4 — bounded native authorization-code exchange contract

Design only, frozen 2026-10-16 Melbourne / 2026-10-15 UTC by actual `gpt-6-astra`. Task `01a13fe0-ef80-7210-a5d1-f8a1e9ffa9b7`. **No implementation authorized by this document. Root must approve it and assign the next slice.** Worktree `drive-desktop-token-response`, detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`, under `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/`.

## 1. Next slice and prerequisites

Assign exactly one new source file, `src-tauri/src/drive_auth/exchange.rs`, with inline tests: one native asynchronous authorization-code exchange against Google's fixed token endpoint. Root alone adds the export/registration in an isolated review/integration snapshot; no worker edits `mod.rs`, `lib.rs`, Cargo, locks, PKCE, parser, listener, UI or Drive state. Use existing direct reqwest0.13.4, tokio1/full, uuid1 and std only; futures is already present but not required. No new features/crates or dependency resolution.

Inputs: accepted corrected PKCE `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8` (verified in canonical `drive-foundation-integration` HEAD `cc397890144615938f6036bea2e2e7b507206050`); parser `a943573b0ab5a00be6df8d7e5cd98f6d27c66fffb92766d7d2eeb9b233656f61` (static accepted, runtime pending in assignment). Listener is not an exchange-module dependency. Its prior definition `ee3b4ccb…` remains separately frozen/reviewed. Actual integration waits for prerequisite runtime acceptance; this design did not wait or claim those tests passed.

Read GD0 original contract `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19` and desktop scope addendum `6c34f7bafacab48573b2d70f6dfed7d74b868d2ec24f4920d7885356494c646c`. Exact appdata/file selection remains the complete fresh requested set, never incremental grant accumulation. No added identity or broad Drive scopes.

## 2. Small native API and ownership

Suggested concrete API in that single file:

```rust
// Metadata, not account identity or authorization proof.
pub struct ExchangeAttempt { pub id: uuid::Uuid, pub generation: u64 }
// Private fields; borrowed metadata/token accessors only; no Debug/Clone/serde.
pub struct ExchangeResult { /* attempt, ParsedTokenResponse */ }
pub async fn exchange_code(
    grant: AuthorizationCodeGrant,
    attempt: ExchangeAttempt,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<ExchangeResult, ExchangeError>;
```

Consume the existing non-Clone grant by value. Use its five native getters (`code_for_exchange`, `verifier_for_exchange`, `client_id`, `redirect_uri`, `requested_scopes`). No public endpoint, client, transport options, token strings or secret configuration argument. Do not return the grant/code on error or implement resend. Drop internal form/body/grant buffers after use; no guaranteed-zeroization claim. Result containing tokens must not implement Debug/Display/Clone/Serialize. Error is a bounded enum (`Cancelled`, `Deadline`, `ClientSetup`, `Transport`, `Redirect`, `ResponseHeaders`, `ContentEncoding`, `BodyLimit`, `TokenResponse(TokenResponseError)`), with no inner reqwest error/URL/header/body/parser dump. Metadata access does not establish account ownership.

Caller must run this async function on a native Tokio runtime with timers, never synchronously block the UI. Capture `operation_started` at entry. A oneshot send **or sender drop** means cancellation; preclosed/pretriggered cancellation must prevent dispatch. Race the owned send-and-read future against cancellation and an absolute deadline using a biased select with cancel/deadline before result. Recheck cancellation/deadline before dispatch and before returning a parsed result. Cancel by dropping the in-flight future/response/client; do not drain a hostile body or detach an unowned request task. Blocking platform certificate/DNS internals and scheduling are not hard real-time abort guarantees.

**Stale-result boundary:** carry the unchanged attempt UUID and session generation in the native result. The later controller retains the matching identity, invalidates/increments generation on cancel/disconnect/account switch, and under its single commit lock checks active ID+generation+cancellation before adopting tokens. A stale result is dropped without replacing existing credentials or changing UI/account state. Exchange itself has no global session authority; a late success cannot be treated as current solely because the future returned Ok. This controller check is a later implementation gate, not a stub to add to `exchange.rs`.

After POST dispatch, cancellation/timeout/disconnect may leave remote issuance unknown. Never retry the authorization code, assume it remained unused, or revoke an existing grant as cleanup. Require a new user authorization attempt if recovery needs a new code. Dropping local work does not prove Google rolled back issuance.

## 3. Fixed request, pinned-client policy

POST only `https://oauth2.googleapis.com/token`, no query, fragment, userinfo, path override or redirects. Exactly **five** form fields: `grant_type=authorization_code`, `code`, `code_verifier`, `client_id`, `redirect_uri`; values come unchanged from the owned grant. `client_secret` is currently optional in Google's native instructions. Do not fabricate a confidential installed-client secret, embed a secret, use HTTP Basic authentication, or silently retry with a different client type. Omission compatibility for the actual configured desktop registration is a live-provider gate; a rejection requires review, not a guessed credential. No `scope` request parameter, DPoP header or refresh-token grant in this slice.

Send `Content-Type: application/x-www-form-urlencoded`, `Accept: application/json`, `Accept-Encoding: identity`. Never place code/verifier/token in a URL, React, logs or request Debug output. Build a new private client for this attempt only:

| Explicit setting | Reason / pinned source evidence |
|---|---|
| `https_only(true)`, `tls_backend_rustls()`, minimum TLS1.2 | Keep certificate/hostname verification enabled, platform roots, no invalid-cert/hostname flag or custom trust bypass. `client.rs:733–786,2085,2153,2245`. Platform-installed roots can be trusted; this is not certificate pinning. |
| `redirect(Policy::none())`, `referer(false)` | Default follows up to10 redirects; no code forwarding to Location. `client.rs:1383–1397`. |
| `retry(reqwest::retry::never())` | Default retries some protocol NACKs; `retry.rs:84–89`, `client.rs:1401–1407`. No manual retry/backoff/429 replay. |
| `http1_only()`, `pool_max_idle_per_host(0)`, fresh unshared client | Avoid HTTP2 protocol retry cases and reused-connection replay paths. Never accept a caller-supplied shared client or clone the request. `client.rs:1503,1549`; hyper-util legacy client `send_request:257` only resends a retryable request when its connection was reused. Pre-dispatch connection acquisition/DNS/address attempts are not a second authorization-code POST. |
| `no_proxy()` | Direct connection chosen explicitly. Reqwest automatic matcher otherwise includes environment proxies even when `system-proxy` feature is not declared; platform discovery depends on unified features. `client.rs:415–416,1430–1433`, `proxy.rs:515–517`; hyper-util matcher `from_system:99–110`. No proxy credentials/configuration. Proxy-only networks fail, with no weaker fallback. |
| `no_gzip().no_brotli().no_deflate().no_zstd()` | These disabling methods exist even when decoder features are off; make future feature unification unable to silently decompress token bodies. `client.rs:1318–1375`. |
| `connect_timeout(5s)`, `read_timeout(5s)`, `timeout(30s)` | Defaults are none; read timeout resets on each read, total timeout includes body. Add outer absolute30s deadline from function entry so drips/build delay cannot reset the budget. `client.rs:1440–1473`. |
| No cookie provider/store, no added Authorization/Cookie, `connection_verbose(false)` | Cookie store defaults None, verbose false (`client.rs:300,371`). Do not call feature-gated cookie APIs that this crate does not declare. No shared ambient headers. |

Paths above are relative to `<HOME>/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reqwest-0.13.4/src/async_impl/` unless named otherwise. Desktop Cargo.toml:92 explicitly disables defaults and enables json/multipart/rustls; Android:123 is distinct from this desktop assignment. The lock also contains reqwest0.12 for other consumers; its API/defaults are not this contract's authority. Build feature unification has not been resolved/executed here, so explicit runtime settings must not depend on absence of optional transitive features.

**Form feature finding:** reqwest0.13.4 `RequestBuilder::form` is gated by `feature="form"` (`request.rs:426–429`, package Cargo.toml:121–124); the app does not declare it. Do not rely on incidental feature unification or add the feature. Implement a small private byte encoder for these five fixed pairs: ASCII alphanumeric/`*.-_` unchanged, space as `+`, every other UTF-8 byte as uppercase `%HH`, pairs separated by `&` and key/value by `=`. No secret-bearing temporary URL and no general serializer framework. Checked append/cap at8 KiB; current PKCE field bounds fit even worst-case percent expansion. Unit fixtures must cover `+ / % & = ? ~` and published form semantics. Use `.body(owned_encoded_bytes)` with the static content type. If root prefers a dependency feature instead, stop and obtain a separate explicit feature/lock ownership grant before changing this decision.

## 4. Response and parser handoff

Capture `request_started_at: std::time::Instant` immediately before actual send. Retain the exact requested scopes while the grant is owned. No `.json()`, `.text()` or whole-body `.bytes()` convenience read. `Response::chunk()` is available without the `stream` feature (`response.rs:310–322`); repeatedly await it within the original deadline/cancellation race.

Before body collection, reject 3xx as Redirect without following or reflecting Location. Validate response-header metadata after reqwest parsing: at most64 fields and16 KiB summed header-name/value bytes; reject duplicate Content-Type/Content-Encoding/Content-Length. Require case-insensitive `application/json`, allowing no parameter or one `charset=utf-8` parameter (case-insensitive, optional quotes/OWS); reject other types/charsets/parameter ambiguity. Content-Encoding must be absent or one `identity` token, never gzip/br/deflate/zstd or a list. Content-Length, when present, must be one checked decimal length no greater than parser `MAX_RESPONSE_BYTES` (65,536); absence/chunked is allowed. Do not use cacheability headers, cookies, trailers or provider diagnostics as authorization evidence. Reqwest owns HTTP framing; transport framing errors fail, no custom dechunker or trailer-based policy override.

Reserve at most the 64 KiB application accumulation budget. For each returned chunk, checked-add `buffer.len()+chunk.len()` and reject before extending if over budget; enforce even for chunked/absent/misleading Content-Length and non200 responses. Any read error fails, never parse a partial prefix. At EOF call existing `parse_google_token_response(status, bytes, grant.requested_scopes(), request_started_at, Instant::now())`. Do not use `error_for_status()` to lose a bounded OAuth error body. Non200 JSON can produce only parser errors; no account/renewal success. Recheck cancellation/deadline and result expiry before returning the native envelope. Wrong content type/encoding and oversize bodies return static exchange errors without printing them.

The 64 KiB cap bounds accumulated parser input, **not total process/transport memory**. A returned chunk and HTTP header parsing buffers exist before the application can reject them. Reqwest0.13.4 exposes no HTTP1 header-buffer size setter found in this review; pinned hyper1.11.0 has default HTTP1 buffer cap `8192 + 4096*100 = 417792` (`src/proto/h1/io.rs:23,444,527`). Post-parse header limits cannot retroactively prevent that buffering. Keep HTTP1, disabled decompression, one request/body, absolute timeout and bounded parser allocation; do not claim a64KiB total-RAM guarantee or add a new HTTP stack merely to restate the bound.

## 5. Private test seam and acceptance

Only `#[cfg(test)]` may construct a loopback HTTP target and shortened deadline policy. Production entry constructs the constant endpoint and verified HTTPS client internally; there is no public override, env endpoint, permissive certificate flag or production code path selecting test mode. Shared private execution should still use real reqwest request encoding/send/chunk parsing and the production token parser. A test-only HTTP client may differ only in allowing its explicit127.0.0.1 ephemeral target and shorter timeouts; keep redirect/retry/proxy/compression restrictions. Never relax production TLS to make a test pass.

Synthetic grants can be obtained through public frozen PKCE APIs with a synthetic client ID/state/callback; no private-field exposure or helper edit. Use real local TCP test servers (only when separately authorized to execute tests), not mocked successful token results. Test buffers contain fixed synthetic secrets, never real account credentials. Required regressions:

- Exact POST `/token`, form fields once, correctly escaped code/verifier/client/redirect, no URL secrets/client_secret/scope/Authorization/Cookie; native result preserves attempt binding and partial reported scopes.
- Valid fixed-length and chunked bodies; exact64KiB/overflow, declared oversize, missing length, malformed/truncated body/framing and empty response. Malformed scopes, duplicate keys and expiry are passed to the actual parser, not replaced by fixtures returning success.
- Redirect server plus separate capture server: no second request; dropped connection after receiving code: one POST and failure, no automatic resend. This supports local behavior, not every possible TLS/protocol failure.
- Content-Type charset/duplicates, encoded body despite identity request, ambiguous headers, oversized headers and no reflected Location/body/diagnostics.
- Header stall, body stall, continuous small chunks exceeding total deadline; cancellation already signaled or sender dropped causes zero dispatch; cancellation midstream stops awaiting and returns no tokens. Final cancellation/expiry races cannot return an adoptable result.
- Caller harness rejects returned attempt IDs/generations after a newer attempt/account-switch boundary; this is a contract fixture, **not** acceptance of the future real controller's atomic adoption.
- Static production endpoint/TLS policy and native nonserializable result reviewed separately. Mock/local HTTP tests do not prove Google TLS, consent, registration/client-secret omission, DNS/proxy reachability or actual scopes.

Future authorized review commands, from a root-registered isolated snapshot's `src-tauri`, pinned Rust1.98 and the established exclusive-cache/offline/no-debug/no-incremental/private-TMPDIR environment: `cargo test --locked --offline --lib drive_auth::exchange`, then `cargo test --locked --offline --lib drive_auth`, `cargo fmt --all -- --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`. Assert source/lock hashes before/after. Worker freezes source/tests/receipt after its granted source slice; independent owner executes tests/adversarial probes. Stop on a prerequisite API/dependency mismatch; do not edit protected helpers to force it through.

## 6. Subsequent owners and open gates

1. **Exchange transport only:** new exchange.rs as above; no browser/UI, account proof, persistence or refresh flow.
2. **Native controller/session-only:** later separate `drive_auth/session.rs` plus narrowly granted command wiring; own one active generation, cancellation, loopback→browser→code→exchange orchestration, authoritative terminal events and in-memory session lifetime. Secure storage is unavailable today: no plaintext fallback. Do not enable Drive/account UI merely on parsed tokens; separately implement authenticated provider account verification and reconcile actual access before adoption. Unknown `id_token` is not identity.
3. **OS secure storage and refresh:** separate platform ownership/dependency decisions and failure tests. Scoped search of `src-tauri/src`, `src`, Cargo.toml for keyring/keychain/Secret Service/credential-store/secure-store/PasswordVault/CryptProtectData returned no implementation. Generic `tauri-plugin-store`/Clerk dependencies are not evidence of an OS-protected Google credential store. No Keychain/Windows/Linux store crate is authorized now.

Refresh is not interchangeable with replaying an authorization code: future refresh grant uses refresh_token and client ID, and must handle missing versus rotated credentials, explicit scope evidence when responses omit scope, reduced grants, account/generation binding and revocation. The current conservative parser rejects missing scope even though RFC6749 permits omission when unchanged; future refresh compatibility requires an explicit reviewed contract, not substituting requested scopes or silently loosening this parser. Google documents time-based `refresh_token_expires_in` and optional DPoP-bound flows; current parser ignores those extensions, so no indefinite refresh lifetime or DPoP support may be claimed. Session-only work must avoid persisting unsupported credential metadata; later storage/renewal needs a deliberate policy and live tests. Existing offline edits survive auth failure; original-file transfer and profile sync remain unrelated later gates.

## 7. Evidence and receipt

**Pass (read-only):** frozen API/source/dependency inspection, scoped secure-store search, primary documents checked, explicit choices above. **Not exercised:** implementation, build/test/lint, local sockets, provider/Google HTTP, browser/registration, TLS/network/proxy behavior, consent/grant/account verification, OS credential storage, refresh and sync. Latest lead message reports listener independently accepted for canonical integration (16 listener/30 auth/274 default plus10 independent probes, format/strict lint); parser runtime validation is now assigned and still pending. These are attributed lead reports; this design task neither reran nor independently accepted those tests.

Primary documentation accessed 2026-10-16 Melbourne / 2026-10-15 UTC: [Google native token exchange and refresh](https://developers.google.com/identity/protocols/oauth2/native-app) supports the fixed endpoint/form fields, optional client secret, explicit response scopes and the stated refresh/DPoP caveats; [WHATWG form serializer](https://url.spec.whatwg.org/#application/x-www-form-urlencoded-serializing) defines the bounded encoder's escaping. [RFC6749 §§4.1.3,5,6](https://www.rfc-editor.org/rfc/rfc6749.txt) remains the base token/refresh semantics. Provider sources do not establish live compatibility with this user's registration.

Private evidence `/private/tmp/rapidraw-gd1b4-contract-20261016/inputs.json`, 4718 bytes, SHA-256 `cb8bae11d3b00e3096c9b66f9e7036e084535db33dfe3bfa84ea3d54186f7ee7`, binds helper/parser/module, protected manifests/locks/Drive source, pinned reqwest sources, GD0/addendum and exact scoped-search command/exit. Final document hash is supplied externally in delivery evidence to avoid a self-hash field. No source files were written for this design. Root design approval is required before the next implementation assignment.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `2682a642039bfb9a974fc583c4602ef325f176a71e9176aeb3098244e2fe1779` (19643 bytes).
