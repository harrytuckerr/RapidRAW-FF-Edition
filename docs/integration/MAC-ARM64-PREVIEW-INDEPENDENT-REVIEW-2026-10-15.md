# Standard Mac ARM64 repack — independent review

**Accept these exact bytes as an explicitly ad-hoc development-preview package.** The missing resource seal is repaired: strict bundle verification passes. Code payload and non-code resources are preserved; no external non-OS library load remains in this standard variant. This is not Developer ID, notarization, Gatekeeper or runtime acceptance. Tethering remains excluded under its separate failed audit.

Reviewer: Codex CLI - Astra, actual `gpt-6-astra`. Operator reported as Antigravity/Gemini3.8. Only the new `/private/tmp/rapidraw-main-publication-20261015/mac-arm64-repack-review/` scratch/reports were written. No signing/repacking, source/Git/release mutation, app/CLI launch, UI/device, quarantine or OS-policy action by reviewer.

## Ready file and custody

**One ready file:**

`/private/tmp/rapidraw-main-publication-20261015/mac-arm64-repack/RapidRAW-Fuji-preview-2026.10.15.1-macos-arm64-adhoc.dmg`

**38,986,201 bytes; SHA256 `ee4c275631a087fcd40104e19fce64541f52cd82e74cd20298c805cda5942de5`.** Independently rehashed before and after inspection; file unchanged.

GitHub original artifact11960912274 metadata confirms run38708083220/source **2a7346229bc0cc398ee7e3fee126eb89428b7dd9**. Original ZIP SHA `1f0c0dabd4434dc24970290efd38c0e2d81e5662e8c769616f731a95e3eb5b91` matches GitHub and size; ZIP CRC passes and its sole DMG entry hashes to original **`cc48f704df823cf7eb80659956f49a34686619cf7bb369421bd5588bd513225e`**. Worker receipts independently matched JSON `e3dd2a2892cf73e71adab0e4c0910dc6996b6b2aa89ec597cd0169065990f98f` and MD `0822b7efd5e0360082727beb82c48c2d3b2564681a78495eb060f21c8ebcedda`; both read fully.

## Independent checks actually executed

- `hdiutil verify` passed on original and repacked DMGs. Both mounted with **-readonly -nobrowse -noautoopen** at separate owned mountpoints; `diskutil info -plist` confirmed nonwritable volumes.
- Reproduced original `codesign --verify --deep --strict --verbose=4` failure (exit1). The mounted repack passes the same command (exit0), “valid on disk” and designated requirement satisfied. Individual main/CLI/ONNX signature verification also exits0.
- Repacked signature is **ad-hoc**, TeamIdentifier **not set**, Info.plist bound, Sealed Resources version2/files72. `codesign --display --entitlements :-` reports no entitlements for all three code files. No Developer ID certificate/identity is present.
- `xcrun stapler validate` exits65 for repacked app and DMG: neither has a stapled ticket. No notarization-service or Gatekeeper launch assessment was performed. Do not infer trust/notarization from the strict-signature pass.
- Info.plist remains byte-identical: `io.github.CyberTimon.RapidRAW`, version/build1.6.5, minimum macOS13.0. Main executable, validate_dcp and ONNX are arm64. Parsed Mach-O dependency load commands reference only `/System/Library/` or `/usr/lib/`; no Homebrew/user-path dependency. ONNX's `@rpath` install identity is LC_ID_DYLIB, not an external load.
- Inventory comparison shows **only three changed files** (main, validate_dcp, ONNX) and one addition (`Contents/_CodeSignature/CodeResources`), with no removals. Every other regular file's bytes and mode match, including Info.plist, icons, all six licensed LUTs, Spektrafilm/AGPL3/LGPL2.1/LGPL3 texts. No forbidden RAW/DCP/XMP/key/test-assets paths. This preserves the independently audited original resource contents.

## Changed executable bytes: verified, not assumed equivalent

`compare-macho.py` independently parses the three thin64-bit Mach-O files. It verified **19/17/23 section records and contents respectively are identical**, load-command layouts and signature-start offsets are unchanged, and **every byte before the signature blob is identical after normalizing only three size fields**: LC_CODE_SIGNATURE.datasize and __LINKEDIT.filesize/vmsize. The __LINKEDIT file extent ends exactly at the signature end; virtual size equals16KiB-rounded file size both before and after. Neither file has trailing payload after its signature. No other header, code, constant, symbol/link-edit data or load-command bytes differ.

The signatures themselves intentionally differ; these binaries are **not bitwise identical**. The normalized comparison isolates the changes to signature data and the directly corresponding size metadata, instead of treating arbitrary changed Mach-O files as equivalent. Raw per-file hashes, section hashes, changed size values and comparison script are retained in `macho-comparison.json` and `compare-macho.py`.

## Evidence limits and cleanup

The worker receipt does not contain a complete operator command/environment log. This remains a provenance/reproducibility limitation; I have not invented that log or claimed to reproduce the operator's signing commands. It is **not a material blocker for these frozen bytes** because this review independently verifies input custody, full non-code identity, normalized executable payload identity, mounted seal validity and dependencies. Reviewer commands/exit statuses are recorded in `commands.json`; reviewer environment is macOS26.5.2 arm64/Python3.9.6 in `environment.json`. Full tool outputs and inventories are retained.

Owned repack backing image **/dev/disk7** and original **/dev/disk6** were detached in `finally`, both exit0/ejected; both owned mountpoints are empty. The APFS virtual volume created by the repack mount belonged to that image; no unrelated mount was touched.

**Pass:** custody/checksums, original-failure reproduction, repacked strict/individual code verification, resource/Info.plist/license preservation, constrained Mach-O differences, OS-only linked dependencies and cleanup. **Not exercised:** app/CLI runtime, Gatekeeper launch, notarization service record, device/profile/export/Adobe fidelity. No additional artifact or package reviewed.

Lead alone may stage/upload the exact ready file with explicit ad-hoc preview labeling and these limits. Root's source/CI evidence remains separate from this packaging repair. `review.json` contains machine-readable results and evidence hashes. Stop at frozen report.
