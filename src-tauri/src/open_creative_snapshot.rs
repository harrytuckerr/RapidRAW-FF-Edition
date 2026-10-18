//! Inactive immutable assets for a later managed snapshot phase.
//! No source/appearance/AsShot authority, worker, display or cache is created.
//! Blocking existing parsers are checked before/after, not claimed preemptible.
use crate::adjustment_utils::{SnapshotHydrationError, hydrate_snapshot_adjustments};
use crate::app_settings::AppSettings;
use crate::effect_maps::{CapturedMapError, capture_luma_map, capture_rgba_map};
use crate::lut_processing::Lut;
use crate::mask_generation::MaskDefinition;
use crate::raw_processing::RawCancel;
use image::{GrayImage, RgbaImage};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};

#[derive(Debug)]
pub(crate) enum CreativeCaptureError {
    Cancelled,
    Hydration(SnapshotHydrationError),
    InvalidVisibleMask(String),
    MissingEffectMap(&'static str),
    InvalidEffectMap {
        field: &'static str,
        source: CapturedMapError,
    },
    EffectMapTooSmall(&'static str),
    Lut(String),
}
impl CreativeCaptureError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Hydration(error) => match error {
                SnapshotHydrationError::Invalid(_) => "hydration_invalid",
                SnapshotHydrationError::MissingPayload { .. } => "hydration_missing_payload",
                SnapshotHydrationError::ConflictingPayload { .. } => {
                    "hydration_conflicting_payload"
                }
                SnapshotHydrationError::ConflictingAliases { .. } => {
                    "hydration_conflicting_aliases"
                }
                SnapshotHydrationError::UnknownVisibleMask(_) => "hydration_unknown_visible_mask",
                SnapshotHydrationError::TooManyMasks(_) => "hydration_too_many_masks",
            },
            Self::InvalidVisibleMask(_) => "invalid_visible_mask",
            Self::MissingEffectMap(_) => "missing_effect_map",
            Self::InvalidEffectMap { .. } => "invalid_effect_map",
            Self::EffectMapTooSmall(_) => "effect_map_too_small",
            Self::Lut(_) => "lut_error",
        }
    }
}
impl std::fmt::Display for CreativeCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Creative capture cancelled"),
            Self::Hydration(e) => write!(f, "{e}"),
            Self::InvalidVisibleMask(e) => write!(f, "Visible mask: {e}"),
            Self::MissingEffectMap(field) => write!(f, "Missing requested effect map: {field}"),
            Self::InvalidEffectMap { field, source } => write!(f, "{field}: {source}"),
            Self::EffectMapTooSmall(field) => {
                write!(f, "Requested effect map is smaller than2x2: {field}")
            }
            Self::Lut(e) => write!(f, "Creative LUT: {e}"),
        }
    }
}
impl std::error::Error for CreativeCaptureError {}

/// Private immutable ownership. Getters do not expose mutable vectors/JSON.
/// The retained JSON owns all resolved patch/submask inline bytes. Maps are
/// decoded/warped now, LUT parsed now; subsequent users must not reopen paths.
pub(crate) struct CapturedCreativeAssets {
    adjustments: Value,
    settings: AppSettings,
    masks: Vec<MaskDefinition>,
    lut: Option<Arc<Lut>>,
    relight_map: Option<Arc<RgbaImage>>,
    fog_map: Option<Arc<GrayImage>>,
    lens_blur_map: Option<Arc<GrayImage>>,
}
impl CapturedCreativeAssets {
    pub(crate) fn adjustments(&self) -> &Value {
        &self.adjustments
    }
    pub(crate) fn settings(&self) -> &AppSettings {
        &self.settings
    }
    pub(crate) fn masks(&self) -> &[MaskDefinition] {
        &self.masks
    }
    pub(crate) fn lut(&self) -> Option<&Arc<Lut>> {
        self.lut.as_ref()
    }
    pub(crate) fn relight_map(&self) -> Option<&Arc<RgbaImage>> {
        self.relight_map.as_ref()
    }
    pub(crate) fn fog_map(&self) -> Option<&Arc<GrayImage>> {
        self.fog_map.as_ref()
    }
    pub(crate) fn lens_blur_map(&self) -> Option<&Arc<GrayImage>> {
        self.lens_blur_map.as_ref()
    }
}
fn check_cancel(cancel: &RawCancel) -> Result<(), CreativeCaptureError> {
    if let Some((tracker, generation)) = cancel {
        if tracker.load(Ordering::SeqCst) != *generation {
            return Err(CreativeCaptureError::Cancelled);
        }
    }
    Ok(())
}
fn requested_map<'a>(
    value: &'a Value,
    field: &'static str,
) -> Result<&'a str, CreativeCaptureError> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(CreativeCaptureError::MissingEffectMap(field))
}
fn capture_depth(
    value: &Value,
    field: &'static str,
    cancel: &RawCancel,
) -> Result<Arc<GrayImage>, CreativeCaptureError> {
    check_cancel(cancel)?;
    let captured = capture_luma_map(requested_map(value, field)?, value);
    check_cancel(cancel)?;
    let map =
        captured.map_err(|source| CreativeCaptureError::InvalidEffectMap { field, source })?;
    if map.width() < 2 || map.height() < 2 {
        return Err(CreativeCaptureError::EffectMapTooSmall(field));
    }
    Ok(map)
}

