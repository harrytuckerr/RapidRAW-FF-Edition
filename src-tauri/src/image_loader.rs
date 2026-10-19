use crate::Cursor;
use crate::app_settings::{AppSettings, load_settings};
use crate::app_state::{AppState, LoadedImage};
use crate::baseline_exposure::BaselineOptions;
use crate::exif_processing;
use crate::file_management::{parse_virtual_path, read_file_mapped};
use crate::formats::{is_raw_file, validate_image_content};
use crate::image_processing::ImageMetadata;
use crate::image_processing::{
    apply_orientation, apply_srgb_to_linear, effective_baseline_exposure,
    remove_raw_artifacts_and_enhance,
};
use crate::mask_generation::{MaskDefinition, SubMask, generate_mask_bitmap};
use crate::white_balance::WhiteBalance;
use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use exif::{Reader as ExifReader, Tag};
use image::{DynamicImage, GenericImageView, ImageReader, imageops};
use rawler::Orientation;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;
use tauri::Manager;

#[derive(serde::Serialize)]
pub struct LoadImageResult {
    pub width: u32,
    pub height: u32,
    pub metadata: ImageMetadata,
    pub exif: HashMap<String, String>,
    pub is_raw: bool,
    pub as_shot_white_balance: WhiteBalance,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchMaskInfo {
    id: String,
    name: String,
    #[serde(default)]
    invert: bool,
    #[serde(default)]
    sub_masks: Vec<SubMask>,
}

fn srgb_to_linear_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0.0f32; 256];
        for (i, v) in lut.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            *v = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
        }
        lut
    })
}

/// Rejects empty content and a JPEG signature under a RAF name before any
/// decoder runs. An already-cancelled request still reports cancellation.
fn check_image_content(
    bytes: &[u8],
    path: &str,
    cancel_token: &Option<(Arc<AtomicUsize>, usize)>,
) -> Result<()> {
    let Err(issue) = validate_image_content(path, bytes) else {
        return Ok(());
    };

    if let Some((tracker, generation)) = cancel_token
        && tracker.load(Ordering::SeqCst) != *generation
    {
        return Err(anyhow!("Load cancelled"));
    }

    Err(anyhow!(issue.open_error_message(path)))
}

pub fn load_and_composite(
    base_image: &[u8],
    path: &str,
    adjustments: &Value,
    use_fast_raw_dev: bool,
    settings: &AppSettings,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let base_image =
        load_base_image_from_bytes(base_image, path, use_fast_raw_dev, settings, cancel_token)?;
    prepare_base_image(&base_image, path, adjustments)
}

pub fn prepare_base_image(
    base_image: &DynamicImage,
    path: &str,
    adjustments: &Value,
) -> Result<DynamicImage> {
    prepare_base_image_with_lease(base_image, path, adjustments, None)
}

fn prepare_base_image_with_lease(
    base_image: &DynamicImage,
    path: &str,
    adjustments: &Value,
    lease: Option<&crate::ai_denoise::DenoiseLease>,
) -> Result<DynamicImage> {
    let has_patches = adjustments
        .get("aiPatches")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());

    match crate::ai_denoise::apply_ai_denoise_with_lease(base_image, path, adjustments, lease)
        .map_err(anyhow::Error::msg)?
    {
        std::borrow::Cow::Owned(denoised) if !has_patches => Ok(denoised),
        denoised => composite_patches_on_image(&denoised, adjustments),
    }
}

/// Profile-aware entry point used by exported and thumbnail images. A selected
/// camera profile always uses the rawler camera-native path, never Apple RAW RGB.
#[allow(clippy::too_many_arguments)]
pub fn load_and_composite_with_profiles(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    fast: bool,
    settings: &AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    with_unpatched_profile_base(
        bytes,
        path,
        adjustments,
        fast,
        settings,
        registry,
        cancel,
        |image| prepare_base_image(image, path, adjustments),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_and_composite_with_profiles_in_worker(
    app: &tauri::AppHandle,
    external_check: &(dyn Fn() -> std::result::Result<(), String> + Sync),
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    fast: bool,
    settings: &AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let check = || {
        external_check()?;
        check_prepared_cancel(&cancel).map_err(|e| e.to_string())
    };
    check().map_err(anyhow::Error::msg)?;
    with_unpatched_profile_base(
        bytes,
        path,
        adjustments,
        fast,
        settings,
        registry,
        cancel.clone(),
        |image| prepare_base_image_in_worker(app, image, path, adjustments, &check),
    )
}

pub(crate) fn prepare_base_image_in_worker(
    app: &tauri::AppHandle,
    image: &DynamicImage,
    path: &str,
    adjustments: &Value,
    check: &(dyn Fn() -> std::result::Result<(), String> + Sync),
) -> Result<DynamicImage> {
    let lease = crate::ai_denoise::ensure_worker_base(app, image, path, adjustments, check)
        .map_err(anyhow::Error::msg)?;
    check().map_err(anyhow::Error::msg)?;
    let result = prepare_base_image_with_lease(image, path, adjustments, lease.as_ref());
    check().map_err(anyhow::Error::msg)?;
    result
}

fn with_unpatched_profile_base(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    fast: bool,
    settings: &AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: Option<(Arc<AtomicUsize>, usize)>,
    work: impl FnOnce(&DynamicImage) -> Result<DynamicImage>,
) -> Result<DynamicImage> {
    check_image_content(bytes, path, &cancel)?;

    if crate::dcp::pipeline::has_selected_profile(adjustments).map_err(anyhow::Error::msg)? {
        if !is_raw_file(path) {
            return Err(anyhow!("Camera profiles require RAW input"));
        }
        if let Some(image) = crate::dcp::pipeline::render_selected_profile(
            bytes,
            adjustments,
            registry,
            fast,
            settings.raw_highlight_compression.unwrap_or(2.5),
            settings.linear_raw_mode.clone(),
            cancel.clone(),
        )
        .map_err(anyhow::Error::msg)?
        {
            return work(&image);
        }
    }
    let image = load_base_image_from_bytes(bytes, path, fast, settings, cancel)?;
    work(&image)
}

/// Explicit execution limits for the inactive accurate source path. No global
/// memory/default policy is selected here. Rawler validates its actual minimum.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AccurateRawPolicy {
    max_workers: usize,
    scratch_budget_bytes: usize,
}

impl AccurateRawPolicy {
    pub(crate) fn new(max_workers: usize, scratch_budget_bytes: usize) -> Result<Self> {
        let never_cancelled = || false;
        rawler::imgop::sensor::xtrans::external::XTransDemosaicOptions::new(
            rawler::imgop::sensor::xtrans::external::XTransPasses::One,
            1.0,
            max_workers,
            scratch_budget_bytes,
            &never_cancelled,
        )
        .map_err(|e| anyhow!("Invalid accurate RAW resource policy: {e:?}"))?;
        Ok(Self {
            max_workers,
            scratch_budget_bytes,
        })
    }

    fn quality(self) -> crate::raw_processing::PreparedQuality {
        crate::raw_processing::PreparedQuality::AccurateXtrans1 {
            max_workers: self.max_workers,
            scratch_budget_bytes: self.scratch_budget_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AppearancePass {
    CurrentFull,
    AccurateOne,
}

/// Source and quality cannot be installed independently. The only constructor
/// retains a genuinely decoded native X-Trans owner, with captured settings.
pub(crate) struct RetainedPreparedSource {
    prepared: Arc<crate::raw_processing::PreparedRaw>,
    policy: AccurateRawPolicy,
    settings: AppSettings,
}

impl RetainedPreparedSource {
    fn quality(&self, pass: AppearancePass) -> crate::raw_processing::PreparedQuality {
        match pass {
            AppearancePass::CurrentFull => crate::raw_processing::PreparedQuality::CurrentFull,
            AppearancePass::AccurateOne => self.policy.quality(),
        }
    }

    pub(crate) fn as_shot(&self) -> crate::as_shot::AsShot {
        self.prepared.as_shot()
    }

    /// Always returns a fresh, oriented, calibrated pristine working base.
    /// Patches and selected profiles are deliberately absent from this image.
    pub(crate) fn render_pristine(
        &self,
        pass: AppearancePass,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<(DynamicImage, crate::raw_processing::RawImageMeta)> {
        record_preview_base_development();
        let (image, orientation, meta) = crate::raw_processing::develop_prepared_raw(
            &self.prepared,
            self.quality(pass),
            self.settings.raw_highlight_compression.unwrap_or(2.5),
            &self.settings.linear_raw_mode,
            false,
            cancel.clone(),
        )
        .inspect_err(|_| {
            crate::progressive_open::log_open_failure("base.pristine_development", "opaque_error")
        })?;
        let image = apply_orientation(image, orientation);
        let mut image = filter_accurate_base(image, pass, &cancel)?;
        RawPreprocessing::from_settings(&self.settings).apply(&mut image, true, false);
        check_prepared_cancel(&cancel)?;
        Ok((image, meta))
    }

    /// A later profile request acquires new immutable assets but reuses the
    /// decoded source. None is returned before development for no selection.
    pub(crate) fn render_selected(
        &self,
        adjustments: &Value,
        registry: &crate::dcp::registry::ProfileRegistry,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<Option<DynamicImage>, String> {
        let Some(assets) =
            crate::dcp::pipeline::prepare_selected_profile(adjustments, registry, &cancel)?
        else {
            return Ok(None);
        };
        self.render_selected_assets(&assets, AppearancePass::AccurateOne, cancel)
            .map(Some)
    }

    /// One selected working-space producer for preview and prepared export.
    /// DCP/Look conversion and orientation occur inside the existing renderer;
    /// ordinary RawPreprocessing remains bypassed.
    fn render_selected_assets(
        &self,
        assets: &crate::dcp::pipeline::SelectedProfileAssets,
        pass: AppearancePass,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<DynamicImage, String> {
        record_preview_base_development();
        let image = crate::dcp::pipeline::render_prepared_selected_profile(
            &self.prepared,
            assets,
            self.quality(pass),
            self.settings.raw_highlight_compression.unwrap_or(2.5),
            &self.settings.linear_raw_mode,
            cancel.clone(),
        )
        .inspect_err(|_| {
            crate::progressive_open::log_open_failure("base.selected_development", "opaque_error")
        })?;
        #[cfg(debug_assertions)]
        let profile_fcs_started = std::time::Instant::now();
        let result = filter_accurate_base(image, pass, &cancel).map_err(|e| e.to_string());
        #[cfg(debug_assertions)]
        if matches!(pass, AppearancePass::AccurateOne) && result.is_ok() {
            log::info!(
                "[preview-stage] stage=profile_fcs elapsed={}ns outcome=completed",
                profile_fcs_started.elapsed().as_nanos()
            );
        }
        result
    }
}

struct AppearanceInputs {
    adjustments: Value,
    settings: AppSettings,
}

impl AppearanceInputs {
    fn capture(adjustments: &Value, settings: &AppSettings) -> Self {
        Self {
            adjustments: adjustments.clone(),
            settings: settings.clone(),
        }
    }
}

pub(crate) struct PreparedOpenAppearance {
    source: Arc<RetainedPreparedSource>,
    inputs: AppearanceInputs,
    assets: Option<crate::dcp::pipeline::SelectedProfileAssets>,
}

impl PreparedOpenAppearance {
    pub(crate) fn retained_source(&self) -> Arc<RetainedPreparedSource> {
        Arc::clone(&self.source)
    }

    pub(crate) fn as_shot(&self) -> crate::as_shot::AsShot {
        self.source.as_shot()
    }

    pub(crate) fn adjustments(&self) -> &Value {
        &self.inputs.adjustments
    }

    pub(crate) fn settings(&self) -> &AppSettings {
        &self.inputs.settings
    }

    /// Both passes borrow the very same source and parsed asset snapshot.
    /// Selected DCP bypasses normal preprocessing, as in the byte wrapper.
    pub(crate) fn render_appearance(
        &self,
        pass: AppearancePass,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<DynamicImage> {
        let base = self.render_unpatched_base(pass, cancel.clone())?;
        composite_prepared_base(base.pixels(), &self.inputs.adjustments, &cancel)
    }

    /// Always develops a fresh working base from this source/asset owner.
    /// No AI patch, creative effect or cached appearance is accepted as input.
    pub(crate) fn render_unpatched_base(
        &self,
        pass: AppearancePass,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<PreparedAppearanceBase<'_>> {
        check_prepared_cancel(&cancel)?;
        let image = if let Some(assets) = &self.assets {
            self.source
                .render_selected_assets(assets, pass, cancel.clone())
                .map_err(anyhow::Error::msg)?
        } else {
            self.source.render_pristine(pass, cancel.clone())?.0
        };
        check_prepared_cancel(&cancel)?;
        Ok(PreparedAppearanceBase {
            image: std::borrow::Cow::Owned(image),
            source: Arc::clone(&self.source),
            as_shot: self.as_shot(),
            kind: PreparedBaseKind::NativeXtransWorking(pass),
            _appearance: self,
        })
    }
}

// The exact historical patch/check sequence, shared with the captured producer.
fn composite_prepared_base(
    base: &DynamicImage,
    adjustments: &Value,
    cancel: &crate::raw_processing::RawCancel,
) -> Result<DynamicImage> {
    check_prepared_cancel(cancel)?;
    let image = composite_patches_on_image(base, adjustments).inspect_err(|_| {
        crate::progressive_open::log_open_failure("base.patch_composition", "opaque_error")
    })?;
    check_prepared_cancel(cancel)?;
    Ok(image)
}

/// Only the genuine retained-source producer above constructs this value.
/// Kind attests this producer route, not caller admission/publication authority.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PreparedBaseKind {
    NativeXtransWorking(AppearancePass),
}

pub(crate) struct PreparedAppearanceBase<'a> {
    image: std::borrow::Cow<'a, DynamicImage>,
    source: Arc<RetainedPreparedSource>,
    as_shot: crate::as_shot::AsShot,
    kind: PreparedBaseKind,
    _appearance: &'a PreparedOpenAppearance,
}
impl PreparedAppearanceBase<'_> {
    pub(crate) fn pixels(&self) -> &DynamicImage {
        &self.image
    }
    pub(crate) fn as_shot(&self) -> crate::as_shot::AsShot {
        self.as_shot
    }
    pub(crate) fn kind(&self) -> PreparedBaseKind {
        self.kind
    }
    pub(crate) fn retained_source(&self) -> &Arc<RetainedPreparedSource> {
        &self.source
    }
}

#[cfg(test)]
thread_local! {
    static ACCURATE_FILTER_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn filter_accurate_base(
    image: DynamicImage,
    pass: AppearancePass,
    cancel: &crate::raw_processing::RawCancel,
) -> Result<DynamicImage> {
    check_prepared_cancel(cancel)?;
    let image = match pass {
        AppearancePass::CurrentFull => image,
        AppearancePass::AccurateOne => {
            #[cfg(test)]
            ACCURATE_FILTER_CALLS.with(|count| count.set(count.get() + 1));
            crate::false_colour::suppress_linear_srgb_v1(image, cancel).inspect_err(|_| {
                crate::progressive_open::log_open_failure("base.fcs", "opaque_error")
            })?
        }
    };
    check_prepared_cancel(cancel)?;
    Ok(image)
}

#[cfg(test)]
pub(crate) fn composition_test_owner() -> PreparedOpenAppearance {
    prepared_appearance_tests::composition_owner()
}

// BEGIN B2-COVERAGE-TEST-BRIDGE
#[cfg(test)]
pub(crate) fn composition_patterned_test_owner() -> PreparedOpenAppearance {
    prepared_appearance_tests::patterned_composition_owner()
}
// END B2-COVERAGE-TEST-BRIDGE

/// Retains a successfully decoded noneligible source rather than classifying
/// it, discarding it, and forcing a second decode. No accurate claim is made.
pub(crate) struct DecodedLegacyAppearance {
    prepared: Arc<crate::raw_processing::PreparedRaw>,
    inputs: AppearanceInputs,
    assets: Option<crate::dcp::pipeline::SelectedProfileAssets>,
}

impl DecodedLegacyAppearance {
    /// Explicit legacy hydration after the genuine decode/route decision. Reuse
    /// the decoded owner; this does not grant accurate provenance or reopen RAW.
    pub(crate) fn render_current_with_adjustments(
        mut self,
        adjustments: Value,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<DynamicImage> {
        self.inputs.adjustments = adjustments;
        self.render_current(cancel)
    }

    pub(crate) fn as_shot(&self) -> crate::as_shot::AsShot {
        self.prepared.as_shot()
    }

    pub(crate) fn render_current(
        self,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<DynamicImage> {
        let (base, adjustments) = self.render_current_base(cancel.clone())?;
        let image = composite_patches_on_image(&base, &adjustments)?;
        check_prepared_cancel(&cancel)?;
        Ok(image)
    }

    pub(crate) fn render_export_with_adjustments(
        mut self,
        app: &tauri::AppHandle,
        path: &str,
        adjustments: Value,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<DynamicImage> {
        self.inputs.adjustments = adjustments;
        let (base, adjustments) = self.render_current_base(cancel.clone())?;
        let image = prepare_base_image_in_worker(app, &base, path, &adjustments, &|| {
            check_prepared_cancel(&cancel).map_err(|e| e.to_string())
        })?;
        check_prepared_cancel(&cancel)?;
        Ok(image)
    }

    fn render_current_base(
        self,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<(DynamicImage, Value)> {
        check_prepared_cancel(&cancel)?;
        let base = if let Some(assets) = &self.assets {
            crate::dcp::pipeline::render_prepared_selected_profile(
                &self.prepared,
                assets,
                crate::raw_processing::PreparedQuality::CurrentFull,
                self.inputs
                    .settings
                    .raw_highlight_compression
                    .unwrap_or(2.5),
                &self.inputs.settings.linear_raw_mode,
                cancel.clone(),
            )
            .map_err(anyhow::Error::msg)?
        } else {
            let (image, orientation, _) = crate::raw_processing::develop_prepared_raw(
                &self.prepared,
                crate::raw_processing::PreparedQuality::CurrentFull,
                self.inputs
                    .settings
                    .raw_highlight_compression
                    .unwrap_or(2.5),
                &self.inputs.settings.linear_raw_mode,
                false,
                cancel.clone(),
            )?;
            let mut image = apply_orientation(image, orientation);
            RawPreprocessing::from_settings(&self.inputs.settings).apply(&mut image, true, false);
            image
        };
        check_prepared_cancel(&cancel)?;
        Ok((base, self.inputs.adjustments))
    }
    /// Match the old independent estimate's NORMAL fast base: no selected
    /// profile, patch or accurate suppression stage. Consume the actual decode.
    pub(crate) fn render_estimate_fast(
        self,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<(DynamicImage, f32)> {
        check_prepared_cancel(&cancel)?;
        let (image, orientation, _) = crate::raw_processing::develop_prepared_raw(
            &self.prepared,
            crate::raw_processing::PreparedQuality::LegacyFast,
            self.inputs
                .settings
                .raw_highlight_compression
                .unwrap_or(2.5),
            &self.inputs.settings.linear_raw_mode,
            false,
            cancel.clone(),
        )?;
        let mut image = apply_orientation(image, orientation);
        RawPreprocessing::from_settings(&self.inputs.settings).apply(&mut image, true, true);
        check_prepared_cancel(&cancel)?;
        let scale = self
            .prepared
            .fast_demosaic_scale_factor(image.width(), image.height());
        Ok((image, scale))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyPreparedReason {
    NonRaw,
    AppleRawPreferred,
}

pub(crate) enum PreparedAppearanceDecision {
    Prepared(PreparedOpenAppearance),
    /// A successful decoded non-XTrans source, never an accurate result.
    DecodedLegacy(DecodedLegacyAppearance),
    /// Explicit route decision, not an accurate result or error fallback.
    /// The existing byte caller remains responsible for its legacy route.
    LegacyRequired(LegacyPreparedReason),
}

fn check_prepared_cancel(cancel: &crate::raw_processing::RawCancel) -> Result<()> {
    if let Some((tracker, generation)) = cancel
        && tracker.load(Ordering::SeqCst) != *generation
    {
        return Err(anyhow!("Load cancelled"));
    }
    Ok(())
}

/// Inactive, synchronous factory. It never reads a path or dummy-decodes.
/// Errors before known eligibility propagate; they are not LegacyRequired.
pub(crate) fn prepare_open_appearance(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    settings: &AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    policy: AccurateRawPolicy,
    cancel: crate::raw_processing::RawCancel,
) -> Result<PreparedAppearanceDecision> {
    check_prepared_cancel(&cancel)?;
    let inputs = AppearanceInputs::capture(adjustments, settings);
    let assets =
        crate::dcp::pipeline::prepare_selected_profile(&inputs.adjustments, registry, &cancel)
            .inspect_err(|_| {
                crate::progressive_open::log_open_failure("factory.profile_assets", "opaque_error")
            })
            .map_err(anyhow::Error::msg)?;
    if !is_raw_file(path) {
        if assets.is_some() {
            return Err(anyhow!("Camera profiles require RAW input"));
        }
        return Ok(PreparedAppearanceDecision::LegacyRequired(
            LegacyPreparedReason::NonRaw,
        ));
    }
    if assets.is_none() && inputs.settings.use_apple_raw9.unwrap_or(false) {
        return Ok(PreparedAppearanceDecision::LegacyRequired(
            LegacyPreparedReason::AppleRawPreferred,
        ));
    }
    let prepared = crate::raw_processing::prepare_raw(bytes, &cancel).inspect_err(|_| {
        crate::progressive_open::log_open_failure("factory.raw_prepare", "opaque_error")
    })?;
    if !prepared.is_native_xtrans() {
        return Ok(PreparedAppearanceDecision::DecodedLegacy(
            DecodedLegacyAppearance {
                prepared: Arc::new(prepared),
                inputs,
                assets,
            },
        ));
    }
    check_prepared_cancel(&cancel)?;
    let source = Arc::new(RetainedPreparedSource {
        prepared: Arc::new(prepared),
        policy,
        settings: inputs.settings.clone(),
    });
    Ok(PreparedAppearanceDecision::Prepared(
        PreparedOpenAppearance {
            source,
            inputs,
            assets,
        },
    ))
}

/// Preserve legacy independent-estimate classification and unpatched fast base.
/// This factory never prepares profiles before the actual decode/eligibility.
pub(crate) fn prepare_estimate_appearance(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    settings: &AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    policy: AccurateRawPolicy,
    cancel: crate::raw_processing::RawCancel,
) -> Result<PreparedAppearanceDecision> {
    check_prepared_cancel(&cancel)?;
    if !is_raw_file(path) {
        return Ok(PreparedAppearanceDecision::LegacyRequired(
            LegacyPreparedReason::NonRaw,
        ));
    }
    let result = crate::raw_processing::prepare_raw(bytes, &cancel);
    check_prepared_cancel(&cancel)?;
    let prepared = result?;
    let inputs = AppearanceInputs::capture(adjustments, settings);
    if !prepared.is_native_xtrans() || settings.use_apple_raw9.unwrap_or(false) {
        return Ok(PreparedAppearanceDecision::DecodedLegacy(
            DecodedLegacyAppearance {
                prepared: Arc::new(prepared),
                inputs,
                assets: None,
            },
        ));
    }
    let result = crate::dcp::pipeline::prepare_selected_profile(adjustments, registry, &cancel);
    check_prepared_cancel(&cancel)?;
    let assets = result.map_err(anyhow::Error::msg)?;
    let source = Arc::new(RetainedPreparedSource {
        prepared: Arc::new(prepared),
        policy,
        settings: settings.clone(),
    });
    Ok(PreparedAppearanceDecision::Prepared(
        PreparedOpenAppearance {
            source,
            inputs,
            assets,
        },
    ))
}

#[derive(Clone, Copy, Debug)]
struct RawPreprocessing {
    color_nr_amount: f32,
    sharpening_amount: f32,
    apply_to_non_raws: bool,
}

impl RawPreprocessing {
    fn from_settings(settings: &AppSettings) -> Self {
        let color_nr_setting = settings.raw_preprocessing_color_nr.unwrap_or(0.5);
        let color_nr_amount = if color_nr_setting <= 0.0 {
            0.0
        } else {
            let x = color_nr_setting.clamp(0.01, 1.0);
            (12.0 / x - 10.0).max(0.1)
        };
        let sharpening_amount = settings.raw_preprocessing_sharpening.unwrap_or(0.35);
        let apply_to_non_raws = settings.apply_preprocessing_to_non_raws.unwrap_or(false);
        Self {
            color_nr_amount,
            sharpening_amount,
            apply_to_non_raws,
        }
    }

    fn should_apply(self, is_raw: bool, fast: bool) -> bool {
        !fast
            && (is_raw || self.apply_to_non_raws)
            && (self.color_nr_amount > 0.0 || self.sharpening_amount > 0.0)
    }

    fn apply(self, image: &mut DynamicImage, is_raw: bool, fast: bool) {
        if self.should_apply(is_raw, fast) {
            remove_raw_artifacts_and_enhance(image, self.color_nr_amount, self.sharpening_amount);
        }
    }
}

pub fn load_base_image_from_bytes(
    bytes: &[u8],
    path_for_ext_check: &str,
    use_fast_raw_dev: bool,
    settings: &AppSettings,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    check_image_content(bytes, path_for_ext_check, &cancel_token)?;

    let highlight_compression = settings.raw_highlight_compression.unwrap_or(2.5);
    let linear_mode = settings.linear_raw_mode.clone();
    let preprocessing = RawPreprocessing::from_settings(settings);

    crate::exif_processing::persist_exif_if_missing(
        Path::new(path_for_ext_check),
        path_for_ext_check,
        bytes,
    );

    if is_raw_file(path_for_ext_check)
        && !use_fast_raw_dev
        && settings.use_apple_raw9.unwrap_or(false)
    {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }

        match crate::apple_raw::develop_raw9(
            bytes,
            path_for_ext_check,
            &crate::apple_raw::Raw9Options::for_loading(),
        ) {
            Ok(image) => return Ok(image),
            Err(e) => log::warn!(
                "Apple RAW 9 unavailable for '{}', falling back to rawler: {}",
                path_for_ext_check,
                e
            ),
        }
    }

    if is_raw_file(path_for_ext_check) {
        match panic::catch_unwind(move || {
            crate::raw_processing::develop_raw_image(
                bytes,
                use_fast_raw_dev,
                highlight_compression,
                linear_mode,
                cancel_token,
            )
        }) {
            Ok(Ok(mut image)) => {
                if preprocessing.should_apply(true, use_fast_raw_dev) {
                    let start = Instant::now();
                    preprocessing.apply(&mut image, true, use_fast_raw_dev);
                    let duration = start.elapsed();
                    log::info!(
                        "Raw enhancing for '{}' took {:?}",
                        path_for_ext_check,
                        duration
                    );
                }
                Ok(image)
            }
            Ok(Err(e)) => {
                let classified = classify_raw_develop_error(path_for_ext_check, e);

                if classified.to_string().contains("Load cancelled") {
                    return Err(classified);
                }

                log::warn!(
                    "Error developing RAW file '{}': {}",
                    path_for_ext_check,
                    classified
                );
                if let Some(preview) = safe_embedded_preview_fallback(bytes, path_for_ext_check) {
                    log::warn!(
                        "Using embedded preview fallback for '{}' ({}x{})",
                        path_for_ext_check,
                        preview.width(),
                        preview.height()
                    );

                    return Ok(linearize_embedded_preview(preview));
                }
                Err(classified)
            }
            Err(_) => {
                log::error!("Panic while processing RAW file: {}", path_for_ext_check);
                if let Some(preview) = safe_embedded_preview_fallback(bytes, path_for_ext_check) {
                    log::warn!(
                        "Using embedded preview fallback for '{}' after RAW decoder panic ({}x{})",
                        path_for_ext_check,
                        preview.width(),
                        preview.height()
                    );

                    return Ok(linearize_embedded_preview(preview));
                }
                Err(anyhow!(
                    "Failed to process RAW file: {}",
                    path_for_ext_check
                ))
            }
        }
    } else {
        let mut image = load_image_with_orientation(bytes, cancel_token)?;

        if preprocessing.should_apply(false, use_fast_raw_dev) {
            let start = Instant::now();
            preprocessing.apply(&mut image, false, use_fast_raw_dev);
            let duration = start.elapsed();
            log::info!(
                "Enhancing non-RAW '{}' took {:?}",
                path_for_ext_check,
                duration
            );
        }

        Ok(image)
    }
}

fn classify_raw_develop_error(path: &str, err: anyhow::Error) -> anyhow::Error {
    let error_text = err.to_string();
    let lowered = error_text.to_ascii_lowercase();
    let unsupported_compression =
        lowered.contains("nef compression") && lowered.contains("not supported");

    if unsupported_compression {
        return anyhow!(
            "Unsupported RAW compression format for '{}'. Original error: {}",
            path,
            error_text
        );
    }

    err
}

fn largest_tiff_jpeg_preview(buf: &[u8]) -> Option<DynamicImage> {
    let le = match buf.get(..4)? {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };
    let rd16 = |o: usize| -> Option<u64> {
        let b: [u8; 2] = buf.get(o..o + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        } as u64)
    };
    let rd32 = |o: usize| -> Option<u64> {
        let b: [u8; 4] = buf.get(o..o + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        } as u64)
    };

    let mut candidates: Vec<(u64, u64)> = Vec::new();
    let mut queue: Vec<u64> = vec![rd32(4)?];
    let mut seen = HashMap::new();

    while let Some(ifd) = queue.pop() {
        if seen.insert(ifd, ()).is_some() || seen.len() > 64 {
            continue;
        }
        let Some(n) = rd16(ifd as usize) else {
            continue;
        };

        let mut compression: u64 = 0;
        let mut strip: Option<(u64, u64)> = None;
        let mut old_jpeg: Option<(u64, u64)> = None;

        for i in 0..n {
            let e = ifd as usize + 2 + (i as usize) * 12;
            let (Some(tag), Some(count), Some(val)) = (rd16(e), rd32(e + 4), rd32(e + 8)) else {
                continue;
            };
            match tag {
                259 => compression = val,
                273 if count == 1 => strip = Some((val, strip.map_or(0, |s| s.1))),
                279 if count == 1 => strip = strip.map(|s| (s.0, val)).or(Some((0, val))),
                513 => old_jpeg = Some((val, old_jpeg.map_or(0, |s| s.1))),
                514 => old_jpeg = old_jpeg.map(|s| (s.0, val)).or(Some((0, val))),
                330 => {
                    if count == 1 {
                        queue.push(val);
                    } else {
                        for j in 0..count.min(8) {
                            if let Some(p) = rd32(val as usize + (j as usize) * 4) {
                                queue.push(p);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if matches!(compression, 6 | 7)
            && let Some(s) = strip
        {
            candidates.push(s);
        }
        if let Some(oj) = old_jpeg {
            candidates.push(oj);
        }
        if let Some(next) = rd32(ifd as usize + 2 + (n as usize) * 12)
            && next != 0
        {
            queue.push(next);
        }
    }

    candidates.sort_by_key(|&(_, len)| std::cmp::Reverse(len));

    for (off, len) in candidates {
        if let Some(bytes) = buf.get(off as usize..(off + len) as usize)
            && let Ok(img) = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        {
            return Some(img);
        }
    }

    None
}

fn embedded_preview_fallback(bytes: &[u8]) -> Option<DynamicImage> {
    let Some(img) = largest_tiff_jpeg_preview(bytes) else {
        return crate::raw_processing::extract_embedded_preview(bytes);
    };

    let orientation = ExifReader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|exif| {
            exif.get_field(Tag::Orientation, exif::In::PRIMARY)?
                .value
                .get_uint(0)
        });

    Some(match orientation {
        Some(o) if o > 1 => apply_orientation(img, Orientation::from_u16(o as u16)),
        _ => img,
    })
}

pub fn safe_embedded_preview_fallback(bytes: &[u8], path: &str) -> Option<DynamicImage> {
    match panic::catch_unwind(panic::AssertUnwindSafe(|| embedded_preview_fallback(bytes))) {
        Ok(preview) => preview,
        Err(_) => {
            log::warn!("Embedded RAW preview extraction panicked for '{}'", path);
            None
        }
    }
}

fn linearize_embedded_preview(preview: DynamicImage) -> DynamicImage {
    let preview = DynamicImage::ImageRgb32F(preview.to_rgb32f());
    let mut linear_preview = apply_srgb_to_linear(preview).into_rgb32f();
    for pixel in linear_preview.pixels_mut() {
        pixel[0] *= 0.4;
        pixel[1] *= 0.4;
        pixel[2] *= 0.4;
    }
    DynamicImage::ImageRgb32F(linear_preview)
}

pub fn load_image_with_orientation(
    bytes: &[u8],
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let check_cancel = || -> Result<()> {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }
        Ok(())
    };

    let cursor = Cursor::new(bytes);
    let mut reader = ImageReader::new(cursor.clone())
        .with_guessed_format()
        .context("Failed to guess image format")?;

    reader.no_limits();

    check_cancel()?;

    let image = reader.decode().context("Failed to decode image")?;
    check_cancel()?;

    let oriented_image = {
        let exif_reader = ExifReader::new();
        if let Ok(exif) = exif_reader.read_from_container(&mut cursor.clone()) {
            if let Some(orientation) = exif
                .get_field(Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
            {
                check_cancel()?;
                apply_orientation(image, Orientation::from_u16(orientation as u16))
            } else {
                image
            }
        } else {
            image
        }
    };

    Ok(DynamicImage::ImageRgb32F(oriented_image.to_rgb32f()))
}

pub fn composite_patches_on_image(
    base_image: &DynamicImage,
    current_adjustments: &Value,
) -> Result<DynamicImage> {
    let patches_val = match current_adjustments.get("aiPatches") {
        Some(val) => val,
        None => return Ok(base_image.clone()),
    };

    let patches_arr = match patches_val.as_array() {
        Some(arr) if !arr.is_empty() => arr,
        _ => return Ok(base_image.clone()),
    };

    let visible_patches: Vec<&Value> = patches_arr
        .par_iter()
        .filter(|patch_obj| {
            let is_visible = patch_obj
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !is_visible {
                return false;
            }
            patch_obj
                .get("patchData")
                .and_then(|data| data.get("color"))
                .and_then(|color| color.as_str())
                .is_some_and(|s| !s.is_empty())
        })
        .collect();

    if visible_patches.is_empty() {
        return Ok(base_image.clone());
    }

    let (base_w, base_h) = base_image.dimensions();

    struct DecodedPatch {
        offset_x: Option<u32>,
        offset_y: Option<u32>,
        mask: image::GrayImage,
        color: image::RgbImage,
        is_srgb_encoded: bool,
    }

    let decoded_patches: Result<Vec<DecodedPatch>> = visible_patches
        .par_iter()
        .map(|patch_obj| {
            let patch_data = patch_obj.get("patchData").context("Missing patchData")?;
            let offset_x = patch_data
                .get("offsetX")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let offset_y = patch_data
                .get("offsetY")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let is_cropped = offset_x.is_some() && offset_y.is_some();

            let is_srgb_encoded = patch_data
                .get("isSrgbEncoded")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let mask_bitmap = if let Some(mask_b64) = patch_data
                .get("mask")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                let mask_bytes = general_purpose::STANDARD.decode(mask_b64)?;
                let mask_img = image::load_from_memory(&mask_bytes)?.to_luma8();
                if !is_cropped && (mask_img.width() != base_w || mask_img.height() != base_h) {
                    imageops::resize(&mask_img, base_w, base_h, imageops::FilterType::Lanczos3)
                } else {
                    mask_img
                }
            } else {
                let patch_info: PatchMaskInfo = serde_json::from_value((*patch_obj).clone())
                    .context("Failed to deserialize patch info for mask generation")?;

                let mask_def = MaskDefinition {
                    id: patch_info.id,
                    name: patch_info.name,
                    visible: true,
                    invert: patch_info.invert,
                    opacity: 100.0,
                    adjustments: Value::Null,
                    sub_masks: patch_info.sub_masks,
                };

                let orientation_steps = current_adjustments
                    .get("orientationSteps")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                let (trans_w, trans_h) = if orientation_steps % 2 == 1 {
                    (base_h, base_w)
                } else {
                    (base_w, base_h)
                };

                let mut gen_mask =
                    generate_mask_bitmap(&mask_def, trans_w, trans_h, 1.0, (0.0, 0.0), None)
                        .context("Failed to generate mask from sub_masks for compositing")?;

                gen_mask =
                    crate::image_processing::inverse_transform_mask(gen_mask, current_adjustments);

                if let (Some(ox), Some(oy)) = (offset_x, offset_y) {
                    let w = patch_data
                        .get("width")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(base_w);
                    let h = patch_data
                        .get("height")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(base_h);
                    let crop_w = w.min(base_w.saturating_sub(ox));
                    let crop_h = h.min(base_h.saturating_sub(oy));
                    gen_mask = imageops::crop_imm(&gen_mask, ox, oy, crop_w, crop_h).to_image();
                }
                gen_mask
            };

            let color_b64 = patch_data
                .get("color")
                .and_then(|v| v.as_str())
                .context("Missing color data")?;
            let color_bytes = general_purpose::STANDARD.decode(color_b64)?;
            let color_image_u8 = image::load_from_memory(&color_bytes)?.to_rgb8();

            let (patch_w, patch_h) = color_image_u8.dimensions();
            let final_color = if !is_cropped && (base_w != patch_w || base_h != patch_h) {
                imageops::resize(
                    &color_image_u8,
                    base_w,
                    base_h,
                    imageops::FilterType::Lanczos3,
                )
            } else {
                color_image_u8
            };

            // Reject inconsistent decoded geometry before any mask-derived color index.
            if final_color.dimensions() != mask_bitmap.dimensions() {
                return Err(anyhow!(
                    "Decoded patch color/mask dimensions differ: color {:?}, mask {:?}",
                    final_color.dimensions(),
                    mask_bitmap.dimensions()
                ));
            }

            Ok(DecodedPatch {
                offset_x,
                offset_y,
                mask: mask_bitmap,
                color: final_color,
                is_srgb_encoded,
            })
        })
        .collect();

    let decoded_patches = decoded_patches?;

    let mut composited_image = base_image.clone();
    let lut = srgb_to_linear_lut();

    let get_color = |patch: &DecodedPatch, r: u8, g: u8, b: u8| -> (f32, f32, f32) {
        if patch.is_srgb_encoded {
            (lut[r as usize], lut[g as usize], lut[b as usize])
        } else {
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
    };

    match &mut composited_image {
        DynamicImage::ImageRgb32F(img_buf) => {
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    img_buf
                        .par_chunks_mut(base_w_usize * 3)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);

                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 3;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    img_buf
                        .par_chunks_mut((base_w * 3) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);

                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 3] = pr * alpha + row[x * 3] * one_minus_alpha;
                                    row[x * 3 + 1] = pg * alpha + row[x * 3 + 1] * one_minus_alpha;
                                    row[x * 3 + 2] = pb * alpha + row[x * 3 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
        }
        DynamicImage::ImageRgba32F(img_buf) => {
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    img_buf
                        .par_chunks_mut(base_w_usize * 4)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 4;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    img_buf
                        .par_chunks_mut((base_w * 4) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);

                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 4] = pr * alpha + row[x * 4] * one_minus_alpha;
                                    row[x * 4 + 1] = pg * alpha + row[x * 4 + 1] * one_minus_alpha;
                                    row[x * 4 + 2] = pb * alpha + row[x * 4 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
        }
        _ => {
            let mut rgba32_img = composited_image.to_rgba32f();
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    rgba32_img
                        .par_chunks_mut(base_w_usize * 4)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 4;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    rgba32_img
                        .par_chunks_mut((base_w * 4) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 4] = pr * alpha + row[x * 4] * one_minus_alpha;
                                    row[x * 4 + 1] = pg * alpha + row[x * 4 + 1] * one_minus_alpha;
                                    row[x * 4 + 2] = pb * alpha + row[x * 4 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
            composited_image = DynamicImage::ImageRgba32F(rgba32_img);
        }
    }

    Ok(composited_image)
}

#[tauri::command]
pub fn is_image_cached(path: String, state: tauri::State<'_, AppState>) -> bool {
    let (source_path, _) = parse_virtual_path(&path);
    let source_path_str = source_path.to_string_lossy().to_string();
    state
        .decoded_image_cache
        .lock()
        .unwrap()
        .get(&source_path_str)
        .is_some()
}

// Admission and publication use the same image mutex, so a late legacy
// decoder cannot overwrite a newer source. Retire prior pixels outside it.
fn begin_original_image_load_checked<T>(
    generation: &AtomicUsize,
    original: &std::sync::Mutex<Option<T>>,
) -> Result<usize, String> {
    let mut slot = original.lock().map_err(|_| "Original image poisoned")?;
    let next = generation
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
        .map_err(|_| "Load generation exhausted")?
        + 1;
    let retired = slot.take();
    drop(slot);
    drop(retired);
    Ok(next)
}

// Keep existing regression calls routed through the production checked helper.
#[cfg(test)]
fn begin_original_image_load<T>(
    generation: &AtomicUsize,
    original: &std::sync::Mutex<Option<T>>,
) -> usize {
    begin_original_image_load_checked(generation, original).unwrap()
}

fn publish_current_original_image<T>(
    generation: &AtomicUsize,
    original: &std::sync::Mutex<Option<T>>,
    expected_generation: usize,
    image: T,
) -> Result<(), String> {
    let mut slot = original.lock().unwrap();
    if generation.load(Ordering::SeqCst) != expected_generation {
        drop(slot);
        return Err("Load cancelled".into());
    }
    let retired = slot.replace(image);
    drop(slot);
    drop(retired);
    Ok(())
}

pub async fn load_image(
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<LoadImageResult, String> {
    // A missing OpenKey cannot mutate a keyed reservation/source. This short
    // admission lease covers only invalidation; release it before draining.
    let admission = state
        .progressive_open
        .legacy_render_lease()
        .map_err(|e| e.to_string())?;
    let my_generation =
        begin_original_image_load_checked(&state.load_image_generation, &state.original_image)?;
    drop(admission);
    state
        .progressive_open
        .wait_legacy_drain()
        .await
        .map_err(|e| e.to_string())?;
    let lease = Arc::new(
        state
            .progressive_open
            .legacy_render_lease()
            .map_err(|e| e.to_string())?,
    );
    crate::gpu_processing::retire_native_presentation(&state).map_err(|e| {
        state.progressive_open.close();
        e
    })?;
    let generation_tracker = state.load_image_generation.clone();
    let cancel_token = Some((generation_tracker.clone(), my_generation));

    let retired = state.clear_native_source().map_err(|e| e.to_string())?;
    retired.retire();
    lease.check().map_err(|e| e.to_string())?;

    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let source_path_str = source_path.to_string_lossy().to_string();

    let metadata: ImageMetadata = crate::exif_processing::load_sidecar(&sidecar_path);

    let mut settings = load_settings(app_handle.clone()).unwrap_or_default();
    if crate::dcp::pipeline::has_selected_profile(&metadata.adjustments)? {
        settings.use_apple_raw9 = Some(false);
    }
    let baseline_options = BaselineOptions::from_settings(&settings);
    let raw_linear_mode = settings.linear_raw_mode.clone();
    let raw_highlight_compression = settings.raw_highlight_compression.unwrap_or(2.5);

    let path_clone = source_path_str.clone();

    let cached_data = state
        .decoded_image_cache
        .lock()
        .unwrap()
        .get(&source_path_str);

    let (pristine_arc, exif_data) = if let Some((cached_img, cached_exif)) = cached_data {
        (cached_img, cached_exif)
    } else {
        if crate::file_management::is_cloud_placeholder(&source_path) {
            return Err(format!(
                "'{}' is stored in iCloud and hasn't been downloaded yet. Download it in Finder, then try again.",
                source_path_str
            ));
        }

        let worker_lease = Arc::clone(&lease);
        let (pristine_img, exif_data_loaded) = tokio::task::spawn_blocking(move || {
            worker_lease.check().map_err(|e| e.to_string())?;
            if generation_tracker.load(Ordering::SeqCst) != my_generation {
                return Err("Load cancelled".to_string());
            }

            let result: Result<(DynamicImage, HashMap<String, String>), String> =
                (|| match read_file_mapped(Path::new(&path_clone)) {
                    Ok(mmap) => {
                        if generation_tracker.load(Ordering::SeqCst) != my_generation {
                            return Err("Load cancelled".to_string());
                        }

                        let img = load_base_image_from_bytes(
                            &mmap,
                            &path_clone,
                            false,
                            &settings,
                            cancel_token.clone(),
                        )
                        .map_err(|e| e.to_string())?;
                        let exif = exif_processing::read_exif_data(&path_clone, &mmap);
                        Ok((img, exif))
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to memory-map file '{}': {}. Falling back to standard read.",
                            path_clone,
                            e
                        );
                        let bytes = fs::read(&path_clone).map_err(|io_err| {
                            format!("Fallback read failed for {}: {}", path_clone, io_err)
                        })?;

                        if generation_tracker.load(Ordering::SeqCst) != my_generation {
                            return Err("Load cancelled".to_string());
                        }

                        let img = load_base_image_from_bytes(
                            &bytes,
                            &path_clone,
                            false,
                            &settings,
                            cancel_token.clone(),
                        )
                        .map_err(|e| e.to_string())?;
                        let exif = exif_processing::read_exif_data(&path_clone, &bytes);
                        Ok((img, exif))
                    }
                })();
            worker_lease.check().map_err(|e| e.to_string())?;
            result
        })
        .await
        .map_err(|e| e.to_string())??;

        lease.check().map_err(|e| e.to_string())?;
        if state.load_image_generation.load(Ordering::SeqCst) != my_generation {
            return Err("Load cancelled".into());
        }
        let arc_img = Arc::new(pristine_img);

        state.decoded_image_cache.lock().unwrap().insert(
            source_path_str.clone(),
            arc_img.clone(),
            exif_data_loaded.clone(),
        );

        (arc_img, exif_data_loaded)
    };

    if state.load_image_generation.load(Ordering::SeqCst) != my_generation {
        return Err("Load cancelled".to_string());
    }

    let is_raw = is_raw_file(&source_path_str);

    if state.load_image_generation.load(Ordering::SeqCst) != my_generation {
        return Err("Load cancelled".to_string());
    }

    let (orig_width, orig_height) = pristine_arc.dimensions();
    let as_shot = crate::as_shot::as_shot(&source_path_str);
    if is_raw {
        log::info!(
            "Baseline exposure for '{}': {:+.2} EV (DNG baseline {:+.2} EV, highlight preservation {:+.2} EV)",
            source_path_str,
            effective_baseline_exposure(&metadata.adjustments, as_shot, baseline_options),
            as_shot.exposure.dng_baseline,
            as_shot.exposure.highlight_preservation
        );
    }

    lease.check().map_err(|e| e.to_string())?;
    publish_current_original_image(
        &state.load_image_generation,
        &state.original_image,
        my_generation,
        LoadedImage {
            path,
            image: pristine_arc,
            is_raw,
            raw_meta: None,
            raw_bytes: None,
            prepared_source: None,
            open_identity: None,
            as_shot,
            raw_linear_mode,
            raw_highlight_compression,
        },
    )?;

    lease.check().map_err(|e| e.to_string())?;
    Ok(LoadImageResult {
        width: orig_width,
        height: orig_height,
        metadata,
        exif: exif_data,
        is_raw,
        as_shot_white_balance: as_shot.white_balance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcp::registry::ProfileRegistry;
    use serde_json::json;
    use std::path::PathBuf;

    const RAF_JPEG_MESSAGE: &str = "has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.";

    /// A complete JPEG produced by the real encoder, not a marker literal.
    fn encoded_jpeg() -> Vec<u8> {
        let image = image::RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb([(x * 8) as u8, (y * 10) as u8, 128])
        });
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .unwrap();
        bytes
    }

    /// Settings with RAW enhancement disabled so outputs can be compared exactly.
    fn settings(apple_raw: bool) -> AppSettings {
        AppSettings {
            use_apple_raw9: Some(apple_raw),
            raw_preprocessing_color_nr: Some(0.0),
            raw_preprocessing_sharpening: Some(0.0),
            ..AppSettings::default()
        }
    }

    fn path_in(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_string_lossy().into_owned()
    }

    fn open(bytes: &[u8], path: &str, apple_raw: bool) -> Result<DynamicImage> {
        load_base_image_from_bytes(bytes, path, false, &settings(apple_raw), None)
    }

    fn open_with_profiles(
        bytes: &[u8],
        path: &str,
        adjustments: &Value,
        cancel: Option<(Arc<AtomicUsize>, usize)>,
    ) -> Result<DynamicImage> {
        load_and_composite_with_profiles(
            bytes,
            path,
            adjustments,
            false,
            &settings(false),
            &ProfileRegistry::new(),
            cancel,
        )
    }

    fn selected_look() -> Value {
        json!({ "profile": { "look": "00000000-0000-0000-0000-000000000001" } })
    }

    #[test]
    fn misnamed_jpeg_raf_is_rejected_before_any_decoder() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = encoded_jpeg();
        for name in ["DSCF5389.RAF", "dscf5389.raf", "DSCF5389.RaF"] {
            let path = path_in(&dir, name);
            for apple_raw in [false, true] {
                let err = open(&jpeg, &path, apple_raw).unwrap_err().to_string();
                assert_eq!(
                    err,
                    format!("“{name}” {RAF_JPEG_MESSAGE}"),
                    "apple_raw={apple_raw}"
                );
            }
            for adjustments in [json!({}), selected_look()] {
                let err = open_with_profiles(&jpeg, &path, &adjustments, None)
                    .unwrap_err()
                    .to_string();
                assert_eq!(err, format!("“{name}” {RAF_JPEG_MESSAGE}"), "{adjustments}");
            }
        }
    }

    #[test]
    fn same_jpeg_named_jpg_opens_and_keeps_profile_raw_requirement() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = encoded_jpeg();
        let path = path_in(&dir, "DSCF5389.jpg");

        for apple_raw in [false, true] {
            let image = open(&jpeg, &path, apple_raw).unwrap();
            assert_eq!(image.dimensions(), (32, 24));
        }
        let composited = open_with_profiles(&jpeg, &path, &json!({}), None).unwrap();
        assert_eq!(composited.dimensions(), (32, 24));

        let err = open_with_profiles(&jpeg, &path, &selected_look(), None)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "Camera profiles require RAW input");
    }

    #[test]
    fn empty_content_is_reported_at_both_entry_points() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["empty.RAF", "empty.jpg", "empty.dng"] {
            let path = path_in(&dir, name);
            let expected = format!("“{name}” is empty and cannot be opened.");
            for apple_raw in [false, true] {
                assert_eq!(
                    open(&[], &path, apple_raw).unwrap_err().to_string(),
                    expected
                );
            }
            for adjustments in [json!({}), selected_look()] {
                let err = open_with_profiles(&[], &path, &adjustments, None).unwrap_err();
                assert_eq!(err.to_string(), expected);
            }
        }
    }

    #[test]
    fn cancelled_request_still_reports_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = encoded_jpeg();
        let cancelled = || Some((Arc::new(AtomicUsize::new(2)), 1));
        for (bytes, name) in [(&jpeg[..], "x.RAF"), (&[][..], "x.jpg")] {
            let path = path_in(&dir, name);
            let err =
                load_base_image_from_bytes(bytes, &path, false, &settings(false), cancelled())
                    .unwrap_err();
            assert_eq!(err.to_string(), "Load cancelled");
            let err = open_with_profiles(bytes, &path, &selected_look(), cancelled()).unwrap_err();
            assert_eq!(err.to_string(), "Load cancelled");
        }
    }

    #[test]
    fn jpeg_signature_under_other_raw_names_still_reaches_rawler() {
        // Minolta JFIF/EXIF RAW is a legitimate rawler input, so no blanket
        // JPEG-under-RAW rejection: this synthetic JPEG (no Minolta EXIF) must
        // fail in the decoder, not in the content guard.
        let dir = tempfile::tempdir().unwrap();
        let err = open(&encoded_jpeg(), &path_in(&dir, "x.MRW"), false)
            .unwrap_err()
            .to_string();
        assert!(!err.contains("JPEG signature"), "{err}");
        assert!(err.contains("No decoder found"), "{err}");
    }

    /// Minimal TIFF from an unknown maker carrying a JPEG preview via tags
    /// 513/514, so rawler rejects it and the embedded-preview fallback runs.
    fn tiff_with_jpeg_preview(jpeg: &[u8]) -> Vec<u8> {
        let make = b"R1 Unknown Maker\0";
        let make_offset = 8 + 2 + 3 * 12 + 4;
        let jpeg_offset = make_offset + make.len() + 1;
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        for (tag, kind, count, value) in [
            (271u16, 2u16, make.len() as u32, make_offset as u32),
            (513, 4, 1, jpeg_offset as u32),
            (514, 4, 1, jpeg.len() as u32),
        ] {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&kind.to_le_bytes());
            tiff.extend_from_slice(&count.to_le_bytes());
            tiff.extend_from_slice(&value.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(make);
        tiff.push(0);
        tiff.extend_from_slice(jpeg);
        tiff
    }

    #[test]
    fn embedded_jpeg_preview_fallback_still_runs_under_raf_name() {
        let dir = tempfile::tempdir().unwrap();
        let tiff = tiff_with_jpeg_preview(&encoded_jpeg());
        for name in ["preview.RAF", "preview.dng"] {
            let image = open(&tiff, &path_in(&dir, name), false).unwrap();
            assert_eq!(image.dimensions(), (32, 24), "{name}");
        }
    }

    fn env_path(var: &str) -> PathBuf {
        PathBuf::from(std::env::var(var).unwrap_or_else(|_| panic!("set {var}")))
    }

    /// Guarded open must equal the unguarded rawler development of the same bytes.
    fn assert_guard_preserves_raw_decode(bytes: &[u8], names: &[&str]) -> (u32, u32) {
        let s = settings(false);
        let direct = crate::raw_processing::develop_raw_image(
            bytes,
            false,
            s.raw_highlight_compression.unwrap_or(2.5),
            s.linear_raw_mode.clone(),
            None,
        )
        .unwrap()
        .into_rgb32f();
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let guarded = open(bytes, &path_in(&dir, name), false)
                .unwrap()
                .into_rgb32f();
            assert_eq!(guarded.dimensions(), direct.dimensions(), "{name}");
            assert!(guarded.as_raw() == direct.as_raw(), "{name}: pixels differ");
        }
        direct.dimensions()
    }

    #[test]
    #[ignore = "needs RAPIDRAW_R1_FIXTURE_DIR from scripts/integration-fixtures.py"]
    fn synthetic_cfa_dng_decodes_identically_including_raf_alias() {
        let dng =
            fs::read(env_path("RAPIDRAW_R1_FIXTURE_DIR").join("synthetic-camera.dng")).unwrap();
        let dims = assert_guard_preserves_raw_decode(
            &dng,
            &["synthetic-camera.dng", "synthetic-camera.RAF"],
        );
        println!(
            "synthetic CFA DNG decoded {}x{} under .dng and .RAF",
            dims.0, dims.1
        );
    }

    #[test]
    #[ignore = "needs RAPIDRAW_R1_NATIVE_RAF pointing at a genuine RAF"]
    fn native_raf_control_decodes_identically_through_guard() {
        let raf = fs::read(env_path("RAPIDRAW_R1_NATIVE_RAF")).unwrap();
        let dims = assert_guard_preserves_raw_decode(&raf, &["native.RAF", "native.raf"]);
        println!("native RAF control decoded {}x{}", dims.0, dims.1);
    }

    #[test]
    #[ignore = "needs RAPIDRAW_R1_FAILED_PAYLOAD (private user payload, read-only)"]
    fn reported_failed_payload_gets_precise_error_and_opens_as_jpg() {
        let payload = fs::read(env_path("RAPIDRAW_R1_FAILED_PAYLOAD")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let err = open(&payload, &path_in(&dir, "DSCF5389.RAF"), false)
            .unwrap_err()
            .to_string();
        assert_eq!(err, format!("“DSCF5389.RAF” {RAF_JPEG_MESSAGE}"));
        let image = open(&payload, &path_in(&dir, "DSCF5389.jpg"), false).unwrap();
        println!("reported payload as .jpg opened {:?}", image.dimensions());
    }
}

#[cfg(test)]
mod source_fence_tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc};

    #[test]
    fn beginning_a_load_advances_generation_and_clears_only_its_image_slot() {
        let generation = AtomicUsize::new(0);
        let original = Mutex::new(Some("previous"));
        let first = begin_original_image_load(&generation, &original);
        assert_eq!(first, 1);
        assert!(original.lock().unwrap().is_none());
        publish_current_original_image(&generation, &original, first, "first").unwrap();
        assert_eq!(*original.lock().unwrap(), Some("first"));
        let second = begin_original_image_load(&generation, &original);
        assert_eq!(second, 2);
        assert!(original.lock().unwrap().is_none());
    }

    #[test]
    fn a_prechecked_old_load_cannot_overwrite_a_newer_published_frame() {
        let generation = Arc::new(AtomicUsize::new(0));
        let original = Arc::new(Mutex::new(None));
        let first = begin_original_image_load(&generation, &original);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let old_generation = generation.clone();
        let old_original = original.clone();
        let worker = std::thread::spawn(move || {
            // Match the old loader's final check, then force the exact gap before publication.
            assert_eq!(old_generation.load(Ordering::SeqCst), first);
            ready_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            publish_current_original_image(&old_generation, &old_original, first, "old A")
        });
        ready_rx.recv().unwrap();
        let second = begin_original_image_load(&generation, &original);
        publish_current_original_image(&generation, &original, second, "current B").unwrap();
        resume_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), Err("Load cancelled".into()));
        assert_eq!(*original.lock().unwrap(), Some("current B"));
        assert_eq!(generation.load(Ordering::SeqCst), second);
    }

    #[test]
    fn returning_to_the_same_path_does_not_revive_an_earlier_load_generation() {
        let generation = AtomicUsize::new(0);
        let original = Mutex::new(None);
        let first_a = begin_original_image_load(&generation, &original);
        let b = begin_original_image_load(&generation, &original);
        let current_a = begin_original_image_load(&generation, &original);
        publish_current_original_image(&generation, &original, current_a, ("same A", 3)).unwrap();
        assert!(
            publish_current_original_image(&generation, &original, first_a, ("same A", 1)).is_err()
        );
        assert!(publish_current_original_image(&generation, &original, b, ("B", 2)).is_err());
        assert_eq!(*original.lock().unwrap(), Some(("same A", 3)));
    }
}

#[cfg(test)]
mod prepared_appearance_tests {
    use super::*;
    use serde_json::json;

    fn settings() -> AppSettings {
        let mut settings = AppSettings::default();
        settings.use_apple_raw9 = Some(false);
        settings.raw_preprocessing_color_nr = Some(0.0);
        settings.raw_preprocessing_sharpening = Some(0.0);
        settings
    }

    fn policy() -> AccurateRawPolicy {
        AccurateRawPolicy::new(
            1,
            rawler::imgop::sensor::xtrans::external::XTransPasses::One.scratch_bytes(),
        )
        .unwrap()
    }

    fn registry() -> crate::dcp::registry::ProfileRegistry {
        crate::dcp::registry::ProfileRegistry::new()
    }

    // Generated in-memory DNG, adapted from scripts/integration-fixtures.py.
    // Synthetic normalized CFA samples near (.125,.25,.5), black64/white4095.
    // This is not an independent calibrated RGB/demosaic-quality oracle.
    fn synthetic_dng(xtrans: bool, orientation: u16) -> Vec<u8> {
        let (width, height) = if xtrans { (512u32, 520u32) } else { (64, 64) };
        let pattern: Vec<u8> = if xtrans {
            b"RBGBRGGGRGGBGGBGGRBRGRBGGGBGGRGGRGGB"
                .iter()
                .map(|c| match c {
                    b'R' => 0,
                    b'G' => 1,
                    _ => 2,
                })
                .collect()
        } else {
            vec![0, 1, 1, 2]
        };
        let side = if xtrans { 6u16 } else { 2 };
        let shorts = |v: &[u16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>();
        let longs = |v: &[u32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>();
        let rationals = |v: &[(i32, i32)]| {
            v.iter()
                .flat_map(|(a, b)| a.to_le_bytes().into_iter().chain(b.to_le_bytes()))
                .collect::<Vec<_>>()
        };
        let mut tags: Vec<(u16, u16, u32, Vec<u8>)> = vec![
            (256, 4, 1, longs(&[width])),
            (257, 4, 1, longs(&[height])),
            (258, 3, 1, shorts(&[16])),
            (259, 3, 1, shorts(&[1])),
            (262, 3, 1, shorts(&[32803])),
            (271, 2, 9, b"RapidRAW\0".to_vec()),
            (272, 2, 19, b"Integration Camera\0".to_vec()),
            (273, 4, 1, longs(&[0])),
            (274, 3, 1, shorts(&[orientation])),
            (277, 3, 1, shorts(&[1])),
            (278, 4, 1, longs(&[height])),
            (279, 4, 1, longs(&[width * height * 2])),
            (284, 3, 1, shorts(&[1])),
            (33421, 3, 2, shorts(&[side, side])),
            (33422, 1, pattern.len() as u32, pattern.clone()),
            (50706, 1, 4, vec![1, 4, 0, 0]),
            (50707, 1, 4, vec![1, 1, 0, 0]),
            (50708, 2, 28, b"RapidRAW Integration Camera\0".to_vec()),
            (50710, 1, 3, vec![0, 1, 2]),
            (50711, 3, 1, shorts(&[1])),
            (50713, 3, 2, shorts(&[1, 1])),
            (50714, 5, 1, rationals(&[(64, 1)])),
            (50717, 4, 1, longs(&[4095])),
            (
                50721,
                10,
                9,
                rationals(&[
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                ]),
            ),
            (50728, 5, 3, rationals(&[(1, 2), (1, 1), (3, 4)])),
            (50778, 3, 1, shorts(&[21])),
            (50829, 4, 4, longs(&[0, 0, height, width])),
        ];
        // ASCII counts include exactly the existing bytes, including NUL.
        for tag in &mut tags {
            if tag.1 == 2 {
                tag.2 = tag.3.len() as u32;
            }
        }
        tags.sort_by_key(|t| t.0);
        let extra_offset = 8 + 2 + tags.len() * 12 + 4;
        let mut extra = Vec::new();
        let mut entries = Vec::new();
        for (id, kind, count, data) in &tags {
            entries.extend(id.to_le_bytes());
            entries.extend(kind.to_le_bytes());
            entries.extend(count.to_le_bytes());
            if data.len() > 4 {
                if (extra_offset + extra.len()) % 2 != 0 {
                    extra.push(0);
                }
                entries.extend(((extra_offset + extra.len()) as u32).to_le_bytes());
                extra.extend(data);
            } else {
                entries.extend(data);
                entries.extend(std::iter::repeat_n(0, 4 - data.len()));
            }
        }
        let pixels_offset = (extra_offset + extra.len()) as u32;
        let strip_index = tags.iter().position(|t| t.0 == 273).unwrap();
        entries[strip_index * 12 + 8..strip_index * 12 + 12]
            .copy_from_slice(&pixels_offset.to_le_bytes());
        let mut bytes = b"II".to_vec();
        bytes.extend(42u16.to_le_bytes());
        bytes.extend(8u32.to_le_bytes());
        bytes.extend((tags.len() as u16).to_le_bytes());
        bytes.extend(entries);
        bytes.extend([0; 4]);
        bytes.extend(extra);
        for y in 0..height {
            for x in 0..width {
                let channel =
                    pattern[((y % side as u32) * side as u32 + x % side as u32) as usize] as usize;
                let value = [568u16, 1072, 2080][channel];
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn preprocessing_defaults_mapping_and_route_gates() {
        let mut settings = settings();
        settings.raw_preprocessing_color_nr = None;
        settings.raw_preprocessing_sharpening = None;
        let p = RawPreprocessing::from_settings(&settings);
        assert_eq!(p.color_nr_amount.to_bits(), 14.0f32.to_bits());
        assert_eq!(p.sharpening_amount.to_bits(), 0.35f32.to_bits());
        assert!(p.should_apply(true, false));
        assert!(!p.should_apply(true, true));
        assert!(!p.should_apply(false, false));
        settings.apply_preprocessing_to_non_raws = Some(true);
        assert!(RawPreprocessing::from_settings(&settings).should_apply(false, false));
        settings.raw_preprocessing_color_nr = Some(1.0);
        assert_eq!(
            RawPreprocessing::from_settings(&settings).color_nr_amount,
            2.0
        );
        settings.raw_preprocessing_color_nr = Some(-1.0);
        assert_eq!(
            RawPreprocessing::from_settings(&settings).color_nr_amount,
            0.0
        );
    }

    #[test]
    fn disabled_preprocessing_preserves_hdr_and_signed_bits() {
        let bits = [
            (-0.25f32).to_bits(),
            2.0f32.to_bits(),
            0.0f32.to_bits(),
            (-0.0f32).to_bits(),
        ];
        let samples = bits.map(f32::from_bits);
        let mut image = DynamicImage::ImageRgba32F(
            image::ImageBuffer::from_raw(1, 1, samples.to_vec()).unwrap(),
        );
        RawPreprocessing::from_settings(&settings()).apply(&mut image, true, false);
        assert_eq!(
            image
                .as_rgba32f()
                .unwrap()
                .as_raw()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            bits
        );
    }

    #[test]
    fn captured_appearance_inputs_do_not_follow_mutation() {
        let mut adjustments = json!({"profile":{"amount":200},"exposure":1});
        let mut settings = settings();
        let input = AppearanceInputs::capture(&adjustments, &settings);
        adjustments["profile"]["amount"] = json!(0);
        settings.raw_preprocessing_color_nr = Some(1.0);
        assert_eq!(input.adjustments["profile"]["amount"], 200);
        assert_eq!(input.settings.raw_preprocessing_color_nr, Some(0.0));
    }

    #[test]
    fn resource_policy_uses_actual_rawler_limits_and_one() {
        use rawler::imgop::sensor::xtrans::external::XTransPasses;
        let minimum = XTransPasses::One.scratch_bytes();
        assert!(AccurateRawPolicy::new(0, minimum).is_err());
        assert!(AccurateRawPolicy::new(1, minimum - 1).is_err());
        match AccurateRawPolicy::new(2, minimum).unwrap().quality() {
            crate::raw_processing::PreparedQuality::AccurateXtrans1 {
                max_workers,
                scratch_budget_bytes,
            } => {
                assert_eq!(max_workers, 2);
                assert_eq!(scratch_budget_bytes, minimum);
            }
            _ => panic!("accurate policy must never choose CurrentFull"),
        }
    }

    #[test]
    fn preparation_decisions_never_turn_unknown_errors_into_legacy() {
        let registry = registry();
        assert!(matches!(
            prepare_open_appearance(
                b"bad",
                "missing.jpg",
                &json!({}),
                &settings(),
                &registry,
                policy(),
                None
            )
            .unwrap(),
            PreparedAppearanceDecision::LegacyRequired(LegacyPreparedReason::NonRaw)
        ));
        let mut apple = settings();
        apple.use_apple_raw9 = Some(true);
        assert!(matches!(
            prepare_open_appearance(
                b"bad",
                "missing.dng",
                &json!({}),
                &apple,
                &registry,
                policy(),
                None
            )
            .unwrap(),
            PreparedAppearanceDecision::LegacyRequired(LegacyPreparedReason::AppleRawPreferred)
        ));
        assert!(
            prepare_open_appearance(
                b"bad",
                "missing.dng",
                &json!({}),
                &settings(),
                &registry,
                policy(),
                None
            )
            .is_err()
        );
        assert!(
            prepare_open_appearance(
                b"bad",
                "missing.dng",
                &json!({"profile":{"look":"missing"}}),
                &apple,
                &registry,
                policy(),
                None
            )
            .is_err()
        );
    }

    // BEGIN patch-shape contract regressions (test-only).
    fn patch_contract_png(image: DynamicImage) -> String {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        general_purpose::STANDARD.encode(bytes.into_inner())
    }

    fn patch_contract_adjustments(
        mask: Option<&str>,
        size: Option<(u32, u32)>,
        offset: (u32, u32),
    ) -> Value {
        let color = patch_contract_png(DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            1,
            1,
            image::Rgb([255, 0, 0]),
        )));
        let mut adjustments = json!({"aiPatches":[{
            "id":"shape-contract", "name":"shape-contract", "visible":true,
            "patchData":{"offsetX":offset.0,"offsetY":offset.1,"color":color,"isSrgbEncoded":true},
            "subMasks":[{"id":"all","type":"all","visible":true,"invert":false,"opacity":100.0,"mode":"additive","parameters":{}}]
        }]});
        let data = &mut adjustments["aiPatches"][0]["patchData"];
        if let Some(mask) = mask {
            data["mask"] = json!(mask);
        }
        if let Some((width, height)) = size {
            data["width"] = json!(width);
            data["height"] = json!(height);
        }
        adjustments
    }

    fn patch_contract_bases() -> [(&'static str, DynamicImage); 4] {
        [
            (
                "RGB32F",
                DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
                    4,
                    3,
                    image::Rgb([0.0, 0.0, 0.0]),
                )),
            ),
            (
                "RGBA32F",
                DynamicImage::ImageRgba32F(image::Rgba32FImage::from_pixel(
                    4,
                    3,
                    image::Rgba([0.0, 0.0, 0.0, 0.375]),
                )),
            ),
            (
                "RGB8",
                DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 3, image::Rgb([0, 0, 0]))),
            ),
            (
                "RGBA8",
                DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    4,
                    3,
                    image::Rgba([0, 0, 0, 96]),
                )),
            ),
        ]
    }

    #[test]
    fn p01_invalid_cropped_patch_returns_error_without_panic() {
        let invalid_mask = patch_contract_png(DynamicImage::ImageLuma8(
            image::GrayImage::from_pixel(2, 2, image::Luma([255])),
        ));
        let valid_mask = patch_contract_png(DynamicImage::ImageLuma8(
            image::GrayImage::from_pixel(1, 1, image::Luma([255])),
        ));
        for (format, base) in patch_contract_bases() {
            let before = base.as_bytes().to_vec();
            for (kind, adjustments) in [
                (
                    "explicit2x2",
                    patch_contract_adjustments(Some(&invalid_mask), Some((2, 2)), (0, 0)),
                ),
                (
                    "generated_omitted_dimensions",
                    patch_contract_adjustments(None, None, (0, 0)),
                ),
            ] {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    composite_patches_on_image(&base, &adjustments)
                }));
                let error = result
                    .unwrap_or_else(|_| panic!("{format}/{kind} panicked"))
                    .expect_err("mismatched decoded shapes must return Err");
                assert!(
                    error
                        .to_string()
                        .contains("Decoded patch color/mask dimensions differ"),
                    "{format}/{kind}: {error:#}"
                );
                assert_eq!(base.as_bytes(), before, "{format}/{kind} changed source");
            }
            // Matching explicit and generated forms prevent unconditional rejection.
            for adjustments in [
                patch_contract_adjustments(Some(&valid_mask), Some((1, 1)), (0, 0)),
                patch_contract_adjustments(None, Some((1, 1)), (0, 0)),
            ] {
                let output = composite_patches_on_image(&base, &adjustments).unwrap();
                assert_eq!(output.dimensions(), (4, 3));
                assert_eq!(
                    base.as_bytes(),
                    before,
                    "{format} valid control changed source"
                );
            }
        }
    }

    #[test]
    fn p02_valid_cropped_patch_preserves_uncovered_pixels() {
        let opaque = patch_contract_png(DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
            1,
            1,
            image::Luma([255]),
        )));
        let partial = patch_contract_png(DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
            1,
            1,
            image::Luma([128]),
        )));
        for (format, base) in patch_contract_bases() {
            let before = base.as_bytes().to_vec();
            for mask in [Some(opaque.as_str()), None] {
                let adjustments = patch_contract_adjustments(mask, Some((1, 1)), (1, 1));
                let output = composite_patches_on_image(&base, &adjustments).unwrap();
                assert_eq!(output.dimensions(), (4, 3));
                match (&base, &output) {
                    (DynamicImage::ImageRgb32F(_), DynamicImage::ImageRgb32F(rgb)) => {
                        for (x, y, pixel) in rgb.enumerate_pixels() {
                            let expected = if (x, y) == (1, 1) {
                                [1.0f32.to_bits(), 0, 0]
                            } else {
                                [0, 0, 0]
                            };
                            assert_eq!(pixel.0.map(f32::to_bits), expected, "{format} at({x},{y})");
                        }
                    }
                    (DynamicImage::ImageRgba32F(_), DynamicImage::ImageRgba32F(rgba)) => {
                        for (x, y, pixel) in rgba.enumerate_pixels() {
                            let expected = if (x, y) == (1, 1) {
                                [1.0f32.to_bits(), 0, 0, 0.375f32.to_bits()]
                            } else {
                                [0, 0, 0, 0.375f32.to_bits()]
                            };
                            assert_eq!(pixel.0.map(f32::to_bits), expected, "{format} at({x},{y})");
                        }
                    }
                    (DynamicImage::ImageRgb8(_), DynamicImage::ImageRgba32F(_))
                    | (DynamicImage::ImageRgba8(_), DynamicImage::ImageRgba32F(_)) => {
                        // The unchanged 8-bit fallback returns RGBA32F, not the input enum.
                        let alpha = if format == "RGBA8" { 96 } else { 255 };
                        for (x, y, pixel) in output.to_rgba8().enumerate_pixels() {
                            let expected = if (x, y) == (1, 1) {
                                [255, 0, 0, alpha]
                            } else {
                                [0, 0, 0, alpha]
                            };
                            assert_eq!(pixel.0, expected, "{format} at({x},{y})");
                        }
                    }
                    _ => panic!("unexpected output format for {format}"),
                }
                assert_eq!(base.as_bytes(), before, "{format} changed source");
            }
            if matches!(
                &base,
                DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_)
            ) {
                let adjustments = patch_contract_adjustments(Some(&partial), Some((1, 1)), (1, 1));
                let output = composite_patches_on_image(&base, &adjustments).unwrap();
                match output {
                    DynamicImage::ImageRgb32F(rgb) => {
                        for (x, y, pixel) in rgb.enumerate_pixels() {
                            let expected = if (x, y) == (1, 1) {
                                [1056997505, 0, 0]
                            } else {
                                [0, 0, 0]
                            };
                            assert_eq!(
                                pixel.0.map(f32::to_bits),
                                expected,
                                "{format} partial at({x},{y})"
                            );
                        }
                    }
                    DynamicImage::ImageRgba32F(rgba) => {
                        for (x, y, pixel) in rgba.enumerate_pixels() {
                            let expected = if (x, y) == (1, 1) {
                                [1056997505, 0, 0, 0.375f32.to_bits()]
                            } else {
                                [0, 0, 0, 0.375f32.to_bits()]
                            };
                            assert_eq!(
                                pixel.0.map(f32::to_bits),
                                expected,
                                "{format} partial at({x},{y})"
                            );
                        }
                    }
                    _ => unreachable!(),
                }
                assert_eq!(base.as_bytes(), before, "{format} partial changed source");
            }
        }
    }
    // END patch-shape contract regressions (test-only).

    #[test]
    fn cancellation_precedes_asset_and_source_work() {
        let cancel = Some((Arc::new(AtomicUsize::new(2)), 1));
        let error = prepare_open_appearance(
            b"bad",
            "missing.dng",
            &json!({"profile":{"look":"missing"}}),
            &settings(),
            &registry(),
            policy(),
            cancel,
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "Load cancelled");
    }

    #[test]
    fn decoded_legacy_retains_real_source_without_path_reopen() {
        let bytes = synthetic_dng(false, 1);
        let source = prepare_open_appearance(
            &bytes,
            "not-on-disk.dng",
            &json!({}),
            &settings(),
            &registry(),
            policy(),
            None,
        )
        .unwrap();
        let PreparedAppearanceDecision::DecodedLegacy(source) = source else {
            panic!("Bayer must remain a decoded legacy owner");
        };
        let as_shot = source.as_shot();
        let image = source.render_current(None).unwrap();
        assert_eq!(image.dimensions(), (64, 64));
        assert!(as_shot.white_balance.temperature.is_finite());
    }

    #[test]
    fn native_prepared_owner_orientation_and_both_cancelled_passes() {
        let bytes = synthetic_dng(true, 6);
        let before = bytes.clone();
        let png = |image: DynamicImage| {
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Png).unwrap();
            general_purpose::STANDARD.encode(out.into_inner())
        };
        let color = png(DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgb([255, 0, 0]),
        )));
        let mask = png(DynamicImage::ImageLuma8(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Luma([128]),
        )));
        let adjustments = json!({"aiPatches":[{"patchData":{"offsetX":0,"offsetY":0,"color":color,"mask":mask,"isSrgbEncoded":false}}]});
        let source = prepare_open_appearance(
            &bytes,
            "not-on-disk.dng",
            &adjustments,
            &settings(),
            &registry(),
            policy(),
            None,
        )
        .unwrap();
        let PreparedAppearanceDecision::Prepared(source) = source else {
            panic!("X-Trans must own prepared accurate source");
        };
        assert!(Arc::ptr_eq(
            &source.retained_source(),
            &source.retained_source()
        ));
        let (pristine, _) = source
            .source
            .render_pristine(AppearancePass::AccurateOne, None)
            .unwrap();
        assert_eq!(pristine.dimensions(), (520, 512));
        // Metamorphic compositing oracle, not a demosaic-quality baseline:
        // exactly one half-opacity red patch at (0,0), other pixels unchanged.
        let patched = source
            .render_appearance(AppearancePass::AccurateOne, None)
            .unwrap()
            .to_rgba32f();
        let pristine = pristine.to_rgba32f();
        let base = pristine.get_pixel(0, 0).0;
        let alpha = 128.0f32 / 255.0;
        let expected = [
            alpha + base[0] * (1.0 - alpha),
            base[1] * (1.0 - alpha),
            base[2] * (1.0 - alpha),
        ];
        for (actual, expected) in patched.get_pixel(0, 0).0[..3].iter().zip(expected) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        assert_eq!(
            patched.get_pixel(1, 0).0.map(f32::to_bits),
            pristine.get_pixel(1, 0).0.map(f32::to_bits)
        );

        assert_eq!(bytes, before);
        let cancel = Some((Arc::new(AtomicUsize::new(2)), 1));
        for pass in [AppearancePass::CurrentFull, AppearancePass::AccurateOne] {
            assert_eq!(
                source
                    .render_appearance(pass, cancel.clone())
                    .unwrap_err()
                    .to_string(),
                "Load cancelled"
            );
        }
        // No profile returns None without developing or reopening even a missing path.
        assert!(
            source
                .source
                .render_selected(&json!({}), &registry(), None)
                .unwrap()
                .is_none()
        );
    }

    pub(super) fn composition_owner() -> PreparedOpenAppearance {
        compose_owner(&json!({}), &settings(), &registry())
    }

    fn compose_owner(
        adjustments: &Value,
        settings: &AppSettings,
        registry: &crate::dcp::registry::ProfileRegistry,
    ) -> PreparedOpenAppearance {
        let decision = prepare_open_appearance(
            &synthetic_dng(true, 6),
            "generated-not-on-disk.dng",
            adjustments,
            settings,
            registry,
            policy(),
            None,
        )
        .unwrap();
        match decision {
            PreparedAppearanceDecision::Prepared(source) => source,
            _ => panic!("Generated native X-Trans must use its actual prepared owner"),
        }
    }

    // Fixture encoding only, adapted byte-for-byte from registry's synthetic
    // DCP builder. This is not another colour renderer or quality oracle.
    fn write_synthetic_dcp(path: &Path, camera: &str, name: &str) {
        #[derive(Clone)]
        struct Tag {
            tag: u16,
            typ: u16,
            data: Vec<u8>,
        }
        let ascii = |s: &str| -> Vec<u8> {
            let mut v = s.as_bytes().to_vec();
            v.push(0);
            v
        };
        let srat = |v: &[(i32, i32)]| -> Vec<u8> {
            v.iter()
                .flat_map(|(n, d)| {
                    let mut b = n.to_le_bytes().to_vec();
                    b.extend_from_slice(&d.to_le_bytes());
                    b
                })
                .collect()
        };
        let tags = [
            Tag {
                tag: 50708,
                typ: 2,
                data: ascii(camera),
            },
            Tag {
                tag: 50936,
                typ: 2,
                data: ascii(name),
            },
            Tag {
                tag: 50721,
                typ: 10,
                data: srat(&[
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                    (0, 1),
                    (0, 1),
                    (0, 1),
                    (1, 1),
                ]),
            },
            Tag {
                tag: 50778,
                typ: 3,
                data: 17u16.to_le_bytes().to_vec(),
            },
            Tag {
                tag: 50779,
                typ: 3,
                data: 21u16.to_le_bytes().to_vec(),
            },
            Tag {
                tag: 50941,
                typ: 4,
                data: 2u32.to_le_bytes().to_vec(),
            },
        ];

        let mut ifd = Vec::new();
        ifd.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        let entry_start = ifd.len();
        ifd.resize(entry_start + tags.len() * 12, 0);
        ifd.extend_from_slice(&0u32.to_le_bytes());
        let mut pool = Vec::new();
        for (i, t) in tags.iter().enumerate() {
            let count = match t.typ {
                2 => t.data.len() as u32,
                3 => (t.data.len() / 2) as u32,
                4 => (t.data.len() / 4) as u32,
                10 => (t.data.len() / 8) as u32,
                _ => 0,
            };
            let base = entry_start + i * 12;
            ifd[base..base + 2].copy_from_slice(&t.tag.to_le_bytes());
            ifd[base + 2..base + 4].copy_from_slice(&t.typ.to_le_bytes());
            ifd[base + 4..base + 8].copy_from_slice(&count.to_le_bytes());
            if t.data.len() <= 4 {
                ifd[base + 8..base + 8 + t.data.len()].copy_from_slice(&t.data);
            } else {
                let off = 8 + ifd.len() + pool.len();
                ifd[base + 8..base + 12].copy_from_slice(&(off as u32).to_le_bytes());
                pool.extend_from_slice(&t.data);
            }
        }
        ifd.extend_from_slice(&pool);

        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&0x4352u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&ifd);
        std::fs::write(path, &out).unwrap();
    }

    fn synthetic_look_file(path: &std::path::Path) {
        use std::io::Write;
        let mut raw = Vec::new();
        for v in [1u32, 1, 3, 3] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..27 {
            for _ in 0..3 {
                raw.extend_from_slice(&32768u16.to_le_bytes());
            }
        }
        for v in [2u32, 0, 0] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        raw.extend_from_slice(&0.0f64.to_le_bytes());
        raw.extend_from_slice(&2.0f64.to_le_bytes());
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).unwrap();
        let mut block = (raw.len() as u32).to_le_bytes().to_vec();
        block.extend(encoder.finish().unwrap());
        // Existing decoder's reverse alphabet; least-significant digit first.
        const ALPHABET: &[u8;85] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#";
        let mut encoded = String::new();
        for chunk in block.chunks(4) {
            let mut bytes = [0u8; 4];
            bytes[..chunk.len()].copy_from_slice(chunk);
            let mut value = u32::from_le_bytes(bytes) as u64;
            for _ in 0..chunk.len() + 1 {
                encoded.push(ALPHABET[(value % 85) as usize] as char);
                value /= 85;
            }
        }
        let xml = format!(
            r#"<rdf:Description xmlns:rdf="rdf" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:PresetType="Look" crs:Cluster="Synthetic" crs:UUID="composition-look" crs:SupportsAmount="True" crs:CameraModelRestriction="RapidRAW Integration Camera" crs:CameraProfile="Composition base" crs:RGBTable="synthetic" crs:Table_synthetic="{encoded}" crs:ProcessVersion="15.4"></rdf:Description>"#
        );
        std::fs::write(path, xml).unwrap();
    }

    #[test]
    fn r01_actual_native_producer_filters_once_currentfull_zero() {
        let source = compose_owner(&json!({}), &settings(), &registry());
        let same_raw = &source.source.prepared;
        for pass in [AppearancePass::CurrentFull, AppearancePass::AccurateOne] {
            ACCURATE_FILTER_CALLS.with(|count| count.set(0));
            let base = source.render_unpatched_base(pass, None).unwrap();
            let count = ACCURATE_FILTER_CALLS.with(|count| count.get());
            assert_eq!(
                count,
                if matches!(pass, AppearancePass::AccurateOne) {
                    1
                } else {
                    0
                }
            );
            let (raw_image, orientation, _) = crate::raw_processing::develop_prepared_raw(
                same_raw,
                source.source.quality(pass),
                2.5,
                &settings().linear_raw_mode,
                false,
                None,
            )
            .unwrap();
            let mut expected = apply_orientation(raw_image, orientation);
            if matches!(pass, AppearancePass::AccurateOne) {
                expected = crate::false_colour::suppress_linear_srgb_v1(expected, &None).unwrap();
            }
            RawPreprocessing::from_settings(&settings()).apply(&mut expected, true, false);
            assert_eq!(base.pixels().as_bytes(), expected.as_bytes());
            assert_eq!(base.pixels().dimensions(), (520, 512));
            assert!(Arc::ptr_eq(
                base.retained_source(),
                &source.retained_source()
            ));
        }
    }

    #[test]
    fn l02_selected_amounts_use_one_working_filter_and_no_gpu_profile() {
        let dir = tempfile::tempdir().unwrap();
        let dcp = dir.path().join("base.dcp");
        write_synthetic_dcp(&dcp, "RapidRAW Integration Camera", "Composition base");
        let look_path = dir.path().join("look.xmp");
        synthetic_look_file(&look_path);
        let registry = registry();
        let entry = crate::dcp::registry::parse_dcp_entry(
            &dcp,
            crate::dcp::registry::ProfileSource::Managed,
        )
        .unwrap();
        let id = entry.id.to_hex();
        registry.insert_dcp(entry);
        let look = crate::dcp::look_xmp::parse_look_xmp(&look_path).unwrap();
        assert!(matches!(
            look.table,
            crate::dcp::look_xmp::LookTableSource::Decoded(_)
        ));
        registry.register_look(crate::dcp::registry::LookMetadata {
            uuid: look.uuid,
            name: look.name,
            group: look.group,
            required_base_profile: look.required_base_profile,
            camera_model_restriction: look.camera_model_restriction,
            file_path: look_path.clone(),
        });
        for amount in [0, 100, 200] {
            let adjustments =
                json!({"profile":{"base":id,"look":"composition-look","amount":amount}});
            let mut settings = settings();
            // Selected route must still bypass even strong generic preprocessing.
            settings.raw_preprocessing_color_nr = Some(0.5);
            settings.raw_preprocessing_sharpening = Some(0.35);
            let source = compose_owner(&adjustments, &settings, &registry);
            let assets = crate::open_creative_snapshot::capture_creative_assets(
                &adjustments,
                &settings,
                None,
            )
            .unwrap();
            ACCURATE_FILTER_CALLS.with(|count| count.set(0));
            let base = source
                .render_unpatched_base(AppearancePass::AccurateOne, None)
                .unwrap();
            assert_eq!(ACCURATE_FILTER_CALLS.with(|count| count.get()), 1);
            let original = crate::dcp::pipeline::render_prepared_selected_profile(
                &source.source.prepared,
                source.assets.as_ref().unwrap(),
                source.source.policy.quality(),
                2.5,
                &settings.linear_raw_mode,
                None,
            )
            .unwrap();
            let expected = crate::false_colour::suppress_linear_srgb_v1(original, &None).unwrap();
            assert_eq!(base.pixels().as_bytes(), expected.as_bytes());
            let fit = crate::open_composition::compose_fit(
                &base,
                &assets,
                crate::open_composition::FitPreviewSpec::new(32).unwrap(),
                None,
            )
            .unwrap();
            assert_eq!(fit.request().adjustments.global.has_dcp, 0);
            assert!(fit.request().dcp_textures.is_none());
            assert_eq!(base.pixels().dimensions(), (520, 512));
        }
    }

    #[test]
    fn r02_fresh_pristine_and_single_patch_share_export_seam() {
        let color = {
            let mut png = std::io::Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0])))
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            general_purpose::STANDARD.encode(png.into_inner())
        };
        let adjustments = json!({"aiPatches":[{"id":"red","name":"red","visible":true,"patchData":{"offsetX":0,"offsetY":0,"width":1,"height":1,"color":color,"isSrgbEncoded":true},"subMasks":[{"id":"all","type":"all","visible":true,"invert":false,"opacity":50.0,"mode":"additive","parameters":{}}]}]});
        let source = compose_owner(&adjustments, &settings(), &registry());
        let first = source
            .render_unpatched_base(AppearancePass::AccurateOne, None)
            .unwrap();
        let before = first.pixels().as_bytes().to_vec();
        let expected = composite_patches_on_image(first.pixels(), &adjustments).unwrap();
        let actual = source
            .render_appearance(AppearancePass::AccurateOne, None)
            .unwrap();
        assert_eq!(actual.as_bytes(), expected.as_bytes());
        let repeat = source
            .render_unpatched_base(AppearancePass::AccurateOne, None)
            .unwrap();
        assert_eq!(repeat.pixels().as_bytes(), before);
        assert_eq!(first.pixels().as_bytes(), before);
    }

    #[test]
    fn r04_profile_errors_and_cancel_remain_fail_closed() {
        let bytes = synthetic_dng(true, 6);
        let error = match prepare_open_appearance(
            &bytes,
            "generated.dng",
            &json!({"profile":{"look":"missing"}}),
            &settings(),
            &registry(),
            policy(),
            None,
        ) {
            Err(error) => error,
            Ok(_) => panic!("missing profile must not succeed"),
        };
        assert!(error.to_string().contains("explicitly selected base"));
        let source = compose_owner(&json!({}), &settings(), &registry());
        let cancel = Some((Arc::new(AtomicUsize::new(2)), 1));
        for pass in [AppearancePass::CurrentFull, AppearancePass::AccurateOne] {
            let error = match source.render_unpatched_base(pass, cancel.clone()) {
                Err(error) => error,
                Ok(_) => panic!("cancelled producer returned pixels"),
            };
            assert_eq!(error.to_string(), "Load cancelled");
        }
    }

    // BEGIN B2-COVERAGE-TEST-ADDITIONS
    // Fixed BEFORE execution. Only sample-strip bytes of the genuine old DNG
    // generator change; metadata, CFA phase, WB, levels and matrices stay exact.
    fn patterned_composition_dng() -> Vec<u8> {
        let mut bytes = synthetic_dng(true, 6);
        let u16_at = |bytes: &[u8], offset: usize| {
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
        };
        let u32_at = |bytes: &[u8], offset: usize| {
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
        };
        let ifd = u32_at(&bytes, 4) as usize;
        let count = u16_at(&bytes, ifd) as usize;
        let scalar = |tag: u16| {
            let entry = (0..count)
                .map(|i| ifd + 2 + 12 * i)
                .find(|offset| u16_at(&bytes, *offset) == tag)
                .unwrap();
            assert_eq!(u16_at(&bytes, entry + 2), 4);
            assert_eq!(u32_at(&bytes, entry + 4), 1);
            u32_at(&bytes, entry + 8) as usize
        };
        let (width, height, strip, length) = (scalar(256), scalar(257), scalar(273), scalar(279));
        assert_eq!((width, height), (512, 520));
        assert_eq!(length, width * height * 2);
        assert_eq!(strip + length, bytes.len());
        let pattern = b"RBGBRGGGRGGBGGBGGRBRGRBGGGBGGRGGRGGB";
        for y in 0..height {
            for x in 0..width {
                let channel = match pattern[(y % 6) * 6 + x % 6] {
                    b'R' => 0,
                    b'G' => 1,
                    _ => 2,
                };
                let luma = ((x / 8 + 3 * (y / 9)) % 97) as i32 - 48;
                let sign = if (x / 2 + y / 3) % 2 == 0 { 1 } else { -1 };
                let chroma = [32, -11, -48][channel] * sign;
                let impulse = if (13 * x + 29 * y) % 37 == 0 {
                    [54, -27, 18][channel]
                } else {
                    0
                };
                let value = 64 + [960, 1920, 1440][channel] + luma + chroma + impulse;
                assert!((64..4095).contains(&value));
                let offset = strip + (y * width + x) * 2;
                bytes[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
            }
        }
        bytes
    }

    pub(super) fn patterned_composition_owner() -> PreparedOpenAppearance {
        let decision = prepare_open_appearance(
            &patterned_composition_dng(),
            "fixed-patterned-not-on-disk.dng",
            &json!({}),
            &settings(),
            &registry(),
            policy(),
            None,
        )
        .unwrap();
        match decision {
            PreparedAppearanceDecision::Prepared(owner) => owner,
            _ => panic!("The real patterned native source must prepare successfully"),
        }
    }

    #[test]
    fn r06_normal_producer_filters_before_nonzero_preprocessing() {
        let bytes = patterned_composition_dng();
        let original_bytes = bytes.clone();
        let mut captured = settings();
        captured.raw_preprocessing_color_nr = Some(0.5);
        captured.raw_preprocessing_sharpening = Some(0.35);
        let decision = prepare_open_appearance(
            &bytes,
            "fixed-patterned-not-on-disk.dng",
            &json!({}),
            &captured,
            &registry(),
            policy(),
            None,
        )
        .unwrap();
        let PreparedAppearanceDecision::Prepared(owner) = decision else {
            panic!("Expected actual native prepared owner")
        };
        let preprocessing = RawPreprocessing::from_settings(&captured);
        assert_eq!(owner.settings().raw_preprocessing_color_nr, Some(0.5));
        assert_eq!(owner.settings().raw_preprocessing_sharpening, Some(0.35));
        assert_eq!(preprocessing.color_nr_amount, 14.0);
        assert_eq!(preprocessing.sharpening_amount, 0.35);
        ACCURATE_FILTER_CALLS.with(|count| count.set(0));
        let actual = owner
            .render_unpatched_base(AppearancePass::AccurateOne, None)
            .unwrap();
        assert_eq!(ACCURATE_FILTER_CALLS.with(|count| count.get()), 1);
        let (developed, orientation, _) = crate::raw_processing::develop_prepared_raw(
            &owner.source.prepared,
            owner.source.quality(AppearancePass::AccurateOne),
            captured.raw_highlight_compression.unwrap_or(2.5),
            &captured.linear_raw_mode,
            false,
            None,
        )
        .unwrap();
        let oriented = apply_orientation(developed, orientation);
        let oriented_before = oriented.as_bytes().to_vec();
        // Frozen, existing operations are the comparator. No candidate producer
        // helper generates the answer, and the controls must actually differ.
        let mut correct =
            crate::false_colour::suppress_linear_srgb_v1(oriented.clone(), &None).unwrap();
        preprocessing.apply(&mut correct, true, false);
        let mut missing_fcs = oriented.clone();
        preprocessing.apply(&mut missing_fcs, true, false);
        let mut reversed = oriented.clone();
        preprocessing.apply(&mut reversed, true, false);
        let reversed = crate::false_colour::suppress_linear_srgb_v1(reversed, &None).unwrap();
        assert_eq!(actual.pixels().as_bytes(), correct.as_bytes());
        assert_ne!(
            correct.as_bytes(),
            missing_fcs.as_bytes(),
            "Frozen source must distinguish absent FCS; failure requires root adjudication"
        );
        assert_ne!(
            correct.as_bytes(),
            reversed.as_bytes(),
            "Frozen source must distinguish reversed order; do not tune after execution"
        );
        assert_eq!(oriented.as_bytes(), oriented_before);
        ACCURATE_FILTER_CALLS.with(|count| count.set(0));
        let current = owner
            .render_unpatched_base(AppearancePass::CurrentFull, None)
            .unwrap();
        assert_eq!(ACCURATE_FILTER_CALLS.with(|count| count.get()), 0);
        let (legacy, orientation, _) = crate::raw_processing::develop_prepared_raw(
            &owner.source.prepared,
            owner.source.quality(AppearancePass::CurrentFull),
            captured.raw_highlight_compression.unwrap_or(2.5),
            &captured.linear_raw_mode,
            false,
            None,
        )
        .unwrap();
        let mut legacy = apply_orientation(legacy, orientation);
        preprocessing.apply(&mut legacy, true, false);
        assert_eq!(current.pixels().as_bytes(), legacy.as_bytes());
        assert_eq!(bytes, original_bytes);
        assert_eq!(captured.raw_preprocessing_color_nr, Some(0.5));
        assert_eq!(captured.raw_preprocessing_sharpening, Some(0.35));
    }
    // END B2-COVERAGE-TEST-ADDITIONS

    // Test bridge runs the actual B2a prepared factory; no fabricated RawImage,
    // retained source or proof constructor crosses the sibling-module boundary.
    pub(super) fn progressive_candidate(
        context: &crate::progressive_open::WorkerContext,
        xtrans: bool,
    ) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
        let bytes = synthetic_dng(xtrans, 6);
        super::prepare_keyed_candidate(
            &bytes,
            "not-on-disk.dng",
            context.spec()?,
            &registry(),
            context.raw_cancel(),
        )
    }

    #[test]
    fn keyed_snapshot_reads_sidecar_once_and_keeps_captured_settings() {
        let directory =
            std::env::temp_dir().join(format!("rapidraw-keyed-snapshot-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        let path = directory.join("synthetic.dng");
        let text = path.to_string_lossy().to_string();
        let (_, sidecar) = parse_virtual_path(&text);
        let original = ImageMetadata {
            rating: 3,
            adjustments: json!({"exposure":1.25}),
            ..Default::default()
        };
        std::fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut saved_settings = settings();
        let (metadata, captured) = super::capture_keyed_metadata(&text, saved_settings.clone());
        saved_settings.raw_preprocessing_sharpening = Some(1.0);
        assert_eq!(saved_settings.raw_preprocessing_sharpening, Some(1.0));
        std::fs::write(
            &sidecar,
            serde_json::to_vec(&ImageMetadata::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata.rating, 3);
        assert_eq!(metadata.adjustments["exposure"], 1.25);
        assert_eq!(captured.raw_preprocessing_sharpening, Some(0.0));
    }

    #[test]
    fn keyed_legacy_nonraw_reuses_exact_orientation_and_preprocessing_pixels() {
        let image = DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(3, 2, |x, y| {
            image::Rgb([(x * 60) as u8, (y * 90) as u8, 255])
        }));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let mut expected = load_image_with_orientation(&bytes, None).unwrap();
        RawPreprocessing::from_settings(&settings()).apply(&mut expected, false, false);
        let actual = super::keyed_legacy_pixels(
            &bytes,
            "not-on-disk.png",
            LegacyPreparedReason::NonRaw,
            &settings(),
            None,
        )
        .unwrap();
        assert_eq!(actual.dimensions(), (3, 2));
        assert_eq!(
            actual
                .to_rgb32f()
                .as_raw()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected
                .to_rgb32f()
                .as_raw()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
    }
    // BEGIN C3b fixed eight preparation groups; definitions only at source delivery.
    mod captured_preparation_tests {
        use super::*;
        use crate::progressive_open::{
            JobKind, OpenCandidate, OpenError, OpenKey, OpenOwner, PreparationObservation,
            WorkerContext,
        };
        use std::sync::{Mutex, Weak, atomic::AtomicBool, mpsc};
        use std::time::Duration;
        use tokio::sync::oneshot;

        const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        fn owner() -> Arc<OpenOwner> {
            Arc::new(OpenOwner::new(policy()))
        }
        fn directory() -> std::path::PathBuf {
            let p = std::env::temp_dir().join(format!("c3b-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&p).unwrap(); // Retain private synthetic evidence.
            p
        }
        fn file(bytes: &[u8], suffix: &str) -> String {
            let p = directory().join(format!("generated.{suffix}"));
            fs::write(&p, bytes).unwrap();
            p.to_str().unwrap().to_owned()
        }
        fn metadata(value: Value) -> ImageMetadata {
            ImageMetadata {
                rating: 3,
                adjustments: value,
                ..Default::default()
            }
        }
        fn png_bytes() -> Vec<u8> {
            let image = DynamicImage::ImageRgb8(image::RgbImage::from_fn(3, 2, |x, y| {
                image::Rgb([(x * 60) as u8, (y * 90) as u8, 255])
            }));
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        }
        fn black_dng() -> Vec<u8> {
            let mut b = synthetic_dng(true, 6);
            let start = b.len() - 512 * 520 * 2;
            for sample in b[start..].chunks_exact_mut(2) {
                sample.copy_from_slice(&64u16.to_le_bytes());
            }
            b
        }
        fn selected() -> (
            Arc<crate::dcp::registry::ProfileRegistry>,
            String,
            std::path::PathBuf,
            std::path::PathBuf,
        ) {
            let dir = directory();
            let dcp = dir.join("base.dcp");
            let look_path = dir.join("look.xmp");
            write_synthetic_dcp(&dcp, "RapidRAW Integration Camera", "Composition base");
            synthetic_look_file(&look_path);
            let registry = Arc::new(registry());
            let entry = crate::dcp::registry::parse_dcp_entry(
                &dcp,
                crate::dcp::registry::ProfileSource::Managed,
            )
            .unwrap();
            let id = entry.id.to_hex();
            registry.insert_dcp(entry);
            let look = crate::dcp::look_xmp::parse_look_xmp(&look_path).unwrap();
            registry.register_look(crate::dcp::registry::LookMetadata {
                uuid: look.uuid,
                name: look.name,
                group: look.group,
                required_base_profile: look.required_base_profile,
                camera_model_restriction: look.camera_model_restriction,
                file_path: look_path.clone(),
            });
            (registry, id, dcp, look_path)
        }
        async fn admit(
            owner: &OpenOwner,
            path: String,
            value: ImageMetadata,
            settings: AppSettings,
            captured: bool,
        ) -> OpenKey {
            let key = owner.reserve(A, path).unwrap();
            if captured {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        move |c| c.capture_snapshot(value, settings),
                        |c, p| c.finish_captured_snapshot(p),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
            } else {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        move |_| Ok((value, settings)),
                        |c, (v, s)| c.finish_snapshot(v, s),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
            }
            key
        }
        type Images = (
            crate::progressive_open::KeyedLoadResult,
            (LoadedImage, DynamicImage),
        );
        fn finish(
            c: &WorkerContext,
            candidate: OpenCandidate,
        ) -> std::result::Result<Images, OpenError> {
            let appearance = candidate.images_for_test().1.clone();
            c.commit(candidate, move |image| {
                Ok((image.take().unwrap(), appearance))
            })
        }
        async fn render(
            owner: &OpenOwner,
            key: OpenKey,
            registry: Arc<crate::dcp::registry::ProfileRegistry>,
        ) -> std::result::Result<Images, OpenError> {
            tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Pixels,
                    move |c| read_keyed_candidate(c, &registry),
                    finish,
                ),
            )
            .await
            .unwrap()
        }
        fn counts(owner: &OpenOwner) -> Arc<[AtomicUsize; 3]> {
            let counts = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
            let seen = Arc::clone(&counts);
            owner.observe_preparation_for_test(Arc::new(move |stage| {
                let index = match stage {
                    PreparationObservation::Read => 0,
                    PreparationObservation::Factory => 1,
                    PreparationObservation::Render => 2,
                };
                seen[index].fetch_add(1, Ordering::SeqCst);
            }));
            counts
        }

        #[tokio::test]
        async fn genuine_native_read_factory_and_same_spec_owner() {
            let owner = owner();
            let path = file(&synthetic_dng(true, 6), "dng");
            let original = metadata(json!({}));
            let expected = serde_json::to_value(&original).unwrap();
            let key = admit(&owner, path, original, settings(), true).await;
            let count = counts(&owner);
            let registry = Arc::new(registry());
            let (context_send, context_recv) = mpsc::channel();
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Pixels,
                    move |c| {
                        let actual = c.pixels_spec()?;
                        assert!(Arc::ptr_eq(&actual, &c.spec()?));
                        let candidate = read_keyed_candidate(c, &registry)?;
                        assert!(Arc::ptr_eq(&actual, &c.pixels_spec()?));
                        context_send
                            .send(c.clone())
                            .unwrap_or_else(|_| panic!("context observer lost"));
                        Ok(candidate)
                    },
                    finish,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                count.each_ref().map(|x| x.load(Ordering::SeqCst)),
                [1, 1, 1]
            );
            assert!(
                context_recv
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .pixels_spec()
                    .is_err()
            );
            assert!(result.1.0.prepared_source.is_some());
            assert_eq!(result.1.0.image.dimensions(), (520, 512));
            assert_eq!(serde_json::to_value(result.0.metadata).unwrap(), expected);
            let held = owner.accurate_snapshot(Some(&key)).unwrap();
            assert_eq!(held.identity(), held.spec().identity());
            assert_eq!(held.initial_appearance().as_bytes(), result.1.1.as_bytes());
            assert!(held.is_current().is_ok());
            // These counters observe real adapter attempts, not decoder.raw_image.
        }

        #[tokio::test]
        async fn cancellation_wins_actual_read_factory_and_render_errors() {
            let pre = owner();
            let key = admit(
                &pre,
                "pre-cancel-missing.dng".into(),
                metadata(json!({})),
                settings(),
                true,
            )
            .await;
            let attempted = counts(&pre);
            pre.cancel(key).unwrap();
            assert!(matches!(
                render(&pre, key, Arc::new(registry())).await,
                Err(OpenError::Cancelled)
            ));
            assert_eq!(
                attempted.each_ref().map(|n| n.load(Ordering::SeqCst)),
                [0, 0, 0]
            );
            for case in ["read", "decode", "profile", "render"] {
                for cancelled in [false, true] {
                    let owner = owner();
                    let path = if case == "read" {
                        directory().join("missing.dng").to_str().unwrap().to_owned()
                    } else if case == "decode" {
                        file(b"invalid real source bytes", "dng")
                    } else {
                        file(&synthetic_dng(true, 6), "dng")
                    };
                    let input = match case {
                        "profile" => json!({"profile":{"look":"missing"}}),
                        "render" => {
                            let mask = patch_contract_png(DynamicImage::ImageLuma8(
                                image::GrayImage::from_pixel(2, 2, image::Luma([128])),
                            ));
                            patch_contract_adjustments(Some(&mask), Some((1, 1)), (0, 0))
                        }
                        _ => json!({}),
                    };
                    let key = admit(&owner, path, metadata(input), settings(), true).await;
                    if !cancelled {
                        assert!(matches!(
                            render(&owner, key, Arc::new(registry())).await,
                            Err(OpenError::DevelopmentFailed)
                        ));
                        continue;
                    }
                    let stage = match case {
                        "read" => PreparationObservation::Read,
                        "render" => PreparationObservation::Render,
                        _ => PreparationObservation::Factory,
                    };
                    let (entered, rx) = oneshot::channel();
                    let entered = Mutex::new(Some(entered));
                    let (release, wait) = mpsc::channel();
                    let wait = Mutex::new(wait);
                    owner.observe_preparation_for_test(Arc::new(move |observed| {
                        if observed == stage {
                            entered.lock().unwrap().take().unwrap().send(()).unwrap();
                            wait.lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(2))
                                .unwrap();
                        }
                    }));
                    let runner = Arc::clone(&owner);
                    let job =
                        tokio::spawn(
                            async move { render(&runner, key, Arc::new(registry())).await },
                        );
                    tokio::time::timeout(Duration::from_secs(2), rx)
                        .await
                        .unwrap()
                        .unwrap();
                    owner.cancel(key).unwrap();
                    release.send(()).unwrap();
                    assert!(matches!(
                        tokio::time::timeout(Duration::from_secs(3), job)
                            .await
                            .unwrap()
                            .unwrap(),
                        Err(OpenError::Cancelled)
                    ));
                    assert!(owner.accurate_snapshot(Some(&key)).is_err());
                }
            }
        }

        #[tokio::test]
        async fn genuine_eligibility_controls_deferred_capture() {
            for (input, captured, error) in [
                (json!({}), true, None),
                (
                    json!({"fogEnabled":true}),
                    true,
                    Some(OpenError::DevelopmentFailed),
                ),
                (json!({}), false, Some(OpenError::Unavailable)),
                (
                    json!({"masks":[{"visible":true}]}),
                    true,
                    Some(OpenError::DevelopmentFailed),
                ),
            ] {
                let owner = owner();
                let path = file(&synthetic_dng(true, 6), "dng");
                let key = admit(&owner, path, metadata(input), settings(), captured).await;
                let result = render(&owner, key, Arc::new(registry())).await;
                if let Some(error) = error {
                    assert!(matches!(result,Err(actual) if actual==error));
                } else {
                    assert!(result.unwrap().1.0.prepared_source.is_some());
                }
            }
            for malformed in [false, true] {
                let owner = owner();
                let path = file(&synthetic_dng(true, 6), "dng");
                let (_, sidecar) = parse_virtual_path(&path);
                if malformed {
                    fs::write(&sidecar, b"{not-json").unwrap();
                }
                let key = owner.reserve(A, path).unwrap();
                let admission = tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        |c| capture_keyed_snapshot(c, settings()),
                        |c, p| c.finish_captured_snapshot(p),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                assert!(admission.metadata.adjustments.is_null());
                let result = render(&owner, key, Arc::new(registry())).await.unwrap();
                assert!(result.0.metadata.adjustments.is_null());
                assert!(result.1.0.prepared_source.is_some());
                let held = owner.accurate_snapshot(Some(&key)).unwrap();
                assert!(held.spec().metadata().adjustments.is_null());
                assert_eq!(
                    held.spec().captured_creative().unwrap().adjustments(),
                    &json!({})
                );
                if malformed {
                    assert_eq!(fs::read(&sidecar).unwrap(), b"{not-json");
                } else {
                    assert!(!sidecar.exists());
                }
            }
            for native_bayer in [true, false] {
                for captured in [false, true] {
                    let bytes = if native_bayer {
                        synthetic_dng(false, 1)
                    } else {
                        png_bytes()
                    };
                    let path = file(&bytes, if native_bayer { "dng" } else { "png" });
                    let owner = owner();
                    let key = admit(
                        &owner,
                        path.clone(),
                        metadata(json!({"fogEnabled":true})),
                        settings(),
                        captured,
                    )
                    .await;
                    tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let expected = if captured { OpenError::DevelopmentFailed } else { OpenError::Unavailable };
                            assert!(matches!(c.pixels_spec()?.captured_creative(), Err(actual) if actual == expected));
                            let r = registry();
                            let decision = prepare_captured_decision(c, &bytes, &r)?;
                            assert!(if native_bayer {
                                matches!(&decision, KeyedPreparation::DecodedLegacy { .. })
                            } else {
                                matches!(
                                    &decision,
                                    KeyedPreparation::LegacyRequired {
                                        reason: LegacyPreparedReason::NonRaw,
                                        ..
                                    }
                                )
                            });
                            let old = prepare_keyed_candidate(
                                &bytes,
                                &path,
                                c.pixels_spec()?,
                                &r,
                                c.raw_cancel(),
                            )?;
                            let candidate = decision.into_candidate(c, &bytes)?;
                            assert_eq!(
                                old.images_for_test().0.as_bytes(),
                                candidate.images_for_test().0.as_bytes()
                            );
                            assert_eq!(
                                old.images_for_test().1.as_bytes(),
                                candidate.images_for_test().1.as_bytes()
                            );
                            Ok(candidate)
                        },
                        finish,
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                }
            }
            let owner = owner();
            let mut s = settings();
            s.use_apple_raw9 = Some(true);
            let key = admit(
                &owner,
                "explicit-apple.dng".into(),
                metadata(json!({"fogEnabled":true})),
                s,
                true,
            )
            .await;
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Pixels,
                    |c| {
                        assert!(matches!(
                            prepare_captured_decision(c, b"not decoded", &registry())?,
                            KeyedPreparation::LegacyRequired {
                                reason: LegacyPreparedReason::AppleRawPreferred,
                                ..
                            }
                        ));
                        Ok(()) // Decision-only probe intentionally never renders or commits Apple.
                    },
                    |_, ()| Err::<(), _>(OpenError::SnapshotFailed),
                ),
            )
            .await
            .unwrap();
            assert_eq!(result, Err(OpenError::SnapshotFailed));
        }

        #[tokio::test]
        async fn foreign_snapshot_stale_and_superseded_contexts_reject() {
            let probe = owner();
            let key = probe.reserve(A, "snapshot.dng".into()).unwrap();
            tokio::time::timeout(
                Duration::from_secs(3),
                probe.execute(
                    key,
                    JobKind::Snapshot,
                    |c| {
                        assert!(c.pixels_spec().is_err());
                        c.capture_snapshot(metadata(json!({})), settings())
                    },
                    |c, p| c.finish_captured_snapshot(p),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            for foreign in [true, false] {
                let donor = owner();
                let bytes = synthetic_dng(true, 6);
                let path = file(&bytes, "dng");
                let key = admit(&donor, path.clone(), metadata(json!({})), settings(), true).await;
                let (send, recv) = mpsc::channel();
                let factory_bytes = bytes.clone();
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    donor.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let pair = prepare_captured_decision(c, &factory_bytes, &registry())?;
                            send.send((pair, c.clone()))
                                .unwrap_or_else(|_| panic!("pair receiver lost"));
                            Ok(())
                        },
                        |_, ()| Err::<(), _>(OpenError::SnapshotFailed),
                    ),
                )
                .await
                .unwrap();
                assert_eq!(result, Err(OpenError::SnapshotFailed));
                let (pair, completed) = recv.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(completed.pixels_spec().is_err());
                let target = if foreign { owner() } else { Arc::clone(&donor) };
                if !foreign {
                    target.reserve(B, "other.dng".into()).unwrap();
                }
                let fresh = admit(&target, path, metadata(json!({})), settings(), true).await;
                if foreign {
                    assert_eq!(fresh, key);
                } else {
                    assert_ne!(fresh, key);
                }
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    target.execute(
                        fresh,
                        JobKind::Pixels,
                        move |c| pair.into_candidate(c, &bytes),
                        finish,
                    ),
                )
                .await
                .unwrap();
                assert!(matches!(result, Err(OpenError::Stale)));
                assert!(target.accurate_snapshot(Some(&fresh)).is_err());
            }
        }

        #[tokio::test]
        async fn held_metadata_settings_selected_assets_and_hydration_survive_mutation() {
            let (registry, id, dcp, look) = selected();
            let bytes = synthetic_dng(true, 6);
            let path = file(&bytes, "dng");
            let cube = directory().join("held.cube");
            fs::write(&cube, b"LUT_3D_SIZE 1\n0.25 0.5 1\n").unwrap();
            let map = patch_contract_png(DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
                2,
                2,
                image::Luma([128]),
            )));
            let input = json!({"profile":{"base":id,"look":"composition-look","amount":100},"lutPath":cube.to_str().unwrap(),"fogEnabled":true,"fogDepthMap":map,"exposure":1.25});
            let original = metadata(input);
            let (_, sidecar) = parse_virtual_path(&path);
            fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
            let owner = owner();
            let key = owner.reserve(A, path.clone()).unwrap();
            let mut caller = settings();
            let saved = caller.clone();
            tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Snapshot,
                    move |c| capture_keyed_snapshot(c, saved),
                    |c, p| c.finish_captured_snapshot(p),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            caller.raw_preprocessing_color_nr = Some(1.0);
            fs::write(
                &sidecar,
                serde_json::to_vec(&metadata(json!({"exposure":99.0}))).unwrap(),
            )
            .unwrap();
            fs::write(&cube, b"later bad bytes").unwrap();
            fs::remove_file(&cube).unwrap();
            tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Pixels,
                    move |c| {
                        let decision = prepare_captured_decision(c, &bytes, &registry)?;
                        let KeyedPreparation::Prepared(pair) = &decision else {
                            panic!("real native pair required")
                        };
                        let spec = c.pixels_spec()?;
                        assert!(Arc::ptr_eq(&spec, &pair.spec));
                        assert_eq!(spec.metadata().adjustments["exposure"], 1.25);
                        assert_eq!(spec.settings().raw_preprocessing_color_nr, Some(0.0));
                        let assets = spec.captured_creative()?;
                        assert_eq!(assets.lut().unwrap().data, vec![0.25, 0.5, 1.0]);
                        assert!(assets.fog_map().unwrap().as_raw().iter().all(|v| *v == 128));
                        let before = pair
                            .appearance
                            .render_unpatched_base(AppearancePass::AccurateOne, c.raw_cancel())
                            .unwrap()
                            .pixels()
                            .clone();
                        fs::write(&dcp, b"later invalid profile").unwrap();
                        fs::remove_file(&dcp).unwrap();
                        fs::remove_file(&look).unwrap();
                        let after = pair
                            .appearance
                            .render_unpatched_base(AppearancePass::AccurateOne, c.raw_cancel())
                            .unwrap();
                        assert_eq!(before.as_bytes(), after.pixels().as_bytes());
                        drop(after);
                        let candidate = decision.into_candidate(c, &bytes)?;
                        assert_eq!(candidate.images_for_test().1.as_bytes(), before.as_bytes());
                        Ok(candidate)
                    },
                    finish,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(caller.raw_preprocessing_color_nr, Some(1.0));
        }

        #[tokio::test]
        async fn hydrated_single_patch_literal_and_uncovered_pixels() {
            let bytes = black_dng();
            let path = file(&bytes, "dng");
            let mut input = patch_contract_adjustments(None, Some((1, 1)), (10, 10));
            let bitmap = patch_contract_png(DynamicImage::ImageLuma8(
                image::GrayImage::from_pixel(520, 512, image::Luma([128])),
            ));
            input["aiPatches"][0]["subMasks"] = json!([{"id":"bitmap","type":"ai-sky","visible":true,"invert":false,"opacity":100.0,"mode":"additive","parameters":{"mask_data_base64":bitmap}}]);
            let owner = owner();
            let key = admit(&owner, path, metadata(input), settings(), true).await;
            tokio::time::timeout(Duration::from_secs(3),owner.execute(key,JobKind::Pixels,move |c| {
                let candidate=read_keyed_candidate(c,&registry())?; let spec=c.pixels_spec()?;
                let (base,actual)=candidate.images_for_test();
                assert!(base.to_rgb32f().as_raw().iter().all(|v|*v==0.0));
                assert_eq!(base.dimensions(),(520,512));
                let view=spec.captured_creative()?.adjustments();
                assert!(spec.metadata().adjustments["aiPatches"][0]["subMasks"][0]["parameters"]["maskDataBase64"].is_null());
                assert!(view["aiPatches"][0]["subMasks"][0]["parameters"]["maskDataBase64"].is_string());
                let once=composite_patches_on_image(base,view).unwrap();
                let twice=composite_patches_on_image(&once,view).unwrap();
                assert_eq!(actual.as_bytes(),once.as_bytes());
                assert_ne!(actual.as_bytes(),base.as_bytes()); assert_ne!(actual.as_bytes(),twice.as_bytes());
                let original=composite_patches_on_image(base,&spec.metadata().adjustments).unwrap();
                assert_eq!(original.as_bytes(),base.as_bytes()); // Tolerant old unresolved child gives zero mask, not a parser error.
                let pixels=actual.to_rgb32f(); let pristine=base.to_rgb32f();
                assert_eq!(pixels.get_pixel(10,10).0.map(f32::to_bits),[1056997505,0,0]);
                for (x,y,p) in pixels.enumerate_pixels() { if (x,y)!=(10,10) { assert_eq!(p.0.map(f32::to_bits),pristine.get_pixel(x,y).0.map(f32::to_bits)); } }
                Ok(candidate)
            },finish)).await.unwrap().unwrap();
        }

        #[tokio::test]
        async fn once_only_filtered_bases_and_currentfull_zero() {
            for amount in [None, Some(0), Some(100), Some(200)] {
                let (registry, id, _, _) = selected();
                let bytes = patterned_composition_dng();
                let path = file(&bytes, "dng");
                let mask = patch_contract_png(DynamicImage::ImageLuma8(
                    image::GrayImage::from_pixel(1, 1, image::Luma([128])),
                ));
                let mut input = patch_contract_adjustments(Some(&mask), Some((1, 1)), (0, 0));
                if let Some(amount) = amount {
                    input["profile"] = json!({"base":id,"look":"composition-look","amount":amount});
                }
                let mut s = settings();
                s.raw_preprocessing_color_nr = Some(0.5);
                s.raw_preprocessing_sharpening = Some(0.35);
                let owner = owner();
                let key = admit(&owner, path, metadata(input), s, true).await;
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let decision = prepare_captured_decision(c, &bytes, &registry)?;
                            let KeyedPreparation::Prepared(pair) = &decision else {
                                panic!("native pair")
                            };
                            ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                            let current = pair
                                .appearance
                                .render_unpatched_base(AppearancePass::CurrentFull, c.raw_cancel())
                                .unwrap();
                            assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                            let patched_current = composite_patches_on_image(
                                current.pixels(),
                                pair.spec.captured_creative()?.adjustments(),
                            )
                            .unwrap();
                            assert_ne!(current.pixels().as_bytes(), patched_current.as_bytes());
                            drop(current);
                            let source = &pair.appearance.source;
                            let (raw, orientation, _) =
                                crate::raw_processing::develop_prepared_raw(
                                    &source.prepared,
                                    source.quality(AppearancePass::AccurateOne),
                                    2.5,
                                    &source.settings.linear_raw_mode,
                                    false,
                                    c.raw_cancel(),
                                )
                                .unwrap();
                            let raw = apply_orientation(raw, orientation);
                            let mut expected_pristine =
                                crate::false_colour::suppress_linear_srgb_v1(raw, &c.raw_cancel())
                                    .unwrap();
                            RawPreprocessing::from_settings(&source.settings).apply(
                                &mut expected_pristine,
                                true,
                                false,
                            );
                            let expected_base = if let Some(assets) = &pair.appearance.assets {
                                let working =
                                    crate::dcp::pipeline::render_prepared_selected_profile(
                                        &source.prepared,
                                        assets,
                                        source.quality(AppearancePass::AccurateOne),
                                        2.5,
                                        &source.settings.linear_raw_mode,
                                        c.raw_cancel(),
                                    )
                                    .unwrap();
                                crate::false_colour::suppress_linear_srgb_v1(
                                    working,
                                    &c.raw_cancel(),
                                )
                                .unwrap()
                            } else {
                                expected_pristine.clone()
                            };
                            let expected = composite_patches_on_image(
                                &expected_base,
                                pair.spec.captured_creative()?.adjustments(),
                            )
                            .unwrap();
                            ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                            let candidate = decision.into_candidate(c, &bytes)?;
                            assert_eq!(
                                ACCURATE_FILTER_CALLS.with(|n| n.get()),
                                if amount.is_some() { 2 } else { 1 }
                            );
                            assert_eq!(
                                candidate.images_for_test().0.as_bytes(),
                                expected_pristine.as_bytes()
                            );
                            assert_eq!(
                                candidate.images_for_test().1.as_bytes(),
                                expected.as_bytes()
                            );
                            Ok(candidate)
                        },
                        finish,
                    ),
                )
                .await
                .unwrap()
                .unwrap();
            }
        }

        #[tokio::test]
        async fn compatibility_and_failure_retirement_remain_exact() {
            for kind in ["native", "bayer", "png"] {
                let bytes = match kind {
                    "native" => synthetic_dng(true, 6),
                    "bayer" => synthetic_dng(false, 1),
                    _ => png_bytes(),
                };
                let path = file(&bytes, if kind == "png" { "png" } else { "dng" });
                let owner = owner();
                let key = admit(&owner, path.clone(), metadata(json!({})), settings(), true).await;
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let r = registry();
                            let old = prepare_keyed_candidate(
                                &bytes,
                                &path,
                                c.pixels_spec()?,
                                &r,
                                c.raw_cancel(),
                            )?;
                            let actual = prepare_captured_decision(c, &bytes, &r)?
                                .into_candidate(c, &bytes)?;
                            assert_eq!(
                                old.images_for_test().0.as_bytes(),
                                actual.images_for_test().0.as_bytes()
                            );
                            assert_eq!(
                                old.images_for_test().1.as_bytes(),
                                actual.images_for_test().1.as_bytes()
                            );
                            Ok(actual)
                        },
                        finish,
                    ),
                )
                .await
                .unwrap()
                .unwrap();
            }
            for mode in ["drop", "supersede", "close"] {
                let owner = owner();
                let path = file(&synthetic_dng(true, 6), "dng");
                let key = owner.reserve(A, path).unwrap();
                let dropped = Arc::new(AtomicBool::new(false));
                let unlocked = Arc::new(AtomicBool::new(false));
                let d = Arc::clone(&dropped);
                let u = Arc::clone(&unlocked);
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        move |c| {
                            let mut package =
                                c.capture_snapshot(metadata(json!({})), settings())?;
                            package.observe_retirement_for_test(d, u);
                            Ok(package)
                        },
                        |c, p| c.finish_captured_snapshot(p),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                let (entered, rx) = oneshot::channel();
                let (release, wait) = mpsc::channel();
                let runner = Arc::clone(&owner);
                let mut job = Some(tokio::spawn(async move {
                    runner
                        .execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let (path, _) = parse_virtual_path(c.path());
                                let bytes = fs::read(path).unwrap();
                                let decision = prepare_captured_decision(c, &bytes, &registry())?;
                                let KeyedPreparation::Prepared(pair) = &decision else {
                                    panic!("native pair")
                                };
                                entered
                                    .send((
                                        Arc::downgrade(&pair.spec),
                                        Arc::downgrade(&pair.appearance.source),
                                    ))
                                    .unwrap_or_else(|_| panic!("retirement observer lost"));
                                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                                decision.into_candidate(c, &bytes)
                            },
                            finish,
                        )
                        .await
                }));
                let (spec_weak, source_weak): (
                    Weak<crate::progressive_open::OpenSpec>,
                    Weak<RetainedPreparedSource>,
                ) = tokio::time::timeout(Duration::from_secs(2), rx)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(source_weak.upgrade().is_some());
                if mode == "close" {
                    owner.close();
                    assert!(matches!(
                        owner.reserve(B, "closed.dng".into()),
                        Err(OpenError::Unavailable)
                    ));
                    release.send(()).unwrap();
                    assert!(
                        tokio::time::timeout(Duration::from_secs(3), job.take().unwrap())
                            .await
                            .unwrap()
                            .unwrap()
                            .is_err()
                    );
                } else {
                    if mode == "drop" {
                        job.as_ref().unwrap().abort();
                        match tokio::time::timeout(Duration::from_secs(3), job.take().unwrap())
                            .await
                            .unwrap()
                        {
                            Err(e) => assert!(e.is_cancelled()),
                            Ok(_) => panic!("caller did not drop"),
                        }
                    }
                    let latest = owner.reserve(B, "latest.dng".into()).unwrap();
                    let calls = Arc::new(AtomicUsize::new(0));
                    let seen = Arc::clone(&calls);
                    let (started, started_rx) = oneshot::channel();
                    let runner = Arc::clone(&owner);
                    let next = tokio::spawn(async move {
                        let future = runner.execute(
                            latest,
                            JobKind::Snapshot,
                            move |c| {
                                seen.fetch_add(1, Ordering::SeqCst);
                                c.capture_snapshot(metadata(json!({})), settings())
                            },
                            |c, p| c.finish_captured_snapshot(p),
                        );
                        started.send(()).unwrap();
                        future.await
                    });
                    tokio::time::timeout(Duration::from_secs(2), started_rx)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(calls.load(Ordering::SeqCst), 0);
                    assert!(!dropped.load(Ordering::SeqCst));
                    assert!(source_weak.upgrade().is_some());
                    release.send(()).unwrap();
                    if let Some(job) = job.take() {
                        assert!(
                            tokio::time::timeout(Duration::from_secs(3), job)
                                .await
                                .unwrap()
                                .unwrap()
                                .is_err()
                        );
                    }
                    assert_eq!(
                        tokio::time::timeout(Duration::from_secs(3), next)
                            .await
                            .unwrap()
                            .unwrap()
                            .unwrap()
                            .open_key,
                        latest
                    );
                    assert_eq!(calls.load(Ordering::SeqCst), 1);
                }
                assert!(dropped.load(Ordering::SeqCst));
                assert!(unlocked.load(Ordering::SeqCst));
                assert!(spec_weak.upgrade().is_none());
                assert!(source_weak.upgrade().is_none());
                assert!(owner.gate_available_for_test());
            }
        }
        // Fresh-RAW camera default inside the genuine keyed open. Definitions only
        // until the root native gate; real generated DNG/DCP/Look fixtures.
        mod initial_profile_default_tests {
            use super::*;
            use crate::progressive_open::InitialProfileDefault;

            const MODULAR: &str = "Cobalt Modular";

            /// Existing synthetic DCP/Look constructors renamed into the Modular
            /// system; extra bases differ only in content, never camera or name.
            fn modular_registry(
                bases: usize,
                witness: bool,
            ) -> (Arc<crate::dcp::registry::ProfileRegistry>, Vec<String>) {
                let dir = directory();
                let registry = Arc::new(registry());
                let mut ids = Vec::new();
                for index in 0..bases {
                    let dcp = dir.join(format!("modular-{index}.dcp"));
                    write_synthetic_dcp(&dcp, "RapidRAW Integration Camera", MODULAR);
                    if index > 0 {
                        let mut bytes = fs::read(&dcp).unwrap();
                        bytes.push(index as u8);
                        fs::write(&dcp, bytes).unwrap();
                    }
                    let entry = crate::dcp::registry::parse_dcp_entry(
                        &dcp,
                        crate::dcp::registry::ProfileSource::Managed,
                    )
                    .unwrap();
                    ids.push(entry.id.to_hex());
                    registry.insert_dcp(entry);
                }
                if witness {
                    let look_path = dir.join("modular-witness-look.xmp");
                    synthetic_look_file(&look_path);
                    let xml = fs::read_to_string(&look_path)
                        .unwrap()
                        .replace(
                            r#"crs:CameraProfile="Composition base""#,
                            &format!(r#"crs:CameraProfile="{MODULAR}""#),
                        )
                        .replace(
                            r#"crs:UUID="composition-look""#,
                            r#"crs:UUID="modular-witness""#,
                        );
                    fs::write(&look_path, xml).unwrap();
                    let look = crate::dcp::look_xmp::parse_look_xmp(&look_path).unwrap();
                    assert_eq!(look.required_base_profile, MODULAR);
                    registry.register_look(crate::dcp::registry::LookMetadata {
                        uuid: look.uuid,
                        name: look.name,
                        group: look.group,
                        required_base_profile: look.required_base_profile,
                        camera_model_restriction: look.camera_model_restriction,
                        file_path: look_path,
                    });
                }
                (registry, ids)
            }

            async fn fresh_snapshot(owner: &OpenOwner, path: String, s: AppSettings) -> OpenKey {
                let key = owner.reserve(A, path).unwrap();
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        move |c| capture_keyed_snapshot(c, s),
                        |c, p| c.finish_captured_snapshot(p),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                key
            }

            async fn open_default(
                path: String,
                registry: Arc<crate::dcp::registry::ProfileRegistry>,
                s: AppSettings,
            ) -> std::result::Result<Option<InitialProfileDefault>, OpenError> {
                let o = owner();
                let key = fresh_snapshot(&o, path, s).await;
                tokio::time::timeout(
                    Duration::from_secs(20),
                    o.execute(
                        key,
                        JobKind::Pixels,
                        move |c| read_keyed_candidate(c, &registry),
                        finish,
                    ),
                )
                .await
                .unwrap()
                .map(|images| images.0.initial_profile_default)
            }

            #[tokio::test]
            async fn fresh_raw_open_holds_one_default_base_with_immutable_original() {
                for xtrans in [true, false] {
                    let (registry, ids) = modular_registry(1, true);
                    let bytes = synthetic_dng(xtrans, 1);
                    let path = file(&bytes, "dng");
                    let (_, sidecar) = parse_virtual_path(&path);
                    let o = owner();
                    let key = fresh_snapshot(&o, path, settings()).await;
                    let expected = json!({"base": ids[0], "look": null, "amount": 100});
                    let want = expected.clone();
                    let r = Arc::clone(&registry);
                    let result = tokio::time::timeout(
                        Duration::from_secs(20),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let spec = c.pixels_spec()?;
                                assert!(spec.initial_profile_default_allowed());
                                assert!(spec.metadata().adjustments.is_null());
                                let decision = prepare_captured_decision(c, &bytes, &r)?;
                                match &decision {
                                    KeyedPreparation::Prepared(pair) => {
                                        assert!(xtrans, "X-Trans retains the prepared owner");
                                        assert!(pair.appearance.assets.is_some());
                                        assert_eq!(pair.appearance.adjustments()["profile"], want);
                                        assert!(pair.spec.metadata().adjustments.is_null());
                                    }
                                    KeyedPreparation::DecodedLegacy { spec, appearance } => {
                                        assert!(!xtrans, "Bayer CFA keeps its decoded owner");
                                        assert!(appearance.assets.is_some());
                                        assert_eq!(appearance.inputs.adjustments["profile"], want);
                                        assert!(spec.metadata().adjustments.is_null());
                                    }
                                    KeyedPreparation::LegacyRequired { .. } => {
                                        panic!("genuine decoded RAW route required")
                                    }
                                }
                                decision.into_candidate(c, &bytes)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert!(result.0.metadata.adjustments.is_null());
                    assert_eq!(
                        result.0.initial_profile_default,
                        Some(InitialProfileDefault {
                            base: ids[0].clone()
                        })
                    );
                    let wire = serde_json::to_value(&result.0).unwrap();
                    assert_eq!(wire["initialProfileDefault"], json!({"base": ids[0]}));
                    if xtrans {
                        let held = o.accurate_snapshot(Some(&key)).unwrap();
                        assert!(held.spec().metadata().adjustments.is_null());
                        let retained = held.retained().unwrap();
                        assert!(retained.appearance.assets.is_some());
                        assert_eq!(retained.appearance.adjustments()["profile"], expected);
                        let consumer = ConsumerRequest::capture(
                            crate::app_state::ConsumerSource::Accurate(
                                o.accurate_snapshot(Some(&key)).unwrap(),
                            ),
                            None,
                            settings(),
                            &registry,
                        )
                        .unwrap();
                        assert!(consumer.check().is_ok());
                        assert_eq!(consumer.key(), Some(key));
                        assert_eq!(consumer.adjustments()["profile"], expected);
                        assert!(matches!(
                            consumer.view.as_ref(),
                            Some(RequestView::Initial(initial)) if Arc::ptr_eq(initial, retained)
                        ));
                        assert!(consumer.appearance().unwrap().assets.is_some());
                        assert!(held.spec().metadata().adjustments.is_null());
                    }
                    assert!(!sidecar.exists(), "viewing never writes a sidecar");
                }
            }

            #[tokio::test]
            async fn default_open_adds_no_development_pass_per_phase() {
                for xtrans in [true, false] {
                    let (registry, ids) = modular_registry(1, true);
                    let explicit =
                        json!({"profile": {"base": ids[0], "look": null, "amount": 100}});
                    let mut outcomes = Vec::new();
                    for default in [false, true] {
                        let bytes = synthetic_dng(xtrans, 1);
                        let path = file(&bytes, "dng");
                        let o = owner();
                        let key = if default {
                            fresh_snapshot(&o, path, settings()).await
                        } else {
                            admit(&o, path, metadata(explicit.clone()), settings(), true).await
                        };
                        let authority = Arc::clone(&o);
                        let r = Arc::clone(&registry);
                        let phases = Arc::new(Mutex::new(([0usize; 3], [0usize; 3])));
                        let seen = Arc::clone(&phases);
                        let result = tokio::time::timeout(
                            Duration::from_secs(20),
                            o.execute(
                                key,
                                JobKind::Pixels,
                                move |c| {
                                    assert_eq!(
                                        c.pixels_spec()?.initial_profile_default_allowed(),
                                        default
                                    );
                                    let decision = prepare_captured_decision(c, &bytes, &r)?;
                                    let count = |reset: bool| {
                                        crate::raw_processing::PREPARED_DEVELOPMENTS.with(|n| {
                                            let value = n.get();
                                            if reset {
                                                n.set([0; 3]);
                                            }
                                            value
                                        })
                                    };
                                    count(true);
                                    if let KeyedPreparation::Prepared(pair) = &decision {
                                        pair.with_current_fit(c, &authority, |fit, _| {
                                            Ok(fit.pixels().dimensions())
                                        })?;
                                    }
                                    let fit = count(true);
                                    let candidate = decision.into_candidate(c, &bytes)?;
                                    *seen.lock().unwrap() = (fit, count(true));
                                    Ok(candidate)
                                },
                                finish,
                            ),
                        )
                        .await
                        .unwrap()
                        .unwrap();
                        assert_eq!(result.0.initial_profile_default.is_some(), default);
                        let pixels = result.1.1.to_rgb32f().into_raw();
                        outcomes.push((*phases.lock().unwrap(), pixels));
                    }
                    // Per phase: fit (CurrentFull) and accurate open, never one global call.
                    assert_eq!(outcomes[0].0.0, outcomes[1].0.0, "fit phase");
                    assert_eq!(outcomes[0].0.1, outcomes[1].0.1, "accurate phase");
                    assert_eq!(
                        outcomes[0].1, outcomes[1].1,
                        "same appearance as explicit base"
                    );
                }
            }

            #[tokio::test]
            async fn optional_default_skips_every_ineligible_or_uncertain_state() {
                let (modular, _) = modular_registry(1, true);
                let raw = || synthetic_dng(true, 1);
                let sidecar_cases: [(&str, Option<Vec<u8>>); 3] = [
                    (
                        "present edits",
                        Some(serde_json::to_vec(&metadata(json!({"exposure": 1.0}))).unwrap()),
                    ),
                    (
                        "explicit none",
                        Some(
                            serde_json::to_vec(&metadata(json!({
                                "profile": {"base": null, "look": null, "amount": 100}
                            })))
                            .unwrap(),
                        ),
                    ),
                    ("malformed", Some(b"{not-json".to_vec())),
                ];
                for (label, contents) in sidecar_cases {
                    let path = file(&raw(), "dng");
                    let (_, sidecar) = parse_virtual_path(&path);
                    let original = contents.unwrap();
                    fs::write(&sidecar, &original).unwrap();
                    let outcome = open_default(path, Arc::clone(&modular), settings()).await;
                    assert!(matches!(outcome, Ok(None)), "{label}");
                    assert_eq!(fs::read(&sidecar).unwrap(), original, "{label}: unchanged");
                }
                for (label, extension) in [("stem xmp", None), ("appended xmp", Some(".xmp"))] {
                    let path = file(&raw(), "dng");
                    let source = std::path::PathBuf::from(&path);
                    let xmp = match extension {
                        None => source.with_extension("xmp"),
                        Some(suffix) => std::path::PathBuf::from(format!("{path}{suffix}")),
                    };
                    fs::write(&xmp, b"<x:xmpmeta/>").unwrap();
                    let outcome = open_default(path, Arc::clone(&modular), settings()).await;
                    assert!(matches!(outcome, Ok(None)), "{label}");
                }
                let virtual_copy = format!("{}?vc=copy-1", file(&raw(), "dng"));
                assert!(matches!(
                    open_default(virtual_copy, Arc::clone(&modular), settings()).await,
                    Ok(None)
                ));
                assert!(matches!(
                    open_default(file(&png_bytes(), "png"), Arc::clone(&modular), settings()).await,
                    Ok(None)
                ));
                let mut apple = settings();
                apple.use_apple_raw9 = Some(true);
                assert!(matches!(
                    open_default(file(&raw(), "dng"), Arc::clone(&modular), apple).await,
                    Ok(None)
                ));
                for (label, registry) in [
                    ("no modular", Arc::new(registry())),
                    ("ambiguous", modular_registry(2, true).0),
                    ("no witness", modular_registry(1, false).0),
                ] {
                    let outcome = open_default(file(&raw(), "dng"), registry, settings()).await;
                    assert!(matches!(outcome, Ok(None)), "{label}");
                }
                // A registered asset that disappears only skips the OPTIONAL default.
                let (vanished, ids) = modular_registry(1, true);
                let base = vanished.get_by_hex(&ids[0]).unwrap();
                fs::remove_file(&base.file_path).unwrap();
                assert!(matches!(
                    open_default(file(&raw(), "dng"), vanished, settings()).await,
                    Ok(None)
                ));
            }

            #[tokio::test]
            async fn explicit_errors_cancellation_and_aba_still_win() {
                let (modular, _) = modular_registry(1, true);
                // Explicit saved profile keeps strict error-before-decode precedence.
                let path = file(&synthetic_dng(true, 1), "dng");
                let (_, sidecar) = parse_virtual_path(&path);
                let missing = "ab".repeat(32);
                fs::write(
                    &sidecar,
                    serde_json::to_vec(&metadata(json!({
                        "profile": {"base": missing, "look": null, "amount": 100}
                    })))
                    .unwrap(),
                )
                .unwrap();
                assert!(matches!(
                    open_default(path, Arc::clone(&modular), settings()).await,
                    Err(OpenError::DevelopmentFailed)
                ));
                for aba in [false, true] {
                    let bytes = synthetic_dng(true, 1);
                    let path = file(&bytes, "dng");
                    let other = file(&synthetic_dng(true, 1), "dng");
                    let o = owner();
                    let key = fresh_snapshot(&o, path.clone(), settings()).await;
                    let authority = Arc::clone(&o);
                    let r = Arc::clone(&modular);
                    let outcome = tokio::time::timeout(
                        Duration::from_secs(20),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                if aba {
                                    authority.reserve(B, other).unwrap();
                                    authority.reserve(A, path).unwrap();
                                } else {
                                    authority.cancel(key).unwrap();
                                }
                                read_keyed_candidate(c, &r)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap();
                    assert!(
                        matches!(outcome, Err(OpenError::Cancelled | OpenError::Stale)),
                        "aba={aba}"
                    );
                    assert!(o.accurate_snapshot(Some(&key)).is_err());
                }
            }
        }
        mod consumer_storage_tests {

            #[tokio::test]
            async fn native_fit_initialization_is_classified_and_error_fenced() {
                for kind in ["native-error", "native-cancel", "legacy"] {
                    let o = owner();
                    let bytes = if kind == "legacy" {
                        png_bytes()
                    } else {
                        synthetic_dng(true, 6)
                    };
                    let path = file(&bytes, if kind == "legacy" { "png" } else { "dng" });
                    let k = admit(&o, path, metadata(json!({})), settings(), true).await;
                    let authority = Arc::clone(&o);
                    let n = Arc::new(AtomicUsize::new(0));
                    let seen = Arc::clone(&n);
                    let result = tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            k,
                            JobKind::Pixels,
                            move |c| {
                                read_keyed_candidate_with_fit_initialization(
                                    c,
                                    &registry(),
                                    &authority,
                                    &Mutex::new(None),
                                    || {
                                        seen.fetch_add(1, Ordering::SeqCst);
                                        if kind == "native-cancel" {
                                            authority.reserve(B, "supersede.png".into()).unwrap();
                                        }
                                        Err(OpenError::DevelopmentFailed)
                                    },
                                    |_| panic!("failed init or legacy cannot emit"),
                                )
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap();
                    match kind {
                        "legacy" => {
                            assert!(result.is_ok());
                            assert_eq!(n.load(Ordering::SeqCst), 0);
                        }
                        "native-error" => {
                            assert!(matches!(result, Err(OpenError::DevelopmentFailed)));
                            assert_eq!(n.load(Ordering::SeqCst), 1);
                        }
                        _ => {
                            assert!(matches!(
                                result,
                                Err(OpenError::Stale | OpenError::Cancelled)
                            ));
                            assert_eq!(n.load(Ordering::SeqCst), 1);
                        }
                    }
                }
            }

            #[test]
            fn retained_legacy_fast_matches_byte_wrapper_whole_image() {
                let bytes = synthetic_dng(false, 6);
                let path = file(&bytes, "dng");
                let (registry, id, _, _) = selected();
                let mut settings = settings();
                settings.raw_preprocessing_color_nr = Some(0.5);
                settings.raw_preprocessing_sharpening = Some(0.35);
                for profile in [false, true] {
                    let mut adjustments = patch_contract_adjustments(None, Some((1, 1)), (10, 10));
                    if profile {
                        adjustments["profile"] =
                            json!({"base":id,"look":"composition-look","amount":100});
                    }
                    let expected =
                        load_base_image_from_bytes(&bytes, &path, true, &settings, None).unwrap();
                    let PreparedAppearanceDecision::DecodedLegacy(held) =
                        prepare_estimate_appearance(
                            &bytes,
                            &path,
                            &adjustments,
                            &settings,
                            &registry,
                            policy(),
                            None,
                        )
                        .unwrap()
                    else {
                        panic!("real Bayer owner required")
                    };
                    let (actual, scale) = held.render_estimate_fast(None).unwrap();
                    assert_eq!(
                        scale.to_bits(),
                        crate::raw_processing::get_fast_demosaic_scale_factor(
                            &bytes,
                            expected.width(),
                            expected.height()
                        )
                        .to_bits()
                    );
                    assert_eq!(actual.dimensions(), expected.dimensions());
                    assert_eq!(actual.color(), expected.color());
                    assert_eq!(actual.as_bytes(), expected.as_bytes());
                }
                let invalid = json!({"profile":{"base":"missing"}});
                assert!(matches!(
                    prepare_estimate_appearance(
                        &bytes,
                        &path,
                        &invalid,
                        &settings,
                        &registry,
                        policy(),
                        None
                    )
                    .unwrap(),
                    PreparedAppearanceDecision::DecodedLegacy(_)
                ));
                let native = synthetic_dng(true, 6);
                assert!(
                    prepare_estimate_appearance(
                        &native,
                        &path,
                        &invalid,
                        &settings,
                        &registry,
                        policy(),
                        None
                    )
                    .is_err()
                );
                settings.use_apple_raw9 = Some(true);
                assert!(matches!(
                    prepare_estimate_appearance(
                        &native,
                        &path,
                        &invalid,
                        &settings,
                        &registry,
                        policy(),
                        None
                    )
                    .unwrap(),
                    PreparedAppearanceDecision::DecodedLegacy(_)
                ));
            }
            use super::*;
            use crate::app_state::{ConsumerSource, capture_consumer_source_parts};
            fn request(
                o: &OpenOwner,
                k: OpenKey,
                edit: Option<Value>,
                s: AppSettings,
                r: &crate::dcp::registry::ProfileRegistry,
            ) -> std::result::Result<Arc<ConsumerRequest>, RequestCaptureError> {
                ConsumerRequest::capture(
                    ConsumerSource::Accurate(o.accurate_snapshot(Some(&k)).unwrap()),
                    edit,
                    s,
                    r,
                )
            }
            async fn ready(
                bytes: &[u8],
                input: Value,
                r: Arc<crate::dcp::registry::ProfileRegistry>,
            ) -> (Arc<OpenOwner>, OpenKey, Images) {
                let o = owner();
                let path = file(
                    bytes,
                    if bytes.starts_with(b"\x89PNG") {
                        "png"
                    } else {
                        "dng"
                    },
                );
                let k = admit(&o, path, metadata(input), settings(), true).await;
                let result = render(&o, k, r).await.unwrap();
                (o, k, result)
            }
            #[tokio::test]
            async fn nd03_real_captured_owner_survives_preview_cancel_rejects_new_source() {
                let (registry, id, _, _) = selected();
                let registry = Arc::new(registry);
                let edit = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let (owner, key, _images) =
                    ready(&synthetic_dng(true, 1), edit.clone(), Arc::clone(&registry)).await;
                let preview =
                    request(&owner, key, Some(edit.clone()), settings(), &registry).unwrap();
                let inference =
                    request(&owner, key, Some(edit.clone()), settings(), &registry).unwrap();
                let original = inference.denoise_request_key().unwrap();
                assert_eq!(original, preview.denoise_request_key().unwrap());
                preview.cancel();
                assert!(preview.check().is_err());
                assert!(inference.check().is_ok());
                let mut exposure = edit.clone();
                exposure["exposure"] = json!(2);
                let next = request(&owner, key, Some(exposure), settings(), &registry).unwrap();
                assert_eq!(original, next.denoise_request_key().unwrap());
                let mut changed = edit;
                changed["profile"]["amount"] = json!(50);
                let profile = request(&owner, key, Some(changed), settings(), &registry).unwrap();
                assert_ne!(original, profile.denoise_request_key().unwrap());
                let borrowed =
                    request(&owner, key, Some(json!({})), settings(), &registry).unwrap();
                assert!(matches!(
                    borrowed.request_base_without_denoise().unwrap().image,
                    Cow::Borrowed(_)
                ));
                assert!(matches!(
                    inference.request_base_without_denoise().unwrap().image,
                    Cow::Borrowed(_)
                ));
                let weak = Arc::downgrade(&inference);
                // A completed open is terminal: cancel is deliberately a no-op.
                assert_eq!(
                    owner.cancel(key).unwrap(),
                    crate::progressive_open::CancelResult::AlreadyFinished
                );
                assert!(inference.check().is_ok());
                // Reservation retires the same-path old source before snapshot work.
                // Keep its render leases held: snapshot work must wait for those leases.
                let replacement = owner
                    .reserve(B, inference.loaded().path.clone())
                    .unwrap();
                assert_ne!(replacement, key);
                assert!(inference.check().is_err());
                assert!(next.check().is_err());
                drop(inference);
                assert!(weak.upgrade().is_none());
            }
            #[tokio::test]
            #[ignore = "actual GPU and generated RAW; separate explicit grant required"]
            async fn pc05_genuine_selected_owner_preview_export_and_filter_count() {
                let (context, errors) = crate::gpu_processing::point_colour_test_context();
                let scope = context
                    .device
                    .push_error_scope(wgpu::ErrorFilter::Validation);
                let (registry, id, _, _) = selected();
                let registry = Arc::new(registry);
                for selected_profile in [false, true] {
                    let mut edit = json!({"pointColour":{"v":1,"centerHue":240,"widthDegrees":40,"hue":10,"saturation":10,"luminance":10}});
                    if selected_profile {
                        edit["profile"] = json!({"base":id,"look":"composition-look","amount":100});
                    }
                    let (o, k, _images) =
                        ready(&synthetic_dng(true, 1), edit.clone(), Arc::clone(&registry)).await;
                    let q = request(&o, k, Some(edit), settings(), &registry).unwrap();
                    ACCURATE_FILTER_CALLS.with(|c| c.set(0));
                    q.with_full_composition(|composed| {
                        assert_eq!(composed.request().adjustments.global.has_dcp, 0);
                        for precision in [
                            crate::gpu_processing::RenderOutputPrecision::EightBit,
                            crate::gpu_processing::RenderOutputPrecision::SixteenBit,
                        ] {
                            let mut renderer =
                                crate::gpu_processing::IsolatedCreativeRenderer::new(&context);
                            let preview = renderer.render(
                                composed.pixels(),
                                composed.request(),
                                precision,
                            )?;
                            let mut settings = crate::export_processing::ExportSettings {
                                jpeg_quality: 90,
                                tiff_bit_depth: crate::export_processing::TiffBitDepth::Sixteen,
                                resize: None,
                                border: None,
                                pad: None,
                                keep_metadata: false,
                                preserve_timestamps: false,
                                strip_gps: false,
                                filename_template: None,
                                watermark: None,
                                export_masks: false,
                                preserve_folders: false,
                                destination_type: None,
                                subfolder: None,
                                dpi: Some(300),
                            };
                            settings.tiff_bit_depth = match precision {
                                crate::gpu_processing::RenderOutputPrecision::EightBit => {
                                    crate::export_processing::TiffBitDepth::Eight
                                }
                                crate::gpu_processing::RenderOutputPrecision::SixteenBit => {
                                    crate::export_processing::TiffBitDepth::Sixteen
                                }
                            };
                            let exported = crate::export_processing::render_composed_export(
                                composed,
                                &context,
                                &settings,
                                "tiff",
                                &|| q.check(),
                            )?;
                            assert_eq!(preview.dimensions(), exported.dimensions());
                            assert_eq!(preview.as_bytes(), exported.as_bytes());
                        }
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(ACCURATE_FILTER_CALLS.with(|c| c.get()), 0);
                    q.cancel();
                    assert!(q.sample_point_colour(&context, [0.5, 0.5]).is_err());
                }
                context
                    .device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(60)),
                    })
                    .unwrap();
                assert!(pollster::block_on(scope.pop()).is_none());
                assert!(errors.lock().unwrap().is_empty());
            }

            #[tokio::test]
            async fn pc03_real_request_composition_cancel_and_current_uniforms() {
                let r = Arc::new(registry());
                let (o, k, _images) =
                    ready(&synthetic_dng(true, 1), json!({}), Arc::clone(&r)).await;
                let base = request(&o, k, None, settings(), &r).unwrap();
                let mut edited = json!({"pointColour":{"v":1,"centerHue":0,"widthDegrees":40,"hue":50,"saturation":0,"luminance":0}});
                let q = request(&o, k, Some(edited.clone()), settings(), &r).unwrap();
                edited["pointColour"]["hue"] = json!(-50);
                let original = base
                    .with_full_composition(|c| Ok(c.pixels().as_bytes().to_vec()))
                    .unwrap();
                q.with_full_composition(|c| {
                    assert_eq!(c.pixels().as_bytes(), original.as_slice());
                    assert_eq!(c.request().adjustments.global.point_colour.hue, 50.0 * 0.3);
                    assert_eq!(c.request().adjustments.global.has_dcp, 0);
                    Ok(())
                })
                .unwrap();
                q.cancel();
                let entered = std::cell::Cell::new(false);
                assert!(
                    q.with_full_composition(|_| {
                        entered.set(true);
                        Ok(())
                    })
                    .is_err()
                );
                assert!(!entered.get());
                let current = request(&o, k, None, settings(), &r).unwrap();
                assert!(
                    current
                        .with_full_composition(|_| {
                            current.cancel();
                            Err::<(), _>("stage-error".into())
                        })
                        .unwrap_err()
                        .contains("cancelled")
                );
            }

            #[test]
            fn preview_settings_set_order_is_canonical_and_membership_exact() {
                let config = settings();
                let mut original = json!({"current":&config,"source":&config});
                let mut reversed = original.clone();
                for side in ["current", "source"] {
                    for field in ["includedAdjustments", "knownAdjustments"] {
                        reversed[side]["copyPasteSettings"][field]
                            .as_array_mut()
                            .unwrap()
                            .reverse();
                    }
                }
                assert_ne!(original, reversed);
                canonicalize_preview_settings(&mut original);
                canonicalize_preview_settings(&mut reversed);
                assert_eq!(original, reversed);
                let mut changed_members = original.clone();
                changed_members["current"]["copyPasteSettings"]["knownAdjustments"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("fixture-added-member"));
                canonicalize_preview_settings(&mut changed_members);
                assert_ne!(original, changed_members);
                original["current"]["rootFolders"] = json!(["first", "second"]);
                let mut ordered = original.clone();
                ordered["current"]["rootFolders"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
                canonicalize_preview_settings(&mut original);
                canonicalize_preview_settings(&mut ordered);
                assert_ne!(original, ordered);
            }

            // Genuine cold controls explicitly develop from the same held source/assets.
            fn cold_preview_pixels(
                q: &ConsumerRequest,
                edge: u32,
                divisor: Option<f32>,
            ) -> crate::open_composition::OwnedPreviewPixels {
                q.stage(|| {
                    let base = q
                        .appearance()?
                        .render_unpatched_base(AppearancePass::AccurateOne, q.raw_cancel())
                        .map_err(|e| e.to_string())?;
                    let composed = crate::open_composition::compose_accurate_preview(
                        &base,
                        q.creative()?,
                        edge,
                        divisor,
                        q.raw_cancel(),
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(composed.into_preview_pixels())
                })
                .unwrap()
            }
            fn cold_preview_render(
                q: &ConsumerRequest,
                context: &crate::image_processing::GpuContext,
                edge: u32,
            ) -> (DynamicImage, (u32, u32), (u32, u32)) {
                q.stage(|| {
                    let pixels = cold_preview_pixels(q, edge, None);
                    let draw = pixels
                        .request(q.creative()?, q.appearance()?.as_shot())
                        .map_err(|e| e.to_string())?;
                    let mut renderer =
                        crate::gpu_processing::IsolatedCreativeRenderer::new(context);
                    let output = renderer.render(
                        pixels.pixels(),
                        draw,
                        crate::gpu_processing::RenderOutputPrecision::EightBit,
                    )?;
                    let dimensions = pixels.pixels().dimensions();
                    Ok((output, dimensions, (0, 0)))
                })
                .unwrap()
            }

            #[tokio::test]
            async fn cb01_committed_selected_is_borrowed_not_pristine() {
                let (r, id, _, _) = selected();
                let input = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let (o, k, _) =
                    ready(&patterned_composition_dng(), input.clone(), Arc::clone(&r)).await;
                let q = request(&o, k, Some(input), settings(), &r).unwrap();
                let before = preview_base_development_count();
                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                let base = q.request_base_without_denoise().unwrap();
                assert!(matches!(&base.image, Cow::Borrowed(_)));
                let ConsumerSource::Accurate(snapshot) = &q.source else {
                    panic!("actual accurate lease required")
                };
                assert!(std::ptr::eq(base.pixels(), snapshot.initial_appearance()));
                assert_ne!(base.pixels().as_bytes(), q.loaded().image.as_bytes());
                assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
            }

            #[tokio::test]
            async fn cb02_colour_curve_zoom_borrow_equals_actual_cold_producer() {
                let (r, id, _, _) = selected();
                let input = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let (o, k, _) =
                    ready(&patterned_composition_dng(), input.clone(), Arc::clone(&r)).await;
                let baseline = request(&o, k, Some(input.clone()), settings(), &r).unwrap();
                let baseline_pixels = baseline.compose_preview_pixels(Some(32), None).unwrap();
                let baseline_uniforms = baseline_pixels
                    .request(
                        baseline.creative().unwrap(),
                        baseline.appearance().unwrap().as_shot(),
                    )
                    .unwrap()
                    .adjustments;
                for (field, value, edge) in [
                    ("saturation", json!(31), 32),
                    (
                        "curves",
                        json!({"luma":[{"x":0,"y":0},{"x":128,"y":180},{"x":255,"y":255}]}),
                        32,
                    ),
                    ("exposure", json!(1.25), 48),
                ] {
                    let mut edit = input.clone();
                    edit[field] = value;
                    let q = request(&o, k, Some(edit), settings(), &r).unwrap();
                    let before = preview_base_development_count();
                    ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                    let actual = q.compose_preview_pixels(Some(edge), None).unwrap();
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                    assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                    let current = actual
                        .request(q.creative().unwrap(), q.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments;
                    assert_ne!(
                        bytemuck::bytes_of(&baseline_uniforms),
                        bytemuck::bytes_of(&current)
                    );
                    let cold = cold_preview_pixels(&q, edge, None);
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                    assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 1);
                    assert_eq!(actual.pixels().as_bytes(), cold.pixels().as_bytes());
                    assert_eq!(actual.geometry(), cold.geometry());
                }
            }

            #[tokio::test]
            async fn cb03_asset_amount_and_ai_changes_require_real_development() {
                let (r, id, dcp, look) = selected();
                let input = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let (o, k, _) =
                    ready(&patterned_composition_dng(), input.clone(), Arc::clone(&r)).await;
                for edit in [
                    json!({"profile":{"base":id,"look":"composition-look","amount":50}}),
                    json!({"profile":{"base":id,"look":null,"amount":100}}),
                    json!({"profile":{"base":id,"look":"composition-look","amount":100},"aiDenoiseEnabled":true,"aiDenoiseAmount":100}),
                ] {
                    let q = request(&o, k, Some(edit), settings(), &r).unwrap();
                    let before = preview_base_development_count();
                    assert!(matches!(
                        q.request_base_without_denoise().unwrap().image,
                        Cow::Owned(_)
                    ));
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                }
                let xml = fs::read_to_string(&look).unwrap();
                let changed = xml.replace("ProcessVersion=\"15.4\"", "ProcessVersion=\"15.3\"");
                assert_ne!(xml, changed);
                fs::write(&look, changed).unwrap();
                let q = request(&o, k, Some(input.clone()), settings(), &r).unwrap();
                assert!(matches!(
                    q.request_base_without_denoise().unwrap().image,
                    Cow::Owned(_)
                ));
                fs::write(&look, xml).unwrap();
                let original_dcp = fs::read(&dcp).unwrap();
                let mut changed_dcp = original_dcp.clone();
                changed_dcp.push(0);
                fs::write(&dcp, changed_dcp).unwrap();
                let entry = crate::dcp::registry::parse_dcp_entry(
                    &dcp,
                    crate::dcp::registry::ProfileSource::Managed,
                )
                .unwrap();
                let changed_id = entry.id.to_hex();
                assert_ne!(changed_id, id);
                r.insert_dcp(entry);
                let edit = json!({"profile":{"base":changed_id,"look":null,"amount":100}});
                let q = request(&o, k, Some(edit), settings(), &r).unwrap();
                assert!(matches!(
                    q.request_base_without_denoise().unwrap().image,
                    Cow::Owned(_)
                ));
                fs::write(&dcp, original_dcp).unwrap();
                let mut initial_ai = input.clone();
                initial_ai["aiDenoiseEnabled"] = json!(true);
                let (ai_owner, ai_key, _) =
                    ready(&patterned_composition_dng(), initial_ai, Arc::clone(&r)).await;
                let q = request(&ai_owner, ai_key, Some(input), settings(), &r).unwrap();
                assert!(matches!(
                    q.request_base_without_denoise().unwrap().image,
                    Cow::Owned(_)
                ));
            }

            #[tokio::test]
            async fn cb04_initial_patch_and_cancel_aba_never_borrow_or_publish() {
                let (r, id, _, _) = selected();
                let input = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let mut patched = patch_contract_adjustments(None, Some((1, 1)), (10, 10));
                patched["profile"] = input["profile"].clone();
                let (patched_owner, patched_key, _) = ready(
                    &patterned_composition_dng(),
                    patched.clone(),
                    Arc::clone(&r),
                )
                .await;
                let q = request(
                    &patched_owner,
                    patched_key,
                    Some(input.clone()),
                    settings(),
                    &r,
                )
                .unwrap();
                assert!(matches!(
                    q.request_base_without_denoise().unwrap().image,
                    Cow::Owned(_)
                ));
                let (o, k, _) =
                    ready(&patterned_composition_dng(), input.clone(), Arc::clone(&r)).await;
                let current_patch = request(&o, k, Some(patched), settings(), &r).unwrap();
                assert!(matches!(
                    current_patch.request_base_without_denoise().unwrap().image,
                    Cow::Borrowed(_)
                ));
                let actual = current_patch
                    .compose_preview_pixels(Some(32), None)
                    .unwrap();
                let cold = cold_preview_pixels(&current_patch, 32, None);
                assert_eq!(actual.pixels().as_bytes(), cold.pixels().as_bytes());
                drop(current_patch);
                let q = request(&o, k, Some(input.clone()), settings(), &r).unwrap();
                let source = Arc::downgrade(&q.appearance().unwrap().source);
                {
                    let base = q.request_base_without_denoise().unwrap();
                    assert!(matches!(base.image, Cow::Borrowed(_)));
                }
                q.cancel();
                assert!(q.request_base_without_denoise().is_err());
                let live = request(&o, k, Some(input), settings(), &r).unwrap();
                let path = live.loaded().path.clone();
                let replacement = o.reserve(B, path.clone()).unwrap();
                let again = o.reserve(A, path).unwrap();
                assert_ne!(replacement, k);
                assert_ne!(again, k);
                assert!(live.request_base_without_denoise().is_err());
                o.close();
                drop((q, live));
                assert!(source.upgrade().is_none());
            }

            #[tokio::test]
            async fn dg01_final_drag_release_reuses_noninitial_dcp_and_equals_cold() {
                let (r, id, _, _) = selected();
                let (o, k, _) = ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let input = json!({
                    "profile":{"base":id,"look":"composition-look","amount":100},
                    "rotation":7.0,
                    "crop":{"x":7.25,"y":11.5,"width":311.75,"height":273.25}
                });
                let a = request(&o, k, Some(input.clone()), settings(), &r).unwrap();
                let ConsumerSource::Accurate(snapshot) = &a.source else {
                    panic!("genuine accurate owner required")
                };
                assert!(snapshot.retained().unwrap().appearance.assets.is_none());
                let mut cache = None;
                let before = preview_base_development_count();
                let baseline_exposure;
                {
                    let frame = a
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(!frame.hit);
                    assert!(matches!(
                        frame.pixels().pixels(),
                        DynamicImage::ImageRgba32F(_)
                    ));
                    baseline_exposure = frame
                        .pixels()
                        .request(a.creative().unwrap(), a.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments
                        .global
                        .exposure;
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                let retained = cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr();
                let final_bytes = cache.as_ref().unwrap().pixels.pixels().as_bytes().to_vec();
                let mut edited = input.clone();
                edited["exposure"] = json!(1.25);
                let b = request(&o, k, Some(edited), settings(), &r).unwrap();
                let (drag_bytes, drag_size, drag_geometry, drag_uniforms);
                {
                    let frame = b
                        .prepare_preview_pixels(&mut cache, Some(32), Some(1.4))
                        .unwrap();
                    assert!(!frame.hit); // derived transient, not an exact geometry hit
                    assert!(matches!(&frame.storage, PreviewPixelsStorage::Transient(_)));
                    drag_bytes = frame.pixels().pixels().as_bytes().to_vec();
                    drag_size = frame.pixels().pixels().dimensions();
                    drag_geometry = frame.pixels().geometry();
                    drag_uniforms = frame
                        .pixels()
                        .request(b.creative().unwrap(), b.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments;
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                assert_ne!(
                    baseline_exposure.to_bits(),
                    drag_uniforms.global.exposure.to_bits()
                );
                assert_eq!(
                    cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr(),
                    retained
                );
                {
                    let frame = b
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(frame.hit);
                    assert_eq!(frame.pixels().pixels().as_bytes(), final_bytes);
                    let uniforms = frame
                        .pixels()
                        .request(b.creative().unwrap(), b.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments;
                    assert_eq!(
                        bytemuck::bytes_of(&uniforms),
                        bytemuck::bytes_of(&drag_uniforms)
                    );
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                assert_eq!(
                    cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr(),
                    retained
                );
                let before_cold = preview_base_development_count();
                let cold = cold_preview_pixels(&b, 32, Some(1.4));
                assert_eq!(
                    preview_base_development_count().wrapping_sub(before_cold),
                    1
                );
                assert_eq!(drag_bytes, cold.pixels().as_bytes());
                assert_eq!(drag_size, cold.pixels().dimensions());
                let cold_geometry = cold.geometry();
                assert_eq!(drag_geometry.0.to_bits(), cold_geometry.0.to_bits());
                assert_eq!(drag_geometry.1.0.to_bits(), cold_geometry.1.0.to_bits());
                assert_eq!(drag_geometry.1.1.to_bits(), cold_geometry.1.1.to_bits());
                assert_eq!(
                    drag_geometry.0.to_bits(),
                    ((drag_size.0 as f64 / 312.0) as f32).to_bits()
                );
                let before_resolution = preview_base_development_count();
                assert!(
                    !b.prepare_preview_pixels(&mut cache, Some(31), Some(1.4))
                        .unwrap()
                        .hit
                );
                assert_eq!(
                    preview_base_development_count().wrapping_sub(before_resolution),
                    1
                );
                let mut amount = input;
                amount["profile"]["amount"] = json!(50);
                let changed = request(&o, k, Some(amount), settings(), &r).unwrap();
                let before_change = preview_base_development_count();
                assert!(
                    !changed
                        .prepare_preview_pixels(&mut cache, Some(32), Some(1.4))
                        .unwrap()
                        .hit
                );
                assert_eq!(
                    preview_base_development_count().wrapping_sub(before_change),
                    1
                );
                o.reserve(B, b.loaded().path.clone()).unwrap();
                assert!(
                    b.prepare_preview_pixels(&mut cache, Some(32), Some(1.4))
                        .is_err()
                );
            }

            #[tokio::test]
            async fn dg02_oversized_zoom_preserves_final_for_drag_release() {
                // Expand to 71,884,800 RGB32F bytes, above the 64MiB budget.
                // Only genuine DNG dimensions/strip change; CFA,
                // levels, matrices, orientation and decoder path stay real.
                let mut bytes = synthetic_dng(true, 6);
                let read16 = |b: &[u8], p| u16::from_le_bytes(b[p..p + 2].try_into().unwrap());
                let read32 = |b: &[u8], p| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
                let ifd = read32(&bytes, 4) as usize;
                let count = read16(&bytes, ifd) as usize;
                let (width, height) = (2400u32, 2496u32);
                let mut strip = None;
                for i in 0..count {
                    let entry = ifd + 2 + 12 * i;
                    let tag = read16(&bytes, entry);
                    let value = match tag {
                        256 => Some(width),
                        257 | 278 => Some(height),
                        279 => Some(width * height * 2),
                        _ => None,
                    };
                    if let Some(value) = value {
                        assert_eq!(read16(&bytes, entry + 2), 4);
                        bytes[entry + 8..entry + 12].copy_from_slice(&value.to_le_bytes());
                    }
                    if tag == 273 {
                        strip = Some(read32(&bytes, entry + 8) as usize);
                    } else if tag == 50829 {
                        let offset = read32(&bytes, entry + 8) as usize;
                        bytes[offset + 8..offset + 12].copy_from_slice(&height.to_le_bytes());
                        bytes[offset + 12..offset + 16].copy_from_slice(&width.to_le_bytes());
                    }
                }
                bytes.truncate(strip.unwrap());
                let pattern = b"RBGBRGGGRGGBGGBGGRBRGRBGGGBGGRGGRGGB";
                for y in 0..height {
                    for x in 0..width {
                        let sample: u16 = match pattern[((y % 6) * 6 + x % 6) as usize] {
                            b'R' => 1024,
                            b'G' => 1984,
                            _ => 1504,
                        };
                        bytes.extend(sample.to_le_bytes());
                    }
                }
                let (r, id, _, _) = selected();
                let o = owner();
                let k = admit(&o, file(&bytes, "dng"), metadata(json!({})), settings(), true).await;
                let pixels_registry = Arc::clone(&r);
                // This genuine 5.99MP fixture exceeds the small fixtures' 3s
                // render helper bound; only its Pixels setup gets 30s.
                tokio::time::timeout(
                    Duration::from_secs(30),
                    o.execute(
                        k,
                        JobKind::Pixels,
                        move |c| read_keyed_candidate(c, &pixels_registry),
                        finish,
                    ),
                )
                .await
                .expect("large genuine fixture Pixels setup exceeded 30s")
                .unwrap();
                let input = json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let a = request(&o, k, Some(input.clone()), settings(), &r).unwrap();
                let ConsumerSource::Accurate(snapshot) = &a.source else {
                    panic!("genuine accurate owner required")
                };
                assert!(snapshot.retained().unwrap().appearance.assets.is_none());
                let mut cache = None;
                let before = preview_base_development_count();
                drop(
                    a.prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap(),
                );
                assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                let retained = cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr();
                let final_bytes = cache.as_ref().unwrap().pixels.pixels().as_bytes().to_vec();
                {
                    let oversized = a
                        .prepare_preview_pixels(&mut cache, Some(4096), None)
                        .unwrap();
                    assert!(!oversized.hit);
                    assert!(matches!(
                        &oversized.storage,
                        PreviewPixelsStorage::Transient(_)
                    ));
                    assert!(oversized.pixels().retained_capacity_bytes().unwrap() > PREVIEW_PIXELS_BUDGET);
                    assert!(
                        !oversized
                            .pixels()
                            .within_retention_budget(PREVIEW_PIXELS_BUDGET)
                    );
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 2);
                assert_eq!(
                    cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr(),
                    retained
                );
                let mut edit = input.clone();
                edit["exposure"] = json!(1.25);
                let b = request(&o, k, Some(edit), settings(), &r).unwrap();
                let (drag_bytes, drag_dimensions, drag_geometry, drag_uniforms);
                {
                    let drag = b
                        .prepare_preview_pixels(&mut cache, Some(32), Some(1.4))
                        .unwrap();
                    assert!(!drag.hit);
                    assert!(matches!(&drag.storage, PreviewPixelsStorage::Transient(_)));
                    drag_bytes = drag.pixels().pixels().as_bytes().to_vec();
                    drag_dimensions = drag.pixels().pixels().dimensions();
                    drag_geometry = drag.pixels().geometry();
                    drag_uniforms = drag
                        .pixels()
                        .request(b.creative().unwrap(), b.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments;
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 2);
                {
                    let final_frame = b
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(final_frame.hit);
                    assert_eq!(final_frame.pixels().pixels().as_bytes(), final_bytes);
                    let uniforms = final_frame
                        .pixels()
                        .request(b.creative().unwrap(), b.appearance().unwrap().as_shot())
                        .unwrap()
                        .adjustments;
                    assert_eq!(
                        bytemuck::bytes_of(&uniforms),
                        bytemuck::bytes_of(&drag_uniforms)
                    );
                }
                assert_eq!(preview_base_development_count().wrapping_sub(before), 2);
                assert_eq!(
                    cache.as_ref().unwrap().pixels.pixels().as_bytes().as_ptr(),
                    retained
                );
                let cold = cold_preview_pixels(&b, 32, Some(1.4));
                assert_eq!(drag_bytes, cold.pixels().as_bytes());
                assert_eq!(drag_dimensions, cold.pixels().dimensions());
                let geometry = cold.geometry();
                assert_eq!(drag_geometry.0.to_bits(), geometry.0.to_bits());
                assert_eq!(drag_geometry.1.0.to_bits(), geometry.1.0.to_bits());
                assert_eq!(drag_geometry.1.1.to_bits(), geometry.1.1.to_bits());
                for changed in [
                    json!({"profile":{"base":id,"look":"composition-look","amount":100},"orientationSteps":1}),
                    json!({"profile":{"base":id,"look":"composition-look","amount":50}}),
                ] {
                    drop(
                        a.prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap(),
                    );
                    let q = request(&o, k, Some(changed), settings(), &r).unwrap();
                    let before_miss = preview_base_development_count();
                    assert!(
                        !q.prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap()
                            .hit
                    );
                    assert_eq!(
                        preview_base_development_count().wrapping_sub(before_miss),
                        1
                    );
                }
            }

            mod cheap_preview_reuse_tests {
                use super::*;

                fn gpu_edits(cube: &str) -> Vec<(&'static str, Value)> {
                    vec![
                        (
                            "wb",
                            json!({"whiteBalance":{"temperature":8500.0,"tint":17.0},"temperature":8.0,"tint":-4.0}),
                        ),
                        (
                            "hsl",
                            json!({"hsl":{"blues":{"hue":12.0,"saturation":20.0,"luminance":-8.0}}}),
                        ),
                        (
                            "colour",
                            json!({"saturation":12.0,"vibrance":7.0,"colorGrading":{"global":{"hue":45.0,"saturation":10.0,"luminance":3.0}}}),
                        ),
                        ("lut", json!({"lutPath":cube,"lutIntensity":60.0})),
                        ("exposure", json!({"exposure":1.0})),
                        (
                            "curves",
                            json!({"curves":{"luma":[{"x":0.0,"y":0.0},{"x":128.0,"y":180.0},{"x":255.0,"y":255.0}]},"pointCurves":{"luma":[{"x":128.0,"y":180.0}]},"curveMode":"point"}),
                        ),
                        (
                            "visibility",
                            json!({"sectionVisibility":{"basic":true,"color":true,"curves":true,"details":true},"exposure":0.5}),
                        ),
                    ]
                }
                // Native parser order is reds=0 ... blues=5. This selected
                // fixture renders blue, so a red-only edit was a valid no-op.
                fn assert_blue_hsl_control(
                    before: &crate::image_processing::AllAdjustments,
                    after: &crate::image_processing::AllAdjustments,
                ) {
                    assert_ne!(
                        bytemuck::bytes_of(&before.global.hsl[5]),
                        bytemuck::bytes_of(&after.global.hsl[5]),
                    );
                    let blue: [f32; 4] = bytemuck::cast(after.global.hsl[5]);
                    // Exact existing parser scales: hue*0.3, saturation/100,
                    // luminance/100; no change to production uniform math.
                    assert_eq!(blue, [12.0 * 0.3, 20.0 / 100.0, -8.0 / 100.0, 0.0]);
                    for index in 0..8 {
                        if index != 5 {
                            assert_eq!(
                                bytemuck::bytes_of(&before.global.hsl[index]),
                                bytemuck::bytes_of(&after.global.hsl[index]),
                                "unexpected HSL channel: {index}",
                            );
                        }
                    }
                }

                fn lut_file() -> std::path::PathBuf {
                    let cube = directory().join("cheap-preview.cube");
                    fs::write(&cube, "LUT_3D_SIZE 2\n0.1 0.2 0.3\n0.9 0.2 0.3\n0.1 0.8 0.3\n0.9 0.8 0.3\n0.1 0.2 0.7\n0.9 0.2 0.7\n0.1 0.8 0.7\n0.9 0.8 0.7\n").unwrap();
                    cube
                }
                fn overlay(base: &Value, edit: &Value) -> Value {
                    let mut result = base.clone();
                    for (key, value) in edit.as_object().unwrap() {
                        result[key] = value.clone();
                    }
                    result
                }

                #[tokio::test]
                async fn preview_gpu_only_edits_reuse_real_producer_and_equal_cold() {
                    let (r, id, _, _) = selected();
                    let original =
                        json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                    let (o, k, _) = ready(
                        &patterned_composition_dng(),
                        original.clone(),
                        Arc::clone(&r),
                    )
                    .await;
                    let first = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                    let cube = lut_file();
                    let mut cache = None;
                    let before = preview_base_development_count();
                    drop(
                        first
                            .prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap(),
                    );
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                    let baseline_uniforms = cache
                        .as_ref()
                        .unwrap()
                        .pixels
                        .request(
                            first.creative().unwrap(),
                            first.appearance().unwrap().as_shot(),
                        )
                        .unwrap()
                        .adjustments;
                    for (name, edit) in gpu_edits(cube.to_str().unwrap()) {
                        let current =
                            request(&o, k, Some(overlay(&original, &edit)), settings(), &r)
                                .unwrap();
                        let before = preview_base_development_count();
                        let (bytes, dimensions, geometry, uniforms, lut);
                        {
                            let frame = current
                                .prepare_preview_pixels(&mut cache, Some(32), None)
                                .unwrap();
                            assert!(frame.hit, "{name}");
                            let draw = frame
                                .pixels()
                                .request(
                                    current.creative().unwrap(),
                                    current.appearance().unwrap().as_shot(),
                                )
                                .unwrap();
                            bytes = frame.pixels().pixels().as_bytes().to_vec();
                            dimensions = frame.pixels().pixels().dimensions();
                            geometry = frame.pixels().geometry();
                            uniforms = draw.adjustments;
                            lut = draw.lut.as_ref().map(|lut| (lut.size, lut.data.clone()));
                        }
                        assert_eq!(
                            preview_base_development_count().wrapping_sub(before),
                            0,
                            "{name}"
                        );
                        assert_ne!(
                            bytemuck::bytes_of(&baseline_uniforms),
                            bytemuck::bytes_of(&uniforms),
                            "fresh uniforms: {name}"
                        );
                        if name == "hsl" {
                            assert_blue_hsl_control(&baseline_uniforms, &uniforms);
                        }
                        let cold = cold_preview_pixels(&current, 32, None);
                        assert_eq!(
                            preview_base_development_count().wrapping_sub(before),
                            1,
                            "cold: {name}"
                        );
                        assert_eq!(bytes, cold.pixels().as_bytes(), "whole CPU pixels: {name}");
                        assert_eq!(dimensions, cold.pixels().dimensions());
                        assert_eq!(geometry, cold.geometry());
                        let draw = cold
                            .request(
                                current.creative().unwrap(),
                                current.appearance().unwrap().as_shot(),
                            )
                            .unwrap();
                        assert_eq!(
                            bytemuck::bytes_of(&uniforms),
                            bytemuck::bytes_of(&draw.adjustments)
                        );
                        assert_eq!(
                            lut,
                            draw.lut.as_ref().map(|lut| (lut.size, lut.data.clone()))
                        );
                        assert!(draw.mask_bitmaps.is_empty());
                    }
                    // Key omission never bypasses the actual fresh LUT parser.
                    let lut_edit = overlay(&original, &json!({"lutPath":cube.to_str().unwrap()}));
                    let before = preview_base_development_count();
                    fs::remove_file(&cube).unwrap();
                    assert!(request(&o, k, Some(lut_edit.clone()), settings(), &r).is_err());
                    fs::write(&cube, "invalid LUT").unwrap();
                    assert!(request(&o, k, Some(lut_edit.clone()), settings(), &r).is_err());
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                    fs::write(
                        &cube,
                        "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n",
                    )
                    .unwrap();
                    let fresh = request(&o, k, Some(lut_edit), settings(), &r).unwrap();
                    let weak_lut = Arc::downgrade(fresh.creative().unwrap().lut().unwrap());
                    let frame = fresh
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(frame.hit);
                    let draw = frame
                        .pixels()
                        .request(
                            fresh.creative().unwrap(),
                            fresh.appearance().unwrap().as_shot(),
                        )
                        .unwrap();
                    assert_eq!(
                        draw.lut.as_ref().unwrap().data[0].to_bits(),
                        0.0f32.to_bits()
                    );
                    drop(draw);
                    drop(frame);
                    drop(fresh);
                    assert!(weak_lut.upgrade().is_none());
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                }

                #[tokio::test]
                async fn preview_cpu_edits_miss_real_producer_and_equal_cold() {
                    let (r, id, _, _) = selected();
                    let original =
                        json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                    let (o, k, _) = ready(
                        &patterned_composition_dng(),
                        original.clone(),
                        Arc::clone(&r),
                    )
                    .await;
                    for edit in [
                        json!({"orientationSteps":1}),
                        json!({"crop":{"x":16.0,"y":8.0,"width":128.0,"height":96.0}}),
                        json!({"flipHorizontal":true}),
                        json!({"transformXOffset":1.0}),
                        json!({"lensDistortionParams":{"k1":0.0,"k2":0.0,"k3":0.0,"model":0}}),
                        json!({"fogAmount":12.0}),
                        json!({"relightAmbient":12.0}),
                        json!({"lensBlurAmount":12.0}),
                        json!({"sectionVisibility":{"effects":false}}),
                    ] {
                        let first = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                        let mut cache = None;
                        drop(
                            first
                                .prepare_preview_pixels(&mut cache, Some(32), None)
                                .unwrap(),
                        );
                        let current =
                            request(&o, k, Some(overlay(&original, &edit)), settings(), &r)
                                .unwrap();
                        let descriptor =
                            current.preview_descriptor(Some(32), None).unwrap().unwrap();
                        assert_eq!(
                            cache
                                .as_ref()
                                .unwrap()
                                .descriptor
                                .mismatch_reason(&descriptor),
                            Some("cpu_inputs")
                        );
                        let before = preview_base_development_count();
                        let bytes;
                        {
                            let frame = current
                                .prepare_preview_pixels(&mut cache, Some(32), None)
                                .unwrap();
                            assert!(!frame.hit);
                            bytes = frame.pixels().pixels().as_bytes().to_vec();
                        }
                        assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                        let cold = cold_preview_pixels(&current, 32, None);
                        assert_eq!(preview_base_development_count().wrapping_sub(before), 1);
                        assert_eq!(bytes, cold.pixels().as_bytes());
                    }
                    let unknown = request(
                        &o,
                        k,
                        Some(overlay(&original, &json!({"futureCpuInput":1}))),
                        settings(),
                        &r,
                    )
                    .unwrap();
                    let (descriptor, reason) = unknown
                        .preview_descriptor_diagnostic(Some(32), None)
                        .unwrap();
                    assert!(descriptor.is_none());
                    assert_eq!(reason, "unclassified_adjustment");
                    let mut cache = None;
                    let frame = unknown
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(!frame.hit);
                    assert!(matches!(frame.storage, PreviewPixelsStorage::Transient(_)));
                }

                #[tokio::test]
                async fn preview_unit_divisor_and_invalid_values_are_fenced() {
                    let (r, id, _, _) = selected();
                    let original =
                        json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                    let (o, k, _) = ready(
                        &patterned_composition_dng(),
                        original.clone(),
                        Arc::clone(&r),
                    )
                    .await;
                    let q = request(&o, k, Some(original), settings(), &r).unwrap();
                    let mut cache = None;
                    drop(
                        q.prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap(),
                    );
                    let before = preview_base_development_count();
                    let bytes;
                    {
                        let frame = q
                            .prepare_preview_pixels(&mut cache, Some(32), Some(1.0))
                            .unwrap();
                        assert!(frame.hit);
                        bytes = frame.pixels().pixels().as_bytes().to_vec();
                    }
                    assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                    assert!(
                        q.prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap()
                            .hit
                    );
                    let cold = cold_preview_pixels(&q, 32, Some(1.0));
                    assert_eq!(bytes, cold.pixels().as_bytes());
                    for divisor in [
                        0.0,
                        -0.0,
                        0.999,
                        -1.0,
                        f32::NAN,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                    ] {
                        let before = preview_base_development_count();
                        assert!(
                            q.prepare_preview_pixels(&mut cache, Some(32), Some(divisor))
                                .is_err()
                        );
                        assert_eq!(preview_base_development_count().wrapping_sub(before), 0);
                        assert!(q.compose_preview_pixels(Some(32), Some(divisor)).is_err());
                    }
                    let frame = q
                        .prepare_preview_pixels(&mut cache, Some(32), Some(1.5))
                        .unwrap();
                    assert!(!frame.hit);
                    assert_eq!(
                        frame.pixels().pixels().as_bytes(),
                        q.compose_preview_pixels(Some(32), Some(1.5))
                            .unwrap()
                            .pixels()
                            .as_bytes()
                    );
                }

                #[tokio::test]
                #[ignore = "requires separately granted actual GPU readback parity"]
                async fn preview_gpu_only_reuse_matches_fresh_readback() {
                    use crate::gpu_processing::RenderOutputPrecision::EightBit;
                    let (context, errors) = crate::gpu_processing::preview_reuse_hardware_context();
                    let (r, id, _, _) = selected();
                    let original =
                        json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                    let (o, k, _) = ready(
                        &patterned_composition_dng(),
                        original.clone(),
                        Arc::clone(&r),
                    )
                    .await;
                    let cube = lut_file();
                    let first = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                    let mut session = PreviewRendererSession::default();
                    let baseline = session
                        .render(&first, &context, Some(32), None, EightBit, None)
                        .unwrap();
                    assert!(
                        baseline
                            .0
                            .as_rgba8()
                            .unwrap()
                            .pixels()
                            .any(|pixel| { pixel[2] > pixel[0] && pixel[2] > pixel[1] }),
                        "selected fixture must contain blue-dominant pixels"
                    );
                    for (name, edit) in gpu_edits(cube.to_str().unwrap()) {
                        let current =
                            request(&o, k, Some(overlay(&original, &edit)), settings(), &r)
                                .unwrap();
                        if name == "hsl" {
                            let render = crate::image_processing::resolve_render_options(
                                current.settings(),
                                true,
                            );
                            let before = crate::image_processing::get_all_adjustments_from_json(
                                first.creative().unwrap().adjustments(),
                                true,
                                first.appearance().unwrap().as_shot(),
                                render,
                            );
                            let after = crate::image_processing::get_all_adjustments_from_json(
                                current.creative().unwrap().adjustments(),
                                true,
                                current.appearance().unwrap().as_shot(),
                                render,
                            );
                            assert_blue_hsl_control(&before, &after);
                        }
                        let before = preview_base_development_count();
                        let actual = session
                            .render(&current, &context, Some(32), None, EightBit, Some(1.0))
                            .unwrap();
                        assert_eq!(
                            preview_base_development_count().wrapping_sub(before),
                            0,
                            "{name}"
                        );
                        let cold = cold_preview_render(&current, &context, 32);
                        assert_eq!(
                            preview_base_development_count().wrapping_sub(before),
                            1,
                            "{name}"
                        );
                        assert_eq!(actual.0.as_bytes(), cold.0.as_bytes(), "{name}");
                        assert_eq!(actual.1, cold.1);
                        assert_eq!(actual.2, cold.2);
                        assert_ne!(
                            actual.0.as_bytes(),
                            baseline.0.as_bytes(),
                            "current output changed: {name}"
                        );
                    }
                    assert!(errors.lock().unwrap().is_empty());
                }
            }

            #[tokio::test]
            async fn selected_preview_hit_skips_development_and_refreshes_uniforms() {
                let (r, id, _, _) = selected();
                let edit = json!({"profile":{"base":id,"look":"composition-look","amount":100},"exposure":0.0});
                let (o, k, _) =
                    ready(&patterned_composition_dng(), edit.clone(), Arc::clone(&r)).await;
                let first = request(&o, k, Some(edit.clone()), settings(), &r).unwrap();
                let mut cache = None;
                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                let first_uniforms;
                {
                    let frame = first
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(!frame.hit);
                    first_uniforms = frame
                        .pixels()
                        .request(
                            first.creative().unwrap(),
                            first.appearance().unwrap().as_shot(),
                        )
                        .unwrap()
                        .adjustments;
                }
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                let mut changed = edit;
                changed["exposure"] = json!(1.0);
                changed["curves"] =
                    json!({"luma":[{"x":0.0,"y":0.0},{"x":128.0,"y":180.0},{"x":255.0,"y":255.0}]});
                let second = request(&o, k, Some(changed), settings(), &r).unwrap();
                let (cached_bytes, current_uniforms);
                {
                    let frame = second
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(frame.hit);
                    cached_bytes = frame.pixels().pixels().as_bytes().to_vec();
                    current_uniforms = frame
                        .pixels()
                        .request(
                            second.creative().unwrap(),
                            second.appearance().unwrap().as_shot(),
                        )
                        .unwrap()
                        .adjustments;
                }
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                assert_ne!(
                    first_uniforms.global.exposure.to_bits(),
                    current_uniforms.global.exposure.to_bits()
                );
                assert_eq!(current_uniforms.global.luma_curve_count, 3);
                assert_ne!(
                    bytemuck::bytes_of(&first_uniforms),
                    bytemuck::bytes_of(&current_uniforms)
                );
                let fresh = cold_preview_pixels(&second, 32, None);
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 1);
                assert_eq!(cached_bytes, fresh.pixels().as_bytes());
                let fresh_uniforms = fresh
                    .request(
                        second.creative().unwrap(),
                        second.appearance().unwrap().as_shot(),
                    )
                    .unwrap()
                    .adjustments;
                assert_eq!(
                    bytemuck::bytes_of(&current_uniforms),
                    bytemuck::bytes_of(&fresh_uniforms)
                );
                assert!(
                    cache
                        .as_ref()
                        .unwrap()
                        .pixels
                        .retained_capacity_bytes()
                        .unwrap()
                        <= PREVIEW_PIXELS_BUDGET
                );
            }

            #[tokio::test]
            async fn selected_preview_identity_and_error_changes_cannot_hit() {
                let (r, id, dcp, look) = selected();
                let original =
                    json!({"profile":{"base":id,"look":"composition-look","amount":100}});
                let (o, k, _) = ready(
                    &patterned_composition_dng(),
                    original.clone(),
                    Arc::clone(&r),
                )
                .await;
                let q = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                let mut cache = None;
                drop(
                    q.prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap(),
                );
                let initial_identity = q
                    .appearance()
                    .unwrap()
                    .assets
                    .as_ref()
                    .unwrap()
                    .content_identity();
                let xml = fs::read_to_string(&look).unwrap();
                let changed_xml = xml.replace("ProcessVersion=\"15.4\"", "ProcessVersion=\"15.3\"");
                assert_ne!(xml, changed_xml);
                fs::write(&look, &changed_xml).unwrap();
                let changed = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                assert_ne!(
                    initial_identity,
                    changed
                        .appearance()
                        .unwrap()
                        .assets
                        .as_ref()
                        .unwrap()
                        .content_identity()
                );
                assert!(
                    !changed
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap()
                        .hit
                );
                for mode in ["amount", "settings", "geometry", "resolution", "divisor"] {
                    let baseline = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                    drop(
                        baseline
                            .prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap(),
                    );
                    assert!(
                        baseline
                            .prepare_preview_pixels(&mut cache, Some(32), None)
                            .unwrap()
                            .hit
                    );
                    let mut edit = original.clone();
                    let mut config = settings();
                    let mut edge = 32;
                    let mut divisor = None;
                    match mode {
                        "amount" => edit["profile"]["amount"] = json!(50),
                        "settings" => config.theme = Some("cache-distinguishing-theme".into()),
                        "geometry" => edit["orientationSteps"] = json!(1),
                        "resolution" => edge = 31,
                        "divisor" => divisor = Some(1.5),
                        _ => unreachable!(),
                    }
                    let candidate = request(&o, k, Some(edit), config, &r).unwrap();
                    assert!(
                        !candidate
                            .prepare_preview_pixels(&mut cache, Some(edge), divisor)
                            .unwrap()
                            .hit,
                        "{mode}"
                    );
                }
                fs::remove_file(&look).unwrap();
                assert!(request(&o, k, Some(original.clone()), settings(), &r).is_err());
                fs::write(&look, b"invalid look").unwrap();
                assert!(request(&o, k, Some(original.clone()), settings(), &r).is_err());
                fs::write(&look, xml).unwrap();
                let mut content = fs::read(&dcp).unwrap();
                content.push(0); // Valid trailing byte: actual parsed DCP content identity changes.
                fs::write(&dcp, content).unwrap();
                assert!(request(&o, k, Some(original.clone()), settings(), &r).is_err());
                let entry = crate::dcp::registry::parse_dcp_entry(
                    &dcp,
                    crate::dcp::registry::ProfileSource::Managed,
                )
                .unwrap();
                let changed_id = entry.id.to_hex();
                assert_ne!(changed_id, id);
                r.insert_dcp(entry);
                let mut new_base = original;
                new_base["profile"]["base"] = json!(changed_id);
                let valid = request(&o, k, Some(new_base), settings(), &r).unwrap();
                assert!(
                    !valid
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap()
                        .hit
                );
            }

            #[tokio::test]
            async fn preview_pixels_cancel_aba_and_lifetimes_are_fenced() {
                let r = Arc::new(registry());
                let (o, k, images) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let q = request(&o, k, Some(json!({})), settings(), &r).unwrap();
                let mut cache = None;
                drop(
                    q.prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap(),
                );
                let weak_request = Arc::downgrade(&q);
                let weak_source = Arc::downgrade(&q.appearance().unwrap().source);
                for unsupported in [
                    json!({"aiPatches":[{"visible":false}]}),
                    json!({"masks":[{"id":"hidden-valid-mask","name":"Hidden valid mask","visible":false,"invert":false,"opacity":100.0,"adjustments":{},"subMasks":[]}]}),
                ] {
                    let candidate = request(&o, k, Some(unsupported), settings(), &r).unwrap();
                    assert!(
                        candidate
                            .preview_descriptor(Some(32), None)
                            .unwrap()
                            .is_none()
                    );
                    let transient = candidate
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap();
                    assert!(!transient.hit);
                    assert!(matches!(
                        &transient.storage,
                        PreviewPixelsStorage::Transient(_)
                    ));
                    drop(transient);
                    assert!(cache.is_none());
                }
                let map = patch_contract_png(DynamicImage::ImageLuma8(
                    image::GrayImage::from_pixel(2, 2, image::Luma([128])),
                ));
                let effect = request(
                    &o,
                    k,
                    Some(json!({"fogEnabled":true,"fogAmount":50.0,"fogDepthMap":map})),
                    settings(),
                    &r,
                )
                .unwrap();
                assert!(effect.creative().unwrap().fog_map().is_some());
                assert!(effect.preview_descriptor(Some(32), None).unwrap().is_none());
                drop(effect);
                let cube = directory().join("preview.cube");
                fs::write(
                    &cube,
                    "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n",
                )
                .unwrap();
                let lut_request = request(
                    &o,
                    k,
                    Some(json!({"lutPath":cube.to_str().unwrap()})),
                    settings(),
                    &r,
                )
                .unwrap();
                let old_lut = Arc::downgrade(lut_request.creative().unwrap().lut().unwrap());
                drop(
                    lut_request
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .unwrap(),
                );
                drop(lut_request);
                assert!(old_lut.upgrade().is_none());
                q.cancel();
                assert!(
                    q.prepare_preview_pixels(&mut cache, Some(32), None)
                        .is_err()
                );
                drop(q);
                assert!(weak_request.upgrade().is_none());
                let current = request(&o, k, None, settings(), &r).unwrap();
                o.reserve(
                    &uuid::Uuid::new_v4().to_string(),
                    current.loaded().path.clone(),
                )
                .unwrap();
                assert!(
                    current
                        .prepare_preview_pixels(&mut cache, Some(32), None)
                        .is_err()
                );
                o.close();
                drop((current, images));
                assert!(weak_source.upgrade().is_none()); // Entry only has a Weak source.
                cache = None;
                assert!(cache.is_none());
            }

            #[tokio::test]
            #[ignore = "requires explicit selected-preview actual GPU parity gate; source-only definition"]
            async fn selected_preview_reused_gpu_output_matches_fresh() {
                use crate::gpu_processing::RenderOutputPrecision::EightBit;
                let (context, errors) = crate::gpu_processing::preview_reuse_hardware_context();
                let (r, id, _, _) = selected();
                let original = json!({"profile":{"base":id,"look":"composition-look","amount":100},"exposure":0.0});
                let (o, k, _) = ready(
                    &patterned_composition_dng(),
                    original.clone(),
                    Arc::clone(&r),
                )
                .await;
                let mut session = PreviewRendererSession::default();
                let first = request(&o, k, Some(original.clone()), settings(), &r).unwrap();
                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                let before = session
                    .render(&first, &context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                let mut changed = original;
                changed["exposure"] = json!(1.0);
                changed["curves"] =
                    json!({"luma":[{"x":0.0,"y":0.0},{"x":128.0,"y":180.0},{"x":255.0,"y":255.0}]});
                let second = request(&o, k, Some(changed), settings(), &r).unwrap();
                let actual = session
                    .render(&second, &context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                assert_eq!(session.entry.as_ref().unwrap().1.processor_build_count(), 1);
                let fresh = cold_preview_render(&second, &context, 32);
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 1);
                assert_eq!(actual.0.as_bytes(), fresh.0.as_bytes());
                assert_ne!(before.0.as_bytes(), actual.0.as_bytes());
                assert_eq!(actual.1, fresh.1);
                assert_eq!(actual.2, fresh.2);
                assert!(errors.lock().unwrap().is_empty());
                second.cancel();
                assert!(
                    session
                        .render(&second, &context, Some(32), None, EightBit, None)
                        .is_err()
                );
                assert!(session.pixels.is_none());
                assert!(session.entry.is_none());
            }

            #[tokio::test]
            async fn preview_session_keyed_transition_and_request_retirement() {
                let r = Arc::new(registry());
                let (o, k, images) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let q = request(&o, k, None, settings(), &r).unwrap();
                let weak = Arc::downgrade(&q);
                assert_eq!(PreviewRendererSession::checked_key(&q).unwrap(), k);
                let called = std::cell::Cell::new(false);
                q.cancel();
                assert!(PreviewRendererSession::checked_key(&q).is_err());
                assert!(
                    q.stage(|| {
                        called.set(true);
                        Ok(())
                    })
                    .is_err()
                );
                assert!(!called.get());
                drop(q);
                assert!(weak.upgrade().is_none());
                let current = request(&o, k, None, settings(), &r).unwrap();
                assert_eq!(PreviewRendererSession::checked_key(&current).unwrap(), k);
                let _new_key = o
                    .reserve(
                        &uuid::Uuid::new_v4().to_string(),
                        current.loaded().path.clone(),
                    )
                    .unwrap();
                assert!(PreviewRendererSession::checked_key(&current).is_err());
                o.close();
                assert!(PreviewRendererSession::checked_key(&current).is_err());
                drop(images);
            }

            #[tokio::test]
            #[ignore = "requires explicit actual GPU preview-session gate; source-only definition"]
            async fn preview_session_reuses_current_owner_and_retires_without_source_hold() {
                use crate::gpu_processing::RenderOutputPrecision::EightBit;
                let (context, errors) = crate::gpu_processing::preview_reuse_hardware_context();
                let r = Arc::new(registry());
                let (o, k, images) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let mut session = PreviewRendererSession::default();
                let first = request(&o, k, Some(json!({"exposure":0.0})), settings(), &r).unwrap();
                session
                    .render(&first, &context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(session.entry.as_ref().unwrap().1.processor_build_count(), 1);
                let edited = request(&o, k, Some(json!({"exposure":0.5})), settings(), &r).unwrap();
                let actual = session
                    .render(&edited, &context, Some(32), None, EightBit, None)
                    .unwrap();
                let fresh = edited
                    .render_for_preview(&context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(actual.0.as_bytes(), fresh.0.as_bytes());
                assert_eq!(actual.1, fresh.1);
                assert_eq!(actual.2, fresh.2);
                assert_eq!(session.entry.as_ref().unwrap().1.processor_build_count(), 1);
                let weak = Arc::downgrade(&edited);
                drop(edited);
                assert!(weak.upgrade().is_none());
                first.cancel();
                assert!(
                    session
                        .render(&first, &context, Some(32), None, EightBit, None)
                        .is_err()
                );
                assert!(session.entry.is_none());
                let current = request(&o, k, None, settings(), &r).unwrap();
                session
                    .render(&current, &context, Some(32), None, EightBit, None)
                    .unwrap();
                let weak_source =
                    Arc::downgrade(current.loaded().prepared_source.as_ref().unwrap());
                o.close();
                session.retire_if_stale(&o);
                assert!(session.entry.is_none());
                assert!(
                    session
                        .render(&current, &context, Some(32), None, EightBit, None)
                        .is_err()
                );
                drop((current, first, images));
                assert!(weak_source.upgrade().is_none());
                let (o2, k2, images2) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let next = request(&o2, k2, None, settings(), &r).unwrap();
                session
                    .render(&next, &context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(session.entry.as_ref().unwrap().0, k2);
                let (other_context, other_errors) =
                    crate::gpu_processing::preview_reuse_hardware_context();
                session
                    .render(&next, &other_context, Some(32), None, EightBit, None)
                    .unwrap();
                assert!(
                    session
                        .entry
                        .as_ref()
                        .unwrap()
                        .1
                        .matches_context(&other_context)
                );
                assert!(!session.entry.as_ref().unwrap().1.matches_context(&context));
                let (o3, k3, images3) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let replacement = request(&o3, k3, None, settings(), &r).unwrap();
                session
                    .render(&replacement, &other_context, Some(32), None, EightBit, None)
                    .unwrap();
                assert_eq!(session.entry.as_ref().unwrap().0, k3);
                assert_eq!(session.entry.as_ref().unwrap().1.processor_build_count(), 1);
                session.clear(); // Actual worker Legacy handoff uses this exact retirement.
                assert!(session.entry.is_none());
                drop((next, images2, replacement, images3));
                assert!(errors.lock().unwrap().is_empty());
                assert!(other_errors.lock().unwrap().is_empty());
            }

            #[tokio::test]
            async fn retained_genuine_source_assets_and_normal_base_reuse() {
                let r = Arc::new(registry());
                let (o, k, images) =
                    ready(&patterned_composition_dng(), json!({}), Arc::clone(&r)).await;
                let q = request(&o, k, None, settings(), &r).unwrap();
                let retained = o.accurate_snapshot(Some(&k)).unwrap();
                assert!(Arc::ptr_eq(
                    &q.appearance().unwrap().retained_source(),
                    q.loaded().prepared_source.as_ref().unwrap()
                ));
                assert!(std::ptr::eq(
                    q.appearance().unwrap(),
                    &retained.retained().unwrap().appearance
                ));
                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                assert_eq!(
                    q.unpatched().unwrap().as_bytes(),
                    images.1.0.image.as_bytes()
                );
                q.with_full_composition(|c| {
                    assert_eq!(c.pixels().dimensions(), images.1.0.image.dimensions());
                    Ok(())
                })
                .unwrap();
                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                {
                    let base = q.request_base().unwrap();
                    let assets = q.creative().unwrap();
                    let fit = crate::open_composition::compose_fit(
                        &base,
                        assets,
                        crate::open_composition::FitPreviewSpec::new(32).unwrap(),
                        None,
                    )
                    .unwrap();
                    let preview = crate::open_composition::compose_accurate_preview(
                        &base, assets, 32, None, None,
                    )
                    .unwrap();
                    assert_eq!(fit.pixels().as_bytes(), preview.pixels().as_bytes());
                    let interactive = crate::open_composition::compose_accurate_preview(
                        &base,
                        assets,
                        32,
                        Some(2.0),
                        None,
                    )
                    .unwrap();
                    let expected =
                        crate::image_processing::downscale_f32_image(fit.pixels(), 16, 16);
                    assert_eq!(interactive.pixels().as_bytes(), expected.as_bytes());
                    assert!(crate::open_composition::FitPreviewSpec::new(3000).is_err());
                    let zoom = crate::open_composition::compose_accurate_preview(
                        &base, assets, 3000, None, None,
                    )
                    .unwrap();
                    assert_eq!(zoom.pixels().dimensions(), base.pixels().dimensions());
                    assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                }
                let (r, id, dcp, look) = selected();
                let (o, k, _) = ready(
                    &synthetic_dng(true, 6),
                    json!({"profile":{"base":id,"look":"composition-look","amount":100}}),
                    Arc::clone(&r),
                )
                .await;
                let q = request(&o, k, None, settings(), &r).unwrap();
                let before = q.unpatched().unwrap();
                fs::remove_file(dcp).unwrap();
                fs::remove_file(look).unwrap();
                fs::remove_file(&q.loaded().path).unwrap();
                assert_eq!(q.unpatched().unwrap().as_bytes(), before.as_bytes());
                assert!(q.appearance().unwrap().assets.is_some());
            }
            #[tokio::test]
            async fn edited_snapshot_is_fresh_immutable_and_never_initial_fallback() {
                let (r, id, _, _) = selected();
                let (o, k, _) = ready(
                    &synthetic_dng(true, 6),
                    json!({"exposure":1.0}),
                    Arc::clone(&r),
                )
                .await;
                let mut edit = json!({"exposure":2.0,"profile":{"base":id,"look":"composition-look","amount":0}});
                let q = request(&o, k, Some(edit.clone()), settings(), &r).unwrap();
                edit["exposure"] = json!(99.0);
                assert_eq!(q.adjustments()["exposure"], 2.0);
                assert_eq!(q.creative().unwrap().adjustments()["exposure"], 2.0);
                assert_eq!(
                    request(&o, k, None, settings(), &r).unwrap().adjustments()["exposure"],
                    1.0
                );
                edit["profile"]["amount"] = json!(200);
                let q2 = request(&o, k, Some(edit), settings(), &r).unwrap();
                assert_eq!(q.adjustments()["profile"]["amount"], 0);
                assert_eq!(q2.adjustments()["profile"]["amount"], 200);
                assert!(Arc::ptr_eq(
                    &q.appearance().unwrap().source,
                    &q2.appearance().unwrap().source
                ));
                assert!(matches!(
                    request(&o, k, Some(Value::Null), settings(), &r),
                    Err(RequestCaptureError::InvalidAdjustments)
                ));
                assert!(matches!(
                    request(
                        &o,
                        k,
                        Some(json!({"profile":{"base":"missing"}})),
                        settings(),
                        &r
                    ),
                    Err(RequestCaptureError::Profile)
                ));
                let mut changed = settings();
                changed.raw_preprocessing_sharpening = Some(0.35);
                assert!(matches!(
                    request(&o, k, Some(json!({})), changed, &r),
                    Err(RequestCaptureError::SourceSettingsChanged)
                ));
                let mut changed = settings();
                changed.raw_highlight_compression = Some(7.0);
                assert!(matches!(
                    request(&o, k, Some(json!({})), changed, &r),
                    Err(RequestCaptureError::SourceSettingsChanged)
                ));
                assert!(q.check().is_ok());
            }
            #[tokio::test]
            async fn foreign_pair_and_missing_retained_descriptor_are_rejected() {
                let bytes = synthetic_dng(true, 6);
                let path = file(&bytes, "dng");
                let a = owner();
                let ka = admit(&a, path.clone(), metadata(json!({})), settings(), true).await;
                render(&a, ka, Arc::new(registry())).await.unwrap();
                let foreign = Arc::clone(
                    &a.accurate_snapshot(Some(&ka))
                        .unwrap()
                        .retained()
                        .unwrap()
                        .spec,
                );
                let b = owner();
                let kb = admit(&b, path.clone(), metadata(json!({})), settings(), true).await;
                assert_eq!(ka, kb);
                tokio::time::timeout(
                    Duration::from_secs(3),
                    b.execute(
                        kb,
                        JobKind::Pixels,
                        move |c| {
                            let spec = c.pixels_spec()?;
                            let KeyedPreparation::Prepared(pair) =
                                prepare_captured_decision(c, &bytes, &registry())?
                            else {
                                panic!("native")
                            };
                            let (candidate, appearance) = render_keyed_decision_owned(
                                &bytes,
                                &path,
                                &spec,
                                PreparedAppearanceDecision::Prepared(pair.appearance),
                                PreparedPatchPolicy::RequireCaptured,
                                c.raw_cancel(),
                            )?;
                            let token = RetainedPreparedCandidate {
                                candidate,
                                retained: Arc::new(RetainedOpenAppearance {
                                    spec: foreign,
                                    appearance: appearance.unwrap(),
                                }),
                            };
                            assert!(matches!(
                                OpenCandidate::from_retained(token),
                                Err(OpenError::Stale | OpenError::CommitFailed)
                            ));
                            // Old compatibility constructor still commits; new request admission cannot invent its absent owner.
                            prepare_keyed_candidate(
                                &bytes,
                                &path,
                                spec,
                                &registry(),
                                c.raw_cancel(),
                            )
                        },
                        finish,
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                assert!(matches!(
                    request(&b, kb, None, settings(), &registry()),
                    Err(RequestCaptureError::Open(OpenError::Unavailable))
                ));
            }
            #[tokio::test]
            async fn pending_aba_and_missing_key_never_fall_back() {
                let r = Arc::new(registry());
                let (o, k, images) =
                    ready(&synthetic_dng(true, 6), json!({}), Arc::clone(&r)).await;
                let original = Mutex::new(Some(images.1.0));
                let generation = Arc::new(AtomicUsize::new(0));
                assert!(
                    capture_consumer_source_parts(&o, &original, &generation, None, None).is_err()
                );
                assert!(
                    capture_consumer_source_parts(
                        &o,
                        &original,
                        &generation,
                        Some(&k),
                        Some("wrong")
                    )
                    .is_err()
                );
                let q = request(&o, k, None, settings(), &r).unwrap();
                let skipped = o.reserve(B, "different.dng".into()).unwrap();
                let next = o.reserve(A, q.loaded().path.clone()).unwrap();
                assert_ne!(skipped, next);
                assert_ne!(k, next);
                assert!(q.check().is_err());
                assert!(
                    capture_consumer_source_parts(&o, &original, &generation, Some(&next), None)
                        .is_err()
                );
                assert!(
                    capture_consumer_source_parts(&o, &original, &generation, None, None).is_err()
                );
                o.close();
                assert!(q.check().is_err());
            }
            #[tokio::test]
            async fn last_real_request_and_analytics_owner_fence_next_worker() {
                let r = Arc::new(registry());
                let o = owner();
                let path = file(&synthetic_dng(true, 6), "dng");
                let k = o.reserve(A, path).unwrap();
                let dropped = Arc::new(AtomicBool::new(false));
                let unlocked = Arc::new(AtomicBool::new(false));
                let d = Arc::clone(&dropped);
                let u = Arc::clone(&unlocked);
                tokio::time::timeout(
                    Duration::from_secs(3),
                    o.execute(
                        k,
                        JobKind::Snapshot,
                        move |c| {
                            let mut captured =
                                c.capture_snapshot(metadata(json!({})), settings())?;
                            captured.observe_retirement_for_test(d, u);
                            Ok(captured)
                        },
                        |c, p| c.finish_captured_snapshot(p),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                drop(render(&o, k, Arc::clone(&r)).await.unwrap());
                let q = request(&o, k, None, settings(), &r).unwrap();
                let weak = Arc::downgrade(&q.appearance().unwrap().source);
                let callback = crate::app_state::AnalyticsJob {
                    request: Some(Arc::clone(&q)),
                    path: q.loaded().path.clone(),
                    image: Arc::clone(&q.loaded().image),
                    compute_waveform: false,
                    active_waveform_channel: None,
                };
                let (release, wait) = mpsc::channel();
                let (entered_callback, entered) = mpsc::channel();
                let callback_thread = std::thread::spawn(move || {
                    entered_callback.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(callback.request.as_ref().unwrap().check().is_err());
                    assert_eq!(callback.request.as_ref().unwrap().key(), Some(k));
                    drop(callback);
                });
                entered.recv_timeout(Duration::from_secs(2)).unwrap();
                let next = o.reserve(B, "next.png".into()).unwrap();
                let (entered, mut rx) = oneshot::channel();
                let run = Arc::clone(&o);
                let job = tokio::spawn(async move {
                    run.execute(
                        next,
                        JobKind::Snapshot,
                        move |c| {
                            let _ = entered.send(());
                            c.capture_snapshot(metadata(json!({})), settings())
                        },
                        |c, p| c.finish_captured_snapshot(p),
                    )
                    .await
                });
                assert!(
                    tokio::time::timeout(Duration::from_millis(30), &mut rx)
                        .await
                        .is_err()
                );
                q.cancel();
                assert!(q.check().is_err());
                drop(q);
                assert!(weak.upgrade().is_some());
                assert!(
                    tokio::time::timeout(Duration::from_millis(30), &mut rx)
                        .await
                        .is_err()
                );
                release.send(()).unwrap();
                callback_thread.join().unwrap();
                tokio::time::timeout(Duration::from_secs(3), rx)
                    .await
                    .unwrap()
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(3), job)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert!(weak.upgrade().is_none());
                assert!(dropped.load(Ordering::SeqCst));
                assert!(unlocked.load(Ordering::SeqCst));
                assert!(o.gate_available_for_test());
            }
            #[tokio::test]
            async fn legacy_lease_generation_and_keyed_refusal() {
                for bytes in [synthetic_dng(false, 1), png_bytes()] {
                    let r = Arc::new(registry());
                    let (o, _, images) = ready(&bytes, json!({}), r).await;
                    let original = Mutex::new(Some(images.1.0));
                    let generation = Arc::new(AtomicUsize::new(4));
                    assert!(
                        capture_consumer_source_parts(
                            &o,
                            &Mutex::new(None),
                            &generation,
                            None,
                            None
                        )
                        .is_err()
                    );
                    tokio::time::timeout(Duration::from_secs(1), o.wait_legacy_drain())
                        .await
                        .unwrap()
                        .unwrap();
                    let source =
                        capture_consumer_source_parts(&o, &original, &generation, None, None)
                            .unwrap();
                    assert!(source.key().is_none());
                    assert!(source.check().is_ok());
                    generation.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(source.check(), Err(OpenError::Stale));
                    drop(source);
                    let lease =
                        capture_consumer_source_parts(&o, &original, &generation, None, None)
                            .unwrap();
                    let wait = o.wait_legacy_drain();
                    assert!(
                        tokio::time::timeout(Duration::from_millis(20), wait)
                            .await
                            .is_err()
                    );
                    drop(lease);
                    tokio::time::timeout(Duration::from_secs(1), o.wait_legacy_drain())
                        .await
                        .unwrap()
                        .unwrap();
                    o.reserve(B, "pending.png".into()).unwrap();
                    assert!(
                        capture_consumer_source_parts(&o, &original, &generation, None, None)
                            .is_err()
                    );
                }
            }
            #[tokio::test]
            async fn strict_separate_mask_uses_full_reference_and_reports_errors() {
                let r = Arc::new(registry());
                let (o, k, _) = ready(&black_dng(), json!({}), Arc::clone(&r)).await;
                let input = patch_contract_adjustments(None, Some((1, 1)), (10, 10));
                let q = request(&o, k, Some(input), settings(), &r).unwrap();
                let reference = q.full_reference().unwrap();
                assert_eq!(reference.dimensions(), (520, 512));
                assert!(reference.to_rgb32f().as_raw().iter().all(|v| *v == 0.0));
                assert_ne!(
                    q.patched().unwrap().as_bytes(),
                    q.unpatched().unwrap().as_bytes()
                );
                let bitmap = patch_contract_png(DynamicImage::ImageLuma8(
                    image::GrayImage::from_pixel(520, 512, image::Luma([128])),
                ));
                let mask=crate::mask_generation::capture_standalone_mask(&json!({"id":"same","name":"mask","visible":true,"invert":false,"adjustments":{},"subMasks":[{"id":"child","type":"ai-sky","visible":true,"opacity":100,"mode":"additive","parameters":{"mask_data_base64":bitmap}}]})).unwrap();
                let got = q
                    .mask_bitmap(&mask, 520, 512, 1.0, (0.0, 0.0))
                    .unwrap()
                    .unwrap();
                assert!(got.as_raw().iter().all(|v| *v == 128));
                assert!(crate::mask_generation::capture_standalone_mask(&json!({"id":"bad","name":"bad","visible":true,"invert":false,"adjustments":{},"subMasks":[{"id":"missing","type":"ai-sky","visible":true,"mode":"additive","parameters":{}}]})).is_err());
                q.cancel();
                assert!(q.mask_bitmap(&mask, 520, 512, 1.0, (0.0, 0.0)).is_err());
            }
            #[tokio::test]
            async fn queue_reversal_cancellation_and_failed_stage_keep_authority() {
                let r = Arc::new(registry());
                let (o, k, _) = ready(&synthetic_dng(true, 6), json!({}), Arc::clone(&r)).await;
                let first = request(&o, k, Some(json!({"exposure":1})), settings(), &r).unwrap();
                let second = request(&o, k, Some(json!({"exposure":2})), settings(), &r).unwrap();
                let weak = Mutex::new(Some(Arc::downgrade(&first)));
                crate::app_state::replace_preview_request(&weak, &second).unwrap();
                assert!(first.check().is_err());
                assert!(second.check().is_ok());
                assert_eq!(first.adjustments()["exposure"], 1);
                assert_eq!(second.adjustments()["exposure"], 2);
                let result = second.stage(|| {
                    o.reserve(B, "other.png".into()).unwrap();
                    Err::<(), _>("stage failure".into())
                });
                assert_ne!(result, Err("stage failure".into()));
                assert!(result.is_err());
                assert_eq!(second.key(), Some(k));
                let third = owner();
                let key = third.reserve(A, "none".into()).unwrap();
                assert!(third.accurate_snapshot(Some(&key)).is_err());
            }
        }

        // BEGIN C3c fixed eight fit/event groups. Source definitions, not execution.
        mod fit_event_tests {
            use super::*;
            use crate::image_processing::GpuContext;
            use crate::progressive_open::{FitObservation, PlaceholderEvent, RenderMode};
            use std::io::Write;

            fn poisoned_gpu() -> Arc<Mutex<Option<GpuContext>>> {
                let slot = Arc::new(Mutex::new(None));
                let fault = Arc::clone(&slot);
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                        let _guard = fault.lock().unwrap();
                        panic!("intentional GPU-slot poison fixture");
                    }))
                    .is_err()
                );
                slot
            }

            async fn cpu_fit(
                owner: &Arc<OpenOwner>,
                key: OpenKey,
                registry: Arc<crate::dcp::registry::ProfileRegistry>,
                gpu: Arc<Mutex<Option<GpuContext>>>,
            ) -> std::result::Result<Images, OpenError> {
                let authority = Arc::clone(owner);
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            read_keyed_candidate_with_fit(c, &registry, &authority, &gpu, |_| {
                                panic!("CPU absence/Legacy must not emit")
                            })
                        },
                        finish,
                    ),
                )
                .await
                .unwrap()
            }

            fn codec_image() -> DynamicImage {
                DynamicImage::ImageRgba8(image::RgbaImage::from_fn(8, 5, |x, y| {
                    image::Rgba([(x * 27) as u8, (y * 43) as u8, 91, 255])
                }))
            }

            fn codec_event(key: OpenKey) -> PlaceholderEvent {
                let bytes = encode_fit_jpeg(&codec_image(), 64).unwrap();
                PlaceholderEvent::from_jpeg(key, 8, 5, 64, &bytes).unwrap()
            }

            async fn admit_witness(
                owner: &OpenOwner,
                path: String,
                s: AppSettings,
            ) -> (OpenKey, Arc<AtomicBool>, Arc<AtomicBool>) {
                let key = owner.reserve(A, path).unwrap();
                let dropped = Arc::new(AtomicBool::new(false));
                let unlocked = Arc::new(AtomicBool::new(false));
                let d = Arc::clone(&dropped);
                let u = Arc::clone(&unlocked);
                tokio::time::timeout(
                    Duration::from_secs(3),
                    owner.execute(
                        key,
                        JobKind::Snapshot,
                        move |c| {
                            let mut snapshot = c.capture_snapshot(metadata(json!({})), s)?;
                            snapshot.observe_retirement_for_test(d, u);
                            Ok(snapshot)
                        },
                        |c, snapshot| c.finish_captured_snapshot(snapshot),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                (key, dropped, unlocked)
            }

            #[tokio::test]
            async fn open_diagnostics_preserve_capture_and_factory_failures() {
                for (input, expected_stage, expected_cause) in [
                    (
                        json!({"fogEnabled":true}),
                        "creative.use",
                        "missing_effect_map",
                    ),
                    (
                        json!({"profile":{"look":"missing"}}),
                        "factory.profile_assets",
                        "opaque_error",
                    ),
                ] {
                    let o = owner();
                    let path = file(&synthetic_dng(true, 6), "dng");
                    let key = admit(&o, path, metadata(input), settings(), true).await;
                    let authority = Arc::clone(&o);
                    let records = Arc::new(Mutex::new(Vec::new()));
                    let observed = Arc::clone(&records);
                    let result = tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                crate::progressive_open::observe_open_failures_for_test();
                                let result = read_keyed_candidate_with_fit(
                                    c,
                                    &registry(),
                                    &authority,
                                    &Mutex::new(None),
                                    |_| panic!("failure emitted"),
                                );
                                *observed.lock().unwrap() =
                                    crate::progressive_open::take_open_failures_for_test();
                                result
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap();
                    assert!(matches!(result, Err(OpenError::DevelopmentFailed)));
                    assert!(
                        records
                            .lock()
                            .unwrap()
                            .contains(&(expected_stage, expected_cause))
                    );
                }
            }

            #[tokio::test]
            async fn open_diagnostics_keep_fit_postcheck_authoritative() {
                let o = owner();
                let path = file(&synthetic_dng(true, 6), "dng");
                let key = admit(&o, path, metadata(json!({})), settings(), true).await;
                let authority = Arc::clone(&o);
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    o.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let spec = c.pixels_spec()?;
                            crate::progressive_open::observe_open_failures_for_test();
                            let result =
                                fit_stage(c, &authority, &spec, FitObservation::Base, || {
                                    authority.cancel(key)?;
                                    Err::<(), _>(OpenError::DevelopmentFailed)
                                });
                            let records = crate::progressive_open::take_open_failures_for_test();
                            assert_eq!(result, Err(OpenError::Cancelled));
                            assert!(records.contains(&("fit.postcheck", "cancelled")));
                            result
                        },
                        |_, ()| -> std::result::Result<(), OpenError> {
                            panic!("cancelled work published")
                        },
                    ),
                )
                .await
                .unwrap();
                assert_eq!(result, Err(OpenError::Cancelled));
            }

            #[tokio::test]
            async fn absence_legacy_and_poison_policy() {
                let owner = owner();
                let bytes = synthetic_dng(true, 6);
                let path = file(&bytes, "dng");
                let key = admit(&owner, path, metadata(Value::Null), settings(), true).await;
                let observed = counts(&owner);
                let actual = cpu_fit(
                    &owner,
                    key,
                    Arc::new(registry()),
                    Arc::new(Mutex::new(None)),
                )
                .await
                .unwrap();
                assert_eq!(actual.0.render_mode, RenderMode::AccurateXtrans1Encoded);
                assert!(actual.0.metadata.adjustments.is_null());
                assert_eq!(
                    observed.each_ref().map(|n| n.load(Ordering::SeqCst)),
                    [1, 1, 1]
                );
                for (input, expected) in [
                    (json!({}), OpenError::Poisoned),
                    (json!({"fogEnabled":true}), OpenError::DevelopmentFailed),
                    (
                        json!({"profile":{"look":"missing"}}),
                        OpenError::DevelopmentFailed,
                    ),
                ] {
                    let o = super::owner();
                    let path = file(&bytes, "dng");
                    let key = admit(&o, path, metadata(input), settings(), true).await;
                    let result = cpu_fit(&o, key, Arc::new(registry()), poisoned_gpu()).await;
                    assert!(matches!(result,Err(e) if e==expected));
                }
                for (bytes, suffix) in [(synthetic_dng(false, 6), "dng"), (png_bytes(), "png")] {
                    let o = super::owner();
                    let path = file(&bytes, suffix);
                    let key = admit(
                        &o,
                        path.clone(),
                        metadata(json!({"fogEnabled":true})),
                        settings(),
                        true,
                    )
                    .await;
                    let authority = Arc::clone(&o);
                    let gpu = poisoned_gpu();
                    tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let r = registry();
                                let old = prepare_keyed_candidate(
                                    &bytes,
                                    &path,
                                    c.pixels_spec()?,
                                    &r,
                                    c.raw_cancel(),
                                )?;
                                let actual =
                                    read_keyed_candidate_with_fit(c, &r, &authority, &gpu, |_| {
                                        panic!("Legacy emitted")
                                    })?;
                                assert_eq!(
                                    old.images_for_test().0.as_bytes(),
                                    actual.images_for_test().0.as_bytes()
                                );
                                assert_eq!(
                                    old.images_for_test().1.as_bytes(),
                                    actual.images_for_test().1.as_bytes()
                                );
                                Ok(actual)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                }
                // Actual Apple-preference decision, deliberately no OS/Apple rendering.
                let o = super::owner();
                let mut s = settings();
                s.use_apple_raw9 = Some(true);
                let key = admit(
                    &o,
                    "explicit-apple.dng".into(),
                    metadata(json!({"fogEnabled":true})),
                    s,
                    true,
                )
                .await;
                let authority = Arc::clone(&o);
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    o.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let decision =
                                prepare_captured_decision(c, b"not decoded", &registry())?;
                            assert!(matches!(
                                &decision,
                                KeyedPreparation::LegacyRequired {
                                    reason: LegacyPreparedReason::AppleRawPreferred,
                                    ..
                                }
                            ));
                            decision.offer_fit(c, &authority, &poisoned_gpu(), |_| {
                                panic!("Apple decision emitted")
                            })?;
                            Err::<(), _>(OpenError::SnapshotFailed)
                        },
                        |_, ()| Ok(()),
                    ),
                )
                .await
                .unwrap();
                assert_eq!(result, Err(OpenError::SnapshotFailed));
            }

            #[tokio::test]
            async fn owner_spec_and_cancellation_authority() {
                let path = file(&synthetic_dng(true, 6), "dng");
                let other = owner();
                let other_key =
                    admit(&other, path.clone(), metadata(json!({})), settings(), true).await;
                let held = Arc::new(Mutex::new(None));
                let put = Arc::clone(&held);
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    other.execute(
                        other_key,
                        JobKind::Pixels,
                        move |c| {
                            *put.lock().unwrap() = Some(c.pixels_spec()?);
                            Err::<(), _>(OpenError::DevelopmentFailed)
                        },
                        |_, ()| Ok(()),
                    ),
                )
                .await
                .unwrap();
                assert_eq!(result, Err(OpenError::DevelopmentFailed));
                let foreign_spec = held.lock().unwrap().take().unwrap();
                for mode in ["foreign", "wrong-spec", "cancel", "aba"] {
                    let o = owner();
                    let key = admit(&o, path.clone(), metadata(json!({})), settings(), true).await;
                    assert_eq!(key, other_key); // Equal wire key is insufficient across owners.
                    let authority = Arc::clone(&o);
                    let foreign = Arc::clone(&other);
                    let wrong = Arc::clone(&foreign_spec);
                    let path = path.clone();
                    let observed = counts(&o);
                    let result = tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                if mode == "wrong-spec" {
                                    return check_fit_current(c, &authority, &wrong);
                                }
                                if mode == "cancel" {
                                    authority.cancel(key)?;
                                }
                                if mode == "aba" {
                                    authority.reserve(B, path.clone())?;
                                    let latest = authority.reserve(A, path)?;
                                    assert_ne!(key, latest);
                                }
                                let chosen = if mode == "foreign" {
                                    &foreign
                                } else {
                                    &authority
                                };
                                read_keyed_candidate_with_fit(
                                    c,
                                    &registry(),
                                    chosen,
                                    &Mutex::new(None),
                                    |_| panic!("rejected work emitted"),
                                )
                                .map(|_| ())
                            },
                            |_, ()| Ok(()),
                        ),
                    )
                    .await
                    .unwrap();
                    let expected = if matches!(mode, "cancel" | "aba") {
                        OpenError::Cancelled
                    } else {
                        OpenError::Stale
                    };
                    assert_eq!(result, Err(expected));
                    assert_eq!(
                        observed.each_ref().map(|n| n.load(Ordering::SeqCst)),
                        [0, 0, 0]
                    );
                }
                let o = owner();
                let key = o.reserve(A, path).unwrap();
                let authority = Arc::clone(&o);
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    o.execute(
                        key,
                        JobKind::Snapshot,
                        move |c| {
                            read_keyed_candidate_with_fit(
                                c,
                                &registry(),
                                &authority,
                                &Mutex::new(None),
                                |_| panic!("Snapshot emitted"),
                            )
                            .map(|_| ())
                        },
                        |_, ()| Ok(()),
                    ),
                )
                .await
                .unwrap();
                assert_eq!(result, Err(OpenError::Unavailable));
            }

            #[tokio::test]
            async fn currentfull_captured_composition_and_error_postchecks() {
                for selected_profile in [false, true] {
                    let (r, id, dcp, look) = selected();
                    let bytes = black_dng();
                    let path = file(&bytes, "dng");
                    let mut input = patch_contract_adjustments(None, Some((1, 1)), (10, 10));
                    let bitmap = patch_contract_png(DynamicImage::ImageLuma8(
                        image::GrayImage::from_pixel(520, 512, image::Luma([128])),
                    ));
                    input["aiPatches"][0]["subMasks"] = json!([{"id":"bitmap","type":"ai-sky","visible":true,"invert":false,"opacity":100.0,"mode":"additive","parameters":{"mask_data_base64":bitmap}}]);
                    if selected_profile {
                        input["profile"] =
                            json!({"base":id,"look":"composition-look","amount":100});
                    }
                    let o = owner();
                    let original = metadata(input);
                    let saved = original.adjustments.clone();
                    let mut s = settings();
                    s.editor_preview_resolution = Some(1920);
                    let key = admit(&o, path, original, s, true).await;
                    let authority = Arc::clone(&o);
                    let stages = Arc::new(Mutex::new(Vec::new()));
                    let seen = Arc::clone(&stages);
                    o.observe_fit_for_test(Arc::new(move |stage| seen.lock().unwrap().push(stage)));
                    tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let decision = prepare_captured_decision(c, &bytes, &r)?;
                                let KeyedPreparation::Prepared(pair) = &decision else {
                                    panic!("genuine native pair")
                                };
                                fs::remove_file(dcp).unwrap();
                                fs::remove_file(look).unwrap();
                                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                                let actual =
                                    pair.with_current_fit(c, &authority, |fit, limit| {
                                        assert_eq!(limit, 1920);
                                        assert_eq!(fit.pixels().dimensions(), (520, 512));
                                        let request = fit.request();
                                        assert_eq!(request.adjustments.global.has_dcp, 0);
                                        assert!(
                                            request.dcp_textures.is_none() && request.roi.is_none()
                                        );
                                        Ok(fit.pixels().to_rgb32f())
                                    })?;
                                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                                assert_eq!(pair.spec.metadata().adjustments, saved);
                                let base = pair
                                    .appearance
                                    .render_unpatched_base(
                                        AppearancePass::CurrentFull,
                                        c.raw_cancel(),
                                    )
                                    .unwrap();
                                let once = composite_patches_on_image(
                                    base.pixels(),
                                    pair.spec.captured_creative()?.adjustments(),
                                )
                                .unwrap();
                                let twice = composite_patches_on_image(
                                    &once,
                                    pair.spec.captured_creative()?.adjustments(),
                                )
                                .unwrap();
                                assert_eq!(actual.as_raw(), once.to_rgb32f().as_raw());
                                assert_ne!(actual.as_raw(), twice.to_rgb32f().as_raw());
                                if !selected_profile {
                                    assert_eq!(
                                        actual.get_pixel(10, 10).0.map(f32::to_bits),
                                        [1056997505, 0, 0]
                                    );
                                    let pristine = base.pixels().to_rgb32f();
                                    for (x, y, pixel) in actual.enumerate_pixels() {
                                        if (x, y) != (10, 10) {
                                            assert_eq!(
                                                pixel.0.map(f32::to_bits),
                                                pristine.get_pixel(x, y).0.map(f32::to_bits)
                                            );
                                        }
                                    }
                                }
                                drop(base);
                                assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                                decision.into_candidate(c, &bytes)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(
                        *stages.lock().unwrap(),
                        vec![FitObservation::Base, FitObservation::Composition]
                    );
                }
                for cancelled in [false, true] {
                    let o = owner();
                    let bytes = black_dng();
                    let path = file(&bytes, "dng");
                    let bitmap = patch_contract_png(DynamicImage::ImageLuma8(
                        image::GrayImage::from_pixel(2, 2, image::Luma([128])),
                    ));
                    let key = admit(
                        &o,
                        path,
                        metadata(patch_contract_adjustments(
                            Some(&bitmap),
                            Some((1, 1)),
                            (0, 0),
                        )),
                        settings(),
                        true,
                    )
                    .await;
                    let saw = Arc::new(AtomicBool::new(false));
                    let recorded = Arc::clone(&saw);
                    let weak = Arc::downgrade(&o);
                    o.observe_fit_for_test(Arc::new(move |stage| {
                        if stage == FitObservation::Composition {
                            recorded.store(true, Ordering::SeqCst);
                            if cancelled {
                                weak.upgrade().unwrap().cancel(key).unwrap();
                            }
                        }
                    }));
                    let authority = Arc::clone(&o);
                    let result = tokio::time::timeout(
                        Duration::from_secs(3),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let decision = prepare_captured_decision(c, &bytes, &registry())?;
                                let KeyedPreparation::Prepared(pair) = &decision else {
                                    panic!("genuine native pair")
                                };
                                pair.with_current_fit(c, &authority, |_, _| {
                                    panic!("real malformed patch must not compose")
                                })
                            },
                            |_, ()| Ok(()),
                        ),
                    )
                    .await
                    .unwrap();
                    assert_eq!(
                        result,
                        Err(if cancelled {
                            OpenError::Cancelled
                        } else {
                            OpenError::DevelopmentFailed
                        })
                    );
                    assert!(saw.load(Ordering::SeqCst));
                }
            }

            #[test]
            fn bounded_jpeg_and_real_event() {
                for (setting, expected) in [
                    (None, 1920),
                    (Some(0), 1920),
                    (Some(1), 1),
                    (Some(1280), 1280),
                    (Some(1920), 1920),
                    (Some(4096), 1920),
                ] {
                    let mut s = settings();
                    s.editor_preview_resolution = setting;
                    assert_eq!(captured_fit_limit(&s), expected);
                }
                let key: OpenKey =
                    serde_json::from_value(json!({"ticket":A,"generation":"1"})).unwrap();
                let bytes = encode_fit_jpeg(&codec_image(), 64).unwrap();
                assert!(bytes.len() <= 2 * 1024 * 1024);
                let decoded =
                    image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap();
                assert_eq!(decoded.dimensions(), (8, 5));
                let event = PlaceholderEvent::from_jpeg(key, 8, 5, 64, &bytes).unwrap();
                let value = serde_json::to_value(&event).unwrap();
                assert_eq!(value["schemaVersion"], 1);
                assert_eq!(value["openKey"], serde_json::to_value(key).unwrap());
                assert_eq!(value["mime"], "image/jpeg");
                assert_eq!(
                    general_purpose::STANDARD
                        .decode(value["dataBase64"].as_str().unwrap())
                        .unwrap(),
                    bytes
                );
                assert_eq!(value.as_object().unwrap().len(), 6);
                assert!(serde_json::to_vec(&event).unwrap().len() <= 3 * 1024 * 1024);
                assert!(matches!(
                    encode_fit_jpeg(&codec_image(), 1),
                    Err(OpenError::InvalidPlaceholder)
                ));
                for (w, h) in [(0, 1), (1921, 1)] {
                    let bad = DynamicImage::ImageRgba8(image::RgbaImage::new(w, h));
                    assert!(matches!(
                        encode_fit_jpeg(&bad, 1920),
                        Err(OpenError::InvalidPlaceholder)
                    ));
                }
                assert!(matches!(
                    PlaceholderEvent::from_jpeg(key, 8, 5, 64, b"not jpeg"),
                    Err(OpenError::InvalidPlaceholder)
                ));
                assert!(matches!(
                    PlaceholderEvent::from_jpeg(key, 8, 5, 64, &vec![0; 2 * 1024 * 1024 + 1]),
                    Err(OpenError::InvalidPlaceholder)
                ));
                let mut sink = FitJpegWriter { bytes: Vec::new() };
                sink.write_all(&vec![7; 2 * 1024 * 1024]).unwrap();
                assert_eq!(sink.bytes.len(), 2 * 1024 * 1024);
                assert!(sink.write(&[8]).is_err());
                assert_eq!(sink.bytes.len(), 2 * 1024 * 1024);
                assert!(sink.bytes.iter().all(|b| *b == 7));
            }

            // Real owned transport and real owner worker. CPU mode isolates codec/
            // lease semantics; hardware mode actually enters composition and H5.
            async fn transport_case(gpu: Option<GpuContext>, mode: &'static str) {
                let hardware = gpu.is_some();
                let o = owner();
                let path = file(&patterned_composition_dng(), "dng");
                let mut s = settings();
                s.editor_preview_resolution = Some(64);
                let (key, dropped, unlocked) = admit_witness(&o, path, s).await;
                let gpu = Arc::new(Mutex::new(gpu));
                let authority = Arc::clone(&o);
                let witnesses = Arc::new(Mutex::new(None));
                let recorded = Arc::clone(&witnesses);
                let peer = Arc::new(Mutex::new(None));
                let peer_out = Arc::clone(&peer);
                let emitted = Arc::new(AtomicUsize::new(0));
                let seen = Arc::clone(&emitted);
                let result = tokio::time::timeout(
                    Duration::from_secs(if hardware { 120 } else { 3 }),
                    o.execute(
                        key,
                        JobKind::Pixels,
                        move |c| {
                            let (bytes, decision) = read_keyed_preparation(c, &registry())?;
                            let KeyedPreparation::Prepared(pair) = &decision else {
                                panic!("actual prepared owner required")
                            };
                            let spec_weak = Arc::downgrade(&pair.spec);
                            let source_weak = Arc::downgrade(&pair.appearance.source);
                            *recorded.lock().unwrap() =
                                Some((spec_weak.clone(), source_weak.clone()));
                            let emit = |event: &PlaceholderEvent| {
                                seen.fetch_add(1, Ordering::SeqCst);
                                let wire = serde_json::to_value(event).unwrap();
                                assert_eq!(wire["openKey"], serde_json::to_value(key).unwrap());
                                assert!(authority.gate_available_for_test());
                                assert!(
                                    spec_weak.upgrade().is_some()
                                        && source_weak.upgrade().is_some()
                                );
                                let (release, wait) = mpsc::channel();
                                let next_owner = Arc::clone(&authority);
                                let peer_spec = spec_weak.clone();
                                let peer_source = source_weak.clone();
                                let peer_task = tokio::spawn(async move {
                                    if mode == "supersede" {
                                        let latest =
                                            next_owner.reserve(B, "next.dng".into()).unwrap();
                                        let calls = Arc::new(AtomicUsize::new(0));
                                        let called = Arc::clone(&calls);
                                        let future = next_owner.execute(
                                            latest,
                                            JobKind::Snapshot,
                                            move |next| {
                                                called.fetch_add(1, Ordering::SeqCst);
                                                next.capture_snapshot(
                                                    metadata(json!({})),
                                                    settings(),
                                                )
                                            },
                                            |next, snapshot| {
                                                next.finish_captured_snapshot(snapshot)
                                            },
                                        );
                                        tokio::pin!(future);
                                        assert!(
                                            tokio::time::timeout(
                                                Duration::from_millis(20),
                                                future.as_mut()
                                            )
                                            .await
                                            .is_err()
                                        );
                                        assert_eq!(calls.load(Ordering::SeqCst), 0);
                                        assert!(
                                            peer_spec.upgrade().is_some()
                                                && peer_source.upgrade().is_some()
                                        );
                                        release.send(()).unwrap();
                                        let admission =
                                            tokio::time::timeout(Duration::from_secs(3), future)
                                                .await
                                                .unwrap()
                                                .unwrap();
                                        assert_eq!(admission.open_key, latest);
                                        assert_eq!(calls.load(Ordering::SeqCst), 1);
                                    } else {
                                        if mode == "cancel" {
                                            next_owner.cancel(key).unwrap();
                                        }
                                        release.send(()).unwrap();
                                    }
                                });
                                *peer_out.lock().unwrap() = Some(peer_task);
                                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                                if mode == "panic" {
                                    panic!("intentional transport panic");
                                }
                                if mode == "error" {
                                    Err(OpenError::DevelopmentFailed)
                                } else {
                                    Ok(())
                                }
                            };
                            let offered = if hardware {
                                decision.offer_fit(c, &authority, &gpu, emit)
                            } else {
                                offer_fit_event(c, &authority, &pair.spec, codec_event(key), emit)
                            };
                            if matches!(mode, "ok" | "error") {
                                assert!(matches!(
                                    offer_fit_event(
                                        c,
                                        &authority,
                                        &pair.spec,
                                        codec_event(key),
                                        |_| panic!("second offer transported")
                                    ),
                                    Err(OpenError::Unavailable)
                                ));
                            }
                            offered?;
                            decision.into_candidate(c, &bytes)
                        },
                        finish,
                    ),
                )
                .await
                .unwrap();
                let peer_task = peer.lock().unwrap().take().expect("actual transport peer");
                tokio::time::timeout(Duration::from_secs(3), peer_task)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(emitted.load(Ordering::SeqCst), 1);
                match mode {
                    "ok" => assert!(result.is_ok()),
                    "error" => assert!(matches!(result, Err(OpenError::DevelopmentFailed))),
                    "panic" => assert!(matches!(result, Err(OpenError::WorkerFailed))),
                    _ => assert!(matches!(result, Err(OpenError::Cancelled))),
                }
                drop(result);
                o.close();
                let (spec, source) = witnesses.lock().unwrap().take().unwrap();
                assert!(spec.upgrade().is_none() && source.upgrade().is_none());
                assert!(dropped.load(Ordering::SeqCst) && unlocked.load(Ordering::SeqCst));
                assert!(o.gate_available_for_test());
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn counted_offer_and_error_retirement() {
                for mode in ["ok", "error", "panic", "cancel", "supersede"] {
                    transport_case(None, mode).await;
                }
            }

            // A real offscreen adapter, never AppState/mock/default GPU success.
            // Request the adapter's supported limits directly; no private normal
            // context initializer or copy of its minimum-limit policy is used.
            fn offscreen() -> (GpuContext, Arc<Mutex<Vec<String>>>) {
                let mut descriptor =
                    wgpu::InstanceDescriptor::new_without_display_handle_from_env();
                if cfg!(target_os = "android") && std::env::var_os("WGPU_BACKEND").is_none() {
                    descriptor.backends = wgpu::Backends::VULKAN;
                }
                let instance = wgpu::Instance::new(descriptor);
                let adapter =
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        ..Default::default()
                    }))
                    .expect("actual offscreen adapter required");
                assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
                let adapter_info = adapter.get_info();
                assert!(
                    adapter
                        .get_downlevel_capabilities()
                        .flags
                        .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
                );
                let limits = adapter.limits();
                let features =
                    adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
                let (device, queue) =
                    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                        label: Some("C3c actual offscreen"),
                        required_limits: limits.clone(),
                        required_features: features,
                        ..Default::default()
                    }))
                    .expect("actual device");
                let errors = Arc::new(Mutex::new(Vec::new()));
                let recorded = Arc::clone(&errors);
                device.on_uncaptured_error(Arc::new(move |error| {
                    recorded.lock().unwrap().push(error.to_string())
                }));
                (
                    GpuContext {
                        device: Arc::new(device),
                        queue: Arc::new(queue),
                        limits,
                        adapter_info,
                        display: Arc::new(Mutex::new(None)),
                    },
                    errors,
                )
            }
            fn finish_offscreen(context: &GpuContext, errors: &Mutex<Vec<String>>) {
                context
                    .device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(60)),
                    })
                    .expect("actual readback completion");
                assert!(errors.lock().unwrap().is_empty());
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            #[ignore = "requires actual offscreen GPU; genuine C3c fit/JPEG/event then accurate production"]
            async fn genuine_fit_then_accurate_pipeline() {
                let (hardware, errors) = offscreen();
                for selected_profile in [false, true] {
                    let (r, id, dcp, look) = selected();
                    let bytes = patterned_composition_dng();
                    let path = file(&bytes, "dng");
                    let bitmap = patch_contract_png(DynamicImage::ImageLuma8(
                        image::GrayImage::from_pixel(1, 1, image::Luma([128])),
                    ));
                    let mut input = patch_contract_adjustments(Some(&bitmap), Some((1, 1)), (0, 0));
                    if selected_profile {
                        input["profile"] =
                            json!({"base":id,"look":"composition-look","amount":100});
                    }
                    let original = input.clone();
                    let mut s = settings();
                    s.editor_preview_resolution = Some(64);
                    let o = owner();
                    let key = admit(&o, path, metadata(input), s.clone(), true).await;
                    s.editor_preview_resolution = Some(1920); // Live/caller mutation cannot change captured64.
                    let counters = Arc::new([
                        AtomicUsize::new(0),
                        AtomicUsize::new(0),
                        AtomicUsize::new(0),
                    ]);
                    let observed = Arc::clone(&counters);
                    o.observe_preparation_for_test(Arc::new(move |stage| {
                        let index = match stage {
                            PreparationObservation::Read => 0,
                            PreparationObservation::Factory => 1,
                            PreparationObservation::Render => 2,
                        };
                        observed[index].fetch_add(1, Ordering::SeqCst);
                        if stage == PreparationObservation::Factory {
                            fs::remove_file(&dcp).unwrap();
                            fs::remove_file(&look).unwrap();
                        }
                    }));
                    let stages = Arc::new(Mutex::new(Vec::new()));
                    let seen = Arc::clone(&stages);
                    let weak = Arc::downgrade(&o);
                    o.observe_fit_for_test(Arc::new(move |stage| {
                        assert!(weak.upgrade().unwrap().gate_available_for_test());
                        seen.lock().unwrap().push(stage);
                    }));
                    let slot = Arc::new(Mutex::new(Some(hardware.clone())));
                    let used = Arc::clone(&slot);
                    let authority = Arc::clone(&o);
                    let display_refs = Arc::strong_count(&hardware.display);
                    let device_refs = Arc::strong_count(&hardware.device);
                    let queue_refs = Arc::strong_count(&hardware.queue);
                    let result = tokio::time::timeout(
                        Duration::from_secs(120),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                ACCURATE_FILTER_CALLS.with(|n| n.set(0));
                                let candidate = read_keyed_candidate_with_fit(
                                    c,
                                    &r,
                                    &authority,
                                    &used,
                                    |event| {
                                        assert_eq!(ACCURATE_FILTER_CALLS.with(|n| n.get()), 0);
                                        let wire = serde_json::to_value(event).unwrap();
                                        let width = wire["width"].as_u64().unwrap();
                                        let height = wire["height"].as_u64().unwrap();
                                        assert_eq!(width.max(height), 64);
                                        assert_eq!(
                                            wire["openKey"],
                                            serde_json::to_value(key).unwrap()
                                        );
                                        let jpeg = general_purpose::STANDARD
                                            .decode(wire["dataBase64"].as_str().unwrap())
                                            .unwrap();
                                        assert!(jpeg.len() <= 2 * 1024 * 1024);
                                        let decoded = image::load_from_memory_with_format(
                                            &jpeg,
                                            image::ImageFormat::Jpeg,
                                        )
                                        .unwrap();
                                        assert_eq!(
                                            decoded.dimensions(),
                                            (width as u32, height as u32)
                                        );
                                        assert!(
                                            serde_json::to_vec(event).unwrap().len()
                                                <= 3 * 1024 * 1024
                                        );
                                        assert!(used.try_lock().is_ok());
                                        assert!(authority.gate_available_for_test());
                                        Ok(())
                                    },
                                )?;
                                assert_eq!(
                                    ACCURATE_FILTER_CALLS.with(|n| n.get()),
                                    if selected_profile { 2 } else { 1 }
                                );
                                let spec = c.pixels_spec()?;
                                assert_eq!(spec.metadata().adjustments, original);
                                assert_eq!(spec.settings().editor_preview_resolution, Some(64));
                                assert!(
                                    candidate
                                        .images_for_test()
                                        .0
                                        .width()
                                        .max(candidate.images_for_test().0.height())
                                        >= 128
                                );
                                Ok(candidate)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                    assert_eq!(result.0.render_mode, RenderMode::AccurateXtrans1Encoded);
                    assert_eq!(
                        counters.each_ref().map(|n| n.load(Ordering::SeqCst)),
                        [1, 1, 1]
                    );
                    assert_eq!(
                        *stages.lock().unwrap(),
                        vec![
                            FitObservation::Capture,
                            FitObservation::GpuSnapshot,
                            FitObservation::GpuOwner,
                            FitObservation::Base,
                            FitObservation::Composition,
                            FitObservation::Render,
                            FitObservation::Encoding,
                            FitObservation::Event,
                            FitObservation::Offer,
                            FitObservation::Transport
                        ]
                    );
                    assert_eq!(Arc::strong_count(&hardware.display), display_refs);
                    assert_eq!(Arc::strong_count(&hardware.device), device_refs);
                    assert_eq!(Arc::strong_count(&hardware.queue), queue_refs);
                    assert!(hardware.display.lock().unwrap().is_none());
                    assert_eq!(s.editor_preview_resolution, Some(1920));
                }
                finish_offscreen(&hardware, &errors);
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            #[ignore = "requires actual offscreen GPU; cancel only after actual H5 render return"]
            async fn cancellation_after_actual_h5_return() {
                let (hardware, errors) = offscreen();
                for supersede in [false, true] {
                    let o = owner();
                    let path = file(&patterned_composition_dng(), "dng");
                    let mut s = settings();
                    s.editor_preview_resolution = Some(64);
                    let (key, dropped, unlocked) = admit_witness(&o, path, s).await;
                    let stages = Arc::new(Mutex::new(Vec::new()));
                    let seen = Arc::clone(&stages);
                    let weak = Arc::downgrade(&o);
                    o.observe_fit_for_test(Arc::new(move |stage| {
                        seen.lock().unwrap().push(stage);
                        if stage == FitObservation::Render {
                            let o = weak.upgrade().unwrap();
                            assert!(o.gate_available_for_test());
                            if supersede {
                                o.reserve(B, "latest.dng".into()).unwrap();
                            } else {
                                o.cancel(key).unwrap();
                            }
                        }
                    }));
                    let authority = Arc::clone(&o);
                    let slot = Mutex::new(Some(hardware.clone()));
                    let witness = Arc::new(Mutex::new(None));
                    let seen = Arc::clone(&witness);
                    let result = tokio::time::timeout(
                        Duration::from_secs(120),
                        o.execute(
                            key,
                            JobKind::Pixels,
                            move |c| {
                                let (bytes, decision) = read_keyed_preparation(c, &registry())?;
                                let KeyedPreparation::Prepared(pair) = &decision else {
                                    panic!("native pair")
                                };
                                *seen.lock().unwrap() = Some((
                                    Arc::downgrade(&pair.spec),
                                    Arc::downgrade(&pair.appearance.source),
                                ));
                                decision.offer_fit(c, &authority, &slot, |_| {
                                    panic!("cancelled H5 output emitted")
                                })?;
                                decision.into_candidate(c, &bytes)
                            },
                            finish,
                        ),
                    )
                    .await
                    .unwrap();
                    assert!(matches!(result, Err(OpenError::Cancelled)));
                    let observed = stages.lock().unwrap();
                    assert_eq!(observed.last(), Some(&FitObservation::Render));
                    assert!(!observed.contains(&FitObservation::Encoding));
                    drop(observed);
                    o.close();
                    let (spec, source) = witness.lock().unwrap().take().unwrap();
                    assert!(spec.upgrade().is_none() && source.upgrade().is_none());
                    assert!(dropped.load(Ordering::SeqCst) && unlocked.load(Ordering::SeqCst));
                }
                finish_offscreen(&hardware, &errors);
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            #[ignore = "requires actual offscreen GPU; real encoded event held through transport/error/drop"]
            async fn real_encoded_transport_lifetime() {
                let (hardware, errors) = offscreen();
                for mode in ["ok", "error", "panic", "cancel", "supersede"] {
                    transport_case(Some(hardware.clone()), mode).await;
                }
                finish_offscreen(&hardware, &errors);
            }
        }
        // END C3c fixed eight fit/event groups.
    }
    // END C3b fixed eight preparation groups.
}

/// Inactive admission adapter. Root supplies the managed owner/policy and later
/// registration; existing public load_image remains wholly on its legacy route.
pub(crate) async fn begin_keyed_image_open(
    ticket: String,
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<crate::progressive_open::OpenAdmission, crate::progressive_open::OpenError> {
    use crate::progressive_open::{JobKind, OpenError};
    let owner = &state.progressive_open;
    let key = owner.reserve(&ticket, path)?;
    state
        .load_image_generation
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
        .map_err(|_| OpenError::Exhausted)?;
    owner
        .execute(
            key,
            JobKind::Snapshot,
            move |context| {
                context.check()?;
                {
                    use tauri::Manager;
                    let state = app_handle.state::<AppState>();
                    if !state.progressive_open.owns(context) {
                        return Err(OpenError::Stale);
                    }
                    let retired = crate::gpu_processing::retire_native_presentation(&state);
                    if retired.is_err() {
                        state.progressive_open.close();
                    }
                    context.check()?;
                    retired.map_err(|_| OpenError::SnapshotFailed)?;
                }
                let settings = load_settings(app_handle).unwrap_or_default();
                context.check()?;
                capture_keyed_snapshot(context, settings)
            },
            |context, captured| context.finish_captured_snapshot(captured),
        )
        .await
        .map_err(|error| match error {
            OpenError::WorkerFailed => OpenError::SnapshotFailed,
            other => other,
        })
}

/// One sidecar read for admission and final result. The ordinary sidecar helper
/// retains its established missing/malformed-file and EXIF-healing behavior.
// Existing single-read regression seam; production uses the state-bearing read.
#[cfg(test)]
fn capture_keyed_metadata(path: &str, settings: AppSettings) -> (ImageMetadata, AppSettings) {
    let (metadata, settings, _) = capture_keyed_source_state(path, settings);
    (metadata, settings)
}

/// The same single sidecar read, plus conservative fresh-source evidence: a
/// primary (non-virtual-copy) path whose sidecar and every associated XMP name
/// provably do not exist. Anything present, malformed or uncertain is not fresh.
fn capture_keyed_source_state(
    path: &str,
    settings: AppSettings,
) -> (ImageMetadata, AppSettings, bool) {
    let (source, sidecar) = parse_virtual_path(path);
    let (metadata, state) = crate::exif_processing::load_sidecar_with_state(&sidecar);
    let primary = source.to_str() == Some(path);
    let fresh = primary
        && state == crate::exif_processing::SidecarState::Missing
        && metadata.adjustments.is_null()
        && crate::exif_processing::associated_xmp_provably_absent(&source);
    (metadata, settings, fresh)
}

/// Capture from the one original metadata/settings read. The derived creative
/// JSON never replaces admission metadata, and errors remain owned by that spec.
pub(crate) fn capture_keyed_snapshot(
    context: &crate::progressive_open::WorkerContext,
    settings: AppSettings,
) -> Result<crate::progressive_open::CapturedSnapshot, crate::progressive_open::OpenError> {
    context.check()?;
    let (metadata, settings, fresh) = capture_keyed_source_state(context.path(), settings);
    context.check()?;
    context.capture_snapshot_with_source_state(metadata, settings, fresh)
}

pub(crate) fn cancel_keyed_image_open(
    key: crate::progressive_open::OpenKey,
    state: &AppState,
) -> Result<crate::progressive_open::CancelResult, crate::progressive_open::OpenError> {
    state.progressive_open.cancel(key)
}

/// The worker reads no path cache, source cache or original_image. Native source
/// bytes are owned once, reused for EXIF and the real B2a preparation, then freed.
fn read_keyed_candidate(
    context: &crate::progressive_open::WorkerContext,
    registry: &crate::dcp::registry::ProfileRegistry,
) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
    let (bytes, decision) = read_keyed_preparation(context, registry)?;
    decision.into_candidate(context, &bytes)
}

fn read_keyed_preparation(
    context: &crate::progressive_open::WorkerContext,
    registry: &crate::dcp::registry::ProfileRegistry,
) -> Result<(Vec<u8>, KeyedPreparation), crate::progressive_open::OpenError> {
    use crate::progressive_open::OpenError;
    let spec = context.pixels_spec()?;
    let (path, _) = parse_virtual_path(spec.path());
    let cloud = crate::file_management::is_cloud_placeholder(&path);
    ensure_pixels_spec(context, &spec)?;
    if cloud {
        crate::progressive_open::log_open_failure("source.cloud_placeholder", "not_materialized");
        return Err(OpenError::DevelopmentFailed);
    }
    let bytes = fs::read(&path);
    #[cfg(test)]
    context.observe_preparation(crate::progressive_open::PreparationObservation::Read);
    ensure_pixels_spec(context, &spec)?;
    let bytes = bytes.map_err(|error| {
        let cause = match error.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::InvalidData => "invalid_data",
            _ => "io_error",
        };
        crate::progressive_open::log_open_failure("source.read", cause);
        OpenError::DevelopmentFailed
    })?;
    let decision = prepare_captured_decision(context, &bytes, registry)?;
    Ok((bytes, decision))
}

struct PreparedKeyedOpen {
    spec: Arc<crate::progressive_open::OpenSpec>,
    appearance: PreparedOpenAppearance,
}

enum KeyedPreparation {
    Prepared(PreparedKeyedOpen),
    DecodedLegacy {
        spec: Arc<crate::progressive_open::OpenSpec>,
        appearance: DecodedLegacyAppearance,
    },
    LegacyRequired {
        spec: Arc<crate::progressive_open::OpenSpec>,
        reason: LegacyPreparedReason,
    },
}

fn ensure_pixels_spec(
    context: &crate::progressive_open::WorkerContext,
    expected: &Arc<crate::progressive_open::OpenSpec>,
) -> Result<(), crate::progressive_open::OpenError> {
    let current = context.pixels_spec()?;
    if Arc::ptr_eq(&current, expected) {
        Ok(())
    } else {
        Err(crate::progressive_open::OpenError::Stale)
    }
}

fn prepare_captured_decision(
    context: &crate::progressive_open::WorkerContext,
    bytes: &[u8],
    registry: &crate::dcp::registry::ProfileRegistry,
) -> Result<KeyedPreparation, crate::progressive_open::OpenError> {
    use crate::progressive_open::OpenError;
    let spec = context.pixels_spec()?;
    let (path, _) = parse_virtual_path(spec.path());
    let decision = prepare_open_appearance(
        bytes,
        &path.to_string_lossy(),
        &spec.metadata().adjustments,
        spec.settings(),
        registry,
        spec.policy(),
        context.raw_cancel(),
    );
    #[cfg(test)]
    context.observe_preparation(crate::progressive_open::PreparationObservation::Factory);
    ensure_pixels_spec(context, &spec)?;
    let decision = decision.map_err(|_| {
        crate::progressive_open::log_open_failure("factory.prepare", "opaque_error");
        OpenError::DevelopmentFailed
    })?;
    // Optional fresh-RAW default after the genuine decode, before any development.
    // Its own failures only skip it; currentness/cancellation is rechecked here.
    let decision = with_initial_profile_default(decision, &spec, registry, &context.raw_cancel());
    ensure_pixels_spec(context, &spec)?;
    match decision {
        PreparedAppearanceDecision::Prepared(appearance) => {
            let captured = spec.captured_creative().map(|_| ());
            ensure_pixels_spec(context, &spec)?;
            captured?;
            Ok(KeyedPreparation::Prepared(PreparedKeyedOpen {
                spec,
                appearance,
            }))
        }
        PreparedAppearanceDecision::DecodedLegacy(appearance) => {
            Ok(KeyedPreparation::DecodedLegacy { spec, appearance })
        }
        PreparedAppearanceDecision::LegacyRequired(reason) => {
            Ok(KeyedPreparation::LegacyRequired { spec, reason })
        }
    }
}

// The fresh default selects only a base; Look stays null and Amount uses the
// same fresh value as an explicit base choice in the editor.
const INITIAL_DEFAULT_AMOUNT: u64 = 100;

/// Applies at most one eligible camera-matched Modular base to an unselected
/// prepared X-Trans or decoded Bayer/CFA appearance. The immutable spec is never
/// changed: effective adjustments are derived from its original Null value.
fn with_initial_profile_default(
    decision: PreparedAppearanceDecision,
    spec: &crate::progressive_open::OpenSpec,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: &crate::raw_processing::RawCancel,
) -> PreparedAppearanceDecision {
    if !spec.initial_profile_default_allowed() || !spec.metadata().adjustments.is_null() {
        return decision;
    }
    match decision {
        PreparedAppearanceDecision::Prepared(mut open) if open.assets.is_none() => {
            if let Some((adjustments, assets)) = initial_default_assets(
                &open.source.prepared,
                &open.inputs.adjustments,
                registry,
                cancel,
            ) {
                open.inputs.adjustments = adjustments;
                open.assets = Some(assets);
            }
            PreparedAppearanceDecision::Prepared(open)
        }
        PreparedAppearanceDecision::DecodedLegacy(mut open) if open.assets.is_none() => {
            if let Some((adjustments, assets)) =
                initial_default_assets(&open.prepared, &open.inputs.adjustments, registry, cancel)
            {
                open.inputs.adjustments = adjustments;
                open.assets = Some(assets);
            }
            PreparedAppearanceDecision::DecodedLegacy(open)
        }
        // Explicit selections, nonRAW and Apple-preferred routes are unchanged.
        other => other,
    }
}

/// Decoded camera facts only. Any skip, asset/registry failure or cancellation
/// returns None; the caller's currentness check decides cancellation/staleness.
fn initial_default_assets(
    prepared: &crate::raw_processing::PreparedRaw,
    original: &Value,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: &crate::raw_processing::RawCancel,
) -> Option<(Value, crate::dcp::pipeline::SelectedProfileAssets)> {
    use crate::dcp::defaults::{ModularDecision, RawDefaultFacts, decide_modular_default};
    check_prepared_cancel(cancel).ok()?;
    let facts = match prepared.camera_native_cfa_meta() {
        Some(meta) => RawDefaultFacts::CameraNative(meta),
        None => RawDefaultFacts::Unknown,
    };
    let evidence = match decide_modular_default(facts, registry) {
        ModularDecision::Eligible(evidence) => evidence,
        ModularDecision::Skip(reason) => {
            log::info!("[open-default] outcome=skipped reason={reason:?}");
            return None;
        }
    };
    check_prepared_cancel(cancel).ok()?;
    let mut adjustments = match original {
        Value::Object(map) => Value::Object(map.clone()),
        _ => Value::Object(serde_json::Map::new()),
    };
    adjustments["profile"] = serde_json::json!({
        "base": evidence.base.to_hex(),
        "look": Value::Null,
        "amount": INITIAL_DEFAULT_AMOUNT,
    });
    match crate::dcp::pipeline::prepare_selected_profile(&adjustments, registry, cancel) {
        Ok(Some(assets)) => {
            log::info!("[open-default] outcome=selected");
            Some((adjustments, assets))
        }
        Ok(None) | Err(_) => {
            log::info!("[open-default] outcome=skipped reason=asset_preparation");
            None
        }
    }
}

/// Provenance is derived, never stored separately: an originally Null fresh spec
/// can only hold selected assets through `with_initial_profile_default`.
fn initial_profile_default_provenance(
    spec: &crate::progressive_open::OpenSpec,
    decision: &PreparedAppearanceDecision,
) -> Option<crate::progressive_open::InitialProfileDefault> {
    if !spec.initial_profile_default_allowed() || !spec.metadata().adjustments.is_null() {
        return None;
    }
    let adjustments = match decision {
        PreparedAppearanceDecision::Prepared(open) if open.assets.is_some() => {
            &open.inputs.adjustments
        }
        PreparedAppearanceDecision::DecodedLegacy(open) if open.assets.is_some() => {
            &open.inputs.adjustments
        }
        _ => return None,
    };
    let base = adjustments.get("profile")?.get("base")?.as_str()?;
    Some(crate::progressive_open::InitialProfileDefault {
        base: base.to_owned(),
    })
}

impl KeyedPreparation {
    fn into_candidate(
        self,
        context: &crate::progressive_open::WorkerContext,
        bytes: &[u8],
    ) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
        let (spec, decision) = match self {
            Self::Prepared(pair) => (
                pair.spec,
                PreparedAppearanceDecision::Prepared(pair.appearance),
            ),
            Self::DecodedLegacy { spec, appearance } => {
                (spec, PreparedAppearanceDecision::DecodedLegacy(appearance))
            }
            Self::LegacyRequired { spec, reason } => {
                (spec, PreparedAppearanceDecision::LegacyRequired(reason))
            }
        };
        ensure_pixels_spec(context, &spec)?;
        let initial_default = initial_profile_default_provenance(&spec, &decision);
        let (path, _) = parse_virtual_path(spec.path());
        let result = render_keyed_decision_owned(
            bytes,
            &path.to_string_lossy(),
            &spec,
            decision,
            PreparedPatchPolicy::RequireCaptured,
            context.raw_cancel(),
        );
        #[cfg(test)]
        context.observe_preparation(crate::progressive_open::PreparationObservation::Render);
        ensure_pixels_spec(context, &spec)?;
        let (candidate, appearance) = result?;
        let candidate = candidate.with_initial_profile_default(initial_default);
        match appearance {
            Some(appearance) => {
                crate::progressive_open::OpenCandidate::from_retained(RetainedPreparedCandidate {
                    candidate,
                    retained: Arc::new(RetainedOpenAppearance { spec, appearance }),
                })
            }
            None => Ok(candidate),
        }
    }
}

// BEGIN C3c captured fit side channel. No public loader/command activates it.
#[cfg(test)]
fn read_keyed_candidate_with_fit(
    context: &crate::progressive_open::WorkerContext,
    registry: &crate::dcp::registry::ProfileRegistry,
    owner: &crate::progressive_open::OpenOwner,
    gpu: &std::sync::Mutex<Option<crate::image_processing::GpuContext>>,
    emit: impl FnOnce(
        &crate::progressive_open::PlaceholderEvent,
    ) -> std::result::Result<(), crate::progressive_open::OpenError>,
) -> std::result::Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError>
{
    read_keyed_candidate_with_fit_initialization(context, registry, owner, gpu, || Ok(()), emit)
}

fn read_keyed_candidate_with_fit_initialization(
    context: &crate::progressive_open::WorkerContext,
    registry: &crate::dcp::registry::ProfileRegistry,
    owner: &crate::progressive_open::OpenOwner,
    gpu: &std::sync::Mutex<Option<crate::image_processing::GpuContext>>,
    initialize: impl FnOnce() -> std::result::Result<(), crate::progressive_open::OpenError>,
    emit: impl FnOnce(
        &crate::progressive_open::PlaceholderEvent,
    ) -> std::result::Result<(), crate::progressive_open::OpenError>,
) -> std::result::Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError>
{
    let spec = context.pixels_spec()?;
    check_fit_current(context, owner, &spec)?;
    let (bytes, decision) = read_keyed_preparation(context, registry)?;
    check_fit_current(context, owner, &spec)?;
    if matches!(&decision, KeyedPreparation::Prepared(_)) {
        let captured = snapshot_fit_gpu(gpu);
        check_fit_current(context, owner, &spec)?;
        if captured
            .inspect_err(|error| {
                crate::progressive_open::log_open_error("fit.gpu_snapshot", error);
            })?
            .is_none()
        {
            let initialized = initialize();
            check_fit_current(context, owner, &spec)?;
            initialized.inspect_err(|error| {
                crate::progressive_open::log_open_error("fit.gpu_init", error);
            })?;
        }
    }
    decision.offer_fit(context, owner, gpu, emit)?;
    // Every fit buffer/renderer/offer is gone; this is the original accurate path.
    decision.into_candidate(context, &bytes)
}

impl KeyedPreparation {
    fn offer_fit(
        &self,
        context: &crate::progressive_open::WorkerContext,
        owner: &crate::progressive_open::OpenOwner,
        gpu: &std::sync::Mutex<Option<crate::image_processing::GpuContext>>,
        emit: impl FnOnce(
            &crate::progressive_open::PlaceholderEvent,
        ) -> std::result::Result<(), crate::progressive_open::OpenError>,
    ) -> std::result::Result<(), crate::progressive_open::OpenError> {
        match self {
            Self::Prepared(pair) => pair.offer_fit(context, owner, gpu, emit),
            Self::DecodedLegacy { spec, .. } | Self::LegacyRequired { spec, .. } => {
                // Real legacy classification ignores unused capture/GPU state.
                check_fit_current(context, owner, spec)
            }
        }
    }
}

fn check_fit_current(
    context: &crate::progressive_open::WorkerContext,
    owner: &crate::progressive_open::OpenOwner,
    spec: &Arc<crate::progressive_open::OpenSpec>,
) -> std::result::Result<(), crate::progressive_open::OpenError> {
    let current = context.pixels_spec()?;
    if !owner.owns(context) || !Arc::ptr_eq(&current, spec) {
        return Err(crate::progressive_open::OpenError::Stale);
    }
    Ok(())
}

// Real stage result is retained until the postcheck, including Err. No result is
// supplied by the test-only observer; cancellation is authoritative after return.
fn fit_stage<T>(
    context: &crate::progressive_open::WorkerContext,
    owner: &crate::progressive_open::OpenOwner,
    spec: &Arc<crate::progressive_open::OpenSpec>,
    #[cfg(test)] stage: crate::progressive_open::FitObservation,
    work: impl FnOnce() -> std::result::Result<T, crate::progressive_open::OpenError>,
) -> std::result::Result<T, crate::progressive_open::OpenError> {
    check_fit_current(context, owner, spec)?;
    let result = work();
    #[cfg(test)]
    context.observe_fit(stage);
    check_fit_current(context, owner, spec).inspect_err(|error| {
        crate::progressive_open::log_open_error("fit.postcheck", error);
    })?;
    result
}

fn captured_fit_limit(settings: &AppSettings) -> u32 {
    match settings.editor_preview_resolution {
        None | Some(0) => 1920,
        Some(value) => value.min(1920),
    }
}

fn snapshot_fit_gpu(
    gpu: &std::sync::Mutex<Option<crate::image_processing::GpuContext>>,
) -> std::result::Result<
    Option<crate::image_processing::GpuContext>,
    crate::progressive_open::OpenError,
> {
    let handles = {
        let guard = gpu
            .lock()
            .map_err(|_| crate::progressive_open::OpenError::Poisoned)?;
        guard.as_ref().map(|context| {
            (
                Arc::clone(&context.device),
                Arc::clone(&context.queue),
                context.limits.clone(),
                context.adapter_info.clone(),
            )
        })
    };
    // The app slot is unlocked before either display Arc or H5 is constructed.
    Ok(handles.map(
        |(device, queue, limits, adapter_info)| crate::image_processing::GpuContext {
            device,
            queue,
            limits,
            adapter_info,
            display: Arc::new(std::sync::Mutex::new(None)),
        },
    ))
}

impl PreparedKeyedOpen {
    // Own the genuine base locally: no independent base/assets pairing API.
    // The consumer borrows actual composed output; production uses real H5.
    fn with_current_fit<T>(
        &self,
        context: &crate::progressive_open::WorkerContext,
        owner: &crate::progressive_open::OpenOwner,
        use_fit: impl FnOnce(
            &crate::open_composition::ComposedFit<'_, '_>,
            u32,
        ) -> std::result::Result<T, crate::progressive_open::OpenError>,
    ) -> std::result::Result<T, crate::progressive_open::OpenError> {
        use crate::progressive_open::OpenError;
        let limit = captured_fit_limit(self.spec.settings());
        let base = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::Base,
            || {
                self.appearance
                    .render_unpatched_base(AppearancePass::CurrentFull, context.raw_cancel())
                    .map_err(|_| {
                        crate::progressive_open::log_open_failure("fit.base", "opaque_error");
                        OpenError::DevelopmentFailed
                    })
            },
        )?;
        let composed = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::Composition,
            || {
                let assets = self.spec.captured_creative()?;
                let fit = crate::open_composition::FitPreviewSpec::new(limit)
                    .map_err(|_| OpenError::InvalidPlaceholder)?;
                let composed =
                    crate::open_composition::compose_fit(&base, assets, fit, context.raw_cancel())
                        .inspect_err(|error| {
                            let cause = match error {
                                crate::open_composition::ComposeError::Cancelled => "cancelled",
                                crate::open_composition::ComposeError::InvalidFit => "invalid_fit",
                                crate::open_composition::ComposeError::InvalidGeometry => {
                                    "invalid_geometry"
                                }
                                crate::open_composition::ComposeError::TooManyMasks(_) => {
                                    "too_many_masks"
                                }
                                crate::open_composition::ComposeError::Mask(_) => "mask_error",
                                crate::open_composition::ComposeError::MaskOrderMismatch => {
                                    "mask_order_mismatch"
                                }
                                crate::open_composition::ComposeError::ProfileAlreadyApplied => {
                                    "profile_already_applied"
                                }
                                crate::open_composition::ComposeError::Stage { .. } => {
                                    "composition_stage_error"
                                }
                            };
                            crate::progressive_open::log_open_failure("fit.composition", cause);
                        })
                        .map_err(|error| match error {
                            crate::open_composition::ComposeError::Cancelled => {
                                OpenError::Cancelled
                            }
                            _ => OpenError::DevelopmentFailed,
                        })?;
                let (width, height) = composed.pixels().dimensions();
                if width == 0 || height == 0 || width > limit || height > limit {
                    return Err(OpenError::InvalidPlaceholder);
                }
                Ok(composed)
            },
        )?;
        check_fit_current(context, owner, &self.spec)?;
        let result = use_fit(&composed, limit);
        check_fit_current(context, owner, &self.spec)?;
        // Borrowed composition drops before base. No gate is held here/on Err.
        result
    }

    fn offer_fit(
        &self,
        context: &crate::progressive_open::WorkerContext,
        owner: &crate::progressive_open::OpenOwner,
        gpu: &std::sync::Mutex<Option<crate::image_processing::GpuContext>>,
        emit: impl FnOnce(
            &crate::progressive_open::PlaceholderEvent,
        ) -> std::result::Result<(), crate::progressive_open::OpenError>,
    ) -> std::result::Result<(), crate::progressive_open::OpenError> {
        use crate::progressive_open::OpenError;
        fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::Capture,
            || self.spec.captured_creative().map(|_| ()),
        )?;
        let Some(snapshot) = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::GpuSnapshot,
            || snapshot_fit_gpu(gpu),
        )?
        else {
            return Ok(()); // Only an actually absent optional GPU omits fit.
        };
        let mut renderer = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::GpuOwner,
            || {
                Ok(crate::gpu_processing::IsolatedCreativeRenderer::new(
                    &snapshot,
                ))
            },
        )?;
        drop(snapshot);
        let (pixels, limit) = self.with_current_fit(context, owner, |composed, limit| {
            fit_stage(
                context,
                owner,
                &self.spec,
                #[cfg(test)]
                crate::progressive_open::FitObservation::Render,
                || {
                    let request = composed.request();
                    if request.adjustments.global.has_dcp != 0
                        || request.dcp_textures.is_some()
                        || request.roi.is_some()
                    {
                        crate::progressive_open::log_open_failure(
                            "fit.gpu_request",
                            "unexpected_profile_or_roi",
                        );
                        return Err(OpenError::DevelopmentFailed);
                    }
                    let pixels = renderer
                        .render(
                            composed.pixels(),
                            request,
                            crate::gpu_processing::RenderOutputPrecision::EightBit,
                        )
                        .map_err(|_| {
                            crate::progressive_open::log_open_failure(
                                "fit.gpu_render",
                                "opaque_error",
                            );
                            OpenError::DevelopmentFailed
                        })?;
                    if !matches!(&pixels, DynamicImage::ImageRgba8(_))
                        || pixels.dimensions() != composed.pixels().dimensions()
                    {
                        crate::progressive_open::log_open_failure(
                            "fit.gpu_output",
                            "invalid_placeholder",
                        );
                        return Err(OpenError::InvalidPlaceholder);
                    }
                    Ok((pixels, limit))
                },
            )
        })?;
        drop(renderer); // Real return, not a claim of driver-wide completion on Err.
        let (width, height) = pixels.dimensions();
        let jpeg = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::Encoding,
            || {
                encode_fit_jpeg(&pixels, limit).inspect_err(|error| {
                    crate::progressive_open::log_open_error("fit.jpeg", error);
                })
            },
        )?;
        drop(pixels);
        let event = fit_stage(
            context,
            owner,
            &self.spec,
            #[cfg(test)]
            crate::progressive_open::FitObservation::Event,
            || {
                crate::progressive_open::PlaceholderEvent::from_jpeg(
                    context.key(),
                    width,
                    height,
                    limit,
                    &jpeg,
                )
            },
        )?;
        drop(jpeg);
        offer_fit_event(context, owner, &self.spec, event, emit)
    }
}

struct FitJpegWriter {
    bytes: Vec<u8>,
}
impl std::io::Write for FitJpegWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(buffer.len())
            .ok_or_else(|| std::io::Error::other("Fit JPEG size overflow"))?;
        if next > 2 * 1024 * 1024 {
            return Err(std::io::Error::other("Fit JPEG byte limit"));
        }
        self.bytes
            .try_reserve_exact(buffer.len())
            .map_err(|_| std::io::Error::other("Fit JPEG allocation failed"))?;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encode_fit_jpeg(
    pixels: &DynamicImage,
    limit: u32,
) -> std::result::Result<Vec<u8>, crate::progressive_open::OpenError> {
    use crate::progressive_open::OpenError;
    let (width, height) = pixels.dimensions();
    if limit == 0
        || limit > 1920
        || width == 0
        || height == 0
        || width > limit
        || height > limit
        || !matches!(pixels, DynamicImage::ImageRgba8(_))
    {
        return Err(OpenError::InvalidPlaceholder);
    }
    let mut writer = FitJpegWriter { bytes: Vec::new() };
    pixels
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut writer,
            80,
        ))
        .map_err(|_| OpenError::InvalidPlaceholder)?;
    Ok(writer.bytes)
}

fn offer_fit_event(
    context: &crate::progressive_open::WorkerContext,
    owner: &crate::progressive_open::OpenOwner,
    spec: &Arc<crate::progressive_open::OpenSpec>,
    event: crate::progressive_open::PlaceholderEvent,
    emit: impl FnOnce(
        &crate::progressive_open::PlaceholderEvent,
    ) -> std::result::Result<(), crate::progressive_open::OpenError>,
) -> std::result::Result<(), crate::progressive_open::OpenError> {
    let offer = fit_stage(
        context,
        owner,
        spec,
        #[cfg(test)]
        crate::progressive_open::FitObservation::Offer,
        || {
            owner.offer_placeholder(event).inspect_err(|error| {
                crate::progressive_open::log_open_error("fit.offer", error);
            })
        },
    )?;
    let result = fit_stage(
        context,
        owner,
        spec,
        #[cfg(test)]
        crate::progressive_open::FitObservation::Transport,
        || {
            emit(offer.event()?).inspect_err(|error| {
                crate::progressive_open::log_open_error("fit.event", error);
            })
        },
    );
    drop(offer); // Also drops on panic; no gate is held across emit or retirement.
    result
}
// END C3c captured fit side channel.

#[derive(Clone, Copy)]
enum PreparedPatchPolicy {
    OriginalMetadata,
    RequireCaptured,
}

/// Explicit legacy pixels without load_base_image_from_bytes' EXIF-sidecar
/// persistence/read. Admission metadata must never be reread here. The old public
/// wrapper is unchanged; Apple preference/fallback and pixel helpers are reused.
fn keyed_legacy_pixels(
    bytes: &[u8],
    path: &str,
    reason: LegacyPreparedReason,
    settings: &AppSettings,
    cancel: crate::raw_processing::RawCancel,
) -> Result<DynamicImage> {
    check_prepared_cancel(&cancel)?;
    let preprocessing = RawPreprocessing::from_settings(settings);
    if reason == LegacyPreparedReason::NonRaw {
        let mut image = load_image_with_orientation(bytes, cancel)?;
        preprocessing.apply(&mut image, false, false);
        return Ok(image);
    }
    if let Ok(image) =
        crate::apple_raw::develop_raw9(bytes, path, &crate::apple_raw::Raw9Options::for_loading())
    {
        return Ok(image);
    }
    let developed = panic::catch_unwind(move || {
        crate::raw_processing::develop_raw_image(
            bytes,
            false,
            settings.raw_highlight_compression.unwrap_or(2.5),
            settings.linear_raw_mode.clone(),
            cancel,
        )
    });
    match developed {
        Ok(Ok(mut image)) => {
            preprocessing.apply(&mut image, true, false);
            Ok(image)
        }
        Ok(Err(error)) => {
            let error = classify_raw_develop_error(path, error);
            if error.to_string().contains("Load cancelled") {
                return Err(error);
            }
            if let Some(preview) = safe_embedded_preview_fallback(bytes, path) {
                return Ok(linearize_embedded_preview(preview));
            }
            Err(error)
        }
        Err(_) => {
            if let Some(preview) = safe_embedded_preview_fallback(bytes, path) {
                return Ok(linearize_embedded_preview(preview));
            }
            Err(anyhow!("Failed to process RAW file: {}", path))
        }
    }
}

/// Synchronous production body also used by the generated decoder fixtures.
/// One real prepared factory call; this is not an instrumented decoder counter.
fn prepare_keyed_candidate(
    bytes: &[u8],
    source_path: &str,
    spec: Arc<crate::progressive_open::OpenSpec>,
    registry: &crate::dcp::registry::ProfileRegistry,
    cancel: crate::raw_processing::RawCancel,
) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
    use crate::progressive_open::OpenError;
    let failed = |_| OpenError::DevelopmentFailed;
    check_prepared_cancel(&cancel).map_err(failed)?;
    let decision = prepare_open_appearance(
        bytes,
        source_path,
        &spec.metadata().adjustments,
        spec.settings(),
        registry,
        spec.policy(),
        cancel.clone(),
    )
    .map_err(failed)?;
    render_keyed_decision(
        bytes,
        source_path,
        &spec,
        decision,
        PreparedPatchPolicy::OriginalMetadata,
        cancel,
    )
}

fn render_keyed_decision(
    bytes: &[u8],
    source_path: &str,
    spec: &Arc<crate::progressive_open::OpenSpec>,
    decision: PreparedAppearanceDecision,
    patch_policy: PreparedPatchPolicy,
    cancel: crate::raw_processing::RawCancel,
) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
    let (candidate, owner) =
        render_keyed_decision_owned(bytes, source_path, spec, decision, patch_policy, cancel)?;
    drop(owner);
    Ok(candidate)
}

fn render_keyed_decision_owned(
    bytes: &[u8],
    source_path: &str,
    spec: &Arc<crate::progressive_open::OpenSpec>,
    decision: PreparedAppearanceDecision,
    patch_policy: PreparedPatchPolicy,
    cancel: crate::raw_processing::RawCancel,
) -> Result<
    (
        crate::progressive_open::OpenCandidate,
        Option<PreparedOpenAppearance>,
    ),
    crate::progressive_open::OpenError,
> {
    use crate::progressive_open::{OpenCandidate, OpenError, RenderMode};
    let failed = |_| OpenError::DevelopmentFailed;
    check_prepared_cancel(&cancel).map_err(failed)?;
    let exif = crate::exif_processing::read_exif_data_from_bytes(source_path, bytes);
    check_prepared_cancel(&cancel).map_err(failed)?;
    let mut retained_appearance = None;
    let (pristine, appearance, meta, retained, as_shot, mode) = match decision {
        PreparedAppearanceDecision::Prepared(open) => {
            let retained = open.retained_source();
            let (pristine, meta) = retained
                .render_pristine(AppearancePass::AccurateOne, cancel.clone())
                .inspect_err(|_| {
                    crate::progressive_open::log_open_failure("accurate.pristine", "opaque_error")
                })
                .map_err(failed)?;
            check_prepared_cancel(&cancel).map_err(failed)?;
            let appearance = match patch_policy {
                PreparedPatchPolicy::OriginalMetadata => {
                    if open.assets.is_some() {
                        open.render_appearance(AppearancePass::AccurateOne, cancel.clone())
                            .inspect_err(|_| {
                                crate::progressive_open::log_open_failure(
                                    "accurate.selected_appearance",
                                    "opaque_error",
                                )
                            })
                            .map_err(failed)?
                    } else {
                        composite_patches_on_image(&pristine, &spec.metadata().adjustments)
                            .inspect_err(|_| {
                                crate::progressive_open::log_open_failure(
                                    "accurate.patch_composition",
                                    "opaque_error",
                                )
                            })
                            .map_err(failed)?
                    }
                }
                PreparedPatchPolicy::RequireCaptured => {
                    let adjustments = spec.captured_creative()?.adjustments();
                    if open.assets.is_some() {
                        let base = open
                            .render_unpatched_base(AppearancePass::AccurateOne, cancel.clone())
                            .inspect_err(|_| {
                                crate::progressive_open::log_open_failure(
                                    "accurate.selected_base",
                                    "opaque_error",
                                )
                            })
                            .map_err(failed)?;
                        composite_prepared_base(base.pixels(), adjustments, &cancel)
                            .inspect_err(|_| {
                                crate::progressive_open::log_open_failure(
                                    "accurate.captured_patches",
                                    "opaque_error",
                                )
                            })
                            .map_err(failed)?
                    } else {
                        composite_prepared_base(&pristine, adjustments, &cancel)
                            .inspect_err(|_| {
                                crate::progressive_open::log_open_failure(
                                    "accurate.captured_pristine_patches",
                                    "opaque_error",
                                )
                            })
                            .map_err(failed)?
                    }
                }
            };
            let as_shot = open.as_shot();
            retained_appearance = Some(open);
            (
                pristine,
                appearance,
                Some(meta),
                Some(retained),
                as_shot,
                RenderMode::AccurateXtrans1Encoded,
            )
        }
        PreparedAppearanceDecision::DecodedLegacy(open) => {
            // Keep the successful noneligible decode; never discard/classify and
            // then reopen. Pristine and selected appearance borrow the same owner.
            let as_shot = open.as_shot();
            let (image, orientation, meta) = crate::raw_processing::develop_prepared_raw(
                &open.prepared,
                crate::raw_processing::PreparedQuality::CurrentFull,
                spec.settings().raw_highlight_compression.unwrap_or(2.5),
                &spec.settings().linear_raw_mode,
                false,
                cancel.clone(),
            )
            .map_err(failed)?;
            let mut pristine = apply_orientation(image, orientation);
            RawPreprocessing::from_settings(spec.settings()).apply(&mut pristine, true, false);
            check_prepared_cancel(&cancel).map_err(failed)?;
            let appearance = if open.assets.is_some() {
                open.render_current(cancel.clone())
                    .inspect_err(|_| {
                        crate::progressive_open::log_open_failure(
                            "accurate.legacy_selected",
                            "opaque_error",
                        )
                    })
                    .map_err(failed)?
            } else {
                composite_patches_on_image(&pristine, &spec.metadata().adjustments)
                    .map_err(failed)?
            };
            (
                pristine,
                appearance,
                Some(meta),
                None,
                as_shot,
                RenderMode::Legacy,
            )
        }
        PreparedAppearanceDecision::LegacyRequired(reason) => {
            // Explicit nonRAW/Apple-preferred routing only. Its historical Apple
            // fallback remains legacy; an eligible prepared failure never enters here.
            let pristine =
                keyed_legacy_pixels(bytes, source_path, reason, spec.settings(), cancel.clone())
                    .map_err(failed)?;
            check_prepared_cancel(&cancel).map_err(failed)?;
            let appearance = composite_patches_on_image(&pristine, &spec.metadata().adjustments)
                .map_err(failed)?;
            let as_shot = match reason {
                LegacyPreparedReason::NonRaw => crate::as_shot::AsShot::reference(),
                LegacyPreparedReason::AppleRawPreferred => {
                    crate::raw_processing::read_as_shot(bytes)
                        .unwrap_or_else(crate::as_shot::AsShot::reference)
                }
            };
            (
                pristine,
                appearance,
                None,
                None,
                as_shot,
                RenderMode::Legacy,
            )
        }
    };
    check_prepared_cancel(&cancel).map_err(failed)?;
    let loaded = LoadedImage {
        path: spec.path().to_owned(),
        image: Arc::new(pristine),
        is_raw: is_raw_file(source_path),
        raw_meta: meta,
        raw_bytes: None,
        prepared_source: retained,
        open_identity: (mode == RenderMode::AccurateXtrans1Encoded).then_some(spec.identity()),
        as_shot,
        raw_linear_mode: spec.settings().linear_raw_mode.clone(),
        raw_highlight_compression: spec.settings().raw_highlight_compression.unwrap_or(2.5),
    };
    let candidate = OpenCandidate::new(Arc::clone(spec), loaded, Arc::new(appearance), exif, mode)?;
    Ok((candidate, retained_appearance))
}

pub(crate) async fn load_keyed_image(
    path: String,
    open_key: crate::progressive_open::OpenKey,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<crate::progressive_open::KeyedLoadResult, crate::progressive_open::OpenError> {
    use crate::progressive_open::{JobKind, OpenError};
    use tauri::{Emitter, Manager};
    state.progressive_open.check_path(open_key, &path)?;
    let registry = Arc::clone(&state.profile_registry);
    let worker_app = app_handle.clone();
    state
        .progressive_open
        .execute(
            open_key,
            JobKind::Pixels,
            move |context| {
                if context.path() != path {
                    return Err(OpenError::Stale);
                }
                let worker_state = worker_app.state::<AppState>();
                read_keyed_candidate_with_fit_initialization(
                    context,
                    &registry,
                    &worker_state.progressive_open,
                    &worker_state.gpu_context,
                    || {
                        crate::gpu_processing::get_or_init_gpu_context(&worker_state, &worker_app)
                            .map(|_| ())
                            .map_err(|_| OpenError::DevelopmentFailed)
                    },
                    |event| {
                        worker_app
                            .emit("xtrans-placeholder-ready", event)
                            .map_err(|_| OpenError::DevelopmentFailed)
                    },
                )
            },
            move |context, candidate| {
                let state = app_handle.state::<AppState>();
                if !state.progressive_open.owns(context) {
                    return Err(OpenError::Stale);
                }
                let (result, retired) =
                    context.commit(candidate, |image| state.install_keyed_source(image))?;
                retired.retire(); // All open/original/cache guards are already released.
                Ok(result)
            },
        )
        .await
}

#[cfg(test)]
pub(crate) fn progressive_test_candidate(
    context: &crate::progressive_open::WorkerContext,
    xtrans: bool,
) -> Result<crate::progressive_open::OpenCandidate, crate::progressive_open::OpenError> {
    prepared_appearance_tests::progressive_candidate(context, xtrans)
}

// B2d: the only constructor is the genuine paired render above. No Inner/task
// edge is retained, and no independently supplied appearance can attest a source.
pub(crate) struct RetainedOpenAppearance {
    spec: Arc<crate::progressive_open::OpenSpec>,
    appearance: PreparedOpenAppearance,
}
impl RetainedOpenAppearance {
    pub(crate) fn spec(&self) -> &Arc<crate::progressive_open::OpenSpec> {
        &self.spec
    }
    pub(crate) fn source(&self) -> &Arc<RetainedPreparedSource> {
        &self.appearance.source
    }
}
pub(crate) struct RetainedPreparedCandidate {
    candidate: crate::progressive_open::OpenCandidate,
    retained: Arc<RetainedOpenAppearance>,
}
impl RetainedPreparedCandidate {
    pub(crate) fn into_parts(
        self,
    ) -> (
        crate::progressive_open::OpenCandidate,
        Arc<RetainedOpenAppearance>,
    ) {
        (self.candidate, self.retained)
    }
}

#[derive(Debug)]
pub(crate) enum RequestCaptureError {
    Open(crate::progressive_open::OpenError),
    SourceSettingsChanged,
    InvalidAdjustments,
    Profile,
    Creative,
}
impl std::fmt::Display for RequestCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(error) => write!(f, "{error}"),
            Self::SourceSettingsChanged => f.write_str("SourceSettingsChanged: reopen image"),
            Self::InvalidAdjustments => f.write_str("InvalidAdjustments"),
            Self::Profile => f.write_str("Profile"),
            Self::Creative => f.write_str("Creative"),
        }
    }
}
impl std::error::Error for RequestCaptureError {}

struct OwnedRequestAppearance {
    appearance: PreparedOpenAppearance,
    creative: crate::open_creative_snapshot::CapturedCreativeAssets,
}
enum RequestView {
    Initial(Arc<RetainedOpenAppearance>),
    Edited(Box<OwnedRequestAppearance>),
}
/// Jobs/callbacks hold this whole owner. It is never stored strongly in AppState
/// or in the source owner, and never substitutes a detached image for its lease.
pub(crate) struct ConsumerRequest {
    source: crate::app_state::ConsumerSource,
    denoise_lease: std::sync::Mutex<Option<crate::ai_denoise::DenoiseLease>>,
    view: Option<RequestView>,
    adjustments: Value,
    settings: AppSettings,
    cancelled: Arc<AtomicUsize>,
}
impl ConsumerRequest {
    pub(crate) fn capture(
        source: crate::app_state::ConsumerSource,
        adjustments: Option<Value>,
        settings: AppSettings,
        registry: &crate::dcp::registry::ProfileRegistry,
    ) -> std::result::Result<Arc<Self>, RequestCaptureError> {
        use RequestCaptureError as E;
        source.check().map_err(E::Open)?;
        let cancelled = Arc::new(AtomicUsize::new(0));
        let cancel = Some((Arc::clone(&cancelled), 0));
        let (view, adjustments, settings) = match &source {
            crate::app_state::ConsumerSource::Accurate(snapshot) => {
                let retained = Arc::clone(snapshot.retained().map_err(E::Open)?);
                match adjustments {
                    None => (
                        Some(RequestView::Initial(Arc::clone(&retained))),
                        // Effective values: identical to the original spec unless
                        // this open applied its unsaved fresh camera default.
                        retained.appearance.adjustments().clone(),
                        retained.spec.settings().clone(),
                    ),
                    Some(adjustments) => {
                        if !adjustments.is_object() {
                            source.check().map_err(E::Open)?;
                            return Err(E::InvalidAdjustments);
                        }
                        let original = retained.spec.settings();
                        let before = RawPreprocessing::from_settings(original);
                        let after = RawPreprocessing::from_settings(&settings);
                        if original.linear_raw_mode != settings.linear_raw_mode
                            || original.raw_highlight_compression.unwrap_or(2.5).to_bits()
                                != settings.raw_highlight_compression.unwrap_or(2.5).to_bits()
                            || before.color_nr_amount.to_bits() != after.color_nr_amount.to_bits()
                            || before.sharpening_amount.to_bits()
                                != after.sharpening_amount.to_bits()
                            || original.use_apple_raw9.unwrap_or(false)
                                != settings.use_apple_raw9.unwrap_or(false)
                        {
                            source.check().map_err(E::Open)?;
                            return Err(E::SourceSettingsChanged);
                        }
                        let assets = crate::dcp::pipeline::prepare_selected_profile(
                            &adjustments,
                            registry,
                            &cancel,
                        );
                        source.check().map_err(E::Open)?;
                        let assets = assets.map_err(|_| E::Profile)?;
                        let creative = crate::open_creative_snapshot::capture_creative_assets(
                            &adjustments,
                            &settings,
                            cancel,
                        );
                        source.check().map_err(E::Open)?;
                        let creative = creative.map_err(|_| E::Creative)?;
                        let appearance = PreparedOpenAppearance {
                            source: Arc::clone(&retained.appearance.source),
                            inputs: AppearanceInputs::capture(&adjustments, &settings),
                            assets,
                        };
                        (
                            Some(RequestView::Edited(Box::new(OwnedRequestAppearance {
                                appearance,
                                creative,
                            }))),
                            adjustments,
                            settings,
                        )
                    }
                }
            }
            crate::app_state::ConsumerSource::Legacy { .. } => (
                None,
                adjustments.unwrap_or_else(|| serde_json::json!({})),
                settings,
            ),
        };
        source.check().map_err(E::Open)?;
        Ok(Arc::new(Self {
            source,
            denoise_lease: std::sync::Mutex::new(None),
            view,
            adjustments,
            settings,
            cancelled,
        }))
    }
    pub(crate) fn check(&self) -> std::result::Result<(), String> {
        if self.cancelled.load(Ordering::SeqCst) != 0 {
            return Err("Request cancelled".into());
        }
        self.source.check().map_err(|e| e.to_string())
    }
    pub(crate) fn cancel(&self) {
        self.cancelled.store(1, Ordering::SeqCst);
    }
    pub(crate) fn raw_cancel(&self) -> crate::raw_processing::RawCancel {
        Some((Arc::clone(&self.cancelled), 0))
    }
    pub(crate) fn key(&self) -> Option<crate::progressive_open::OpenKey> {
        self.source.key()
    }
    pub(crate) fn is_keyed(&self) -> bool {
        self.key().is_some()
    }
    pub(crate) fn loaded(&self) -> &LoadedImage {
        self.source.loaded()
    }
    pub(crate) fn adjustments(&self) -> &Value {
        &self.adjustments
    }
    pub(crate) fn settings(&self) -> &AppSettings {
        &self.settings
    }
    pub(crate) fn stage<T>(
        &self,
        work: impl FnOnce() -> std::result::Result<T, String>,
    ) -> std::result::Result<T, String> {
        self.check()?;
        let result = work();
        self.check()?;
        result
    }
    fn appearance(&self) -> std::result::Result<&PreparedOpenAppearance, String> {
        match self.view.as_ref() {
            Some(RequestView::Initial(retained)) => Ok(&retained.appearance),
            Some(RequestView::Edited(owned)) => Ok(&owned.appearance),
            None => Err("Accurate request required".into()),
        }
    }
    pub(crate) fn creative(
        &self,
    ) -> std::result::Result<&crate::open_creative_snapshot::CapturedCreativeAssets, String> {
        self.check()?;
        match self.view.as_ref() {
            Some(RequestView::Initial(retained)) => {
                retained.spec.captured_creative().map_err(|e| e.to_string())
            }
            Some(RequestView::Edited(owned)) => Ok(&owned.creative),
            None => Err("Accurate request required".into()),
        }
    }
    // Normal consumers borrow the genuinely committed pristine AccurateOne
    // base. Selected requests may also borrow the genuinely committed selected
    // appearance, but only when it is proven to be the same unpatched base.
    fn request_base_without_denoise(
        &self,
    ) -> std::result::Result<PreparedAppearanceBase<'_>, String> {
        self.check()?;
        let appearance = self.appearance()?;
        if appearance.assets.is_none() {
            let loaded = self.loaded();
            if !loaded
                .prepared_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &appearance.source))
            {
                return Err("Retained source mismatch".into());
            }
            Ok(PreparedAppearanceBase {
                image: std::borrow::Cow::Borrowed(loaded.image.as_ref()),
                source: Arc::clone(&appearance.source),
                as_shot: loaded.as_shot,
                kind: PreparedBaseKind::NativeXtransWorking(AppearancePass::AccurateOne),
                _appearance: appearance,
            })
        } else {
            if let crate::app_state::ConsumerSource::Accurate(snapshot) = &self.source {
                let retained = snapshot.retained().map_err(|e| e.to_string())?;
                let initial = &retained.appearance;
                let initial_creative = retained
                    .spec
                    .captured_creative()
                    .map_err(|e| e.to_string())?;
                // Exactly the compositor's candidate predicate, applied to the
                // hydrated initial view rather than raw/path-based patch IDs.
                let has_initial_patch = initial_creative
                    .adjustments()
                    .get("aiPatches")
                    .and_then(Value::as_array)
                    .is_some_and(|patches| {
                        patches.iter().any(|patch| {
                            patch
                                .get("visible")
                                .and_then(Value::as_bool)
                                .unwrap_or(true)
                                && patch
                                    .get("patchData")
                                    .and_then(|data| data.get("color"))
                                    .and_then(Value::as_str)
                                    .is_some_and(|color| !color.is_empty())
                        })
                    });
                let same_assets = initial
                    .assets
                    .as_ref()
                    .zip(appearance.assets.as_ref())
                    .is_some_and(|(before, current)| {
                        before.content_identity() == current.content_identity()
                    });
                if Arc::ptr_eq(&initial.source, &appearance.source)
                    && snapshot
                        .loaded()
                        .prepared_source
                        .as_ref()
                        .is_some_and(|source| Arc::ptr_eq(source, &appearance.source))
                    && same_assets
                    && !has_initial_patch
                    && !crate::ai_denoise::is_requested(initial.adjustments())
                    && !crate::ai_denoise::is_requested(initial_creative.adjustments())
                    && !crate::ai_denoise::is_requested(&self.adjustments)
                {
                    self.check()?;
                    return Ok(PreparedAppearanceBase {
                        image: std::borrow::Cow::Borrowed(snapshot.initial_appearance()),
                        source: Arc::clone(&appearance.source),
                        as_shot: appearance.as_shot(),
                        kind: PreparedBaseKind::NativeXtransWorking(AppearancePass::AccurateOne),
                        _appearance: appearance,
                    });
                }
            }
            appearance
                .render_unpatched_base(AppearancePass::AccurateOne, self.raw_cancel())
                .map_err(|e| e.to_string())
        }
    }
    fn request_base(&self) -> std::result::Result<PreparedAppearanceBase<'_>, String> {
        let mut base = self.request_base_without_denoise()?;
        let lease = self.denoise_lease()?;
        let denoised = crate::ai_denoise::apply_ai_denoise_with_lease(
            base.image.as_ref(),
            &self.loaded().path,
            &self.adjustments,
            lease.as_ref(),
        )?;
        if let Cow::Owned(image) = denoised {
            base.image = Cow::Owned(image);
        }
        self.check()?;
        Ok(base)
    }

    pub(crate) fn retain_denoise_lease(
        &self,
        lease: crate::ai_denoise::DenoiseLease,
    ) -> std::result::Result<(), String> {
        self.check()?;
        lease.check_runtime(&self.denoise_request_key()?)?;
        let retired = self
            .denoise_lease
            .lock()
            .map_err(|_| "AI result owner poisoned")?
            .replace(lease);
        drop(retired);
        self.check()
    }
    pub(crate) fn denoise_lease(
        &self,
    ) -> std::result::Result<Option<crate::ai_denoise::DenoiseLease>, String> {
        self.check()?;
        let held = self
            .denoise_lease
            .lock()
            .map_err(|_| "AI result owner poisoned")?
            .clone();
        if let Some(held) = &held {
            held.check_epoch()?;
        }
        Ok(held)
    }

    pub(crate) fn denoise_request_key(&self) -> std::result::Result<String, String> {
        self.check()?;
        let mut h = blake3::Hasher::new();
        h.update(self.loaded().path.as_bytes());
        h.update(&(Arc::as_ptr(&self.loaded().image) as usize).to_le_bytes());
        match &self.source {
            crate::app_state::ConsumerSource::Accurate(snapshot) => {
                h.update(
                    serde_json::to_string(&snapshot.identity().open_key())
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                );
            }
            crate::app_state::ConsumerSource::Legacy { generation, .. } => {
                h.update(&generation.1.to_le_bytes());
            }
        }
        let mut profile = self.adjustments["profile"].clone();
        profile.sort_all_objects();
        h.update(profile.to_string().as_bytes());
        if let Ok(appearance) = self.appearance() {
            h.update(
                format!(
                    "{:?}",
                    appearance.assets.as_ref().map(|a| a.content_identity())
                )
                .as_bytes(),
            );
        }
        h.update(crate::ai_processing::DENOISE_SHA256.as_bytes());
        Ok(h.finalize().to_hex().to_string())
    }

    /// The same held normal/selected source, before patches and geometry. This
    /// is called on a separately captured inference request, never registered as
    /// the replaceable preview request. No detached JPEG/pristine substitution.
    pub(crate) fn denoise_input(
        &self,
        state: &tauri::State<crate::app_state::AppState>,
    ) -> std::result::Result<Arc<DynamicImage>, String> {
        self.stage(|| {
            if self.is_keyed() {
                let base = self.request_base_without_denoise()?;
                Ok(match base.image {
                    Cow::Borrowed(_) => Arc::clone(&self.loaded().image),
                    Cow::Owned(image) => Arc::new(image),
                })
            } else {
                crate::profile_base_image(state, self.loaded(), &self.adjustments)
            }
        })
    }
    pub(crate) fn ensure_denoise_in_worker(
        &self,
        app: &tauri::AppHandle,
    ) -> std::result::Result<(), String> {
        self.check()?;
        if !crate::ai_denoise::is_requested(&self.adjustments) {
            return Ok(());
        }
        let state = app.state::<AppState>();
        let base = self.denoise_input(&state)?;
        let lease = crate::ai_denoise::ensure_worker_base(
            app,
            &base,
            &self.loaded().path,
            &self.adjustments,
            &|| self.check(),
        )?;
        if let Some(lease) = lease {
            self.retain_denoise_lease(lease)?;
        }
        self.check()
    }
    pub(crate) fn unpatched(&self) -> std::result::Result<DynamicImage, String> {
        self.stage(|| self.request_base().map(|base| base.image.into_owned()))
    }
    pub(crate) fn patched(&self) -> std::result::Result<DynamicImage, String> {
        self.stage(|| {
            let base = self.unpatched()?;
            composite_patches_on_image(&base, self.creative()?.adjustments())
                .map_err(|e| e.to_string())
        })
    }
    pub(crate) fn full_reference(&self) -> std::result::Result<Arc<DynamicImage>, String> {
        self.stage(|| {
            let base = self.unpatched()?;
            Ok(Arc::new(
                crate::mask_generation::build_full_warped_image(
                    &base,
                    true,
                    self.creative()?.adjustments(),
                )
                .into_owned(),
            ))
        })
    }
    /// Explicit standalone mask, captured in the same full request/domain. No
    /// fit reference and no .ok() fallback on missing/invalid source or payload.
    pub(crate) fn mask_bitmap(
        &self,
        mask: &crate::mask_generation::MaskDefinition,
        width: u32,
        height: u32,
        scale: f32,
        offset: (f32, f32),
    ) -> std::result::Result<Option<image::GrayImage>, String> {
        self.stage(|| {
            let reference = if mask.requires_warped_image() {
                Some(self.full_reference()?)
            } else {
                None
            };
            crate::mask_generation::generate_mask_bitmap_checked(
                mask,
                width,
                height,
                scale,
                offset,
                reference.as_deref(),
            )
            .map_err(|e| e.to_string())
        })
    }
    /// Same source/assets/CPU composition and shader as full accurate rendering.
    /// The sampler records only the pre-Point Colour float stage, not final JPEG.
    pub(crate) fn sample_point_colour(
        &self,
        context: &crate::image_processing::GpuContext,
        output_uv: [f32; 2],
    ) -> std::result::Result<crate::image_processing::PointColourSample, String> {
        self.with_full_composition(|composed| {
            let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
            renderer.sample_point_colour(composed.pixels(), composed.request(), output_uv)
        })
    }

    pub(crate) fn render(
        &self,
        context: &crate::image_processing::GpuContext,
        resolution: Option<u32>,
        roi: Option<(f32, f32, f32, f32)>,
        precision: crate::gpu_processing::RenderOutputPrecision,
    ) -> std::result::Result<(DynamicImage, (u32, u32), (u32, u32)), String> {
        self.render_for_preview(context, resolution, roi, precision, None)
    }
    pub(crate) fn render_for_preview(
        &self,
        context: &crate::image_processing::GpuContext,
        resolution: Option<u32>,
        roi: Option<(f32, f32, f32, f32)>,
        precision: crate::gpu_processing::RenderOutputPrecision,
        interactive_divisor: Option<f32>,
    ) -> std::result::Result<(DynamicImage, (u32, u32), (u32, u32)), String> {
        let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
        self.render_for_preview_with_renderer(
            context,
            &mut renderer,
            resolution,
            roi,
            precision,
            interactive_divisor,
        )
    }

    fn preview_timed<T>(
        &self,
        name: &str,
        work: impl FnOnce() -> std::result::Result<T, String>,
    ) -> std::result::Result<T, String> {
        let start = std::time::Instant::now();
        let result = work();
        let outcome = if self.check().is_err() {
            "cancelled_or_stale"
        } else if result.is_ok() {
            "completed"
        } else {
            "error"
        };
        log::info!(
            "[preview-stage] stage={} elapsed={:?} outcome={}",
            name,
            start.elapsed(),
            outcome
        );
        result
    }

    fn render_for_preview_with_renderer(
        &self,
        context: &crate::image_processing::GpuContext,
        renderer: &mut crate::gpu_processing::IsolatedCreativeRenderer,
        resolution: Option<u32>,
        roi: Option<(f32, f32, f32, f32)>,
        precision: crate::gpu_processing::RenderOutputPrecision,
        interactive_divisor: Option<f32>,
    ) -> std::result::Result<(DynamicImage, (u32, u32), (u32, u32)), String> {
        self.stage(|| {
            if !renderer.matches_context(context) {
                return Err("Preview renderer context changed".into());
            }
            let base = self.preview_timed("base_development", || self.request_base())?;
            self.check()?;
            let assets = self.creative()?;
            let composed = self.preview_timed("cpu_composition", || {
                match resolution {
                    Some(edge) => crate::open_composition::compose_accurate_preview(
                        &base,
                        assets,
                        edge,
                        interactive_divisor,
                        self.raw_cancel(),
                    ),
                    None => crate::open_composition::compose_full_accuracy(
                        &base,
                        assets,
                        self.raw_cancel(),
                    ),
                }
                .map_err(|e| e.to_string())
            })?;
            self.check()?;
            let (width, height) = composed.pixels().dimensions();
            let mut request = composed.request();
            request.roi = roi.map(|(x, y, w, h)| crate::gpu_processing::Roi {
                x: (x * width as f32).round() as u32,
                y: (y * height as f32).round() as u32,
                width: (w * width as f32).round() as u32,
                height: (h * height as f32).round() as u32,
            });
            let origin = request.roi.map_or((0, 0), |r| (r.x, r.y));
            let result = self.preview_timed("gpu_render_readback", || {
                renderer.render(composed.pixels(), request, precision)
            });
            self.check()?;
            Ok((result?, (width, height), origin))
        })
    }
}

impl ConsumerRequest {
    pub(crate) fn render_uncropped(
        &self,
        context: &crate::image_processing::GpuContext,
    ) -> std::result::Result<DynamicImage, String> {
        self.stage(|| {
            let base = self.request_base()?;
            self.check()?;
            let composed = crate::open_composition::compose_uncropped_accuracy(
                &base,
                self.creative()?,
                self.raw_cancel(),
            )
            .map_err(|e| e.to_string())?;
            self.check()?;
            let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
            let result = renderer.render(
                composed.pixels(),
                composed.request(),
                crate::gpu_processing::RenderOutputPrecision::EightBit,
            );
            self.check()?;
            result
        })
    }
}

/// One genuinely produced unpatched AccurateOne base for an independent file
/// export, retaining its actual prepared source and selected assets. No parts
/// constructor and no editor source/key are manufactured for a file operation.
pub(crate) struct PreparedStandaloneAppearance {
    appearance: PreparedOpenAppearance,
    pixels: DynamicImage,
}
impl PreparedOpenAppearance {
    pub(crate) fn prepare_independent_export(
        self,
        cancel: crate::raw_processing::RawCancel,
    ) -> Result<PreparedStandaloneAppearance> {
        let pixels = {
            let base = self.render_unpatched_base(AppearancePass::AccurateOne, cancel)?;
            base.image.into_owned()
        };
        Ok(PreparedStandaloneAppearance {
            appearance: self,
            pixels,
        })
    }
}
impl PreparedStandaloneAppearance {
    pub(crate) fn apply_denoise_for_export(
        &mut self,
        app: &tauri::AppHandle,
        path: &str,
        check: &(dyn Fn() -> std::result::Result<(), String> + Sync),
    ) -> Result<()> {
        let lease = crate::ai_denoise::ensure_worker_base(
            app,
            &self.pixels,
            path,
            self.appearance.adjustments(),
            check,
        )
        .map_err(anyhow::Error::msg)?;
        if let Cow::Owned(image) = crate::ai_denoise::apply_ai_denoise_with_lease(
            &self.pixels,
            path,
            self.appearance.adjustments(),
            lease.as_ref(),
        )
        .map_err(anyhow::Error::msg)?
        {
            self.pixels = image;
        }
        check().map_err(anyhow::Error::msg)?;
        Ok(())
    }
    pub(crate) fn base(&self) -> PreparedAppearanceBase<'_> {
        PreparedAppearanceBase {
            image: std::borrow::Cow::Borrowed(&self.pixels),
            source: Arc::clone(&self.appearance.source),
            as_shot: self.appearance.as_shot(),
            kind: PreparedBaseKind::NativeXtransWorking(AppearancePass::AccurateOne),
            _appearance: &self.appearance,
        }
    }
    pub(crate) fn adjustments(&self) -> &Value {
        self.appearance.adjustments()
    }
    pub(crate) fn settings(&self) -> &AppSettings {
        self.appearance.settings()
    }
    pub(crate) fn as_shot(&self) -> crate::as_shot::AsShot {
        self.appearance.as_shot()
    }
}
impl ConsumerRequest {
    pub(crate) fn with_full_composition<T>(
        &self,
        work: impl FnOnce(
            &crate::open_composition::ComposedFit<'_, '_>,
        ) -> std::result::Result<T, String>,
    ) -> std::result::Result<T, String> {
        self.stage(|| {
            let base = self.request_base()?;
            self.check()?;
            let composed = crate::open_composition::compose_full_accuracy(
                &base,
                self.creative()?,
                self.raw_cancel(),
            )
            .map_err(|e| e.to_string())?;
            self.check()?;
            let result = work(&composed);
            self.check()?;
            result
        })
    }
}

// Only one pure fitted CPU entry per serial worker; descriptor bookkeeping is
// separate from this image+mask buffer-capacity budget. This is not a RSS cap.
const PREVIEW_PIXELS_BUDGET: usize = 64 * 1024 * 1024;

struct PreviewPixelsDescriptor {
    key: crate::progressive_open::OpenKey,
    source: std::sync::Weak<RetainedPreparedSource>,
    policy: [usize; 2],
    contract: &'static str,
    settings: Vec<u8>,
    selected: Option<crate::dcp::pipeline::SelectedProfileIdentity>,
    adjustments: Vec<u8>,
    resolution: u32,
    divisor_bits: Option<u32>,
}
impl PreviewPixelsDescriptor {
    fn mismatch_reason(&self, other: &Self) -> Option<&'static str> {
        if self.key != other.key
            || !self.source.ptr_eq(&other.source)
            || self.source.upgrade().is_none()
        {
            Some("source")
        } else if self.policy != other.policy || self.contract != other.contract {
            Some("policy")
        } else if self.settings != other.settings {
            Some("settings")
        } else if self.selected != other.selected {
            Some("profile_assets")
        } else if self.adjustments != other.adjustments {
            Some("cpu_inputs")
        } else if self.resolution != other.resolution || self.divisor_bits != other.divisor_bits {
            Some("output_geometry")
        } else {
            None
        }
    }
}
struct PreviewPixelsEntry {
    descriptor: PreviewPixelsDescriptor,
    pixels: crate::open_composition::OwnedPreviewPixels,
}
enum PreviewPixelsStorage<'a> {
    Retained(&'a crate::open_composition::OwnedPreviewPixels),
    Transient(crate::open_composition::OwnedPreviewPixels),
}
struct PreparedPreviewPixels<'a> {
    storage: PreviewPixelsStorage<'a>,
    hit: bool,
}
impl PreparedPreviewPixels<'_> {
    fn pixels(&self) -> &crate::open_composition::OwnedPreviewPixels {
        match &self.storage {
            PreviewPixelsStorage::Retained(p) => p,
            PreviewPixelsStorage::Transient(p) => p,
        }
    }
}
thread_local! {
    static PREVIEW_BASE_DEVELOPMENT_ATTEMPTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn record_preview_base_development() {
    PREVIEW_BASE_DEVELOPMENT_ATTEMPTS.with(|n| n.set(n.get().wrapping_add(1)));
}
fn preview_base_development_count() -> u64 {
    PREVIEW_BASE_DEVELOPMENT_ATTEMPTS.with(std::cell::Cell::get)
}
fn log_preview_cache(outcome: &'static str, reason: &'static str, before: u64) {
    let attempts = preview_base_development_count().wrapping_sub(before);
    log::info!(
        "[preview-cache] outcome={outcome} reason={reason} base_development_attempts={attempts}"
    );
}

// JSON cannot faithfully distinguish nonfinite typed setting values. Such
// programmatic settings execute uncached rather than gaining a false match.
fn preview_settings_finite(s: &AppSettings) -> bool {
    [
        s.high_res_zoom_multiplier,
        s.raw_highlight_compression,
        s.zoom_speed_multiplier,
        s.raw_preprocessing_color_nr,
        s.raw_preprocessing_sharpening,
    ]
    .into_iter()
    .flatten()
    .all(f32::is_finite)
        && s.baseline_exposure.is_finite()
        && s.custom_aspect_ratios
            .iter()
            .all(|r| r.width.is_finite() && r.height.is_finite())
        && s.export_presets.iter().all(|p| {
            [
                p.pad_ratio_width,
                p.pad_ratio_height,
                p.border_horizontal_percent,
                p.border_vertical_percent,
            ]
            .into_iter()
            .flatten()
            .all(f32::is_finite)
        })
}
// These two fields serialize HashSet<String> as arrays. Canonicalize their
// order only; all members and every genuinely ordered settings array remain.
fn canonicalize_preview_settings(settings: &mut Value) {
    for side in ["current", "source"] {
        for field in ["includedAdjustments", "knownAdjustments"] {
            if let Some(values) = settings[side]["copyPasteSettings"][field].as_array_mut() {
                values.sort_unstable_by(|a, b| a.as_str().cmp(&b.as_str()));
            }
        }
    }
    settings.sort_all_objects();
}
impl ConsumerRequest {
    #[cfg(test)]
    fn preview_descriptor(
        &self,
        resolution: Option<u32>,
        divisor: Option<f32>,
    ) -> std::result::Result<Option<PreviewPixelsDescriptor>, String> {
        self.preview_descriptor_diagnostic(resolution, divisor)
            .map(|(descriptor, _)| descriptor)
    }
    fn preview_descriptor_diagnostic(
        &self,
        resolution: Option<u32>,
        divisor: Option<f32>,
    ) -> std::result::Result<(Option<PreviewPixelsDescriptor>, &'static str), String> {
        self.check()?;
        let divisor = crate::open_composition::canonical_preview_divisor(divisor)
            .map_err(|e| e.to_string())?;
        let Some(resolution) = resolution else {
            return Ok((None, "full_accuracy"));
        };
        let assets = self.creative()?;
        let empty = |name: &str| {
            assets
                .adjustments()
                .get(name)
                .is_none_or(|v| v.is_null() || v.as_array().is_some_and(|a| a.is_empty()))
        };
        if !empty("aiPatches") {
            return Ok((None, "patches"));
        }
        if !empty("masks") || !assets.masks().is_empty() {
            return Ok((None, "masks"));
        }
        if assets.relight_map().is_some()
            || assets.fog_map().is_some()
            || assets.lens_blur_map().is_some()
        {
            return Ok((None, "effect_maps"));
        }
        let appearance = self.appearance()?;
        if !self
            .loaded()
            .prepared_source
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &appearance.source))
        {
            return Err("Retained source mismatch".into());
        }
        if !preview_settings_finite(self.settings())
            || !preview_settings_finite(&appearance.source.settings)
        {
            return Ok((None, "nonfinite_settings"));
        }
        let mut adjustments =
            match crate::open_composition::preview_cpu_inputs(assets.adjustments()) {
                Ok(adjustments) => adjustments,
                Err(crate::open_composition::PreviewProjectionError::NonObject) => {
                    return Ok((None, "nonobject_adjustments"));
                }
                Err(crate::open_composition::PreviewProjectionError::Unclassified) => {
                    return Ok((None, "unclassified_adjustment"));
                }
            };
        // Canonical object ordering avoids HashMap iteration producing false
        // misses. Include the two intentionally nonserialized legacy settings.
        let mut settings = serde_json::json!({
            "current":self.settings(), "source":&appearance.source.settings,
            "current_legacy":self.settings.group_associated_files,
            "current_legacy_type":self.settings.group_preferred_type,
            "source_legacy":appearance.source.settings.group_associated_files,
            "source_legacy_type":appearance.source.settings.group_preferred_type,
        });
        canonicalize_preview_settings(&mut settings);
        if crate::ai_denoise::is_requested(assets.adjustments()) {
            adjustments["_denoiseCacheEpoch"] = serde_json::json!(crate::ai_denoise::cache_epoch());
        }
        adjustments.sort_all_objects();
        let (Ok(settings), Ok(adjustments)) = (
            serde_json::to_vec(&settings),
            serde_json::to_vec(&adjustments),
        ) else {
            return Ok((None, "descriptor_encoding"));
        };
        Ok((
            Some(PreviewPixelsDescriptor {
                key: self.key().ok_or("Accurate preview request required")?,
                source: Arc::downgrade(&appearance.source),
                policy: [
                    appearance.source.policy.max_workers,
                    appearance.source.policy.scratch_budget_bytes,
                ],
                contract: "AccurateOne/linear-sRGB/FCS-v1/cpu-inputs-v2",
                settings,
                selected: appearance.assets.as_ref().map(|a| a.content_identity()),
                adjustments,
                resolution,
                divisor_bits: divisor.map(f32::to_bits),
            }),
            "eligible",
        ))
    }

    fn compose_preview_pixels(
        &self,
        resolution: Option<u32>,
        divisor: Option<f32>,
    ) -> std::result::Result<crate::open_composition::OwnedPreviewPixels, String> {
        self.stage(|| {
            let base = self.preview_timed("base_development", || self.request_base())?;
            self.check()?;
            let assets = self.creative()?;
            let composed = self.preview_timed("cpu_composition", || {
                match resolution {
                    Some(edge) => crate::open_composition::compose_accurate_preview(
                        &base,
                        assets,
                        edge,
                        divisor,
                        self.raw_cancel(),
                    ),
                    None => crate::open_composition::compose_full_accuracy(
                        &base,
                        assets,
                        self.raw_cancel(),
                    ),
                }
                .map_err(|e| e.to_string())
            })?;
            self.check()?;
            Ok(composed.into_preview_pixels())
        })
    }

    fn prepare_preview_pixels<'a>(
        &self,
        cache: &'a mut Option<PreviewPixelsEntry>,
        resolution: Option<u32>,
        divisor: Option<f32>,
    ) -> std::result::Result<PreparedPreviewPixels<'a>, String> {
        let before = preview_base_development_count();
        let descriptor = self
            .preview_descriptor_diagnostic(resolution, divisor)
            .inspect_err(|_| {
                log_preview_cache("rejected", "request_or_descriptor", before);
            })?;
        let (descriptor, bypass) = descriptor;
        let reason = match (&descriptor, cache.as_ref()) {
            (Some(current), Some(entry)) => {
                entry.descriptor.mismatch_reason(current).unwrap_or("hit")
            }
            (Some(_), None) => "empty",
            (None, _) => bypass,
        };
        self.check()?;
        if reason == "hit" {
            log_preview_cache("hit", "matched", before);
            return Ok(PreparedPreviewPixels {
                storage: PreviewPixelsStorage::Retained(
                    &cache.as_ref().ok_or("Preview entry unavailable")?.pixels,
                ),
                hit: true,
            });
        }
        if let (Some(current), Some(entry)) = (&descriptor, cache.as_ref()) {
            if entry.descriptor.divisor_bits.is_none()
                && current.divisor_bits.is_some()
                && entry.descriptor.resolution == current.resolution
                && entry.descriptor.mismatch_reason(current) == Some("output_geometry")
            {
                let produced = entry.pixels.derive_interactive(
                    current.resolution,
                    f32::from_bits(current.divisor_bits.ok_or("Interactive divisor missing")?),
                    self.raw_cancel(),
                );
                self.check()
                    .inspect_err(|_| log_preview_cache("rejected", "currentness", before))?;
                let pixels = produced.map_err(|e| e.to_string())
                    .inspect_err(|_| log_preview_cache("error", "composition", before))?;
                log_preview_cache("hit", "derived_interactive", before);
                return Ok(PreparedPreviewPixels {
                    storage: PreviewPixelsStorage::Transient(pixels),
                    // CPU reuse, but not an exact retained output-geometry hit.
                    hit: false,
                });
            }
        }
        // Unsupported requests keep their existing bypass retirement policy.
        // An eligible oversized result must not discard the bounded old entry;
        // every later use still checks its complete descriptor before reuse.
        if descriptor.is_none() {
            *cache = None;
        }
        let produced = self.compose_preview_pixels(resolution, divisor);
        self.check()
            .inspect_err(|_| log_preview_cache("rejected", "currentness", before))?; // authoritative even when development/composition failed
        let pixels = produced.inspect_err(|_| log_preview_cache("error", "composition", before))?;
        let eligible = descriptor.is_some();
        if let Some(descriptor) = descriptor {
            if pixels.within_retention_budget(PREVIEW_PIXELS_BUDGET) {
                self.check()?;
                *cache = Some(PreviewPixelsEntry { descriptor, pixels });
                log_preview_cache("miss", reason, before);
                return Ok(PreparedPreviewPixels {
                    storage: PreviewPixelsStorage::Retained(
                        &cache.as_ref().ok_or("Preview entry unavailable")?.pixels,
                    ),
                    hit: false,
                });
            }
        }
        let reason = if eligible {
            "capacity_or_format"
        } else {
            reason
        };
        log_preview_cache("uncached", reason, before);
        Ok(PreparedPreviewPixels {
            storage: PreviewPixelsStorage::Transient(pixels),
            hit: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cached_preview(
        &self,
        context: &crate::image_processing::GpuContext,
        renderer: &mut crate::gpu_processing::IsolatedCreativeRenderer,
        cache: &mut Option<PreviewPixelsEntry>,
        resolution: Option<u32>,
        roi: Option<(f32, f32, f32, f32)>,
        precision: crate::gpu_processing::RenderOutputPrecision,
        divisor: Option<f32>,
    ) -> std::result::Result<(DynamicImage, (u32, u32), (u32, u32)), String> {
        self.stage(|| {
            if !renderer.matches_context(context) { return Err("Preview renderer context changed".into()); }
            let start = std::time::Instant::now();
            let prepared = self.prepare_preview_pixels(cache, resolution, divisor)?;
            log::info!("[preview-stage] stage=cpu_preview_entry elapsed={:?} hit={}",start.elapsed(),prepared.hit);
            self.check()?;
            let pixels = prepared.pixels();
            let (width,height) = pixels.pixels().dimensions();
            let (scale, offset) = pixels.geometry();
            log::info!("[preview-stage] stage=cpu_preview_geometry width={} height={} scale={} offset_x={} offset_y={}",width,height,scale,offset.0,offset.1);
            let mut request = pixels.request(self.creative()?, self.appearance()?.as_shot()).map_err(|e| e.to_string())?;
            request.roi = roi.map(|(x,y,w,h)|crate::gpu_processing::Roi { x:(x*width as f32).round() as u32, y:(y*height as f32).round() as u32, width:(w*width as f32).round() as u32, height:(h*height as f32).round() as u32 });
            let origin = request.roi.map_or((0,0),|r|(r.x,r.y));
            let result = self.preview_timed("gpu_render_readback",||renderer.render(pixels.pixels(),request,precision));
            self.check()?;
            Ok((result?,(width,height),origin))
        })
    }
}

/// One serial preview worker's private resource owner, never an AppState cache.
/// Keys come only from checked genuine ConsumerRequests. No request/source lease
/// survives a call. Resource drop occurs after short owner checks release locks.
#[derive(Default)]
pub(crate) struct PreviewRendererSession {
    pixels: Option<PreviewPixelsEntry>,
    entry: Option<(
        crate::progressive_open::OpenKey,
        crate::gpu_processing::IsolatedCreativeRenderer,
    )>,
}
impl PreviewRendererSession {
    pub(crate) fn clear(&mut self) {
        self.entry = None;
        self.pixels = None;
    }

    fn checked_key(
        request: &ConsumerRequest,
    ) -> std::result::Result<crate::progressive_open::OpenKey, String> {
        request.check()?;
        request
            .key()
            .ok_or_else(|| "Accurate preview request required".into())
    }

    pub(crate) fn retire_if_stale(&mut self, owner: &crate::progressive_open::OpenOwner) {
        let stale = self
            .entry
            .as_ref()
            .is_some_and(|(key, _)| owner.check_current(*key).is_err());
        if stale {
            self.clear();
        }
    }

    pub(crate) fn render(
        &mut self,
        request: &ConsumerRequest,
        context: &crate::image_processing::GpuContext,
        resolution: Option<u32>,
        roi: Option<(f32, f32, f32, f32)>,
        precision: crate::gpu_processing::RenderOutputPrecision,
        interactive_divisor: Option<f32>,
    ) -> std::result::Result<(DynamicImage, (u32, u32), (u32, u32)), String> {
        let result = (|| {
            let key = Self::checked_key(request)?;
            let start = std::time::Instant::now();
            let reused = self
                .entry
                .as_ref()
                .is_some_and(|(old, renderer)| *old == key && renderer.matches_context(context));
            if !reused {
                self.clear();
                self.entry = Some((
                    key,
                    crate::gpu_processing::IsolatedCreativeRenderer::new(context),
                ));
            }
            log::info!(
                "[preview-stage] stage=gpu_owner elapsed={:?} reused={}",
                start.elapsed(),
                reused
            );
            let (_, renderer) = self.entry.as_mut().ok_or("Preview renderer unavailable")?;
            request.render_cached_preview(
                context,
                renderer,
                &mut self.pixels,
                resolution,
                roi,
                precision,
                interactive_divisor,
            )
        })();
        if result.is_err() {
            self.clear();
        }
        result
    }
}
