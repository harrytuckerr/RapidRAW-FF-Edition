# H0 Asset Inventory — Local Fuji RAW, Model and Paired-JPEG Availability

Date: 2026-10-15
Task: H0b (read-only metadata inventory)
Base: detached `4c1e965a53de9c835df920b2e3d69c2c8a864708` (worktree `asset-inventory`, clean before this edit)
Executor: Claude Code - Haiku (claude-haiku-5-5, as reported in the agent environment; configuration file not inspected)

## Summary

- **Fuji X-Pro2 RAF inventory: Pass.** 356 `.RAF` files under `~/Documents`, all with the `FUJIFILMCCD-RAW` magic and camera field `X-Pro2` (header offset 28, read directly). Format version `0201` on all 356.
- **Real X-Pro2 RAW+JPEG pairs: Not available (finding: count 0).** 0 of 356 RAF files have a same-basename JPEG among the files scanned under `~/Documents` (basename match, case-insensitive). This does not prove that no renamed or locally stored JPEG exists elsewhere. No pair was selected, so no pair SHA256 or size was recorded.
- **Adobe-reference settings: Partial, not a controlled Adobe reference.** One Fuji RAF has an Adobe Camera Raw settings sidecar: `DSCF2029.xmp` (XMP, not TIFF). It may support a base/Look comparison test, but it is not a controlled Adobe reference. No TIFF under `~/Documents` has Fuji camera provenance. The one TIFF candidate (`Zero.tif`) carries Adobe Lightroom 3.2 settings but no RAW link and no Fuji make/model, so it is not a Fuji reference.
- **Modular DCP: Pass for file identity** (SHA256 matches, `Fujifilm X-Pro2` and `Cobalt Modular` strings present). The internal `policy2` tag was not checked in this task (see Not exercised).
- **Downloads `IMG_0138.DNG`: Not Fuji** (Apple / iPhone 13 Pro, confirmed from TIFF IFD0 tags). Confirmed identical to the copy under `CoWork/Pokemon/docs/_ref/jess/photos-20261005/`.
- **Next-consumer readiness: Not ready** for any RAW+JPEG pair consumer. A paired X-Pro2 JPEG source is missing.

## Reconciliation with lead count (357)

The lead's 357 RAW/reference filename candidates are reproduced as **356 RAF + 1 DNG** under `~/Documents`. Other RAW-like files found, not Fuji:

| Extension | Count | Location | Camera (IFD0 Make / Model) |
|---|---|---|---|
| `.RW2` | 66 | `Documents/Moments` (63), `Documents` (2), `Documents/ArnieSpace` (1) | Panasonic / DC-S5 |
| `.DNG` | 1 | `CoWork/Pokemon/docs/_ref/jess/photos-20261005/IMG_0138.DNG` (identical to `~/Downloads/IMG_0138.DNG`) | Apple / iPhone 13 Pro |
| `.TIF` | 1 | `MacBook Backuo/Downloads/Zero.tif` | none in IFD0 (see Adobe section) |

## RAF inventory by folder (header-verified)

| Folder (absolute) | RAF count | Total bytes | Camera (header) | Same-basename JPEG |
|---|---|---|---|---|
| `/Users/harrisontucker/Documents/Photos - Personal/DCIM/211_FUJI` | 105 | 5,308,447,232 | X-Pro2 (105/105) | 0 |
| `/Users/harrisontucker/Documents/Photos - Personal/DCIM/212_FUJI` | 124 | 6,268,865,024 | X-Pro2 (124/124) | 0 |
| `/Users/harrisontucker/Documents/Groove/RAW` | 127 | 6,422,787,072 | X-Pro2 (127/127) | 0 |
| **Total** | **356** | | **X-Pro2 (356/356)** | **0** |

The `212_FUJI` folder also contains `DSCF2007.MOV` (960,911,168 bytes) and `DSCF2029.xmp`. No JPEG exists in `211_FUJI` or `212_FUJI`. `Groove/RAW` contains only RAF files.

JPEG context: 720 `.jpg`/`.jpeg` files exist under `~/Documents`. None share a basename with any RAF. Folder counts are for context only; camera model was taken from the file header, not the folder name.

## Candidate pairs

- X-Pro2 RAF + same-basename JPEG: **none found among scanned `~/Documents` files**. Nothing selected. This does not rule out a renamed or locally stored JPEG elsewhere.
- Reference-settings candidate (not a RAW+JPEG pair, for settings comparison only):

| Field | Value |
|---|---|
| RAW | `/Users/harrisontucker/Documents/Photos - Personal/DCIM/212_FUJI/DSCF2029.RAF` |
| RAW size | 50,262,016 bytes |
| RAW SHA256 | `b64d06dd639cea79792751b70a1409eae5c08108e0db5a5180bb1596085b9abb` |
| RAW camera | X-Pro2 (header offset 28; magic `FUJIFILMCCD-RAW`, version `0201`) |
| Sidecar | `/Users/harrisontucker/Documents/Photos - Personal/DCIM/212_FUJI/DSCF2029.xmp` |
| Sidecar size | 227,558 bytes |
| Sidecar SHA256 | `a9c39357e8b6e02309ae0fee6ed0524b9f78e0b0118073507aa7b1d14b17ecbe` |
| Sidecar fields | `crs:RawFileName="DSCF2029.RAF"`, `crs:CameraProfile="Cobalt Modular"`, `crs:CameraProfileDigest="89281DFD6A7A6562A42FC91DE45B1E9C"`, `crs:ProcessVersion="15.4"`, `crs:Version="18.2"`, `crs:HasSettings="True"`, `crs:HasCrop="True"`, `crs:Exposure2012="0.00"`, `tiff:Make="FUJIFILM"`, `tiff:Model="X-Pro2"`, `xmp:CreatorTool="Digital Camera X-Pro2 Ver3.12"` |

