# GD1b5-S3A — Candidate lifetime metadata and sibling test bridge

Source-only delivery, 2026-10-16 Australia/Melbourne. Author: Codex CLI - Astra, actual model `gpt-6-astra`. Task `01a14084-7e14-73a2-8327-946b0a2a96ac`.

## Scope and frozen inputs

Worktree: `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-account-lifetime`; detached base `5a2208c0184657e1d2a364ad46a6cddcb5f43290`. Only existing `src-tauri/src/drive_auth/account.rs` and this new receipt changed. Private evidence: `/private/tmp/rapidraw-gd1b5-account-lifetime-implementation-20261016`.

Authority: approved adoption contract `e1ea97d2cc024d82f6b046657ece99965ee1a83026865bd5fe48522e047c15fc` and lead review `ec61fe37642c3c039f3372dd0bdc7413dd2835ec845f423db4b8de7a08887782`. This delivery implements prerequisite A only. B registration catalog and C session adoption remain separately gated.

The accepted S2 baseline has historical canonical evidence of 396 passed, zero failed, 14 ignored, fmt and strict all-target Clippy (read `GD1B5-S2-CANONICAL-VALIDATION-2026-10-16.json` and account runtime/review receipts). Those results do **not** validate this changed source. No historical tests were rerun.

## Production change

Two borrowed metadata methods on the existing sealed `VerifiedAccountCandidate`:

```rust
pub fn credential_expires_at(&self) -> Instant
pub fn credentials_usable_at(&self, now: Instant) -> bool
```

They delegate directly to the retained token result's `expires_at()` and `is_usable_at(now)`. The original ownership, token bytes and expiry remain unchanged. Usability means only the parser's local validation-time/expiry window. It does not establish remote validity or revocation state, admission validity, current controller/configuration/generation, project mapping, account adoption or Drive readiness. A candidate can therefore have locally usable credentials after its admission deadline while `matches_native_attempt` is false. That original method remains byte-for-byte unchanged and must still participate in future atomic adoption checks.

No production HTTP, parser, final gate, limit, error, type-construction or serialization behavior changed. No credential getter, token replacement, candidate constructor, Debug/Clone/serde, endpoint override or parts API was added.

## Test-only sibling interface

`#[cfg(test)] pub(in crate::drive_auth) mod session_test_support` exports only equally restricted items:

- Closed `SyntheticLabel::{A, B}` and `FixtureSpec` with private fields.
- `FixtureSpec::new(label, attempt, actual_scopes, issued_at, expires_in)` returns existing binding/parser failures wrapped by a cfg-only typed `FixtureError`. It rejects nil/zero correlation, empty/duplicate/oversized scopes and invalid/overflowing expiry. Unsupported scopes and arbitrary labels are unrepresentable in the approved closed enums.
- Fixed synthetic access/refresh strings are selected by label; there is no arbitrary token, raw about body or proof argument. The existing pure token parser is called **once at construction**, with the explicit fixture issue time. The parsed tokens are retained so dispatch cannot renew their expiry.
- `verify_fixture(spec, context, cancel, SocketAddr)` moves those same parsed tokens and correlation into the existing `OwnedInput::Fixture`, then calls the actual private `verify_at` with `Target::Loopback`. Limits are total 2 seconds and connect/read 500 milliseconds. Existing target validation, request construction, bounded response parser and final binding gate remain in charge. A real synthetic HTTP response is required; the bridge never fabricates a successful candidate.

The module disappears from production builds. The native account verifier's production errors are unchanged; the fixture wrapper introduces no production error variant. No sibling private fields, constructor or sealed-result bypass is used.

## New regression definitions — not executed

Four test functions were added: three `tokio::test`, one synchronous `test`. The existing 17 author functions and permanent 11 independent child functions are unchanged.

| Definition | Intended evidence when separately executed |
| --- | --- |
| `lifetime_metadata_separates_admission_validation_and_expiry_without_renewal` | Actual bridge response, fixed immutable expiry; usable before admission ends, false before token validation, true after admission but before token expiry while attempt match is false, false exactly at/after expiry. Explicit Instant comparisons avoid long sleeps. |
| `sibling_bridge_binds_distinct_fixed_credentials_to_actual_http_candidates` | A/B table uses actual requests and fixed expected permission IDs; captured bearer credentials match the label, refresh values stay off the request, actual partial File grant stays separate from requested AppData+File, expiry is preserved. |
| `fixture_constructor_rejects_invalid_binding_scopes_and_lifetime` | Nil/zero attempt, empty/duplicate/oversized scopes, zero/overflow expiry rejected; valid combined approved scopes accepted as fixture syntax only. |
| `bridge_requires_a_valid_local_transport_response_and_matching_binding` | Missing-user HTTP response cannot produce proof; invalid loopback target rejected; mismatched attempt rejected. |

