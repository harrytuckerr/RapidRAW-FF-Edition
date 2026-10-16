# R3A full resolved-name boundary — 15 October 2026

**Pass — actual-driver regression, focused tests, owned formatting and strict all-target Clippy.** Frozen worker repair for independent re-review; not native/frontend integration or provider/UI acceptance.

Worker: Codex CLI - Sol, slot 01a13e03-d91b-7810-a557-ca8e5fd5cd16; actual Team roster model gpt-6.1-sol. Task 01a13e97-4e30-77a3-8e75-c285ae5fbbca. Snapshot <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/import-native-fullname, detached HEAD ea191124c9a52c761336a307fb1bcb09da4e84fc. Explicit initial uncommitted source was rejected-ad1, SHA-256 ad1f52f03e2ca136a6639ee7ac9e9a9a870fee853a58e0022fd8894972a63a20.

## Change and one-callsite proof

The driver still held the full resolved display_name, but detail formatting received a label already reduced to256 scalars. Replacement therefore knew only a prefix and could leave enough filename text to remove the cause/action and useful extension.

The private import_failure_detail helper now accepts resolved_name: Option<&str>. It retains the complete known resolved basename internally, excludes URI-like names using the existing redactor/fallback policy, and normalizes controls without a length slice. Repeated full-name occurrences are removed from the reason using that exact known string. Only then is the64-scalar label generated and the256-scalar detail budget applied. No English parsing, guessed filename boundaries, admission cap or extra I/O.

The sole authorized driver-callsite change is:

    -let detail = import_failure_detail(&label, source, &e);
    +let detail = import_failure_detail(&label, source, &e, display_name.as_deref());

Full name is private context, never an outward unbounded payload. Progress retains the existing bounded label; ItemError and Complete remain bounded. Short ordinary messages,64/256/1024 limits and the first-three-detail policy are unchanged.

Owned writes: the new R3 detail helper, this one driver call, the existing helper-test call signature, two appended focused R3 tests, and this new receipt. No counting/order/event/DTO/RAII/item-operation/staging/alias change. F2 whole-token URI redaction and URI-like-name fallback are unchanged.

Exact final diff against rejected-ad1:132 added /11 removed lines including tests, preserved as fullname.patch below. This is separate from the larger pre-supplied uncommitted R3A delta.

| Preservation check | Evidence |
|---|---|
| Entire prefix before detail-helper comment, including R2a/ownership/DTO code, F2 redactor, item-label and64-scalar detail-label helpers, is byte-identical | SHA-256 ba25c0fefb59140849d42d960b3928cda1e9fa1fa989b3653fdfce16405403c6 |
| Reversing only the single new driver argument restores every byte from summary through R3 test-module start, including alias, driver, operations and R2a tests | SHA-256 7b0e1b9e4523cd611a9b95641bb5aec03343d6bd2b0850cbcd177d6fa53c1427 |
| That region contains exactly one changed callsite | Asserted by stdin exact-string proof |
| Final test assertions equal the pre-fix regression module after necessary helper-call signature reversal | Exact string equality |
| Old import-native-revision source/receipt remain frozen | Reverified ad1 source and receipt17cab33689ee9eac270fb1575ce8508d8b7892d24763e1b339f15a23bae1866f |

## Regression before and after

Two tests were added first with all ad1 production/helper code unchanged. The exact new mismatch test uses real run_import_batch, an injected URI operation setting the full name, and actual ImageContentIssue::RafHasJpegSignature.open_error_message.

That test failed against the old helper: **Cargo exit101;0 passed,1 failed**. Its1004-character ASCII case emitted an A-ending label rather than .RAF, followed by leftover filename text instead of the JPEG cause/action. The pre-fix source, log and binary were preserved. Its source SHA-256 was813bbe8c4a0f25d553d7210496a1e3cabda0e45293cad62f6efe5b641ea9b097: ad1 plus the two tests only.

The unchanged regression assertions then passed with the fix. Cases:

- A×500 + B×500 + .RAF, through actual driver and actual R1 mismatch formatter.
- 撮×300 + 影×300 + .RAF, through that same boundary. Metadata-only injection; no filesystem-name-limit assumption or provider execution.
- The oversized ASCII name repeated in the actual validate_import_read error, through the real driver, with a synthetic URI/grant suffix input.

ItemError and Complete must retain “has a JPEG signature” and the full original-selection action for mismatch, or permission/action text for read failure. Assertions require useful recognizable prefix and .RAF tail, <=64 label, <=256 detail, <=1024 summary, Complete containing actual detail, exact1/0/1 counts, safe progress, and no source-URI/fake-grant/path markers.

Existing255-scalar and URI-punctuation cases and all other assertions are unchanged except the required helper-call signature update. No assertions/goldens were weakened after the observed failure.

