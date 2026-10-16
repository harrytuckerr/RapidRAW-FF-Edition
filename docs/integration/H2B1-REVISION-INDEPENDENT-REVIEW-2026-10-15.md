# H2b1 revision independent review — 15 October 2026

**Verdict: Accept the frozen revised statistics source.** Both prior false-Pass findings are repaired. No blocking arithmetic, feasibility-validation or allocation defect found within the finite nonnegative binary64 /80M-sample contract. One nonblocking bit-width comment correction is noted below. This accepts an offline statistics helper, not a colour comparator, production performance or Adobe/device evidence.

Reviewer: Codex CLI - Astra / actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/stats-revision`, HEAD `8a4f02bcec8a3edb567979462cbf022adde6f8b9`. Only this report is written in `raf-review/docs/integration/`. No production code import, Rust/test-binary execution, build, shared-cache/device/Git mutation or private photograph access. Small independent stdin arithmetic and source/data verification probes ran; their exact scope is below.

## Verified identity and preservation

Full worker receipt, implementation and test logic inspected; all embedded fixture rows were parsed and compared with independent data. Rejected review `852a69f9afc93ab67f60aec14fe22c55bf799ebc5ff4cc51d310cc5297c28e65` and final contract `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880` were rehashed and remain governing inputs.

| Input | SHA-256 independently recomputed |
|---|---|
| Revised `src-tauri/src/dcp/delta_e.rs` | `6d58fe2ce868944234e6508b287b9f4c6837d10ede5b327e0932f735522d1d1b` |
| `docs/integration/H2B1-REVISION-RECEIPT-2026-10-15.md` | `e53ade8142599a4366ef727aa483c17ceafc4fbb1b85a4c950f6e545b6952251` |
| Private `rejected-to-revision.patch` | `c25e969789dff8fe64f5712ddef588c496b49ce240e673dffa7d02a920e73d75` |
| Private `base-to-revision.patch` | `226e33ed975cd6259c16cdc3899767b8e953dafb8b4327bb4549d26c88b1b625` |
| Private `oracle.py` | `e64cdb98938b072500ff384c5751a45900d3c91ece191ed0b0491d2ef6d76de0` |
| Private `oracle-fixtures.json` | `05b0eb2e2ebe108a0b1f5ce8c7d5da147c4d154df54e31c040bb3871c91a9f3d` |
| Preserved `delta-e-tests` | `12281dde74a0e62cda295bb22effc4390a3e5756fcb321a7401a5b99bcd38807` |

Private prefix: `/private/tmp/rapidraw-h2b1-revision-20261015/`. Current git diff equals the frozen base-to-revision patch. The entire colour/conversion prefix preceding statistics, all34 Sharma values, and the allocation-guard block are byte-identical to rejected input. Extracted Sharma span again hashes `e68fb52bda5df455cd964ed73525e85d93dacd4837946b9b6b4fe181f68f2d11`; `<1e-4` assertions and strict CPU/GPU expressions remain unchanged. Source scope remains one file plus worker receipt. Binary hash verification is identity evidence, not an independent source-rebuild attestation.

## Exact accumulator assessment

Locations are in the revised `stats-revision/src-tauri/src/dcp/delta_e.rs`.

| Area | Reasoning and result |
|---|---|
| Input decomposition,352–371 | Finite/nonnegative predicate precedes bit decoding. A normal biased exponent e uses significand `(2^52+fraction)` shifted e−1 in units2^-1074; subnormal fraction uses shift0. Negative nonzero/NaN/Inf reject. Negative zero is numerically nonnegative and contributes integer0, yielding canonical +0 mean; no negative value is silently converted positive. Weight<=80M is checked. |
| Capacity,315–320 | Each finite value is <2^1024; integer units are <2^2098. N<2^27 gives sum<2^2125.34×64=2176 bits is sufficient. Actual largest permitted weighted term has bit length2125; there are51 spare bits. Callers enforce total sample/weight counts: the sample loop has N terms, lower feasibility weights total N−k+1<=N and upper weights total N. The private accumulator need not authenticate arbitrary future call sequences; any cap/caller expansion requires a new proof. |
| Split addition/carry,327–371 | A significand×weight product is <2^80 and fits u128. Low/high u64 halves shifted by s and s+64 reconstruct the exact product. Offsets are modulo64; the right shift by64 is guarded out. `overflowing_add` carries through checked array access. In these bounded callers no contribution can escape the34 limbs. Partial state after a returned error is never accepted. No floating sum or per-sample heap allocation. |
| Division,377–392 | Positive divisor<=80M. Long division maintains remainder<divisor; `(remainder<<64)|word` is <2^91 and fits u128. The resulting per-limb quotient is <2^64. Exact remainder survives the division, rather than being rounded away before conversion. |
| Leading bits,393–404 | Highest set bit determines the required53-bit significand shift; cross-word extraction and mask retain the correct top bits. All-zero quotient is deliberately handled with shift0, retaining a possibly nonzero division remainder. Maximum index remains inside34 limbs, including checked out-of-domain overflow paths. |
| Rounding,405–432 | For shift0, compare twice the exact remainder with divisor, breaking ties by significand parity. For larger shifts, the guard bit identifies the half-ULP; lower quotient bits **or nonzero division remainder** are sticky. Thus a true tie uses even parity and any above-half residual rounds up. No lost small term can turn an above-half case into a tie. |
| Normal/subnormal/overflow | Below bit52, rounded integer units are direct subnormal encoding; carry to bit52 produces minimum normal. Otherwise biased exponent=shift+1; a53-bit carry shifts the significand and increments exponent. Exponent>=2047 errors rather than producing infinity. A valid average cannot exceed its finite maximum, so [MAX,MAX] remains MAX. Final finiteness check retained. |

The integer sum is associative/order-independent. Rounding happens once at the final rational average. The stored metric is correctly rounded **binary64**, and acceptance compares that metric to the unchanged binary64 limits; this is not an infinite-precision decimal-threshold claim.

**Prior F1 repaired:** `compute_stats:571–575` uses the exact sum. The exact `[2.5×4,1.5×4]` mean is2 in both orders, so strict CPU mean<2 fails. Explicit regression1964–1972 executes this, and the Fraction fixtures also include permutations/adjacent means. No threshold epsilon, clipping, recurrence or renderer change was introduced.

## Feasibility and validity assessment

`validate_stats:449–519` retains nonempty, capacity, checked count/exclusion sums, finite/nonnegative metrics, mean/max and small-sample rank checks. Rank remains nearest-rank `ceil(.95N)` at441–446. The80M cap keeps N exactly representable; multiplication error is far below the0.05 spacing of noninteger exact ranks and rounds the integer cases correctly under this bound. Required1/19/20/21 cases remain tested.

For sorted nonnegative samples, rank k with value P and maximum M gives minimum sum `(N−k)P+M` and maximum sum `kP+(N−k)M`. Both constructions are attainable with the fixed P/M endpoints. Weighted calls506–511 use those counts, and the same exact arithmetic gives correctly rounded lower/upper means. Round-to-nearest/even is monotone, so a correctly rounded feasible mean cannot fall outside the correctly rounded bounds; no arbitrary validity epsilon is required. Subnormal positive averages that round to0 are therefore accepted when feasible. Public records outside those rounded bounds reject.

**Prior F2 repaired:** the valid `[0,0.4]` record has mean0.2 and GPU `Ok(false)`; mutating mean to0 now fails both validators (1976–1981). Necessary feasibility bounds do not authenticate a plausible record's provenance, nor claim to uniquely reconstruct samples. That limitation is accurate and is not the previous impossible-record loophole.

The three Result APIs still distinguish invalid Err from measured Ok(false). CPU mean<2,p95<3,max<5 and GPU mean<0.1,max<0.5 are untouched at597–605. Excluded invalid data is checked before masking. The640MB/80M helper-copy cap, checked multiplication/actual-capacity validation, fallible exact reserve and in-place sort are preserved. No valid rounded-bound rejection or remaining order-dependent mean acceptance was found in the inspected domain.

## Independent probes and fixture provenance

**Executed by this reviewer, Python stdin only:**

1. Recomputed all80 JSON mean cases using `Fraction.from_float`, exact rational sum/count and the standard rational→float conversion. Verified exact numerator/denominator, expected mean bits and CPU/GPU booleans. Recomputed all20 bound cases with exact integer rank `(95*N+99)//100` and exact weighted rational bounds; verified their fractions and bits.
2. Parsed all80 embedded Rust mean rows, expanded run-length multiplicities and compared names/inputs/expected bits/booleans to the JSON. Parsed all20 embedded bound rows and compared every literal to JSON. Embedded arrays also match the separately generated Rust fixture text after removal of formatting/comments. No product converter generated any expectation.
3. Ran4096 deterministic binary64 decomposition/weighted-split arithmetic probes (seed20261015), checking the integer-unit identity against Fraction and the low/high split against unbounded integer multiplication. Verified80M×MAX has2125-bit length and fits2176 bits. No80M-array allocation was made.
4. Ran10000 deterministic guard/sticky/remainder predicate probes (seed314159), including zero, subnormal/normal boundaries, word boundaries and near-maximum exponent. Compared the rounding-decision algebra with exact Fraction distance from half an ULP and parity. This validates the arithmetic rule independently; it is **not execution of the Rust implementation**.
5. Verified frozen diff, preserved source blocks/array, fixture counts and hashes. `git diff --check` passed.

