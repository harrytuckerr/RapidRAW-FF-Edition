# GD1b1 — desktop PKCE transaction helper: source-only receipt

Revision 1 frozen 2026-10-16 Australia/Melbourne (2026-10-15 UTC). Author: actual `gpt-6-astra`, Team slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Task `01a13fa2-534c-7f11-bcb7-e4850745600d`. **Source ready for independent review; compilation and all Rust tests Not exercised. No authorization or Drive feature enabled.**

Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-pkce`, detached base `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Only two new source files and this receipt were written in the worktree. Existing `lib.rs`, manifests, locks, model and graph remain byte-identical to HEAD. No registration, dependency changes, Cargo/build/cache activity, browser launch, listener, token exchange, credentials storage, provider request, device or Git mutation.

## Frozen files and evidence

| File | Bytes | SHA-256 |
|---|---:|---|
| `src-tauri/src/drive_auth/mod.rs` | 97 | `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8` |
| `src-tauri/src/drive_auth/pkce.rs` | 34111 | `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8` |
| `/private/tmp/rapidraw-gd1b1-pkce-20261015/revision-1/source.patch` | 35528 | `ebdf469adc5cf86827317f9be25288edf644d5977463ad4171e41098e47275c6` |
| `/private/tmp/rapidraw-gd1b1-pkce-20261015/revision-1/manifest.json` | 2461 | `a67c3596d1c59a950272f9d7bb0bd65606418745ed0ee7d302a9134fd53e6ef0` |
| `/private/tmp/rapidraw-gd1b1-pkce-20261015/revision-1/commands.json` | 43620 | `bfab2f56af1ad007887c0a78f1ae77d600999a653498d1e64d6d9ba7407dd658` |

The revision manifest binds the original manifest (SHA-256 `02799d3cc61a7e9729c6bb17861ca7e5b5ed6c45a6573237cb48c2bbda4442b9`), which records exact protected-file, pinned API input and contract hashes. All protected hashes were rechecked unchanged for revision 1. The revision commands file records final format/diff checks, source/correction patch generation and exits; original `commands.json` retains environment output and earlier checks; it is not a transcript of every exploratory read. New-file `git diff --no-index /dev/null <path>` exits 1 because files differ, as expected; it is not a build/test failure.

Frozen GD0 input: original `../drive-contract/docs/drive/contract.md`, SHA-256 `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`, §6 line 122. The assigned `profile-preset-integration` copy now hashes `a7a171f6e5a1154439941899b0efcb99d65bb73842b94f7d750d9a65a2b6bc29`; independent byte comparison found only the lead's new desktop clarification paragraph. Its dated addendum SHA-256 is `6c34f7bafacab48573b2d70f6dfed7d74b868d2ec24f4920d7885356494c646c`. The helper honors that approved clarification; the original frozen contract remains the identifiable input.

## Bounded correction and preserved lineage

Lead independent static review found **P2** in original `pkce.rs` lines 369–383: rejecting every unknown response key contradicted [RFC6749 §4.1.2](https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2). Reproducer: an otherwise valid bound callback with matching state/code plus `provider_extension=synthetic` returned `InvalidCallback` and remained pending, preventing completion when a provider adds an extension. Primary text independently checked 2026-10-16 Melbourne / 2026-10-15 UTC, including §8.2 extension parameter name grammar.

Original source SHA-256 `50d623cfe07972c843351339ba1d5e3f6810e40dee0b5e7788514f8c2f00f9c4` and original receipt `8d26fa9ea8bbfefc427f2c8a48c314e9b94187060f07647f7fa3bd07fa3e8488` are preserved byte-for-byte under `/private/tmp/rapidraw-gd1b1-pkce-20261015/original-candidate/`. Original private manifest/patch/commands/delivery files remain unchanged. Revision delta `/private/tmp/rapidraw-gd1b1-pkce-20261015/revision-1/correction.patch` is 4875 bytes, SHA-256 `111259a0e7b82b3417840472a24116f5cbc078c1286290bc0c27ac24feeccd46`.

