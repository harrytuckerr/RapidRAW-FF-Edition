# H1a — Fold read-only preflight and next-test preparation

Receipt dated 2026-10-15. Task `01a13e07-7da5-7970-a3be-7cc48f0a292c`; sole operator slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`. Model `gpt-6.1-sol` as reported by the Team runtime roster. No sub-agents used.

## Outcome and boundary

**Read-only preflight Pass after USB reconnection. H1b has not started.** Installed APK is byte-identical to the known attempt-8 debug APK. Synthetic inputs and the minimal next flow are ready. At 05:31:41 UTC the phone was awake, CLOSED, with another app foreground; RapidRAW was already running in the background. An available test window cannot be inferred from connectivity. Lead must dispatch H1b separately and coordinate the interaction window before taking over the foreground.

Only this new document and private evidence under `/private/tmp/rapidraw-fuji-H1-20261015` were written. That directory was created new with mode 0700. No source/lock edits, staging, commits, pushes, emulator operations, device pushes, taps, launches/relaunches, force-stop, install/uninstall, reset, permissions or settings changes occurred. Installed APK was pulled read-only into private evidence. No screenshots, user photos, commercial profiles or full personal logs were copied. App-log capture retains only GPU/profile-stage lines; no photo paths or content were retained.

## Frozen input and environment

- Snapshot: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/device`, detached at `4c1e965a53de9c835df920b2e3d69c2c8a864708`; tree `9f36c891f35d9310a7977bda7d851178ad92039d`.
- `git diff --exit-code 8e6318814d42e155d88a1d0efd485f9d571b4ffb HEAD`: exit 0, empty. Initial working tree clean.
- Host: macOS 26.5.2 / Darwin 25.5.0, Apple arm64; Python 3.9.6, Node 22.22.0, ADB 1.0.41 / 33.0.3-8952118. Host verification used SDK build-tools 36.0.0 and Zulu JDK17. No build or dependency resolution run.
- Read full original `ORCHESTRATOR_HANDOVER_2026-10-15.md`, historical `docs/integration/STATUS-2026-10-15.md`, `docs/BUILD.md`, attempt-8 build/source receipts, `scripts/integration-fixtures.py` and the lead continuation ledger. The original handover's old PR/physical/emulator status is historical; the lead ledger supersedes it where dated.
- Attempt-8 source receipt SHA256 `35fcc77ba1f3cc663255aab0a7184f2f3fc4243706966c374a2bfc9192bbffde`. Loaded all 459 file records and compared content: 456 match, none missing. Three differences are `docs/android-integration-2026-10-15.md`, `docs/integration/STATUS-2026-10-15.md`, `docs/integration/evidence/desktop-render-receipt.json`; no production-source or lock mismatch. Exact hashes are in `source-receipt-verification.json`. The receipt's old staging `source_head` is not used instead of its content hashes.

## Device/package receipt

Read-only capture interval: 05:31:40–05:32:11 UTC, after an initial `get-state` failure at 05:28:35 UTC (`device not found`). Lead relayed the user's “connected now”; one fresh attempt at 05:31:08 UTC returned `device`.