Provenance note: the `crs:` namespace (camera-raw-settings) is Adobe's. `xmp:CreatorTool` names the camera, not the writing application, so the exact writing app and version are **not proven** by this inventory. Filename alone was not used as provenance. The sidecar is the only XMP in `~/Documents` whose `crs:RawFileName` is a Fuji RAF; 47 XMP files in total carry camera-raw-settings, all others are unrelated to Fuji.

## Adobe-reference TIFF check

- `MacBook Backuo/Downloads/Zero.tif` (12,606,259 bytes, SHA256 `33d0f53c0743477335c48f77cb110697a1dc6650c705cb0cfdcd414a35555741`): TIFF with embedded XMP packet, `xmp:CreatorTool="Adobe Lightroom 3.2 (Macintosh)"`, `crs:Version="12.2"`, `crs:ProcessVersion="11.0"`, `crs:CameraProfile="Embedded"`, `crs:RawFileName=""`. No Make/Model in IFD0; no source RAW link. **Not a Fuji reference; no provable Fuji settings.** It shares a basename with `MacBook Backuo/Downloads/zero.JPG` (255,284 bytes), which is not Fuji either.
- No other TIFF exists under `~/Documents` (the only other TIFF-family file is `Zero.tif` above).

## Modular DCP

- Path: `/Users/harrisontucker/Pictures/Lightroom Library.lrlibrary/eefc35bd2aff4c438f63a1a74bcf2265/cr_cameraprofiles/Fujifilm X-Pro2 Cobalt Modular v3.0.dcp`
- Size: 65,350 bytes
- SHA256: `4b80ed63b210e5738cde04430af1511c3e12e2bb53bf2471b4bb0750dceb87f3` (matches the value in the task)
- Strings present: `Fujifilm X-Pro2`, `Cobalt Modular`
- Read only. No Adobe UI, login, catalog, or DCP edits.

## Commands run (all read-only; scratch scripts outside the repository)

```text
cd /Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/asset-inventory
git rev-parse HEAD                      # 4c1e965a53de9c835df920b2e3d69c2c8a864708
git status --porcelain | wc -l          # 0 before this edit
git symbolic-ref -q HEAD || echo DETACHED   # DETACHED
dd if=<file> bs=1 skip=28 count=32      # RAF camera field (header only)
dd if=<file> bs=1 count=16              # RAF magic
find ~/Documents -iname '*.raf' ...     # RAW/JPEG survey (directory listing only)
python3 -I /tmp/h0b-inventory/scan.py   # header-only TIFF/RAF metadata, basename pairing (scratch, outside repo)
shasum -a 256 <DCP> <sidecar> <RAF> <DNG> <TIF>
strings -n 6 <DCP> | grep -E 'X-Pro2|Cobalt Modular|Fujifilm'
grep -o -E 'crs:...' <sidecar>          # XMP fields
```

No photo was decoded. No original was modified. No photo or vendor bytes were copied into the repository. No dependencies were installed and no build was run.

## Evidence and results

| Check | Result |
|---|---|
| Worktree at frozen base, detached | Pass |
| RAF magic on all 356 RAF | Pass (356/356) |
| RAF camera field = X-Pro2 | Pass (356/356) |
| RAF folder counts (211 / 212 / Groove) | Pass (105 / 124 / 127) |
| Same-basename JPEG for any X-Pro2 RAF among scanned `~/Documents` files | Finding: count 0 (0/356); not a test Fail |
| Fuji RAW+JPEG candidate pair selected | Not available (no input) |
| Adobe crs sidecar for a Fuji RAF | Pass (DSCF2029.xmp) — writing app version not proven; not a controlled Adobe reference |
| Adobe reference / RAW+JPEG workflow test | Not exercised (input unavailable; no camera-rendered JPEG) |
| TIFF with provable Fuji Adobe settings | Finding: none found (Zero.tif is not Fuji) |
| DCP SHA256 matches task value | Pass |
| DCP internal strings (camera, profile name) | Pass |
| DCP internal `policy2` embed tag | Not exercised (not checked in this task) |
| Downloads `IMG_0138.DNG` = iPhone 13 Pro | Pass (not Fuji) |
| Haiku model identity | Reported by environment; configuration not inspected |

## Next-consumer readiness

- **Not ready** for a RAW+JPEG pair consumer: no X-Pro2 JPEG source exists locally.
- **Partially ready** for a base/Look comparison consumer: `DSCF2029.RAF` + `DSCF2029.xmp` are a documented candidate. It is not a controlled Adobe reference. Decoding or rendering either is outside H0b and needs a separately scoped lead assignment; this boundary does not imply additional user permission is required.
- **E3 (camera-rendered JPEG reference)** needs an actual camera-rendered JPEG. An arbitrary exported JPEG with a matching basename does not satisfy it.
- To unblock the pair consumer, a camera-rendered X-Pro2 JPEG matched to a 211/212 RAF, or a user-supplied matched pair, is needed.

## Changed-document receipt

- Changed file: `docs/integration/H0-ASSET-INVENTORY-2026-10-15.md` (new, only file changed)
- Diff: new file; `git status --porcelain` should show only `?? docs/integration/H0-ASSET-INVENTORY-2026-10-15.md`
- Document SHA256: recorded in the team report after writing