pub(crate) fn capture_creative_assets(
    adjustments: &Value,
    settings: &AppSettings,
    cancel: RawCancel,
) -> Result<CapturedCreativeAssets, CreativeCaptureError> {
    check_cancel(&cancel)?;
    let settings = settings.clone();
    let hydrated = hydrate_snapshot_adjustments(adjustments);
    check_cancel(&cancel)?;
    let adjustments = hydrated.map_err(CreativeCaptureError::Hydration)?;
    let mut masks = Vec::new();
    if let Some(values) = adjustments.get("masks").and_then(Value::as_array) {
        for value in values {
            if value["visible"].as_bool() != Some(true) {
                continue;
            }
            let mask: MaskDefinition = serde_json::from_value(value.clone())
                .map_err(|e| CreativeCaptureError::InvalidVisibleMask(e.to_string()))?;
            if !mask.sub_masks.is_empty() {
                masks.push(mask);
            }
        }
    }
    let effects_visible = adjustments["sectionVisibility"]["effects"]
        .as_bool()
        .unwrap_or(true);
    // Matches existing has_lut: effects visible AND lutPath.is_string().
    // Empty strings are requests too and retain the real parser's failure.
    let lut = if effects_visible && adjustments["lutPath"].is_string() {
        check_cancel(&cancel)?;
        let parsed =
            crate::lut_processing::parse_lut_file(adjustments["lutPath"].as_str().unwrap());
        check_cancel(&cancel)?;
        let lut = parsed.map_err(|e| CreativeCaptureError::Lut(e.to_string()))?;
        let expected = (lut.size as usize)
            .checked_mul(lut.size as usize)
            .and_then(|n| n.checked_mul(lut.size as usize))
            .and_then(|n| n.checked_mul(3));
        if lut.size == 0
            || expected != Some(lut.data.len())
            || !lut.data.iter().all(|v| v.is_finite())
        {
            return Err(CreativeCaptureError::Lut(
                "Invalid parsed LUT shape/finite samples".into(),
            ));
        }
        Some(Arc::new(lut))
    } else {
        None
    };
    let relight_map = if crate::relight::requires_normal_map(&adjustments) {
        check_cancel(&cancel)?;
        let field = "relightNormalMap";
        let captured = capture_rgba_map(requested_map(&adjustments, field)?, &adjustments);
        check_cancel(&cancel)?;
        let map =
            captured.map_err(|source| CreativeCaptureError::InvalidEffectMap { field, source })?;
        if map.width() < 2 || map.height() < 2 {
            return Err(CreativeCaptureError::EffectMapTooSmall(field));
        }
        Some(map)
    } else {
        None
    };
    // Match apply_fog's effective amount before requesting any map bytes.
    let fog_amount =
        (adjustments["fogAmount"].as_f64().unwrap_or(50.0) as f32 / 100.0).clamp(0.0, 1.0);
    let fog_noop = fog_amount <= 0.0;
    let fog_map =
        if effects_visible && adjustments["fogEnabled"].as_bool().unwrap_or(false) && !fog_noop {
            Some(capture_depth(&adjustments, "fogDepthMap", &cancel)?)
        } else {
            None
        };
    // Lens blur uses the direct f32 amount, without fog's division/clamp.
    let lens_blur_amount = adjustments["lensBlurAmount"].as_f64().unwrap_or(50.0) as f32;
    let lens_blur_noop = lens_blur_amount <= 0.0;
    let lens_blur_map = if effects_visible
        && adjustments["lensBlurEnabled"].as_bool().unwrap_or(false)
        && !lens_blur_noop
    {
        Some(capture_depth(&adjustments, "lensBlurDepthMap", &cancel)?)
    } else {
        None
    };
    check_cancel(&cancel)?;
    Ok(CapturedCreativeAssets {
        adjustments,
        settings,
        masks,
        lut,
        relight_map,
        fog_map,
        lens_blur_map,
    })
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use base64::Engine as _;
    use image::{DynamicImage, ImageFormat, Rgba};
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    fn inline(color: [u8; 4]) -> String {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes.get_ref())
    }
    fn patch(id: &str, data: Value) -> Value {
        json!({"id":id,"visible":true,"patchData":data,"subMasks":[]})
    }
    fn patch_data(color: &str) -> Value {
        json!({"color":color,"mask":color})
    }
    fn sub(id: &str, kind: &str, parameters: Value) -> Value {
        json!({"id":id,"type":kind,"visible":true,"invert":false,
               "opacity":100.0,"mode":"additive","parameters":parameters})
    }
    fn mask(id: &str, subs: Value) -> Value {
        json!({"id":id,"name":id,"visible":true,"invert":false,
               "opacity":100.0,"adjustments":{},"subMasks":subs})
    }
    fn capture(value: &Value) -> Result<CapturedCreativeAssets, CreativeCaptureError> {
        capture_creative_assets(value, &AppSettings::default(), None)
    }

    #[test]
    fn c01_capture_immutability_and_independent_hydration() {
        let png = inline([10, 20, 30, 40]);
        let mut settings = AppSettings::default();
        settings.raw_preprocessing_sharpening = Some(0.125);
        let mut input = json!({"exposure":1.25,"aiPatches":[patch("shared",Value::Null),
                             patch("shared",patch_data(&png))],
                             "relightEnabled":true,"relightAmbient":12.0,"relightNormalMap":png,
                             "fogEnabled":true,"fogDepthMap":png,
                             "lensBlurEnabled":true,"lensBlurDepthMap":png});
        let original = input.clone();
        let captured = capture_creative_assets(&input, &settings, None).unwrap();
        assert_eq!(input, original);
        assert_eq!(
            captured.adjustments()["aiPatches"][0]["patchData"],
            patch_data(&png)
        );
        assert_eq!(captured.adjustments()["exposure"], 1.25);
        assert_eq!(
            captured.settings().raw_preprocessing_sharpening,
            Some(0.125)
        );
        let rgba = Arc::clone(captured.relight_map().unwrap());
        let fog = Arc::clone(captured.fog_map().unwrap());
        assert_eq!(rgba.get_pixel(0, 0).0, [10, 20, 30, 40]);
        assert_eq!(fog.dimensions(), (2, 2));
        assert_eq!(captured.lens_blur_map().unwrap().dimensions(), (2, 2));
        input["exposure"] = json!(-9.0);
        input["aiPatches"] = json!([]);
        input["relightNormalMap"] = json!("invalid");
        settings.raw_preprocessing_sharpening = Some(0.75);
        assert_eq!(settings.raw_preprocessing_sharpening, Some(0.75));
        assert_eq!(
            captured.settings().raw_preprocessing_sharpening,
            Some(0.125)
        );
        assert_eq!(captured.adjustments()["exposure"], 1.25);
        assert!(Arc::ptr_eq(&rgba, captured.relight_map().unwrap()));
        assert_eq!(rgba.get_pixel(0, 0).0, [10, 20, 30, 40]);
        assert!(matches!(
            capture(&json!({"aiPatches":[patch("shared",Value::Null)]})),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::MissingPayload { .. }
            ))
        ));
        let cancelled = Arc::new(AtomicUsize::new(1));
        assert!(matches!(
            capture_creative_assets(
                &json!({"lutPath":"must-not-open.cube"}),
                &AppSettings::default(),
                Some((cancelled, 0))
            ),
            Err(CreativeCaptureError::Cancelled)
        ));
    }

    #[test]
    fn c02_visible_payload_alias_domain_and_mask_boundaries() {
        let a = inline([255, 0, 0, 255]);
        let other = inline([0, 0, 255, 255]);
        for params in [
            json!({"mask_data_base64":a}),
            json!({"maskDataBase64":a}),
            json!({"mask_data_base64":a,"maskDataBase64":a}),
            json!({"mask_data_base64":null,"maskDataBase64":a}),
        ] {
            let input = json!({"masks":[mask("one",json!([sub("bitmap","ai-sky",params)]))]});
            let captured = capture(&input).unwrap();
            assert_eq!(captured.masks().len(), 1);
            assert_eq!(
                captured.masks()[0].sub_masks[0].parameters["maskDataBase64"],
                a
            );
            let typed: crate::ai_processing::AiSkyMaskParameters =
                serde_json::from_value(captured.masks()[0].sub_masks[0].parameters.clone())
                    .unwrap();
            assert_eq!(typed.mask_data_base64.as_deref(), Some(a.as_str()));
        }
        let refs = json!({"aiPatches":[patch("same",patch_data(&a))],"masks":[
            mask("first",json!([sub("same","ai-sky",json!({"maskDataBase64":null}))])),
            mask("second",json!([sub("same","ai-sky",json!({"maskDataBase64":a}))]))]});
        assert_eq!(
            capture(&refs).unwrap().masks()[0].sub_masks[0].parameters["maskDataBase64"],
            a
        );
        let missing = json!({"aiPatches":[patch("same",patch_data(&a))],"masks":[
            mask("first",json!([sub("same","ai-sky",json!({"maskDataBase64":null}))]))]});
        assert!(matches!(
            capture(&missing),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::MissingPayload {
                    domain: "submask bitmap",
                    ..
                }
            ))
        ));
        let conflict = json!({"masks":[mask("one",json!([sub("x","ai-sky",
            json!({"mask_data_base64":a,"maskDataBase64":other}))]))]});
        assert!(matches!(
            capture(&conflict),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::ConflictingAliases { .. }
            ))
        ));
        let conflict =
            json!({"aiPatches":[patch("x",patch_data(&a)),patch("x",patch_data(&other))]});
        assert!(matches!(
            capture(&conflict),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::ConflictingPayload {
                    domain: "patchData",
                    ..
                }
            ))
        ));
        let bitmap_conflict = json!({"masks":[
            mask("a",json!([sub("duplicate","ai-sky",json!({"maskDataBase64":a}))])),
            mask("b",json!([sub("duplicate","ai-sky",json!({"maskDataBase64":other}))]))]});
        assert!(matches!(
            capture(&bitmap_conflict),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::ConflictingPayload {
                    domain: "submask bitmap",
                    ..
                }
            ))
        ));
        for invalid in [
            json!({"aiPatches":[patch("x",json!({"color":"invalid","mask":a}))]}),
            json!({"aiPatches":[patch("x",json!({"mask":a}))]}),
            json!({"masks":[mask("x",json!([sub("x","unknown-visible",json!({}))]))]}),
        ] {
            assert!(matches!(
                capture(&invalid),
                Err(CreativeCaptureError::Hydration(_))
            ));
        }
        let hidden = json!({"aiPatches":[{"visible":false,"patchData":"malformed"}],
            "masks":[{"visible":false},mask("empty",json!([])),
                mask("hidden-child",json!([{"visible":false}]))],
            "sectionVisibility":{"effects":false},"lutPath":"must-not-open.cube",
            "relightEnabled":true,"relightNormalMap":"invalid",
            "fogEnabled":true,"lensBlurEnabled":true});
        let before = hidden.clone();
        let captured = capture(&hidden).unwrap();
        assert_eq!(hidden, before);
        assert!(captured.masks().is_empty());
        assert!(
            captured.lut().is_none()
                && captured.relight_map().is_none()
                && captured.fog_map().is_none()
                && captured.lens_blur_map().is_none()
        );
        let disabled = capture(&json!({"relightEnabled":false,"relightNormalMap":"invalid",
            "fogEnabled":false,"fogDepthMap":"invalid","lensBlurEnabled":false,
            "lensBlurDepthMap":"invalid"}))
        .unwrap();
        assert!(
            disabled.relight_map().is_none()
                && disabled.fog_map().is_none()
                && disabled.lens_blur_map().is_none()
        );
        let all: Vec<Value> = (0..32)
            .map(|i| mask(&format!("m{i}"), json!([sub("all", "all", json!({}))])))
            .collect();
        assert_eq!(
            capture(&json!({"masks":all.clone()}))
                .unwrap()
                .masks()
                .len(),
            32
        );
        let mut too_many = all;
        too_many.push(mask("33", json!([sub("all", "all", json!({}))])));
        assert!(matches!(
            capture(&json!({"masks":too_many})),
            Err(CreativeCaptureError::Hydration(
                SnapshotHydrationError::TooManyMasks(33)
            ))
        ));
        // Irrelevant generated submasks are not requested when explicit mask exists.
        let mut p = patch("explicit", patch_data(&a));
        p["subMasks"] = json!([sub("bad", "unknown", json!({}))]);
        assert!(capture(&json!({"aiPatches":[p]})).is_ok());
        for value in [
            json!({"fogEnabled":true}),
            json!({"relightEnabled":true,"relightAmbient":12.0}),
            json!({"lensBlurEnabled":true,"lensBlurDepthMap":"invalid"}),
        ] {
            assert!(capture(&value).is_err());
        }
    }

    #[test]
    fn c03_actual_LUT_parsers_snapshot_once() {
        let parent =
            std::env::temp_dir().join(format!("b2c-creative-lut-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&parent).unwrap(); // Retained synthetic evidence, not user files.
        let cube = parent.join("one.cube");
        let three = parent.join("one.3dl");
        let hald = parent.join("one.png");
        std::fs::write(&cube, b"LUT_3D_SIZE 1\n0.25 0.5 1\n").unwrap();
        std::fs::write(&three, b"0.25 0.5 1\n").unwrap();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255])))
            .save_with_format(&hald, ImageFormat::Png)
            .unwrap();
        for (path, expected) in [
            (&cube, [0.25, 0.5, 1.0]),
            (&three, [0.25, 0.5, 1.0]),
            (&hald, [1.0, 0.0, 0.0]),
        ] {
            let input = json!({"lutPath":path.to_str().unwrap(),"lutIntensity":50.0});
            let captured = capture(&input).unwrap();
            let held = Arc::clone(captured.lut().unwrap());
            assert_eq!(held.size, 1);
            assert_eq!(held.data, expected);
            std::fs::write(path, b"malformed replacement").unwrap();
            assert!(Arc::ptr_eq(&held, captured.lut().unwrap()));
            assert_eq!(captured.lut().unwrap().data, expected);
            assert!(matches!(capture(&input), Err(CreativeCaptureError::Lut(_))));
        }
        let missing = parent.join("missing.cube");
        for path in ["", missing.to_str().unwrap()] {
            assert!(matches!(
                capture(&json!({"lutPath":path})),
                Err(CreativeCaptureError::Lut(_))
            ));
        }
        let bad = parent.join("nan.cube");
        std::fs::write(&bad, b"LUT_3D_SIZE 1\nNaN 0 1\n").unwrap();
        assert!(matches!(
            capture(&json!({"lutPath":bad.to_str().unwrap()})),
            Err(CreativeCaptureError::Lut(_))
        ));
        assert!(
            capture(&json!({"sectionVisibility":{"effects":false},"lutPath":"must-not-open.cube"}))
                .unwrap()
                .lut()
                .is_none()
        );
        // Android URI resolution requires its actual platform/provider gate;
        // no desktop mock or this local parser test claims Android acceptance.
    }

    #[test]
    fn c06_zero_effect_relight_ignores_missing_or_malformed_map() {
        let base = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(4, 4, |x, y| {
            image::Rgb([0.1 + x as f32 * 0.02, 0.2 + y as f32 * 0.03, 0.4])
        }));
        let cases = [
            json!({"relightEnabled":true}),
            json!({"relightEnabled":true,"relightAmbient":0.0,"relightLights":[]}),
            json!({"relightEnabled":true,"relightAmbient":f64::MIN_POSITIVE}),
            json!({"relightEnabled":true,"relightAmbient":-0.0,
                   "relightLights":[{"visible":false}]}),
            json!({"relightEnabled":true,"relightLights":"legacy-non-array"}),
        ];
        for case in cases {
            for map in [None, Some("not-base64!")] {
                let mut input = case.clone();
                if let Some(map) = map {
                    input["relightNormalMap"] = json!(map);
                }
                let before = input.clone();
                let captured = capture(&input).unwrap();
                assert!(captured.relight_map().is_none());
                assert_eq!(captured.adjustments(), &before);
                assert_eq!(input, before);
                let rendered =
                    crate::relight::apply_relight(std::borrow::Cow::Borrowed(&base), &input)
                        .into_owned();
                assert_eq!(rendered.as_bytes(), base.as_bytes());
            }
        }
    }

    #[test]
    fn c07_missing_light_visibility_keeps_legacy_active_default() {
        let missing = json!({"relightEnabled":true,"relightLights":[{}]});
        assert!(matches!(
            capture(&missing),
            Err(CreativeCaptureError::MissingEffectMap("relightNormalMap"))
        ));

        let malformed = json!({"relightEnabled":true,"relightLights":[{"visible":"hidden"}],
                               "relightNormalMap":"not-base64!"});
        assert!(matches!(
            capture(&malformed),
            Err(CreativeCaptureError::InvalidEffectMap { field, .. })
                if field == "relightNormalMap"
        ));

        let subnormal = json!({"relightEnabled":true,
                               "relightAmbient":f32::from_bits(1) as f64});
        assert!(matches!(
            capture(&subnormal),
            Err(CreativeCaptureError::MissingEffectMap("relightNormalMap"))
        ));
    }

    #[test]
    fn c08_active_ambient_requires_map_and_changes_pixels_with_valid_map() {
        let active = json!({"relightEnabled":true,"relightAmbient":12.0});
        assert!(matches!(
            capture(&active),
            Err(CreativeCaptureError::MissingEffectMap("relightNormalMap"))
        ));
        let malformed = json!({"relightEnabled":true,"relightAmbient":12.0,
                               "relightNormalMap":"not-base64!"});
        assert!(matches!(
            capture(&malformed),
            Err(CreativeCaptureError::InvalidEffectMap { field, .. })
                if field == "relightNormalMap"
        ));

        let with_map = json!({"relightEnabled":true,"relightAmbient":12.0,
                              "relightNormalMap":inline([128,128,255,255])});
        let captured = capture(&with_map).unwrap();
        let base = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(8, 8, |x, y| {
            image::Rgb([0.1 + x as f32 * 0.01, 0.2 + y as f32 * 0.02, 0.35])
        }));
        let mut scratch = crate::relight::LocalRelightScratch::default();
        let rendered = crate::relight::apply_relight_captured(
            std::borrow::Cow::Borrowed(&base),
            captured.adjustments(),
            captured.relight_map().unwrap(),
            &mut scratch,
        )
        .into_owned();
        assert_ne!(rendered.as_bytes(), base.as_bytes());
    }
}

