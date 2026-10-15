# R3A independent source review — 15 October 2026

**Verdict: Revise before accepting this native source.** Run ownership and terminal outcome accounting are sound in static review. Two P2 defects in the new user-visible diagnostic helpers remain: long labels remove the actionable reason, and URI punctuation can expose an unredacted suffix. Repairs fit the already owned backend file and its tests; no DTO, R2a, logging or frontend scope expansion is needed. Backend/frontend still must integrate atomically after review and tests.

Reviewer: Codex CLI - Astra, actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed producer `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/import-terminal`, detached HEAD **`ea191124c9a52c761336a307fb1bcb09da4e84fc`**. Report checkout `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/raf-review` remains **`4c1e965a53de9c835df920b2e3d69c2c8a864708`**. Only this new report written. No Rust/test-binary execution, cargo/build/cache/device/source/lock/Git mutation.

## Identity and preservation

All supplied hashes independently recomputed; the current source diff equals the full frozen diff byte-for-byte. Full worker receipt and changed source/test regions were read, with old/new item-operation comparison. Governing contract remains `050005a661b88d7cec03252ae500970452b7cae599b409edc03507991aadab88`.

| Input | SHA-256 |
|---|---|
| `src-tauri/src/file_management.rs` | `0fa55c4ff247519e4e01962b918725b539cf0bfdc2d0fdd36ae16b1f9c08287d` |
| `docs/integration/R3A-NATIVE-TERMINAL-RECEIPT-2026-10-15.md` | `6c6e7647a044773dd48abf782c601bc7fc2582bd09c61a48f6743f230e4f3cfa` |
| `/private/tmp/rapidraw-r3a-20261015/r3a-full.diff` | `e6d89648a52d97db72b70f07c94d0e2e81a2617a40b80006d2a80ce3b09ef0a0` |
| Committed producer base file (same source as lead `05dd25e7`) | `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7` |

Executed stdin Python comparison confirms the prefix before the replaced command is identical except the AtomicBool import, all content from `generate_filename_from_template` through the old file end is preserved, and the old item closure/new `import_one_item` bodies are whitespace-insensitively equal after removing only `*display_name = Some(resolved_name.clone());`. This confirms R2a helpers and existing local copy/sidecar/delete/optional-error behavior were preserved. The formatting-normalized check is not a parser/compiler proof; the text diff was also inspected.

## Required corrections

### F1 — P2: truncation removes the entire failure reason

**Location:** `file_management.rs:4122,4129–4138`, consumed at4192–4201; existing long-name test5537–5583 only checks length/safety, not usefulness.

**Concrete reproducer:** use a missing local source with basename `"L".repeat(251) + ".RAF"` (255 ASCII characters). The real item operation returns `Source file not found: <source>` at4288. Label retains255 characters; `import_failure_detail` formats label, colon, space, reason and takes the first256 scalars. The resulting ItemError and summary detail are **the entire filename plus a colon, with no reason**. A256-character label yields filename only. The existing300-character fixture also hides the defect because its assertions never require a reason for that detail. This occurs without accepting any bad image or using a provider.

**Consequence:** the final counts correctly report failure, but the user cannot distinguish missing source, JPEG-under-RAF mismatch, read failure or publication failure for long names. This fails the contract's actionable bounded error requirement. It is not a false-count or data-loss finding.

**Smallest correction:** give the failure label and reason separate scalar budgets within256, reserving meaningful space for the reason (for example64 +2 separator +190). Normalize repeated occurrences of the known full source/display name within the reason to the short label too. Merely shortening the prepended label is insufficient for existing mismatch text: `ImageContentIssue::open_error_message` starts with the quoted full display name before “has a JPEG signature” (`formats.rs:164–167`). Keep progress's independent <=256 limit and terminal DTO unchanged; preserve R2a error producers.

**Required regression:** through the real batch driver, assert a255-character missing-source basename retains `Source file not found` in both ItemError and Complete; also pass a long resolved URI display name plus the actual mismatch formatter's message through the diagnostic seam and require `JPEG signature` to survive. Test Unicode truncation and total <=256, first-three storage and summary <=1024. The URI seam is explicitly an injected outcome, not provider execution.

### F2 — P2: redactor treats valid URI punctuation as the end of the URI

**Location:** `file_management.rs:4098–4106`, invoked for new failure details4136. Existing resolved-name test5587–5607 includes a secondary URI, so this is within the intended input domain.

**Concrete reproducer:** use source `content://provider/item/7`, resolved name `DSCF5389.RAF`, and an error containing `via CONTENT://other/document/a,b?grant=TEST_ONLY`. The exact-source replacement does not match the secondary URI. The redactor outputs:

```text
via [content URI],b?grant=TEST_ONLY
```

