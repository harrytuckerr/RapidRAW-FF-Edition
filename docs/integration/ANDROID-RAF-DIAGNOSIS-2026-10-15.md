# Android Fuji RAF "No decoder found" — Diagnosis (H1R)

- Date: 2026-10-15 (revision 4, replaces `cc869b28…`, `6fabfeb0…`, `c4747370…`)
- Author: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Base: detached `4c1e965a53de9c835df920b2e3d69c2c8a864708` (worktree `rapidraw-fuji-team-20261015/raf-diagnosis`)
- rawler pin (Cargo.lock at base): `git+https://github.com/CyberTimon/RapidRAW-DngLab.git#a32bc1ff1f4f3c56d4d3226082f6eb17414f9159` (rawler 0.7.1)
- Installed APK (lead-verified): sha256 `bf8d139932aba062ec4da0d8ffcf446d6465ecec96b258dd658e5aace018a546`. The rawler commit linked into the APK was not extracted from the binary; it is inferred from the lock.
- Device evidence: Sol, accepted `lead/docs/integration/ANDROID-RAF-DEVICE-2026-10-15.md` sha256 `51f10e64afd1277ed2874753a392610a10d5b6385318125214ecf7cd3ca10713`. I did no device work.
- Scope: diagnosis only. No production source, dependency, lock or Git changes.

## 1. Summary

User report (via lead): a physical Fold fails to open a Fujifilm **X-T5** RAW. It was imported into RapidRAW and then opened from RapidRAW's own picker.

Directly observed (Sol, device):
- Staged file: `/storage/emulated/0/Android/media/io.github.CyberTimon.RapidRAW/.library/DSCF5389.RAF`.
- 2,940,309 bytes, sha256 `82bd1e04633c491824b90cc31486b5c3ff0770a6dc712edf95d74c691bc958c6`, stat stable before and after.
- Header `FF D8 FF E0 00 10 4A 46 49 46` (JPEG/JFIF).
- The app log shows five `Error: No decoder found, model '', make: '', mode: ''` RAW-development errors and two frontend load failures for this path (05:34:25–05:35:00 UTC). Each is preceded by a memory-map "file is locked" warning and the fallback read.

Conclusion:
- The failing bytes are a valid JPEG carrying a `.RAF` name.
- rawler's detection correctly finds no RAW container in them.
- This is a **detection** result on non-RAW bytes. It is **not** evidence about X-T5 RAF support, compression support, or rendering.
- Where the JPEG bytes came from is **not established**. Either the source item was already a JPEG named `.RAF`, or something in the pick/import chain supplied a rendition.

## 2. Producer and consumer lines

Producer (rawler, pinned checkout `~/.cargo/git/checkouts/rapidraw-dnglab-d9adc5b1c1279a46/a32bc1f/rawler`):

| Line | Behaviour |
|---|---|
| `src/decoders/mod.rs:847-856` | `get_decoder`: `mrw::is_mrw`, then `raf::is_raf` → `RafDecoder::new`, returned directly |
| `src/decoders/raf.rs:61-66` | `is_raf` = `subview(0,8) == b"FUJIFILM"`. Buffers shorter than 8 bytes → `false` |
| `src/decoders/mod.rs:864-905` | QTK / CIFF / JFIF-EXIF / X3F / BMFF probes. A JFIF/EXIF file is routed to `MrwDecoder::new_jfif` only when the Exif `Make` is `Konica Minolta Photo Imaging, Inc.`. That is a legitimate JPEG-container RAW route, so some RAW extensions can carry JFIF bytes. Any other make falls through. The DSCF5389 payload has no `Make` tag at all |
| `src/decoders/mod.rs:907-990` | Generic TIFF probe. Not a TIFF here |
| `src/decoders/mod.rs:991-1001` | Naked-filesize lookup fails, giving `Err(Unsupported{what:"No decoder found", make:"", model:"", mode:""})` |
| `src/error.rs:143` | Display: `Error: No decoder found, model '', make: '', mode: ''` (the exact logged text) |
| `src/rawsource.rs:55,72` | `new_from_slice` copies the bytes. `subview` returns `UnexpectedEof` out of range |

At this rawler commit, any buffer that starts with `FUJIFILM` is routed to `RafDecoder` before this error can occur. So this message implies the buffer did not start with a RAF signature.

