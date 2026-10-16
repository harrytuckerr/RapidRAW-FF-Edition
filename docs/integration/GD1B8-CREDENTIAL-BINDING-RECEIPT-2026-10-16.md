# GD1b8-A credential-binding metadata — source-only receipt

Author: GPT-6 Astra (`gpt-6-astra`), 2026-10-16. Task `01a14335-e1c1-77c1-b969-670b24f11046`. Detached base `eadcc985455d33deaa8f5677b641d511342c2f17` in `drive-desktop-credential-binding`. This is author delivery for independent review, not compilation/runtime acceptance.

## Scope and API

Only `src-tauri/src/drive_auth/account.rs` changed, plus this new receipt. Three insertion regions add `VerifiedAccountCandidate::credential_binding()`, `CredentialBinding<'a>` and four inline test groups/two private test helpers. The view holds exactly one private candidate reference. Its ten immutable getters expose the retained account key, registration/client IDs, requested/actual scope slices, original input attempt ID/generation, historical admission deadline, credential expiry and optional-refresh presence. No allocation, new owner, constructor, mutable API, secret bytes/hash, extraction, replacement, Debug/Display/Clone/Copy/serde or production consumer was added.

The original input supplies attempt identity; context supplies configured mapping/requested scopes/admission. These remain historical native metadata, not current controller generation/epoch, provider-attested client/project provenance, storage authority or renewed usability. Refresh presence is only `is_some()` on the existing private borrow; it implies neither validity nor persistence/renewability. Normal String destruction is not guaranteed zeroization. Existing matching, parsing, transport, expiry and session retention are unchanged.

## Source custody and preservation

- Baseline account: 87892 bytes, SHA-256 `6b0ab79ea3245fbe4a7c5ddfb7aa39001930f748b98f2aeb6dde5e24dd521d87`.
- Delivered account: 100068 bytes, SHA-256 `ac7cdfd79a1f24b6a24a586abc8dee8555a950713d0568f39453df9f49ceaf45`.
- Unified account patch: 13208 bytes, SHA-256 `e7c9022d1fb564ad228d9b16f84e877dd15666a07e82341ef8a33a205e3ee716`.
- Entire baseline reconstructs exactly by removing the three inserted blocks. Unified patch forward/reverse replay passes. Thus all original production, 21 test names/bodies/assertions, helpers, sibling bridge and final independent-child footer remain byte-exact. Independent children are untouched.
- Binding inputs manifest: 169994 bytes, SHA-256 `0171bec143ff1d412b213ae2779e2d005bd131892d3b81561b43e7d48d1ffbd8`; root source grant SHA-256 `4505e50c236476eb0120a3c67e0b0646d24ef77cb2504ef206e42ddfe3357e37`.
- Private before/after custody records verify all 757 supplied inputs: account is the sole changed input; the other 756 are exact. This receipt is the sole new product path, yielding 758 paths. Detached HEAD, raw index/checksum/backup, worktree pointer and shared branch reference remain exact. The absent node_modules alias remains absent. No dependency traversal or Git command occurred.

Private evidence: `/private/tmp/rapidraw-gd1b8-credential-binding-implementation-20261016/`. `source-preservation.json`, `before-custody.json`, `after-custody.json`, `format-evidence.json`, `account.patch`, `delivery.json` and `artifact-manifest.json` bind the full records; manifest/self hashes are reported externally to avoid cycles. No unrelated historical audit was repeated.

## Definitions and checks

**Four new test functions; 25 total inline definitions; ZERO tests executed.** New groups cover exact full tuple and independent requested/actual order; absent/present refresh; distinct account/project and same account with changed registration/client/attempt/generation plus partial grants; repeated borrowed views and explicit before-validation/admission/exact-expiry/future-clock comparisons. The four groups contain eight defined synthetic HTTP verifications; they are not eight executed tests. Six explicit clock rows require no clock sleeps.

New helpers feed fixed synthetic token canaries through the real pure parser, then the real `verify_at` localhost path. They reuse the unchanged Server: ephemeral IPv4 loopback, at most four bounded requests, finite socket timeouts, three-second worker ceiling, stop flag and joined Drop. A sealed candidate is obtained only after actual verifier transport when these tests are later run. No proof constructor, sibling-field bypass, real Google flow or OS launcher is used. Structural lifetime/no-constructor/no-trait review is distinct from compile-fail proof, which was not executed.

**Pass:** standalone owned-file formatting/check, whitespace check, exact three-region reconstruction, unified patch replay and full custody. Pinned executable: `<HOME>/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt`; reported identity `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Write/check arguments: `--edition 2024 --config skip_children=true` with only account.rs; check adds `--check`. No child/module formatting. Exact argv, tool hash and check exits are private evidence.

**Not exercised:** compiler, Cargo, Clippy, tests, fixture/socket/process execution, browser/provider/network/keychain/device, app IPC or storage. No cache lease, dependency operation, configuration scan or source outside ownership. No scope deviation or protected API mismatch observed.

Next separately authorized validator: focused `cargo test --locked --offline --lib drive_auth::account -- --test-threads=1` from src-tauri, then independently assigned formatting/strict gates and additional public-API probes. These commands were not run here. Root alone decides registration/integration. Empty production configuration, seven-key authorization, mobile and existing application behavior stay unchanged. Secure-store backend, secret custody/retention, renewal, offline consent and actual configuration remain later decisions.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `2e370391f21f1a816d89c006aa5c01ea315f960433b06d6e8f1997bf4caa20a3` (5740 bytes).
