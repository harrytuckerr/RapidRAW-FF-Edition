# H2a independent design review — 15 October 2026

**Recommendation: Revise.** Do not dispatch the proposed monolithic H2b. The common-space, fail-closed, and threshold design is fit to implement only after the amendments below are binding, and only as the serial slices in §6. H2b1 alone may be dispatched before those colour amendments because it does not record a route.

Reviewer: Grok Build / `grok-4.7` (Team slot `01a13e02-3c22-7ad0-9ed3-8bab4bc7d945`). Snapshot reviewed: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/harness-review`. Frozen HEAD `4c1e965a53de9c835df920b2e3d69c2c8a864708`, tree `9f36c891f35d9310a7977bda7d851178ad92039d`. `git diff 8e6318814d42e155d88a1d0efd485f9d571b4ffb HEAD` is empty. Contract read from `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/harness-design/docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md`, SHA-256 `a17b2e45277f1a7480952a964dfcf3d4c590569e7c941ba2075f317ce3ffd0fe` (matches the assignment). Environment: macOS 26.5.2 / 25F84, arm64. This task writes only this file. No source, lock, contract, device, build, or Git mutation.

Authoritative inputs also read: handover `/Users/harrisontucker/Documents/CoWork/RapidRaw/ORCHESTRATOR_HANDOVER_2026-10-15.md` §§6–7 and H2/H3, and `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/lead/docs/integration/TEAM-LEDGER-2026-10-15.md`. A lead message during this review said the X-T5 diagnosis is complete and Opus is implementing isolated R1 RAW-name/JPEG errors, with H2 source unchanged. Do not assign these H2 slices to Opus while that R1 implementation is open.

## 1. Design-review result

| Area | Result | Meaning |
|---|---|---|
| Comparator colour contract (linear sRGB and ICC matrix/gamma → one Lab, no comparator clamp, relative intent, no second `chad`) | **Pass**, with F3 before the ICC slice | Static. Not a runtime conversion. F4 and F6 bind the report slice, not the matrix reader. |
| Original thresholds, masks, custody, empty/invalid Pass | **Pass** as a design; current helper is a static **Fail** | Design keeps mean <2, p95 <3, max <5 and closes the zero Pass. Code still returns one. |
| Recorded CPU route versus what the pipeline executes | **Fail** | F1 and F2. A literal implementation would certify settings the pipeline does not apply. |
| Handwritten ICC/TIFF subset versus installed `moxcms` | **Pass** to keep the subset | Transitive `moxcms` is not a usable or faithful CMS for this contract. |
| One H2b worker package | **Fail** | Eight files and six responsibilities. Replace with §6. |
| Builds, Adobe ICC bytes, Sharma execution, devices, preview/export/thumbnail/upstream | **Not exercised** | No `cargo` test or build was run. Missing Adobe assets are an input gap, not a model failure. |

Overall: **Revise**. Lead adjudicates. This file does not edit the contract.

## 2. Commands and fingerprints

```text
shasum -a 256 <harness-design>/docs/integration/H2-COMPARISON-CONTRACT-2026-10-15.md
git -C <harness-review> rev-parse HEAD 'HEAD^{tree}'
git -C <harness-review> diff --stat 8e6318814d42e155d88a1d0efd485f9d571b4ffb HEAD
shasum -a 256 src-tauri/src/bin/validate_dcp.rs src-tauri/src/dcp/validation.rs \
  src-tauri/src/dcp/delta_e.rs src-tauri/Cargo.lock
sw_vers; uname -m
```

No test, build, formatter, or dependency resolution. Source hashes match the contract’s own table:

| File | SHA-256 |
|---|---|
| `src-tauri/src/bin/validate_dcp.rs` | `bb85db6be65047126c7143a1693ee49c7e9de0408cd64d0f561d2f855db6a293` |
| `src-tauri/src/dcp/validation.rs` | `1ec1913bd9575eca24b767925df00d0a27b0298a9bbf7ab1116793983da660cc` |
| `src-tauri/src/dcp/delta_e.rs` | `586d4ee19f07dabc46e65902821bf6e84ad100f45318a5d7e8b4b4b2d56c7f50` |
| `src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

`validation.rs` is already 1,324 lines and contains the GPU parity test. `delta_e.rs` is 571. `validate_dcp.rs` is 301. Diff of this review is one new documentation file. Its SHA-256 is reported in the delivery message, not embedded here.

