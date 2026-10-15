# Android integration and APK handoff — 2026-10-15

## Final build and validation status

**Pass — worker verified:** attempt6 ARM64 debug APK built, APK v2 signature
verified, packaged native library matched the built library, and relevant
prepared source matched main. No worker device mutation occurred.

**Pass — main reported:** attempt6 APK installed successfully on emulator-5584.
Main is testing that final APK; final editor/profile-picker runtime acceptance
has not yet been reported at this document's freeze. Earlier attempt5 had a
visible editor and no GPU validation errors on Vulkan SwiftShader. Those
results are not silently promoted to final-attempt runtime acceptance.

Main also reports the desktop full GpuProcessor 8/16-bit Metal hardware test
**Pass**, all tests **134 + 2 Pass**, and strict Clippy **Pass**. These are
main-reported desktop results, not Android device acceptance. The earlier main
runtime evidence includes genuine SAF RAW import/thumbnail and native DCP+XMP
import ("Imported 2 profiles") passing. Physical Android testing was not
exercised by this worker.

Final APK:
`/private/tmp/rapidraw-android-prep-20261015-v2/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`

- SHA-256: `797a270fcdaeb9327b875a65cfddd9b6f65e1885ce19691dfb9b304d57c63fee`.
- Size: 206,956,359 bytes (197.37 MiB). Application ID
  `io.github.CyberTimon.RapidRAW`; version 1.6.5, versionCode 1006005;
  minSdk 24, targetSdk 36; Android debug certificate.
- ARM64 `librapidraw_lib.so` and `libonnxruntime.so` packaged.
- Attempt6 exited 0 after 191 seconds using the retained Android target.
  14.84 GiB free at completion; no disk-reserve stop or cache deletion.
- Final GPU SHA-256 (worker verified and main reconfirmed):
  `d02767e76a565d2937b460fd3079e53e9ad67c850fd3d1146f9236b52069cb22`.
- Explicit GPU requirements include storage textures 2, bind groups 2 and
  uniform buffers 1. Final full GPU tests are included in the source snapshot.
- ProfileBrowser touch/popup fix SHA-256:
  `25489e41b4484a94a4e917a4aa052c453789422f01f62f01f194be00d0f72a67`.

Main reports importing the frozen script, workflows, Gradle/JNI source and
repository-local dependency locks; this document was deliberately held back.
This is the final documentation-only handoff, with its own receipt; it
supersedes the document entry in the earlier owned-source handoff. No further
source or dependency change, rebuild, or device action was performed for it.

Clean exported-source preparation **Pass** with the repository-local staged
lock bundle and latest script; no dependencies installed or duplicate target
compiled in that test. CI runner execution itself remains **Not exercised**.
Evidence: `scripts/android-clean-prepare-receipt-20261015.json`.

## Final source and dependency receipts

Main source: `/private/tmp/rapidraw-integration-20261015/working`, upstream
3385c9f0 integrated onto Cobalt 6f171d9b. The Git HEAD alone does not identify
the staged merged tree; use the file receipt.

- Full prepared-input receipt: `scripts/android-attempt6-source-receipt-20261015.json`.
- APK, log hashes, source hashes and toolchain receipt:
  `scripts/android-attempt6-build-receipt-20261015.json`.
- Source manifest SHA-256:
  `2421a9621f868db17d7d7976ef5b49e0117b55ca084daf918ba6ea594da5a6b3`.
- Prepared manifest SHA-256 (including Android overrides and lock bundle):
  `0af8a31f8e779f3cad4980549d2fda123ba81a453b7c267abec9216f32f8c831`.

The receipt includes main's final raw_processing.rs no-profile calibration fix
and the four mechanical updates: dcp/pipeline.rs, dcp/rgb_look.rs, apple_raw.rs,
and bench/replay.js. All five were checked byte-for-byte against main after
packaging. raw_processing.rs SHA-256:
`8db7b2209ad9fb5d2ceb9af2cb7c6023ee6d83b3b09e49a4eff744d1384bd160`.

Repository-local CI bundle (main reports imported):

- `src-tauri/gen/android/dependency-locks/Cargo.lock`
  SHA-256 `2958d08a0cdf5303f165ae0e9b05c2ba99aa902d428fab21db875165149c8c49`
- `src-tauri/gen/android/dependency-locks/package-lock.json`
  SHA-256 `1fab8b696139c37da88d1c76980799f210a4b00914cd91f717e3b100923bd37c`
- `src-tauri/gen/android/dependency-locks/lock-hashes.json` records the preparation
  state when frozen. The separate build receipt above records completed APK status.

Both actual Android locks remained unchanged during the successful build.
Main's desktop Cargo.lock and npm lock were never modified by this worker.
The frontend react-test-renderer 19.2.8 dependency and final test script are
included. No shared node_modules; npm ci ran only in the Android copy.

Exact npm version delta against main is in
`scripts/android-dependency-delta-20261015.json`:

- App API 2.12.1 → 2.11.1; CLI and platform CLI packages 2.12.1 → 2.11.4.
- HTTP plugin 2.8.0 → 2.7.0, matching the Android Rust HTTP plugin.
- HTTP 2.7.0 declares API ^2.12.0, so npm additionally resolves a nested API
  2.12.4. No out-of-range override was imposed; Tauri's check and packaging pass.
- No JS store/fs packages exist in the manifest or npm tree; none were added.
  Rust store/fs remain 2.4.0/2.5.2. Other Rust pins are unchanged from upstream's
  Android pin list, enforced by the build script.

