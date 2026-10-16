# H2b1 independent statistics review — 15 October 2026

**Verdict: Revise before import.** Two concrete false-Pass cases remain: the new running mean can move an exact threshold-equality sample below the CPU limit, and the public-record checks accept an impossible mean/max/count combination that passes GPU parity. Both corrections fit the assigned statistics file; no colour formula, ICC, renderer, CLI or tolerance change is requested.

Reviewer: Codex CLI - Astra, actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/stats-guards`, detached base `8877d7eaa14442814912a199c8de9abdfba5c8b9`. Only this report is written under `raf-review/docs/integration/`. No source edit, Cargo/build/test-binary execution, cache mutation, device operation or private photograph access. Independent small Python arithmetic/source-preservation probes ran from stdin without importing the production module or writing files.

## Frozen inputs verified

| Input | SHA-256 independently recomputed |
|---|---|
| `stats-guards/src-tauri/src/dcp/delta_e.rs` | `5313d08521d63db0be0357e88e9caaac5100828dca866ad8a9ecaa61764e75fb` |
| `stats-guards/docs/integration/H2B1-STATISTICS-RECEIPT-2026-10-15.md` | `228d4ac511eaf21010e5209ce31d9dec22a0e22a6aae90157fcf8d80e9722a34` |
| `target/h2b1-statistics-20261015/delta_e.patch` | `200ccceed399ae86692abbecf506f3378097d78d1969085701b00d01cdf150df` |
| Same evidence directory, `cargo-test.log` | `f8c07d9d5968faabd249b925087a95abdacdd41d4a0be439fffec0186240f634` |
| Lead `docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md` | `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880` |

Evidence target prefix is `/private/tmp/rapidraw-integration-20261015/target/`; only the assigned log/JSON/patch files there were read. Current source diff byte-equals the preserved patch. Only delta_e.rs is tracked-modified in this snapshot. Contract §§5–7 govern this review.

## F1 — P1: exact mean equality becomes a false CPU Pass

**Where:** modified `src-tauri/src/dcp/delta_e.rs:412–420`, especially416; acceptance at445–447. Tests at833–841 and902–918 do not exercise a mixed-valued mean boundary.

**Reproducer:** all masks false; samples `[2.5,2.5,2.5,2.5,1.5,1.5,1.5,1.5]`. Every input and the true mean are exactly representable as f64. Sum is16 and mean is **exactly2**; nearest-rank p95/max are2.5. Strict mean<2 must fail. The source's recurrence produces `1.9999999999999998`, which passes `validate_stats` and makes `check_thresholds` return `Ok(true)`.

An independent Python IEEE-double recurrence with `Fraction` as an exact oracle executed and printed:

```text
high_first mean=1.9999999999999998 exact_mean_float=2.0 p95=2.5 max=2.5 cpu=True
low_first  mean=2.0                exact_mean_float=2.0 p95=2.5 max=2.5 cpu=False
```

The low-first case is the same multiset in reverse order. This is an acceptance-changing regression, not simply an irrelevant last-bit difference, unknown source custody or an invalid sample. The old direct sum is exact for this particular input. This reviewer did not execute the Rust helper; the source mapping is direct, and the worker should add/run the Rust regression on its separately assigned turn.

**Smallest correction:** replace only the uncompensated statistics-mean accumulation with an overflow-safe, sufficiently accurate accumulation strategy; retain finite huge-value support and the strict comparisons. A compensated sum in the ordinary safe range plus a justified overflow-safe scaled path is a bounded candidate. Do not return to an overflowing plain sum for all inputs, add an acceptance epsilon, clamp the result to a threshold or modify CIEDE2000. Independently verify the chosen method rather than treating this suggestion as implementation acceptance.

Required regression: both orders above produce mean2 and `Ok(false)`; retain near-boundary positive cases, zero/singleton/constant and `[f64::MAX,f64::MAX]` tests. Include mixed finite magnitudes and permutations against a separate exact/high-precision mean oracle. No tolerance widening. Do not repair F2 by adding strict derived bounds to the current inaccurate mean and thereby converting valid inputs into spurious errors.

## F2 — P2: impossible public statistics pass GPU parity

**Where:** modified `delta_e.rs:315–360`, especially351–354, then451–453.

Construct a public record with `count=2`, `total=2`, every exclusion counter0, `mean=0`, `p95=0.4`, `max=0.4`. It passes every current accounting/finite/nonnegative/metric test: p95 equals max for count<=19, mean<=max, and count is not1. Both threshold APIs return `Ok(true)`; GPU is the sharp counterexample because any two nonnegative samples whose maximum is0.4 have mean **at least0.2**, which necessarily fails mean<0.1.

Independent arithmetic probe executed: `current predicates=True, GPU result=True, necessary mean>=0.2`. An equivalent worker regression is to compute valid stats for `[0.0,0.4]`, replace only `.mean=0.0`, and require **Err from both threshold APIs**. The valid original must stay `Ok(false)` for GPU. This is a provably impossible record, distinguishable without original pixels; the receipt's legitimate custody caveat does not excuse it.

**Smallest correction:** strengthen public-record consistency checks with the necessary mean/count/max and nearest-rank feasibility bounds (or an equally effective validated-construction boundary within this file). At minimum this example must be rejected. For N samples, rank k=ceil(.95N), p95=P and max=M, nonnegative real samples imply:

```text
((N-k)*P + M) / N <= mean <= (k*P + (N-k)*M) / N
```

The simpler lower bound `M/N` already catches this example. Implement bounds with checked counts and overflow-safe arithmetic; account for f64 representation in **validity** checking so legitimate computed records are not rejected due to arithmetic rounding. This does not authorize epsilon changes to CPU/GPU acceptance. Test these grossly impossible records and valid boundary records after fixing F1. A plausible fabricated record remains outside provenance attestation; no demand to reconstruct missing pixels is made.

## What passes static review

- All three public APIs now return `Result`; `Err` differs from measured `Ok(false)`. Empty/all-excluded primary samples cannot become the old numeric-zero shortcut (`371–400`).
- Both masks must have exactly the sample length. All samples are checked for NaN/+Inf/−Inf and negativity before exclusion. An excluded invalid value fails (`376–397`).
- Four disjoint buckets partition the input; the exclusion union counts overlap once. Inclusive legacy counters are reconstructed explicitly. `validate_stats` checks sums using checked-add and rejects overflow/inconsistent public count records (`319–341`). Loop counters cannot overflow under the80M cap.
- Checked multiplication and80M count/640MB sample-copy caps precede allocation. `try_reserve_exact` is fallible; actual capacity and insufficient capacity are checked. Included push count cannot exceed the validated reservation; sort is in-place unstable and no second scratch vector is introduced (`286–313,402–421`). Actual allocator OOM was not attempted; bounded injected tests cover the intended error branches.
- p95 remains nearest-rank, with checked subtraction/range before indexing. N=1/19/20/21 yields ranks1/19/19/20; cap keeps usize→f64 exact. CPU limits remain strict mean<2,p95<3,max<5; GPU mean<0.1,max<0.5. The expressions are unchanged in meaning; F1 corrupts their input.
- No production callers exist: current call-site search finds the in-file tests and unchanged validation re-export. The logged library compilation supports the re-export compatibility at this base.
- The entire source prefix before statistics, including CIEDE2000, XYZ/Lab/ProPhoto conversions and D50, is byte-identical to HEAD. The entire Sharma array is byte-identical, extracted-span SHA-256 `e68fb52bda5df455cd964ed73525e85d93dacd4837946b9b6b4fe181f68f2d11`. The34-pair assertion remains `<1e-4`; only the stale subset comment changed. `git diff --check` passes.

## Tests and receipt assessment

**Worker log inspected, not rerun:** `cargo test --locked --lib dcp::delta_e -- --nocapture`, exit0,23 passed/0failed/0ignored/138filtered. Ten baseline tests plus13 new tests agrees with source. The34 Sharma pairs logged maximum error0.000049498977271467126 on pair23. Existing `cargo-test-receipt.json` records74.84seconds, correct snapshot cwd and isolated target/environment. Preservation JSON agrees with this reviewer's independent byte comparisons. Build-script generated ONNX artifact and scope limits are disclosed. No misleading skip-shaped Pass was found in these tests.

Coverage is substantial for empty sets, mask sizes, overlap, invalid excluded samples, strict scalar thresholds and resource branches. It does **not** establish the receipt's broad fail-closed claim yet: mean-threshold tests use a singleton; constant/huge means miss F1, and forged-record tests cover mean>max, singleton/short-rank inconsistencies but miss F2. Preserve the successful tests; add the two concrete regressions and numerical-validity boundary coverage. Update the receipt after repair rather than changing expected outcomes to match the flawed accumulation.

Independent reviewer executions: small stdin Python using `math`, `fractions.Fraction`, `hashlib`, `pathlib`, `subprocess`, `re` only. No production import, Rust compiler, Cargo, shared test binary or giant allocation. The following core reproduces F1 without repository dependencies:

```python
from fractions import Fraction
xs = [2.5]*4 + [1.5]*4
mean = 0.0
for n, value in enumerate(xs, 1):
    mean += (value-mean)/n
