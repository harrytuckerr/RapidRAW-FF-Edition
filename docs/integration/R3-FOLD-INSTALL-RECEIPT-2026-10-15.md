# Fold APK update receipt — 2026-10-15

**Pass:** verified preview installed in place, installed bytes matched, and one normal launch reported `Status: ok`. Sole device operator released ownership and stopped after this receipt.

| Check | Result |
|---|---|
| APK/device preflight and existing signer compatibility | Pass |
| Replace install and exact installed APK hash/size | Pass |
| Package/version/signing identity | Pass |
| UID/install-time/data-directory preservation metadata | Pass |
| Normal launch command | Pass |
| App-data contents, visible pixels, provider/UI/import/profile/export/Fold | Not exercised |

Actual model `gpt-6.1-sol`; source HEAD `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`. User authorization was the lead-relayed “fold is plugged in for apk install”, user message01a13ec7-c862-7252-9055-82eb832ecaf7. This was install/identity/normal-launch scope only.

Device: Samsung SM-F971B / h8q, Android17/API37, arm64-v8a, authorized target ending E9XM. Exact target/argv are in private machine receipts; every ADB command explicitly used `-s <DEVICE_SERIAL>`. No emulator or second operator was used.

Source APK `/private/tmp/rapidraw-r3-android-20261015/RapidRAW-R3-2a734622-android-debug.apk`:117,724,369bytes; SHA256 `b56d8bdcd7bc108197f4f34558ac43e368a5117290eb29fb1eeb5c35c006c516`. Host hash remained unchanged after install. Fresh apksigner and aapt verification both exit0.

Existing installed APK: version1.6.5/code1006005,207,525,759bytes, SHA256 `bf8d139932aba062ec4da0d8ffcf446d6465ecec96b258dd658e5aace018a546`. Its SHA256 matched the read-only historical host APK, whose signature was freshly verified. Existing/new signer certificate SHA256 `974dd044afa762e9c2fff3c42bf904ba3d292e3064cf243f8d94bc1450b278f8`; apksigner exit0. No downgrade/signing conflict was found.

The authorized install command was:

```sh
adb -s <DEVICE_SERIAL> install -r --no-incremental /private/tmp/rapidraw-r3-android-20261015/RapidRAW-R3-2a734622-android-debug.apk
```

Exit0, `Performing Streamed Install
Success`, elapsed4.668s. The installed base APK was queried with `pm path`, then read-only `sha256sum` and `stat`:117,724,369bytes and exact `b56d8bdcd7bc108197f4f34558ac43e368a5117290eb29fb1eeb5c35c006c516` equality. Package remains `io.github.CyberTimon.RapidRAW` version1.6.5/code1006005; public signing identity follows exact APK equality. Private installed paths appear only in task-specific raw receipts, not this summary.

Package UID, firstInstallTime, dataDir, ceDataInode and deDataInode were unchanged. Last update metadata was recorded. No app-data contents were read or compared; this preservation result covers the replace-install method and package/data-directory metadata.

The one normal launch command was:

```sh
adb -s <DEVICE_SERIAL> shell am start -W -n io.github.CyberTimon.RapidRAW/.MainActivity
```

Exit0, elapsed0.283s:

```text
Starting: Intent { cmp=io.github.CyberTimon.RapidRAW/.MainActivity }
Status: ok
LaunchState: COLD
Activity: io.github.CyberTimon.RapidRAW/.MainActivity
TotalTime: 238
WaitTime: 240
Complete
```

This establishes the launch command result only; no visible pixels, sustained app health, runtime image/provider/import/profile/export or Fold behavior was assessed. No taps/selections/imports, force-stop/reset/uninstall/data clearing, settings/permissions/Home/accessibility/other-app/photo writes, source/build/Git/release operations were performed.

Exact argv/exits/start times/elapsed times and task-specific sanitized metadata are in `receipt.json`, `apk-preflight.json`, `device-preflight.json`, `installed-before-signature.json` and `install-launch.json`; `artifact-hashes.json` freezes all evidence hashes. Failed receipts would be retained; no command failed in this install/launch task. The earlier Team mailbox schema-error probe did not operate the device and was corrected by inspecting the Team CLI contract.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `8ecc62b88a469569b654fcae6a0cb5830e843a08067e5329fdd8107fd1fd0d82` (3895 bytes).
