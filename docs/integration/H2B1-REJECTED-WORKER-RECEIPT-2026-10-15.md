# H2b1 — Fail-closed statistics receipt

**Pass: scoped implementation/tests; not yet independently reviewed or integrated.** Empty/invalid statistics can no longer produce a numeric-zero acceptance shortcut. Only `src-tauri/src/dcp/delta_e.rs` and this new receipt are changed in the repository. The comparator command remains unavailable; this slice does not implement or close Adobe, real-camera, route or GPU acceptance.

## Identity, inputs and ownership

- Task `01a13e23-6679-77e3-83df-0cfa98d577e9`; Codex CLI - Sol, slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`, actual model `gpt-6.1-sol` per current Team runtime roster. No sub-agents.
- Detached snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/stats-guards`, frozen HEAD `8877d7eaa14442814912a199c8de9abdfba5c8b9`; initially clean.
- Reviewed contract SHA256 `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880`, sections5/6/7; independent review SHA256 `c9be23b548446ace92365dafa17f08f1362f8237d9451c355aecd53525aede0b`. Both read from the lead checkout and hashes verified before edits.
- Host macOS26.5.2 arm64, Python3.9.6; `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1 (797e8a9bc 2026-08-05)`, selected inside `src-tauri`.
- Exclusive desktop target `/private/tmp/rapidraw-integration-20261015/target` was granted after R1 released it. **Explicitly released back to the lead after the sole scoped test completed. No build/test remains running.** Logs/patch/metadata are in its new mode0700 child `h2b1-statistics-20261015/`; these generated artifacts are outside the repository.

## API and resulting behaviour

`compute_stats(...) -> Result<DeltaEStats, DeltaEStatsError>`; both `check_thresholds` and `check_gpu_parity` now return `Result<bool, DeltaEStatsError>`. Typed errors distinguish empty sample, mask mismatch, nonfinite sample, negative sample, invalid statistics and resource failure. Invalid input returns `Err`; a valid measured threshold exceedance returns `Ok(false)`; only valid passing statistics return `Ok(true)`. The future report/CLI layer still owns mapping typed errors to its report codes/status.

- Validate both mask lengths exactly, then every supplied error for finite/nonnegative values **before exclusion**. Empty arrays/all-excluded primary samples fail; excluded NaN/Inf/negative values cannot disappear.
- Expose `total`, included `count`, disjoint `shadow_only_excluded`/`clipped_highlights_only_excluded`/`overlap_excluded`, union `excluded`, and legacy inclusive shadow/highlight counters. The union counts overlap once. Threshold functions recheck checked-add accounting, total capacity, finite/nonnegative metrics, bounds and small-sample nearest-rank consistency, rejecting malformed public records instead of trusting their counts.
- Preserve strict CPU/reference mean<2, p95<3, max<5 and GPU/CPU mean<0.1, max<0.5. No epsilon or tolerance relaxation. Preserve nearest-rank p95 `sorted[ceil(.95*N)-1]`; rank arithmetic is checked before indexing.
- The finite nonnegative errors use a running mean to avoid overflow from summing large finite samples. No input clipping is introduced. Sorting is in-place `sort_unstable_by(f64::total_cmp)` after finite checks, removing NaN-sort panic and stable-sort scratch allocation.
- Bound the helper's own included f64 copy using the reviewed80,000,000-sample ceiling (640,000,000bytes), checked byte arithmetic, `try_reserve_exact`, returned-capacity/budget checks and insufficient-reservation rejection. No unbounded sample `collect` or growth-driven copy. Capacity/reserve tests use small injected helpers, not huge allocations. This bounds this helper's managed copy; caller-owned arrays/production internals and the later full2GiB comparison budget are outside this slice. H2b5 must use its one-pass records rather than call this helper and duplicate a full sample set.
- Masks remain caller-supplied; this helper no longer documents deriving a candidate clipping rule. Reference-only shadow/ceiling mask generation, mask hashes, excluded diagnostic metrics and pooled multi-case aggregation belong to later owned slices. These are not implemented by this receipt.

`DeltaEStats` fields remain public for the later harness. Checks validate structural/statistical constraints; without the original samples a threshold helper cannot independently attest that a plausible externally fabricated record came from actual measurements. Input custody and later one-pass construction remain separate evidence.

## Preservation and caller checks