## 3. Findings

### F1 — High. `highlight=2.5` is not the clamp that runs

- Where: `src-tauri/src/raw_processing.rs:170` (`_highlight_compression` unused), `:273–318` (hard-coded limit); call sites `src-tauri/src/bin/validate_dcp.rs:134–140` and `src-tauri/src/dcp/pipeline.rs:177–183`. Contract §2 says the fixed route is `highlight=2.5`.
- Failing input: any RAW passed through `render_selected_profile(..., false, 2.5, "auto", ...)`.
- Consequence: camera RGB is floored with `.max(0.0)` and clamped to `1000.0` whenever `fast` is false (`1.0` when `fast` is true). The `2.5` argument is discarded. A report that lists `highlight=2.5` as an executed setting is false. Negatives never reach the DCP, so a real-route sample cannot prove the extended-negative Lab rule. `fast=false` and `linear_mode="auto"` are real: `"auto"` is the default arm at `raw_processing.rs:196–201` (no un-gamma, calibration requested, then stripped because a DCP is active). Do not conflate those with highlight.
- Minimum correction: do not change `raw_processing.rs` in H2. The route record must say `highlight_argument=2.5` and `effective_camera_rgb_clamp=[0,1000]` for this `fast=false` path. Synthetic in-memory negatives remain mandatory in the colour slice. Making `2.5` actually take effect is a separate production assignment, not H2.
- Status: **static**. The parameter name is unused and the `1000.0` literal is unconditional on that argument.

### F2 — Medium. Look amount 0–200 can be certified after the pipeline has clamped it

- Where: `src-tauri/src/dcp/rgb_look.rs:54–72` and `:120–127`. Contract §2 accepts amount 0–200 inclusive and says not to record unexecuted settings.
- Failing input: a Look whose parsed `max_amount` is `1.0`, manifest amount `200` (or any non-integer such as `100.4`). `RgbLook::new` rounds percent and clamps into `[min_amount, max_amount]`. Amount `0` bypasses the clamp; any other amount clamps ProPhoto channels to `[0,1]` before the table.
- Consequence: pixels can match an amount-100 reference while the report says 200. Extended-range diagnostics on a nonzero Look are post-clamp. The private `amount` field cannot be read from the binary.
- Minimum correction: v1 accepts only integer percent. Before render, apply the same public formula as `rgb_look.rs:68–72` to the parsed table and fail `SETTINGS_UNSUPPORTED` unless the effective fraction equals `percent/100`. Without a Look, keep the contract’s inert amount `100`. Report `look_clamp_applied` when amount is nonzero. Do not change production clamping and do not add a getter on `RgbLook`.
- Status: **static**.

### F3 — Medium. A structurally valid matrix profile can still miss the PCS white

- Where: contract §3. The reader requires a finite nonsingular `rXYZ/gXYZ/bXYZ` and a D50 `wtpt`, then Bradford-adapts from the nominal PCS white `[0.9642, 1, 0.8249]`, not from the column sum.
- Failing input: `wtpt` within one s15Fixed16 unit of that nominal D50, while `rXYZ+gXYZ+bXYZ` is far from it. RGB `(1,1,1)` then does not land on the white that Lab treats as neutral.
- Consequence: the profile passes `icc_matrix_gamma_v1` and paints a chromatic cast onto every neutral. Known-answer fixtures built from a correct matrix will not catch it.
- Minimum correction: reject unless each component of the colorant-column sum is within one s15Fixed16 unit of the validated `wtpt`. That is a structural gate, not a new ΔE tolerance. Keep adapting from the nominal PCS D50 to `D50_XYZ`, not from the column sum.
- Status: **static** design gap. No profile bytes were decoded.

### F4 — Medium. Full-frame ΔE storage has no ceiling or error code

