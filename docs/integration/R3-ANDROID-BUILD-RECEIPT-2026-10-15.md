# R3 Android build receipt — 2026-10-15

**Pass:** one fresh wrapper build compiled the reviewed Android source and produced a verified debug APK. This is compilation/package evidence only. The builder is released; all work stops at this receipt.

| Check | Result |
|---|---|
| Frozen source, preparation and lock custody | Pass |
| Android compilation and APK packaging | Pass |
| APK identity, signature and packaged native/runtime hashes | Pass |
| Install, real Tauri transport, provider/import, UI/Fold | Not exercised |
| Physical profile preview/export, RAW support, Adobe acceptance | Not exercised |

## Source and ownership

- Actual model: `gpt-6.1-sol`; host `macOS-26.5.2-arm64-arm-64bit` / `arm64`.
- Read-only source: `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/lead`; clean HEAD `2a7346229bc0cc398ee7e3fee126eb89428b7dd9` before/after.
- All 533 source hashes match before/after. Manifest SHA256 `ef47ae3cafb1ef4427f0ad1f729a8deef8673c6fba8ea006e037330eaceaa576`.
- Native reviewed file SHA256 `56ca21c1488f0e72fde52796e3b3fa97e1a3179f56d467cce599e615ad09d81e`. Four frontend hashes are in `receipt.json` and `source-before.json`.
- Fresh workspace: `/private/tmp/rapidraw-r3-android-build-20261015`; evidence: `/private/tmp/rapidraw-r3-android-20261015`; both created for this task with mode0700.
- Wrapper preparation changed only the declared package.json pins and the Android npm/Cargo lock overlays. Every pre-build prepared hash matched. The leader independently checked preparation identity, separately from build acceptance.
- Tauri generated file-association intent filters in the new workspace AndroidManifest.xml during build. `generated-manifest.diff` and `prepared-after.json` preserve that generated delta; canonical source was unchanged. No implementation/configuration patch was made.

## Commands and environment

```sh
cd <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/lead
python3 scripts/android-build.py prepare --source <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/lead --workspace /private/tmp/rapidraw-r3-android-build-20261015 --locks src-tauri/gen/android/dependency-locks
python3 scripts/android-build.py build --workspace /private/tmp/rapidraw-r3-android-build-20261015
```

Prepare exit0 (0.519s); build exit0 (834.590s). Exact argv, timestamps and environment are in `prepare-result.json`, `build-result.json` and `run-build.py`.

- Rust1.98.1/cargo1.98.1, aarch64-linux-android; JDK Zulu17.0.17; NDK26.3.11579264; SDK/platform/build-tools36; Node22.22.0/npm10.9.4; Gradle8.14.3.
- JAVA_HOME=/Library/Java/JavaVirtualMachines/zulu-17.jdk/Contents/Home; ANDROID_HOME=<HOME>/Library/Android/sdk; NDK_HOME=that SDK/ndk/26.3.11579264.
- Wrapper: CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0, both CARGO_PROFILE_*_INCREMENTAL=false, CARGO_INCREMENTAL=0, private android-target. Cargo --locked; npm ci; no resolve/clean or dependency refresh command.
- Cargo/npm download caches and Gradle module/wrapper downloads were APFS copies into private cache homes; no old compiled target reused or old cache intentionally opened for writing. No Cargo credentials/configuration or account material selected for copying. Existing debug key reused privately; no key bytes in evidence/logs. Private Gradle options keep Kotlin compilation in-process and persistence in the new workspace.
- Cargo/npm forced offline. Gradle was not explicitly forced offline; no download message was observed, and network traffic was not measured. Verified ONNX runtime was read-copied; build log confirms download skipped.
- A pre-build private-cache cleanup initially matched dependency Cargo.lock files; all673 were restored exactly from the immutable originals before the build. Only the copied Gradle process lock was omitted. Full audit: cache-lock-restoration.json. No source/overlay lock was affected.
- Free space: 36.59GiB before preparation, 36.51GiB before build, 31.83GiB at final receipt; >=6GiB reserve maintained at recorded checks.
- One wrapper invocation includes its normal Rust/Gradle native steps (8m39s and1m56s in the log); 15Rust warnings per invocation plus existing Gradle/Kotlin deprecations. No source fixes applied; no tests run in this artifact-only task.

## Frozen artifacts

- APK: `/private/tmp/rapidraw-r3-android-20261015/RapidRAW-R3-2a734622-android-debug.apk`; 117,724,369bytes; SHA256 `b56d8bdcd7bc108197f4f34558ac43e368a5117290eb29fb1eeb5c35c006c516`.
- Package `io.github.CyberTimon.RapidRAW`, version `1.6.5` / code `1006005`, minSDK24 / targetSDK36; sole native ABI arm64-v8a. Package and version match canonical tauri.conf.json.
- `apksigner verify --verbose --print-certs`: exit0; APKv2 verification Pass. Signer certificate SHA256 `974dd044afa762e9c2fff3c42bf904ba3d292e3064cf243f8d94bc1450b278f8`, equal to the existing historical debug identity. `aapt dump badging`: exit0.
- Built and packaged librapidraw_lib.so: 89,110,080bytes, SHA256 `6cb411c247e5b8783c21f77f6c764020a90591989f3b7cafdf255bbe192967ad`; exact equality verified.
- Packaged ONNX runtime: 18,214,224bytes, SHA256 `999ecfdb5b5a13e4097487773b6d71ce8a075408a237daab072e8f5e817bd78e`; exact equality with the verified private input.
- Android Cargo.lock SHA256 `2958d08a0cdf5303f165ae0e9b05c2ba99aa902d428fab21db875165149c8c49`; npm lock SHA256 `1fab8b696139c37da88d1c76980799f210a4b00914cd91f717e3b100923bd37c`; both unchanged by build.
- Full raw/sanitized logs, source/lock manifests, public signature logs, exact commands, toolchain inputs and artifact hashes are retained here. No private photo/commercial assets were used, no device commands/install/signing configuration/source/Git mutation occurred.

Builder processes ended and exclusive builder ownership is explicitly released. Worker completion does not establish provider, UI, device, decoding, visual, profile or production acceptance.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `fa07defc89766044d321c3009bfebcd9cf8ee0111eb91d8a3f59095c3699c2ea` (5916 bytes).