| Command/check | Result |
|---|---|
| Pre-fix exact new mismatch-driver regression | Cargo101;0 passed/1 failed, expected reproduction preserved |
| cargo test --offline --locked --lib file_management::r3_terminal_tests -- --nocapture | **0:19 passed,0 failed,0 ignored,195 filtered out** |
| cargo test --offline --locked --lib file_management::tests -- --nocapture | **0:13 passed,0 failed,2 ignored,199 filtered out** |
| cargo clippy --offline --locked --all-targets -- -D warnings | **0**, including tests |
| rustfmt --check --edition 2024 src/file_management.rs | **0** |
| Whitespace/scope/artifact checks | **0** |

Exact pre-fix command:

    cargo test --offline --locked --lib file_management::r3_terminal_tests::oversized_resolved_names_keep_actual_raf_error_through_the_batch_driver -- --nocapture --exact

The wrapper asserted the expected failing count; Cargo's recorded failure remains a failure, not relabelled test Pass. The old255-case results did not cover this composition boundary.

Batch-outcome tests use the real driver; local existing outcome tests also use real item operations and temporary files. Admission/guard/serialization/helper tests use their respective seams. New resolved-name cases inject the operation result and use actual formatters; no provider, Android JNI or Tauri event transport. Existing pre-next-item progress is unchanged; terminal-emitter barrier coverage remains unexercised/nonblocking. Two private RAW controls remain ignored.

## Environment and immutable artifacts

Actual environment: gpt-6.1-sol; macOS26.5.2 /25F84 arm64; Rust1.98.1 (48a229cea), Cargo1.98.1 (797e8a9bc), rustfmt1.9.0-stable. All builds ran from this snapshot's src-tauri directory with:

    CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target
    CARGO_PROFILE_DEV_DEBUG=0
    CARGO_PROFILE_DEV_INCREMENTAL=false
    CARGO_PROFILE_TEST_DEBUG=0
    CARGO_PROFILE_TEST_INCREMENTAL=false
    CARGO_INCREMENTAL=0
    CARGO_NET_OFFLINE=true

Free space asserted >=6GiB before each command: pre-fix39,385,636,864 bytes; final R3/R2a/Clippy39,300,714,496 /39,268,982,784 /39,269,064,704. Command JSON contains argv/cwd/env/toolchain/input hashes/exits and monotonic durations: pre-fix60.60s; final59.85s /1.53s /14.70s. These are build receipts, not product performance.

Pre-copied ignored runtime remains2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a; all logs report valid existing runtime and no download. Cargo.lock remains7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def. No cold target, clean, resolve or dependency refresh.

**Final src-tauri/src/file_management.rs SHA-256:56ca21c1488f0e72fde52796e3b3fa97e1a3179f56d467cce599e615ad09d81e.**

Evidence directory:
 /private/tmp/rapidraw-integration-20261015/target/r3a-fullname-20261015/

| Artifact | SHA-256 |
|---|---|
| fullname.patch (7,923 bytes) | 958b39cf025176118b33611d374509095db5ad543e2aea6e061b23bd45ae0003 |
| red-regression.log | d8a384f15746d1734a5ead6706dd9bbf54abe4973c1efe81d476e7c1ecd098c3 |
| red-regression-test-binary | f4a0abb48d795d00dad727b4d7a18a462329eb60d7279f677234d03ea0f30494 |
| native-final-test-binary (29,829,504 bytes) | dae698e51536ced9848cc7e93c3eaba9bea82db02e494c9d6460d1b0300242aa |
| r3-terminal-tests.final.log | 5f700c64dd2b1037ab0b44b27bb309dfd88ec792e2ddd361a70ce430bac0acd0 |
| r2a-tests.final.log | 1b26159d4ce78fce7df316e6dc78cb12136c3e9c044bd8499d618859dd303b4c |
| clippy.final.log | 19547f339c1f29fe2422b1e82c03162386262b6d079993cde5daa74daab64384 |
| artifacts.json | 7aacf711223d895d1b68ec70b0304d2237eba2fee85685357dbde5d49b28f777 |

Manifest also hashes baseline/prior source/receipt/log copies, pre-fix/final full source and all command metadata. Binaries were copied immediately after test runs before cache reuse; host unit-test artifacts, not application/device packages. Receipt SHA is supplied in Team delivery.

Old ad1 source/17cab336 receipt and prior artifact directory were not edited. Read-only commands included git rev-parse/status/diff/check, shasum, rg/sed/tail and stdin Python string/diff/hash proofs. Rustfmt wrote only the owned source. Final worktree status is that source and this new receipt.

**Pass:** demonstrated actual-driver regression; final19R3 +13R2a tests; strict all-target lint, formatting, one-callsite proof and artifact identity.

**Fail — preserved historical observation:** exact new regression against unchanged ad1 helpers failed as expected. No final failing check.

**Not exercised:** full suite/two ignored private RAW controls, actual provider/Android/JNI/Tauri transport, frontend/UI/device/atomic integration, R2b, rendering/profile/fidelity. No real photo or credential input.

No Git mutation, old snapshot/receipt/artifact, frontend, provider, dependency/lock or device edit. Source/tests/receipt are frozen for independent re-review; exclusive native cache is explicitly released in delivery. Stop here.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `bfe4ac880f1ebcb51441ca1919957f6e9978f993fb405b31dc49a1c6373c4dd2` (9949 bytes).
