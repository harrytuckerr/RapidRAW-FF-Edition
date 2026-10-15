# H3 Adobe capture preflight — 15 October 2026

**Pass as a route preflight. Not an Adobe match, and not an H2b3 release.** Revision of superseded `47f6695d9586de94ba851b550878315c1e7b72344005e4ed3b3eda01abc8dfe3`. The route is unchanged. This revision adds reproducible export-token locators and withdraws the claim that a future capture is library-write-free.

The installed product is cloud Lightroom desktop 9.6 (`com.adobe.lightroomCC`), not Lightroom Classic and not a separate Camera Raw app. Four compiled export resources name 16-bit, ProPhoto RGB, Full Size, and Output Sharpening None. The desktop export guide names TIFF and Color Space, but not 16-bit or ProPhoto. No export was made. H2b3 stays pending. This note is not an oracle.

Reviewer: Grok Build / `grok-4.7`, slot `01a13e02-3c22-7ad0-9ed3-8bab4bc7d945`. Harness-review HEAD `4c1e965a53de9c835df920b2e3d69c2c8a864708`. Check date 2026-10-15 AEDT. macOS 26.5.2 / 25F84, arm64. This revision did not launch the app, log in, export, read a photo, or write Git, cache, or the library.

## 1. Reused inputs

H0 `447a331b4c3cbb80a64c8df375df90cde1c3d0adbf73c87f64a9266a685e9316`: 356 X-Pro2 RAFs; no controlled Fuji TIFF. Cobalt Modular DCP is already inside the cloud `.lrlibrary` package, SHA-256 `4b80ed63b210e5738cde04430af1511c3e12e2bb53bf2471b4bb0750dceb87f3`. Flat and the 17 Looks stay incompatible. `DSCF2029.xmp` has a crop and an unproven writer; do not apply it. The assignment names `DSCF1893.RAF`. H0 published no per-file path or hash, and this preflight did not open it.

H2 contract `f29936252b3ec103a41a76961d97902c82970416ea5802f88d497ef4341a6880`: reference v1 is classic RGB16 TIFF, Orientation 1, no alpha, embedded ICC pinned by hash. A menu label is not that ICC. The TIFF is a full Adobe render, not an isolated DCP stage. ICC-shape bootstrap and full-image fidelity stay separate.

## 2. Installed app

| Fact | Value |
|---|---|
| Bundle | `/Applications/Adobe Lightroom CC/Adobe Lightroom.app` |
| Display name / id | Adobe Lightroom / `com.adobe.lightroomCC` |
| Version / build | 9.6 / `20260923-1659-a32d844` |
| Signature | Team `JQ525L2MZD`, timestamp 24 Sep 2026, universal |
| Absent | No Classic, Photoshop, or Camera Raw app in `/Applications`. Classic ownership is not inferred. |
| ICC in bundle | No `*.icc` or `*.icm`. Nothing to hash. A bundled profile would not be the H2b3 input. |

