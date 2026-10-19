<div align="center">

<img src="src-tauri/icons/full_res_original.png" alt="RapidRAW app icon" width="88">

# RapidRAW · FujiFilm Edition

**Open a RAF. Find your colours. Make it yours.**

Camera profiles, creative colour tools and a growing Android workflow for Fujifilm photographers.

[**Download Android v1.6.19**](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases/download/v1.6.19/RapidRAW-Fujifilm-Edition-1.6.19-android-arm64-debug.apk) · [**Releases**](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases) · [**Start editing**](#start-editing-on-android) · [**Camera profiles**](#bring-your-own-camera-profiles) · [**Current status**](#where-things-stand) · [**Build & contribute**](#build--contribute)

[![Android arm64 community builds](https://img.shields.io/badge/Android-arm64_community_builds-3c7657?style=flat-square)](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases)
[![AGPL-3.0 license](https://img.shields.io/badge/License-AGPL--3.0-476584?style=flat-square)](LICENSE)

</div>

<br>

![Upstream RapidRAW desktop editor showing its photo-editing workspace](https://raw.githubusercontent.com/CyberTimon/RapidRAW/assets/.github/assets/editor.jpg)

_Upstream RapidRAW desktop screenshot, credited to [Timon Käch / RapidRAW](https://github.com/CyberTimon/RapidRAW). It does not show the current Android/Fold build._

## A few ways to make a photo yours

Tools in this edition, subject to image, asset and calibration support. Check the release notes for the APK you install:

|     | Tool                       | What you can do                                                                                                  |
| :-: | -------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 🌿  | **Fujifilm RAF / X-Trans** | Develop supported X-Trans RAWs with the CPU Markesteijn path and false-colour filtering.                         |
| 🎨  | **Point Colour**           | Pick one colour range, then adjust its Hue, Saturation and Luminance.                                            |
| 📐  | **Auto straighten**        | Estimate a small rotation from the completed preview, with normal undo. Ambiguous scenes can return no change.   |
| ✨  | **Denoise**                | Compare a noise-reduction preview before saving the result.                                                      |
| 🔎  | **Lens correction**        | Try automatic lens matching, or choose a profile where calibration is available.                                 |
| 📷  | **Camera-aware defaults**  | Start eligible fresh RAWs with a matching Cobalt Modular base while respecting saved and manual profile choices. |

Profiles set the starting colour. Your exposure, curves, masks, presets and creative adjustments build on it.

## Start editing on Android

1. **Get v1.6.19.** [Download the Android ARM64 APK](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases/download/v1.6.19/RapidRAW-Fujifilm-Edition-1.6.19-android-arm64-debug.apk). This is a **debug-signed working community build**; read its [release notes](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases/tag/v1.6.19) before installing. GitHub marks this Android community build as a prerelease.
2. **Install it.** Open the APK and allow installation from that app if Android asks. Use the offered update for an existing installation.
3. **Open a photo.** Choose an image or folder through Android’s file picker. Wait for the full preview before sampling the photo.
4. **Make your edit.** Explore the colour, crop and lens tools. Undo and redo let you revisit your choices.
5. **Export to share.** Edit settings autosave as `.rrdata` data; your original RAW stays untouched. Export creates the finished image.

## Bring your own camera profiles

In the profile browser, use **Import** to add your camera’s `.dcp` base and optional `.xmp` Looks through Android’s file picker. Choose a base, then a compatible Look. Unsupported pairings stay disabled.

**Cobalt profiles and Looks are proprietary and are not included.** Import files you lawfully own; a Look needs the correct base for your camera. Profiles are distinct from presets and creative LUTs.

Eligible fresh, unsaved RAWs can start with a unique matching **Cobalt Modular** base when a supported compatible Look is installed. Only the base is selected. Saved edits and explicit **No Profile** choices take priority.

See the [profile guide](docs/profiles.md) for pairing and controls; its platform-status notes are historical.

## Where things stand

**Status: 19 October 2026**

This is an actively developed fork. Use the latest [release notes](https://github.com/harrytuckerr/RapidRAW-FF-Edition/releases) for the exact build contents and device checks.

- **v1.6.19** is a working **Android ARM64 community build**, debug-signed and tested on a **Samsung Fold**. It is published as a prerelease while validation remains Fold-scoped.
- **User-reported on v1.6.19:** editing and repeated zooming now feel responsive again, with the other reported lag seemingly gone. Initial DCP selection and switching profiles still have noticeable latency and remain a [lower-priority follow-up](https://github.com/harrytuckerr/RapidRAW-FF-Edition/issues/17).
- The source includes upstream RapidRAW changes through `6e2631ad`, alongside the Fuji RAW path, profiles, Point Colour and guarded preview editing. Each release records its own build, test and device checks.

Recent device validation is **Android-focused**. Desktop features are inherited from upstream; this fork has not received equivalent current desktop validation.

Auto straighten is a helpful starting point, not a guarantee of a level horizon. You can keep adjusting manually or undo the result; ambiguous scenes may produce no change.

**GPU demosaicing/GPU1 is not accepted.** Capture One performance/quality parity and Adobe Camera Raw parity are unverified; matching their rendering or speed is not promised.

On the later list: [give the white-balance template dropdown the same styling as the editor](https://github.com/harrytuckerr/RapidRAW-FF-Edition/issues/18).

Found something unexpected? [Open an issue](https://github.com/harrytuckerr/RapidRAW-FF-Edition/issues) with the build version, camera, device and steps that reproduce it.

## Build & contribute

- [Build instructions](docs/BUILD.md) — desktop and isolated Android workflows.
- [Known CI baseline](https://github.com/harrytuckerr/RapidRAW-FF-Edition/issues/1) — inherited checks remain unresolved; release notes record the scoped local validation.
- [Profile pipeline](docs/dcp-pipeline.md) — rendering design and known limitations.
- [Documentation](docs/) — background guides; check dated status notes against this README.
- [Notices and attribution](NOTICE.md) · [Source license](LICENSE).

## Built on RapidRAW

Built on **[RapidRAW](https://github.com/CyberTimon/RapidRAW) by [Timon Käch](https://github.com/CyberTimon)** and its contributors. Thank you for making this foundation available.

The application remains **AGPL-3.0**. X-Trans code retains its [LGPL-2.1-only license](src-tauri/resources/licenses/LGPL-2.1.txt) and attribution to Frank Markesteijn’s method in LibRaw. See [NOTICE.md](NOTICE.md) for attribution and profile-use terms.

**FujiFilm Edition is an independent project, not an official Fujifilm product.**
