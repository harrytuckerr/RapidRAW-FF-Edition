# GD1b5-S3C4 protected child migration — source only

Pass: the requested test-source migration, standalone formatting, exact patch replay, assertion preservation and custody checks. Eight existing test definitions remain; **ZERO tests executed**. Compilation, runtime, Clippy and C4 integration acceptance are **Not exercised**.

Reviewer/migration author: Codex CLI - Sol, actual live roster model `gpt-6.1-sol`; C4 production author: Astra. Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-switch-decision-review`, detached HEAD `47a1a46b2fec95826ffe5e71e39b4b5889211919`. This independently adapts the protected C3 test to C4's explicit consent contract; it does not validate production behavior by execution. No production/API blocker was found during the bounded source inspection.

Only `independent_different_and_expired_identity_require_explicit_disconnect` changed in `src-tauri/src/drive_auth/connected_lifecycle_independent_tests.rs` (original lines 507–571, migrated 507–711). Every helper, seven other test bodies, test name, original fixture duration and timing limit is unchanged. There are no new test definitions.

## Exact migration and preserved expectations

| Original lines | Migrated lines | Source change / expected outcome |
| --- | --- | --- |
| 507–534 | 507–534 | Exact original loop: different permission ID, same permission ID in another project, and expired incumbent stamp. Existing real-watcher setup and restricted HTTP candidate helper remain unchanged. |
| Before old 535 | 535–550 | Capture the candidate's public immutable credential expiry and existing authorization/verification terminals. Authorization is the exact attempt/generation `CompletedPendingVerification`; the prior verification is `AdoptedSessionOnly`. |
| Old 538 | 554–621 | Replace the obsolete terminal `SwitchRequired` expectation with nonterminal `AwaitingSwitch` authorization/verification phases. Assert exact attempt, generation, nonnil decision ID, registration, expected candidate project/permission ID, unchanged incumbent identity/epoch and exact File grants. Both previous terminal histories stay unchanged. Repeated public proposal observation must retain every metadata field and deadline. |
| 542–546 | 625–629 | Original stamp key/ID and whole incumbent metadata assertions remain text-exact. Expired mode additionally remains without connected credentials and publicly Expired. |
| Before old 547 | 630–686 | Public exact-tuple Decline produces `Declined`, removes the proposal and preserves authorization and incumbent stamp/epoch/whole metadata (or Expired/no credentials). The consumed exact tuple returns `AlreadyResolved`; the same terminal record remains. No candidate adoption is permitted by Decline. |
| 547–571 | 687–711 | Original synthetic reap, explicit Disconnect/SwitchAccount, identity/connected absence, epoch+1, fresh B adoption and epoch+2 remain byte-exact. No live resource is manually reaped; this unchanged helper reservation has no listener/supervisor. |

The original child contains 141 assertion macros and 11 panic macros. Exactly one assertion is replaced (old `SwitchRequired`); **all other 140 assertion and all 11 panic macros retain exact original text and order**. Fifty assertions are added. The complete assertion-by-assertion ledger, original/replacement reason and each added macro are in private `assertion-ledger.json` (SHA256 `24e00c1c2f3b988bfeba1ca1449a853fd39a12de0c3963be645ac95c628fe0b6`); `changed-lines.json` records every changed line. Equal counts are not used as proof.

The deadline expected value is `min(work.deadline, credential_expiry)`, using the sealed candidate's public immutable expiry. The unchanged candidate helper already asserts that expiry equals its original issue time plus the unchanged 60-second fixture lifetime. Publication occurs later, so publication+60 cannot be the earlier cap; the new source assertion explicitly checks that ordering. A second snapshot must preserve the exact deadline. This is a **token-limited fixture**, not independent runtime coverage of all three cap boundaries. No raw credential/token inspection or fresh publication-time expiry replacement is used.

## Executed source checks

Standalone pinned formatter: `<HOME>/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt`, reporting `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Overrides: `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`; cwd is the worktree above. Actual UTC, argv, elapsed time, stdout/stderr and exits are in private `format-results.json`. All three exits were 0:

```text
rustfmt --version
rustfmt --edition 2024 --config skip_children=true /private/tmp/rapidraw-gd1b5-switch-child-migration-20261016/proposed-child.rs
rustfmt --edition 2024 --config skip_children=true --check <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-switch-decision-review/src-tauri/src/drive_auth/connected_lifecycle_independent_tests.rs
```

The first two commands prepare/check the private proposal; only the permitted function was installed after proving protected prefix/suffix equality. Python source checks (exit 0) independently replayed the unified patch, reversed the two logged source edits to the entire original child and verified formatter token equality. Whitespace checks passed. Source-only read/hash scripts performed no Rust or fixture execution.

## Frozen bytes and custody

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| Original child | 40455 | `b671ea103c7756967f6e9be1b3fa67fb015b24574c60a01922b0a729065d1af0` |
| Migrated child | 46479 | `04bf5d97950e7cc33716ea297ad169eac06780da4b97b07e23b8a42e4a790444` |
| Unified patch | 7081 | `9ecda89461a1c2f216390ff1103832c5ee5611f0c60629e2c1db39c79563f3fc` |
| Protected prefix outside function | 18298 | `c210980aaa7c362170b61e54f6248cc00daf6abe9fb89aa100fa025fd21c885b` |
| Protected suffix outside function | 19792 | `28eece26df5ca87b4b404a2b781e79adaf044973244c1d0ac0d562d4c1a7dea1` |
| Immutable production session | 209371 | `525be1112c4273988beee762458a603ef8094d05a38cc0d06e9721b65af041ce` |

Authority manifest: `/private/tmp/rapidraw-gd1b5-switch-source-root-review-20261016/migration-inputs.json`, 112067 bytes, SHA256 `55e1c94cafb3fdc81e869dd7db2a3aa3a632eb8215e514748b98b526654ad25b`. All 677 original inputs and exact inventory were verified before editing. Afterward only the owned child differs; all other 676 original files, including production/footer/module/C2 and S1 children, are exact. This new report is the only added path: final inventory **678**. Direct read/hash checks retain HEAD, index and `.git` pointer exactly; no Git command ran.

Private evidence: `/private/tmp/rapidraw-gd1b5-switch-child-migration-20261016` (mode 0700), with `before.json`, `after-before-report.json`, `after.json`, `preservation.json`, full baseline/final/reconstructed child, exact patch, operation/line/assertion ledgers, provenance, formatter outputs, machine receipt, delivery and sealed artifact manifest. Final report/receipt/manifest hashes are reported through Team after sealing to avoid a self-referential hash.

**Pass:** source preservation/patch replay, standalone owned-file formatting and whitespace, original-input/Git-metadata custody. **Fail:** none in these executed source checks. **Not exercised:** compilation/tests/Clippy, localhost/watcher/ticket runtime, candidate/consent races, provider identity or Google/full chain, app/IPC/storage/device/Android/sync acceptance. Historical C3 results and all existing receipts remain unchanged and cannot validate these C4 bytes.

No shared cache lease was acquired or used. No Cargo/rustc/build/cache/socket/provider/network/browser/device/dependency/Git action occurred. Source and evidence are frozen for root review; independent probes/runtime require a separate grant.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `4827df2c58d7c56f226ead2db9040759f1a2688f081544f28d4161b65a2dc3a6` (7892 bytes).
