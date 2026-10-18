//! Shared CPU profile path for preview and export. Output is oriented, linear
//! sRGB/Rec.709 working RGB. No selected profile may silently fall back.
use super::{
    camera_aliases::camera_model_matches, model::DcpProfile, registry::ProfileRegistry,
    render::DcpRenderer,
};
use image::DynamicImage;
use rayon::prelude::*;
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

type Cancel = Option<(Arc<AtomicUsize>, usize)>;

fn selection(adjustments: &Value) -> Result<(Option<String>, Option<&str>), String> {
    let Some(p) = adjustments.get("profile").filter(|p| !p.is_null()) else {
        return Ok((None, None));
    };
    if !p.is_object() {
        return Err("profile must be an object".into());
    }
    let field = |key| match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        _ => Err(format!("profile.{key} must be a string or null")),
    };
    let base = match p.get("base") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let id: super::model::ProfileId = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid profile.base: {e}"))?;
            Some(id.to_hex())
        }
    };
    // Only base IDs had the historical array wire format. Look UUIDs stay strings.
    Ok((base, field("look")?))
}

/// True also for a Look-only selection; malformed selections are errors.
pub fn has_selected_profile(adjustments: &Value) -> Result<bool, String> {
    let (base, look) = selection(adjustments)?;
    Ok(base.is_some() || look.is_some())
}

/// Rawler coefficients are RGBE, NOT Bayer RGGB. E is unused for RGB.
pub fn neutral_from_wb(wb: [f32; 4]) -> Result<[f32; 3], String> {
    if wb[..3].iter().any(|c| !c.is_finite() || *c <= 0.0) {
        return Err("DCP requires finite positive RGB white-balance coefficients".into());
    }
    let neutral = [wb[1] / wb[0], 1.0, wb[1] / wb[2]];
    if neutral.iter().any(|c| !c.is_finite() || *c <= 0.0) {
        return Err("DCP white-balance neutral is outside the supported range".into());
    }
    Ok(neutral)
}

fn check_cancel(cancel: &Cancel) -> Result<(), String> {
    if let Some((token, expected)) = cancel
        && token.load(Ordering::Relaxed) != *expected
    {
        return Err("Profile rendering cancelled".into());
    }
    Ok(())
}

pub(super) fn matches_camera(profile_camera: &str, make: &str, model: &str, clean: &str) -> bool {
    !profile_camera.trim().is_empty()
        && [
            model.to_string(),
            clean.to_string(),
            format!("{make} {model}"),
            format!("{make} {clean}"),
        ]
        .iter()
        .filter(|s| !s.trim().is_empty())
        .any(|s| camera_model_matches(profile_camera, s))
}

fn load_base(registry: &ProfileRegistry, id: &str) -> Result<DcpProfile, String> {
    let entry = registry
        .get_by_hex(id)
        .ok_or_else(|| format!("Selected DCP '{id}' is missing from the registry"))?;
    super::parser::parse_dcp(&entry.file_path)
        .map_err(|e| format!("Cannot read selected DCP: {e}"))?
        .into_iter()
        .find(|p| p.id == entry.id)
        .ok_or_else(|| "Selected DCP content changed; rescan and select the profile again".into())
}

pub(super) fn validate_pairing(
    profile: &DcpProfile,
    required: &str,
    restriction: Option<&str>,
    make: &str,
    model: &str,
    clean: &str,
) -> Result<(), String> {
    if profile.profile_name != required {
        return Err(format!(
            "Selected Look requires base '{required}', but selected base is '{}'",
            profile.profile_name
        ));
    }
    if let Some(camera) = restriction.filter(|s| !s.trim().is_empty())
        && !matches_camera(camera, make, model, clean)
    {
        return Err(format!("Selected Look is restricted to camera '{camera}'"));
    }
    Ok(())
}

fn render_working(
    image: DynamicImage,
    renderer: &DcpRenderer,
    look: Option<&super::rgb_look::RgbLook>,
    cancel: &Cancel,
) -> Result<DynamicImage, String> {
    let mut rgb = image.to_rgb32f();
    rgb.as_mut()
        .par_chunks_mut(3 * 4096)
        .try_for_each(|chunk| {
            check_cancel(cancel)?;
            for pixel in chunk.as_chunks_mut::<3>().0 {
                let mut prophoto = renderer.render_pixel([pixel[0], pixel[1], pixel[2]]);
                if let Some(look) = look {
                    // Explicit bounded non-HDR adaptation; RGB Look is not an
                    // HDR or full Adobe exposure/tone pipeline implementation.
                    prophoto = look.apply_non_hdr(prophoto)?;
                }
                let working = renderer.to_working_space(prophoto);
                pixel.copy_from_slice(&working);
            }
            Ok::<_, String>(())
        })?;
    check_cancel(cancel)?;
    Ok(DynamicImage::ImageRgb32F(rgb))
}

