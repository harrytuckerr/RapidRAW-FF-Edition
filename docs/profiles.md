# Camera profiles in RapidRAW

Camera profiles define how camera-native RAW data is rendered before ordinary edits. In the current selected-profile path, RapidRAW sends un-white-balanced camera RGB through the CPU DCP renderer, applies an optional native RGB Look in linear ProPhoto, converts once to linear sRGB, then runs ordinary edits, masks, creative LUTs and tone mapping. A native profile Look is part of the Profile slot; it is separate from a preset and from the later creative LUT control.

A missing, malformed or camera-incompatible selected profile is reported as an error. RapidRAW does not silently substitute another base profile. See the [current checkpoint notes](releases/1.6.27.md) for validation scope and known limitations.

## Base profiles and Looks

A DCP can render on its own as the base profile. Some XMP Looks require a particular base DCP. The Look declares that requirement in its `CameraProfile` metadata. RapidRAW checks the source camera model and required base-profile name together; a similarly named profile for another camera does not satisfy the requirement.

For example, the supplied Cobalt Flat DCP does not satisfy a Look that requires Cobalt Modular.

When a compatible base is available, the base DCP is applied first and the native RGB Look follows it in linear ProPhoto. Both are selected through the Profile slot. If the required base is missing, the browser keeps the Look visible but disabled and explains the exact required base and camera—for example: “`<Look name>` requires the base profile `<CameraProfile>` for `<camera model>`, which is not installed.” This reason is distinct from the render error shown if a selected profile later becomes missing, malformed or incompatible; RapidRAW does not silently substitute another profile. The physical Fold profile picker, disabled state and message have not been exercised.

## Installing profiles

### Desktop

RapidRAW can discover profiles in Adobe Camera Raw settings directories on macOS and Windows:

- **macOS:** `~/Library/Application Support/Adobe/CameraRaw/Settings/` (recursive)
- **Windows:** `%PROGRAMDATA%\Adobe\CameraRaw\Settings\` and `%APPDATA%\Adobe\CameraRaw\Settings\` (recursive)
- **Linux:** use the profile importer.

To install profiles manually, use **Import** in the profile browser and select `.dcp` base profiles or `.xmp` Looks. Imported files are copied to RapidRAW's managed profile directory. A rescan refreshes discovery after files are added there manually.

### Android

Import profiles through Android's system file picker (SAF). RapidRAW copies selected `.dcp` or `.xmp` files into its managed profile directory; desktop Adobe-directory discovery does not apply on Android.

The Android profile implementation is merged. Device checks are build-specific; see the [current checkpoint notes](releases/1.6.27.md). A historical API 33 SwiftShader emulator preview hang does not establish a physical Fold result.

A previous picker failure involved JPEG/JFIF content under a `.RAF` filename. Its relation to the original selection or provider remained unknown; it did not establish a native X-T5 decoder regression. Current opening and recovery limitations are listed in the [checkpoint notes](releases/1.6.27.md).

## Amount

For Looks that support adjustable Amount, the control ranges from 0 to 200 percent and defaults to 100. Amount changes the Look layer; the base DCP remains applied. At 0, the Look is bypassed exactly and the base rendering remains. The current nonzero Look path clamps finite pre-Look ProPhoto values to `[0,1]`. This is a bounded non-HDR adaptation. Adobe fidelity and scene-referred/HDR behaviour remain unaccepted.

The exact Amount rendering has not been validated against Adobe Camera Raw. This guide does not claim that Amount follows Adobe's internal calculation.

## Unsupported Looks and capture

Native RGB Looks are supported only when their table variant is decoded by the current implementation. Unsupported and captured-table variants return an error; they do not fall back to a different profile. Capture-related code or UI scaffolding is not a supported capture workflow. There are no user steps here for exporting or importing a captured LUT.

Commercial-profile render acceptance and Adobe comparison remain open. The current `validate_dcp` CLI does not provide Adobe reference comparison; a successful synthetic render is not evidence of Adobe parity. See [DCP pipeline status](dcp-pipeline.md) and the [current checkpoint notes](releases/1.6.27.md).

## No-profile behaviour

The intended invariant is that leaving the Profile slot empty preserves the upstream rendering. One controlled synthetic desktop TIFF comparison was byte-identical. A broad camera, platform and backend corpus has not been validated, so this result does not establish universal output or sidecar identity.

## Presets and profiles

Presets and profiles are independent. Applying a preset leaves the selected profile unchanged unless the preset was explicitly saved with a profile through the opt-in control, which is off by default.

When settings are copied to an image with a different camera, RapidRAW applies the profile only when the destination has a compatible required base. Otherwise the profile is skipped and the other settings can still be applied. Physical-device UX for this case has not been exercised.

## Evidence limits

- Adobe Camera Raw colour-match acceptance remains open; the four original colour questions still require Adobe-backed comparison.
- Desktop synthetic checks do not establish commercial-profile, physical-device or Adobe acceptance.
- On the physical Fold, app launch, visible UI and basic adjustments are Pass — user-reported. Physical profile import, preview, save/reopen and export are Not exercised.

The [DCP specification](../COBALT_DCP_SPEC.md) describes the original profile and pairing goals. The [pipeline guide](dcp-pipeline.md) describes implementation behavior; [current checkpoint notes](releases/1.6.27.md) distinguish validation results and limitations.
