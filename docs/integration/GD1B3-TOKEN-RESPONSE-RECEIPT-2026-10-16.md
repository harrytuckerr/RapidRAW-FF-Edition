# GD1b3 — native token-response parser, source-only receipt

Frozen 2026-10-16 Melbourne / 2026-10-15 UTC. Actual author `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`; task `01a13fca-3d1b-7610-bad1-8f9b2942eb43`. **Source and 14 test definitions ready for independent review. Zero Rust tests/compiles executed. No verified Google grant, account ownership, storage, Connect Google or sync acceptance.**

Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-token-response`; detached HEAD `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Own writes: new `token_response.rs`, one `pub mod token_response;` export in the supplied module, and this receipt. No helper API/dependency change was needed. Root retains registration/review/integration.

## Frozen custody

| Path | Bytes | SHA-256 |
|---|---:|---|
| `src-tauri/src/drive_auth/token_response.rs` | 33807 | `a943573b0ab5a00be6df8d7e5cd98f6d27c66fffb92766d7d2eeb9b233656f61` |
| `src-tauri/src/drive_auth/mod.rs` | 121 | `b19422c07fed9d158a0e809f8d4248e6a17be33f4c554434b838438a46b6d802` |
| `/private/tmp/rapidraw-gd1b3-token-response-20261016/source.patch` | 35355 | `7dd67b873de15fcf11c266d777a88a17b8aa37154bd18481f4d856f6d1812fe9` |
| `/private/tmp/rapidraw-gd1b3-token-response-20261016/manifest.json` | 3776 | `8f572a2808ce676df6f69d806def5c66be74cd5e4af0bc210ecc391b07fdfff7` |
| `/private/tmp/rapidraw-gd1b3-token-response-20261016/commands.json` | 38954 | `99cec31fc207a3522303e52fba37c9a2bf88ce8a0e1e7825f58384a08e0aa104` |
| `/private/tmp/rapidraw-gd1b3-token-response-20261016/inputs.json` | 1503 | `caabc01f16e2f413a7e1e3dc4b83c0fb623e5be444f2ed406d2188aca50f9cb0` |

Protected helper remains exactly `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`. Original module `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8` is preserved as private `original-mod.rs`. Manifest records unchanged before/after hashes for PKCE, `lib.rs`, Cargo manifest/lock, Drive model/graph and package manifest/lock. All tracked files remain identical to HEAD. The separately frozen listener was neither read as a dependency nor changed. No source registration, build/native cache, Cargo/npm, dependency resolution, browser/app/device/account/HTTP token request/socket or Git mutation occurred. Primary documentation browsing is separate from provider operations.

## Concrete API and policies

`parse_google_token_response(http_status: u16, body: &[u8], requested: &[ApprovedScope], request_started_at: Instant, now: Instant) -> Result<ParsedTokenResponse, TokenResponseError>` is a pure parser. Inputs are caller assertions: it cannot verify TLS provenance, the endpoint, the association with a particular code/refresh transaction, HTTP headers or account identity. The later HTTP owner must supply the exact requested scopes and bind the response to its authenticated endpoint/request. No persistent Drive DTO or public serde Value is used.

`ParsedTokenResponse` exposes only native borrowed accessors for access/optional refresh tokens and the explicit reported scope subset, plus monotonic expiry and `is_usable_at`. The secret-bearing result and private wire staging have no Debug, Display, Clone or Serialize. Raw staging implements private Deserialize only. Errors are bounded enums with static Display/Debug, including HTTP/provider categories; no raw body, parser error dump, received key/value, description or URI escapes. Tokens are never decoded as JWT/base64 or treated as identity. String drop is not guaranteed zeroization.