- Where: contract §5 error list and §8’s “measure memory later”.
- Failing input: a 24 MP pair if both sides retain f64 XYZ and Lab plus the ΔE vector. Order of magnitude: candidate f32 RGB ≈ 288 MB, reference u16 RGB ≈ 144 MB, one f64 ΔE vector ≈ 192 MB; keeping both Lab images as well is about another 1.2 GB.
- Consequence: the process can die on allocation, or an implementer may subsample, which the contract forbids but does not make a tested failure.
- Minimum correction: add stable code `RESOURCE` before the pixel slice. Compute ΔE in one pass and retain only the included ΔE samples (p95 needs them) plus scalar min/max and non-finite flags. Refuse when `width*height` exceeds 80 million or the ΔE allocation would exceed 2 GiB. Do not subsample. This does not change mean/p95/max thresholds.
- Status: **static** estimate, not a measured resident set.

### F5 — Medium. The current harness README describes a comparator that does not exist

- Where: `bench/dcp_validation/README.md:20–30` and `:53–59`, against `validate_dcp.rs:155–159` and `validation.rs:817–824`, `:1267–1274`. README:59 also calls `gpu_vs_cpu_parity` a ΔE test. The test at `validation.rs:1236–1259` is absolute RGB error (max < `1e-3`, mean < `1e-4`) and does not call `check_gpu_parity`.
- Failing input: `RAPIDRAW_TEST_ASSETS=1 cargo run --bin validate_dcp -- compare ...` as the README prints it.
- Consequence: the command always exits 1 with “not implemented”. The ignored D2/D6 tests panic when invoked; the env var does not turn them on. Elsewhere the env var is a skip-shaped Pass (`parser.rs:1116–1118` returns without asserting). Copying that pattern into H2 would create an empty Pass.
- Minimum correction: only the README slice in §6 rewrites the compare/GPU rows. State that the GPU test is RGB error and that ΔE CPU/GPU thresholds are not exercised. Do not convert the GPU test to ΔE. H6b already excludes this README; do not give it a second writer. New tests must fail closed, not `return` when an env var is absent.
- Status: **static**.

### F6 — Low. Source identity must not be labelled verified

- Where: contract §4. Binary hash is checked against `current_exe`. Commit, tree, and `Cargo.lock` are “expected” and also “not independently proven”.
- Failing input: a manifest whose source commit string does not match the worktree, while `current_exe` matches.
- Consequence: an implementer can print `source verified` from a self-consistent manifest.
- Minimum correction: report `source_attestation="external_wrapper"` for commit, tree, lock, and source-file receipt. The only self-checked code identity is `BINARY_MISMATCH` against `current_exe`. A Pass does not prove source identity.
- Status: **static** wording hazard.

### F7 — Low. Metadata verification cannot be done from the binary by calling the existing helper

- Where: `lib.rs:58` and `:63` (`mod multi_exposure`, `mod raw_processing`, both private). `neutralize_wb_if_multiexposure` is `pub` at `multi_exposure.rs:53` but not visible to `validate_dcp`. `metadata_orientation` at `raw_processing.rs:69` is private. `dcp/mod.rs:29` already exports `pub mod validation`.
- Failing input: `use rapidraw_lib::multi_exposure::...` from the bin.
- Consequence: it does not compile. Widening `lib.rs` or `dcp/mod.rs` is outside ownership. Re-developing through `render_selected_profile` to observe WB would run the pipeline twice and still would not return WB.
- Minimum correction: perform the check inside `validation.rs`, which can call the public helper and the same public rawler sequence `metadata_orientation` uses (`raw_metadata`, default `RawDecodeParams`, EXIF orientation, `Orientation::from_u16`, default Normal). WB is `RawImage.wb_coeffs` after `neutralize_wb_if_multiexposure`, matching `raw_processing.rs:254–259`. If that requires `raw_image()` rather than metadata alone, record a read-only decode and do not develop twice. No new production API. First three gains and the `[G/R, 1, G/B]` neutral from `pipeline.rs:49–57`; the fourth RGBE slot may be NaN. Predicate to freeze: `abs(got-expected) <= 1e-6 * (1 + abs(expected))` per component.
- Status: **static**.

### F8 — Low. Do not copy the singular-matrix identity fallback

- Where: `delta_e.rs:186–188`, `render.rs:63–65`, `render.rs:266–268`.
- Failing input: a singular 3×3 passed to `prophoto_to_lab` or the private Bradford helper.
- Consequence: the code substitutes identity and returns a finite, wrong Lab or adaptation. `partial_cmp(...).unwrap()` at `delta_e.rs:267` panics on NaN instead.
- Minimum correction: the harness returns `ICC_INVALID` / `NONFINITE_SAMPLE` and never calls `prophoto_to_lab`. p95 indexing is fallible. This is already implied by §3 and §5; it is called out so the identity fallback is not reused.
- Status: **static**.

