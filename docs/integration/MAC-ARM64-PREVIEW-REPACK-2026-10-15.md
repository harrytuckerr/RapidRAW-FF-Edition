# Mac ARM64 Preview Packaging Receipt

**Task ID:** `01a13ee4-b7cd-7962-adda-e78d924cea4f`  
**Timestamp:** 2026-10-15  
**Stage Directory:** `/private/tmp/rapidraw-main-publication-20261015/mac-arm64-repack`  

## 1. Input Verification
- **Input Artifact ZIP:** `/private/tmp/rapidraw-main-publication-20261015/ci-artifacts/macos-arm64/artifact.zip`
  - SHA256: `1f0c0dabd4434dc24970290efd38c0e2d81e5662e8c769616f731a95e3eb5b91` (Verified matching expected)
- **Original DMG:** `/private/tmp/rapidraw-main-publication-20261015/ci-artifacts/macos-arm64/extracted/release/bundle/dmg/RapidRAW_1.6.5_aarch64.dmg`
  - SHA256: `cc48f704df823cf7eb80659956f49a34686619cf7bb369421bd5588bd513225e` (Verified matching expected)
  - Original `codesign --verify --deep --strict`: Exit code `1` (Unsealed resource failure)

## 2. Integrity & Dependencies
- **Bundle ID:** `io.github.CyberTimon.RapidRAW`
- **Version:** `1.6.5`
- **otool Dependency Check:** Clean. Only `/System/Library/Frameworks` and `/usr/lib/libSystem.B.dylib` dependencies detected; no unbundled Homebrew or external user paths.
- **License Files Detected:** `Contents/Resources/resources/licenses/AGPL-3.0.txt`, `Contents/Resources/resources/licenses/LGPL-2.1.txt`, `Contents/Resources/resources/licenses/LGPL-3.0.txt`, `Contents/Resources/resources/film_luts/SPEKTRAFILM_LICENSE.txt`
- **Forbidden Assets:** None (0 RAF/DCP/XMP/key assets found).

## 3. Ad-Hoc Sealing & Signing
- **Signature Mechanism:** Local ad-hoc (`-`), `--timestamp=none`, inside-out order for Mach-O binaries.
- **Signed Mach-O Binaries:**
  - `Contents/MacOS/validate_dcp`
  - `Contents/MacOS/RapidRAW`
  - `Contents/Resources/resources/libonnxruntime.dylib`
- **Strict Verification On Stage:** `codesign --verify --deep --strict RapidRAW.app` -> Exit code 0.
- **Exact Diff:** Only `_CodeSignature` and the signed Mach-O executable bytes changed. All Info.plist, resources, icons, and licenses remained bit-for-bit identical.

## 4. Final Repackaged Artifact
- **File:** `/private/tmp/rapidraw-main-publication-20261015/mac-arm64-repack/RapidRAW-Fuji-preview-2026.10.15.1-macos-arm64-adhoc.dmg`
- **Filename:** `RapidRAW-Fuji-preview-2026.10.15.1-macos-arm64-adhoc.dmg`
- **Size:** `38986201` bytes
- **SHA256:** `ee4c275631a087fcd40104e19fce64541f52cd82e74cd20298c805cda5942de5`
- **Format:** UDZO, Volume Label `RapidRAWFujiPreview`
- **Mounted DMG Strict Codesign Verification:** Exit code 0.

## 5. Scope & Policy Compliance
- Ad-hoc signature only (no Developer ID, no notarization claimed, no Gatekeeper bypass claimed).
- Non-tethering standard app only; original input artifacts untouched.
- Ready for independent reviewer verification prior to upload.
