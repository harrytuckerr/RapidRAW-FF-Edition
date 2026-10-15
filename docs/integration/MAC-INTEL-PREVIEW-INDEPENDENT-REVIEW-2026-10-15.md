# Intel standard preview: Accept

Independent reviewer: GPT-6 Astra (`gpt-6-astra`). This accepts only the frozen **standard x86_64 ad-hoc development preview**. No blocking package defect found. Source head `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`; GitHub run `38708083220`, artifact `11962992400`.

Ready file: `/private/tmp/rapidraw-main-publication-20261015/mac-intel-repack/RapidRAW-Fuji-preview-2026.10.15.1-macos-x86_64-adhoc.dmg` — **43,985,589 bytes**, SHA-256 `41c152ba8934b83dc3f43591b7c60b3b4cec3a93f5fc250f8ef774ec78b1fcb7`.

## Executed package verification

- **Pass:** GitHub artifact metadata matches run/source/ZIP digest and size. ZIP CRC passes; its sole DMG entry matches the on-disk original, **38,370,929 bytes**, SHA-256 `8fdb6f57b9abfb51b33594a09508c045b2e27423f91351935b955febd928709e`. The worker chat's different original hash is incorrect; frozen receipts and actual bytes agree.
- **Pass:** `hdiutil verify` exits 0 for original and output. Both mounted using `-readonly -nobrowse -noautoopen`; volume information confirms not writable. Only these mounts were detached, both exit 0, and their owned mountpoints are now empty. Initial free space was 29,534,072,832 bytes; more than 6 GiB remains.
- **Fail (original, expected):** strict deep codesign verification exits 1, reporting **“code object is not signed at all”**, rather than the worker receipt's unsealed-resource wording. All three original Intel Mach-O files lack `LC_CODE_SIGNATURE`.
- **Pass (repack):** app `codesign --verify --deep --strict --verbose=4` and individual verification of RapidRAW, validate_dcp and libonnxruntime.dylib all exit 0. Display reports `Signature=adhoc`, `TeamIdentifier=not set`, sealed resources; no entitlements were emitted for the three code files.
- **Pass:** all three code files are x86_64. Info.plist is byte-identical: identifier `io.github.CyberTimon.RapidRAW`, version/build `1.6.5`, minimum macOS `13.0`. Actual dependency load commands contain only `/System/Library/` and `/usr/lib/` dependencies, with no Homebrew or other external non-OS load path. Runtime-selected dependencies were not exercised.
- **Pass:** exactly the three Mach-O files changed and `Contents/_CodeSignature/CodeResources` was added; nothing removed. All remaining resource bytes, modes and symlink identities match. Six LUTs and AGPL-3.0, LGPL-2.1, LGPL-3.0 and SPEKTRAFILM license files are retained. Bounded private-asset filename and private-key/token/table byte-signature scans found no matches; this is not universal secret or rights attestation.

## Independent binary comparison

`compare-macho.py` independently parsed both sets of Mach-O bytes. All original load commands remain identical apart from proven `__LINKEDIT` size fields; one 16-byte `LC_CODE_SIGNATURE` command occupies previously zero header padding. Header command count/size increases are exactly one/16. The original file extent remains intact. Only those header/signature-command/linkedit-size fields were normalized before comparing **every original payload byte** against the new prefix. Each new suffix is exactly eight zero alignment bytes followed by its signature blob, with validated signature and linkedit extents/page bounds. All section metadata and contents match: **20 / 18 / 22 sections** respectively. This demonstrates signature-only changes for these concrete binaries; the signed files themselves are not bit-identical.

## Limits and custody

App and DMG `xcrun stapler validate` each exit 65: no stapled ticket observed. **Developer ID signing: absent. Online notarization status: not established. Gatekeeper, installation, Intel/Rosetta execution, UI, import/export and Adobe/fidelity acceptance: Not exercised.** No app or included executable was launched, and no signing, repack, source, OS-policy, quarantine, device or publication mutation was performed.

Worker receipts lack a complete operator command/environment log. No historical commands are invented here; independent commands, exact argv/exits and outputs are retained in `commands.json`, `bundle-comparison.json` and adjacent logs. Reviewer environment: macOS 26.5.2 arm64, Python 3.9.6. Scripts and evidence hashes are indexed in `review.json`. This evidence gap does not block acceptance of the independently verified frozen output.

Worker receipt hashes: JSON `4b90b68b9c3456e79f3a1c49cb9b98275482455cbccfa792dd75d7752f0af278`; Markdown `4059e73573d88d004fcb8d4defaf47ed2e33a880ea2e7d521f5b63066a19a5c2`. Original ZIP SHA-256 `e04c0b9d61ca8b85937022a3233feadd9baf43cd9b126e833de79647d9eef634`. Finalization rehashed every input/receipt and repeated ZIP CRC/entry-custody checks; all remained frozen.
