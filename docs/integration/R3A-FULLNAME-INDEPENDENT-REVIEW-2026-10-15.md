# R3A full-name correction — independent review, 15 October 2026

**Accept the frozen native correction.** The remaining F1 boundary is repaired: the complete resolved basename survives until known-name substitution, before label/reason limits. No new defect found in this incremental change. Earlier accepted F2/alias/ownership/counting conclusions remain supported by byte-preservation checks. This is native source acceptance, not combined frontend, transport or device acceptance.

Reviewer: Codex CLI - Astra, actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Producer `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/import-native-fullname`, detached **`ea191124c9a52c761336a307fb1bcb09da4e84fc`**. Report checkout `raf-review` remains **`4c1e965a53de9c835df920b2e3d69c2c8a864708`**. Baseline: rejected-ad1 in `import-native-revision` and prior report SHA `118f3ef26d2496eb9c487745fb9e4807dd75a018492ffd2c30a440ceaa632bbd`. No frontend review/self-approval or source changes performed.

## Identity and scope

Independently verified source **`56ca21c1488f0e72fde52796e3b3fa97e1a3179f56d467cce599e615ad09d81e`**, full receipt **`bfe4ac880f1ebcb51441ca1919957f6e9978f993fb405b31dc49a1c6373c4dd2`**, and every artifact hash in `/private/tmp/rapidraw-integration-20261015/target/r3a-fullname-20261015/artifacts.json` (manifest SHA `7aacf711223d895d1b68ec70b0304d2237eba2fee85685357dbde5d49b28f777`). Full incremental patch/receipt read.

| Artifact | SHA-256 |
|---|---|
| `fullname.patch` | `958b39cf025176118b33611d374509095db5ad543e2aea6e061b23bd45ae0003` |
| Preserved final test binary | `dae698e51536ced9848cc7e93c3eaba9bea82db02e494c9d6460d1b0300242aa` |
| Failed pre-fix regression log | `d8a384f15746d1734a5ead6706dd9bbf54abe4973c1efe81d476e7c1ecd098c3` |
| Final R3 log | `5f700c64dd2b1037ab0b44b27bb309dfd88ec792e2ddd361a70ce430bac0acd0` |
| Final R2a log | `1b26159d4ce78fce7df316e6dc78cb12136c3e9c044bd8499d618859dd303b4c` |
| Strict all-target Clippy log | `19547f339c1f29fe2422b1e82c03162386262b6d079993cde5daa74daab64384` |

## Correction assessment

`import_failure_detail:4153` now takes private `resolved_name: Option<&str>`. It selects the full known basename, rejects URI-like names using the existing fallback policy, and normalizes controls without length truncation. URI-token redaction still precedes known-name substitutions. Full repeated basename occurrences are replaced before constructing the64-scalar label and calculating the remaining256-scalar detail budget. The prior1004-scalar ASCII and604-scalar Unicode names therefore retain their recognizable tail/extension, JPEG cause and full retry action. The repeated read-error name similarly no longer occupies the reason budget. No guessed English parsing or provider name-length rejection was added.

The driver passes its already available `display_name.as_deref()` at the single authorized Err-branch call. Full metadata remains private formatting context; progress remains bounded, and ItemError/Complete retain the256/1024 limits and first-three-failure policy. No I/O or wire field was added.

Executed exact comparisons establish:

- All bytes before the detail-helper comment are unchanged from ad1, including R2a, guard, DTOs, F2 redaction and label helpers.
- Reversing **only** the new driver argument restores the entire summary-through-existing-R2a-tests region byte-for-byte: alias, ordering, counts, item operations and staging are preserved.
- The failed-regression source has exactly the ad1 production code plus new tests. Final test assertions equal that regression module after the necessary helper-signature adjustment. Assertions were not weakened to obtain a pass.

## Runtime evidence and disposition

**Executed by reviewer**, after binary hash verification:

```sh
/private/tmp/rapidraw-integration-20261015/target/r3a-fullname-20261015/native-final-test-binary file_management::r3_terminal_tests::oversized_resolved --nocapture
```

**Exit0:2 passed,0 failed,0 ignored,212 filtered.** These two functions cover ASCII1004 and Unicode604 mismatch names through the real batch driver and actual R1 formatter, plus repeated oversized read-error names through the driver and actual read-error formatter. They assert both ItemError and Complete retain cause/action, bounds, counts and safe display text. The URI operation is injected; this is not JNI/provider, real RAW decode or Tauri delivery evidence.

**Inspected, hash-verified worker evidence:** exact mismatch regression failed before the fix (Cargo101,0 passed/1 failed), then final R3 **19 passed**, R2a **13 passed/2 ignored**, strict all-target Clippy exit0 and owned formatting exit0. The historical failure remains failure evidence, not relabelled Pass. The two private controls and terminal-emitter barrier remain unexercised; no coverage expansion claimed.

**Pass — incremental source/preservation review, unchanged-assertion failure-to-pass evidence and two independently rerun focused regressions. No remaining finding in the assigned correction. Not exercised by reviewer — Cargo/build/cache writes, full suite/R2a rerun, private controls, real transport/provider/device, combined native/frontend integration or fidelity.** Read-only commands were git rev-parse, shasum, rg/cat/tail and stdin Python identity/string comparisons; only this report was written. The existing binary ran without new compilation. No Git/lock/source/UI/device edits.

Next consumer: lead may integrate this exact accepted native source with the separately independently accepted frontend and run its assigned combined checks. This report does not independently approve my own R3B implementation or close native transport/device gates. Stop at this frozen report; its hash is supplied through Team message.