The original `oracle.py` was read, not executed because it writes artifacts. It uses standard-library Fraction/struct/math/JSON and no product imports. Its Python rational→float expectation path is distinct from the Rust limb algorithm. The fixtures include subnormal half/even ties, transition to minimum normal, MAX and mixed exponent samples, positive/zero constants through257 samples, permutations, strict threshold neighbors and weighted80M feasibility records. The latter test scalar arithmetic at that count, not actual80M throughput or allocation.

## Rust evidence inspected; maintainability and limits

**No Rust/test binary run by this reviewer.** Worker `cargo test --locked --lib dcp::delta_e -- --nocapture` log and lead's rerun of the preserved binary both record **28 passed,0failed,0ignored,152filtered**. These are28 test functions, not100 or128 tests:80 mean fixture rows are exercised within one test and20 bound rows within another. Five test functions were added to the prior23. Boundary feasibility tests2001–2031 check exact bits, valid rounded endpoints and adjacent rejected endpoints. Prior resource/nonfinite/exclusion cases remain. Strict worker Clippy is `--lib -- -D warnings`, not all-target/full-product lint.

Verified logs:

- Worker `cargo-test.log`: `fbe67950eb78dc806365ca1034651bcef109826f66ccfca75ab39465b5dd7808`
- Lead `lead-independent-tests.log`: `b5c7cefc36bae5050b3ba0ddf7ac26a93b115a424c45c7d5bbfd08de40012800`
- `clippy.log`: `c272a89dd1921022f4495f2840593efb428d83439b5912966e9da7a9a979fe8d`