Desktop 9.x is the cloud app: [system requirements](https://helpx.adobe.com/lightroom/desktop/introduction/system-requirements.html), updated 12 Oct 2026.

## 3. Export-token locators

Resources are compiled Lua (`1b 4c 75 61 51`), mtime 2026-09-30, under `Export.lrmodule/Contents/Resources`. Token names only. No module bytes are copied here.

| File | Bytes | SHA-256 | Exact strings found |
|---|---:|---|---|
| `AgExportTIFFSettingsProvider.lua` | 4695 | `d943c23c0328bc43eebf9674edab16a8379a911b8a861fdc06d352a2cdcd9b0f` | `$$$/AgExport/TiffExport/BitDepth/16bits=16 bits`; `$$$/AgExport/TiffExport/Compression/None=None` |
| `AgExportColorSpaceProvider.lua` | 8888 | `de81fc23942462caa86c903d9853fec272364a980d7ebe128e33e3e6036bb981` | `$$$/AgExport/ColorSpace/ColorSpaceOption/ProPhotoRGB=ProPhoto RGB`; `tiffSDRColorSpace` |
| `AgExportDimensionSettingsProvider.lua` | 11458 | `f082e44a8e46b3226eff17a9ca8c95bd27ddb789551bc0dfac44fea54539c8bc` | `$$$/AgExport/Dimension/Full=Full Size` |
| `AgExportOutputSharpeningProvider.lua` | 4810 | `1541490f6208ac4d19af537888c12cda4ac3eff11f92273b629b09c1ba74d9ac` | `$$$/AgExport/OutputSharpening/OutputSharpeningTitle=Output Sharpening`; `$$$/AgExport/OutputSharpening/MediaOption/None=None` |

`shasum -a 256` on those four paths exited 0. The read-only presence check also exited 0. It reads each file and reports only found or missing for the strings above:

```text
R="/Applications/Adobe Lightroom CC/Adobe Lightroom.app/Contents/PlugIns/Export.lrmodule/Contents/Resources"
python3 -c 'import pathlib,sys
b=pathlib.Path("'"$R"'")
need={"AgExportTIFFSettingsProvider.lua":[b"$$$/AgExport/TiffExport/BitDepth/16bits=16 bits",b"$$$/AgExport/TiffExport/Compression/None=None"],
"AgExportColorSpaceProvider.lua":[b"$$$/AgExport/ColorSpace/ColorSpaceOption/ProPhotoRGB=ProPhoto RGB",b"tiffSDRColorSpace"],
"AgExportDimensionSettingsProvider.lua":[b"$$$/AgExport/Dimension/Full=Full Size"],
"AgExportOutputSharpeningProvider.lua":[b"$$$/AgExport/OutputSharpening/OutputSharpeningTitle=Output Sharpening",b"$$$/AgExport/OutputSharpening/MediaOption/None=None"]}
rc=0
for n,ns in need.items():
 d=(b/n).read_bytes()
 for s in ns:
  ok=s in d; rc=rc or not ok; print(("FOUND" if ok else "MISSING"), n, s.decode())
sys.exit(rc)'
```

These strings show the labels exist in this build. They do not prove the live dialog or the embedded ICC.

## 4. Controls

| Need | Local 9.6 fact | Primary doc, read 15 Oct 2026 | Still needs the screen |
|---|---|---|---|
| 16-bit TIFF | §3 TIFF resource: 8/16/32, compression None/ZIP/LZW | [Export](https://helpx.adobe.com/lightroom/desktop/save-share-and-export/save-share-photos.html) lists TIF via Custom Settings. It does not name bit depth. | Live dialog matches §3. |
| ProPhoto | §3 color-space resource: `ProPhotoRGB` and `tiffSDRColorSpace` | Same page: Color Space choices are in-app tooltips. ProPhoto is not named. | Embedded ICC. A 2023 TIFF color-space bug is history, not this build. |
| Full Size | §3 dimension resource | Doc Full Size sentence says JPEG quality. | TIFF pixel count equals the oriented RAW. No crop. |
| Output sharpen None | §3 sharpening resource | Doc names Output Sharpening under More Options. | Set None. |
| Metadata | All Metadata or Copyright Only, same export module, not re-hashed here | Same page. | All Metadata. No watermark. |
| Profile | Library already holds the X-Pro2 Cobalt Modular DCP. | [Edit](https://helpx.adobe.com/lightroom/desktop/edit-photos/edit-photos.html): raw default is Adobe Color. Documented import is XMP, not DCP. | If Cobalt Modular is absent, stop. Do not convert the DCP. |
| Amount | Shown only when `supportsProfileAmount`. | Doc Amount is creative profiles, not a camera profile. | Record shown, hidden, or disabled. Do not invent 100. |
| As Shot / reset | White Balance includes As Shot. Reset Edits, Reset to Original, Reset to As Shot. | Doc: Reset or double-click a slider. Reset may restore Adobe Color. | Re-select Modular. Record Temp/Tint. |
| Linear curve | A tone-curve preset string Linear exists, separate from Linear Gradient. | Control-click the curve and choose Reset Channel. | Confirm the identity line. |
| Lens / detail | Enable Lens Corrections; Remove Chromatic Aberration; Sharpening; Noise Reduction; Color Noise Reduction. | Edit page documents the three detail controls, not a zero default. | All off or 0. No Lens Blur, Denoise, Raw Details, or Super Resolution. |
| Process Version | No user menu. | Edit guide has none. Classic's menu is another product. | TIFF XMP, or `not_exposed`. Do not copy 15.4. |
| Orientation / HDR | Orientation tag is not promised. HDR Output exists. [HDR guide](https://helpx.adobe.com/lightroom/desktop/edit-photos/hdr-output.html), 27 Oct 2025. | v1 needs Orientation 1, HDR off, not 32-bit. |

## 5. Future capture is not write-free

This preflight did not observe a Local tab. [Import and access](https://helpx.adobe.com/lightroom/desktop/add-import-and-capture-photos/access-photos.html), updated 27 Mar 2026, documents Photos > Local as a way to open a folder without the Cloud import dialog ("Photos will be Synced to Creative Cloud"). The same page says Local-tab edits are stored as in-file metadata or an XMP sidecar, and that the Local browser can rename or move files. That behavior was not observed here. Do not promise an edit or export that leaves the app catalog, cache, and sidecars untouched. Do not hand-edit `.lrlibrary`.

When a later operator is assigned, on this 9.6 build only:

1. Copy one inventoried RAF into a new private task folder. Hash the copy and require a match. Leave the original bytes in place. Do not use Flat, a Look, or `DSCF2029.xmp`.
2. If Photos > Local is absent, stop. Do not use Add Photos, the Cloud tab, or Copy to Cloud. Do not rename, move, or delete from that browser.
3. Open only the task-folder copy. Reset, then select Cobalt Modular. As Shot WB; tone sliders 0; curve Reset Channel; lens and chromatic aberration off; sharpen and both noise sliders 0; no mask, crop, geometry, heal, grain, dehaze, texture, clarity, vignette, or HDR.
4. Custom Settings: TIF, ProPhoto RGB, 16 bits, compression None, transparency off, HDR off, Output Sharpening None, Full Size, All Metadata, no watermark. Write the TIFF in the task folder, outside Git. If 16-bit or ProPhoto is missing, stop. Do not substitute JPEG, 8-bit, Adobe RGB, or 32-bit.
5. Hash any new sidecar beside the copy. It is provenance only. Expect app-state writes; record them. Do not call the session write-free.

## 6. Manifest, blocker, next assignment

Record `producer=adobe` and `reference_scope=controlled_adobe_export`: product, bundle id, 9.6, build, profile name as shown, DCP SHA above, Look none, Amount as shown, WB numbers, every edit, export settings, RAW/copy/TIFF/ICC/sidecar SHA-256 and lengths, Orientation, and operator id. State that the TIFF is a full Adobe render.

H2b3 remains pending. No ICC bytes and no white bound are frozen. The smallest next input is one task-folder TIFF plus its embedded ICC. If Cobalt Modular cannot be selected, stop. Do not install another app.

**Next action:** the lead coordinates one native UI session: private RAW copy, Local tab only if it is actually present, §5 settings, TIFF outside Git. A later read-only slice hashes the embedded ICC and stops before any parser change. Full-image fidelity is not that slice.

**Not exercised:** UI, Local tab, login, export, embedded ICC, images, Adobe metrics, GPU, device, and performance. **Fail:** none. The prior "no library write" capture instruction is withdrawn. Document SHA-256 is in the delivery message.

## 7. Commands this revision

`git rev-parse HEAD` exited 0 (`4c1e965a53de9c835df920b2e3d69c2c8a864708`). `shasum -a 256` on the four §3 files exited 0. The §3 `python3 -c` presence check exited 0 with every listed string FOUND. No other command wrote state.
