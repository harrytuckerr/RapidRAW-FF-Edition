# UI1 main promotion and local delivery

Recorded 2026-10-16 (Australia/Melbourne) by the lead.

PR [#16](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/pull/16) merged at 2026-10-15T13:05:34Z. Exact reviewed PR head: `9b9b1b7933e39d2c347badf00a7a571ad0c843d5`; prior main: `5ca276ac1dab210f11ee4cecbf904038516164f8`; resulting merge: `42524cce5578503388667749a380fe3d204cce93`. The PR head and merge have identical Git trees: `c1313487fe90ec08810e36ca8b732769305a0db6`.

## Decision and checks

The user explicitly said, “don't worry about intel mac builds. I won't use it”. This removes the two Intel macOS builds from this promotion's gate. At promotion, all other 14 checks were successful on the exact PR head: Windows x64/ARM64, Apple Silicon standard/tethering, Linux builds, Android, frontend advisory lint, native format and native Clippy. The Intel standard and tethering builds were still in progress, not failed or passed. The remaining run `38732494156` was deliberately cancelled after promotion. Its completed successful checks remain distinct from the two cancelled Intel checks.

Duplicate main-push CI was requested cancelled (`38739806449`, `38739806393`) after verifying the merge tree exactly equals the checked PR tree. One lint cancellation request returned HTTP 502; a subsequent request was accepted. Cancellation is not a passing check. No workflow source, platform support declaration or branch protection was changed.

This is a scoped integration decision, not an Adobe-fidelity, full physical-profile-workflow, HDR, GPU-performance or release-readiness waiver. Canonical verification before promotion remains 46 frontend tests, 213 library tests and two CLI tests passing, format and strict native Clippy passing, with 70 inherited TypeScript diagnostics. See the canonical validation receipt for scope and ignored tests.

## Delivered artifacts

The local Mac installer and installed Fold APK were built from `fbc6b6e511814fdcbea46e3f5225f9bbc66f8973`. Only README and documentation differ between that source and the promoted PR head; the application inputs did not change.

| Artifact | SHA-256 | Evidence |
|---|---|---|
| Apple Silicon ad-hoc DMG | `a715ceb8cd90164b91bdce8f5f588d0485ade13f4def5b83df20cc88fa0526d2` | [Local Mac artifact](UI1-MAC-LOCAL-ARTIFACT-2026-10-15.json) |
| Android ARM64 debug APK | `6be831f069933305d4ecdff4ac928f79133ba289402b14e463ac63fa101b43f8` | [Android artifact and Fold update](UI1-ANDROID-FOLD-ARTIFACT-2026-10-15.json) |

Both are available in the user's Downloads folder under the names in those receipts. The Android APK was installed in place on `RFGL73CE9XM`; the installed hash matched and MainActivity cold-launched successfully. Metadata-scoped preservation checks passed; app data contents were not exhaustively compared. Mac installation/launch and revised profile/preset workflows through the real OS chooser, physical rendering, save/reopen and export remain Not exercised. The Mac package is ad-hoc signed, not Developer ID signed or notarized.

The existing public preview `fuji-preview-2026.10.15.1` remains unchanged and predates UI1. This promotion does not publish a new binary release. The packaging-notice inventory records outstanding attribution work before another public package is represented as distribution-ready. The README's distinction between current source and the older published preview remains accurate.

Drive model/graph implementation and desktop OAuth helper development remain in separate local worktrees. They are not enabled Google features in this main promotion. No Google login, credentials, account access or provider acceptance is implied.
