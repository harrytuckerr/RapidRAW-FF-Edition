# H2b1 revision — Accurate mean and feasible public records

**Worker result: Pass on the scoped correction/tests; independent re-review and import remain pending.** The rejected H2b1 artifact was not imported and is preserved. This revision fixes its demonstrated mean-boundary false Pass and impossible-record false Pass without changing the strict acceptance expressions.

## Identity, inputs and ownership

- Task `01a13e39-3770-7833-b770-b64bfa237196`; Codex CLI - Sol, slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`; actual model `gpt-6.1-sol` per current Team roster. No sub-agents.
- Snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/stats-revision`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9`. Initial uncommitted `delta_e.rs` was exactly the rejected SHA256 `5313d08521d63db0be0357e88e9caaac5100828dca866ad8a9ecaa61764e75fb`; verified before editing and privately preserved as `rejected-delta_e.rs`.
- Final independent review SHA256 `852a69f9afc93ab67f60aec14fe22c55bf799ebc5ff4cc51d310cc5297c28e65`; final comparison contract SHA256 `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880`, both verified from lead docs. Contract §§5–7 and review F1/F2 govern this slice.
- Original `stats-guards` source still hashes to rejected5313d085; no original source/receipt was modified. Owned repository writes are only `src-tauri/src/dcp/delta_e.rs` and this new receipt. No validation/colour/renderer/CLI/dependency/lock/device/Git-index/commit/push mutations.
- New private evidence root `/private/tmp/rapidraw-h2b1-revision-20261015`, mode0700. Host macOS26.5.2 arm64, Python3.9.6, Rust/Cargo1.98.1. Library tests use the explicitly granted desktop target. **Cache was explicitly released to lead after test/Clippy completion and binary preservation; no build/test remains running.** No later H2 slice started.

## F1 correction: exact bounded sum, one final rounding

The prior running recurrence was insufficient at a strict mixed-valued boundary. It is replaced by a fixed, checked integer accumulator for finite nonnegative binary64 inputs. This is contained entirely in the statistics file; it does not alter colour science or add dependencies.

Every nonnegative f64 is an integer multiple of `2^-1074`. For normal values, its integer is its53-bit significand shifted by `(biased_exponent-1)`; subnormals use the fraction bits with zero shift. With at most80,000,000 samples (<2^27) and each value<2^1024, the sum is<2^1051, or<2^2125 of those integer units. **34u64 limbs provide2176bits**, so each accumulator occupies272 stack bytes with no per-sample allocation. Shifted additions/carry propagation are checked. Weighted additions for F2 multiply at most53significand bits by a<2^27 count, safely within u128 and the same total-sum bound.

Division by the positive sample count uses exact u128 limb division. Its remainder is always<count<2^27, so each `(remainder<<64)|limb` is<2^91. The integer quotient and exact remainder retain the complete rational mean. Conversion takes the top53significand bits plus guard/sticky/remainder information and rounds **once, nearest with ties to even**. The subnormal region uses exact `2*remainder` vs divisor; carry into the minimum normal and maximum-exponent overflow are checked explicitly. No floating sum/recurrence, threshold clamp, clipping, epsilon or scaled-underflow path is used. Integer addition makes input order irrelevant.

The returned mean is the correctly rounded binary64 value of the exact average of the supplied binary64 errors. Strict thresholds still compare that f64 metric; no infinite-precision colour/decimal-threshold claim is made. The finite [MAX,MAX] mean stays MAX rather than overflowing a floating sum. Private helper misuse producing an unrepresentable quotient returns an error.

## F2 correction: correctly rounded exact feasibility bounds

For N included samples, nearest rank k=ceil(.95N), p95=P and maximum=M, the reviewer-derived necessary real bounds are:

```text
L = ((N-k)*P + M)/N
U = (k*P + (N-k)*M)/N
L <= exact mean <= U
```

Lower construction uses k-1 zeros, N-k copies of P and one M; upper uses k copies of P and N-k copies of M. Count subtraction is checked, existing total/exclusion accounting stays checked, and the cap preserves exact usize→f64 rank arithmetic. The same integer accumulator computes these weighted sums and rounds each bound correctly once. Round-to-nearest is monotone: for a feasible mean, RN(L)<=RN(mean)<=RN(U). Comparing the stored mean to those rounded bounds therefore accounts for legitimate representation rounding, including subnormal means that round to zero, **without an arbitrary validity epsilon**. Bounds are validity gates only; CPU2/3/5 and GPU0.1/0.5 acceptance expressions remain unchanged and strict.

Computing valid stats([0,0.4]) gives mean0.2 and GPU `Ok(false)`. Changing only that public record's mean to0 now yields `Err` from both threshold APIs. Bounds are necessary structural conditions, not provenance attestation; a plausible fabricated record within them cannot be authenticated without source samples. No claim to reconstruct all original pixels is made.

## Independent oracle and regressions

Standalone private `oracle.py` imports only Python stdlib `fractions`, `struct`, `math`, `json`, `itertools`, `pathlib`. It imports no product function. `Fraction.from_float` converts every input bit pattern to an exact rational; sums/division are exact integers, then Python's rational→float conversion supplies the independently rounded expected binary64 bits. It also computes feasibility limits as exact weighted rational expressions. JSON retains exact numerators/denominators or fractions as well as bit-pattern goldens.

The script produced **80mean cases and20bound cases**; `oracle-rust.txt` is only a generated data transfer. The fixture bit patterns and expected booleans are embedded in the owned Rust test module, so tests need no private path/file. Identical input runs are compressed, preserving multiplicity. Expected values are never obtained by calling the new accumulator. Script SHA256 `e64cdb98938b072500ff384c5751a45900d3c91ece191ed0b0491d2ef6d76de0`; JSON SHA256 `05b0eb2e2ebe108a0b1f5ce8c7d5da147c4d154df54e31c040bb3871c91a9f3d`; generated Rust fixture text SHA256 `02934629b0869d924ba15d42dc376d20dd5f552cb8b0f095284be3241a5bad6d`.

New tests:

- Exact `[2.5×4,1.5×4]` and reverse each return mean2 / CPU `Ok(false)`.
- The F2 mutated [0,0.4] record errors in both APIs; its valid original remains GPU `Ok(false)`.
- All80 independently rounded means match **bit-for-bit**, with strict CPU/GPU results checked too. Includes permutations/mixed magnitudes, zeros, singletons/constants through257elements, MAX, maximum/subnormal mixes, subnormal half/even ties, the normal/subnormal transition, exact/adjacent mixed mean limits and adjacent p95/max limits.
- All20 weighted feasibility bounds match the Fraction oracle bit-for-bit for N1/19/20/21/80M and ordinary/zero-rank/subnormal/huge values. Public records at both rounded bounds are accepted as valid; the immediately adjacent value outside each bound is rejected. The80M cases are weighted scalar plans, not huge allocations.
- Invalid exact-sum terms/counts and an unrepresentable quotient fail; [MAX,MAX] remains representable. All23 prior tests/guards remain and pass, including exclusions/masks/nonfinite/negativity, rank edges, forged accounting and bounded reserve injection.

## Preservation, commands and actual results

`preservation-check.json` confirms the full colour/conversion prefix, D50, all34 Sharma array values (span SHA256 `e68fb52bda5df455cd964ed73525e85d93dacd4837946b9b6b4fe181f68f2d11`), strict threshold expressions and existing allocation-guard block are byte-identical to rejected input. All34 Sharma errors remain<1e-4; observed max0.000049498977271467126 at pair23. No changes to the80M/640MB sample-copy cap, fallible reserve/capacity checks, in-place unstable sort, mask semantics or public API signatures. Call-site search still finds only in-file tests and the unchanged validation re-export; no external compile break.

From snapshot `src-tauri`, with disk reserve checked>=6GiB before each Cargo command:

```sh
export CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false
export CARGO_INCREMENTAL=0
cargo test --locked --lib dcp::delta_e -- --nocapture
cargo clippy --locked --lib -- -D warnings
rustfmt --edition 2024 --check src/dcp/delta_e.rs
```

Owned file was formatted with `rustfmt --edition 2024 src/dcp/delta_e.rs` before tests. Format check and `git diff --check` exit0. Test **exit0:28passed /0failed /0ignored /152filtered**,98.94seconds including compile. Strict library Clippy **exit0**,23.52seconds; this is `--lib`, not all-target/full-product lint. Exact cwd/argv/env/UTC/exit/disk are in `cargo-test-receipt.json`/`clippy-receipt.json`. Available disk before/after test39422152704/38263480320bytes; before/after Clippy38266736640/38266818560bytes, all above6GiB. No ignored/GPU/Android/full-suite test, dependency resolution, build sweep or cargo clean.

The normal build script downloaded/verified its ignored ONNX runtime resource in this new snapshot; Clippy reused it. Generated artifacts retained; no tracked resource/lock change. No new numerical/test failure occurred during this revision.

## Frozen outputs and limitations

- Revised source SHA256 `6d58fe2ce868944234e6508b287b9f4c6837d10ede5b327e0932f735522d1d1b`; private `revised-delta_e.rs` contains identical bytes.
- Minimal correction from rejected input: `rejected-to-revision.patch`, SHA256 `c25e969789dff8fe64f5712ddef588c496b49ce240e673dffa7d02a920e73d75`. Full delta against current snapshot HEAD: `base-to-revision.patch`, SHA256 `226e33ed975cd6259c16cdc3899767b8e953dafb8b4327bb4549d26c88b1b625`. Original rejected file/receipt remain preserved separately.
- Preserved test binary `/private/tmp/rapidraw-h2b1-revision-20261015/delta-e-tests`,29,385,632bytes, SHA256 `12281dde74a0e62cda295bb22effc4390a3e5756fcb321a7401a5b99bcd38807`. Copied and hash-verified before cache release; this is a unit-test executable, not the comparator/product binary.
- Private `cargo-test.log`, `clippy.log`, format/input/preservation receipts and oracle artifacts are hash-listed in `evidence-manifest.json`. This document's hash/diff is supplied in `report-receipt.json` and Team delivery to avoid a self-hash cycle.

**Pass — worker evidence:** F1/F2 regressions, exact-oracle fixtures, preserved prior tests, Sharma, format/diff checks and strict library Clippy. **Fail — historical rejected artifact:** both independently identified false passes remain recorded; that artifact is not relabelled accepted. **Not exercised:** independent re-review/import, integrated/full-suite/all-target checks, actual OOM at scale, bulk performance/RSS, comparator CLI/ICC/Adobe/RAW/cross-route/GPU/device acceptance. **User-reported:** no new user evidence adopted. Compare remains unavailable and all original acceptance gates remain open.

The accumulator's safety/correctness depends on its documented finite nonnegative binary64 and80M sample/weight bounds. Extending that cap or supporting signed sums requires a separately reviewed proof/change; this slice does neither. Necessary feasibility bounds do not establish measurement custody. No product-fidelity or performance claim follows from these tests.

Next consumer: lead freezes these hashes for independent re-review before any import. Worker stops at this receipt and released cache; no H2b2 or broader architecture work is authorized by completion.
