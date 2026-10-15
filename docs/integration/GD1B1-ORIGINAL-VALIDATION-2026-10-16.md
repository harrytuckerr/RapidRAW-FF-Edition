# GD1b1 ORIGINAL candidate validation — 2026-10-16

**Revise.** Exact original helper compiles and its12 defined tests pass, but the independent valid-response extension probe **Fail** reproduces a protocol/interoperability defect. Original candidate is not accepted. No correction or revised-source validation was performed; desktop cache lease is released.

## Finding F1 — P2: valid callbacks with an unknown extension are rejected

`src-tauri/src/drive_auth/pkce.rs:369–381` rejects every parameter name outside an allow-list. Independent test `rfc6749_valid_matching_code_must_ignore_unknown_provider_extension` invokes the actual public `PkceTransaction::begin`/`accept_callback` with real production SysRng, synthetic client, requested drive.appdata scope, exact asserted loopback redirect/port, matching freshly generated state, valid synthetic code and `provider_extension=synthetic`.

Expected: Code outcome under the assigned [RFC6749 §4.1.2](https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2) requirement to ignore unrecognized response parameters. Observed: **InvalidCallback, status Pending**. Suite exit101,6passed/1failed/0ignored/0filtered. Expectation remains success; no should_panic, current-behavior golden or source repair. The raw generated state/authorization URL/verifier is not printed or persisted.

This can prevent an otherwise valid extended authorization response completing the transaction. It is a synthetic real-helper reproducer, not an observed Google/provider failure. Smallest correction: ignore unrecognized fields while retaining bounded parsing, known-field checks, state/redirect binding and single-use protection. A separately frozen revision needs a new review. The author's original malformed-input test at line755 explicitly expects rejection of `code=abc&unknown=x`, so its Pass does not clear this conformance requirement. RFC expectation was supplied in the assignment; no HTTP/browser lookup occurred in this task.

## Executed checks and boundaries

| Check | Result |
|---|---|
| Exact project helper compilation +12 author functions | Pass:12passed,0failed,0ignored,258filtered; exit0,137.159s |
| cargo fmt --all -- --check | Pass:exit0,0.682s; no product formatting writes |
| Strict Clippy --lib --tests -D warnings | Pass:exit0,28.163s |
| Independent public-API controls | Pass:6 functions |
| Valid matching code + unknown provider_extension | Fail:1 function; suite exit101,112.861s |
| Protected inputs and owned temporary test restoration | Pass |
| Actual listener, browser, HTTP/Google account/grants/tokens/store/sync, UI/device | Not exercised |
| Author's in-progress/revised source | Not exercised |

Seven separately authored public-API functions cover: two production SysRng transactions with distinct fresh state/challenge pairs, exact endpoint/query keys/scopes and returned-verifier S256 binding; all exact selected scope variants/no implicit union; false state/host/port/path staying pending then matching-code acceptance/replay rejection; strict deadline despite invalid callbacks; bounded typed error non-echo plus denial/provider-error terminal use; cancellation/clock regression; and the deliberately failing extension case. Assertions do not log generated payloads. S256 reference applies SHA256/base64url directly to the returned verifier, without calling the production private challenge helper. The12 author's functions use their original internal fixtures; their count is separate from the7 independent functions, and258 filtered tests were not run.

Synthetic ports are unbound caller assertions. Receiving a code in this helper is neither authentication nor a grant/token/account proof. URL scopes are requests, not verified grants. OS entropy success/freshness is a finite observed control, not a randomness-quality or zeroization audit. No provider, browser, socket/listener or account action occurred.

## Frozen input, commands and custody

Actual model `gpt-6.1-sol`, macOS26.5.2 arm64. Snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-pkce-review`, detached base `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`, plus the root-provided single module-registration line and exact original untracked helper/author receipt. Registration is not a Tauri command or an enabled authorization feature.

- pkce.rs SHA256 `50d623cfe07972c843351339ba1d5e3f6810e40dee0b5e7788514f8c2f00f9c4`.
- mod.rs SHA256 `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8`.
- All563 protected files unchanged before/after cleanup; manifest `ae751549fa378a76326f4b2d06e483b8780a53eb1bf18d520e1d3e66b4d4cc23`. Existing registration/lib.rs, locks, sources and author receipt remain unchanged.
- Only new report in the repo is this file. Private probe was copied to `src-tauri/tests/gd1b1_original_review_probe.rs`, preserved at `/private/tmp/rapidraw-gd1b1-original-validation-20261016/gd1b1_original_review_probe.rs` (SHA `0e653dc6a3b7c8625f57701b5cf61e3f66e47d2dfa75234a741fc1b368df9d86`), then only that owned test file/new empty directory were removed. No product source/Git edit.
- Known ORT33481272bytes, SHA `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`, read-copied to ignored resources, verified and download skipped. Four ignored Tauri schema artifacts were generated; exact before/after hashes are in source-after.json/receipt.json. No tracked input changed.

From `src-tauri`, exact commands:

```sh
cargo test --locked --offline --lib drive_auth::pkce -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --offline --lib --tests -- -D warnings
cargo test --locked --offline --test gd1b1_original_review_probe -- --nocapture --test-threads=1
```

Rust/cargo1.98.1; rustfmt1.9.0-stable. Exclusive declared CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target; CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0, both CARGO_PROFILE_*_INCREMENTAL=false, CARGO_INCREMENTAL=0, RUSTUP_TOOLCHAIN=1.98, CARGO_NET_OFFLINE=true, TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin, ORT_SKIP_DOWNLOAD=1, private TMPDIR. Exact argv/env/UTC/exits/disk/toolchain are in private result receipts. No unrelated compiler was running at entry or release. Every native command's preflight had>=6GiB free; final free17.74GiB. No npm/node, dependency resolution/update, clean, full/ignored/GPU/Android suite, device, browser/HTTP or account operation.

Preserved actual binaries:

| Binary | Bytes | SHA256 |
|---|---:|---|
| Private binaries/author12 | 33036304 | `fa9b2bad82f48812e25aa6de68025eaf215ded8de018eea5d069c98f9ecbefaf` |
| Private binaries/independent7 | 2326608 | `352bfeb84f7d5f2ec9524acd5e81d0081226c1da82edc24f01ce728da849f947` |

Evidence `/private/tmp/rapidraw-gd1b1-original-validation-20261016`: receipt.json SHA `577232a6c7461d2e18fd881aa760600bdc5b370a1d2c53705362ef8d73a17e4b`; exact result/log files, probe, original registration diff, full protected/ignored manifests and preserved binaries. Cache lease explicitly released at2026-10-15T13:34:27.516744+00:00. Worker completion is not integration, authorization, provider or product acceptance.
