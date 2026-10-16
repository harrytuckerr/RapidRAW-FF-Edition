# R3A revision independent review — 15 October 2026

**Verdict: bounded Revise — F2 is repaired; F1 still fails for a URI-resolved display name truncated before substitution.** The255-scalar regressions pass, and native ownership/counting/DTOs remain unchanged. One narrow formatting-context correction and regression are needed; no broader native or frontend redesign.

Reviewer: Codex CLI - Astra, actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed Sol's `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/import-native-revision` at detached **`ea191124c9a52c761336a307fb1bcb09da4e84fc`**. Report checkout remains **`4c1e965a53de9c835df920b2e3d69c2c8a864708`**. Incremental comparison only against rejected0fa and prior review `c23f81f0cb4d36950d646a42af875cf65a4c9891f9eac2331883646b54519019`; no restart of the original guard/item-operation review. R3B frontend was not edited or independently approved here.

## Frozen inputs verified

| Input | SHA-256 independently verified |
|---|---|
| Revised `src-tauri/src/file_management.rs` | `ad1f52f03e2ca136a6639ee7ac9e9a9a870fee853a58e0022fd8894972a63a20` |
| `docs/integration/R3A-REVISION-RECEIPT-2026-10-15.md` | `17cab33689ee9eac270fb1575ce8508d8b7892d24763e1b339f15a23bae1866f` |
| `revision.final.patch` | `c944fa7cb5fe06f6a17b8c7b689f6db8a7294cdd4325ef71a3fe43d0065e5fdb` |
| `alias-only.patch` | `6482ebeb1330f723aadd3f4e2491929145290b0e5094fcb5f0eb5b5da3cce9a5` |
| `artifacts.final.json` | `e357cf34de9929d73f5dc78c7aeb6a0430f3f6b8b83690babf5c732412e15699` |
| Immutable `native-final-test-binary` | `df3bf0498f31d81eb0bd038cef36fcf36078234f50b6f6a68e59695f21aba221` |

Evidence prefix: `/private/tmp/rapidraw-integration-20261015/target/r3a-display-revision-20261015/`. All artifacts enumerated in the final manifest were rehashed, including final/historical source, logs, binaries and failed initial lint. Full patch and receipt read. Original `import-terminal` source remains rejected hash `0fa55c4ff247519e4e01962b918725b539cf0bfdc2d0fdd36ae16b1f9c08287d`.

## Remaining F1 — P2, lost full resolved name

**Locations:** revised `file_management.rs:4127` truncates the resolved basename to256; `4151–4174` receives only that label and the source URI, and replaces the truncated label inside the error; `4175–4179` bounds the remaining reason. The full resolved name is available in the driver but is not retained as formatting context through this path.

**Concrete reproducer, ordinary native string inputs:** source `content://synthetic/item/7`; resolved display name `"A".repeat(500) + "B".repeat(500) + ".RAF"`; mismatch message from actual R1 formatter:

```text
“<full 1004-scalar name>” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.
```

The label becomes256 A characters. The displayed64-scalar label therefore also loses `.RAF`. URI source basename is merely `7`, so the basename replacement cannot remove the full provider display name. Replacing256 A characters in the actual message leaves244 A characters plus500 B characters before the reason. The190-scalar reason budget is exhausted by this residue: the final256-scalar detail contains neither `JPEG signature` nor `Select the original RAW`. Counts still correctly report failure; this is the original actionable-error defect, not data loss or a false count.

An independent stdin ASCII probe of these exact string operations executed and reported raw-name/label/detail lengths1004/256/256, cause=False, action=False, extension=False. No new Rust compilation or provider invocation was used for this counterexample. This is a supported native helper/formatter input with no declared name-length admission cap, **not a claim that a physical provider produced this name** or a speculative script attack. `validate_import_read` and the R1 formatter preserve the full supplied display name, while the URI carries no filesystem basename from which to recover it.

