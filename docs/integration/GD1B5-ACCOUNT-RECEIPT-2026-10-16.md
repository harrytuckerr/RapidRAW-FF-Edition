# GD1b5-S2 — native account candidate, source-only receipt

Frozen 2026-10-16 Melbourne by actual **gpt-6-astra**, task `01a1405f-237a-7503-97e8-7e2141e4328f`. Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-account`, clean detached input **bd7f2d00a498315f48898c88ed99fbb225fa1113**. Approved S1 source commit4cbd8b5968851f7eb4b9c7219efd223d894faeb0 and its reported368Pass/14ignored/fmt/strict baseline are prerequisites, **not validation of this new module**.

**Ready for independent source review and a separately granted compile/runtime assignment. ZERO tests executed.** Root alone registers account.rs. No Google/app/account feature was enabled.

## Custody and owned output

Only two new repository paths:

- `src-tauri/src/drive_auth/account.rs`, **68171 bytes**, SHA-256 **7834f144d3ca806dbdf669b15f62e714fa5b80d9d360ce331550534d441921f7**.
- This new `docs/integration/GD1B5-ACCOUNT-RECEIPT-2026-10-16.md`; exact final size/hash in private delivery.json (no self-hash).

Private evidence: `/private/tmp/rapidraw-gd1b5-account-implementation-20261016`. Exact new-file `account-final-source.patch`,70265 bytes, SHA-256 **938c52981564b7851e1110485b210224175bb4d4d8afdf4b548bc43e5445950c**. Earlier authoring/check artifacts remain separately named; the **final** source/patch/checks are the handoff authority.

Dispatch ledger `/private/tmp/rapidraw-gd1b5-account-dispatch-20261016/ownership-and-inputs.json` matched **6c85f73d8dd820f383b856a189155a43da1284fdcf32ac5ed9749b773ebdca5e**. All **624 tracked input lengths and hashes** matched before and after authoring. `protected-before.json` / `protected-after.json` are byte-identical,96115 bytes each, SHA-256 **77cb8410beab75671eb3a89f42f8e562c0148b8430e938160b15befad04f0d7d**. Final delivery rechecks the same624 files and the exact two new paths. No existing source/helper/session/test/doc/README/mod/lib/manifest/lock edit occurred.

Read FULL approved account contract8afb6aa7 and lead review441222fd. The latter explicitly authorizes bounded native API items to be `pub` in the isolated library: this implementation chooses that visibility. All secret/PII-bearing fields remain private; there is no Debug/Display/Clone/serde/default implementation on context/candidate/display/quota, no verified-candidate constructor, token getter, parts extraction or replacement API. No unused-item suppression or dummy registration was added. Public static error enums are Copy/Debug/Display and contain no supplied strings.

## Implemented boundary

Native `VerificationContext::from_native_attempt` (line94) validates the bounded registration/client/project syntax, unique approved requested scopes, nonnil UUID/nonzero generation and admission deadline. Project number is canonical positive u64, preserving the existing model boundary; S1's broader32-digit string is rejected if it cannot be represented, not truncated or reinterpreted. This constructor **does not authenticate client/project mapping**. The future S3 producer must supply these fields from the same original slot and trusted configured registration, then recheck them under the current commit mutex at adoption.

`verify_account` (line330) consumes the original ExchangeResult. It calls one private execution path (line344) with the fixed production target `https://www.googleapis.com/drive/v3/about` and exactly one constant fields query, `user(permissionId,displayName,emailAddress),storageQuota`. Existing Url::query_pairs_mut is used; no undeclared reqwest query feature. The GET has no body and one sensitive Authorization Bearer header. Token syntax at this HTTP boundary follows RFC6750's b64token repertoire,1–8192 bytes; unsupported opaque parser tokens fail without normalization or token inspection.

A fresh verified-rustls client uses minimum TLS1.2, https-only, HTTP1, no idle pool, redirect:none, retry:never, no proxy/referer/cookies/decompression/connection-verbose logging and finite connect/read/total limits. No URL/client/trust/environment override exists in production. The fixed Google URL cannot become a supplied target, photoLink fetch or redirect destination. Direct-only connectivity intentionally fails in proxy-required environments; no fallback.

