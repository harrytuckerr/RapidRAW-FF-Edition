# README rewrite — receipt (revision 2)

- Date: 2026-10-15
- Author: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5
- Worktree: `rapidraw-fuji-team-20261015/readme-rewrite`, frozen HEAD `05dd25e74256bf0318cf43e6462cfd778bb4de53` (verified)
- Owned writes: `README.md` and this receipt. Private evidence is in `/private/tmp/rapidraw-readme-rewrite-20261015/`. No source, lock, profile asset, LICENSE/NOTICE, device, build/cache, Git stage/commit/push or release action.

## Change

| File | Before sha256 | After sha256 |
|---|---|---|
| `README.md` | `2bdbb2b39d716bcce74032458a42afd25b550134415fa90e8973a67b9db5319b` (1,100 lines) | `654e1e72a7fc1a7c4c35053dc1d7cb9800d579c33e6e2fb1d532329fc13ff5b5` (357 lines) |

Diff vs HEAD: +241/−984 (`readme-r2.diff`, sha256 `5d9f9d7918ef78b89cb1c74eca0384eb44b0f481b3a2837367322e8f54c38f84`). `git diff --check` is clean. Revision 1 (`6454683a…`, receipt `2f00bc39…`) is superseded.

### Revision 2: the lead's six required changes

1. **Upstream binary links removed.** The bottom "Upstream downloads" list (upstream releases, Flathub, AUR) is deleted, and the upstream-section intro no longer mentions download links. The top Download section keeps one caveat, "Installers published by upstream RapidRAW do not include anything described in the next sections," with no binary link. The only download links left are the fork's `/releases` page and its badge.
2. **RAW scope bounded.** "Works with any RAW that RapidRAW decodes" is replaced with: designed for camera-native RAW with an installed base for that camera, and some formats such as linear RAW are not supported with profiles yet, linking to status.
3. **Planned wording.** "None of them exist in the app yet" is replaced with: the workflow additions are not built yet, and some extend tools RapidRAW already has (collage maker, JPEG editing, export).
4. **Comparison.** "No comparison command" is replaced with: colour comparison is not implemented yet, and the developer tool's `compare` command reports itself as unavailable. Checked against `src-tauri/src/bin/validate_dcp.rs:59` ("compare  Unavailable; exits with an explicit error") and `:155`.
5. **Import safety.** The "leaves no half-written image behind" promise is removed. New wording: a completed file is added without replacing anything already in the destination, and a temporary file that can't be cleaned up is reported rather than ignored. There is no crash-durability or no-remnant guarantee.
6. **Credit.** The new line reads "Built on RapidRAW, created by Timon Käch (CyberTimon), and its contributors." This replaces the inaccurate "everything outside the profile work comes from upstream".

The new structure is **RapidRAW — Fujifilm Edition**:

1. **Download:** states that no fork builds are published, links the fork's `/releases` page, and says upstream installers don't contain fork changes.
2. **Why use it**, then **The workflow:** RAW → camera base DCP → optional Look and Amount → presets/masks/LUTs/edits → `.rrdata` and export.
3. **Profiles and presets stay separate:** the `Include Profile` opt-in is off by default, and copy/paste to another camera skips an incompatible profile.
4. **Base and Look pairing:** Flat is not Modular; Looks without their base are disabled with a reason; the camera in a Look title is the emulated camera, not the source camera; source-camera restrictions are checked.
5. **Amount** 0–200%, with 0 bypassing the Look and keeping the base.
6. **What else this fork changes:** discovery and import, the shared route, persistence and migration, cache refresh, clearer errors including the R1 empty and RAF-JPEG messages, and R2a picker staging.
7. **Status and limitations** as a compact table.
8. **Planned (approved, not yet built):** E1–E5 and the limited Drive workspace, followed by an explicit out-of-scope line.
9. **Build from source** using `cobalt/main`.
10. **Licence and credit.**
11. A **"From upstream RapidRAW"** section that keeps features, formats, tethering (build-from-source only), CLI, companion apps, requirements and troubleshooting, contributing, thanks and the licence. It contains no upstream binary links.

Removed:
- the personal author panel ("The Idea" portrait box);
- the upstream download and docs card table, and every upstream binary link (upstream releases, Flathub, AUR, `_tethering` release text);
- upstream social and star badges;
- the long recent-changes history, the development log and current priorities;
- the detailed AI strategy and cloud subscription comparison, condensed into a neutral line;
- crypto donation addresses, replaced by one line linking to upstream support;
- the stale fork section (its obsolete status table and link to the historical STATUS report).

Attribution to Timon Käch/CyberTimon, the upstream repo link, the AGPL `LICENSE` and `NOTICE.md` links, and the upstream thanks list are retained.

## Remote release findings (queried 2026-10-15 07:03:59 UTC, GitHub REST API via `gh`, plus `curl`; release count rechecked 07:13:13 UTC: still 0)