Semicolon and parenthesis examples similarly preserve `;b?grant=TEST_ONLY` and `(b)?grant=TEST_ONLY`. These are harmless synthetic marker values, not observed credentials. Independent ASCII stdin probes reproduced the specified transformation; no Rust code was executed by this reviewer.

**Cause/consequence:** comma, semicolon, apostrophe and parentheses are allowed URI sub-delimiters, including in path/query components; they cannot safely terminate this redaction. A returned provider diagnostic can therefore carry a raw URI suffix/query into the new ItemError and terminal toast even though `content://` itself is gone. This is a concrete masking defect, not evidence of an actual provider leak or a claim that URI text grants authority. Primary grammar: [RFC3986 §2.2](https://www.rfc-editor.org/rfc/rfc3986.html#section-2.2), [§3.3](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.3), [§3.4](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.4).

**Smallest correction:** conservatively redact the entire URI-bearing non-whitespace token (or another boundary rule that consumes all URI-valid punctuation), accepting removal of adjacent punctuation when ambiguous. Do not preserve punctuation at the cost of exposing URI components. No new parser/dependency or changes to existing diagnostic logging are needed. Apply redaction before display truncation. Update the existing helper expectation if the trailing comma is deliberately consumed.

**Required regression:** extend the existing secondary-URI driver seam with comma/semicolon/apostrophe/parentheses and mixed-case scheme, asserting the entire synthetic suffix marker is absent from ItemError **and** Complete, not merely that the scheme is absent. Retain Unicode/control-character/bounds cases and ordinary actionable reason text.

## Native ownership/counting assessment

| Area | Static conclusion |
|---|---|
| Admission3975–4004 | Empty list, blank destination, noncanonical UUID and non-JS-safe count reject before any events/work. Uuid parse+canonical string equality rejects uppercase/nonhyphenated forms. Canonical nil UUID is allowed by this backend predicate; normal client generates fresh UUIDs, and the contract does not require a UUID-version allowlist. |
| Ownership3953–3964,4395–4408 | Compare-exchange occurs before spawn. Successful Acquire and Drop Release provide correct exclusion/handoff; Relaxed failure ordering is sufficient because the loser uses no protected data. The non-Copy guard moves into the closure and named `_guard` remains alive through `run_import_batch` and its complete-emission attempt. It is not an underscore-pattern discarded binding. Normal return/unwind releases it; no await occurs inside its ownership path. |
| Spawn/handle | Installed locked Tauri2.12.1 `src/async_runtime.rs:119–125,140–144,311–317` delegates to Tokio. Installed Tokio1.53.1 `src/runtime/task/join.rs:18–23,357–364` detaches on handle drop. Dropping the command's JoinHandle therefore does not release the closure-owned guard or cancel a running batch. No crash/abort terminal guarantee is claimed; unwind release is distinct from delivering a completion. |
| Counts4163–4222 | Admitted N is positive and <=2^53−1; each successful item return increments imported once, each Err increments failed once, without early batch return. Each counter <=N and their sum=N, so u64 arithmetic is safe. Typed complete is constructed once from those counters; failures are not inferred from events or destination existence. Start/progress/error/complete all carry the same runId. |
| Transport4050–4060 | Each typed variant maps to the intended event. Emit failure logs and returns; the loop and terminal attempt continue, and transport failure does not alter file outcomes. The driver emitter is a void callback, while the real adapter absorbs emit Err. Tests collect events; they do not exercise real Tauri emit failure. |
| Bounds4143–4158,4194–4195 | At most three already-bounded details are retained. Counts remain exact for all failures. Summary includes remaining-failure count and is capped at1024 scalars. At the admitted maximum, numeric prefix + three256-character details + suffix still fit1024; no unbounded error list. Intermediate string transformations may allocate proportional to a single error, so this is a retained-message bound, not a global memory budget. |
| Existing operation4252–4383 | Only resolved URI display-name capture added; no second provider read. Local copying, sidecars, ignored optional errors and deletion remain as before. Imported means the existing operation returned Ok, not proof that R2b transaction/original-preservation issues are solved. |

**Progress timing: nonblocking deviation, no functional correction required.** Lines4180–4185 emit current=i before attempt i, then4207–4212 emit N after the final attempt. For each intermediate i>0, that emission occurs after attempt i−1 and before any next item I/O. Thus the observable currents remain0,1,…,N and truthfully count completed attempts; path identifies the about-to-attempt item. There is no false terminal success, count overstatement or stalled next-item work hidden by this placement. It differs literally from contract “after each outcome” because failure-detail/error emission occurs before the following progress. Record this accepted timing interpretation in the receipt; do not churn the loop solely for prose conformity. Frontend must continue treating Complete as authoritative and tolerate independent event delivery order.

## Test and receipt scope

**13 test functions defined; zero executed in this review and zero reported executed by worker.** The five local batch tests use actual `run_import_batch` → `import_item_operation` → `import_one_item`, real temporary filesystem copies and real missing/collision errors. Their bytes are intentionally arbitrary strings, not valid encoded photos (`5449,5489–5490`); they validate copy/bookkeeping, not RAW/JPEG decode or Android URI validation. Existing R2a tests are preserved, not rerun. The resolved-URI test injects a result/name; it does not invoke a provider.

Admission/count/guard tests use real helpers and local atomics; concurrent barrier test holds the winning guard until all contenders tried. Busy test holds the guard during item work and releases after join. It does **not** currently assert exclusion while the terminal emitter itself is blocked. Source lifetime is correct; add a meaningful emitter barrier assertion when strengthening the test, rather than claiming this terminal-stage runtime coverage already exists. Unwind test proves intended guard Drop semantics if executed, not Tauri panic completion.

**P3 receipt precision:** line73 says “All tests use the real run_import_batch”; wire serialization, request/count, concurrency, unwind and display-helper tests do not. Replace with “Batch outcome tests exercise the real driver; other tests exercise admission, guard, serialization and display helpers.” The detailed table is otherwise candid. Line111's “no mismatch” should acknowledge F1/F2 and the accepted progress timing interpretation; regenerate the receipt/hash after fixes. No source-level test pass may be inferred from rustfmt.

## Supplemental lead runtime evidence received before delivery

Lead supplied `lead/docs/integration/R3A-FIRST-NATIVE-VALIDATION-2026-10-15.json` (SHA256 `615b0e42d1bc7dfac0ea8e1dde5220555d06e9d37dafa522a1cb762311b4e468`), read in full. It records `cargo test --offline --locked --lib file_management:: -- --nocapture`, exit0, **26 passed / 0 failed / 2 ignored**:13 R3 plus13 existing R2a tests, with the two private controls ignored. Inspected native log SHA256 `0421af0c2b623b8c7a23f4d7bd8c124da9478966331adc9aeb18c99e7225bf09` agrees. This is lead execution against frozen0fa source, not execution by this reviewer; it does not waive F1/F2.

Lead also compiled and ran a standalone Rust1.98.1 probe of the exact three string-helper bodies. Reviewer read the probe and verified those extracted bodies equal frozen source exactly. Probe source SHA256 `72b304f20fa38d1c50058e91dcc30eb199b65863d33614beea4b4a4cf1f65abe`; log SHA256 `179017e95da9c1af87bbb811f01a3bbd6573556c92865ab6e2d0bdfe3a91905f`. It reproduces lost `Permission denied` with a255-character label and the comma-URI fake suffix. Compile/run exit0 are recorded in the lead receipt. This adds isolated **helper runtime** corroboration to the independent source/probe findings, not real native command, provider or UI reproduction. Reviewer did not execute either preserved binary or use the cache.

## Disposition, executed checks and next consumer

**Pass — static guard/ownership/counting review; frozen identity and preservation checks; read-only `git diff --check`. Fail — F1 actionable reason and F2 full URI-token redaction. Pass — inspected lead26-test log and helper runtime reproduction evidence. Not exercised by reviewer — compilation/Rust tests/cache; not established by either evidence set — real Tauri emission failures, terminal-emitter exclusion at runtime, frontend R3B, device/provider, R2b, fidelity.** Worker rustfmt pass is recorded as worker evidence only; reviewer did not run rustfmt or a compiler.

Executed commands: read-only git rev-parse/show/diff/check, shasum, rg/cat/sed/nl; stdin Python for hashes, exact diff/preservation comparisons, test-function count, ASCII255/256/300-character truncation probes and three URI-punctuation probes; primary RFC3986 read. Probe output is an independently executed model of the short string rules, **not production Rust execution or an installed-app reproduction**. No probe files, fixtures or source copies written.

Next worker: repair only F1/F2 and associated backend tests/receipt under explicit assignment; keep guard, event DTOs, item-operation and R2a code frozen unless a newly found defect requires review. Return new source/diff/receipt hashes for independent confirmation. When lead grants the native cache window, run scoped R3 tests and existing R2a tests, then assigned integration checks; no zero-test filter acceptance:

```sh
# Future only, in assigned implementation checkout/src-tauri with exclusive cache ownership.
cargo test --offline --locked --lib r3_terminal_tests -- --nocapture
cargo test --offline --locked --lib file_management::tests -- --nocapture
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
```

This backend changes the required command argument and event shapes. It cannot land alone with the old frontend. Independent R3B review, combined checks and real native transport remain separate gates. This report does not authorize source repairs, native-cache use or any later implementation slice. Report SHA is supplied through Team message.