Time/cancellation gate (line447): one absolute deadline min(entry+30s, original admission deadline, token expiry); connect/read5s. Sender signal **or sender drop** cancels; the request future is owned locally and dropped rather than detached/replayed. Before dispatch, after bounded await/yield boundaries, after parsing/error completion and immediately before candidate construction, enforce cancellation, result/context ID+generation, actual approved subset and token lifetime. A result returned before a later switch/cancel is still subject to S3's atomic current-generation check; this helper cannot inspect live controller state.

Response acceptance: <=64 exposed headers, checked aggregate name/value16KiB (line535); status200 only; required singleton JSON Content-Type with optional UTF-8 charset, identity/absent Content-Encoding, checked optional length<=64KiB. Body accumulation uses one preallocated64KiB Vec with checked append, chunk yielding and advertised-length comparison. Non200 responses are dropped without parsing/displaying bodies.401,403,redirect,429,server and other status failures have distinct static categories;403 does not diagnose scope alone. No raw reqwest/serde/body/header/URL/provider error is retained in public diagnostics.

Private parser (line666) returns **data only**. It enforces whole-body UTF-8 even for IgnoredAny extensions, JSON64KiB/depth16/4096 structural-work bounds (line724), <=32 decoded keys per root/user/quota map and128-byte keys, decoded duplicate rejection including escaped aliases, strict known types and trailing-content rejection. Three small serde Seeds/Visitors and a private static-error Cell preserve useful field categories without returning parser strings or building a public Value tree. Unknown extensions are skipped under the same budgets; duplicate keys inside entirely ignored extension objects are not claimed to be rejected.

Required user and permissionId are distinct missing/invalid errors; ProviderId's unchanged1–256 ASCII alnum/underscore/hyphen policy is reused, not claimed to cover every possible Google identifier. Optional name/email absent/null/bounded blank becomesNone; supplied strings first pass byte/scalar/control limits (name<=256scalars/1024bytes, email<=320bytes). Optional quota absent/null remainsUnknown; present object may omit its limit, with optional canonical nonnegative int64 strings retained exactly. Malformed optional metadata fails explicitly. Email/display/quota never choose account identity; unknown id_token/photoLink do not become identity or trigger I/O.

Only successful request completion constructs `VerifiedAccountCandidate` (line213; final gate line421), owning the same input, correlation, context and authenticated permission ID. Partial grants are preserved; the missing-requested list disables nothing automatically and grants nothing new. Candidate metadata getters expose no credentials. `matches_native_attempt` compares all registration/correlation/requested-set/original-deadline fields and usable token time, but must be used by S3 against current state under its lock. Copying its AccountKey creates only syntax/data, not a transferable proof object.

No session pending-take/watchdog/adoption/Connected changes, registration catalog, browser, IPC, store/refresh/revocation, sync or local-edit mutation. Failed verification drops this new owned input and never replaces a prior session. The documented S3 private-field/continuation seam remains a future grant.

## Defined tests, zero executions

**17 inline test functions:9 synchronous +8 async, none executed.** Names are recorded in final-checks.json; table rows/race alternatives are not counted as additional functions. Definitions cover:

- Native context u64/syntax/scope/identity guards; sensitive Bearer boundary; parser known-answer optional display/quota/int64 cases; missing/null/wrong types; decoded duplicate aliases; blank/control/size limits; depth/work/fields/UTF-8/trailing content.
- Real localhost GET capture for exact method/path/one fields pair, empty body, native Authorization and partial subset. A second synthetic thumbnail endpoint records zero requests; unknown id_token is ignored.
- Actual request A/B fixtures preserve their exact owned synthetic credentials and expected distinct permission IDs; mismatched ID/generation/scope fails before dispatch. Returned candidates reject changed project, registration, client, generation and deadline metadata. No test claims to detect a dishonest provider or establish configured project provenance.
- Real status/non-JSON redaction, redirect capture301/302/307/308 with zero destination requests, connection-close/no-replay, chunked success and overflow, truncation, missing/duplicate content type, unsupported encoding, and actual header count/byte overflow. Pure boundary checks supplement those transport definitions.
- Signal/drop cancellation before dispatch and during a delayed real request; absolute deadline, read stall/drip and expiring input; already expired/future-clock/unsupported-header inputs with zero dispatch; private final-gate rejection after parsed data, never parser-made proof.

Private cfg(test) OwnedInput::Fixture contains only synthetic tokens from the existing **pure** token parser and a synthetic attempt, then enters the same actual transport/reader/parser/final gate. It is absent from production. No sibling ExchangeResult constructor/helper edit, public endpoint override or fake verified-candidate constructor exists. Actual production entry takes only an owned ExchangeResult and fixed Google target.

