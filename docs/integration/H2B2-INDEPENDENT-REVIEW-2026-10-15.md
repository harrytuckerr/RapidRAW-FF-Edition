# H2b2 independent implementation review — 15 October 2026

**Verdict: Accept.** The private candidate conversion matches the contracted path, keeps finite negative and above-one values, fails closed on non-finite stages and required inverses, and checks all 108 frozen components and 43 fault cells at strict absolute error `< 1e-6` in the correct input domain. No conversion, custody, tolerance, or false-range defect found. This is not acceptance of image comparison, ICC, Adobe, GPU, or a device. Compare remains unavailable.

Reviewer: Grok Build / `grok-4.7`, slot `01a13e02-3c22-7ad0-9ed3-8bab4bc7d945`. This note is written only in `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/harness-review`, whose actual HEAD is still `4c1e965a53de9c835df920b2e3d69c2c8a864708`, tree `9f36c891f35d9310a7977bda7d851178ad92039d`. That snapshot does not contain the candidate. The reviewed tree is the sibling `candidate-lab` worktree, producer base `6f1aa7abb5eeff7a1e072f1232b39a3ae3574cc8`, tree `be4f79c0644a315aed604c1d090cbe9f8e0f70bb`, with an uncommitted append. Environment: macOS 26.5.2 / 25F84, arm64. No Cargo, cache write, device, source edit, lock edit, or Git mutation.

The oracle note `3019ff22…` and Sol's oracle review `7d64cd84…` were frozen inputs for expected-value identity only. This review read the Rust and rechecked the fixture bytes. It does not adopt either document as the implementation verdict.

## 1. Identity

| Object | SHA-256 | Result |
|---|---|---|
| `candidate-lab` `src-tauri/src/dcp/validation.rs` | `70515957ee552bcd7cfb9e7b0ab40933df8027348bf0e333db77718eb91e25e1` | matches the assignment |
| Base file at `6f1aa7ab` / byte prefix of the reviewed file | `1ec1913bd9575eca24b767925df00d0a27b0298a9bbf7ab1116793983da660cc` | prefix match; 1324 lines unchanged |
| Added lines | 508 insertions, 0 deletions | append after the old `tests` module brace |
| `bench/dcp_validation/known-answers.json` | `1b0bae0896865a388cfd56e21c967517771a762a052042783a96ac1efa019b25` | matches |
| Receipt | `2ef30772637d1903a81f413739755186e1c890aec9b1a8e91c05a8bc96f0bc49` | matches |
| Contract | `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880` | matches |
| Oracle design | `3019ff22a4275b6ed18d0bbd434ea7c8482eac9c89fbb93d51d81f15569fa2b9` | matches |
| Sol oracle review | `7d64cd842c6deb2349bb7e59a407e5759fd5b0c4a323e338b971b17fdaf6fbe8` | matches |
| `delta_e.rs` and `Cargo.lock` versus `6f1aa7ab` | `70f57e66d3b4d95f8038cc6c698a39203f1eb41f8d8ccf993408f4c24dc0e296`, `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` | unchanged |
| `artifacts.json` | `539a24390d9d40154ff4ebb96b27bd7e07b6cbc1450740d78748506f0b20a904` | matches |
| Preserved final test binary | `2f184eab17f8b7d091b0c4284ba7b1ab8eacdea6d21f6c8f137bcb9964f1c464` | matches; rerun below |
| `validation.final.patch` | `9a2ae12043aea9cdecc537d2e842cdd0f92e5062585e6cef2a9a956b176716c9` | byte-identical to `git diff` of `validation.rs` |

`candidate-lab` status is the modified `validation.rs`, the new fixture, and the new receipt. `src-tauri/resources/libonnxruntime.dylib` is gitignored (`.gitignore:31`). No schema, CLI, lock, or production renderer file is in the diff.

The only difference between the preserved initial patch and the final patch is the determinant finiteness check in `inverse` and the `identity * f64::MAX` assertion. Both preserved logs report 25 passed / 3 ignored before and after that line.

## 2. Static review

Contract §7 H2b2 asks for private conversion and tests in `validation.rs`, a new `known-answers.json`, independent sRGB matrix → Bradford → existing Lab, negative/>1 channels, and omitted/reversed/doubled adaptation, with frozen f64 values at `< 1e-6`. No RAW, ICC, TIFF, or schema. Existing GPU and ignored tests stay. Stop for review.

`#[cfg(test)] mod candidate_lab` is private and unwired. That matches this slice: compare is still a later assignment, and lifting the helper out of test scope would be that later change, not a defect here.

Checked in source, not by trusting the receipt:

