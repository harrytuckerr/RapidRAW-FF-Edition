# H1R-device — X-T5 open failure, read-only device evidence

**The logged failure targets a staged `.RAF` whose first bytes are JPEG/JFIF.** It is not yet evidence that this build cannot decode a genuine Fujifilm X-T5 RAF. The original selected source, its MIME type/bytes and the exact content URI are unavailable in the captured logs, so the boundary at which the filename/content diverged remains unresolved.

## Task/provenance and limits

- Task `01a13e10-29dc-7192-bcbc-09806a1726a4`; sole device operator slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`, model `gpt-6.1-sol` per current Team roster. No sub-agents.
- Snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/device`, frozen HEAD `4c1e965a53de9c835df920b2e3d69c2c8a864708`.
- Host macOS26.5.2 arm64, Python3.9.6; same ADB33.0.3 host as H1a. Device serial `RFGL73CE9XM`, current RapidRAW PID24047; serial-specific `get-state` returns `device`.
- H1a preflight document remains unchanged: SHA256 `40d3d15669d650b4bf6ef9cd411dfb58236919204eb2b8c0c46b5d1f85b7637e`. That prior receipt verified the installed attempt-8 APK SHA256 `bf8d139932aba062ec4da0d8ffcf446d6465ecec96b258dd658e5aace018a546`; H1R did not pull/re-hash the APK again.
- Private evidence is in newly created mode0700 `/private/tmp/rapidraw-fuji-H1-20261015/raf-diagnostic/`. Device photo paths/basename are replaced here by `<APP_MEDIA>/.library/<FAILED_RAF>`; exact task identifiers/argv are retained only in private JSON receipts.
- No UI taps, launches, force-stop, install, log clearing, data/settings/permission/device-file mutation, source/lock/Git changes, builds or emulator operations. No ADB forward/CDP connection was needed. User remained free to test the phone.
- No complete photograph was transferred. Only the exact failed staged file's first64bytes were streamed; SHA256 was computed on-device. No arbitrary photo search/copy was performed.

## Observed failure, with time/call context

User report relayed by lead: an X-T5 `.raf` was imported, then opening it from RapidRAW's picker failed with “No decoder found”. Camera/source/import action is **User-reported**, not independently repeated by this operator.

Existing private app log lines4715–4726 and current-PID logcat independently establish:

| Device-local time | UTC from logcat epoch | Observation |
|---|---|---|
| 16:34:25 | 05:34:25.722 | `Error developing RAW file '<APP_MEDIA>/.library/<FAILED_RAF>': Error: No decoder found, model '', make: '', mode: ''` |
| 16:34:25–16:34:27 | 05:34:25.857–05:34:27.348 | Three further RAW-development errors for the same path; each preceded by a file-is-locked fallback/memory-map warning |
| 16:34:27 | 05:34:27.351 | `[frontend] Failed to load image: Error: No decoder found, model '', make: '', mode: ''`; same error in Tauri console |
| 16:35:00 | 05:35:00.253 | Fifth RAW-development failure at the same staged path, again after memory-map fallback warning |
| 16:35:00 | 05:35:00.254–.255 | Frontend/Tauri-console error repeats |

Private `initial-receipts.json` records host capture UTC05:38:46–05:38:47, PID24047, logcat epoch values and sanitized relevant context. It does not contain the whole personal app log. The earlier nearby GPU preview completion is a different operation; it does not demonstrate success on this failed file. The activity query returned no matching top-resumed line; H1R does not infer a foreground owner or screen availability from that empty filter.

The file-is-locked warning precedes a normal-read fallback in the recorded error sequence; it is not proof that locking is the root cause. This task did not rerun or modify the read path.

## Exact failed staged-file identity

The file path is unambiguous in five logged RAW-development errors. Reads used the app's existing `run-as` identity against that one path.

| Item | Read-only observed value |
|---|---|
| Storage relation | App media `.library` local staged file, rather than a `content://` URI at decoder entry |
| Size | 2,940,309bytes |
| SHA256 | `82bd1e04633c491824b90cc31486b5c3ff0770a6dc712edf95d74c691bc958c6` |
| Stability | Before/after stat identical: size2940309, mtimeepoch1792042465, inode359459 |
| Header bytes received | Exactly64, exit0; private `failed-header64.bin` |
| Prefix | `FF D8` JPEG SOI, `FF E0` APP0, `JFIF`; followed by `FF E1`/`Exif` and little-endian TIFF marker |
| Full validity | Not exercised: JPEG prefix observed, complete JPEG or RAW parsing not performed |

First64bytes, which contain no image raster:

```text
ffd8ffe000104a46494600010100000100010000ffe1008c45786966000049492a00080000000300310102000700000032000000120203000200000002000200
```

`source-receipts.json` holds exact private source path, argv, UTC, stat/header/hash values and exits. Hash/header capture UTC05:39:25; source mtime matches the first logged failure second. That relation supports capturing the same staged file, but cannot prove what bytes its original provider returned at import time.

## Content-URI and decode boundaries: static source, not a reproduction

