# Packaging Notices & Attribution Inventory (Revised)

**Date:** 2026-10-15  
**Task ID:** `01a13f85-7170-7a53-9a94-2dfb589928f6`  
**Repository Branch / Commit:** Detached worktree `packaging-notices-audit` at commit `9b9b1b7933e39d2c347badf00a7a571ad0c843d5`  
**Scope:** Read-only documentation inventory of attribution, license, and notice inputs across desktop and mobile packaging bundles. No source code, locks, build configuration, or packaging scripts were modified.

---

## 1. Executive Summary & Findings

An independent review of recent local desktop packaging (`/private/tmp/rapidraw-ui1-mac-review-20261015/review.md`) observed that while primary licenses (`AGPL-3.0.txt`, `LGPL-2.1.txt`, `LGPL-3.0.txt`, `SPEKTRAFILM_LICENSE.txt`) are present in packaged desktop bundles under `Resources/resources/`, two attribution items are currently absent from bundled artifacts:
1. **The root `NOTICE.md`** (which includes upstream copyright, third-party acknowledgments, and fork scope disclosures).
2. **A dedicated Lensfun database license / attribution document** covering the files bundled from `src-tauri/lensfun_db`.

This revised inventory maps all current attribution assets, verifies exact file hashes and byte counts, incorporates verified central directory inspection from prior accepted Android APKs, clarifies how desktop and mobile builds consume these resources, and identifies the exact minimal packaging seam for a future owner to incorporate missing attribution files without touching application logic or binary dependencies.

---

## 2. File & Provenance Inventory

| File Path in Repository | Status in Repo | Status in Observed Local macOS Bundle | Status in Packaged Android APK (Observed in R3 APK `b56d8bdc…`) | Size (Bytes) | SHA-256 Digest | Provenance / Origin |
|---|---|---|---|---|---|---|
| `NOTICE.md` | Present at repo root | **Missing** | **Missing** | 3,027 | `2d25b9e268336959863894a46aa03f619d2d563f5b518db7db4827c59c098592` | RapidRAW fork project notices, upstream attribution to Timon Käch (AGPL-3.0), and Cobalt/Adobe trademark disclosures. |
| `LICENSE` | Present at repo root | **Missing** (covered by `resources/licenses/AGPL-3.0.txt`) | **Missing** | 34,523 | `8486a10c4393cee1c25392769ddd3b2d6c242d6ec7928e1414efff7dfb2f07ef` | GNU Affero General Public License v3.0 (standard AGPL text). |
| `src-tauri/resources/licenses/AGPL-3.0.txt` | Present | **Bundled** (`Resources/resources/licenses/`) | **Bundled** (`assets/resources/licenses/`) | 34,522 | `4df3c306dddaaf4baffdff5ca820cc679ac8cd6dc263c6a74517783e42fa7a3b` | Primary project license copy for bundled app distribution. |
| `src-tauri/resources/licenses/LGPL-2.1.txt` | Present | **Bundled** (`Resources/resources/licenses/`) | **Bundled** (`assets/resources/licenses/`) | 26,418 | `dcbea5e0d91fbe14fbdfcba9a478013d346a1bb8fb0007df87bfe0f14ef09b88` | Upstream dependency license. |
| `src-tauri/resources/licenses/LGPL-3.0.txt` | Present | **Bundled** (`Resources/resources/licenses/`) | **Bundled** (`assets/resources/licenses/`) | 7,651 | `a5681bf9b05db14d86776930017c647ad9e6e56ff6bbcfdf21e5848288dfaf1b` | Upstream dependency license. |
| `src-tauri/resources/film_luts/SPEKTRAFILM_LICENSE.txt` | Present | **Bundled** (`Resources/resources/film_luts/`) | **Bundled** (`assets/resources/film_luts/`) | 23,677 | `76d629501783ae52e8d63aee1e054c1e1a9713b9fe7d36fb306e25b1d14b24e9` | License accompanying the 6 bundled `.cube` film LUT presets. |
| `src-tauri/lensfun_db/` (59 files total: 56 `.xml`, `lensfun-database.dtd`, `lensfun-database.xsd`, `timestamp.txt`) | Present | **Bundled** (`Resources/lensfun_db/`) | **Bundled** (`assets/lensfun_db/` in R3 APK; also embedded in native binary via `include_dir!`) | ~1.6 MB total | Multiple (e.g. `compact-fujifilm.xml`: `58af9ebef2410c42…`, `timestamp.txt`: `3f03b290…`) | Upstream Lensfun database snapshot added by upstream commit `d2af03d3` ("lens correction using lensfun"). |
| Lensfun DB License / Attribution | **Missing** | **Missing** | **Missing** | — | — | Upstream Lensfun database license not present in tree; exact upstream licensing terms for this database snapshot are unverified locally (Unknown). |