- `repos/harrytuckerr/RapidRAW` resolves (HTTP 200) to `harrytuckerr/RapidRAW-Fujifilm-edition`: public fork of `CyberTimon/RapidRAW`, AGPL-3.0, issues enabled, `pushed_at` 2026-10-15T04:57:48Z.
- `GET /releases` returns **0** releases. `GET /tags` returns **0**. `GET /releases/latest` returns **404**. The web page `https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/releases` returns 200, and the old `harrytuckerr/RapidRAW/releases` redirects there. The shields release badge renders "no releases found".
- Branches:
  - `main` is at `e4d3c8133168` (2026-08-20, an upstream README commit). GitHub's default branch is still `main`.
  - `cobalt/main` is at `4c1e965a53de` = PR #14 merge (base `cobalt/main`, merged 2026-10-15T04:57:48Z).
  - Local commits `8877d7ea…05dd25e7`, including R1 and R2a, are **not on the remote** as of this check.
- Workflows: CI Build, Cobalt CI, Lint and PR CI exist. CI artifacts are not releases and are not linked.

So the README links only the fork's `/releases` page (`https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/releases`) and says no builds are published. No asset names, versions, signing or platform availability are claimed, and no claim is made that local unpublished changes are in any binary. No upstream binary links remain anywhere in the README. The inherited `_tethering` release download text was replaced with build-from-source instructions.

## Claims check (sources)

| README claim | Verified against |
|---|---|
| Profile slot at the top of the editing controls | `src/components/panel/right/ControlsPanel.tsx:291-299` (ProfilePanel first in scroll area) |
| `Include Profile` opt-in, default off | `src/i18n/locales/en.json:1262`; `ConfigurePresetModal.tsx:113` (`useState(false)`) |
| Desktop discovery folders, Import, Android SAF import, pairing message, Amount, copy to another camera, no silent fallback, capture/unsupported variants error, clamp | `docs/profiles.md` (H6b-accepted) |
| Shared route for preview/thumbnail/export | `lib.rs:346` (`render_selected_profile`); `file_management.rs:1586,1771`; `export_processing.rs:1772` (`load_and_composite_with_profiles`) |
| Persistence and migration, cache invalidation, preview-error propagation and superseded jobs, rawler vs Apple RAW, linear RAW rejected, CPU profile stage with GPU shader parity-only | Orchestrator handover §6, ledger and `docs/BUILD.md` |
| R1 errors and R2a picker staging in source | Base contains `8a4f02bc` (R1) and `05dd25e7` (R2a). `formats.rs` `66134f6c…`, `image_loader.rs` `fcf43150…`, `file_management.rs` `e66dceb6…` |
| Import status can still show success; local-path imports not hardened | R2a review and receipts (R3 and R2b open) |
| Fold user-reported pass; DCP/Look/export on device not exercised; Adobe not verified; colour comparison not implemented (`compare` reports unavailable) | Ledger physical-device addendum; `docs/profiles.md`; `docs/BUILD.md`; `validate_dcp.rs:59,155` |
| Import publishes completed bytes without replacing existing files; cleanup failures reported | R2a revision `publish_import_bytes` (`file_management.rs` `e66dceb6…`) and R2a review |
| X-T5 case was JPEG data named `.RAF`; native X-T5 untested, not called unsupported | Ledger and accepted H1R device/payload receipts |
| Planned E1–E5 and Drive limits; out-of-scope promises | Handover §15 and §16.1 |
| npm scripts `start`, `start:tethering`, `tauri` | `package.json` |

No historical STATUS or `dcp-pipeline.md` math or Done tables were copied. No completion percentage, Adobe match, HDR, GPU performance or all-platform claim is made.

## Link validation

Rechecked on revision 2:

- **Relative links and anchors:** 16 checked by script (GitHub-style slugging, code blocks excluded). All resolve: `LICENSE`, `NOTICE.md`, `docs/profiles.md`, `docs/BUILD.md`, `docs/integration/TEAM-LEDGER-2026-10-15.md`, plus anchors `#build-from-source`, `#status-and-limitations` (now linked twice), `#camera-tethering` and `#command-line-interface-cli`.
- **External links:** 41 unique, checked with `curl -L` (`ext-status-r2.txt`, sha256 `5e3570bfe532df01cbb44a3ab7b8c0796686321bcbe9ab98028ee1f0cffaad6a`; list `ext-links-r2.txt`, sha256 `8d68ef6b5f4e2e8a62b99f3731a6c2582c14386c634d9b484b24282913bf210b`). 40 returned 200. `https://ko-fi.com/cybertimon` returned **403** to the scripted request, likely bot protection. It is upstream's own support link and was not verified in a browser.
- Upstream screenshots (`raw.githubusercontent.com/CyberTimon/RapidRAW/assets/...`) are labelled as upstream screenshots in their captions and alt text.

## Points for lead decision (not changed by this task)

1. GitHub's default branch is `main`, which holds an old upstream README. This README only appears on the repo front page if it lands on the default branch, or if the default branch is switched to `cobalt/main`.
2. The README describes R1/R2a as "in the source". They exist locally (`05dd25e7`) but are not yet on remote `cobalt/main`. Push timing decides when that statement is true remotely.

## Gates

| Item | Status |
|---|---|
| README rewritten as the fork's product README, with workflow, separation, pairing, Amount, fork changes, status, planned items, source build and credit | Pass (docs) |
| Lead revisions 1–6 applied | Pass (docs) |
| All upstream binary CTAs removed; fork downloads accurate (none published) | Pass (live API check above) |
| Relative links and anchors; external links | Pass (16/16; 40/41 HTTP 200, ko-fi 403 noted) |
| `git diff --check` | Pass |
| Rendered GitHub preview | Not exercised |
| Runtime, build, device | Not exercised (docs-only, not required) |