| Item | Observed value / scope |
|---|---|
| Authorized serial | `RFGL73CE9XM` (`get-state=device`, matching `get-serialno`) |
| Device | Samsung `SM-F971B`, device `h8q`, `arm64-v8a` |
| OS | Android 17, API37; security patch 2026-10-05 |
| Build fingerprint | `samsung/h8qxxx/h8q:17/CP2A.260605.016/F971BXXS3AZIJ_OXM3AZIJ:user/release-keys` |
| WebView | `com.google.android.webview` 155.0.8059.43, versionCode805904303, target37, installed/enabled |
| Activity/power | `com.instagram.android/com.instagram.mainactivity.InstagramMainActivity` top-resumed; Awake, powered. This is an observation, not proof of touch activity or permission to interrupt. |
| Posture/display | Device-state base/committed name CLOSED, identifier0; `wm size` physical1248×1972. No hinge/fold transition exercised. |
| GPU hardware/driver | SurfaceFlinger: Qualcomm Adreno(TM)840, OpenGL ES3.2 `V@0842.19.11`, GIT2e66171171, I1fac26e44e, driver date05/01/26. This system GLES receipt is separate from RapidRAW's backend. |
| Actual app backend | Existing private app log at device-local15:37:56: `GPU processing adapter 'Adreno (TM) 840' backend Vulkan: storage textures 8388608, storage buffers 8388608, compute invocations 1024`. Historical session log, read now; no new render triggered. |
| App process | Already-running PID24047; existing `@webview_devtools_remote_24047` socket observed. PID/socket must be rediscovered on H1b; no forward or CDP connection created. |
| Package | `io.github.CyberTimon.RapidRAW`, v1.6.5, versionCode1006005, minSdk24, targetSdk36, DEBUGGABLE; installed and not stopped for user0 |
| Installed base APK | 207,525,759bytes; SHA256 `bf8d139932aba062ec4da0d8ffcf446d6465ecec96b258dd658e5aace018a546` |
| Local preserved APK | Same size/hash at historical integration `src-tauri/target/integration-review/RapidRAW-android-debug.apk` |
| Signing | Pulled installed APK and local APK both verify, one signer, APK signature schemev2; Android Debug certificate SHA256 `974dd044afa762e9c2fff3c42bf904ba3d292e3064cf243f8d94bc1450b278f8` |

The retained app-log tail contains completed GPU main-submit/readback callbacks from the user's earlier session. These support existing GPU-stage execution only: no profile choice, image pixels or export was observed here. Device log clock/timezone was not aligned to host; use host UTC capture timestamps for ordering and do not interpret those stage timings as a measured H1 benchmark.

## Acceptance ledger

| Case | Result | Evidence limit |
|---|---|---|
| USB available initially | Fail | First attempt device not found; preserved receipt, later recovered |
| Serial/OS/WebView/installed APK/signing identity | Pass | Read-only current capture and pulled APK |
| GPU/backend inventory | Pass | System driver plus existing app adapter log, no new processing |
| Frozen production source and known APK provenance | Pass | Content/hash checks described above; not workflow acceptance |
| Local synthetic generation/identity/determinism | Pass | Five original generated files byte-identical on second generation; task pair metadata/table checks below |
| App opens | Pass — user-reported | Handover user clarification only; not launched by H1a |
| UI functional | Pass — user-reported | Same report |
| Basic adjustments | Pass — user-reported | Same report; input type unspecified |
| Physical DCP/Look preview; Amount0/100/200 | Not exercised | No UI/input/render action |
| Physical SAF, save/reopen, export, negative persistence | Not exercised | Prepared only |
| Matched commercial RAW/base/Look and Adobe fidelity | Not exercised | No camera-matched real RAW/reference verified |
| Fold continuity, TalkBack, sustained performance | Not exercised | Inventory does not exercise these cases |

## Synthetic fixtures, kept outside Git

`python3 scripts/integration-fixtures.py --output-dir /private/tmp/rapidraw-fuji-H1-20261015/fixtures` exited0. Repeat into `fixture-repeat` exited0 and produced 5/5 byte-identical originals. Local evidence script `prepare-fixtures.py --output-dir <new-child-directory>` reproduces the task-specific derivatives from the same generator without editing it. Executed it into `fixture-reproduction`: all 10 original/derived fixtures match the first set byte-for-byte.

Task pair: camera `RapidRAW Integration Camera`; base `H1 20261015 Look Base`; Look UUID `H1-20261015-SYNTHETIC-LOOK`, display `H1 20261015 channel swap`, exact required base `H1 20261015 Look Base`. Only an equal-length DCP ProfileName string and XMP display/pairing/UUID metadata changed from the known generated pair; the embedded RGB table remains byte-identical. Independent TIFF/XML inspection verifies camera/name match and table presence. Native product import/evaluation of these derivatives remains Not exercised. Namespace prevents reliance on old synthetic imports being absent; verify this new required name is absent before the missing-base assertion rather than deleting an existing profile.