| Area | Accepted policy / rejection behavior |
|---|---|
| HTTP | Only 200 can yield a parsed token result. Other valid statuses map to typed redirect/client/server/specific 400/401/403/429/unexpected categories; values outside 100–599 reject. No redirect following or HTTP request exists. |
| Access token | Required JSON string, 1–8192 decoded bytes, visible non-whitespace ASCII 0x21–0x7e. Punctuation remains opaque and exact, with no trim/prefix/JWT inference. Empty, controls, whitespace, non-ASCII and oversize reject. These are adapter safety/resource limits, not a claimed universal OAuth token format or provider maximum. |
| Token type | Required JSON string equal to Bearer case-insensitively, per RFC6749. Other types, padding/whitespace or wrong types reject. |
| Refresh token | Optional string under the same token limits. Absence returns None meaning **not supplied**, not deletion, expiry, revocation or failed renewal. Present null/empty/malformed is not absence. A supplied rotated string is returned unchanged. The parser never overwrites existing credentials. |
| Expiry | Required positive JSON u64 integer seconds. Strings, floating/exponent notation, negatives, zero, overflow and null reject; no expiry default. `request_started_at.checked_add(Duration::from_secs(seconds))` must succeed and be strictly later than `now`. |
| Clock | Require `now >= request_started_at`. Anchoring at request start is conservative: transfer/parse time cannot extend the provider's reported remaining lifetime. Equality at expiry rejects. No arbitrary upper lifetime is guessed beyond representable positive u64/checked Instant; future renewal/use policy is separate. |
| Continued use | `is_usable_at` requires time at/after the validation Instant and strictly before expiry. Parsing cannot guarantee usability after the caller retains a result; native token accessors do not automatically perform network/expiry checks. |
| Requested scopes | Nonempty unique typed set of at most the two existing ApprovedScope variants; no helper modification or broad scope option. |
| Reported scopes | Required explicit scope string, 1–256 bytes, exactly space-delimited approved appdata/file tokens, unique and a subset of this request. Empty/duplicate/broad/unknown/unrequested/malformed claims reject. Preserve provider order and partial subset; never fill denied scopes from requested or old grants. |

Missing `scope` is an intentionally conservative Google-adapter failure. RFC6749 allows omission when unchanged; this parser does not claim the RFC universally requires it. The Google native documentation describes the response field as the access token's granted scope list. This parser validates that **claim**, not its provenance/usability/account. Absence of an approved capability can later disable it; partial approval does not become a full requested-set success.

No existing refresh credential, account/session generation or prior grant is input, so none can be silently overwritten or unioned. Unknown `id_token`, account-looking fields, DPoP metadata and `refresh_token_expires_in` are not interpreted. This slice does not claim refresh-token lifetime/DPoP support, credential persistence or that tokens are indefinitely valid; later transport/storage/renewal work must define those separately.

## JSON and failure boundaries

Before serde, a single linear scan enforces a 64 KiB whole-body cap, 16 container levels including root, and 4096 structural opening delimiters/commas/colons outside strings. It handles escaped quotes when counting but does **not** replace JSON syntax validation. Serde validates the root object, UTF-8, known-field types, numbers and complete consumption; trailing non-whitespace fails.

The private root visitor accepts at most 32 fields and 128 decoded bytes per key. Its bounded BTreeSet rejects every duplicate decoded root key, including escaped aliases of critical keys. Known string fields reject null/wrong types. Unknown fields use `IgnoredAny`: they are skipped semantically under the same whole-body/depth/work bounds, without building or retaining an arbitrary Value tree. Nested extension keys have no credential semantics and are not promoted to root; nested duplicate extension keys are not separately interpreted. Unknown well-formed numeric values are skipped rather than converted into expiry/identity.

Success fields combined with any error/error_description/error_uri field fail as ambiguous. A pure error body on status200 fails. Non-200 responses never yield a token even if their body resembles success. Recognized provider errors map to a finite enum; unknown codes become Other. Provider error codes are bounded to 128 ASCII NQSCHAR bytes; optional description/URI strings are bounded to 2048 NQSCHAR bytes, discarded and never followed. No raw provider diagnostic is displayed. Malformed non-200 serde bodies return a typed HTTP category; preflight resource/syntax failures retain their bounded failure category. Error classification never implies a credential should be deleted or that an account was verified.

No custom secret-owning Value tree, generic schema framework, network adapter, storage layer or public serialization was added. Whole-body bounds also limit temporary string allocations before individual-field validation. They are conservative adapter policy, not measured performance/provider compatibility evidence.

