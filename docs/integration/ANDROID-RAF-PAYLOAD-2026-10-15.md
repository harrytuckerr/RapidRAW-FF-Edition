# H1R-payload — Exact failed staged-file preservation

**Pass:** the one authorized failed staged payload was privately transferred unchanged. Pillow identifies a readable **JPEG, RGB, 7752×5178**, despite its `.RAF` filename. This establishes the staged payload's format, not the original source/provider bytes or genuine X-T5 RAW support.

## Scope and identity

- Task `01a13e16-9f9b-78b1-9f81-093cb57861fb`; sole operator slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`, actual model `gpt-6.1-sol` per current Team roster.
- Frozen base `4c1e965a53de9c835df920b2e3d69c2c8a864708`; snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/device`.
- Host macOS26.5.2 arm64, Python3.9.6, preinstalled Pillow11.3.0. No dependency installation/build.
- Authorized source is exactly the staged file identified by the accepted H1R-device private `source-receipts.json`. Source device path/basename are redacted here as `<APP_MEDIA>/.library/<FAILED_RAF>`; exact argv remain private.
- Private host path for lead/Opus reproduction: `/private/tmp/rapidraw-fuji-H1-20261015/raf-payload/failed-staged-payload.RAF`. New directory mode0700; new file mode0600, created exclusively without overwriting an existing file.
- Payload size **2,940,309bytes**; SHA256 **`82bd1e04633c491824b90cc31486b5c3ff0770a6dc712edf95d74c691bc958c6`**. Same on-device pre-transfer hash, expected H1R hash and host hash.
- Original device file's before/after stat was unchanged: size2940309, mtimeepoch1792042465, inode359459. Capture timestamps are in `device-receipts.json`.

No other photograph/source was searched or copied. No raster was displayed, exported or written to the repository/logs; the complete payload remains solely in the explicitly authorized private evidence file. No EXIF/personal metadata values were reported. No UI, launch/force-stop/import, device-file/settings/permission mutation, source/lock edit, staging/commit/push, emulator or build.

## Transfer and decoder verification

The host read the previously identified exact source path from the private receipt and asserted equality to the known literal path before use. All device operations used explicit `-s RFGL73CE9XM`. Python `subprocess.run` argument arrays were used without a host shell or interpolated filename command. `exec-out run-as ... cat` streamed directly into an exclusive mode0600 host file, not the tool output. The source path contains only the already verified literal safe characters. Device stat/hash matched before transfer; host size/hash matched afterward; a mismatch would have stopped processing.

Pillow opened by content, reporting `JPEG`, `RGB`, dimensions7752×5178. `Image.verify()` passed; a second open plus `Image.load()` completed. Decoder bounds were at most50,000,000pixels and16,000pixels per dimension, with decompression-bomb warnings promoted to errors; this file fits those bounds. No decoded image was saved or shown. This is format validity/readability under the available Pillow decoder, not a colour-fidelity, provenance or RAW assertion.

## Results

| Check | Result | Evidence limit |
|---|---|---|
| Explicit authorized-device connection | Pass | Serial-specific `get-state=device` |
| Exact source stat/hash still expected | Pass | No source drift; before/after stat identical |
| Private full transfer equality | Pass | Device/host/expected SHA and byte count match |
| JPEG format/dimensions and bounded complete decode | Pass | Pillow11.3.0 verify/load; no raster output |
| New failure during this slice | None | Every device command and host verifier exited0 |
| User's original X-T5/import source | User-reported | Original camera/provider/file not independently verified |
| Original-provider↔staged byte preservation | Not exercised | Original URI/MIME/bytes/hash unavailable |
| Genuine native X-T5 RAF support; corrected import/open | Not exercised | No genuine RAF or new app action used |
| Profiles, export or Adobe fidelity | Not exercised | Unrelated acceptance remains open |

## Exact commands and receipts

Device source aliases below are redacted; full task-relevant argv/UTC/exits are in private `device-receipts.json`.

- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM get-state` → 0 (connection).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW stat -c ''"'"'%s %Y %i'"'"'' <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (stat-before).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW sha256sum <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (sha256-before).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM exec-out run-as io.github.CyberTimon.RapidRAW cat <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (exact-private-stream).
- `/Users/harrisontucker/Library/Android/sdk/platform-tools/adb -s RFGL73CE9XM shell run-as io.github.CyberTimon.RapidRAW stat -c ''"'"'%s %Y %i'"'"'' <APP_MEDIA>/.library/<FAILED_RAF>` → 0 (stat-after).

Host verification ran in one Python3 subprocess, exit0: streaming `hashlib.sha256`/size equality, then preinstalled `PIL.Image.open(payload)` → format/size/mode → bounds → `verify()`, then a second `open(payload)` → `load()`. Results are in `payload-identity.json` and `format-result.json`; environment/permissions are in `environment.json`. No payload bytes, EXIF fields or raster data were emitted into tool logs.

Prior reports are unchanged: preflight SHA256 `40d3d15669d650b4bf6ef9cd411dfb58236919204eb2b8c0c46b5d1f85b7637e`; H1R-device SHA256 `51f10e64afd1277ed2874753a392610a10d5b6385318125214ecf7cd3ca10713`. New document hash, new-file diff/hash and exact Git status are in private `report-receipt.json` and `ANDROID-RAF-PAYLOAD.patch`; `evidence-manifest.json` hashes private evidence. Only this new document and the new private child directory were written in this task.

Lead and Opus received the exact private host path/hash and format result through Team messaging. This payload is ready for their separately scoped host reproduction. Keep source bytes private and unchanged; any copies/outputs remain within their assigned evidence scope. H1R-payload stops here without a new UI reproduction, fallback, source change or provider claim.
