# H2b2 candidate linear-sRGB → Lab — 15 October 2026

**Pass — private synthetic conversion core, frozen answers, targeted tests, owned formatting, and strict all-target Clippy.** This worker delivery is ready for independent source review. It does not implement image comparison or establish rendering, ICC, Adobe, GPU, or device acceptance.

Worker: Codex CLI - Sol, slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`, actual Team roster model `gpt-6.1-sol`. Task `01a13e6d-3b6f-73a1-b1a9-a7828e4b0e13`. Snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/candidate-lab`, frozen HEAD `6f1aa7abb5eeff7a1e072f1232b39a3ae3574cc8`. Initial worktree status was clean.

## Owned changes and private API

Only these worktree files are intentionally changed:

- `src-tauri/src/dcp/validation.rs`: append one private `#[cfg(test)] mod candidate_lab`, 508 added lines, no deleted or altered existing lines.
- `bench/dcp_validation/known-answers.json`: new frozen independent data and provenance.
- This new receipt.

The appended core has private `CandidateLabConverter::new() -> Result<Self, ColourError>` and `convert(&self, [f64;3]) -> Result<CandidateLab, ColourError>`. The result retains XYZ(D65), XYZ(Wc), and Lab for staged fixture verification. It derives the sRGB matrix from the published primary chromaticities and unit D65 white, derives Bradford D65→existing `delta_e::D50_XYZ`, then **calls the existing `delta_e::xyz_to_lab`**. No renderer matrix inverse or production converter supplies an expected value.

The core retains finite negative and above-one values. Each matrix-vector product and partial sum is finite-checked, as are matrix construction outputs, cone ratios, XYZ stages, white-relative XYZ, and Lab outputs. Required inverses reject nonfinite matrices/determinants/results or singular matrices; no identity fallback. The inverse is used only on the fixed primary/cone matrices, not an ICC or caller-supplied matrix API.

Both the core and its fixture tests are confined to the same private test module because no real comparison route is wired yet. There is no new public API or lint suppression. Later wiring requires a separately owned implementation change to move the appropriate private helpers out of test scope. Existing validation tests, GPU test, ignored acceptance blockers, and their source bytes are unchanged.

Exact minimal source diff is preserved at:

`/private/tmp/rapidraw-integration-20261015/target/h2b2-candidate-lab-20261015/validation.final.patch`

SHA-256 `9a2ae12043aea9cdecc537d2e842cdd0f92e5062585e6cef2a9a956b176716c9` (19,388 bytes). It is append-only after the original closing test-module brace. The fixture is a new file, not an edit to an existing golden.

## Frozen expected-value custody

Committed inputs were read and their SHA-256 verified:

| Input document | SHA-256 |
|---|---|
| H2-COMPARISON-CONTRACT-2026-10-15.md, §3 and §7 H2b2 | `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880` |
| H2B2-KNOWN-ANSWER-DESIGN-2026-10-15.md | `3019ff22a4275b6ed18d0bbd434ea7c8482eac9c89fbb93d51d81f15569fa2b9` |
| H2B2-ORACLE-INDEPENDENT-REVIEW-2026-10-15.md | `7d64cd842c6deb2349bb7e59a407e5759fd5b0c4a323e338b971b17fdaf6fbe8` |

The fixture copies both §6 JSON fence payloads **verbatim**, preserving all 108 expected XYZ/Lab numbers, input values, domain labels, f32 hex and exact widened strings. It also copies the 35 §7 fault deltas and eight clipping deltas as decimal strings. Provenance records the frozen source/review/contract and author-script/output hashes, primary-reference URL, matrix/adaptation/Lab formulas, exact Wc, quantum `1e-12`, `ROUND_HALF_EVEN`, 80-digit author arithmetic and maximum rounding error `5e-13`. No candidate function generated or regenerated these values.

The analytic domain contains ten rows: black, white, gray18, three primaries, chroma, extended, negative_blue, and double_white. The widened-pixel domain contains chroma and extended bound to their little-endian f32 bits. Tests decode those bits and widen exactly to f64; they also check the widened inputs against the independent exact strings. Expected analytic decimals are never substituted for widened-pixel expected Lab. Both widened rows demonstrably fail comparison with the analytic-domain Lab at the unchanged strict absolute `<1e-6` tolerance.