## 4. Confirmed pre-existing defects the contract already handles

These are static Failures of the current helper, not new runtime executions. The design’s correction is accepted. H2b1 is what implements it. Do not loosen thresholds to make them green.

| Defect | Site | Concrete input | Today | Required correction |
|---|---|---|---|---|
| Empty sample Passes | `delta_e.rs:250–259` then `:283–285` | `delta_es=[]`, empty masks | mean/p95/max `0`, `check_thresholds` and `check_gpu_parity` are true | Fallible stats. Empty primary sample is `EMPTY_SAMPLE`, never numeric 0. Same for the GPU helper. No production caller: `validation.rs:25` only re-exports them. |
| Overlap counted twice | `delta_e.rs:238–240` versus the unique filter at `:245` | one pixel in both masks | `excluded = shadow + highlight` | Disjoint `shadow_only` / `ceiling_only` / `both`, as §5. Do not change the numeric thresholds. |
| Mask length unchecked | `delta_e.rs:245` | mask shorter than `delta_es` | index panic; a longer mask is ignored | Equal lengths or `MASK_INVALID`. |
| NaN panics; excluded NaN is ignored | `delta_e.rs:242–247`, `:267` | NaN in an excluded pixel | sort never sees it, so the case can Pass | Any non-finite RGB, XYZ, Lab, or ΔE fails, including excluded pixels, before sort or JSON. |
| Stale “subset” comment | `delta_e.rs:301–305` | the array itself | comment says subset; `// Pair 1` through `// Pair 34` are all present; assertion is `< 1e-4` | Correct the comment only. Keep values, `cie_de2000`, and `1e-4`. kL=kC=kH=1 at `:95–97`. |

`compute_stats`’s p95 index, `(count as f64 * 0.95).ceil() - 1`, is the definition §5 freezes. Keep it. H2b1 adds the 1/19/20/21 cases. Do not switch to a NumPy default.

Shadow `Y < 0.001` is the original documented exclusion (`delta_e.rs:231`, README:75), retargeted to reference adapted XYZ Y because the candidate is no longer linear ProPhoto. That Y is below the Lab linear branch `(6/29)^3`, so L* is about 0.9: the exclusion is narrow and does not loosen mean <2 / p95 <3 / max <5. Replacing “any linear ProPhoto channel > 0.999” with “any original reference u16 channel == 65535”, evaluated only on the reference, is required. Excluding on candidate clip or on ΔE would hide errors. Equal-to-threshold fails because the original comparisons are strict `<`. `synthetic_harness` Pass is comparator evidence only.

Ignored D2/D6 gates at `validation.rs:817–824` and `:1267–1274` panic when invoked and do not consult an env var. Leave them ignored. A file-presence or env-var Pass is forbidden.

## 5. Colour contract that holds

Checked against `delta_e.rs`, `render.rs`, installed `image` 0.25.10, locked `moxcms` 0.8.1, and the ICC transform-selection model (matrix/TRC relative intent applies TRC then matrix and does not apply chromatic adaptation; absolute intent adds that adaptation). ICC.1-2022 §§8.4.3 and 10.6 remain the unread-in-full primary for the gamma equation; the direction below is the standard device→PCS reading and matches the contract’s 0.5→0.25 test.

