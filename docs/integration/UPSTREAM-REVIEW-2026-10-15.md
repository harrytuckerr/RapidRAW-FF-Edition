# RapidRAW upstream integration review

Checked 15 October 2026 (Australia/Melbourne).

## Recommendation

Integrate upstream on a separate branch, with explicit reconciliation of the DCP pipeline. Do not merge upstream straight into cobalt/main or replace conflicting files wholesale. This is a substantial update, not a dependency refresh.

No application source changes, commits, pushes, or merges were made during this review. Remote-tracking refs and tags were fetched. The merge simulation ran in a temporary local clone. This report is the only new workspace file.

## Exact baseline

- Working fork: https://github.com/harrytuckerr/RapidRAW
- Upstream: https://github.com/CyberTimon/RapidRAW
- Local branch: `cobalt/main`, `6f171d9bc0724361efc385cc43f3a6d1c36f5054` (PR #12 fixes included).
- `origin/cobalt/main` matches local HEAD after fetch.
- Common ancestor: `99bb0da3271921f3e70a81b8be44171e97e1c991`.
- Upstream reviewed: `3385c9f0` (14 October upstream commit date, export-time-zone PR #1925).
- Divergence: 34 fork-only commits; 344 upstream-only commits (includes merges).
- Upstream delta: 160 files, 29,550 added lines, 11,476 deleted lines.
- 33 files changed on both sides.
- Latest published release: [v1.6.5](https://github.com/CyberTimon/RapidRAW/releases/tag/v1.6.5), 6 October, commit `79c2a46b448b9a0bee0b30f8c345daad003ad694`.
- 314 upstream-only commits through v1.6.5, plus 30 commits after that release through reviewed main.

## Valuable merged updates

| Area | Evidence / representative commits | Integration recommendation |
|---|---|---|
| Kelvin white balance and area picker | `9ba20c02`, `11e20e77`, `e4d6fd16`; new white_balance.rs and as_shot.rs | High value. Reconcile with DCP camera-neutral and illuminant interpolation; do not replace DCP math with this superficially similar module. |
| Highlight recovery and guided filters | `40cfa3df`, `e99082ad`; guided shader and changed processing stages | High value, high overlap. Check stage placement and input colour space before enabling on the camera-native DCP path. |
| RAW baseline exposure and highlight-preservation compensation | [PR #1908](https://github.com/CyberTimon/RapidRAW/pull/1908), `bc7791a4`; baseline_exposure.rs | Relevant to Fujifilm dynamic-range settings. Keep RAW metadata exposure, DCP BaselineExposureOffset, and user exposure distinct; define when each applies. |
| Export precision and colour metadata | `ad179a45` true 16-bit TIFF, `e3022f65` sRGB ICC in JPEG/PNG | Priority for usable exports and validation. Verify actual pixel precision and profile metadata through our export route. |
| Export metadata correctness | `f090c3f9`, `44f77264`, `0baaae42` | Preserve EXIF, keywords/rating/label and capture timezone; integrate with profile-aware export. |
| Stability and caches | `55321519` GPU recovery, `cf6813f1` export cache, `dad0cec1` editor-exit crash | Priority. Retain profile identity/amount in cache keys and profile resources during GPU recovery. |
| Android builds and layout | `6e757834`, `7bbd1ffd`, `8387fc12`; build workflow changes | Bring along the matching dependency/build changes. Android needs runtime checks, not just APK compilation. |
| Library/editing workflow | pick/reject flags, improved cropping/panels, embedded previews, retouching | Useful, generally less colour-sensitive. Ensure the profile panel and preset opt-in survive layout/state changes. |
| Apple RAW 9 | `9b7e3368`, apple_raw.rs | Conditional only: its output is extended linear sRGB, not camera-native RGB. Route DCP-selected images through a compatible decoder unless a separately validated design exists. |

## Concrete integration hazards

### 1. GPU binding collision (confirmed source conflict)

Upstream shader.wgsl uses group 0 binding 12 for `gf_coeffs_texture` and binding 13 for `gf_dehaze_texture`. Our shader uses 12 for `dcp_huesat_tex`, 13 for `dcp_look_tex`, and 14 for `dcp_tone_curve_tex`.

Allocate unique bindings and update WGSL declarations, Rust layouts, resources, dummy resources and test harness together. Also reconcile storage/uniform structure layouts. A text merge is insufficient.

### 2. White-balance algorithms have different responsibilities

Upstream introduces its own Robertson table, temperature/tint model and A/D65 camera-matrix interpolation. Our DCP path resolves profile calibration illuminants and camera-neutral conversion. Similar constants do not establish interchangeable semantics. Define one contract for decoded pixel space, WB already applied, profile interpolation white, and subsequent user WB edits. Verify no duplicated white balancing.

### 3. Exposure compensation is now a separate upstream feature

`baseline_exposure.rs` reads DNG baseline and vendor highlight-preservation metadata (including Fujifilm dynamic range); `image_processing.rs` threads compensation through render settings. This is not automatically the same as the DCP profile offset. Specify combined behavior and cache invalidation, and test each toggle independently. A double application could produce bright but plausibly rendered images.

### 4. Apple RAW output cannot be treated as sensor RGB

Upstream image_loader.rs can return early from Apple RAW development; apple_raw.rs explicitly renders to `kCGColorSpaceExtendedLinearSRGB`. Feeding this into a camera-profile matrix would apply camera conversion to already converted pixels. Preserve the rawler/camera-native route when DCP is selected; report incompatible decoder choices explicitly.

### 5. Toolchains and Android version policy changed

Upstream requires Rust 1.98 (our base specifies 1.96). Desktop lockfile uses the newer Tauri stack, while upstream's Android workflow pins Tauri/wry/tao and the JS CLI/API to older compatible versions. The pinning is build-time, so a plain local Android build does not automatically reproduce CI. Carry this policy into local Android instructions or make it reproducible through a shared script. `wgpu` remains 29.0 upstream; there is no reason to bump it during reconciliation.

### 6. Cloud integration adds service assumptions

Upstream includes cloud UI/auth/storage dependencies. Inspect endpoint/configuration and fork identity before presenting cloud features in our fork; do not assume upstream service access or credentials carry over. Keep local editing functional without signing in.

## Merge simulation

`git merge-tree --write-tree` in a temporary shared clone returned conflicts in these nine files:

1. .gitignore
2. src-tauri/Cargo.toml
3. src-tauri/src/app_state.rs
4. src-tauri/src/gpu_processing.rs
5. src-tauri/src/image_loader.rs
6. src-tauri/src/image_processing.rs
7. src-tauri/src/lib.rs
8. src-tauri/src/raw_processing.rs
9. src-tauri/src/shaders/shader.wgsl

This identifies textual conflicts only. Cleanly merged files such as export_processing.rs, cache_utils.rs, UI and locale files still require semantic review. The fork-only dcp module being retained does not prove it remains connected to production rendering.

## Open upstream PRs worth tracking (not merged or accepted here)

Open status checked through GitHub API. Inspected the newest 100 open PR summaries; this is a shortlist, not an exhaustive audit of every open PR. Read detailed descriptions for #1911, #1806, #1923 and #1917. Their test claims are author-reported, not independently reproduced here.

| PR | Relevance | Action |
|---|---|---|
| [#1911](https://github.com/CyberTimon/RapidRAW/pull/1911) | Android startup: avoid reinitializing ndk-context after tao has initialized it | High priority to review with the chosen Android dependency stack. Author reports two device tests; explicitly says Mali editor crashes remain. |
| [#1806](https://github.com/CyberTimon/RapidRAW/pull/1806) | Android application-context lifetime across activity recreation | Relevant to content-URI/profile import. Reconcile ownership with #1911 rather than stacking both blindly. |
| [#1923](https://github.com/CyberTimon/RapidRAW/pull/1923) | Failed headless export currently can return success exit code | Important for trustworthy validation automation. Candidate separate backport after code review and reproduction. |
| [#1917](https://github.com/CyberTimon/RapidRAW/pull/1917) | Sony reduced-resolution LinearRaw white-balance bug | Reinforces that decoded RGB/unity WB is not necessarily a sensor neutral. Test if those formats are supported. |
| [#1913](https://github.com/CyberTimon/RapidRAW/pull/1913), [#1914](https://github.com/CyberTimon/RapidRAW/pull/1914), [#1907](https://github.com/CyberTimon/RapidRAW/pull/1907) | Android touch selection, zoom and culling gestures | Useful after core rendering and startup are stable; titles/status reviewed only. |

Do not import unmerged PRs merely because their titles sound relevant. They carry separate review and regression risk.

## Proposed integration plan

1. Snapshot the current fork commit and preserve its tests/assets. Use a separate `codex/upstream-sync-2026-10-15` branch or isolated checkout. Keep cobalt/main unchanged until acceptance.
2. Target the reviewed upstream main commit (which includes post-v1.6.5 Android mitigations and metadata fixes), recording v1.6.5 as the release comparison point. Avoid a long chain of hundreds of cherry-picks.
3. Establish an unmodified upstream build/render baseline before blaming inherited failures on the merge. Record Rust/Node/lockfile and Android pinning.
4. One owner reconciles RAW loading, AsShot/WB/exposure contracts, GPU bindings and resources. Parallel frontend or Android work must have separate owned files/checkouts and wait for those contracts.
5. Preserve DCP import/registry, strict per-camera base/Look pairing, profile persistence, Amount, preset opt-in, decoder routing, cache identities, shared WGSL validation and actual CPU/GPU comparison.
6. Run compilation, meaningful DCP tests, all production shader validation, and actual GPU parity (not just ignored tests listed as green). Exercise editor, export, thumbnails and CLI with profile off, base only, and a correctly paired Look.
7. Rebaseline the no-profile guarantee: compare the integrated fork to the NEW upstream commit with the same settings and decoder. Keeping August pixels bit-identical is incompatible with deliberately adopting upstream rendering changes; retain old fixtures as historical migration evidence.
8. Test Android cold startup, editor open/render, profile SAF import, activity recreation and low-memory behavior on devices. Distinguish build success from device acceptance. Verify Vulkan/GLES separately where relevant.
9. Record Adobe-reference fidelity as unverified until matched assets/reference renders exist; synthetic or GPU parity is not Adobe parity. The supplied Cobalt Flat DCP still does not satisfy Looks requiring Cobalt Modular unless additional assets have been installed.
10. Open an integration PR only after conflicts and source-level checks are resolved; report runtime/device/reference gaps explicitly before merge.

## Evidence limits

Pass: remote fetch, branch comparison, release lookup, relevant source/diff inspection, open-PR status lookup, non-mutating merge-conflict simulation.

Not exercised: compilation of an integrated tree, rendering, UI, Android hardware, Adobe comparisons. No claim that upstream or a future merge passes those gates.

Prior BUILD_REVIEW.md describes an older fork commit. Current HEAD includes PR #12 fixes; this review did not rerun or reconfirm its historical defects.