Fault tests check the independent deltas for omitted/reversed/doubled adaptation, M/A transposes, R↔B swap and RGB clipping. Clipping tests cover negative XYZ before/after adaptation and L-only clipping, retaining the stated nonseparability: negative-blue XYZ/RGB clips all produce black; double-white L/RGB clips are indistinguishable at this tolerance. These intentional faults exist only in tests.

The small serde structures read frozen test data; they do not define an application manifest/report schema or implement any comparator.

## Verification

Final source validation:

| Command/check | Observed result |
|---|---|
| `cargo test --locked --lib dcp::validation -- --nocapture` | Exit 0; **25 passed, 0 failed, 3 ignored, 175 filtered out** |
| New `candidate_lab` tests | **8 passed**, included in the 25 |
| Frozen staged values | 12 bound rows / 108 components; maximum absolute component error `4.94770890924201e-13` |
| `cargo clippy --locked --all-targets -- -D warnings` | Exit 0, including test targets |
| `rustfmt --check --edition 2024 src/dcp/validation.rs` | Exit 0 |
| Whitespace and custody checks | Exit 0; existing source prefix and both frozen domains preserved |

New tests also reject all three nonfinite values in each RGB position, exercise finite huge/tiny/subnormal inputs without an arbitrary range cutoff, reject XYZ/Lab/product/sum/normalization overflows, and check singular/nonfinite/overflowing inverse and zero-cone-scale failure. Successful finite negative/extended fixtures retain negative XYZ/L and L above 100.

The three existing ignored tests remain ignored: upstream no-profile acceptance, optional GPU parity, and ACR reference acceptance. No `--ignored` run occurred. Existing synthetic render and shader-validation tests executed because they are part of the required existing validation filter; they remain synthetic evidence.

An initial source version also passed the same 25/0/3 test result and all-target Clippy. A final one-line determinant-finiteness guard and its extreme-matrix assertion were then added, so both required commands were rerun on the final hash below. Earlier and final logs/binaries/patches are preserved separately. There were no test, compile, ownership/API, format, or Clippy failures in this slice.

## Environment, exact commands, and build effects

Observed environment: macOS 26.5.2 / 25F84, arm64; `rustc 1.98.1 (48a229cea 2026-09-01)`; `cargo 1.98.1 (797e8a9bc 2026-08-05)`; `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Builds ran from the snapshot's `src-tauri` directory using the exclusively granted shared cache, without a cold target or dependency refresh.

All build commands used:

```sh
export CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_DEV_INCREMENTAL=false
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_TEST_INCREMENTAL=false
export CARGO_INCREMENTAL=0
export CARGO_NET_OFFLINE=true
cargo test --locked --lib dcp::validation -- --nocapture
cargo clippy --locked --all-targets -- -D warnings
rustfmt --check --edition 2024 src/dcp/validation.rs
```

The exports above describe the exact subprocess environment; the Python runner applied these key/value pairs directly. A Python disk preflight asserted at least 6 GiB free before **each** Cargo command. Initial test/Clippy free bytes: 39,887,355,904 / 39,797,923,840. Final test/Clippy: 39,796,985,856 / 39,767,588,864. Full argv, cwd, environment, versions, source/fixture/lock hashes and start/end times are preserved in command JSON. Final build-plus-test and Clippy wall durations were about 60.78 s and 12.56 s, respectively; these are build receipts, not production performance measurements.

**Runtime download disclosure:** the first build's existing `build.rs` downloaded ONNX Runtime v1.22.0 from the existing Hugging Face URL and reported successful integrity verification. `CARGO_NET_OFFLINE` applies to Cargo and did not stop that build-script download. It created the ignored build byproduct `src-tauri/resources/libonnxruntime.dylib` (33,481,272 bytes), matched by `.gitignore:31`. Its SHA-256 is `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`; a copy is preserved with the evidence. Subsequent builds reported an existing valid runtime and skipped download. No dependency or lock changed. This byproduct was disclosed to the lead while the build was active.

Read-only identity/custody commands included `git rev-parse HEAD`, `git status --short`, `git show HEAD:src-tauri/src/dcp/validation.rs`, `git diff --stat`, `git diff --numstat`, `git diff --check`, `shasum -a 256` on the identified inputs/outputs, `git check-ignore -v src-tauri/resources/libonnxruntime.dylib`, and `stat -f '%z bytes' src-tauri/resources/libonnxruntime.dylib`. Fixture extraction was a stdlib Python heredoc reading only the frozen Markdown, copying both raw JSON fences and table cells into the owned fixture. Post-write verification asserted both raw fences occur verbatim in that fixture and their parsed domains are equal.

## Frozen hashes and artifacts

| File or immutable input | SHA-256 |
|---|---|
| Final `src-tauri/src/dcp/validation.rs` | `70515957ee552bcd7cfb9e7b0ab40933df8027348bf0e333db77718eb91e25e1` |
| `bench/dcp_validation/known-answers.json` | `1b0bae0896865a388cfd56e21c967517771a762a052042783a96ac1efa019b25` |
| Unchanged original validation prefix / entire base file | `1ec1913bd9575eca24b767925df00d0a27b0298a9bbf7ab1116793983da660cc` |
| Unchanged `src-tauri/src/dcp/delta_e.rs` | `70f57e66d3b4d95f8038cc6c698a39203f1eb41f8d8ccf993408f4c24dc0e296` |
| Unchanged `src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