- `D50_XYZ` at `delta_e.rs:120` is exactly the contract’s `Wc`. `xyz_to_lab` divides by it. Adapting **to Wc** before that call is the correct white. `lab_f` (`:128–134`) sends `t <= (6/29)^3`, including negatives, down the linear branch. It does not clamp and does not call `cbrt` on negatives. Reuse is valid for finite extended Lab. Non-finite inputs still need an explicit rejection; `lab_f` will propagate them.
- Candidate path: published sRGB/Rec.709 chromaticities, independent of `render.rs`’s row-normalised `XYZ_TO_SRGB_D65` (`render.rs:102–106`, `:270–287`), then Bradford D65→Wc, then `xyz_to_lab`. Do not invert the renderer's matrix and do not apply an sRGB EOTF. `to_working_space` (`render.rs:364–369`) is already linear. Out-of-gamut working RGB can still be negative after that matrix even though RAW development floored camera RGB (F1).
- Bradford coefficients in §3 match `BRADFORD_CONE` at `render.rs:51–54`. Compute the inverse in f64. Do not call the private `bradford_adapt_matrix`, and do not edit `render.rs`. `render.rs:111` says the renderer has one D50. The ICC nominal PCS white stays a harness-only constant named `ICC_PCS_D50_XYZ` inside `validation.rs`. Do not “simplify” it away: skipping PCS-D50→Wc fails the contract’s doubled/omitted adaptation test.
- Canonical header encoding `63190/65536` and `54061/65536` is within one s15Fixed16 unit of `[0.9642, 1, 0.8249]`. The check accepts that encoding. Wc itself is several units away and must not be used as `wtpt`. The two whites stay distinguished.
- Reference TRC then matrix, no gamut map, no black-point compensation, no integer PCS quantisation. Count 0 is identity. Count 1 is a positive u8Fixed8 exponent applied as `encoded^gamma` (512 → 2; 0.5^2 = 0.25, not sqrt). `chad`, if present, is parsed and reported, not applied, for `relative_colorimetric`. A competing `A2B*` / `B2A*` / `D2B*` / `B2D*` / `para` / table curve is `ICC_INVALID`, not a cue to prefer the matrix tags.
- `image` 0.25.10 `TiffDecoder::icc_profile` (`tiff.rs:311–317`) turns every tag error into `Ok(None)`. `orientation` (`:337–346`) maps a missing or unreadable value to `NoTransforms`, which is EXIF 1, so missing and Orientation=1 are indistinguishable. A value above 255 is truncated with `.min(255)` before the EXIF map. The contract’s checked IFD preflight is necessary. Preflight owns orientation presence, sample layout, next IFD, and the ICC byte slice. Use `image` for interleaved u16 pixels only. Hash and decode one buffer; hashing a path and opening it again is not the same bytes.
- App routes are correctly out of H2b. Preview can return the literal `WGPU_RENDER` with no pixels (`lib.rs:744–754`). Export JPEG/PNG attach an sRGB ICC; the TIFF branch does not (`export_processing.rs:1223–1255`). The default RAW shader is not a neutral encode: brightness gamma `1.1`, contrast mix `0.75`, then `clamp` to `[0,1]` (`shader.wgsl:1932–1938` and `:2013`). Thumbnail JPEG quality is 75 (`file_management.rs:1816–1821`) and GPU failure returns the cropped base (`:1760–1761`). H2c has to name those facts. H2b must not infer them from a CLI Pass.

## 6. `moxcms` assessment

`Cargo.lock:2897` lists `moxcms` under `image` 0.25.10. `Cargo.lock:3902–3910` pins `moxcms` 0.8.1. `image`’s crate manifest depends on `moxcms` 0.8.0 with no feature override, so default features apply. Those defaults include LUT and fixed-point shapers. `extended_range` is not a default feature.

A transitive lock entry does not make `use moxcms` legal in this package, and it is not permission to edit `Cargo.toml` or `Cargo.lock`. `image` does not expose a general “apply this TIFF ICC” API. Its TIFF decoder returns raw ICC bytes or `None`. The CICP helpers inside `image` are a different encoding.

`moxcms` 0.8.1 `ColorProfile::new_from_slice` and `create_transform_f64` are a real CMS, and they do not implement this contract:

- `TransformOptions` defaults to perceptual intent and `prefer_fixed_point=true` (the docs on that flag describe Q2.13 for RGB→XYZ). §3 forbids integer PCS quantisation.
- `allow_extended_range_rgb_xyz` exists only behind the non-default `extended_range` feature and defaults to false. Negatives and values above 1 are outside the default transform.
- The CMS will run LUT profiles. This harness must reject them.
- Its PCS white is not the required pair `ICC_PCS_D50_XYZ` then `D50_XYZ`.

Using it as the comparator would hide the failures the handwritten subset is there to expose. Keep the bounded matrix/`curv` reader. Do not add `moxcms` as a direct or dev dependency in these slices. A later oracle would be a new lead assignment and a new review, not an H2b convenience.

