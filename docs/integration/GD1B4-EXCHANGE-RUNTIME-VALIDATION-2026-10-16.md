# GD1b4 independent exchange runtime validation — 2026-10-16

**Pass** for the granted independent native/localhost gates. No failures, repairs, suppressions or expectation changes. Root retains source acceptance/import and later combined/product gates.

Model `gpt-6.1-sol` (live Team roster), macOS-26.5.2-arm64-arm-64bit. Snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-exchange-review`, detached `ec1d032efe7dd321e7a99d1931730b654b922ec3`. Full contract2682a642, author receipt56633c4b and independent review77d5a97a read/hash-verified before writing. Immutable author prefix `a53690ccfc7a5c0bcb4d5bef6aa2d8c71c02764a7a7cc2249b6bdbaa36eb849b`,41,333B; full wired exchange `f8ac7bf6843ccf101610ceef93a0b0b9454203d1da09807ee3ba6e3f1877d408`; desktop module `bc7e526d8ec1082ce55e51c28a3f60ef4e1cf0519d7577daa1178df055e6ef3d`. Exact prefix and root test-only footer unchanged.

| Gate | Result | Exit | Elapsed |
|---|---|---:|---:|
| `exchange-focused` | 22 passed:13 author+9 independent;302 filtered | 0 | 54.787s |
| `fmt` | Pass, check only | 0 | 0.603s |
| `clippy-all-targets` | Pass, warnings denied | 0 | 12.813s |

One focused command executed all13 original and9 independent definitions once (1.79s actual test phase,0failed/0ignored). Table cases are not separate test functions. No standalone author13, full317, combined subset, CLI or ignored test run. Exact commands:

```text
cargo test --locked --offline --lib drive_auth::exchange -- --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

New owned child `src-tauri/src/drive_auth/exchange_independent_tests.rs` is **kept** for permanent regressions:24,729B, SHA256 `ca224e53c259aa1efd7c3ee71f15c9919a79a35792311db7683ba3682f91d434`. It was preformatted alone before the first test, then frozen; no existing source was formatted. Source-only addition patch SHA256 `6adc2321a6978d03cfa6761434aec247b1b644d256f059d8496b2969c3488b58`. All607 existing tracked/untracked input bytes remained identical before/after, including helpers/parser/original tests/README/locks. Inventory adds only this child and this report. Protected manifest `e6c1db621719e6de5c5806caf7714a67c8e7e1b3d918223c56273c44697755ec`. No existing or owned test was removed.

Independent tests use their own nonblocking TCP fixture, no author Server or fake parsed-token success. Literal127.0.0.1 ephemeral listeners, at most4 connections/16KiB captured per request/256KiB scheduled output,4s logical worker deadline, stop-aware waits/writes and joined Drop bound fixture ownership even during unwinding by construction; forced-panic cleanup was not executed. The shared private exchange path and actual accepted parser handle every response.

Observed Pass: exact five native form values with independently fixed escaped code/redirect answer, secret-free request headers/path, no extra confidential/scope fields, preserved UUID/generation and explicit File-only capability; positive64-field and16,384-name/value-byte metadata boundaries, rejection at65/16,385 over actual transport; fixed/chunked/close-delimited body success at65,536 and rejection at65,537; fixed/chunk truncation/invalid chunks/duplicate JSON cannot accept a complete-looking prefix. Duplicate metadata, compression/list encodings, wrong content type/charset fail; quoted UTF8/identity succeeds.

Separate redirect capture observed zero forwarded requests; disconnect after full POST receipt observed one request without local replay. Preclosed/pre-signaled cancellation observed zero connections; send/drop during incomplete body returned cancellation without tokens. Continued25ms body progress still reached the private300ms absolute deadline (303ms observed). Typed actual-parser provider/non200/unrequested/broad-scope errors did not reflect synthetic token/body/id/account/URI markers. These are current macOS/local observations, not universal timing or all-protocol guarantees.

Test-only plaintext Loopback/short Limits do not weaken the unchanged production fixed HTTPS/client/certificate policy. Static verified TLS settings are **not live TLS acceptance**. Header caps apply after HTTP parsing and the body cap bounds accumulated application input, not total RAM. Dropping local work cannot prove remote issuance rollback or an unused code. Native correlation metadata is not account identity or the future controller atomic adoption check.

Fresh executed binary preserved:39,211,744B, SHA256 `330106f2373825e45f7cc8486d8fcf3c02a6a21eae70714184c283fd37ea9b30`. Logs contain test names/timing and static errors, not a credential environment dump. Rust/Cargo1.98.1, rustfmt1.9.0-stable, Clippy0.1.98. Per-command invocation/result JSON records exact environment, argv, UTC/exits/durations/space and no competing compilers with>=6GiB reserve. Declared exclusive target; DEV/TEST DEBUG0+INCREMENTALfalse; CARGO_INCREMENTAL0; RUSTUP_TOOLCHAIN1.98; CARGO_NET_OFFLINEtrue; private TMPDIR; TAURI_ENV_TARGET_TRIPLE aarch64-apple-darwin; ORT_SKIP_DOWNLOAD1; GIT_OPTIONAL_LOCKS0. Test/lint locked/offline; known ignored ORT33,481,272B/2b885992… hash-read-copied, build.rs download skipped. Ignored schema outputs hashed; no source/lock/dependency/Git mutation.

**Not exercised:** full/canonical combined suite for this new source; Google/provider/live TLS/registration/client-secret omission/proxy-only reachability; external network; remote issuance/rollback/unused-code state; all retry modes/hard abort; account/grant/controller/secure store/refresh/revocation/Drive sync; browser/Tauri/UI/Android/device/ignored/GPU/vendor tests; guaranteed zeroization/totalRAM or complete security audit.

Evidence `/private/tmp/rapidraw-gd1b4-exchange-validation-20261016`; machine `receipt.json` SHA256 `676b7d56c7e685e0c3ace2e8a5d846781cc29d760a4b0a83499e2aa9cbab4141`. Full/sanitized logs, source/patch, binary, definitions versus executed names, commands/environment and pre/post custody sealed by `artifact-hashes.json` (excludes itself). Cache explicitly **released** at `2026-10-15T15:20:29.460730+00:00`; no active cargo/rustc/Clippy then, 16.87GiB free. No further build/downstream work.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `0ec5c62c4301e0278c0d5bbe3f75d81563b681dec3086ea84f11c3bb46d9869e` (6205 bytes).