| File | Bytes | SHA256 |
|---|---:|---|
| h1-look-base.dcp | 280 | `9d79c5177d50d5c5533742b9a8f2e5a1503de9f32e9dfdb7df06062996d86a6d` |
| h1-look.xmp | 604 | `f818cf9f6ed59465bb7117c69bbe2720b6cdb8edc70879859e52d7489da2e904` |
| h1-malformed.dcp | 10 | `a920583e6f2bcf42a98f8418329ca40a73082f1c15d94f46f52e5d519cb1dabf` |
| h1-malformed.xmp | 36 | `ad61e6b7a047a4fd23c32dbb6184c8749ea904798ea1f9a1cc6c17f0eb45f174` |
| h1-uppercase-base.DCP | 280 | `9d79c5177d50d5c5533742b9a8f2e5a1503de9f32e9dfdb7df06062996d86a6d` |
| neutral.json | 3 | `ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356` |
| synthetic-base.dcp | 216 | `49eb76d49a882ecbf3b51dc8b052f7d51a6d62a68c6200d07fb07303a869ad8f` |
| synthetic-camera.dng | 8708 | `b2116c8ba983cca8a183a904fb12217307f2c6c9aa0fa76d7be02f30be20b53f` |
| synthetic-look-base.dcp | 280 | `98d6393220d564932f450aa6b88d07a11bcc4b421963dc149140e217c835ca86` |
| synthetic-look.xmp | 602 | `b3e84f53f285c9d5ff6d80e728b3e6468d1ef5932cdc4e7d010286215bc4f9db` |

`h1-uppercase-base.DCP` deliberately has a distinct stem: this host filesystem cannot retain same-stem files differing only by case. It is byte-identical to `h1-look-base.dcp`. The metadata inspector exits0, but its hardcoded `Table_[A-F0-9]+` regex reports `table:none` for symbolic synthetic `Table_INTEGRATION`; independent XML inspection confirms the unchanged, nonempty payload. Do not mistake that reporting limitation for native decoder validation.

## Minimal next H1b flow (prepared, not executed)

1. Recheck exact serial, activity, posture and PID after the lead supplies a coordinated window and dedicated H1b scope. Record current app/editor state without modifying an existing photo. Use only task-owned inputs/outputs and unique names in a dedicated device folder such as `/sdcard/Download/RapidRAW-H1-20261015`; push/import actions require H1b scope. Preserve existing imports and edits. Verify installed APK hash remains the one above.
2. Import task RAW and only `h1-look.xmp` through actual Android SAF. Verify the camera and required base name; the Look is visibly disabled with a missing-base/no-profiles explanation and cannot substitute another base. Check registry/UI absence first. Do not remove a pre-existing user profile to manufacture the case.
3. Import `h1-look-base.dcp` via the profile SAF picker; record picker completion and correct pairing response. The Look must enable for this camera. Select base only and wait for completed image pixels; capture identical image-region framing and export a base16-bit TIFF if this APK exposes it. Keep no-profile output as a separate baseline.
4. Select task Look; exercise Amount0,100,200 using real controls and wait for each current request. Record base ID, Look UUID and amount with completed image-region captures and uniquely named exports where supported. Amount0 decoded output must equal base-only under identical deterministic settings; it is not the no-profile result. Amount100 must yield the intended synthetic change, and200 must complete with finite readable output. The swap acts in ProPhoto before conversion/ordinary processing: do not assert a direct display-R/G swap from screenshot channels. H2 owns quantitative colour/cross-route acceptance.
5. At100, apply a bounded ordinary exposure change (e.g.+0.5EV). Confirm actual pixel change plus unchanged base/Look/amount. Apply one existing ordinary preset without captured profile only if its task scope is explicit; it must preserve selection. Masks, creative LUT and capture-opt-in remain follow-on full-H1 cases rather than implied passes.
6. Save through the real application route, navigate away and reopen the task RAW. Read back the task `.rrdata` selection (base canonical lowercase64-hex, Look UUID string, amount) where accessible. Confirm actual preview and ordinary edit remain. Repeat save/reopen at Amount0 to catch falsey-zero loss; return to100 for edited export. UI label persistence alone is insufficient.
7. Export through the actual Android UI/destination picker to unique task filenames; pull completed task outputs read-only for hash/dimensions/decodability/settings checks and later H2. Confirm positive byte count and completed write, preserve original DNG hash. If16-bit TIFF/destination is unavailable, record exact limitation; JPEG alone cannot prove exact Amount0 equality or ΔE. Compare decoded pixels separately from container/metadata hashes.
8. Stop at completed preview/save/reopen/export receipt or the first reproducible failure. Preserve source/APK/log markers and current-request/error evidence; report to lead for exact owner assignment. A cold process restart, force-stop, removal/corruption negative and fold/unfold need an explicit bounded follow-on scope; do not silently include them in this preflight.