*Note on Mobile Packaging:* The Android observations above are from direct zip central-directory inspection of the prior accepted R3 Android debug APK (`/private/tmp/rapidraw-r3-android-20261015/RapidRAW-R3-2a734622-android-debug.apk`, 117,724,369 bytes, SHA-256 `b56d8bdcd7bc108197f4f34558ac43e368a5117290eb29fb1eeb5c35c006c516`). The UI1 Android APK was not examined by this worker audit; its later build/install evidence is recorded separately.

---

## 3. Packaging Mechanics: Desktop vs. Mobile

### 3.1 Desktop (macOS, Linux, Windows)
- Configuration: `src-tauri/tauri.conf.json` defines:
  ```json
  "bundle": {
    "resources": [
      "resources",
      "lensfun_db"
    ]
  }
  ```
- Tauri copies everything in `src-tauri/resources` and `src-tauri/lensfun_db` verbatim into the application bundle's `Resources` folder on macOS (and parallel asset paths on Linux/Windows).
- Because `NOTICE.md` and `LICENSE` reside at the repository root (`/NOTICE.md`, `/LICENSE`), Tauri does not include them unless they are explicitly referenced in `tauri.conf.json` or placed inside `src-tauri/resources/`.

### 3.2 Mobile (Android)
- In `src-tauri/src/lens_correction.rs`:
  ```rust
  #[cfg(target_os = "android")]
  use include_dir::{Dir, include_dir};
  #[cfg(target_os = "android")]
  static LENS_DB_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/lensfun_db");
  ```
- On Android, the Lensfun database is directly compiled into the native shared library via the `include_dir!` macro, and Tauri's Android asset bundler also packages the 59 files under `assets/lensfun_db/` along with `assets/resources/licenses/` as confirmed by APK inspection.
- The root `NOTICE.md` is omitted from the APK package.

---

## 4. Minimal Packaging Seam for Future Remediation

To address the missing attribution items in future packaging tasks without modifying application source code or build dependencies:

1. **Inclusion of `NOTICE.md`:**
   - **Option A (Tauri Config Mapping):** In `src-tauri/tauri.conf.json`, update `bundle.resources` to include `"../NOTICE.md"`, or map it to `"resources/NOTICE.md"`.
   - **Option B (Resource Directory Mirror):** Place or symlink a copy of `NOTICE.md` directly inside `src-tauri/resources/licenses/` or `src-tauri/resources/NOTICE.md`.

2. **Inclusion of Lensfun Database License:**
   - Once the exact official upstream license for the Lensfun database version in use is confirmed from primary sources, place the license text file inside `src-tauri/resources/licenses/` (e.g. `src-tauri/resources/licenses/LENSFUN_DB_LICENSE.txt`).
   - Placing it under `src-tauri/resources/licenses/` uses the existing `"resources"` rule; verify its actual inclusion in every target artifact after the change.

---

## 5. Verification Commands & Record

The following read-only commands were used to audit the repository state and produce these findings:
```bash
# 1. Inspect tauri bundle resources definition
python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['bundle']['resources'])"

# 2. Check presence and digests of repository license/notice files
shasum -a 256 NOTICE.md LICENSE src-tauri/resources/licenses/* src-tauri/resources/film_luts/SPEKTRAFILM_LICENSE.txt

# 3. Check Lensfun database contents and file count breakdown
ls -la src-tauri/lensfun_db/
python3 -c "import os; files = os.listdir('src-tauri/lensfun_db'); print('Total:', len(files), 'XML:', len([f for f in files if f.endswith('.xml')]), 'Non-XML:', [f for f in files if not f.endswith('.xml')])"

# 4. Inspect prior accepted Android debug APK zip central directory
python3 -c "import zipfile; z = zipfile.ZipFile('/private/tmp/rapidraw-r3-android-20261015/RapidRAW-R3-2a734622-android-debug.apk'); print('Lensfun:', len([n for n in z.namelist() if 'lensfun' in n.lower()]), 'Licenses:', [n for n in z.namelist() if 'licen' in n.lower()])"
```

## 6. Gates & Scope Compliance
- **Static Documentation & Hash Audit:** **Pass**
- **Binary/Packaging Mutation:** **None** (zero files modified in product code, build configs, or release artifacts)
- **Runtime/Distribution Acceptance:** **Not exercised** (deferred to future implementation task)

## Lead precision addendum

Root read the revised report and verified the 59-file/56-XML inventory and prior APK resource names. The artifact observations above concern the local macOS preview and named prior Android APK; Windows/Linux package contents were not opened by this audit. Configuration and proposed seam descriptions are not proof that a future build includes complete notices. Exact licence terms of the bundled Lensfun snapshot remain Unknown pending primary-source confirmation. This is an inventory, not distribution/legal acceptance. The original worker revision SHA-256 was `496d961b37f835c34f2ff3d5d7300511e421b9af025734ad113e3a5dbee0bbac`; root clarified artifact scope and removed the stale UI1-build-in-progress sentence.