npm audit reports source-map-js <1.2.2 high-severity indexed-source-map denial
of service (GHSA-68fv-2mgg-jv7q). No unrelated audit fix was applied.

## Build fixes and logs

Full logs are retained in `/private/tmp/rapidraw-android-prep-20261015-v2/`:

1. `android-build-20261015.log`: stopped before compile on Rust HTTP 2.7/npm
   HTTP 2.8 mismatch. Fixed the Android manifest policy and npm lock.
2. `android-build-20261015-attempt2.log`: native compile succeeded; packaging
   failed when Gradle's nested Cargo command selected host default Rust 1.97.1.
3. `android-build-20261015-attempt3.log`: successful APK, including the final
   raw calibration fix. Wrapper now passes RUSTUP_TOOLCHAIN from the declared
   src-tauri/rust-toolchain.toml to every descendant, including Gradle.

4. `android-build-20261015-attempt4.log`: cancelled on main instruction when
   review identified the two-storage-texture requirement for flare ghosts.
5. `android-build-20261015-attempt5.log`: built the first GPU/touch-fix APK;
   main reported visible editor and no GPU validation errors.
6. `android-build-20261015-attempt6.log`: final APK from exact current GPU
   source, including explicit bind-groups/uniform-buffer limits and GPU tests.
   Signature evidence: `apk-verification-attempt6.log`.

The first native compile took 10m45s. Subsequent builds reused dependency
artifacts in the same target; final attempt6 completed in 191 seconds. Tauri's Gradle pipeline invokes
Cargo again for packaging; no second independent target/check build was started.
Rust emitted 15 Android warnings (unused desktop-oriented items); Gradle/Kotlin
reported deprecations. Neither was treated as a clean lint result.

Tauri CLI 2.11.4 help and matching source accept `android build ... -- --locked`;
the actual nested Cargo command in the log includes --locked. Source:
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/mobile/android/build.rs

Tauri adds image-association filters to the prepared AndroidManifest.xml during
packaging. All original intent-filter structures, including DCP/XMP VIEW/SEND,
were verified preserved. The input and generated manifest hashes are recorded
separately. Do not mistake that expected generated edit for a main-source change.

## Reproduction and disk policy

The final used workspace is `/private/tmp/rapidraw-android-prep-20261015-v2` and
the only Android Cargo target is its `android-target/` subdirectory. Main's
`/private/tmp/rapidraw-integration-20261015/target` is separate and untouched.

For a future clean clone with the bundle imported, prepare exactly once:

```sh
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/26.3.11579264"
export CARGO_BUILD_JOBS=4
export GRADLE_OPTS=-Dorg.gradle.daemon=false
python3 scripts/android-build.py prepare \
  --workspace /private/tmp/rapidraw-android-next \
  --locks src-tauri/gen/android/dependency-locks
python3 scripts/android-build.py build \
  --workspace /private/tmp/rapidraw-android-next
```

Do not rerun this now to create a duplicate target. Refresh the existing copy
and its receipt if further source corrections are required. The build wrapper
rejects changed prepared inputs; generated manifest additions must be restored
to the frozen input before a repeat or recorded by a deliberate source refresh.
The current successful generated manifest is retained for inspection.

Build environment enforces dev/test debug=0, incremental=false and
CARGO_INCREMENTAL=0. Script checks free space before dependency operations and
build starts. The actual runs additionally used a supervisor checking every
five seconds and terminating only its process group below 6 GiB free. This
continuous watchdog is execution supervision, not a feature of the checked-in
wrapper. Keep it for any retry while disk is constrained. No user caches or
existing 17 GiB target were removed. No commits/pushes performed.

## Provisioning and device boundary

Provisioned with escalation: NDK r26d 26.3.11579264 and Rust 1.98
`aarch64-linux-android` target. Google archive SHA-1 matched
`cebe7af9bdadc67b8f919d549e9ab269831fd2c9`; archive 990,809,031 bytes, expanded
2.94 GiB. Installation used a 6 GiB peak budget and 10 GiB reserve. Older NDKs
and caches were preserved. JDK17, SDK36, build-tools36.0.0, Gradle8.14.3,
AGP8.11.0 and Kotlin1.9.25 were used. App build.rs verified/reused ARM64 ONNX.

Read-only inventory showed emulator-5584 online, ARM64 API33, WebView
101.0.4951.74, RapidRAW absent; no physical device. Inventory is pre-install,
not a current package-state guarantee after main begins testing. No worker
install, launch, reset, uninstall, permission change or user-data mutation.

Main should test startup/resume/recreation; profile SAF picker cancel/success;
valid and malformed DCP/XMP; uppercase names; cold/warm VIEW/SEND with genuine
provider-granted content URIs; duplicate imports and persistence; RAW render
and export. Include the no-profile CFA/linear calibration path. Emulator results
cannot establish physical GPU/fold/provider acceptance. Record the old WebView
when diagnosing UI issues; current-provider coverage remains separate.

## JNI source fix boundary

The scoped context fix retains a process-lifetime application GlobalRef instead
of borrowing wry's replaceable Activity reference. See upstream
https://github.com/CyberTimon/RapidRAW/pull/1806 for the related lifetime report.
It compiled for Android here, but recreation/runtime acceptance remains open.

PR https://github.com/CyberTimon/RapidRAW/pull/1911 addresses tao0.37 double-init
using a process-wide panic-hook probe. That unmerged change was not imported.
This build supports pinned tao0.35.3/Tauri2.11; unpinned Tauri2.12 Android needs
separate context ownership review and runtime validation before removing pins.