## Persistence/import negative cases to dispatch after the positive flow

| Case | Required observation / preservation rule |
|---|---|
| SAF cancel | Selection, registry and task save unchanged; no false success |
| Duplicate + uppercase import | Same content-derived base ID; case-insensitive extension supported; no incorrect pairing. Record actual registry/file behaviour rather than assuming no physical duplicate copy. |
| Malformed DCP/XMP | Prepared invalid files fail with visible per-file reason and no partial selection/acceptance; XMP EOF must not hang |
| Saved Amount0 | Reopen preserves exact zero and Look UUID; base retained; pixels match base |
| Legacy IDs | On task-sidecar/preset copies only, exact32-integer-byte ID loads and next save writes64 lowercase hex. Never patch the user's existing sidecar. |
| Malformed ID | Task copies with31/33 items, fractional/out-of-range bytes, nonhex/wrong-length strings visibly error; no silent reset/fallback. Preserve originals and restore task copy after each case. |
| Selected base subsequently unavailable | Remove only the task-owned imported base through a supported app route under follow-on scope; saved reference produces visible preview/export failure, no silent no-profile render. Reimport exact bytes to restore. |
| Missing Look / inaccessible task source | Follow-on scoped task-copy mutation only; preserve edits, clear current error appropriately, do not export stale pixels or claim success. |
| Rapid switch / newer image | A stale failed request cannot clear/toast over the newer successful image; camera mismatch rejected without altering the newer selection. Real second-camera RAW remains an input dependency. |
| Background/resume, cold restart, fold transition | Separately coordinated lifecycle cases; navigation reopen is not process-death or fold-continuity acceptance. No clear-data/reinstall workaround. |

## Automation path and blockers

- Genuine Android SAF and export pickers: observe screen/UI hierarchy, then use bounded native input against current geometry. H1a performed neither UI dumps nor screenshots/taps. Avoid fixed blind coordinates; cover-screen dimensions may change.
- Existing debug WebView socket was observed for PID24047, and host Node22 exposes built-in `WebSocket`. H1b can rediscover the PID/socket and create a task-owned host forward, inspect `/json/list`, and drive real DOM controls/listeners with CDP. Native SAF remains a real Android picker; direct backend `invoke` would not establish picker/UI acceptance. No dependency install or source instrumentation is necessary for this candidate route. CDP attachment itself is Not exercised; retain screenshot/native input fallback.
- Planned commands, **not run in H1a**: `adb -s RFGL73CE9XM forward tcp:9223 localabstract:webview_devtools_remote_<fresh-pid>`; `adb -s RFGL73CE9XM exec-out screencap -p`; `adb -s RFGL73CE9XM shell uiautomator dump /dev/tty`; scoped `adb -s RFGL73CE9XM shell input ...`. Remove only the task-owned forward afterward. Never redirect to an emulator or change debugging/settings to make a path work.
- Technical preflight prerequisites are satisfied. **Unattended interactive H1b cannot start now:** last observation had another foreground app; the lead still needs to dispatch H1b and coordinate its window. No new build is needed for the initial synthetic test. Whether to cold-restart/remove task dependencies belongs to the follow-on assignment, not inferred authority.
- Commercial assets remain private. Supplied X-Pro2 Flat does not satisfy Modular Looks. Lead message reports its read-only IFD verification of the newly found private X-Pro2 Modular: cameraFujifilm X-Pro2, ProfileNameCobalt Modular, embed policy2, SHA256 `4b80ed63b210e5738cde04430af1511c3e12e2bb53bf2471b4bb0750dceb87f3`; H1a did not copy or independently validate it. Lead identifies `IMG_0138.DNG` as iPhone13Pro/iPhone14,2, unsuitable for the Fuji base. Matching real Fuji RAW and controlled Adobe reference remain unconfirmed. These do not block the synthetic H1b slice.