Consumer (RapidRAW, base `4c1e965a`):

| Line | Behaviour |
|---|---|
| `src-tauri/src/raw_processing.rs:186-187` | `develop_raw_image`: `RawSource::new_from_slice(file_bytes)` → `rawler::get_decoder(&source)?` (error propagated verbatim) |
| `src-tauri/src/formats.rs:25,81-90`, `image_loader.rs:142,166` | The RAW branch is chosen by **file extension only** (case-insensitive `raf`). There is no content sniffing |
| `src-tauri/src/image_loader.rs:196-215` | On error: `classify_raw_develop_error` (`:256`, pass-through for this text), then `safe_embedded_preview_fallback`. This returns `None` for a bare JPEG: `largest_tiff_jpeg_preview` needs TIFF magic and `extract_embedded_preview` (`raw_processing.rs:79-80`) needs a rawler decoder. rawler's raw error text reaches the UI. Not displaying the JPEG as a RAW is correct; only the message is unhelpful |
| `src-tauri/src/image_loader.rs:982-1010`, `file_management.rs:1393-1414` | `read_file_mapped`: `try_lock_shared` fails on this Android path (`:1404`, the logged "locked" warning), so every load falls back to a full `fs::read`. That fallback is a performance/robustness issue only. It returns all of the file's bytes, so it does not cause the error |

Android import code (what the code does; this is not proof of what happened to DSCF5389):

| Line | Behaviour |
|---|---|
| `src/hooks/useFileOperations.ts:342-392` | Android import: `open({filters: []})`, then `resolve_android_content_uri_name`. The extension check uses DISPLAY_NAME only; then `startImportFiles` |
| `tauri-plugin-dialog-2.7.2/android/src/main/java/DialogPlugin.kt:63-77` | `Intent.ACTION_GET_CONTENT`, `type="*/*"`. Any installed provider can answer |
| `src-tauri/src/file_management.rs:3729-3779` | `import_files`, Android branch: `read_android_content_uri`, then `fs::write(dest, bytes)`. The extension comes from DISPLAY_NAME. Bytes are not checked against the extension |
| `src-tauri/src/android_integration.rs:336-397` | `read_android_content_uri`: `openInputStream`, then an 8 KiB loop until `-1`. It writes exactly what the provider streams |

So RapidRAW stages whatever bytes the provider streams, under the provider's display name. That is consistent with both provenance options in §4 and does not decide between them. The manifest declares only `INTERNET`. That makes picker import the expected route, but it does not prove that no other byte source exists.

Failure classes (kept distinct):

| Class | Example message | Status for this report |
|---|---|---|
| Detection | `Error: No decoder found, model '', make: '', mode: ''` | **Observed** (JPEG bytes) |
| Camera support | `Error: Unknown camera, model 'X-T5', make: 'FUJIFILM' …` | Not exercised |
| Decompression | `Failed to decode image, possibly corrupt image. Origin error was: …` | Not exercised for X-T5 |
| Rendering | errors after `raw_image` succeeds | Not exercised |

## 3. Host reproductions

Scratch crate: `/private/tmp/rapidraw-fuji-raf-diagnosis-20261015/repro` (path dependency on the pinned rawler checkout; private target dir; release build 1m47s, done once). It makes the same calls as `raw_processing.rs:186-190`: `RawSource::new_from_slice` → `rawler::get_decoder` → `raw_image`.

```
CARGO_TARGET_DIR=/private/tmp/rapidraw-fuji-raf-diagnosis-20261015/target CARGO_NET_OFFLINE=true cargo build --release
/private/tmp/rapidraw-fuji-raf-diagnosis-20261015/target/release/raf_nodecoder_repro <file>
```

### 3a. Exact failing payload (Sol's private read-only copy)

The input was `/private/tmp/rapidraw-fuji-H1-20261015/raf-payload/failed-staged-payload.RAF`: 2,940,309 bytes, sha256 `82bd1e04…c958c6`, matching the device. The payload was not copied or modified. Only the first probe line is meaningful for this input.

```
A full RAF                         len=2940309    head=......JF get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
```

**Pass:** the exact device message reproduces on the host from the exact staged bytes.

