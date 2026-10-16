# GD1b3 — disjoint HTTP status pattern revision

Source-only correction, 2026-10-16 Melbourne. Actual author `gpt-6-astra`; task `01a13ff0-6247-7410-9714-dd52cde2e135`. Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-token-response`, detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`.

Only `token_response.rs:263` changed. The residual Client match arm is now disjoint from the preceding special-status arms:

```diff
-        400..=499 => HttpFailure::Client,
+        402 | 404..=428 | 430..=499 => HttpFailure::Client,
```

This adds 18 source bytes. The old first-match behavior excluded 400/401/403/429 from the residual range already; the new pattern spells out exactly that remainder. BadRequest/Unauthorized/Forbidden/RateLimited, 3xx/5xx and unexpected-status behavior are unchanged. No lint suppression or other production/test/API/policy change.

The lead reported independent execution against original `a943…`: author14 + independent10 + default272 passed (14 ignored), formatting passed, but strict Clippy exited101 on `match_overlapping_arm`. Those are attributed prior-validator results, not reruns by this author. The original failed candidate/receipt remain preserved; runtime equivalence does not waive strict lint acceptance of the revised candidate.

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| Revised `src-tauri/src/drive_auth/token_response.rs` | 33825 | `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886` |
| Private `original-token_response.rs` | 33807 | `a943573b0ab5a00be6df8d7e5cd98f6d27c66fffb92766d7d2eeb9b233656f61` |
| Private `original-receipt.md` | 13219 | `abd7c8ccd5182492b8cacb02b2f2e17826440fa70df31c5bcdf488fad5327e1c` |
| Private `correction.patch` | 893 | `f8ebf5af5dd553c832bb8bc642d7c7d7f417f38aa265592c27ebceed55cd8a25` |
| Private `protected-before.json` | 92892 | `7c32f469433a3a9373f10922ecb625d83db7b6831efdd7ce0ee7d89fe06b1c4e` |
| Private `checks.json` | 1148 | `2bf72a201cd218f5c35fc09c3b9876e1da7eb399c28094022178e9e09f519a30` |
| Private `manifest.json` | 1413 | `3c74caeb9219099891665ad6cb48afbc4d61221e52f86c2be46fd1257adf394d` |

Private root: `/private/tmp/rapidraw-gd1b3-status-revision-20261016/`. Original source and author receipt were copied and verified before editing. The original worktree receipt and GD1b4 design remain unchanged.

**Pass:** exact one-line byte replacement assertion; complete inline test-section byte equality (all14 definitions preserved); elementary integer-set equality between the old residual and explicit ranges; all564 captured protected files hash-identical after editing. Protected inventory includes tracked sources/manifests/locks and supplied untracked helper/module/author receipt/design. No validator files were edited. Standalone `rustfmt --edition 2024 --check --config skip_children=true src-tauri/src/drive_auth/token_response.rs`, `git diff --check` and tracked `git diff HEAD --exit-code` all exit0. The corrected source remains untracked as supplied; the dedicated no-index patch proves its change. No-index diff exits1 because bytes differ, as expected.

**Not exercised:** revised Rust compile/tests/Clippy, any native/shared cache, socket/provider/account/browser/device operation. No new test functions or altered expectations; no Cargo/dependency/Git mutation. Source-only scope is complete, frozen for independent validation. Root/validator must establish revised strict-lint and runtime acceptance before integration; this receipt makes no OAuth/grant/storage/sync claim.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `fb3017a78f64105e1a09ec1089a6bdddd887c53e0c74f07e8200612f9532713b` (3556 bytes).