#[cfg(test)]
mod zero_strength_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn c05_zero_strength_fog_and_lens_blur_noops() {
        let settings = AppSettings::default();
        for (enabled, amount_field, map_field) in [
            ("fogEnabled", "fogAmount", "fogDepthMap"),
            ("lensBlurEnabled", "lensBlurAmount", "lensBlurDepthMap"),
        ] {
            for amount in [0.0, -0.0, -1.0, -100.0, f64::MIN_POSITIVE] {
                for payload in [None, Some("not-base64!")] {
                    let mut input = json!({});
                    input[enabled] = json!(true);
                    input[amount_field] = json!(amount);
                    if let Some(payload) = payload {
                        input[map_field] = json!(payload);
                    }
                    let before = input.clone();
                    let captured = capture_creative_assets(&input, &settings, None).unwrap();
                    assert!(captured.fog_map().is_none());
                    assert!(captured.lens_blur_map().is_none());
                    assert_eq!(captured.adjustments(), &before);
                    assert_eq!(input, before);
                }
            }

            // Missing and malformed amounts retain the existing default of50.
            for amount in [
                Some(json!(1.0)),
                Some(json!(100.0)),
                Some(json!(f64::MAX)),
                None,
                Some(Value::Null),
                Some(json!("zero")),
                Some(json!(false)),
            ] {
                for payload in [None, Some("not-base64!")] {
                    let mut input = json!({});
                    input[enabled] = json!(true);
                    if let Some(amount) = &amount {
                        input[amount_field] = amount.clone();
                    }
                    if let Some(payload) = payload {
                        input[map_field] = json!(payload);
                    }
                    let before = input.clone();
                    let result = capture_creative_assets(&input, &settings, None);
                    if payload.is_none() {
                        assert!(matches!(result,
                            Err(CreativeCaptureError::MissingEffectMap(field)) if field == map_field));
                    } else {
                        assert!(matches!(result,
                            Err(CreativeCaptureError::InvalidEffectMap { field, .. }) if field == map_field));
                    }
                    assert_eq!(input, before);
                }
            }

            // Fog's division can underflow after a positive f32 conversion;
            // lens blur must still request its map for that same positive f32.
            for payload in [None, Some("not-base64!")] {
                let mut input = json!({});
                input[enabled] = json!(true);
                input[amount_field] = json!(f32::from_bits(1) as f64);
                if let Some(payload) = payload {
                    input[map_field] = json!(payload);
                }
                let before = input.clone();
                let result = capture_creative_assets(&input, &settings, None);
                if enabled == "fogEnabled" {
                    let captured = result.unwrap();
                    assert!(captured.fog_map().is_none());
                    assert!(captured.lens_blur_map().is_none());
                    assert_eq!(captured.adjustments(), &before);
                } else if payload.is_none() {
                    assert!(matches!(result,
                        Err(CreativeCaptureError::MissingEffectMap(field)) if field == map_field));
                } else {
                    assert!(matches!(result,
                        Err(CreativeCaptureError::InvalidEffectMap { field, .. }) if field == map_field));
                }
                assert_eq!(input, before);
            }

            for (visible, flag) in [(false, true), (true, false)] {
                let mut input = json!({"sectionVisibility":{"effects":visible}});
                input[enabled] = json!(flag);
                input[map_field] = json!("not-base64!");
                let before = input.clone();
                let captured = capture_creative_assets(&input, &settings, None).unwrap();
                assert!(captured.fog_map().is_none());
                assert!(captured.lens_blur_map().is_none());
                assert_eq!(captured.adjustments(), &before);
                assert_eq!(input, before);
            }
        }
        let input = json!({"fogEnabled":true,"fogAmount":0,"fogDepthMap":"not-base64!",
            "lensBlurEnabled":true,"lensBlurAmount":0,"lensBlurDepthMap":"not-base64!"});
        let before = input.clone();
        assert!(matches!(
            capture_creative_assets(&input, &settings, Some((Arc::new(AtomicUsize::new(2)), 1))),
            Err(CreativeCaptureError::Cancelled)
        ));
        assert_eq!(input, before);
        let input = json!({"fogEnabled":true,"fogAmount":0,
            "lensBlurEnabled":true,"lensBlurAmount":0,"relightEnabled":true});
        let before = input.clone();
        let captured = capture_creative_assets(&input, &settings, None).unwrap();
        assert!(captured.relight_map().is_none());
        assert!(captured.fog_map().is_none() && captured.lens_blur_map().is_none());
        assert_eq!(captured.adjustments(), &before);
        assert_eq!(input, before);
    }
}
#[cfg(test)]
mod capture_mask_repair_tests {
    use super::*;
    use base64::Engine as _;
    use image::{DynamicImage, ImageFormat, Rgba};
    use serde_json::{Value, json};