Structure of the payload (my host parse of the first 4 KiB, tag IDs only):
- SOI, `APP0` JFIF 1.1 (16 B), `APP1` Exif (140 B).
- IFD0: `0x0131 Software = "Google"`, `0x0212 YCbCrSubSampling = 2,2`, `0x8769 ExifIFD`.
- ExifIFD tags: `0x9000 ExifVersion`, `0x9003 DateTimeOriginal`, `0xA002/0xA003 PixelX/YDimension`.
- There is **no Make, no Model and no MakerNote**.
- `SOF0` at offset 296: **7752 × 5178**, 3 components. Sol's Pillow check independently confirms a valid JPEG at 7752×5178.

These are recorded facts about the staged bytes only. They do not establish when or by which component the JPEG was made, or whether it reached the phone already named `.RAF`. No further metadata inference is drawn.

### 3b. Intact RAF control (X-Pro2, not X-T5)

The input was `…/DCIM/211_FUJI/DSCF1893.RAF`, read-only: sha256 `e736b4af4d80fa12418f92c018ac99b5c5c96d8e772ce3a34fc2cb9c2a4619b1`, 50,596,864 B, header `FUJIFILMCCD-RAW 0201FF129501X-Pro2`. This is a control for the rawler path only. It is **not** an X-T5 reproducer.

```
A full RAF                         len=50596864   head=FUJIFILM get_decoder=OK   raw_image=OK 6160x4032 cam='FUJIFILM X-Pro2'
B empty (0 bytes)                  len=0          head=         get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
C zero-filled, same size           len=50596864   head=........ get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
D RAF truncated to 4 KiB           len=4096       head=FUJIFILM get_decoder=OK   raw_image=ERR Failed to decode image, possibly corrupt image. Origin error was: No RAF data IFD found
E RAF truncated to 50%             len=25298432   head=FUJIFILM get_decoder=OK   raw_image=ERR Failed to decode image, possibly corrupt image. Origin error was: I/O Error without context: subview_padded(): Offset 922624+49674240 is behind EOF
F embedded JPEG only (as .RAF)     len=898145     head=......Ex get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
G RAF with 4-byte prefix           len=50596868   head=....FUJI get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
H HTML/text error body             len=33         head=<!DOCTYP get_decoder=ERR  Error: No decoder found, model '', make: '', mode: ''
```

Valid findings, scoped to the pinned rawler source on a macOS host:
- An intact RAF passes detection and decode through the exact consumer call sequence.
- A headed-but-truncated RAF fails as **decompression** (D, E), not detection.
- "No decoder found" occurs only for buffers that are not RAF-headed (B, C, F, G, H).

Not shown:
- Whether the APK's linked rawler behaves identically on Android. Expected, because detection is a pure byte comparison with no `cfg(target_os)` in this path, but not exercised on device.
- X-T5 support. The pinned rawler has an X-T5 entry at `rawler/data/cameras/fuji/x-t5.toml` (`make = "FUJIFILM"`, `model = "X-T5"`). Without real X-T5 RAF bytes, decoding through either the host build or the APK is **Not exercised**.

## 4. Ranked hypotheses for the provenance of the JPEG bytes

Both remain open. Nothing here proves that the Android import or acquisition code is defective.

1. **P-a: the source item was already a JPEG named `DSCF5389.RAF` before RapidRAW read it.** An earlier transfer, export or sync step kept the RAW filename. RapidRAW then staged it faithfully.
2. **P-b: the content provider that answered the pick streamed a JPEG rendition under the original RAW display name.** RapidRAW then staged that rendition faithfully.

Under both, RapidRAW's staged bytes equal what `openInputStream` returned. Nothing indicates byte corruption inside RapidRAW. The RapidRAW-side gaps under both are the same:
- No content check at import.
- Extension-only RAW routing.
- An unhelpful message on load.

Rejected or deprioritised:
- Camera unsupported: this error string cannot be produced by a RAF-headed buffer at the pinned commit.
- Truncated copy: that would show a decompression error.
- An Android-specific rawler difference: not exercised, but nothing in the detection path is platform-dependent.

## 5. Remaining bounded observations (only what is not yet satisfied)

This only closes provenance. It is not needed for the RapidRAW-side guard in §6. No device queries are requested.

