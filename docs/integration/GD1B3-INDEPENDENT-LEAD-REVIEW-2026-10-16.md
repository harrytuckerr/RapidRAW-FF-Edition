# GD1b3 independent lead source review

Reviewer: Codex CLI, actual gpt-6.1-sol. Reviewed 2026-10-16 Melbourne. **Pass as a bounded source review; runtime acceptance pending.** No blocking source finding identified.

Reviewed full production parser and all 14 fixture functions, full author receipt, and primary Google native OAuth documentation plus RFC6749 sections 5.1, 5.2 and 6. Author snapshot is detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Parser SHA-256 `a943573b0ab5a00be6df8d7e5cd98f6d27c66fffb92766d7d2eeb9b233656f61`, export `b19422c07fed9d158a0e809f8d4248e6a17be33f4c554434b838438a46b6d802`, protected PKCE `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`, receipt `abd7c8ccd5182492b8cacb02b2f2e17826440fa70df31c5bcdf488fad5327e1c`. Root verified manifest `8f572a2808ce676df6f69d806def5c66be74cd5e4af0bc210ecc391b07fdfff7` and all 13 referenced file size/hash records, including protected files, inputs, source patch and commands.

## Source conclusions

Only HTTP200 can return tokens. The parser preserves an explicit approved subset rather than manufacturing denied capabilities from requested or historical grants. Missing scope is a documented conservative Google adapter policy, rather than an asserted universal OAuth requirement. Bearer type is case insensitive. Refresh omission stays distinct from rotation, null, deletion or revocation. Expiry uses positive integer seconds and checked monotonic request-start arithmetic, with receipt latency and exact expiry handled conservatively.

The private deserializer rejects duplicate decoded root keys, including escaped aliases; ignores bounded unknown values without promoting nested token/account-looking fields; and never constructs a generic secret-bearing Value tree. Preflight bounds whole body, nesting and structural work before serde validates syntax, UTF-8, field types and trailing content. The success result has no Debug, Display, Clone or Serialize. Errors contain typed static categories, not raw bodies, token bytes, descriptions or URLs. These are static observations, not executed redaction or resource tests.

[Google native OAuth documentation](https://developers.google.com/identity/protocols/oauth2/native-app) describes token response scopes, expiry in seconds and refresh behavior. [RFC6749 section5.1](https://www.rfc-editor.org/rfc/rfc6749#section-5.1) specifies case-insensitive token type, optional refresh tokens, conditional scope omission and ignoring unrecognized response fields. The implementation documents its deliberately narrower missing-scope policy and finite adapter limits. No live provider compatibility is implied by reviewing these sources.

## Integration contracts and limits

The future transport must independently enforce the exact endpoint/TLS, no unsafe redirects, request/code/PKCE association, HTTP status and content policy, bounded response reading and account/session generation. Passing caller-supplied bytes, status, times and requested scopes does not prove provider provenance, granted access or account ownership. Unknown id_token and account fields supply no identity; DPoP and time-limited refresh metadata are ignored, not supported.

Opaque visible-ASCII token validation rejects controls and whitespace but does not promise every accepted punctuation byte fits every later header builder. The future HTTP owner must use a typed header API without reflecting secrets in errors. Token size and JSON bounds are local policy; they are not claimed provider maximums. No guaranteed zeroization is implemented. Callers must check expiry before use, preserve a prior refresh credential when None is returned, and prevent stale results from changing another active account.

Non200 malformed JSON can return a typed HTTP failure; budget errors can precede HTTP classification. Neither category implies automatic deletion/revocation of existing credentials. Clock/expiry and body policies are explicit; no threshold was widened to fit fixtures.

## Runtime handoff

Root prepared isolated `drive-desktop-token-response-review` at the same frozen base, copied the exact parser/export/helper/receipt and added only `pub mod drive_auth;` in lib.rs. No listener, network client, UI command, dependency or storage change is imported. Runtime cache remains assigned to the independent listener validator; token validation is pending and unassigned until its explicit release.

Next owner must run 14 supplied functions, independent public-API synthetic response probes and appropriate combined format/lint/native gates under the pinned offline environment. Probe explicit partial/missing/broad/unrequested scopes, escaped duplicates, refresh omission/rotation/null, strict integer expiry and receipt boundary, clock regression, malformed/trailing/oversized/deep/work-limited bodies, ignored extensions, non200/success-error ambiguity and typed secret-free failures. Preserve failing expected outcomes and original source before any separately scoped correction. No provider/browser/account/socket/storage/device action is authorized by this parser task.

Pass: full static review and frozen artifact identity. Fail: none identified statically. Not exercised: compilation, 14 defined tests, independent runtime probes, provider responses, token exchange/use/renewal/revocation, account or grants, storage, UI, Android/device and sync. Root alone accepts and integrates after runtime and combined gates.

## Original runtime failure and revision source acceptance

Original runtime14/independent10/default272 (14 ignored)/fmt passed, but required strict Clippy failed on overlapping status arms. Root read the complete report `5173d6e044ff9fffb9408eb1dc495c17fc77c95424f4fdab5402916689c589a2`, failure log, receipt `4bcb50926b9d90d4e98d876510e8ed557ed24104ecdc435e3067b47df4e3ee29`, verified all44 artifact records under manifest `5e5b141ecd633c4772a2013836e13850ff3ae29b8aba3f67675d100b331a4438` and all565 original inputs. Original source remains rejected for integration.

Accept the one-line revision for independent validation: `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886`. Root read full revision receipt `fb3017a78f64105e1a09ec1089a6bdddd887c53e0c74f07e8200612f9532713b`, verified all6 revision manifest records and564 protected files, proved exact one-line replacement and complete test-section byte equality. A symbolic first-match comparison over all65,536 u16 statuses found identical original/revised categories. This static arithmetic check is not execution of revised Rust. The first lead script mistakenly expected ten match arms; inspection confirmed nine and corrected only the script before copying source. Evidence: `/private/tmp/rapidraw-gd1b3-lead-review-20261016/status-revision-proof.json`.

No suppression, resource/expiry/scope/API or test expectation change. A new isolated revision snapshot contains exact corrected source and unchanged exports/helper plus root-only registration. Reuse the exact original ten independent probes `749c353e5f3462112d12283aa0219ae37983604733f32f2a74624e6ee44779c9`. Revised compile/runtime/strict-lint acceptance remains pending; canonical combined checks remain a later gate.

## Revised runtime acceptance

Accept corrected parser7543548a for canonical integration. Root read full revised report `a6290f31eabcdc0136ee0b8c68239c8a0d41cc740490976ace5688aecdb5e2f8`, actual14/10/fmt/strict-Clippy results and logs; verified receipt `d186d20c8d0ee8d0ecf66fdb946d68a2c77805ca0f31c784cdaaecd0b3b3f848`, all41 artifact records under manifest `a1ab18c43d2bc3baf9c3e4f31974f361c16a7f82c5ed17555a2558bfa4cdb886` and all567 frozen inputs. All four authorized gates passed on corrected bytes. Probe749c353e remained byte-identical. Originala943 Clippy failure remains recorded, not replaced by these results. No full272 rerun occurred; the combined canonical full suite remains mandatory before commit. Cache release14:30:23Z and idle were observed. These claims establish native parser behavior only, not provider/account/storage/UI/Android acceptance.