Handwritten parsing is acceptable only with the contract’s bounds: checked `count * entry_size`, range checks, duplicate signatures rejected, partial overlaps rejected, exact shared ranges legal, truncated tags rejected, no hand-rolled strip decompression. If that reader cannot be written on `image` + `serde` + `sha2` + `nalgebra` (all already direct dependencies), stop with the fixture. `sha2` and `nalgebra` are direct; `moxcms` is not.

## 7. Serial implementation slices

No slice starts until the lead assigns it. One writer. No second writer on the same file, including a “small follow-up” in the same snapshot. Finishing early does not authorise the next slice. Do not assign these to Opus while the R1 RAW-name/JPEG implementation is open. No `Cargo.toml`, lock, `lib.rs`, `dcp/mod.rs`, renderer, pipeline, shader, Android, or existing fixture-script edits. No commit. Ignored D2/D6 tests stay ignored.

H6b does not own `bench/dcp_validation/README.md`. Only H2b7 writes it.

### H2b1 — fail-closed statistics

- Files: `src-tauri/src/dcp/delta_e.rs` only.
- Depends on: this review, lead dispatch. No colour amendment required.
- Do: fallible `compute_stats`, `check_thresholds`, and `check_gpu_parity`; length, overlap, empty, and non-finite guards; p95 1/19/20/21; correct the subset comment. Keep the formula, `D50_XYZ`, Sharma bytes, and `< 1e-4`.
- Acceptance: from `src-tauri`, `cargo test --locked --lib dcp::delta_e -- --nocapture`. Lead confirms ≥6 GiB free before a cold target dir. Not `--all-features`, not the bin, not Android.
- Stop: helper cannot yield a zero Pass. If the `validation.rs:25` re-export fails to compile, stop and report. Do not edit `validation.rs` inside this slice.

### H2b2 — candidate Lab known answers

- Files: `src-tauri/src/dcp/validation.rs` (new private functions and tests only; do not modify GPU, identity, or ignored acceptance tests) and new `bench/dcp_validation/known-answers.json`.
- Depends on: H2b1 imported.
- Do: sRGB chromaticity → XYZ → Bradford D65→Wc → existing `xyz_to_lab`. Freeze f64 expectations from a separate calculation in the JSON (formula, Wc, Bradford matrix, precision). Include black, D65 white → L*=100,a*=b*=0, linear 0.18, unequal channels, negative and >1 channels, and omitted/reversed/doubled adaptation. No TIFF, ICC, manifest, or RAW.
- Acceptance: `cargo test --locked --lib dcp::validation -- --nocapture`. Analytic error < `1e-6`. Existing default validation tests still pass. Ignored tests not run as acceptance.
- Stop: if an expected value can only be produced by calling the function under test, stop. Do not create the other bench files yet.

### H2b3 — ICC matrix/gamma bytes, no TIFF container

- Files: `validation.rs` only.
- Depends on: H2b2 imported. Binding amendments: F3 column-sum, F8, §5 `moxcms` refusal.
- Do: v2/v4 `mntr` / `RGB ` / `XYZ ` subset, nominal PCS D50 and `wtpt` checks, column-sum check, count-0 and count-1 TRCs, `chad` parsed and unused, reject LUT/`para`/table/singular/duplicate/truncated. Synthetic profiles are byte arrays in the test. Gamma 2 at 0.5 is 0.25. Identity linear 0.25 is L*≈57.07542089 after the PCS-D50→Wc step. Non-symmetric matrix catches a transpose. u16 fixtures, when added, use ≤0.005 Lab per component, never the product ΔE thresholds.
- Acceptance: new unit tests plus the H2b2 command.
- Stop: unsupported shape is `ICC_INVALID` and a written tag list. Do not widen the subset. No new crate.

### H2b4 — classic TIFF preflight

- Files: `validation.rs` only.
- Depends on: H2b3 imported.
- Do: first IFD only, next IFD 0, Orientation tag present and equal to 1, interleaved RGB u16, no alpha, ICC bytes from the preflight, pixels from `image`. Reject BigTIFF, float, CMYK, Lab, gray, extra samples, and a missing orientation tag. Pinned ICC SHA-256.
- Acceptance: synthetic TIFF bytes through the same core, plus the H2b2 command.
- Stop: do not ignore a following IFD or SubIFD. A real Adobe file that uses one, or a `para`/LUT profile, stops this slice with the observed structure. That stop is a successful fail-closed result.