1. User: which picker source was tapped, and how DSCF5389 reached the phone (camera Wi-Fi/Bluetooth app, card or USB reader, cloud download, share).
2. When available: the original genuine X-T5 RAF, a header starting `FUJIFILM`, to exercise X-T5 detection and decode on the host.

## 6. Minimal repair proposal (not implemented; needs an exact ownership grant)

Principle: never silently process JPEG bytes as camera RAW. Reject the contradiction and say clearly what the file contains.

### 6a. Future imports: guard before staging

- File: `src-tauri/src/file_management.rs`, Android branch of `import_files` (~3729-3779).
- Add a small pure helper, e.g. `validate_imported_raw_bytes(name: &str, bytes: &[u8]) -> Result<(), String>`, called before `fs::write`:
  - Empty → Err.
  - Extension `raf` (case-insensitive) and bytes start `FF D8 FF` → Err: `"<name>" contains JPEG image data, not Fujifilm RAF RAW, so it was not imported. Import the original RAW file.`
  - Otherwise Ok.
  - Keep the check to this RAF/JPEG contradiction only. Do **not** reject JPEG bytes under every RAW extension, because rawler legitimately decodes Konica Minolta JFIF/EXIF containers (`MrwDecoder::new_jfif`, `mod.rs:874-884`). Do not add any other unknown-signature allowlist.
- On Err the file is not staged. Dependency: Astra's review found that the backend `import-error` is followed by an unconditional `import-complete` and a frontend success override. The terminal-outcome seam is being defined by that review. This guard is only useful together with it.
- Desktop import uses a separate branch. Whether to apply the same helper there is a separate ownership decision.

### 6b. Already-staged files, e.g. the current DSCF5389.RAF: precise load error

- File: `src-tauri/src/image_loader.rs`, `classify_raw_develop_error` (`:256`). It is called from `load_base_image_from_bytes` (`:196`), which already has the bytes.
- Pass the bytes (or a "starts with FF D8 FF" flag) into the classifier. When the extension is `raf`, the error contains `No decoder found`, and the bytes are JPEG, return: `"<path>" has a RAW extension but contains JPEG image data; it cannot be developed as RAW.`
- Keep returning Err. Do **not** fall back to decoding the JPEG under the RAW name.
- Do not rename or delete the staged file automatically.

### 6c. Smallest tests (host unit tests, no device needed)

- Helper cases:
  - `("x.RAF", [])` → Err
  - `("DSCF5389.RAF", FF D8 FF E0 …)` → Err containing "JPEG"
  - `("x.raf", b"FUJIFILMCCD-RAW …")` → Ok
  - `("x.jpg", FF D8 …)` → Ok
  - `("x.mrw", FF D8 FF E1 …)` → Ok (the Minolta JFIF route is not regressed)
  - `("x.nef", b"MM\0*…")` → Ok (non-RAF RAW is not over-rejected)
- Classifier: a `No decoder found` error plus JPEG bytes gives the JPEG message; the same error with zero bytes gives the original text plus an "empty file" qualifier (or unchanged, at the owner's choice).
- One device check afterwards: re-import DSCF5389 and expect the precise rejection; open the existing staged copy and expect the precise load message.

### 6d. Out of scope until genuine X-T5 RAF bytes are supplied

Any rawler change, any camera TOML change, any compression claim, and any profile, fidelity or performance gate.

## 7. Gate status

| Item | Status |
|---|---|
| User reports failed open of an X-T5-named source | Fail — user-reported; actual staged input is JPEG |
| Opening staged DSCF5389.RAF | Fail — directly observed in retained logs |
| Staged bytes are a valid JPEG (2,940,309 B, sha256 `82bd1e04…`, RGB 7752×5178) | Pass (Sol: device hash, private transfer, Pillow verify/load; host header parse agrees) |
| Exact message reproduced on host from exact staged bytes | Pass |
| rawler producer/consumer path located; intact X-Pro2 RAF control decodes; truncation is a distinct error | Pass |
| Provenance (P-a mislabeled source vs P-b provider rendition) | Not exercised |
| Genuine X-T5 RAF detection/decode (host or APK) | Not exercised |
| APK-linked rawler identical to lock pin | Not exercised (inferred) |
| Repair 6a/6b and tests | Not exercised (proposal only; ownership needed) |