    fn inline(color: [u8; 4]) -> String {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes.get_ref())
    }

    fn radial_submask(id: &str, visible: bool) -> Value {
        json!({
            "id": id, "type": "radial", "visible": visible, "invert": false,
            "opacity": 100.0, "mode": "additive",
            "parameters": {
                "centerX": 0.5, "centerY": 0.5, "radiusX": 1.0,
                "radiusY": 1.0, "rotation": 0.0, "feather": 0.0
            }
        })
    }

    fn mask(id: &str, invert: bool, sub_masks: Value, adjustments: Value) -> Value {
        json!({
            "id": id, "name": id, "visible": true, "invert": invert,
            "opacity": 100.0, "adjustments": adjustments, "subMasks": sub_masks
        })
    }

    fn patch(
        id: &str,
        color: &str,
        mask_data: &str,
        invert: bool,
        sub_masks: Option<Value>,
    ) -> Value {
        let mut value = json!({
            "id": id, "name": id, "visible": true, "invert": invert,
            "patchData": {"color": color, "mask": mask_data}
        });
        if let Some(sub_masks) = sub_masks {
            value["subMasks"] = sub_masks;
        }
        value
    }

    fn capture(input: &Value) -> Result<CapturedCreativeAssets, CreativeCaptureError> {
        capture_creative_assets(input, &AppSettings::default(), None)
    }

    fn black_base() -> DynamicImage {
        DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            2,
            2,
            image::Rgb([0.0, 0.0, 0.0]),
        ))
    }

    #[test]
    fn c06_hidden_generated_patch_preserves_parent_inversion_and_explicit_mask() {
        let color = inline([255, 0, 0, 255]);
        let hidden_children = json!([radial_submask("hidden", false)]);
        let base = black_base();

        for invert in [false, true] {
            let typed: MaskDefinition = serde_json::from_value(mask(
                "generated",
                invert,
                hidden_children.clone(),
                json!({}),
            ))
            .unwrap();
            let generated =
                crate::mask_generation::generate_mask_bitmap(&typed, 2, 2, 1.0, (0.0, 0.0), None)
                    .unwrap();
            let expected = if invert { 255 } else { 0 };
            assert!(generated.as_raw().iter().all(|value| *value == expected));

            let input = json!({"aiPatches":[patch(
                "generated",
                &color,
                "",
                invert,
                Some(hidden_children.clone())
            )]});
            let before = input.clone();
            let captured = capture(&input).unwrap();
            assert_eq!(input, before);
            assert_eq!(
                captured.adjustments()["aiPatches"][0]["subMasks"],
                hidden_children
            );
            let composited =
                crate::image_loader::composite_patches_on_image(&base, captured.adjustments())
                    .unwrap()
                    .to_rgb32f();
            let expected_pixel = if invert {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            };
            for pixel in composited.pixels() {
                assert_eq!(pixel.0, expected_pixel);
            }
        }

        let white_mask = inline([255, 255, 255, 255]);
        let irrelevant_malformed_children = json!([{
            "visible": true, "type": "unknown-visible", "parameters": {}
        }]);
        let input = json!({"aiPatches":[patch(
            "explicit",
            &color,
            &white_mask,
            false,
            Some(irrelevant_malformed_children.clone())
        )]});
        let captured = capture(&input).unwrap();
        assert_eq!(
            captured.adjustments()["aiPatches"][0]["subMasks"],
            irrelevant_malformed_children
        );
        let composited =
            crate::image_loader::composite_patches_on_image(&base, captured.adjustments())
                .unwrap()
                .to_rgb32f();
        for pixel in composited.pixels() {
            assert_eq!(pixel.0, [1.0, 0.0, 0.0]);
        }

        let missing = json!({"aiPatches":[patch("missing", &color, "", false, None)]});
        assert!(matches!(
            capture(&missing),
            Err(CreativeCaptureError::Hydration(_))
        ));
        let empty = json!({"aiPatches":[patch(
            "empty",
            &color,
            "",
            false,
            Some(json!([]))
        )]});
        assert!(matches!(
            capture(&empty),
            Err(CreativeCaptureError::Hydration(_))
        ));
        let all_pruned = json!({"aiPatches":[patch(
            "pruned",
            &color,
            "",
            false,
            Some(json!([{"id":"broken-hidden","visible":false}]))
        )]});
        assert!(matches!(
            capture(&all_pruned),
            Err(CreativeCaptureError::Hydration(_))
        ));
        let malformed_visible = json!({"aiPatches":[patch(
            "malformed-visible",
            &color,
            "",
            false,
            Some(json!([{
                "id":"bad", "type":"unknown-visible", "visible":true,
                "invert":false, "opacity":100.0, "mode":"additive", "parameters":{}
            }]))
        )]});
        assert!(matches!(
            capture(&malformed_visible),
            Err(CreativeCaptureError::Hydration(_))
        ));
    }

    #[test]
    fn c07_visible_empty_masks_are_validated_and_uniform_order_is_preserved() {
        let radial = |id: &str| json!([radial_submask(id, true)]);
        let malformed_empty = json!({"visible":true,"subMasks":[]});
        let active_first = mask(
            "first",
            false,
            radial("radial-first"),
            json!({"sectionVisibility":{"basic":true},"exposure":1.25}),
        );
        let active_second = mask(
            "second",
            false,
            radial("radial-second"),
            json!({"sectionVisibility":{"basic":true},"exposure":2.5}),
        );
        let invalid = json!({"masks":[
            malformed_empty,
            active_first.clone(),
            active_second.clone()
        ]});
        let invalid_before = invalid.clone();
        assert!(matches!(
            capture(&invalid),
            Err(CreativeCaptureError::Hydration(_))
        ));
        assert_eq!(invalid, invalid_before);

        let empty = mask("empty", false, json!([]), json!({}));
        let input = json!({"masks":[empty,active_first,active_second]});
        let before = input.clone();
        let captured = capture(&input).unwrap();
        assert_eq!(input, before);

        let retained = captured.adjustments()["masks"].as_array().unwrap();
        assert_eq!(retained.len(), 3);
        assert_eq!(retained[0]["id"], "empty");
        assert_eq!(retained[1]["id"], "first");
        assert_eq!(retained[2]["id"], "second");

        let typed = captured.masks();
        assert_eq!(typed.len(), 2);
        assert_eq!(typed[0].id, "first");
        assert_eq!(typed[1].id, "second");

        let uniform = crate::image_processing::get_all_adjustments_from_json(
            captured.adjustments(),
            false,
            crate::as_shot::AsShot::reference(),
            crate::image_processing::RenderOptions::default(),
        );
        assert_eq!(uniform.mask_count, 2);
        let first_exposure = uniform.mask_adjustments[0].exposure;
        let second_exposure = uniform.mask_adjustments[1].exposure;
        assert_ne!(first_exposure, 0.0);
        assert!(second_exposure > first_exposure);
    }
}