- `src-tauri/src/file_management.rs:3730`: Android `import_files` resolves a content URI's display name, reads its bytes through `read_android_content_uri`, derives the extension from that display name at3760, and writes the returned bytes at3774. In this branch, retaining `.RAF` in the display name is separate from validating the payload. This is the current code contract, not proof of this event's provider route.
- `src-tauri/src/android_integration.rs:345`: the URI reader calls ContentResolver `openInputStream`, loops over the returned stream and returns those bytes at391. No transcoding operation is present in this shown helper. Upstream provider/file selection is still an open boundary.
- `src-tauri/src/raw_processing.rs:186`: the production RAW path constructs `RawSource` from supplied bytes and calls `rawler::get_decoder` before developing a RAW image. `src-tauri/src/image_loader.rs:193` logs the observed error after a normal development error; embedded-preview fallback may be tried, but the recorded frontend failure shows it did not yield a successful open for this event.
- `src/hooks/useImageLoader.ts:105`: current frontend catch logs/toasts `Failed to load image` and clears its selected image on failure. This matches the retained frontend message, not a fresh UI observation.
- Lead identifies the exact pinned rawler error producer at local checkout `a32bc1f/rawler/src/decoders/mod.rs:998`; Opus owns its diagnosis. H1R made no decoder support/source-change conclusion.

A failed-basename search returns only the ten read/failure lines. A literal `content://` search of the existing app log returns grep exit1/no matches. Therefore the captured runtime evidence does **not** identify an original URI, source provider, original path, original byte count, MIME type or original hash, and cannot distinguish pre-existing extension mismatch from provider delivery or import-boundary behaviour. No grant token was obtained or put in this public document. No original file was guessed from the basename or searched for elsewhere.

## Acceptance and next consumer

| Case | Result | Scope |
|---|---|---|
| User's imported X-T5 source/click sequence | User-reported | Not repeated or camera-verified |
| Decoder/open failure exists | Fail — directly observed in retained logs | Same staged path and frontend error, five RAW errors/two frontend failures |
| Failed staged source identified/readable | Pass | Exact path in private receipts; app-scoped stat/header/hash succeed |
| Staged filename/header mismatch | Pass — observed diagnostic fact | `.RAF` path has JPEG/JFIF prefix; no complete format decode |
| Original-source↔staged-byte equality | Not exercised | Original URI/file not identified |
| Genuine X-T5 RAF support | Not exercised | Captured staged bytes do not establish a genuine RAF input |
| Host source reproduction or corrected import | Not exercised | No full source transfer, builds, fallback or new open action |
| Prior basic adjustments/launch/UI | Pass — user-reported, unchanged | This report does not overturn earlier limited success |
| Physical profiles/export/fidelity | Not exercised | No profile flow attempted |

Actionable handoff to lead and Opus: reproduce only against the identified bytes after a lead-scoped full-source transfer if needed. The exact staged file is readable with `run-as`; a follow-on may stream that one file into a new private evidence path, but H1R did not do so. For original-byte tracing, first identify the exact user-selected source/provider and compare its MIME/display-name/size/header/hash against the staged SHA above. Do not substitute a random X-T5 file, change profile math, silently rename a JPEG into RAW support, or infer an X-T5 decoder regression solely from this event. Existing synthetic profile flow remains pending.

No active screen-test window is assumed. Stop at this diagnostic receipt; lead/Opus choose the next bounded source/provider or reproduction slice.

## Commands, exits and artifact receipt

Device paths below are redacted aliases; exact executable argv are in private JSON. Every device command explicitly uses `-s RFGL73CE9XM`. All commands are read-only; `logcat -d` does not clear logs.

- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM get-state` → 0 (connection).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell pidof io.github.CyberTimon.RapidRAW` → 0 (pid).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell dumpsys activity activities` → 0 (foreground).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell 'run-as io.github.CyberTimon.RapidRAW grep -n -B 4 -A 4 '"'"'No decoder found'"'"' logs/app.log'` → 0 (exact-error-context).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM logcat -d -v epoch --pid=24047` → 0 (pid-logcat-context).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell 'run-as io.github.CyberTimon.RapidRAW grep -n -i '"'"'<FAILED_STEM>'"'"' logs/app.log'` → 0 (failed-basename-log-history).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell 'run-as io.github.CyberTimon.RapidRAW grep -n -i '"'"'content://'"'"' logs/app.log'` → 1 (content-uri-error-evidence).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW stat -c ''"'"'%s %Y %i'"'"'' <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (stat-before).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM exec-out run-as io.github.CyberTimon.RapidRAW head -c 64 <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (header64).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW sha256sum <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (sha256).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW stat -c ''"'"'%s %Y %i'"'"'' <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (stat-after).

Host source reads used `nl -ba`/`sed` at the file/lines above and `rg` for the literal error/URI boundaries; no source edits occurred. Git HEAD/status and H1a document SHA were checked. A zero-byte bootstrap temporary was accidentally created at the H1 evidence root before the new child existed; it was immediately removed, leaving no persistent out-of-allowlist change. Only this document and `raf-diagnostic/` contain persistent H1R writes.

Private `report-receipt.json`, `ANDROID-RAF-DEVICE.patch` and `evidence-manifest.json` provide the new-document hash/diff, preflight-preservation check, exact status and evidence hashes. No stages/commits/pushes were performed. Next consumers are the lead and Opus slot ending`f6f8`; both received the critical header/hash/log observation through Team messaging.