Evidence directory (within the granted target, no separate target/cache):

`/private/tmp/rapidraw-integration-20261015/target/h2b2-candidate-lab-20261015/`

| Preserved final artifact | SHA-256 |
|---|---|
| `validation-final-test-binary` (29,545,792 bytes) | `2f184eab17f8b7d091b0c4284ba7b1ab8eacdea6d21f6c8f137bcb9964f1c464` |
| `validation-test.final.log` | `3747ce9d96594a4eb90defcf74d29a58a3206e66e91a3db73f64279e676e1d32` |
| `validation-test.final.json` | `877d6885d963e001b30686010f45c8f39f6ca54a588e76067c90ccddca209857` |
| `clippy.final.log` | `ab9fa25827de6f5325099066a0a3f04531ef6fa4750779a0881b9c45c6eef77b` |
| `clippy.final.json` | `09b3eecd2789fb6305c1f79ad0727ea93e6baeb1b2f1a163a60206d0b922e296` |
| `validation.final.patch` | `9a2ae12043aea9cdecc537d2e842cdd0f92e5062585e6cef2a9a956b176716c9` |
| `artifacts.json` | `539a24390d9d40154ff4ebb96b27bd7e07b6cbc1450740d78748506f0b20a904` |

The manifest also hashes the preserved initial command/log/binary/patch and runtime. The final test binary was copied immediately after its test run from `target/debug/deps/rapidraw_lib-b8c68747df43d7c8` so later cache reuse cannot silently replace the evidence. It is a unit-test executable, not an application/release artifact. This receipt's own hash is supplied in the Team delivery message.

**Pass:** bounded private helper and independent frozen-fixture tests; exact preexisting-source preservation; final targeted suite/format/strict all-target lint.

**Fail:** none observed in this slice.

**Not exercised:** actual candidate image inputs or comparison/report/CLI functionality; RAW/TIFF/ICC reference conversion; Adobe custody or fidelity; ignored GPU/upstream/ACR acceptance; Android/device/UI/export; production performance. H2b3 remains pending actual Adobe ICC bytes and a separately reviewed white-consistency bound.

No commits, index changes, push, device action, renderer/statistics/CLI/schema/dependency/lock edit, or other worker-file edit. The ignored runtime download is the disclosed build side effect above. Shared-cache work is finished and the exclusive grant is explicitly released in the delivery message. Stop at frozen source/fixture/receipt for independent review; worker completion is not integration acceptance.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `2ef30772637d1903a81f413739755186e1c890aec9b1a8e91c05a8bc96f0bc49` (12340 bytes).
