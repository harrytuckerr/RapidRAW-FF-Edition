# GD1b1 revised validation — 2026-10-16

**Pass** for the frozen revised native helper checks. The unchanged independent unknown-extension probe now succeeds; this is not Google/provider acceptance. Root retains review and integration authority.

- Model: `gpt-6.1-sol` (live Team roster); host: `macOS-26.5.2-arm64-arm-64bit`.
- Snapshot: `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-pkce-revision`, detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`.
- Revised `pkce.rs`: `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`; module: `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8`.
- Author receipt: `2610413dce7d9ee805c90888c611a38dd8bdfba664fc19f8746213811643f7c2`. Root-supplied registration was preserved; no product source, README, dependency, lock or Git mutation.

| Check | Result | Exit | Elapsed |
|---|---|---:|---:|
| `author14` | 14 passed, 258 filtered | 0 | 132.414s |
| `independent7` | 7 passed, 0 failed | 0 | 106.381s |
| `default-lib` | 258 passed, 14 ignored, 0 failed | 0 | 1.543s |
| `fmt` | Pass | 0 | 0.544s |
| `clippy-all-targets` | Pass, warnings denied | 0 | 18.841s |

Exact commands, each once from `src-tauri`:

```text
cargo test --locked --offline --lib drive_auth::pkce -- --nocapture
cargo test --locked --offline --test gd1b1_original_review_probe -- --nocapture --test-threads=1
cargo test --locked --offline --lib -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

Rust `1.98.1` / Cargo `1.98.1`, rustfmt `1.9.0-stable`, Clippy `0.1.98`. Environment: `RUSTUP_TOOLCHAIN=1.98`, `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_INCREMENTAL=false`, `CARGO_PROFILE_TEST_INCREMENTAL=false`, `CARGO_INCREMENTAL=0`, `CARGO_NET_OFFLINE=true`, private `TMPDIR`, `TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin`, `ORT_SKIP_DOWNLOAD=1`, `GIT_OPTIONAL_LOCKS=0`. Per-command JSON records UTC times, full argv, cleared inherited environment names, exits, space and duration. Initial pre-build free space was 17.68 GiB; every gate checked the 6 GiB reserve and absence of competing cargo/rustc/Clippy processes.

The original probe file `0e653dc6a3b7c8625f57701b5cf61e3f66e47d2dfa75234a741fc1b368df9d86` was used byte-for-byte, including its historical ORIGINAL comment, names and expected-success assertion. It exercises public production SysRng begin, fresh state/challenge pairs, exact scope sets/S256 grant binding, false state/binding, replay, timeout, typed-error redaction, denial/cancellation and clock regression. The formerly failing `rfc6749_valid_matching_code_must_ignore_unknown_provider_extension` now returns Code for valid state/code plus `provider_extension=synthetic`. No socket was bound; the port is a synthetic caller assertion. No expected result was weakened.

Historical comparison is separate: frozen original helper `50d623cf…` had 12/12 author controls Pass and 6/7 independent probes Pass, with that unknown-extension probe Fail (exit 101, InvalidCallback while Pending). Revised helper `85bd9b48…` has 14/14 author controls and the identical 7/7 independent probes Pass. The 14 focused controls are a subset of the full default library run, not additional unique passes. Original logs, binaries and receipt remain immutable.

All 565 baseline files matched before temporary-probe cleanup. The provided probe was preserved privately and removed with its newly empty directory before product fmt/all-target Clippy; no probe formatting was rewritten. The remaining 564 protected files match exactly after all checks (manifest `8fe173d29b95f82e5e569c36d45c6ee73ca35a2a9e034519053e09dc97745900`). Author and root-review receipts, registration, README and Cargo.lock are unchanged. `source-before.json`, `source-after.json`, `probe-cleanup.json` and `original-to-revised.diff` preserve custody.

Known ignored ORT runtime (33,481,272 bytes; `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`) was hash-read-copied from the lead snapshot without modifying it. build.rs confirmed its hash and skipped download; all Cargo invocations were offline/locked. Four ignored schema outputs were generated: acl-manifests, capabilities, desktop-schema and macOS-schema; their hashes are recorded. No dependency resolution/download or source repair occurred.

Preserved actual binaries:

- Focused/default lib: 33,027,664 bytes, SHA256 `93602526a9eab2734da128297a57df667ed22028b8a225c451aba2240aa9d307`.
- Independent seven: 2,326,976 bytes, SHA256 `d80e35d65e4b589731fe7f906d1e7e7916a8b50459aecdd39a7aacc5ce619bce`.

**Not exercised:** real Google authorization/account/grant acceptance, browser/HTTP/loopback listener/OS callback integration, token exchange/storage, Drive APIs/sync, UI/Android/device, ignored/GPU/external-fixture tests. Synthetic returned-code handling establishes helper behavior only.

Evidence: `/private/tmp/rapidraw-gd1b1-revised-validation-20261016`; machine receipt `receipt.json` SHA256 `0ee10bd6c623a1121d28bc6d34a328891741a4f3a22c353e622fa550c4a48285`. Full commands/environment/logs, sanitized log copies, binary/source/probe hashes and cleanup receipts are retained there. `artifact-hashes.json` seals final file records; it excludes itself to avoid self-reference.

Desktop cache explicitly **released** at `2026-10-15T13:46:48.631207+00:00`; no compilers remained, 17.60 GiB free. No further build or follow-on work.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `888b0b2ee4c6284384f4c7839090536b0519c9e0d2a842dfa92d27fa58d0eff0` (5536 bytes).
