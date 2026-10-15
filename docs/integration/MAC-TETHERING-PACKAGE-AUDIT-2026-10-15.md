# Apple Silicon tethering DMG — package audit

**Blocked for the standalone development-preview asset list.** Provenance/integrity/content checks pass, but the executable strongly loads two unbundled Homebrew libraries. Strict bundle signature verification also fails. Do not stage this as a self-contained Mac download. This is a package audit, not a runtime launch result or a source-review rejection.

Auditor: Codex CLI - Astra, actual `gpt-6-astra`. Only `/private/tmp/rapidraw-main-publication-20261015/mac-batch1/` written. No app/CLI launch, UI, settings, signing repair, dependency install, build/cache, Git, upload/release or device action.

## Provenance and retained files

GitHub metadata confirms repository `harrytuckerr/RapidRAW-Fujifilm-edition`, run **38708083220**, head **2a7346229bc0cc398ee7e3fee126eb89428b7dd9**, artifact **11961116418**, completed successful tethering job **116332374975**. Other jobs in the run were still running; their state is not this artifact's verdict.

- ZIP: `artifact-11961116418.zip`, **35,409,899 bytes**, SHA256 **55222f64d04c26e16dc7cde7c4abd50cfd8f85113a88cfc704b295082afad5f7**, matches supplied and GitHub digest. CRC passes. One regular DMG entry; extraction rejected traversal, absolute/backslash paths, symlinks and duplicates, with bounded sizes.
- DMG: `extracted/release/bundle/dmg/RapidRAW_1.6.5_aarch64.dmg`, **35,476,340 bytes**, SHA256 **b932eec78e061cada70bf5858e04b8d3fd8f85c4a8a37ba528c2be66a4350c3c**. `hdiutil verify` exit0, checksum VALID.
- **Ready-for-upload files: none.** Both files remain immutable audit inputs at those paths under this folder, not approved publication assets.

## Concrete packaging findings

**P2 / standalone-preview blocker:** `otool -l RapidRAW.app/Contents/MacOS/RapidRAW` shows strong `LC_LOAD_DYLIB` commands16/17 for:

```text
/opt/homebrew/opt/libgphoto2/lib/libgphoto2.6.dylib
/opt/homebrew/opt/libgphoto2/lib/libgphoto2_port.12.dylib
```

The74-file app inventory has no libgphoto2 libraries or Frameworks bundle, only the ONNX dylib. These are startup dependencies, not merely optional runtime discovery. A machine lacking those exact external libraries cannot satisfy these loads from this bundle. This conclusion is from Mach-O metadata; no launch was attempted. Minimal publication action: omit this tethering variant from the standalone preview. Any future packaging correction or explicitly dependency-requiring developer distribution needs a separate decision; no repair or waiting for other Mac jobs is part of this task.

**Signing result — not a Developer ID claim:** codesign display exit0 reports `Signature=adhoc`, `flags=adhoc,linker-signed`, `TeamIdentifier=not set`, `Info.plist=not bound`, `Sealed Resources=none`; CDHash `e426fa183916c846f062d6508e6a73b026fc54e9`. `codesign --verify --deep --strict --verbose=4` exits1: **“code has no resources but signature indicates they must be present.”** Thus even ordinary strict bundle verification does not pass; do not label this verified/signed-for-distribution. The DMG itself reports no code signature (display exit1). This does not erase the app executable's observed ad-hoc signature.

`xcrun stapler validate` exits65 for both app and DMG: **no stapled ticket**. No notarization service record or Gatekeeper launch assessment was queried; neither build success nor absent stapling alone establishes notarization history. No quarantine removal or policy bypass was attempted.

## Content and architecture

Info.plist: bundle `io.github.CyberTimon.RapidRAW`, version/build **1.6.5**, minimum macOS **13.0**. `file`/`lipo -archs` confirm main executable and bundled `validate_dcp` are **arm64**; ONNX is arm64. ONNX SHA256 **2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a**.

All six Spektrafilm `.cube` LUTs, Spektrafilm license and AGPL3/LGPL2.1/LGPL3 texts are present and hash-identical to the audited2a source. No RAF/RAW/DNG/DCP/XMP/private-env/keystore/key paths or test-assets/Cobalt payload paths found; bounded private-key/token/commercial-table signature scan returned no findings. App contains no symlinks. Full file paths/sizes/SHA256 inventory is in `contents.json`. This content check does not claim universal secret detection or image/fidelity acceptance.

Mounted with `hdiutil attach -readonly -nobrowse -noautoopen` at the owned `mount/`; `diskutil info` confirmed Writable/WritableMedia/WritableVolume all false. Only owned **/dev/disk6** was detached: exit0, ejected, mountpoint empty. No unrelated mount was touched.

**Pass:** GitHub identity/digest/size, ZIP CRC/safe extraction, DMG checksum, architecture, resource/license custody and bounded content audit. **Fail:** standalone dependency closure and strict app-bundle signature verification. **Observed:** ad-hoc executable signature; no Developer ID/team; no app/DMG stapled ticket. **Not exercised:** app/CLI/tethering/UI/device, signing repair, notarization service record, profile/export/Adobe fidelity.

`review.json` records exact paths, checks/exit statuses and evidence hashes; raw metadata and tool logs are retained beside it. Lead alone decides publication. Stop after this artifact; mount released.