assert sum(map(Fraction, xs))/len(xs) == 2
assert mean == 1.9999999999999998  # source recurrence
assert mean < 2                 # incorrect threshold Pass
```

A second probe evaluated the public-record predicates above and the necessary max/count bound. Additional adjacent-representable-value probes were exploratory; only the exact representable F1 case and the impossible F2 record are relied on for the findings. Other read-only commands: `git rev-parse HEAD`, `git status --short`, `git diff`, `git show HEAD:...`, `git diff --check`, `nl`, `rg`, `cat`, `tail`, `shasum -a 256` on assigned files. No additional persistent script/log artifact was written.

## Disposition

**Fail:** F1 exact-boundary false Pass (P1), F2 impossible-record false Pass (P2), supported by source and executed independent arithmetic. **Pass:** frozen hashes/diff identity, formula/conversion/white/Sharma preservation, specified count/mask/nonfinite/allocation guards under static inspection, existing23-test worker log. **Not exercised by reviewer:** Rust tests/builds, actual OOM, real comparator/Adobe/RAW/GPU/device acceptance. **User-reported:** no new user evidence adopted.

Next consumer: lead assigns a one-file repair to the implementation owner, preserves this rejected artifact, then requests independent review of revised source/hash and a refreshed focused-test receipt. Required worker command remains `cargo test --locked --lib dcp::delta_e -- --nocapture` under lead-controlled cache scheduling. Do not import the current source as accepted H2b1 or start later colour slices on its acceptance claim. No source modifications made by this review. Final report SHA-256 is sent in Team delivery.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `852a69f9afc93ab67f60aec14fe22c55bf799ebc5ff4cc51d310cc5297c28e65` (11500 bytes).
