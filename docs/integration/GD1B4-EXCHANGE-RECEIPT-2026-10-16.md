# GD1b4 — native code exchange source receipt

Frozen 2026-10-16 Melbourne / 2026-10-15 UTC. Actual author `gpt-6-astra`, Team slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`; task `01a13fef-9364-76e3-b0f2-7c3f1d274439`. **Source and test definitions ready for independent review. No compile, test, socket or provider operation executed.**

Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-exchange`, detached `ec1d032efe7dd321e7a99d1931730b654b922ec3`. Full frozen contract `2682a642039bfb9a974fc583c4602ef325f176a71e9176aeb3098244e2fe1779` and `GD1B4-INDEPENDENT-LEAD-REVIEW-2026-10-16.md` were read before editing. Root's reported base317Pass/14ignored/fmt/strict-lint is prerequisite evidence, not validation of this new file.

## Frozen artifacts

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| NEW `src-tauri/src/drive_auth/exchange.rs` | 41333 | `a53690ccfc7a5c0bcb4d5bef6aa2d8c71c02764a7a7cc2249b6bdbaa36eb849b` |
| Private `source.patch` | 42668 | `956985463b1a017d8a41d227c8743d048c3c8e1829d74d1d5bc79936d169ec33` |
| Private `manifest.json` | 1487 | `b8a0be0da40e48a2dca7467ceb43b596a7008d54d3f85d569820cbd03154191b` |
| Private `protected-before.json` | 106082 | `0e2c1e45ca496856a07ee013a31aabc95c4f96c6c15c52fd0897ef622fa0af2f` |
| Private `commands.json` | 45891 | `a5dbb4e2852748ee1e677a56f39612f955f298eb904b967ca34e299d5e21de10` |

Private evidence root: `/private/tmp/rapidraw-gd1b4-exchange-20261016/`. All604 tracked protected files were hashed before writing and rechecked unchanged; independently generated `protected-after.json` is byte-identical to `protected-before.json` and has the same SHA-256. This includes module/lib, accepted PKCE/listener/parser, all Drive/renderer/UI/Android files, manifests/locks and contract/review documents. Only this new source and receipt were written in the worktree. No module export or registered command exists for the new file; root owns registration and integration. No Cargo/npm/build/shared-cache/Git/dependency/provider/device activity.

## Implemented boundary

`exchange_code` (line116) consumes `AuthorizationCodeGrant`, native `ExchangeAttempt { id: Uuid, generation: u64 }` and a oneshot cancellation receiver. It has no endpoint/client/trust/configuration parameter. `ExchangeResult` exposes borrowed native attempt metadata and the accepted `ParsedTokenResponse`; neither result nor attempt derives Debug/Display/Clone/Serialize. There is no account proof, controller, browser launch, secure store, refresh manager or UI command. One call owns one fresh request; admission across multiple attempts remains the future controller's responsibility.

Production target is exactly `https://oauth2.googleapis.com/token`, without query/fragment/userinfo. Native code/verifier/client/redirect getters feed exactly five application/x-www-form-urlencoded fields: grant_type=authorization_code, code, code_verifier, client_id, redirect_uri. No client_secret, HTTP Basic, scope parameter, DPoP or refresh grant. The private encoder preserves UTF-8 bytes through standard form escaping (alphanumeric/`*.-_`, plus for space, uppercase percent escapes otherwise); its checked append caps the encoded body at8KiB. No undeclared reqwest form feature, temporary secret URL or extra dependency is used.

`build_client` (line211) constructs a fresh private reqwest0.13.4 client: HTTPS-only, rustls backend/minimum TLS1.2 with normal platform certificate/hostname verification, HTTP1-only, zero idle pool, retry::never, redirect::none, referer disabled, no_proxy, all automatic content decoders disabled, connection verbosity false, finite5s connect/read and30s request timeout. Cookie storage remains the fresh builder's None; no ambient Authorization/Cookie headers or shared client is supplied. Direct-only networks are intentional: proxy-dependent environments can fail without fallback. OS trust roots are not pinning. This source does not prove live Google TLS/registration/client-secret omission compatibility.

The absolute outer30s budget starts on first poll of the async exchange. A cancellation send **or sender drop** rejects before dispatch. Send/read/parse is one owned future raced with cancellation and the original deadline using biased select; cancelled/timed-out work is dropped without draining a body or detaching an author-owned task. Each received chunk explicitly yields so a buffered stream cannot monopolize polling. The final guard checks cancellation, deadline and token expiry again. Synchronous platform setup and OS scheduling are cooperative limits, not hard-abort guarantees.

The native result retains the exact attempt UUID/generation. The later controller must atomically compare active identity/generation/cancel/account state before adoption. A cancellation arriving after final return can still make a result stale; this module does not invent that future lock. Codes are never returned for reuse or automatically retried after failure/cancellation. Dispatch may have caused remote issuance even if the local result is lost; no rollback, revocation or "unused code" claim.

## Response validation and resource bounds

3xx fails without following or exposing Location. Header policy, after reqwest parses HTTP: at most64 fields and16KiB total name/value bytes; duplicate Content-Type/Content-Encoding/Content-Length rejected. Require application/json (case-insensitive), with no parameter or one charset=utf-8 parameter, optional quotes/OWS; content encoding absent or single identity only. Content-Length is checked decimal and cannot exceed65,536. Chunked/close-delimited bodies remain allowed. HTTP framing is delegated to reqwest/hyper, not reimplemented.