/// Owned parsed assets for one selection. Both developments borrow this exact
/// snapshot; neither renderer reopens registry paths or rereads Amount.
pub(crate) struct SelectedProfileAssets {
    profile: DcpProfile,
    look: Option<super::look_xmp::CobaltLook>,
    amount: Option<Value>,
}

/// Identity of these already-parsed owned assets, never registry/path/UUID state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectedProfileIdentity {
    base: super::model::ProfileId,
    look: Option<super::look_xmp::LookId>,
    amount: Option<Value>,
}
impl SelectedProfileAssets {
    pub(crate) fn content_identity(&self) -> SelectedProfileIdentity {
        SelectedProfileIdentity {
            base: self.profile.id,
            look: self.look.as_ref().map(|look| look.id.clone()),
            amount: self.amount.clone(),
        }
    }
}

pub(crate) fn prepare_selected_profile(
    adjustments: &Value,
    registry: &ProfileRegistry,
    cancel: &Cancel,
) -> Result<Option<SelectedProfileAssets>, String> {
    let (base, look_id) = selection(adjustments)?;
    if base.is_none() && look_id.is_none() {
        return Ok(None);
    }
    check_cancel(cancel)?;
    let base = base.ok_or("Selected Look requires an explicitly selected base DCP")?;
    let look = look_id
        .map(|id| {
            let entry = registry
                .looks()
                .into_iter()
                .find(|l| l.uuid == id)
                .ok_or_else(|| format!("Selected Look '{id}' is missing from the registry"))?;
            let look = super::look_xmp::parse_look_xmp(&entry.file_path)
                .map_err(|e| format!("Cannot read selected Look: {e}"))?;
            if look.uuid != id {
                return Err("Selected Look identity changed; rescan profiles".into());
            }
            Ok::<_, String>(look)
        })
        .transpose()?;
    let profile = load_base(registry, &base)?;
    let amount = adjustments
        .get("profile")
        .and_then(|p| p.get("amount"))
        .cloned();
    Ok(Some(SelectedProfileAssets {
        profile,
        look,
        amount,
    }))
}

/// `None` means no selection. `Some` is ready for ordinary GPU adjustments;
/// callers must not apply DCP GPU uniforms or another working-space conversion.
/// RAW development must return unoriented, un-white-balanced camera-native RGB.
#[allow(clippy::too_many_arguments)]
pub fn render_selected_profile(
    bytes: &[u8],
    adjustments: &Value,
    registry: &ProfileRegistry,
    fast: bool,
    highlight: f32,
    linear_mode: String,
    cancel: Cancel,
) -> Result<Option<DynamicImage>, String> {
    let Some(assets) = prepare_selected_profile(adjustments, registry, &cancel)? else {
        return Ok(None);
    };
    let (image, orientation, meta) = crate::raw_processing::develop_raw_image_with_meta(
        bytes,
        fast,
        highlight,
        linear_mode,
        true,
        cancel.clone(),
    )
    .map_err(|e| format!("Profile RAW development failed: {e}"))?;
    render_profile_camera_image(image, orientation, &meta, &assets, &cancel).map(Some)
}

pub(crate) fn render_prepared_selected_profile(
    prepared: &crate::raw_processing::PreparedRaw,
    assets: &SelectedProfileAssets,
    quality: crate::raw_processing::PreparedQuality,
    highlight: f32,
    linear_mode: &str,
    cancel: Cancel,
) -> Result<DynamicImage, String> {
    #[cfg(debug_assertions)]
    let profile_raw_started = std::time::Instant::now();
    let (image, orientation, meta) = crate::raw_processing::develop_prepared_raw(
        prepared,
        quality,
        highlight,
        linear_mode,
        true,
        cancel.clone(),
    )
    .map_err(|e| format!("Profile RAW development failed: {e}"))?;
    #[cfg(debug_assertions)]
    log::info!(
        "[preview-stage] stage=profile_raw elapsed={}ns outcome=completed",
        profile_raw_started.elapsed().as_nanos()
    );
    render_profile_camera_image(image, orientation, &meta, assets, &cancel)
}

