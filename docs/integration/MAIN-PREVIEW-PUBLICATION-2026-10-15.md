# Main promotion and development preview — 15 October 2026

The user explicitly requested merging the latest updates to `main` and deployment. PR [#15](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/pull/15) was merged at 09:58:10 UTC as `255fa978945513331de0c659a76b18b528496c45`. Its complete tree is `db1268edee6d4291d22ce929b9523b95e6a58982`, identical to the reviewed and tested source commit `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`.

This is a scoped integration and development-preview publication. Original Adobe fidelity, physical profile/export acceptance, HDR and production GPU performance requirements remain open. Approved JPEG/workflow and Drive additions remain backlog work.

## Source and build evidence

- **Pass:** all 16 PR checks completed successfully, including all 13 platform build jobs. [CI Build 38708083220](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/actions/runs/38708083220) completed successfully at 09:57:49 UTC on source `2a734622…`. [Lint 38708083173](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/actions/runs/38708083173) also succeeded on that source.
- **Pass:** canonical local validation recorded 208 native library tests, 2 CLI tests, five explicit RAW/import controls, 18 frontend import regressions and 16 profile regressions, frontend build, native formatting and strict native all-target Clippy. Fourteen default-ignored native tests are not counted as passes.
- **Fail — inherited:** frontend typecheck retains 70 unchanged baseline diagnostics; scoped frontend lint retains 37 inherited diagnostics, with none added. The hosted advisory frontend job succeeding does not make those underlying checks clean.
- **Documentation only:** publication follow-up changes README, NOTICE and integration evidence. Application/build inputs remain identical to the tested source; the release's `build-provenance.json` records the final main commit and exact changed documentation paths.
- **Cancelled deliberately, not Pass:** the merge triggered duplicate main CI Build `38716864401` and Lint `38716864308` on the byte-identical tree. The lead requested cancellation to avoid repeating the completed matrix. These runs are not acceptance evidence and were not cancelled to hide a failed result.

## Distribution boundaries

The preview downloads are published on this fork's [release page](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/releases/tag/fuji-preview-2026.10.15.1), with source provenance, SHA-256 checksums and license notices. The release remains a prerelease and is not marked latest stable.

Standard editor packages are selected. Optional tethering downloads are withheld: inspected Mac and Linux variants lack runtime dependency closure. The original CI archives and failed signature/dependency receipts are preserved. Successful tethering builds do not establish standalone distributability.

Mac preview packages use local ad-hoc signatures after copied-bundle sealing and independent payload review. This does not establish Developer ID, notarization, Gatekeeper trust or runtime acceptance. Windows installer stubs have no embedded PE certificate table; compressed installer payload/runtime was not independently exercised. AppImage internal payload/runtime inspection was not completed. See per-asset provenance for the exact scope.

The Android download is the source-matched, owner-debug-signed APK, not a production/Play artifact. No commercial DCP/Look, decoded vendor table, private RAW, credentials or signing key is intentionally distributed. Authentic upstream and third-party notices accompany the downloads.

## Physical Fold evidence and current profile question

The exact APK SHA-256 `b56d8bdcd7bc108197f4f34558ac43e368a5117290eb29fb1eeb5c35c006c516` was installed in place on `<DEVICE_SERIAL>`; installed hash, package and signing identity matched. Normal cold-launch command returned `Status: ok`. UID, first-install time and data-directory metadata were preserved; app-data contents were not compared. No uninstall, data clear, permission/settings changes or photo operations were performed by this install task.

- **Pass — user-reported:** app opens, UI and basic adjustments work; following this update, a RAF opens successfully.
- **User-reported unresolved:** no profiles appear for the opened RAF. This is not yet a proven filtering or rendering defect.
- **Not exercised:** the follow-up read-only registry query could not proceed because the phone was disconnected. Actual installed DCP/Look counts, IDs, pairing and current camera metadata remain unknown, not zero.
- **Verified source contract:** Android discovers managed profile imports; it does not inherit Lightroom profiles from the Mac. A photographer must import a DCP matching the source camera. A Look requiring Cobalt Modular cannot use Cobalt Flat as its base. Do not substitute profiles or disable pairing to populate the list.
- **Not exercised:** comprehensive physical profile/Look application, persistence, export, provider/native transport, fold continuity and Adobe comparison.

The historical emulator rendering failure remains a separate observed environment failure. The user's physical-device RAF/editing success does not erase it or establish the outstanding profile-specific cases.

## Remaining work

The comparison CLI still reports unavailable; statistics and candidate Lab helpers are groundwork, not a completed Adobe oracle. The production profile path remains CPU. Complete physical profile/export tests, real-camera Adobe references, the comparison harness, measured performance and the original support matrix before claiming the product vision is complete.

The inherited in-app update notice still points upstream. Use this fork's release page for Fuji Edition downloads until the separately queued update-link correction is reviewed and delivered.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `6a974e291db007f2f6e55b6d2a2ded3b2164192020db0424d3de8a77f53e3b02` (5875 bytes).