## Exact commands and private evidence

All device argv/UTC/exit codes and sanitized outputs are in `preflight-receipts.json`; the read-only collector is `collect-readonly.py`. Every device command explicitly targets `-s RFGL73CE9XM`. Host ADB version inspection was informational and did not address a device.

- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM get-state` → 1 (authorized).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM get-state` → 0 (final-authorized-recheck).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM get-serialno` → 0 (serial).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.product.manufacturer` → 0 (ro.product.manufacturer).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.product.model` → 0 (ro.product.model).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.product.device` → 0 (ro.product.device).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.build.version.release` → 0 (ro.build.version.release).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.build.version.sdk` → 0 (ro.build.version.sdk).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.build.version.security_patch` → 0 (ro.build.version.security_patch).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.build.fingerprint` → 0 (ro.build.fingerprint).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.product.cpu.abilist` → 0 (ro.product.cpu.abilist).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.hardware.egl` → 0 (ro.hardware.egl).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell getprop ro.hardware.vulkan` → 0 (ro.hardware.vulkan).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys activity activities` → 0 (foreground).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys power` → 0 (power).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys device_state` → 0 (device-state).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell wm size` → 0 (screen-size).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys webviewupdate` → 0 (webview).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys SurfaceFlinger` → 0 (gpu-surfaceflinger).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys package io.github.CyberTimon.RapidRAW` → 0 (package-version).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell pm path io.github.CyberTimon.RapidRAW` → 0 (package-path).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell pidof io.github.CyberTimon.RapidRAW` → 0 (pid).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM logcat -d -v threadtime --pid=24047` → 0 (existing-pid-gpu-log).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW tail -n 300 logs/app.log` → 0 (existing-private-app-gpu-log).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM pull '/data/app/~~ieiGAy7_8849jYMayc-N8g==/io.github.CyberTimon.RapidRAW-WrxlkEeXfi7-1TkJtgh_VQ==/base.apk' /private/tmp/rapidraw-fuji-H1-20261015/installed-base.apk` → 0 (pull-installed-apk).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW grep GPU logs/app.log` → 0 (existing-adapter-log).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell cat /proc/net/unix` → 0 (existing-webview-debug-socket).

Host verification commands: `JAVA_HOME=/Library/Java/JavaVirtualMachines/zulu-17.jdk/Contents/Home /Users/harrisontucker/Library/Android/sdk/build-tools/36.0.0/apksigner verify --verbose --print-certs <APK>` (local preserved and pulled installed APK, both exit0); SDK36.0.0 `aapt dump badging <local-APK>` exit0; SHA256 via Python streaming hash; `python3 tools/inspect_dcp.py <h1-look-base.dcp>` and `python3 tools/inspect_look_xmp.py <h1-look.xmp>` both exit0; `git diff --exit-code <tested-SHA> HEAD` exit0. Full argv/output in the corresponding JSON receipts. First host hash attempt used unavailable `hashlib.file_digest` on Python3.9.6 (exit1); corrected to streaming SHA256, then completed. This was a host helper compatibility error, not a product test failure.

Private evidence includes `environment.json`, `preflight-receipts.json`, `source-receipt-verification.json`, `local-apk-hash.json`, `local-apk-metadata.json`, `installed-apk-hash.json`, `installed-apk-signature.json`, `local-checks.json`, fixture identity/table/repeat/manifest receipts, fixture files, two helper scripts and pulled `installed-base.apk`. `evidence-manifest.json` records their hashes; the report hash and new-file diff are in `report-receipt.json` and `fold8-preflight.patch` outside Git. No production tests were rerun and no acceptance inherited from a build.

Next consumer: lead independent review/import of this allowlisted document, then sole-device H1b dispatch during the coordinated window. H1a stops at this receipt.