## Definitions versus execution

**Pass:** standalone `rustfmt --edition 2024 --check --config skip_children=true src-tauri/src/drive_auth/mod.rs src-tauri/src/drive_auth/token_response.rs`; `git diff --check`; tracked `git diff HEAD --exit-code`; final protected hashes against the captured baseline. Installed formatter is rustfmt 1.9.0/rustc 1.97.1, macOS 26.5.2 arm64. Project review builds must use pinned Rust1.98. Patch `git diff --no-index` exits 1 because files differ, not because tests failed. Private commands record final verification/patch/environment commands and exits, not every exploratory read.

**Defined, not run:** 14 inline functions, with fixed synthetic token fixtures and table cases:

1. Valid code-response secret preservation, exact explicit scope and conservative expiry.
2. Renewal response without refresh token versus supplied rotation; null is not omission.
3. Partial approval for either scope, full reported ordering, and unrequested-scope rejection.
4. Missing/empty/broad/unknown/duplicate/malformed scopes and invalid requested sets.
5. Case-insensitive Bearer versus other token types/whitespace.
6. Missing critical fields and null/boolean/array/object wrong types.
7. Repeated/escaped root critical keys, normal single escaped key, and duplicated error key.
8. Zero/negative/floating/exponent/string/nonfinite/integer-overflow expiry, exact expiry boundary, clock regression and platform checked-Instant overflow.
9. Opaque token punctuation, byte/control limits, exact 8192-byte boundary and overflow.
10. Unknown nested extension/account/id-token fields cannot change secrets/scopes or supply missing root fields.
11. Depth boundary, body/work/root-key/root-field limits, and quoted delimiters not counting as nesting.
12. Non-object/non-JSON/malformed UTF-8/trailing content and valid trailing whitespace.
13. Non200 statuses and success/error ambiguity.
14. Typed provider/HTTP categories and error redaction.

Table entries are not extra test functions. Synthetic test string construction may use serde_json::to_string for JSON quoting; production parsed secrets have no Serialize implementation. **Zero tests executed; no red/green runtime evidence claimed.**

**Not exercised:** compilation, strict lint, runtime parsing, independent adversarial validation, provider responses/grants, HTTP/TLS/headers, account ownership, token use/renewal/revocation, secret storage, browser/native UI/Android, Drive persistence or cross-device sync. Source may still have compiler/lint/runtime issues. A byte parser cannot attest provider origin or consent.

Next independent owner registers only in an isolated review snapshot and runs from `src-tauri` using pinned1.98, offline/no-debug/no-incremental/private-TMPDIR environment: `cargo test --locked --offline --lib drive_auth::token_response`; then suitable combined native tests, strict lint and independent malformed/numeric/scope probes. Root owns shared cache, registration, README and integration. Do not turn this parser into a Tauri command or expose tokens to React to test it.

## Primary evidence and dependencies

Read 2026-10-16 Melbourne / 2026-10-15 UTC:

- [Google native OAuth, token exchange/response and granted-scope checking](https://developers.google.com/identity/protocols/oauth2/native-app): describes status200 JSON, access-token seconds/type/scope and refresh behavior. This task parses the bounded response only; current identity/DPoP/time-based refresh extensions are not implemented or claimed.
- [RFC6749 §§5.1–5.2 and 6](https://www.rfc-editor.org/rfc/rfc6749.txt): case-insensitive token type, optional refresh token, expiry semantics, scope omission rule, unknown response field handling and bounded-category error interpretation informed the adapter. Its metadata requirements do not make supplied bytes authenticated or require local credential deletion.

Existing direct dependencies only: serde1.0 (lock1.0.229) and serde_json1.0 (lock1.0.151), with `ApprovedScope` from unchanged PKCE. Standard-library Instant, Duration and BTreeSet. No raw_value/arbitrary_precision feature, new crate, Cargo/lock edit or dependency resolution. No operational Google or security acceptance is implied by this source receipt.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `abd7c8ccd5182492b8cacb02b2f2e17826440fa70df31c5bcdf488fad5327e1c` (13219 bytes).