Body collection uses `Response::chunk()` only, checked-adds each chunk length before extension, caps the accumulated buffer at accepted parser `MAX_RESPONSE_BYTES` (64KiB), and never parses a prefix after read failure. At EOF it calls the accepted parser with actual status, exact requested scopes, request-start Instant and current Instant. Non200/malformed/broad-scope/error responses cannot yield a successful token envelope. No `.json()`, `.text()`, whole-response `.bytes()` or `error_for_status()` convenience path bypasses the policy.

The8KiB request and64KiB collected-body caps are not total-RAM claims. Hyper header/chunk/connection buffers exist before application checks; the accepted contract documents its pinned HTTP1 buffer default and these limits. No compression decoding, concurrent internal request manager or unlimited retry queue was introduced.

`ExchangeError` stores static categories or the existing secret-free `TokenResponseError`; reqwest errors are classified by timeout without retaining/displaying/logging them. No raw request, response, header, URL, body or provider diagnostic is logged. Raw token metadata is not identity. Dropped Strings/Vecs are not guaranteed zeroized. The result contains native secrets for a later controller only, never React/sidecars/localStorage/library records.

## Test definitions and execution status

**13 inline test functions defined, zero run:** 10 async tests define real localhost HTTP transport using the shared request/execution path and accepted parser; three pure tests cover form/header/final-return guards. Fixed synthetic client/code/token fixtures only. Production entry never selects the test target. `Target::Loopback` exists only under cfg(test), validates literal IPv4 localhost with a nonzero port, and is inaccessible to external production callers. Shorter test limits are private. No certificate-verification bypass exists in either mode; test HTTP does not establish HTTPS/TLS acceptance.

Defined coverage:

- Exact fixed POST/path, five escaped form fields once, no sensitive URL/header fields; native attempt metadata and partial File-only reported approval from a two-scope request.
- Independent fixed form-escaping answer including punctuation/UTF-8, exact8KiB boundary and overflow rejection.
- Fixed-length, chunked and close-delimited success; actual parser rejects malformed/empty/trailing/duplicate-token JSON.
- Exact64KiB collected body, declared/chunked overflow before append, truncated length and invalid chunk framing.
- Actual transport Content-Type/encoding duplicates, compressed/list encoding, wrong charset/type, duplicate Content-Length, oversized headers and excessive field count; pure header checks include numeric overflow and duplicate handling.
- Separate redirect/capture servers prove the intended no-forward path; disconnect after receiving the request defines a no-replay assertion.
- Pre-signaled and sender-dropped cancellation means zero dispatch; signal/drop during a stalled body returns cancellation after one request.
- Header/body stalls and continuous drips exercise read and absolute deadline definitions.
- Final cancellation/deadline/expiry rejection and exact metadata retention; stale-generation comparison is explicitly only a caller-contract fixture, not acceptance of an actual controller.
- Typed provider error redaction and static endpoint/deadline policy checks.

The test server uses ephemeral127.0.0.1 ports, at most4 captured requests of16KiB each,10ms socket timeouts, a3s worker lifetime, stop flag and joined Drop. Script waits/writes check stop/deadline, including panic cleanup. No detached author-owned async operation is spawned; cancellation tests use joined futures. These cleanup/timing properties are source definitions, not measured guarantees until tests run.

**Pass:** standalone `rustfmt --edition 2024 --check --config skip_children=true src-tauri/src/drive_auth/exchange.rs`; `git diff --check`; `git diff HEAD --exit-code`; all604 protected hash comparisons. Commands/exits and source patch are preserved. No-index diff exit1 is expected for a new file, not a test failure. Formatter environment is installed rustfmt1.9.0/rustc1.97.1 on macOS26.5.2 arm64; later review must use project-pinned Rust1.98.

**Not exercised:** new Rust compilation, strict lint, all13 tests, localhost socket behavior, timeout/cancel/cleanup timing, independent adversarial cases, live Google/TLS/registration/scopes, account verification, secret storage, refresh, browser/UI/Android or Drive sync. Source may still have compiler/lint/runtime defects. Base317 passes do not validate exchange.rs.

## Review handoff

Root alone registers `exchange` in an isolated review snapshot. Under separately assigned native cache ownership, from `src-tauri` with pinned1.98 and established locked/offline/no-debug/no-incremental/private-TMPDIR environment: `cargo test --locked --offline --lib drive_auth::exchange`, then appropriate combined `drive_auth`/native checks, `cargo fmt --all -- --check` and `cargo clippy --locked --offline --all-targets -- -D warnings`. Review production fixed-target/TLS/client policy statically; test-only HTTP cannot prove it against Google. Rehash protected inputs and inspect no-public-override/no-serialization boundaries. Stop on failures and assign bounded corrections; do not loosen protected parser/scope/TLS policies to pass.

No protected API/feature mismatch was found during source authoring. Existing reqwest chunk/no_* APIs were inspected in pinned0.13.4. Hyper1.11.0 response code was also read to verify that duplicate header values remain appended for post-parse validation (`src/proto/h1/role.rs:1120–1142`), rather than assuming duplicates disappear. No dependency resolution occurred.

Primary references are those already verified in the frozen accepted contract: [Google native token exchange](https://developers.google.com/identity/protocols/oauth2/native-app), [WHATWG form serialization](https://url.spec.whatwg.org/#application/x-www-form-urlencoded-serializing), [RFC6749 token semantics](https://www.rfc-editor.org/rfc/rfc6749.txt). This assignment read the full contract/review and pinned APIs; it did not perform new provider probes. Root owns independent acceptance, README and any integration/publication.