The exact arithmetic increases complexity, but remains a private fixed-size helper with bounded positive inputs, checked operations, documented bit derivation and independent bit-pattern fixtures. This is defensible within this offline statistics slice and avoids an external dependency or a guessed epsilon. It must not be generalized to signed sums, larger caps or production per-pixel rendering without separate review. Bulk runtime/RSS is still unmeasured; no performance acceptance follows. Necessary public-record bounds remain distinct from source/custody verification.

**P3 comment precision, nonblocking:** `delta_e.rs:316` says the sum needs “fewer than2125 bits”. The correct statement is **at most2125 bits**, since80M×MAX actually has2125-bit length. The numeric inequality sum<2^2125 and the34-limb implementation are correct. A future owned comment correction is sufficient; no algorithm/tolerance change is required and acceptance is not conditional on changing this hash.

## Disposition and next consumer

**Accept — revised frozen source6d58fe2c. Pass — independent static arithmetic/ownership/domain review and executed oracle/proof probes. Pass — inspected worker/lead28-test logs, unchanged colour/D50/Sharma `<1e-4`, strict expressions and allocation guards. Fail — historical5313d085 artifact remains rejected, not relabelled. Not exercised by reviewer — Rust/builds, real OOM/80M bulk performance, combined-source/full-suite/device/Adobe/ICC/CLI/route acceptance. User-reported — no new user evidence adopted.**

Read-only commands included `git rev-parse/status/diff/show`, `nl`/`rg`/`cat`/`tail`, `shasum`, and stdin Python with standard-library Fraction,struct,json,re,random,subprocess,hashlib/math. No shared cache was touched and no persistent probe output was written. This report is the only new artifact; its SHA is delivered via Team message.

Next consumer: lead imports the exact accepted one-file revision under its integration process and runs combined checks when separately assigned. Correcting the P3 wording is a nonfunctional follow-up under ownership. This review authorizes no subsequent H2/ICC work and closes no original product fidelity/device gate.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `f20ebf94772893813ce86334335d646d2f9f9d974aea67fe8891096959e0bce7` (13548 bytes).
