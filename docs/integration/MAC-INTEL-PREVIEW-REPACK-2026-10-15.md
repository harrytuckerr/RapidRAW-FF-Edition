# Intel Mac x86_64 Preview Packaging Receipt

**Task ID:** `01a13efe-0655-7442-9556-1a0db1e49632`  
**Timestamp:** 2026-10-15  
**Stage Directory:** `/private/tmp/rapidraw-main-publication-20261015/mac-intel-repack`  
**Architecture:** `x86_64`  

## 1. Input Verification
- **Artifact Source:** `harrytuckerr/RapidRAW-Fujifilm-edition`, Run `38708083220`, Artifact `11962992400`
- **Source Commit:** `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`
- **Input Artifact ZIP:** `/private/tmp/rapidraw-main-publication-20261015/mac-intel-repack/artifact.zip`
  - SHA256: `e04c0b9d61ca8b85937022a3233feadd9baf43cd9b126e833de79647d9eef634` (Verified matching expected digest)
- **Extracted Original DMG:** `/private/tmp/rapidraw-main-publication-20261015/mac-intel-repack/extracted/release/bundle/dmg/RapidRAW_1.6.5_x64.dmg`
  - Size: `38370929` bytes
  - SHA256: `8fdb6f57b9abfb51b33594a09508c045b2e27423f91351935b955febd928709e`
  - Original `codesign --verify --deep --strict`: Exit code `1` (Unsealed resource failure)

## 2. Integrity & Dependencies
- **Bundle ID:** `io.github.CyberTimon.RapidRAW`
- **Version:** `1.6.5`
- **Architecture Validation:** All Mach-O binaries confirmed `Mach-O 64-bit executable/dylib x86_64`.
- **otool Dependency Check:** Clean. Only macOS system frameworks and `/usr/lib/libSystem.B.dylib` / `/usr/lib/libc++.1.dylib`; no unbundled Homebrew (`/opt/homebrew`, `/usr/local`) dependencies.
- **License Files Detected:** `Contents/Resources/resources/licenses/AGPL-3.0.txt`, `Contents/Resources/resources/licenses/LGPL-2.1.txt`, `Contents/Resources/resources/licenses/LGPL-3.0.txt`, `Contents/Resources/resources/film_luts/SPEKTRAFILM_LICENSE.txt`
- **Forbidden Assets:** None (0 RAF/DCP/XMP/key assets found).

## 3. Ad-Hoc Sealing & Signing
- **Signature Mechanism:** Local ad-hoc (`-`), `--timestamp=none`, inside-out order for Mach-O binaries.
- **Signed Mach-O Binaries:**
  - `Contents/MacOS/validate_dcp`
  - `Contents/MacOS/RapidRAW`
  - `Contents/Resources/resources/libonnxruntime.dylib`
- **Strict Verification On Stage:** `codesign --verify --deep --strict RapidRAW.app` -> Exit code 0.
- **Exact Diff:** Only `_CodeSignature` and the signed Mach-O binary bytes changed. All Info.plist, resources, icons, and licenses remained bit-for-bit identical.

## 4. Final Repackaged Artifact
- **File:** `/private/tmp/rapidraw-main-publication-20261015/mac-intel-repack/RapidRAW-Fuji-preview-2026.10.15.1-macos-x86_64-adhoc.dmg`
- **Filename:** `RapidRAW-Fuji-preview-2026.10.15.1-macos-x86_64-adhoc.dmg`
- **Size:** `43985589` bytes
- **SHA256:** `41c152ba8934b83dc3f43591b7c60b3b4cec3a93f5fc250f8ef774ec78b1fcb7`
- **Format:** UDZO, Volume Label `RapidRAWFujiPreview`
- **Mounted DMG Strict Codesign Verification:** Exit code 0.

## 5. Scope & Policy Compliance
- Ad-hoc signature only (no Developer ID, no notarization claimed, no Gatekeeper bypass claimed).
- Standard x86_64 non-tethering app only; original input artifacts untouched.
- Ready for independent reviewer verification prior to upload.
