# GD1b3 independent runtime validation — 2026-10-16

**Fail: required strict all-target Clippy gate on frozen product source.** Runtime author14, independent10, default272 and format checks **Pass**. No product repair or expectation change was made; root retains correction, acceptance and integration authority.

Model `gpt-6.1-sol` (live Team roster), macOS-26.5.2-arm64-arm-64bit. Snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-token-response-review`, detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Parser `a943573b0ab5a00be6df8d7e5cd98f6d27c66fffb92766d7d2eeb9b233656f61`; module `b19422c07fed9d158a0e809f8d4248e6a17be33f4c554434b838438a46b6d802`; protected PKCE `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`. Both complete frozen receipts read before checks.

P2 F1, `src-tauri/src/drive_auth/token_response.rs:259` /`:263`: `400 => HttpFailure::BadRequest` overlaps the later `400..=499 => HttpFailure::Client`. `clippy::match_overlapping_arm` is denied by `-D warnings` and rejects lib/lib-test compilation (exit101). The first failure log is preserved. Ordinary match priority returned expected special/fallback status categories in runtime probes; no runtime misclassification was observed. Smallest proposed separately owned correction: disjoint residual fallback `402 | 404..=428 | 430..=499`, preserving special400/401/403/429 categories. **Not applied.** No source/threshold/oracle edit, suppression or retry.

| Gate | Result | Exit | Elapsed |
|---|---|---:|---:|
| `author14` | 14 passed | 0 | 48.843s |
| `independent10` | 10 passed | 0 | 81.097s |
| `default-lib` | 272 passed, 14 ignored | 0 | 1.507s |
| `fmt` | Pass | 0 | 0.558s |
| `clippy-all-targets` | Fail, product overlapping match arms | 101 | 10.530s |

Author14 overlaps default272; it is not an additional unique pass count. The isolated base contains no listener. No combined28, old CLI, ignored or redundant default test run.

Exact commands, once each from `src-tauri`:

```text
cargo test --locked --offline --lib drive_auth::token_response -- --nocapture
cargo test --locked --offline --test drive_auth_token_independent -- --nocapture --test-threads=1
cargo test --locked --offline --lib -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

Ten public-API synthetic probes used fixed expected results, with no production parser generating its own expectations. Pass: exact partial capability subsets/provider ordering without requested-set union; missing/broad/unrequested/duplicate/malformed scope rejection; decoded escaped keys and root duplicate rejection; opaque access/refresh strings, omission versus rotation/null/control and8192-byte edges; integer-only positive expiry anchored at request start, receipt latency, exact expiry/use boundaries, regression and checked-std Instant overflow; ignored well-formed nested/numeric/id/account extensions; malformed UTF8/types/trailing JSON rejection; non200 categories and success/error ambiguity; bounded typed Display/Debug without token/body/key/account/id/URI reflection.

Independent exact resource edges passed:65536/65537 whole-body bytes, depth16/17 including root,4096/4097 structural tokens,32/33 root fields,128/129 decoded key bytes and8192/8193 token bytes. Structural work expectation was derived separately: four-field root contributes8; one unknown N-element array adds N+2, so4086 elements reach4096. Quoted punctuation does not consume this budget. Resource policy constants are checked in const blocks. Positive expectations—including valid unknown numeric extension `1e999` and partial scopes—were retained. Std checked arithmetic establishes platform representability independently of the candidate parser.

Probe SHA256 `749c353e5f3462112d12283aa0219ae37983604733f32f2a74624e6ee44779c9`,17297B, preformatted before compilation; no subsequent source or expectation edits. Actual binaries preserved:
- `author14`: 33047904B, SHA256 `e9c6dddc1b0264b92e3890203e0a09b2b9e41ce87ec291898093ec7c6b18d5c3`.
- `independent10`: 2027648B, SHA256 `71312fcfdf1b9b7c591f3063c676ba802b2d233f9fb5dda4bb81ba5948c18858`.

All565 pre-existing tracked/untracked input bytes and inventory match before/after. Preserved probe then removed **only that temporary file**, leaving existing directories untouched. Product registration/parser/helper, docs/README, dependencies, locks and Git unchanged; protected manifest `bf8f57ae72af2cfe2525dfbf88ac823687614128387cd06064ad60fde8f01d65`. Four ignored schema outputs generated/hashed. Ignored ORT33481272B/`2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` hash-read-copied; build.rs confirmed valid existing runtime and skipped download.

Rust/Cargo1.98.1, rustfmt1.9.0-stable, Clippy0.1.98. Each gate checked no competing compilers and>=6GiB reserve. Invocation/result JSON records full argv, UTC/exits/durations/free space and environment: exclusive declared target; DEV/TEST DEBUG0 and INCREMENTALfalse; CARGO_INCREMENTAL0, RUSTUP_TOOLCHAIN1.98, CARGO_NET_OFFLINEtrue, private TMPDIR, TAURI_ENV_TARGET_TRIPLE aarch64-apple-darwin, ORT_SKIP_DOWNLOAD1, GIT_OPTIONAL_LOCKS0. Test/lint Cargo invocations locked/offline; no dependency refresh/download.

**Not exercised:** successful final strict lint/probe-lint completion, combined listener integration, actual HTTP/TLS/headers/network/sockets/provider/Google/account/grant acceptance, token use/exchange/refresh/revocation/storage, UI/browser/Android/device/Drive sync, ignored/GPU/vendor/external-fixture tests, guaranteed zeroization or complete security audit. Supplied response claims are not authenticated grant/account proof. Refresh absence is a parsed absence claim, not credential deletion; future transport/storage must bind provenance, account generation and expiry-before-use independently.

Evidence `/private/tmp/rapidraw-gd1b3-token-validation-20261016`; private `receipt.json` SHA256 `4bcb50926b9d90d4e98d876510e8ed557ed24104ecdc435e3067b47df4e3ee29`; first failure, full probe, actual binaries, commands/environment and source/cleanup manifests sealed in `artifact-hashes.json` (excludes itself).

Cache explicitly **released** at `2026-10-15T14:20:45.835859+00:00`, no compiler active then, 17.37GiB free. No further build or downstream work.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `5173d6e044ff9fffb9408eb1dc495c17fc77c95424f4fdab5402916689c589a2` (6342 bytes).