Only callback unknown-field handling and focused test definitions changed. Unknown fields remain subject to strict decoding, duplicate checks and existing bounds; malformed or non-parameter names still fail. All production bytes outside `callback_parameters` remain identical, verified directly. State/code/error/issuer/scope interpretation, entropy, binding, timeout, secret types and module export are unchanged. Two new test functions define code and denial extension acceptance, inert fake grant/account fields, decoded known/unknown duplicates and extension limits; malformed unknown values remain rejection cases. No red/green runtime result is claimed: **14 defined, zero executed**; runtime validation remains assigned to the independent reviewer.

## Concrete API and boundaries

`PkceTransaction::begin(client_id, already_bound_port, scopes, now: Instant, lifetime)` uses fallible OS `rand::rngs::SysRng` entropy: 64 bytes split into separate 32-byte state and verifier values, each 43 unpadded base64url characters. The private deterministic/failing entropy seam is test-only access, not a public verifier override. S256 is SHA-256 of the verifier's ASCII bytes followed by unpadded base64url. No weak fallback on entropy failure.

Only exact nonempty, unique scope sets drawn from `drive.appdata` and `drive.file` are accepted. The native browser URL targets `https://accounts.google.com/o/oauth2/v2/auth`, with explicit client, redirect, response type, selected scopes, state, challenge and S256 method. No `include_granted_scopes`, implicit grant union, extra identity scopes or broad Drive fallback. Callback scope hints and requested scopes are not granted-scope or account evidence. This slice deliberately does not define refresh-token/offline authorization policy.

Binding is exact raw `http://127.0.0.1:<nonzero port>/oauth2/callback/google`, plus parsed scheme/host/port/path checks. No IPv6 fallback is implemented. Client ID checks only bound a syntactically plausible Google client ID; they do not establish registration, client ownership or platform eligibility. The later listener must first bind the actual loopback socket, validate GET/local socket/Host/raw request target and never trust forwarded headers. Passing a constructed URL alone cannot prove an HTTP request's origin. This helper does not reserve a port or authenticate a server request.

`authorization_url_for_browser(now)` is native-only; code and verifier are returned together in `AuthorizationCodeGrant` through explicit native borrowed accessors. Secret-bearing transaction/grant/outcome types implement neither Debug, Display, Serialize nor Clone. Bounded typed errors contain no input strings or underlying parser/RNG error dumps. No logging is present. String clear/drop and the entropy buffer fill are not guaranteed secure memory erasure. State equality avoids first-difference early return for equal lengths; no compiler-level constant-time claim is made.

Callback parsing is bounded to 8192 raw bytes, 16 query pairs, decoded key 64 bytes/value 4096 bytes, authorization code 2048 visible ASCII bytes and provider error 128 identifier bytes. Strict one-pass form decoding rejects malformed percent triplets, invalid UTF-8 and controls. Duplicate decoded keys—including percent-encoded aliases—fail. Well-formed unrecognized parameters are ignored for response semantics, as required by RFC6749 §4.1.2. Their decoded names obey §8.2 syntax; they still count toward all input limits and duplicate checks. The bounded temporary parameter map is never returned or exposed; only explicitly supported semantic keys are consumed, so extension fields cannot supply identity or granted scopes. Both code and error, neither, fragments, userinfo, whitespace/backslash URL repairs, host aliases, different ports/paths, dot or percent-path aliases, mismatched state/optional issuer/unrequested reported scope fail closed. Error descriptions/URIs are never surfaced or followed.

| Input/state transition | Result |
|---|---|
| Valid matching nonempty code before deadline | Move code + matching verifier into native grant; terminal `CodeReceived` |
| Valid matching `access_denied` / other bounded provider error | Terminal `Denied` / `ProviderRejected`; no raw provider text |
| Invalid syntax/binding/state/response fields | Typed rejection, remain pending; deadline never extended |
| Explicit cancel before deadline | Terminal `Cancelled` |
| `now >= deadline`, lifetime at most 600 seconds | Terminal `Expired` |
| Caller clock regresses from last observed Instant | Terminal `ClockFailed` |
| Any operation requiring active transaction after terminal state | `Finished`; no replay |

Time is explicit monotonic `Instant`, not wall-clock. Expiry is enforced when checked or operated on, not by an internal background timer; later integration must drive timeout checks and close its listener. At an expired deadline, expiry takes precedence over cancellation. Terminal transitions drop pending secrets and clear the stored authorization URL. A successfully returned grant remains a native secret until its caller drops it; token exchange/storage is outside this helper.