Synthetic server fixtures bind ephemeral127.0.0.1, capture at most4 requests of<=16KiB, check stop flags and a3s thread ceiling, enforce finite socket I/O, and join on Drop. Test request futures stay in scope; a panic drops them instead of leaving a spawned task. Private short limits/Target::Loopback are cfg(test)-only. These definitions are **not runtime, TLS, Google identity or product acceptance**. No socket/HTTP/browser/provider activity was executed during authoring.

## Checks actually executed and next gate

Host: `macOS-26.5.2-arm64-arm-64bit`. Standalone pinned `RUSTUP_TOOLCHAIN=1.98 rustfmt --edition 2024 --config skip_children=true --check src-tauri/src/drive_auth/account.rs` **exit0**, empty diagnostics; formatter `rustfmt1.9.0-stable (48a229ceae 2026-09-01)`. `git diff --check` exit0 for protected tracked files. New-file `git diff --no-index --check /dev/null .../account.rs` **exit1 with empty diagnostics**, reflecting a new-file difference, not a whitespace error. The source patch command likewise returns1 for differences. Exact argv/cwd/environment/exits/logs in **final-checks.json**, SHA-256 `c11f09c91f49d482ca37b60c163067f63aff374acb2392f7a1b726be0e955a19`. Only standalone formatting, read-only inspection/hash/status/diff operations and owned file authoring occurred; no Cargo/rustc/test binary/cache/dependency/node operation.

One local editing script failed to locate a rustfmt-adjusted test needle **before writing**; it was corrected and the final owned file reformatted. This was an authoring-script failure, not a compiler/test result, and is noted in earlier checks.json. No compile/lint success is inferred from rustfmt.

Pass: exactbase/clean admission,624 before/after protected hashes, standalone pinned formatting and empty whitespace diagnostics. **Not exercised:** compile, strict lint, all17 tests, localhost/network/TLS/provider, Google registration/account, browser/Tauri/UI/device, S3 adoption or sync. Native build-cache ownership was never acquired or used. Direct transport/JSON limits are application acceptance bounds, not total memory caps or hard abort guarantees. Strings and header buffers are dropped normally, not guaranteed zeroized.

Next consumer: root independently reviews this exact source, registers only the module in an isolated review snapshot, and grants a separate validator pinned1.98/offline/private-TMPDIR/no-debug/no-incremental cache ownership. Future commands from src-tauri:

```sh
cargo test --locked --offline --lib drive_auth::account
cargo test --locked --offline --lib drive_auth
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

Those commands are **not run here**. Independent adversarial transport/parser/ownership checks and canonical integration remain required. S3 owns current-session mapping/adoption and must not reuse S1's pending watchdog unchanged. No downstream implementation begins with this delivery.

## Protected dependencies

- `docs/integration/GD1B5-ACCOUNT-CONTRACT-2026-10-16.md`: `8afb6aa7f32eb7be5ee8aa1a721c70feb06829ef0858695772a14a60da557949`.
- `docs/integration/GD1B5-ACCOUNT-LEAD-REVIEW-2026-10-16.md`: `441222fdfcfa46fec4bc8167e5453697bdb9a1ce450a041f4ef8c0da281ba6d5`.
- `src-tauri/src/drive_auth/session.rs`: `17ed9fec0d126895db683ae89fa9e056673e0fbae0dcc2119129369a1b2d6638`.
- `src-tauri/src/drive_auth/exchange.rs`: `f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408`.
- `src-tauri/src/drive_auth/token_response.rs`: `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886`.
- `src-tauri/src/drive_auth/pkce.rs`: `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`.
- `src-tauri/src/drive/model.rs`: `f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0`.
- `src-tauri/src/drive_auth/mod.rs`: `5c59c0add8d68c464828e7719821ab3dd1b40296f29968abf0175178e7df4301`.
- `src-tauri/src/lib.rs`: `8c71f74aec49a6635a5ab2f959472adeea78e0b48eaabaf9b22c2f0f679e4963`.
- `src-tauri/Cargo.toml`: `454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2`.
- `src-tauri/Cargo.lock`: `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def`.

Full624-file custody is in the before/after manifests. Final receipt/source/private evidence hashes are in delivery.json. Source and receipt stop frozen for independent review.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `8b9cb169ab20b6fb633d152f851258f8e7b7d6b39047dc0e86704e8547213afc` (14194 bytes).