#[cfg(test)]
mod open_capture_diagnostic_tests {
    use super::*;

    #[test]
    fn typed_capture_categories_do_not_expose_payloads() {
        let secret = "/private/user/asset-and-payload";
        let cases = [
            (CreativeCaptureError::Cancelled, "cancelled"),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::Invalid(secret.into())),
                "hydration_invalid",
            ),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::MissingPayload {
                    domain: secret,
                    id: secret.into(),
                }),
                "hydration_missing_payload",
            ),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::ConflictingPayload {
                    domain: secret,
                    id: secret.into(),
                }),
                "hydration_conflicting_payload",
            ),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::ConflictingAliases {
                    id: secret.into(),
                }),
                "hydration_conflicting_aliases",
            ),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::UnknownVisibleMask(
                    secret.into(),
                )),
                "hydration_unknown_visible_mask",
            ),
            (
                CreativeCaptureError::Hydration(SnapshotHydrationError::TooManyMasks(99)),
                "hydration_too_many_masks",
            ),
            (
                CreativeCaptureError::InvalidVisibleMask(secret.into()),
                "invalid_visible_mask",
            ),
            (
                CreativeCaptureError::MissingEffectMap(secret),
                "missing_effect_map",
            ),
            (
                CreativeCaptureError::InvalidEffectMap {
                    field: secret,
                    source: CapturedMapError::InvalidInlineImage,
                },
                "invalid_effect_map",
            ),
            (
                CreativeCaptureError::EffectMapTooSmall(secret),
                "effect_map_too_small",
            ),
            (CreativeCaptureError::Lut(secret.into()), "lut_error"),
        ];
        for (error, expected) in cases {
            assert_eq!(error.diagnostic_category(), expected);
            assert!(!error.diagnostic_category().contains(secret));
        }
    }
}