## Checks and definitions

**Pass:** standalone `rustfmt --edition 2024 --check --config skip_children=true src-tauri/src/drive_auth/mod.rs src-tauri/src/drive_auth/pkce.rs`; `git diff --check`; `git diff HEAD --exit-code` for all existing tracked files; exact protected hashes versus HEAD. Formatting used installed `rustfmt 1.9.0-stable`, toolchain `rustc 1.97.1`, on Darwin 25.5 arm64; no Rust compilation occurred. Project's `src-tauri/rust-toolchain.toml` pins `1.98` for the later review build. A private Python standard-library SHA256/base64 calculation independently confirms the published RFC challenge; it neither imports nor executes the Rust helper.

**Defined, not run:** 14 inline test functions, covering published RFC7636 verifier/challenge and authorization wire keys; approved/empty/broad/duplicate scope sets; invalid configuration and entropy failure; code/verifier binding and replay; wrong state versus matching provider denial/errors; exact raw loopback binding and parser normalization traps; percent-encoded duplicate keys and ambiguous responses; malformed percent/UTF-8/control/bare fields and decode-once behavior; ignored well-formed extensions on valid code and matching denial; extension duplicate/resource/name guards and inability to substitute state/code or change grant bindings; expiry/cancel/clock regression; invalid requests not extending lifetime; callback/code bounds and issuer/scope mismatches; typed error non-echo of synthetic secrets. Table rows are not counted as separate test functions. All test values are synthetic/public RFC fixtures.

**Not exercised:** Cargo check/test/clippy, Rust runtime or OS entropy, socket binding/listener/parser integration, system browser launch, real callback traffic, Google consent/login/grants/account verification, token exchange/refresh/revocation, secure store, reconnect/local-state preservation, Android GIS, cross-client or cross-device Drive behavior. No successful authorization claim follows from constructing a URL or receiving a code. Source may still have compiler/lint/runtime issues until independently checked.

After root grants module registration in an isolated review snapshot and owns the native cache, the focused command from `src-tauri` is `cargo test --locked --offline --lib drive_auth::pkce`, using the project's 1.98 toolchain and established no-debug/no-incremental/private-TMPDIR environment. Then perform appropriate format/strict lint checks and independent callback probes. Do not register or expose a Tauri command just to test this module. Root owns integration and acceptance.

## Primary and pinned API evidence

Primary documents read 2026-10-16 Melbourne / 2026-10-15 UTC:

- [Google OAuth for native apps](https://developers.google.com/identity/protocols/oauth2/native-app): official endpoint, installed-client browser/code flow, S256, loopback redirect and explicit selected scopes. Current documentation says installed-app incremental authorization is unsupported; this was independently confirmed by the lead and captured in the dated desktop addendum. Android authorization semantics remain separate.
- [RFC7636](https://www.rfc-editor.org/rfc/rfc7636.txt), §§4.1–4.3 and Appendix B: unreserved verifier, S256 derivation and published verifier `dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk` / challenge `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`. These are fixed independent expected values, not production-generated expectations.
- [RFC8252](https://www.rfc-editor.org/rfc/rfc8252.txt), §§7.3, 8.4, 8.9–8.10: loopback IP redirect/variable listener port, state and exact per-transaction redirect binding. This slice fixes IPv4 only per assignment; broader platform reachability remains a later gate.

Read actual pinned registry APIs under `/Users/harrisontucker/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`: rand 0.10.2 `src/lib.rs` reexports `TryRng`, `src/rngs/mod.rs:118` exposes `SysRng`, default features enable it; rand_core 0.10.1 `src/lib.rs` defines fallible `try_fill_bytes`; reqwest 0.13.4 `src/lib.rs:282` reexports `url::Url`; url 2.5.8 provides URL/query construction; base64 0.23.1 exposes `Engine`/`URL_SAFE_NO_PAD`; sha2 0.11.0 exposes `Digest`/`Sha256`. All are existing direct dependencies or the existing reqwest URL reexport. No dependency resolution or new feature is required by this source. Input files and their hashes are listed in the private manifest.