**Smallest correction:** keep the full resolved basename as private formatting context until redaction and known-name substitutions finish; apply scalar bounds to displayed output afterward. Passing the already available full name into the detail helper is sufficient; no wire DTO/counter/guard change is needed. If the current helper-only grant excludes the driver call-site argument, lead should explicitly grant that one argument/context change. Preserve whole-token URI redaction before substitutions. Do not “fix” this by rejecting long provider names or clipping the error without its cause.

**Regression:** inject this long resolved name through the real batch-driver seam, create its error via actual `validate_import_read`/R1 mismatch formatter, and assert ItemError and Complete retain the JPEG cause and retry action within256/1024. Include a repeated-name read-error variant and a Unicode name >256 scalars. This seam is formatting/bookkeeping evidence, not real provider/decode evidence. Existing tests stop at255 scalars, so17 passing tests do not cover this case.

## Accepted portions of this revision

- **F1 bounded budgets:** the64-scalar head/ellipsis/tail label plus2 separator leaves at least190 for reason. For full names still available (including tested255-scalar ASCII/Unicode and local-source basenames), repeated names are substituted before reason truncation; actual R1 and read-error cause/action regressions pass. First-three collection, remaining-failure count and1024 summary logic are byte-preserved. The defect above concerns loss of input context before these otherwise sound bounds.
- **F2 repaired:** replacement consumes the entire non-whitespace URI token, including punctuation and fake grant suffixes. Exact-source substitution requires equality with the whole token, preventing the old prefix leak. Mixed case, multiple URI tokens and raw URI-like resolved names are covered; such names become Item N. Redaction precedes control normalization/truncation. Whitespace remains the stated token boundary; no broader credential/transport guarantee is inferred.
- **Alias only:** removing `ImportBatchItem<'a>` and reversing its single parameter annotation restores pre-alias source **exactly**, SHA `2159e54cae67235b72f42f40e075408b937ca681db12c71d76ba2bbcbc468ad5`. `+ 'a` with `ImportBatchItem<'_>` retains nonstatic borrowing closures; no suppression or DTO/count/body change. Prefix before redactor and summary-through-existing-R2a-tests region (after alias reversal) compare byte-for-byte with0fa, preserving previously reviewed ownership, driver and item operations.
- **Receipt:** correctly distinguishes real-driver tests from helper tests and injected URI outcomes; preserves the initial Clippy101 rather than relabeling it; acknowledges accepted progress timing and absence of terminal-emitter barrier coverage. It needs only an updated F1 scope/result after this remaining repair.

## Execution and disposition

**Executed by reviewer:** read-only identity/preservation scripts; exact alias reversal; counterexample string probe; the verified immutable binary with `file_management::r3_terminal_tests --nocapture` — **exit0,17 passed,0 failed,0 ignored,195 filtered**. Expected caught unwind-test panic is not a failure. The tests created only their ordinary temporary local fixtures; no private photos, native cache writes or new binaries. Tool output retains this reviewer run; no new log file was written outside the owned report.

**Read and hash-verified, not rerun:** worker final R3 log `2a36439f…`, R2a log `8bb8cbd3…` (**13 passed,2 ignored**), strict all-target Clippy log `53ac591f…` (exit0), command manifests and formatting receipt. These prove those checks on the frozen source, not coverage of the new counterexample. Original pre-alias Clippy failure remains preserved.

**Pass — F2, alias/lifetime/preservation review, executed17 existing R3 tests. Fail — remaining F1 over256-scalar resolved-name cause/action loss. Not exercised — new-case Rust regression, R2a rerun by reviewer, Cargo/build/cache mutations, combined frontend/native integration, full suite/private controls, Tauri/provider/device or fidelity.** No source/lock/Git/frontend edit. Only this report written.

Next consumer: one narrowly assigned full-name-context repair and regression, then frozen source re-review; retain accepted guard/DTO/counting/F2 changes. Root independently owns frontend review and atomic combined checks. This review does not approve my own R3B frontend or authorize any downstream work. Report hash is delivered through Team message.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `118f3ef26d2496eb9c487745fb9e4807dd75a018492ffd2c30a440ceaa632bbd` (8626 bytes).
