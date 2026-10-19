# NOTICE

This is a fork of [RapidRAW](https://github.com/CyberTimon/RapidRAW), (c) Timon Kach, licensed under AGPL-3.0.

## What this fork adds

An Adobe DNG Camera Profile (DCP) pipeline and Cobalt Image profile support, enabling camera-profile-based rendering inside RapidRAW on desktop and Android. The feature adds a first-class "profile slot" to the editing pipeline (the concept Lightroom/ACR exposes as `Adobe Color` / `Camera Standard` / a Cobalt profile), holding a base DCP plus an optional Cobalt Look and an Amount, applied before all existing adjustments.

## Specifications and trademarks

- **DNG / DCP** is an Adobe specification. Adobe is a trademark of Adobe Inc. No Adobe code or assets are redistributed in this repository.
- **Cobalt Image** is a trademark of its owner. No Cobalt-Image code, profiles, look tables, or other assets are redistributed in this repository, in source, in tests, in fixtures, or in built binaries. Users install their own lawfully-purchased profile files.
- This fork implements **read-only interoperability** with profile files the user already owns. It does not author, convert, export, or redistribute profiles, and it does not circumvent any licence check. A profile's `ProfileEmbedPolicy` (e.g. `EmbedNever`) is honoured on export.

## Licensing

This fork remains licensed under AGPL-3.0, identical to upstream. See `LICENSE`. AGPL obligations apply to any network-served deployment.

## Current implementation and verification

- **Native Looks:** Route A decoding is implemented for supported table variants. Unsupported and captured-table variants return an error. Nonzero Looks use a bounded non-HDR path that clamps finite pre-Look ProPhoto values to `[0,1]`; Adobe and HDR behavior are not established.
- **Production rendering:** selected profiles use the shared CPU path. Optional GPU shader parity is separate from production profile processing. A common helper serves preview, thumbnail and export routes, but measured cross-route comparisons are incomplete.
- **No-profile behavior:** one controlled synthetic desktop export was byte-identical to upstream. This does not establish byte identity across cameras, platforms or backends.
- **Validation:** CIEDE2000 matches all 34 Sharma known-answer pairs. Current host checks passed: 208 library tests, 2 CLI tests and 5 explicit RAW/import controls, plus formatting and strict all-target lint. The `validate_dcp compare` command is explicitly unavailable; Adobe reference comparison has not run.
- **Android:** an Android debug build compiles. Physical profile import/use, persistence and export have not been exercised on the Fold.
- **Adobe reference:** Adobe Lightroom 9.6 is installed. Only preflight inspection has occurred; no Adobe reference export or comparison has been performed.

Approved E1–E5 photography workflows and the limited Google Drive workspace are planned, not implemented.

## Fuji X-Trans demosaicing

The CPU X-Trans implementation uses Frank Markesteijn’s method, ported from LibRaw under LGPL-2.1-only. The [original copyright and derivation notice](src-tauri/resources/licenses/fuji-preview/LIBRAW-XTRANS-NOTICE.txt) and [LGPL-2.1 text](src-tauri/resources/licenses/LGPL-2.1.txt) are retained. The README comparison was generated through this application’s RAW pipeline, rather than using LibRaw-rendered reference images.

## Upstream attribution

Upstream RapidRAW is (c) Timon Kach, AGPL-3.0: https://github.com/CyberTimon/RapidRAW