fn render_profile_camera_image(
    image: DynamicImage,
    orientation: rawler::decoders::Orientation,
    meta: &crate::raw_processing::RawImageMeta,
    assets: &SelectedProfileAssets,
    cancel: &Cancel,
) -> Result<DynamicImage, String> {
    let profile = &assets.profile;
    #[cfg(debug_assertions)]
    log::info!("Profile diagnostics: camera-native RAW development complete");
    check_cancel(cancel)?;
    #[cfg(debug_assertions)]
    let profile_setup_started = std::time::Instant::now();
    if !matches_camera(
        &profile.unique_camera_model,
        &meta.make,
        &meta.model,
        &meta.clean_model,
    ) {
        return Err(format!(
            "Selected DCP is for '{}', not RAW camera '{} {}'",
            profile.unique_camera_model, meta.make, meta.model
        ));
    }
    let rgb_look = if let Some(look) = assets.look.as_ref() {
        validate_pairing(
            profile,
            &look.required_base_profile,
            look.camera_model_restriction.as_deref(),
            &meta.make,
            &meta.model,
            &meta.clean_model,
        )?;
        let amount = match assets.amount.as_ref() {
            None => 100.0,
            Some(value) => value.as_f64().ok_or("Look amount must be numeric")?,
        };
        Some(super::rgb_look::RgbLook::new(
            &look.table,
            amount,
            look.supports_amount,
        )?)
    } else {
        None
    };
    let renderer = DcpRenderer::new(profile, neutral_from_wb(meta.wb_coeffs)?)
        .map_err(|e| format!("Cannot build selected DCP renderer: {e}"))?;
    #[cfg(debug_assertions)]
    log::info!(
        "[preview-stage] stage=profile_setup elapsed={}ns outcome=completed",
        profile_setup_started.elapsed().as_nanos()
    );
    #[cfg(debug_assertions)]
    log::info!("Profile diagnostics: renderer and Look prepared; evaluating pixels");
    #[cfg(debug_assertions)]
    let profile_colour_started = std::time::Instant::now();
    let image = render_working(image, &renderer, rgb_look.as_ref(), cancel)?;
    #[cfg(debug_assertions)]
    log::info!(
        "[preview-stage] stage=profile_colour elapsed={}ns outcome=completed",
        profile_colour_started.elapsed().as_nanos()
    );
    #[cfg(debug_assertions)]
    log::info!("Profile diagnostics: working RGB pixels complete");
    #[cfg(debug_assertions)]
    let profile_orientation_started = std::time::Instant::now();
    let image = crate::image_processing::apply_orientation(image, orientation);
    check_cancel(cancel)?;
    #[cfg(debug_assertions)]
    log::info!(
        "[preview-stage] stage=profile_orientation elapsed={}ns outcome=completed",
        profile_orientation_started.elapsed().as_nanos()
    );
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn synthetic_profile() -> DcpProfile {
        use crate::dcp::model::*;
        DcpProfile {
            id: ProfileId([0; 32]),
            file_path: Default::default(),
            profile_name: "Synthetic base".into(),
            unique_camera_model: "Nikon D850".into(),
            copyright: None,
            embed_policy: EmbedPolicy::NoRestrictions,
            calibration_illuminant_1: Illuminant::D65,
            calibration_illuminant_2: None,
            color_matrix_1: Mat3::identity(),
            color_matrix_2: None,
            forward_matrix_1: None,
            forward_matrix_2: None,
            camera_calibration_1: None,
            camera_calibration_2: None,
            analog_balance: None,
            baseline_exposure_offset: None,
            default_black_render: DefaultBlackRender::Auto,
            hue_sat_map: None,
            look_table: None,
            look_table_encoding: TableEncoding::Linear,
            hue_sat_map_encoding: TableEncoding::Linear,
            tone_curve: None,
        }
    }

    #[test]
    fn actual_selection_migrates_legacy_base_and_normalizes_hex() {
        let id = crate::dcp::model::ProfileId(std::array::from_fn(|i| {
            if i == 0 {
                74
            } else if i == 1 {
                36
            } else {
                i as u8
            }
        }));
        for base in [json!(id.0), json!(id.to_hex().to_uppercase())] {
            let adjustments = json!({"profile":{"base":base,"look":"look-uuid"}});
            let selected = selection(&adjustments).unwrap();
            assert_eq!(selected, (Some(id.to_hex()), Some("look-uuid")));
            assert!(has_selected_profile(&adjustments).unwrap());
        }
        let registry = ProfileRegistry::new();
        let error = render_selected_profile(
            b"not raw",
            &json!({"profile":{"base":id.0}}),
            &registry,
            false,
            2.5,
            "auto".into(),
            None,
        )
        .unwrap_err();
        assert!(
            error.contains(&format!("Selected DCP '{}' is missing", id.to_hex())),
            "{error}"
        );
    }

    #[test]
    fn selection_rejects_invalid_base_arrays_and_array_look_ids() {
        for base in [
            json!(vec![0u8; 31]),
            json!(vec![0u8; 33]),
            json!(vec![256; 32]),
            json!(vec![-1; 32]),
            json!(vec![1.5; 32]),
            json!("invalid"),
            json!(""),
        ] {
            assert!(selection(&json!({"profile":{"base":base}})).is_err());
        }
        assert!(selection(&json!({"profile":{"base":vec![0u8;32],"look":vec![0u8;32]}})).is_err());
    }

    #[test]
    fn working_conversion_is_applied_once_and_preserves_float_range() {
        let renderer = DcpRenderer::new(&synthetic_profile(), [1.0; 3]).unwrap();
        let pixels = [[0.3, 0.1, 0.05], [1.2, 0.8, 0.2]];
        let image = image::Rgb32FImage::from_raw(2, 1, pixels.concat()).unwrap();
        let rendered = render_working(DynamicImage::ImageRgb32F(image), &renderer, None, &None)
            .unwrap()
            .to_rgb32f();
        for (input, actual) in pixels.iter().zip(rendered.pixels()) {
            let prophoto = renderer.render_pixel(*input);
            let expected = renderer.to_working_space(prophoto);
            assert_eq!(actual.0, expected);
            assert!(
                actual
                    .0
                    .iter()
                    .zip(prophoto)
                    .any(|(a, b)| (*a - b).abs() > 0.001)
            );
            let twice = renderer.to_working_space(expected);
            assert!(
                actual
                    .0
                    .iter()
                    .zip(twice)
                    .any(|(a, b)| (*a - b).abs() > 0.001)
            );
        }
        assert!(rendered.as_raw().iter().any(|v| *v > 1.0 || *v < 0.0));
    }

    #[test]
    fn rgb_look_runs_between_dcp_and_single_working_conversion() {
        use crate::dcp::look_xmp::*;
        let mut samples = Vec::new();
        for r in 0u32..3 {
            for g in 0u32..3 {
                for b in 0u32..3 {
                    samples.push(
                        [r, g, b].map(|i| 32768u16.wrapping_sub((i * 65535u32).div_ceil(2) as u16)),
                    );
                }
            }
        }
        let source = LookTableSource::Decoded(CobaltRgbTable {
            divisions: 3,
            samples,
            primaries: LookColorSpace::ProPhoto,
            transfer: LookTransfer::Linear,
            gamut: LookGamut::Clip,
            min_amount: 0.0,
            max_amount: 2.0,
        });
        let look = crate::dcp::rgb_look::RgbLook::new(&source, 100.0, true).unwrap();
        let renderer = DcpRenderer::new(&synthetic_profile(), [1.0; 3]).unwrap();
        let input = image::Rgb32FImage::from_raw(1, 1, vec![0.1, 0.1, 0.1]).unwrap();
        let output = render_working(
            DynamicImage::ImageRgb32F(input),
            &renderer,
            Some(&look),
            &None,
        )
        .unwrap()
        .to_rgb32f();
        let expected = renderer.to_working_space([32768.0 / 65535.0; 3]);
        for (c, expected_channel) in expected.iter().enumerate() {
            assert!((output.get_pixel(0, 0)[c] - expected_channel).abs() < 1e-6);
        }
    }

    #[test]
    fn look_requires_exact_base_and_matching_camera() {
        let profile = synthetic_profile();
        assert!(
            validate_pairing(
                &profile,
                "Synthetic base",
                Some("Nikon D850"),
                "Nikon",
                "D850",
                "D850"
            )
            .is_ok()
        );
        assert!(
            validate_pairing(&profile, "Different base", None, "Nikon", "D850", "D850").is_err()
        );
        assert!(
            validate_pairing(
                &profile,
                "Synthetic base",
                Some("Nikon D750"),
                "Nikon",
                "D850",
                "D850"
            )
            .is_err()
        );
    }

    #[test]
    fn absent_selection_does_not_decode_invalid_raw() {
        let registry = ProfileRegistry::new();
        for adj in [
            json!({}),
            json!({"profile": null}),
            json!({"profile":{"base":null,"look":null}}),
        ] {
            assert!(
                render_selected_profile(
                    b"not raw",
                    &adj,
                    &registry,
                    false,
                    2.5,
                    "auto".into(),
                    None
                )
                .unwrap()
                .is_none()
            );
        }
    }
    #[test]
    fn selected_failures_never_fall_back() {
        let registry = ProfileRegistry::new();
        for adj in [
            json!({"profile":{"base":"missing"}}),
            json!({"profile":{"look":"missing"}}),
            json!({"profile":42}),
            json!({"profile":{"base":false}}),
        ] {
            assert!(
                render_selected_profile(&[], &adj, &registry, false, 2.5, "auto".into(), None)
                    .is_err()
            );
        }
    }
    #[test]
    fn missing_look_returns_specific_error_before_raw_decode() {
        let error = render_selected_profile(
            b"invalid raw",
            &json!({"profile":{"base":"00".repeat(32), "look":"absent"}}),
            &ProfileRegistry::new(),
            false,
            2.5,
            "auto".into(),
            None,
        )
        .unwrap_err();
        assert!(
            error.contains("Selected Look 'absent' is missing"),
            "{error}"
        );
    }

    #[test]
    fn malformed_registered_look_returns_error_not_success() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"<not-a-look>").unwrap();
        let registry = ProfileRegistry::new();
        registry.register_look(crate::dcp::registry::LookMetadata {
            uuid: "broken".into(),
            name: "Broken".into(),
            group: "Synthetic".into(),
            required_base_profile: "Synthetic base".into(),
            camera_model_restriction: None,
            file_path: file.path().to_path_buf(),
        });
        let error = render_selected_profile(
            b"invalid raw",
            &json!({"profile":{"base":"00".repeat(32), "look":"broken"}}),
            &registry,
            false,
            2.5,
            "auto".into(),
            None,
        )
        .unwrap_err();
        assert!(error.contains("Cannot read selected Look"), "{error}");
    }

    #[test]
    fn neutral_uses_rgb_not_rggb() {
        assert_eq!(
            neutral_from_wb([2.0, 1.0, 4.0, f32::NAN]).unwrap(),
            [0.5, 1.0, 0.25]
        );
        for wb in [
            [0.0, 1.0, 1.0, 1.0],
            [1.0, f32::NAN, 1.0, 1.0],
            [1.0, 1.0, -1.0, 1.0],
        ] {
            assert!(neutral_from_wb(wb).is_err());
        }
    }
    #[test]
    fn camera_matching_is_exact_with_aliases() {
        assert!(matches_camera(
            "NIKON D850",
            "NIKON CORPORATION",
            "D850",
            "D850"
        ));
        assert!(!matches_camera("NIKON D850", "Nikon", "D750", "D750"));
        assert!(!matches_camera("", "", "", ""));
    }
    #[test]
    fn cancellation_rejects_obsolete_work() {
        assert!(check_cancel(&Some((Arc::new(AtomicUsize::new(2)), 1))).is_err());
        assert!(check_cancel(&Some((Arc::new(AtomicUsize::new(1)), 1))).is_ok());
    }

    #[test]
    fn prepared_selection_preserves_early_error_order() {
        let registry = ProfileRegistry::new();
        let cancelled = Some((Arc::new(AtomicUsize::new(2)), 1));
        assert!(
            prepare_selected_profile(&json!({}), &registry, &cancelled)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            prepare_selected_profile(&json!({"profile":{"look":"missing"}}), &registry, &None)
                .err()
                .unwrap(),
            "Selected Look requires an explicitly selected base DCP"
        );
        let selected = json!({"profile":{"base":"00".repeat(32),"look":"missing"}});
        assert_eq!(
            prepare_selected_profile(&selected, &registry, &None)
                .err()
                .unwrap(),
            "Selected Look 'missing' is missing from the registry"
        );
        assert_eq!(
            prepare_selected_profile(&selected, &registry, &cancelled)
                .err()
                .unwrap(),
            "Profile rendering cancelled"
        );
    }

    fn prepared_camera_fixture() -> (DynamicImage, crate::raw_processing::RawImageMeta) {
        (
            DynamicImage::ImageRgb32F(
                image::Rgb32FImage::from_raw(2, 1, vec![0.1, 0.1, 0.1, 0.2, 0.2, 0.2]).unwrap(),
            ),
            crate::raw_processing::RawImageMeta {
                wb_coeffs: [1.0, 1.0, 1.0, f32::NAN],
                make: "Nikon".into(),
                model: "D850".into(),
                clean_model: "D850".into(),
            },
        )
    }

    #[test]
    fn owned_profile_snapshot_renders_without_reopening_and_orients_once() {
        let mut profile = synthetic_profile();
        // This deliberately unreadable source name cannot affect a captured model.
        profile.file_path = "captured-profile-do-not-reopen.dcp".into();
        let assets = SelectedProfileAssets {
            profile,
            look: None,
            amount: Some(json!("ignored without Look")),
        };
        let (image, meta) = prepared_camera_fixture();
        let first = render_profile_camera_image(
            image.clone(),
            rawler::decoders::Orientation::Rotate90,
            &meta,
            &assets,
            &None,
        )
        .unwrap();
        let second = render_profile_camera_image(
            image,
            rawler::decoders::Orientation::Rotate90,
            &meta,
            &assets,
            &None,
        )
        .unwrap();
        assert_eq!((first.width(), first.height()), (1, 2));
        assert_eq!(first.to_rgb32f().as_raw(), second.to_rgb32f().as_raw());
        assert_eq!(assets.amount, Some(json!("ignored without Look")));
        let (image, mut wrong) = prepared_camera_fixture();
        wrong.model = "D750".into();
        wrong.clean_model = "D750".into();
        assert!(
            render_profile_camera_image(
                image,
                rawler::decoders::Orientation::Normal,
                &wrong,
                &assets,
                &None
            )
            .unwrap_err()
            .starts_with("Selected DCP is for")
        );
        let (image, mut invalid_wb) = prepared_camera_fixture();
        invalid_wb.wb_coeffs[0] = f32::NAN;
        assert_eq!(
            render_profile_camera_image(
                image,
                rawler::decoders::Orientation::Normal,
                &invalid_wb,
                &assets,
                &None
            )
            .unwrap_err(),
            "DCP requires finite positive RGB white-balance coefficients"
        );
    }

    #[test]
    fn captured_look_keeps_pairing_and_raw_amount_validation() {
        use crate::dcp::look_xmp::*;
        let look = CobaltLook {
            id: "synthetic".into(),
            file_path: "captured-look-do-not-reopen.xmp".into(),
            uuid: "synthetic".into(),
            name: "Synthetic".into(),
            group: String::new(),
            cluster: String::new(),
            short_name: String::new(),
            sort_name: String::new(),
            required_base_profile: "Synthetic base".into(),
            camera_model_restriction: Some("Nikon D850".into()),
            supports_amount: true,
            table_uuid: "synthetic-table".into(),
            table: LookTableSource::Decoded(CobaltRgbTable {
                divisions: 3,
                samples: vec![[32768; 3]; 27],
                primaries: LookColorSpace::ProPhoto,
                transfer: LookTransfer::Linear,
                gamut: LookGamut::Clip,
                min_amount: 0.0,
                max_amount: 2.0,
            }),
            copyright: None,
            process_version: String::new(),
        };
        let mut assets = SelectedProfileAssets {
            profile: synthetic_profile(),
            look: Some(look),
            amount: Some(Value::Null),
        };
        for amount in [Value::Null, json!("100")] {
            assets.amount = Some(amount);
            let (image, meta) = prepared_camera_fixture();
            assert_eq!(
                render_profile_camera_image(
                    image,
                    rawler::decoders::Orientation::Normal,
                    &meta,
                    &assets,
                    &None
                )
                .unwrap_err(),
                "Look amount must be numeric"
            );
        }
        assets.amount = None;
        let (image, meta) = prepared_camera_fixture();
        let implicit = render_profile_camera_image(
            image,
            rawler::decoders::Orientation::Normal,
            &meta,
            &assets,
            &None,
        )
        .unwrap();
        assets.amount = Some(json!(100));
        let (image, meta) = prepared_camera_fixture();
        let explicit = render_profile_camera_image(
            image,
            rawler::decoders::Orientation::Normal,
            &meta,
            &assets,
            &None,
        )
        .unwrap();
        assert_eq!(implicit.to_rgb32f().as_raw(), explicit.to_rgb32f().as_raw());
        assets.look.as_mut().unwrap().required_base_profile = "Different base".into();
        let (image, meta) = prepared_camera_fixture();
        assert!(
            render_profile_camera_image(
                image,
                rawler::decoders::Orientation::Normal,
                &meta,
                &assets,
                &None
            )
            .unwrap_err()
            .starts_with("Selected Look requires base")
        );
    }
}