The new owned peer uses an ephemeral IPv4 loopback port, a single request capped at 16 KiB, fixed response choices, a 3-second worker ceiling, stop-aware accept/read polling and finite write timeouts. Its thread is joined on Drop, including panic paths. This is a bounded test definition, not observed cleanup/runtime evidence. Existing authored and child peer fixtures were not edited.

**Tests executed in this assignment: ZERO.** No compiler, Cargo, socket, browser, provider, account or device operation ran. No shared native cache was acquired or used.

## Preservation and exact hashes

Current source: 87,892 bytes, SHA-256 `6b0ab79ea3245fbe4a7c5ddfb7aa39001930f748b98f2aeb6dde5e24dd521d87`.

Baseline source: 68,249 bytes, SHA-256 `5fe87ede9dc5eee57f5f3ffce719fe6650d91618faa74e7b5125aea211eaa75b`. It contains the accepted 68,171-byte author prefix `7834f144d3ca806dbdf669b15f62e714fa5b80d9d360ce331550534d441921f7` plus the retained 78-byte independent-child registration footer. Footer SHA-256: `74c3b24b0647177ce5781aa81aee434ef75263ff1557580ff8f503fa7ff26d3b`.

Removing only the two-method insertion (747 bytes) and new cfg-test insertion (18,896 bytes, including its boundary newline) restores **the entire original source byte-for-byte**. Thus all old production regions, `matches_native_attempt`, 17 authored tests and the child footer remain exact. Proof: `unchanged-regions.json`, SHA-256 `838b455091e710393322a658710a606fadd6d53c35d8baf60244dcf0c05b885a`.

The current base contains 634 tracked inputs; exact before/after inventory is preserved and the other 633 hashes are unchanged. Historical S2 validation used 630 inputs before later docs-only commits. Full records are `baseline-inputs.json`, `final-inputs.json`, `baseline-inventory.json` and `final-inventory.json` in private evidence.

| Protected input | SHA-256 unchanged |
| --- | --- |
| account_independent_tests.rs | `b1fca07977b32648f4de8b2192abaeadb7c7a899b6b62e668e1e5af1f6e899f8` |
| drive_auth/mod.rs | `a2e83b21989a031ad035b8e80ff1dbaa44713aecefb29835950d92610ca37810` |
| session.rs | `17ed9fec0d126895db683ae89fa9e056673e0fbae0dcc2119129369a1b2d6638` |
| exchange.rs | `f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408` |
| token_response.rs | `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886` |
| lib.rs | `8c71f74aec49a6635a5ab2f959472adeea78e0b48eaabaf9b22c2f0f679e4963` |
| Cargo.toml | `454aa558d0b4e0ac2a645b0d67a3b82f33689af9a2036465e05343ec00a272d2` |
| Cargo.lock | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

Source patch: `account-lifetime.patch`, 20,783 bytes, SHA-256 `6ff7ae95a4c99835221fa9a8997924e06f34ea556a97ca77452c2cb924748f6f`. `checks.json` records commands, environment and definition counts; SHA-256 `f5c456d957edddbe205516dd05f6d1e8e5fe9b941d73cbae3a1f02ef6380b4ba`. Final delivery manifest records this receipt's hash without a self-hash cycle.

## Executed checks and remaining gates

Pass: pinned standalone formatting and `git diff --check`, both exit 0. Formatter was `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)` selected by `RUSTUP_TOOLCHAIN=1.98`; the owned file was formatted/checked with `--edition 2024 --config skip_children=true`, preserving the external child. Read-only HEAD/inventory/hash checks passed. No Git index, branch or commit mutation occurred.

A preliminary reverse-splice byte check failed because its extraction omitted the new block's leading newline. A slow difflib diagnostic was interrupted (exit 130); a linear boundary check corrected the extraction and restored the baseline exactly. This was evidence-script bookkeeping, not a compiler/test failure or an existing-source change. The failed diagnostic was not used as acceptance evidence.

Not exercised: compilation, new or existing tests, strict Clippy, actual local transport/cleanup, external Google/TLS/account validity, session adoption, UI/browser/IPC/device behavior. No network documentation refresh was needed for these delegation-only metadata additions; existing approved contracts and local APIs were the inputs.

Next consumer: root's independent source reviewer, followed by a separately granted runtime validator in an isolated checkout. Under that later grant, from `src-tauri`, with root-owned pinned toolchain/cache/offline environment:

```sh
cargo test --locked --offline --lib drive_auth::account -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

No new registration is needed in this author checkout: account is already registered and its retained independent footer remains intact. Root alone controls review integration and broader canonical validation. If a compile/runtime finding requires a change, preserve this source and evidence and grant a bounded revision; do not infer acceptance from this source-only handoff.

No B catalog, C adoption, browser launch, credential persistence/refresh, IPC, controller session mutation, application activation or Drive readiness is implemented. Source is frozen at this receipt for independent review.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `1a7ffb47e6cf2014122e14a5c2e38ed74db5a4298d76cfad9055be0e1ac0c91e` (10373 bytes).
