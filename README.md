<p align="center">
  <img src="https://raw.githubusercontent.com/CyberTimon/RapidRAW/assets/.github/assets/editor.jpg" alt="RapidRAW editor (upstream screenshot)">
  <br>
  <sub>Editor screenshot from upstream RapidRAW. The HT Edition adds camera-profile and adjustment-preset workflows to this upstream interface.</sub>
</p>

<div align="center">

[![AGPL-3.0](https://img.shields.io/badge/License-AGPL_v3-blue.svg?style=for-the-badge)](LICENSE)
[![RapidRAW HT Edition](https://img.shields.io/badge/fork-RapidRAW%20HT%20Edition-555?style=for-the-badge&logo=github)](https://github.com/harrytuckerr/RapidRAW-HT-Edition)
[![Development preview](https://img.shields.io/github/v/release/harrytuckerr/RapidRAW-HT-Edition?include_prereleases&label=preview&style=for-the-badge)](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/tag/fuji-preview-2026.10.16.1)

</div>

# RapidRAW HT Edition — development preview 2026-10-16

**Your own camera profiles and Looks, underneath a fast, modern RAW editor — on desktop and Android.**

RapidRAW HT Edition is a fork of [RapidRAW](https://github.com/CyberTimon/RapidRAW) by Timon Käch. Its **Profile** slot separates an Adobe-format camera base profile (DCP) for the camera that took the photo from an optional compatible Look such as a Cobalt Image XMP. Presets, masks, curves, creative LUTs and export apply on top of that rendering. This preview also includes separate Camera Profile and Adjustment Presets workflows.

The profile workflow is not Fuji-specific. It is intended for camera-native RAW files with an installed base profile made for that camera. Some formats, such as linear RAW, are not supported with profiles yet (see [status](#status-and-limitations)).

This fork now lives at [harrytuckerr/RapidRAW-HT-Edition](https://github.com/harrytuckerr/RapidRAW-HT-Edition); previous fork URLs redirect here. The published packages retain the `fuji-preview` channel name.

> **Bring your own profiles.** No commercial profiles, Looks or Adobe software are included. You install files you own.

## Download

> **Security status:** dependency and workflow security fixes on `main` postdate this preview. The APK, DMG and source archive linked below retain their original bytes and are **not rebuilt security releases**. See the [security notes](docs/security/README.md) for validation and historical-metadata limits.

[fuji-preview-2026.10.16.1](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/tag/fuji-preview-2026.10.16.1) is a published development preview built from source commit `b75d096b4ef2001377cfc628835a6bd160601c28`. The embedded app version remains **1.6.5**; the dated tag identifies this fork revision. This README refresh follows publication and is not part of the packaged source. The packages also contain the reviewed attribution resources and Android dependency-declaration overlay supplied with the release; those exact inputs are now retained on `main`.

- **Apple Silicon / M-series Mac, ARM64:** [debug, ad-hoc signed DMG](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/download/fuji-preview-2026.10.16.1/RapidRAW-Fuji-fuji-preview-2026.10.16.1-b75d096b-macos-arm64-debug-adhoc.dmg). The package and mounted contents were verified. It is not Developer ID signed or notarized; Gatekeeper and full macOS app acceptance were not established.
- **Android, ARM64:** [debug APK](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/download/fuji-preview-2026.10.16.1/RapidRAW-Fuji-fuji-preview-2026.10.16.1-b75d096b-android-arm64-debug.apk). Package, certificate and resources were verified. A Fold8 in-place update and normal launch were checked through package and activity metadata; this does not establish UI, photo, profile, provider or export acceptance.
- [SHA256SUMS.txt](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/download/fuji-preview-2026.10.16.1/SHA256SUMS.txt) · [source archive](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/download/fuji-preview-2026.10.16.1/RapidRAW-Fuji-fuji-preview-2026.10.16.1-b75d096b-source.tar.gz) · [all release assets and provenance](https://github.com/harrytuckerr/RapidRAW-HT-Edition/releases/tag/fuji-preview-2026.10.16.1).

The prerelease contains 14 assets. All 14 public downloads returned successfully and matched the reviewed local hashes; the checksum manifest covers the other 13 assets. This preview has no current Intel Mac, Windows or Linux packages. Assets from older previews are separate builds and are not evidence for this source revision. The in-app version checker still points upstream, so use this fork’s release page for these downloads.

Installers published by upstream RapidRAW do not include anything described in the next sections.

## Why use it

- **Start from the rendering you paid for.** Apply your camera-matched base profile and a Look in the same conceptual order as Adobe Camera Raw/Lightroom: profile first, edits after.
- **Keep profiles and presets separate.** Switching presets doesn't silently throw away your chosen profile.
- **No silent substitutions.** If a profile is missing, damaged or meant for a different camera, RapidRAW tells you rather than quietly rendering something else.
- **Dial a Look in or out.** Amount runs from 0 to 200%.
- **Profile workflows for desktop and Android.** They are present in source for both; Android picker and full import behavior remain **Not exercised**.

## The workflow

```text
 RAW file
   │
   ▼
 1. Base profile (DCP) for the camera that took the photo
   │
   ▼
 2. Optional Look (XMP) + Amount 0–200%          ← Profile slot
   │
   ▼
 3. Your edits: presets, exposure & tone, curves, HSL,
    masks, retouching, creative LUTs, effects
   │
   ▼
 4. Saved non-destructively (.rrdata) → export
```

1. **Install your profiles.**
   - **macOS and Windows:** RapidRAW looks in the usual Adobe Camera Raw settings folders.
   - In **Camera Profile**, use **Import base profile (.dcp)** for a camera base and **Import Look (.xmp)** for a compatible profile Look.
   - In **Adjustment presets**, use **Import presets** for Lightroom develop-adjustment XMP (exposure, colour or effects) and supported preset files. An `.xmp` extension alone does not identify a profile Look versus a develop-settings preset. The current mapping is incomplete, so full Lightroom preset compatibility is not promised.
   - **Android:** import is designed around the system file picker; the physical picker and complete Tauri import flow remain **Not exercised**.

   Imported base profiles and Looks are copied into RapidRAW's own profile folder.
2. **Open a RAW** from the camera the base profile was made for.
3. **Pick a base profile** in the **Profile** section at the top of the editing controls.
4. **Optionally pick a Look** and set its **Amount**. 100% is the default, 0% bypasses the Look entirely and leaves the base profile in place, and values above 100% push it further.
5. **Edit as usual.** Presets, masks, tone curves, colour tools, creative LUTs and everything else apply on top of the profile.
6. **Export.** Preview, thumbnails and export all render the profile through the same shared route.

See the [profile guide](docs/profiles.md) for install locations, error messages and details.

### Profiles and presets stay separate

- Applying an ordinary adjustment preset, changing its intensity, or turning it off preserves the current Camera Profile selection. A preset carries a profile only when you explicitly enable **Also capture Camera Profile + Cobalt Look (optional)** while saving it; it is off by default.
- Host-side tests of the native preset serializers confirm that explicit capture choices and their profile base, Look and Amount survive JSON save/reopen and preset-file import/export. Older presets whose capture flag was already absent remain unknown; their original opt-in intent cannot be recovered.
- When you copy settings to a photo from a different camera, the profile is applied only if that photo has a compatible base installed. Otherwise the profile is skipped and the rest of the settings still apply.

### Base profiles and Looks must match

Many Looks are built for a specific base profile. A Cobalt Look might require **Cobalt Modular** for your camera, for example. RapidRAW checks both the required base name *and* the source camera:

- A **Flat** profile does not satisfy a Look that needs **Modular**. Nothing is renamed or aliased to make it fit.
- If the required base isn't installed, the Look stays visible but disabled. The reason is shown, naming the exact base profile and camera it needs.
- **The camera in a Look's title is the camera being emulated, not your camera.** A Look called "Pentax 645D Portrait" can be applied to a Fujifilm RAW, as long as that Fujifilm camera's required base profile is installed. Some Looks also restrict the source camera; those restrictions are checked too.

## What else this fork changes

User-level improvements that are in the source (see [status](#status-and-limitations) for what has been verified):

- **Profile discovery and import** are in source on desktop; Android system-picker import code is present, but the actual OS chooser and full Tauri import flow remain **Not exercised**.
- **One rendering route** for the editor preview, library thumbnails and export when a profile is selected.
- **Saved work keeps its profile choices.** Edits and presets remember the base, Look and Amount. Older saved profile IDs still load, and invalid selections are reported instead of being silently dropped.
- **Caches refresh** when you change profile, Look or Amount. Clearing the profile restores the normal rendering.
- **Clearer errors.**
  - Preview failures reach you instead of disappearing.
  - A slow, superseded load no longer overwrites or raises an error over the photo you're viewing now.
  - An empty file is reported as empty.
  - A file named `.RAF` that actually contains JPEG data is reported as exactly that, instead of the generic "No decoder found".
- **Safer Android picker imports.**
  - Files picked through Android's picker are checked before they're added. Empty files and JPEG data named `.RAF` are rejected with a clear message.
  - A completed file is added without replacing anything already in the destination. If a temporary file can't be cleaned up afterwards, that is reported rather than ignored.
- **No profile selected = upstream rendering.** Leaving the Profile slot empty is intended to render exactly as upstream RapidRAW does.

## Status and limitations

This is an active development fork. Published packages and source checks cover specific layers; they do not close all camera, rendering or provider acceptance gates.

| Area | Where it stands |
|---|---|
| Profile use on Fujifilm images | User-reported profile application is working on X-Pro2 and X-T5 images; platform and test inputs were not specified. Switching profiles has a noticeable reported delay of about 1–3 seconds. This is field feedback, not formal decode, matched-profile rendering, Adobe or fidelity acceptance. |
| Camera Profile / Adjustment Presets UI | The published preview includes separate `.dcp` base, `.xmp` Look and **Import presets** workflows. Source/component checks used mocked native boundaries. A user reports that Android **Import presets** returns from selecting an XMP without importing it or displaying an error; a fix is in progress and is not in this preview. Complete physical import-flow acceptance remains open. |
| Preset profile-capture preservation | Host tests of the actual native preset serializers pass for explicit true/false and legacy-absent capture flags through JSON save/reopen and preset-file import/export, with saved profile fields preserved. Legacy intent cannot be inferred. Actual UI-to-native IPC and physical Android round-trips remain **Not exercised**. |
| macOS package | The M-series ARM64 debug/ad-hoc package, architecture and mounted content were checked. It is not Developer ID signed or notarized. Gatekeeper and full app launch/feature acceptance were **Not exercised**. |
| Native checks | The canonical native run completed **560 passed, 0 failed, 14 ignored** (574 named outcomes). Host native formatting and strict all-target Clippy passed. Whole-project TypeScript still has **70 inherited diagnostics** and targeted ESLint has **49 inherited diagnostics**. Scoped baseline comparisons found no new diagnostics in reviewed changes; the absolute TypeScript and ESLint gates remain failed. |
| Google Drive and authorization | Native authorization/lifecycle groundwork is present, including bounded token-free requests, account-switch consent, launcher cleanup, shutdown coordination and a borrowed verified credential-metadata view. The desktop registration catalog is empty (`NotConfigured`). No configured Google Drive connection or full positive OAuth acceptance exists in this preview. Storage, renewal, galleries, transfers and sync are not working features. |
| Colour match with Adobe Camera Raw | **Not yet verified.** No Adobe-backed comparison has been run, and colour comparison is not implemented yet (the developer tool's `compare` command reports itself as unavailable). |
| Android package and launch | The fresh ARM64 debug APK, certificate and resources passed review. A Fold8 in-place update matched the expected APK hash; appId, first-install time, data directory and version metadata were retained, and normal launch/resumed-activity metadata passed. App-data bytes were not compared. UI, photos, profile/import/export and provider behavior are **Not exercised**. |
| Looks | Supported Look table types render in a bounded, non-HDR way: values are clamped before the Look is applied. Unsupported or "captured" Look variants show an error rather than falling back. |
| GPU | The profile stage runs on the CPU, before RapidRAW's GPU editing pipeline. An optional GPU version is tested for parity separately but is not used for real editing. No performance target is claimed. |
| Preview vs thumbnail vs export | They share one profile route. A full measured comparison across all routes hasn't been done. |
| No-profile rendering | Matches upstream byte-for-byte on one controlled synthetic test, but has not been checked across a broad set of cameras. |
| Linear RAW | Linear RAW files (for example some ProRAW/linear DNGs) are not yet supported with a profile. Selecting a profile uses rawler decoding rather than Apple RAW. |
| Android imports | Host-driver and transport-mock tests cover counted rejection outcomes for image imports. Separately, Android adjustment-preset XMP import is **User-reported broken** after file selection; source investigation found a URI-reader mismatch and missing visible failure feedback. Its fix is not in this preview. Other provider/UI import paths remain unverified. |
| X-Pro2 / X-T5 feedback | The earlier `.RAF` report involved JPEG data with a `.RAF` filename. Latest user feedback reports profile application working on X-Pro2 and X-T5 images; platform was not specified. Formal RAW decoding and rendering gates remain open. |
| Windows / Linux | No packages for this source are included in the current preview, and fork-specific profile workflows have not been acceptance-tested there. |

The full evidence lives in the [profile guide](docs/profiles.md), the [build guide](docs/BUILD.md) and the [team ledger](docs/integration/TEAM-LEDGER-2026-10-15.md).

## Upcoming work

The items below are requested or proposed, not completed features in source revision `b75d096b`. The current authorization/lifecycle code is groundwork, not a configured provider connection. Source, component or build checks are not device, provider or rendered-output acceptance. Some ideas build on tools RapidRAW already has, such as the collage maker, JPEG editing and export.

- **Android adjustment-preset import:** read system-picker content URIs correctly and show success, partial-import and failure feedback.
- **Compact Camera Profile import menu:** replace the two large import action cards with one dropdown, keeping base DCP and Look XMP imports separate.
- **White-balance menu:** As Shot, Auto and Kelvin-labelled daylight, shade, cloudy, tungsten, fluorescent and flash presets.
- **Faster profile comparisons:** investigate repeated RAW development and profile processing behind the reported 1–3 second switching delay; no measured improvement is claimed yet.
- **Auto leveling / straighten** and **auto upright / geometry:** requested future correction tools, with design and acceptance still to be defined.
- **Camera-matched profile defaults:** use an installed Modular base matched to the source camera by default, then offer fixed CCD/Fujifilm variants before generic Cobalt choices regardless of import order. This is a new request from user feedback and is not implemented in this preview.
- **Google Drive workspace:** plan account setup and sync of settings, saved edits, named checkpoints, adjustment presets and user-imported profile libraries between desktop and Android. There is no configured Google Drive connection, storage, renewal, gallery, transfer or sync feature in this preview. Provider behavior and distribution-policy fit are unverified. Photos are intended to be fetched on demand; this is not automatic photo backup.
- **Save copy & Share** for single photos and 2–4-image collages, with Android's share sheet.
- **Android JPEG "Lite" mode:** quick edit or collage from a JPEG without setting up a library.
- **Fujifilm shooting settings** shown alongside a photo (film simulation, white balance, dynamic range where reliably available), and side-by-side comparison with the camera's own JPEG.
- **Named checkpoints** with saved edit history.
- **Saved searches** in the library.
- **Point Colour eyedropper:** a planned sampled hue/saturation/luminance (H/S/L) adjustment; this control set is a working assumption and its behavior is not finalized.
- **Lens correction improvements:** build on the existing Lensfun Auto correction using camera and lens metadata; no new lens-correction engine is proposed.
- **AI features:** a separate AI feature brief is proposed planning only; no new AI implementation is claimed for this preview.

The current build contains native authorization and lifecycle groundwork, including bounded token-free requests, exact account-switch consent, launcher cleanup, shutdown coordination and a borrowed verified credential-metadata view. The desktop registration catalog is empty (`NotConfigured`), and the groundwork is not connected to a configured Google Drive provider. Positive OAuth, secure provider storage, renewal, galleries, transfers and cross-device sync remain unavailable. See the [Drive contract](docs/drive/contract.md) for planned integration and acceptance gates.

Full Lightroom preset compatibility and exact reproduction of Fujifilm film simulations or recipes are not promised. Full automatic photo backup is not planned.

## Build from source

You'll need [Node.js](https://nodejs.org/) 22 and Rust. The Rust toolchain is pinned in `src-tauri/rust-toolchain.toml`. You'll also need the usual [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system.

The **`main`** branch contains the current HT Edition integration, this later README refresh and the exact attribution resources/Android lock declaration used for the published packages. The packages were built from commit `b75d096b4ef2001377cfc628835a6bd160601c28` with the release’s recorded packaging overlays; this later documentation commit was not rebuilt or retagged.

```bash
git clone https://github.com/harrytuckerr/RapidRAW-HT-Edition.git
cd RapidRAW-HT-Edition
git checkout main

npm ci
npm start              # development build
npm run tauri build    # release build
```

The [build guide](docs/BUILD.md) has the checks to run, the pinned **Android** build (isolated workspace and reviewed dependency locks), and how to render a synthetic profile test. For camera tethering, see [Camera tethering](#camera-tethering).

## Licence, profiles and credit

- This fork is licensed under the **[GNU AGPL-3.0](LICENSE)**, like upstream. See [NOTICE.md](NOTICE.md) for upstream and third-party notices.
- **No commercial camera profiles, Looks or Adobe SDK binaries are distributed.** Install profiles you have lawfully obtained, and keep commercial profile files and private photos out of version control.
- Built on **[RapidRAW](https://github.com/CyberTimon/RapidRAW)**, created by **[Timon Käch (CyberTimon)](https://github.com/CyberTimon)**, and its contributors. Please consider [supporting upstream development](https://ko-fi.com/cybertimon).

---

# From upstream RapidRAW

The sections below describe RapidRAW itself, adapted from the [upstream README](https://github.com/CyberTimon/RapidRAW#readme). They apply to this fork too. For upstream's changelog and history, see the [upstream repository](https://github.com/CyberTimon/RapidRAW) and the [RapidRAW docs](https://www.getrapidraw.com/docs/).

> A beautiful, non-destructive, and GPU-accelerated RAW image editor built with performance in mind.

RapidRAW is a modern, lightweight alternative to Adobe Lightroom® for Windows, macOS, Linux and Android. Its focus is a clean, fast editing workflow. It is still in active development and not yet as polished as mature tools like darktable, RawTherapee or Lightroom®.

<table width="100%">
  <tr>
    <td width="50%" valign="top">
      <img src="https://raw.githubusercontent.com/CyberTimon/RapidRAW/assets/.github/assets/library_view.jpg" alt="RapidRAW library view (upstream screenshot)" width="100%">
    </td>
    <td width="50%" valign="top">
      <img src="https://raw.githubusercontent.com/CyberTimon/RapidRAW/assets/.github/assets/editor_view.jpg" alt="RapidRAW editor view (upstream screenshot)" width="100%">
    </td>
  </tr>
</table>

## Key features

**Core editing engine**
- **GPU-accelerated:** a full 32-bit processing pipeline written in WGSL.
- **Layered masking:** AI subject, depth, sky and foreground detection, plus parametric colour and luminance masks.
- **Retouching:** local clone and heal tools.
- **Generative edits:** remove or add elements with text prompts, through an optional AI backend.
- **RAW support** through rawler, plus JPEG and other standard formats.
- **Non-destructive:** edits are stored in `.rrdata` sidecar files; originals are untouched.
- **Lens correction:** distortion, TCA and vignette correction via Lensfun.

**Adjustments**
- **Tone:** exposure, tone mapping (including AgX), contrast, highlights, shadows, whites and blacks.
- **Curves:** luma/RGB and parametric tone curves.
- **Colour:** temperature/tint, vibrance, saturation, colour wheels and HSL mixer.
- **Detail:** sharpening, clarity, structure and noise reduction.
- **Effects:** lens blur, LUTs, dehaze, vignette, glow, halation, flares and film grain.
- **Transform:** perspective, rotation, straightening, crop and warping.

**Library and workflow**
- Library with recursive folders, virtual copies, ratings, colour labels and tags.
- Culling view, filmstrip, and file operations (import, copy, move, rename, duplicate).
- Batch adjustments and export.
- Presets with intensity control.
- Copy/paste settings.
- Undo/redo history.
- Scopes: vectorscope, waveform, RGB parade and histogram.
- HDR merge, panorama stitching, collage maker and film negative converter.
- Export to JPEG, PNG, WebP, AVIF, TIFF, JXL or LUT formats, with watermarking and EXIF retention.
- EXIF viewer and a headless [CLI](#command-line-interface-cli) exporter.
- [Camera tethering](#camera-tethering) on macOS and Linux.

**AI options** (all optional)
- Built-in local tools for masking, automatic tagging and simple generative replace.
- Self-hosted generative editing through [ComfyUI](https://github.com/comfyanonymous/ComfyUI) and the [RapidRAW-AI-Connector](https://github.com/CyberTimon/RapidRAW-AI-Connector).
- An **AI-free mode** (Settings → General) that hides AI tools and uses algorithmic fallbacks.
- Upstream also describes an optional cloud service. This fork makes no claims about it.

## Supported formats, lenses and languages

<details>
<summary><strong>Camera RAW formats</strong></summary>

RAW decoding uses [dnglab / rawler](https://github.com/dnglab/dnglab); see its [supported cameras list](https://github.com/dnglab/dnglab/blob/main/SUPPORTED_CAMERAS.md).

| Brand / Manufacturer  | Extensions                     |
| :-------------------- | :----------------------------- |
| Adobe                 | `.dng`                         |
| Apple                 | `.pro`                         |
| ARRI                  | `.ari`                         |
| Canon                 | `.crw`, `.cr2`, `.cr3`         |
| Casio                 | `.bay`                         |
| Contax                | `.raw`                         |
| Epson                 | `.erf`                         |
| Fujifilm              | `.raf` (X-Trans & Bayer)       |
| Hasselblad            | `.3fr`, `.fff`                 |
| Kodak                 | `.kdc`, `.k25`, `.dcs`, `.dcr` |
| Leaf / Imacon         | `.mos`, `.iiq`                 |
| Leica                 | `.rwl`                         |
| Mamiya                | `.mef`                         |
| Minolta               | `.mrw`                         |
| Nikon                 | `.nef`, `.nrw`                 |
| Olympus               | `.orf`                         |
| Panasonic / Leica     | `.rw2`, `.raw`                 |
| Pentax / Ricoh        | `.pef`, `.ptx`                 |
| Phase One             | `.iiq`                         |
| Samsung               | `.srw`                         |
| Sigma                 | `.x3f`                         |
| Sony                  | `.arw`, `.srf`, `.sr2`         |

If your camera's RAW files aren't supported, please request support from [dnglab](https://github.com/dnglab/dnglab/issues).

</details>

<details>
<summary><strong>Non-RAW formats</strong></summary>

- **Standard:** `.jpg`, `.jpeg`, `.png`, `.webp`, `.jxl`, `.tiff`, `.tif`, `.bmp`, `.gif`
- **HDR / wide gamut:** `.exr`, `.hdr`
- **Graphics:** `.tga`, `.ico`, `.dds`
- **Specialist:** `.qoi`, `.ff`
- **Netpbm:** `.pnm`, `.pbm`, `.pgm`, `.ppm`, `.pam`

</details>

<details>
<summary><strong>Lens correction</strong></summary>

Automatic lens detection plus distortion, TCA and vignetting correction come from [Lensfun](https://lensfun.github.io/). Missing lens profiles can be requested on the [Lensfun repository](https://github.com/lensfun/lensfun/issues).

</details>

<details>
<summary><strong>Languages</strong></summary>

English, Català, Čeština, Deutsch, Español, Français, Italiano, Nederlands, Polski, Português, Русский, 日本語, 한국어, 简体中文 and 繁體中文.

</details>

## Camera tethering

RapidRAW can control a USB-connected camera on **macOS and Linux**. Features include live view, exposure controls, autofocus, ghost overlay, battery level, automatic presets and instant ingestion. It uses [libgphoto2](http://gphoto.org/), which supports [thousands of cameras](http://gphoto.org/proj/libgphoto2/support.php). Set your camera's USB mode to **PC Remote**, **Tether Shooting** or **PTP**. Windows and Android are not supported.

Tethering is a separate build because it links against `libgphoto2`. No Fujifilm Edition tethering builds are published. Build it from source:

```bash
# macOS
brew install libgphoto2 pkg-config
# Debian / Ubuntu
sudo apt-get install -y libgphoto2-dev pkg-config
# Arch:   sudo pacman -S libgphoto2 pkgconf
# Fedora: sudo dnf install libgphoto2-devel pkgconf-pkg-config

npm run start:tethering                        # development
npm run tauri build -- --features tethering    # release
```

## Command line interface (CLI)

A headless exporter applies the edits in `.rrdata` sidecars, or a JSON adjustments file, without opening the GUI:

```bash
rapidraw export /path/to/photos --output /path/to/output_dir --format jpeg --quality 90
rapidraw export /path/to/photo.raw --output /path/to/output.png --format png
rapidraw export /path/to/photo.raw --output /path/to/output.tiff --format tiff --tiff-bit-depth 16
rapidraw export /path/to/photos --output /path/to/output_dir --adjustments /path/to/preset.json
```

| Option                 | Description                                                          | Default           |
| :--------------------- | :------------------------------------------------------------------- | :---------------- |
| `<source>`             | Image file or directory                                              | _(Required)_      |
| `--output <path>`      | Target directory or output file                                      | _(Required)_      |
| `--format <fmt>`       | `jpeg`, `png`, `webp`, `avif`, `tiff`, `jxl`, `cube`                 | `jpeg`            |
| `--quality <1-100>`    | Export quality                                                       | `90`              |
| `--tiff-bit-depth <n>` | TIFF channel depth (`8` or `16`)                                     | `16`              |
| `--keep-metadata`      | Retain EXIF/capture metadata                                         | `false`           |
| `--adjustments <path>` | JSON adjustments file that overrides sidecars                        | _(Auto-detected)_ |

## Companion apps

- **[RapidTimelapse](https://github.com/crispin81/RapidTimelapse)** by [@crispin81](https://github.com/crispin81): keyframe ramping and deflicker for time-lapses, written into `.rrdata` sidecars.
- **[RapidReady](https://github.com/on370/rapidready)** by [@on370](https://github.com/on370): ingest, culling and DAM tool with rating and metadata sync to RapidRAW.

## System requirements

- **OS:** Windows 10+, macOS 13 (Ventura)+, or Ubuntu 22.04+ / a comparable modern Linux.
- **RAM:** 16 GB or more is recommended for high-resolution RAWs and complex masks.
- **GPU:** a dedicated GPU is recommended. Very old GPUs (roughly pre-2015) or older integrated graphics may struggle.

<details>
<summary><strong>App crashes when opening an image or entering edit mode</strong></summary>

This is often caused by automatic GPU backend selection. Open **Settings** on the home screen, go to **Processing → Processing Backend**, choose a specific backend (Vulkan, DirectX12, OpenGL or Metal), and restart.

</details>

<details>
<summary><strong>Linux Wayland/WebKit crash</strong></summary>

On Wayland (e.g. GNOME with NVIDIA), try launching with `WEBKIT_DISABLE_DMABUF_RENDERER=1 RapidRAW` or `WEBKIT_DISABLE_COMPOSITING_MODE=1 RapidRAW`. Switching to X11 or an AMD/Intel GPU may also help. See [upstream #306](https://github.com/CyberTimon/RapidRAW/issues/306).

</details>

## Contributing

- **Profile and Look behaviour, Android imports, or anything else specific to this fork:** please open an issue on [this repository](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/issues).
- **General RapidRAW behaviour:** check [upstream RapidRAW](https://github.com/CyberTimon/RapidRAW/issues) too.
- **Unsupported RAW files:** report them to [dnglab/rawler](https://github.com/dnglab/dnglab/issues).

## Special thanks (from upstream)

[rawler](https://github.com/dnglab/dnglab/tree/main/rawler) · [Lensfun](https://lensfun.github.io/) · [LaMa](https://github.com/advimman/lama) · [SAM 2](https://github.com/facebookresearch/sam2) · [U-2-Net](https://github.com/xuebinqin/U-2-Net) · [Depth Anything V2](https://github.com/DepthAnything/Depth-Anything-V2) · [nind-denoise](https://github.com/trougnouf/nind-denoise) · [NegPy](https://github.com/marcinz606/NegPy) · [pixls.us](https://discuss.pixls.us/) · [darktable](https://github.com/darktable-org/darktable) · [libgphoto2](http://gphoto.org/) · [spektrafilm](https://github.com/andreavolpato/spektrafilm) · [Google AI Studio](https://aistudio.google.com)

## License

RapidRAW and this fork are licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**, which keeps RapidRAW and its derivatives open source. See [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).