- `rg` call-site search found only in-file tests and `validation.rs`'s existing re-export. Local tests updated for the fallible API; no `validation.rs` edit. Successful library test compilation confirms no external compile break at this base.
- Byte-for-byte comparison against frozen HEAD confirms the entire pre-statistics CIEDE2000/Lab/ProPhoto conversion prefix, including `D50_XYZ`, is unchanged. The full Sharma array is unchanged, SHA256 `e68fb52bda5df455cd964ed73525e85d93dacd4837946b9b6b4fe181f68f2d11`. Only its stale “subset” description is corrected to all34 pairs. Data/formula/tolerance are preserved.
- No renderer/pipeline/conversion/ICC/validation/CLI/dependency/lock/device changes; no Git index/commit/push mutation, full suite, GPU/Android build or `cargo clean`.

## Verification and exact commands

From snapshot `src-tauri`, after checking disk reserve>=6GiB:

```sh
CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false \
CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
CARGO_INCREMENTAL=0 \
cargo test --locked --lib dcp::delta_e -- --nocapture
rustfmt --edition 2024 src/dcp/delta_e.rs
rustfmt --edition 2024 --check src/dcp/delta_e.rs
```

The owned file was formatted before the test, then checked again afterward; all rustfmt invocations exited0. `git diff --check` exited0. No other file was formatted. Pre-build available disk39700623360bytes (36.974GiB), after39682039808bytes (36.957GiB), both above6GiB. Required environment recorded in `cargo-test-receipt.json`.

Scoped test: **exit0, 23 passed / 0 failed / 0 ignored / 138 filtered out**, 74.84seconds including compilation. Baseline selected file had10 tests;13 targeted tests were added. All34 Sharma known answers remain <1e-4; observed max error0.000049498977271467126, pair23. Filtered-out tests were not exercised.

New tests cover empty/all exclusions (including overlap), both shorter/longer masks, disjoint/union/inclusive counts, NaN/+Inf/-Inf in included and all excluded categories, negative included/excluded errors, exact p95 sample counts1/19/20/21, huge finite means without overflow, forged counts/addition overflow and nonfinite/negative/inconsistent metrics, equality at every CPU/GPU threshold plus the immediately lower representable boundary, checked byte/count plans and bounded reserve failure/insufficient/excess-capacity injection.

Build side effect: the existing build script downloaded and verified its ignored `src-tauri/resources/libonnxruntime.dylib`. It is an ignored generated runtime artifact, not a tracked source/dependency/lock change. The build's log/target artifacts are retained; no clean/delete operation performed. One initial test-insertion helper used a repository-relative path while already inside `src-tauri`, causing host `FileNotFoundError` before that insertion; corrected working directory, then completed. This was a host editing-helper error, not a compiler/product failure.

## Results and next consumer

| Check | Result | Scope |
|---|---|---|
| Fail-closed statistics and threshold API | Pass | New focused regressions execute real helpers |
| Sharma/formula/white/conversion preservation | Pass | Byte checks plus all34 known answers |
| Owned-file format and diff whitespace | Pass | Rustfmt check and git diff check |
| Allocation failure behaviour | Pass | Bounded injection/planning; no actual OOM allocation attempted |
| New scoped test failures | None |23/23 passed; external re-export compiles |
| Full-suite/lint/integrated-source review | Not exercised | Lead and independent reviewer gates remain |
| Comparator CLI, ICC, real Adobe/RAW/route/GPU fidelity | Not exercised | Still unavailable/pending later slices |
| Prior launch/UI/basic edits | User-reported | No new device evidence from this task |

Next consumer: independent reviewer on this exact source/patch and receipt, then lead allowlisted import and combined checks. Do not begin H2b2 or change compare availability based on these helper tests. Source SHA and patch are below; this receipt's own hash is supplied in the Team delivery and external `report-receipt.json`.

- Owned source SHA256: `5313d08521d63db0be0357e88e9caaac5100828dca866ad8a9ecaa61764e75fb` (baseline `586d4ee19f07dabc46e65902821bf6e84ad100f45318a5d7e8b4b4b2d56c7f50`).
- Source patch: `target/h2b1-statistics-20261015/delta_e.patch` under the absolute shared target above; SHA256 `200ccceed399ae86692abbecf506f3378097d78d1969085701b00d01cdf150df`.
- Exact test log/exit/env/disk receipt: `cargo-test.log`, `cargo-test-receipt.json`; preservation check: `preservation-check.json` in that same evidence child.