- **Matrix orientation.** nalgebra 0.35 `Matrix3::new` takes row-wise arguments and stores columns (`construction.rs` `transpose_array`). The cone literal is the contract Bradford matrix. `matrix[(row, column)]` times a column vector is the product used by `multiply`. Primary columns are `(x/y, 1, (1-x-y)/y)` for `(0.64,0.33)`, `(0.30,0.60)`, `(0.15,0.06)`. Scales are `inv(P) * W65` with `W65` from `(0.3127, 0.3290)`. `M` multiplies each primary column by its scale. There is no renderer-matrix inverse and no sRGB transfer.
- **Adaptation.** `A = inv(B) * diag((B*Wc)/(B*W65)) * B`, applied once. Destination white is `delta_e::D50_XYZ`, which is still exactly `[0.9642956764, 1.0, 0.8251046025]`. It is not retyped and it is not the full-quotient D50 or ICC `[0.9642, 1, 0.8249]`.
- **Lab.** `finish_lab` calls `delta_e::xyz_to_lab`. That function is still the piecewise `t > (6/29)^3` cube root, otherwise `t/(3*(6/29)^2)+4/29`, then `L = 116*fy-16`, `a = 500*(fx-fy)`, `b = 200*(fy-fz)`. No gamut clamp and no `[0,1]` clamp on the conversion path.
- **Finite range.** `convert` checks the input and every product, partial sum, XYZ stage, white-relative ratio, and Lab result. Finite negatives and values above 1 are returned. Non-finite RGB is `ColourError::NonFinite("RGB")` before any matrix work. Singular `try_inverse` returns `SingularMatrix` with no identity fallback. A non-finite matrix, determinant, or inverse result returns `NonFinite`. A zero source cone ratio fails as non-finite cone scaling.
- **Determinant guard.** `inverse(I * f64::MAX)` is rejected because the determinant overflows, even though the mathematical inverse entries would be tiny and finite. That input is not a required colour sample. The required finite extremes in the same module (`f64::MAX/32`, `-f64::MAX/4096`, `MIN_POSITIVE`, the smallest subnormal) are expected to succeed, and the preserved binary does succeed on them. Partial-sum checks reject only after an intermediate is already non-finite, so they do not turn a finite colour result into an error. This is the contract's overflow fail-closed rule, not a false rejection of the finite negative/>1 range.
- **Tolerance.** Comparisons use `error < 1e-6`, not `<=`. The fixture's `tolerance_abs` is the string `1e-6` on both domains. Nothing loosens it to hide the f32 gap.
- **Domains.** Analytic rows require decimal `rgb` and forbid f32 fields. Widened rows require `rgb` absent, decode 8-digit little-endian hex with `f32::from_bits` / `f64::from`, and require bitwise equality with the `rgb_exact` string. A separate test requires each widened Lab to miss the analytic Lab by more than `1e-6` and to hit the widened expected Lab.
- **Custody.** Both §6 JSON payloads of oracle `3019ff22` occur verbatim in the fixture. Parsed domains are equal. Including the newline before the closing fence, their SHA-256 values are Sol's `869c2e15…` and `14622041…`, which the fixture provenance records. Component count is 12 rows × 9 = 108. Fault cells are 5 × 7 = 35 and clip cells are 2 × 4 = 8, total 43. The six f32 hex strings decode to the six exact widened decimals. Author scratch `derive.out`, `amend.py`, and `amend.out` still match the provenance hashes.
- **Fault definitions.** Omit sends D65 XYZ straight through `xyz_to_lab` relative to Wc. Reversed builds Bradford from `D50_XYZ` back to the D65 chromaticity quotient. Doubled applies `A` to an already adapted XYZ. `M` and `A` are transposed and still multiply column vectors. Channel swap is `(B,G,R)`. RGB clip is `clamp(0, 1)` only inside the fault test. XYZ clips are `max(0)` before adaptation and before Lab. L clip clamps only `L`. The negative-blue test requires the two XYZ clips and the RGB clip to meet at black, and the L clip to keep `a*` and `b*`. The double-white test requires both XYZ clips to do nothing and the L and RGB clips to agree within `1e-6`. Those are the stated non-separabilities, not accidental holes.
- **Helper under test.** Every value row and the correct side of every witness calls `CandidateLabConverter::convert` or the same matrices and `finish_lab`. Expected numbers are `include_str!` of the fixture, not values computed by the helper.
- **Old tests.** The byte prefix preserves the GPU parity test and the ignored ACR and upstream tests. The final log still shows those three ignored and does not pass them.

## 3. Runtime

Re-executed the preserved final binary directly, with no Cargo invocation:

`validation-final-test-binary dcp::validation::candidate_lab --nocapture --test-threads=1`

Exit 0. All 8 candidate tests passed. The helper printed `12 bound rows / 108 components; maximum error 4.94770890924201e-13`. That is the same figure as the hashed final log and is far inside `1e-6`. It sits next to the publication quantum of `5e-13`; it is an f64-versus-published-literal error, not a tolerance breach.

The full `dcp::validation` result, 25 passed / 0 failed / 3 ignored / 175 filtered out, is the hashed log `3747ce9d…` and command record exit 0. This review did not re-run those other 20 tests. Clippy `--locked --all-targets -- -D warnings` was not re-run; its preserved command record exits 0 and `clippy.final.log` hashes to `ab9fa258…`. The first evidence log shows the existing `build.rs` downloaded ONNX Runtime despite `CARGO_NET_OFFLINE`. The receipt already discloses that ignored byproduct. It is not a candidate-colour defect.

## 4. Findings

No defect is raised. Severity, reproducer, and minimum fix do not apply.

**Fail:** none.

**Not exercised:** ICC parsing and PCS-D50→Wc, actual candidate images, Adobe comparison, GPU parity (`--ignored` was not used), Android, device, UI, export, and production performance. `compare` is still unimplemented. The three ignored validation tests remain ignored.

Commands were read-only `git rev-parse`, `git status`, `git diff`, `git show`, `git check-ignore`, `shasum -a 256`, a stdlib Python custody comparison, and the preserved-binary invocation above. Document SHA-256 is in the delivery message. Stop here. No downstream slice is authorized by this review.
