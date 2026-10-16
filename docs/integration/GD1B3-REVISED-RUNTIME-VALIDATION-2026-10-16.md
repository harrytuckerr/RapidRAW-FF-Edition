# GD1b3 revised native validation — 2026-10-16

**Pass** for the four authorized gates on revised frozen bytes. The original `a943…` strict-lint **Fail** remains unchanged; these new results apply to `7543548a…` only. Root retains source acceptance/import and the later combined canonical gate.

Model `gpt-6.1-sol` (live roster), macOS-26.5.2-arm64-arm-64bit. Snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-token-response-revision`, detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Revised parser `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886`; unchanged module `b19422c07fed9d158a0e809f8d4248e6a17be33f4c554434b838438a46b6d802` and helper `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`. Original author, original runtime failure, status revision and expanded lead review read fully/hash-verified before testing.

| Gate | Result | Exit | Elapsed |
|---|---|---:|---:|
| `author14` | 14 passed | 0 | 49.618s |
| `independent10` | 10 passed | 0 | 73.988s |
| `fmt` | Pass | 0 | 0.581s |
| `clippy-all-targets` | Pass, includes unchanged independent probe | 0 | 12.173s |

Exact commands, each once from `src-tauri`:

```text
cargo test --locked --offline --lib drive_auth::token_response -- --nocapture
cargo test --locked --offline --test drive_auth_token_independent -- --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

No source/test lint failure or correction occurred in this revision task. The original probe remains SHA256 `749c353e5f3462112d12283aa0219ae37983604733f32f2a74624e6ee44779c9`,17,297 bytes, copied exactly without reformatting. New actual binaries preserved:
- `author14`: 33048032 bytes; SHA256 `764bbf4cc56df860741b9da5bf9f1b7d17386c9cae53cf3654da0e51303b496d`.
- `independent10`: 2027632 bytes; SHA256 `9bd6d364b64174ac4225d8e729bfc280277186c79523e8d4c6f727892dfe85b3`.

The exact original ten public-API probes were copied byte-for-byte, including predicates, synthetic response bodies, expected successes and resource assertions. No formatting or expectation changes were made. They check explicit partial scope subsets and order without requested/previous-grant union; missing, unrequested, broad, duplicate and escaped claims; opaque access/refresh preservation, absence versus rotation/null and token/control boundaries; strict positive integer expiry, request-start anchoring, receipt latency, exact expiry/use boundaries, clock regression and checked standard-library arithmetic; valid ignored numeric/nested extensions versus malformed, trailing and resource-limited JSON; non200/success-error ambiguity and typed secret-free diagnostics.

Positive resource edges remain fixed: 65,536 body bytes, depth16 including root,4,096 structural tokens,32 fields,128 decoded key bytes and8,192 token bytes, with failures immediately above each limit. Structural work expectation is independently counted: the four-field root contributes8; adding an N-element extension array adds N+2, so4,086 elements reach4,096. Standard-library checked Instant arithmetic determines platform representability independently of the parser. No candidate converter/parser generated expected answers.

The original `a943…` candidate retains its strict-Clippy **Fail**. Its author14, independent10, default272/14ignored and fmt results belong to those original bytes. This revision changes only the residual Client pattern to disjoint `402 | 404..=428 | 430..=499`. An exact replacement assertion and complete inline-test-section equality were checked here. The lead's separately recorded comparison across all65,536 u16 categories is static evidence, not execution of this revision. No production source was edited by this validator.

No full272, combined28, CLI or ignored tests were repeated in this task. The later combined canonical source still requires its own full native gate (planned288 plus14ignored). These bounded revision gates do not establish combined listener integration or provider acceptance.

Only the owned temporary test is removed after exact private custody; existing directories are retained. All pre-existing tracked/untracked product, registration, README, receipt, dependency and lock bytes must remain unchanged. Known ignored ORT is hash-reused/read-copied with download disabled, and generated ignored schemas are recorded. Build/check commands use pinned Rust/Cargo1.98.1, offline/locked testing and lint, the declared exclusive desktop target, private TMPDIR, no debug or incremental output, and the established Tauri/ORT environment. Full per-command environment, UTC, argv, exits, duration, space and actual binary custody are retained in JSON.

**Not exercised:** a revised full default suite, combined listener/parser integration, ignored/GPU/vendor/external-fixture tests, actual HTTP/TLS/headers/network/socket/provider/Google/account/grant acceptance, token exchange/use/refresh/revocation/storage, browser/Tauri/UI/Android/device or Drive sync. Caller-supplied response claims are not authenticated provenance or account proof. Token strings have no guaranteed zeroization; future transport/storage must bind provenance, exact scopes, account generation and expiry-before-use independently. Root retains acceptance and import authority.

All 567 frozen tracked/untracked input bytes/inventory matched before and after; only the owned temporary test was removed after custody. Protected manifest `702e4798c0c942411f39ef89ed295cd41afde2ddca1012d0d90f4c94a1b442b1`. No existing directory removal. Original private receipt, manifest and first failing Clippy log were rehashed unchanged; their records are in `original-evidence.json`. Four ignored schema outputs were generated and hashed. Known ignored ORT33,481,272 bytes /`2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` hash-read-copied; build.rs confirmed the valid library and skipped download.

Environment: `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`; `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_INCREMENTAL=false`, `CARGO_PROFILE_TEST_INCREMENTAL=false`, `CARGO_INCREMENTAL=0`, `RUSTUP_TOOLCHAIN=1.98`, `CARGO_NET_OFFLINE=true`, private `TMPDIR`, `TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin`, `ORT_SKIP_DOWNLOAD=1`, `GIT_OPTIONAL_LOCKS=0`. Rust/Cargo1.98.1, rustfmt1.9.0-stable, Clippy0.1.98. Each gate recorded no competing compilers and >=6GiB reserve before running. No downloads/dependency refresh.

Evidence `/private/tmp/rapidraw-gd1b3-revised-validation-20261016`; machine `receipt.json` SHA256 `d186d20c8d0ee8d0ecf66fdb946d68a2c77805ca0f31c784cdaaecd0b3b3f848`. Full logs, exact argv/environment/UTC/exits/times, fresh binaries, probe custody, one-line patch and source manifests sealed by `artifact-hashes.json` (excludes itself).

Desktop cache explicitly **released** at `2026-10-15T14:30:23.056583+00:00`; no active cargo/rustc/Clippy then, 17.30GiB free. No further builds or downstream work.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `a6290f31eabcdc0136ee0b8c68239c8a0d41cc740490976ace5688aecdb5e2f8` (7051 bytes).