### H2b5 — masks, aggregate, report types

- Files: `validation.rs` only, plus new `bench/dcp_validation/manifest.example.json` (template hashes clearly unset, not guessed).
- Depends on: H2b1 and H2b4 imported. Binding amendments: F4 `RESOURCE`, F6 attestation label.
- Do: `deny_unknown_fields`, duplicate-key rejection (serde’s default map does not provide it), reference-only shadow/ceiling mask, disjoint counts, null metrics for empty diagnostic groups, pooled ΔE, exit status data, atomic report types, `RESOURCE`. No RAW call yet.
- Acceptance: unit tests for empty, all-excluded, overlap, exact threshold, unequal case sizes, NaN in an excluded pixel, and duplicate JSON keys. Aggregate of one failing pixel must fail even if the mean would pass.
- Stop: no CLI flags in this slice.

### H2b6 — CLI wiring only

- Files: `src-tauri/src/bin/validate_dcp.rs` only.
- Depends on: H2b5 imported. Binding amendments: F1 and F2 recorded as specified.
- Do: `compare --manifest` / `--report`, once each; old `compare --raw` fails with migration text; sequential cases; in-memory `render_selected_profile(false, 2.5, "auto")`; metadata check via the H2b5 library function; effective highlight and Look amount fields; exit 0/1/2/3 with 2 winning over 1. Existing `render` and `self-test` stay, and neither is acceptance.
- Acceptance: `cargo test --locked --bin validate_dcp -- --nocapture` and `cargo build --locked --bin validate_dcp`. Lead sets absolute `CARGO_TARGET_DIR` and checks ≥6 GiB. Do not use a bare `/debug/validate_dcp`.
- Stop: no Python and no README in this slice. A synthetic in-process failure is enough. Do not manufacture an Adobe Pass.

### H2b7 — process driver and README correction

- Files: `bench/dcp_validation/test_compare.py`, `fixtures.py`, and `README.md` only.
- Depends on: H2b6 imported and a lead-recorded binary SHA.
- Do: process tests for missing assets, bad ICC, one-pixel max ΔE > 5, exit codes, and existing report path. README replaces the false compare/env-var/GPU-ΔE claims and documents this slice’s limits. Generated images stay in a private temp directory.
- Acceptance: the contract’s `python3 ../bench/dcp_validation/test_compare.py` invocation against the recorded debug binary. `sharma_all_34_pairs` remains the Rust test from H2b1, not a second implementation.
- Stop: real Adobe and real preview/export/thumbnail/upstream cases are **Not exercised**. Do not flip ignored gates. Do not regenerate goldens from candidate output.

## 8. Unresolved inputs

Not model failures. Do not invent them.

- One real Adobe 16-bit export’s ICC bytes: header illuminant, `wtpt`, `chad`, colorant tags, and whether TRCs are `curv` count 1, `para`, or a LUT. A `para` or LUT profile is a later slice, not an H2b3 extension.
- Whether that TIFF’s next-IFD or SubIFD is nonzero. H2b4 stops if it is.
- Controlled RAW + Modular base + native Look + settings receipt. Flat must not stand in for Modular. H0 found no such controlled reference.
- Custody of those files stays outside the comparator. A consistent rewrite of provenance and pixels is not detectable from ΔE.
- W3C CSS Color 4 is an independent matrix example only. Known answers use Wc and this Bradford matrix, not the W3C D50 triple.

## 9. What this review did not do

No `cargo test`, `cargo build`, device command, or Adobe render. Sharma’s 34 pairs were counted and the tolerance was read; they were not executed. Numeric claims about L*≈57.075 and the s15Fixed16 D50 encoding are static arithmetic. H2c/H2d/H3 thresholds were only checked for consistency with handover §7 (CPU vs Adobe <2/<3/<5, CPU/GPU <0.1/—/<0.5, editor/export/thumbnail and Android/desktop <1/—/<2, upstream bit-identical). They are not implemented here and must not be edited.

**Next consumer:** lead adjudication. Dispatch H2b1 only, to a worker who is not occupied by the R1 RAW-name/JPEG implementation, after import of this file. Later slices wait for their predecessor’s imported receipt and for the amendments marked binding on that slice. Adobe ICC bytes remain unavailable; nothing below expands the parser to cover an assumed Adobe profile shape.
