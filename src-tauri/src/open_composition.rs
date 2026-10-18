//! Inactive, fit-only CPU composition. No source admission, worker, event,
//! context/display cache, renderer call or publication authority lives here.
//! The caller must pair one prepared appearance with its captured creative
//! snapshot; joint owner/lease wiring is a separate slice.
use crate::as_shot::AsShot;
use crate::gpu_processing::RenderRequest;
use crate::image_loader::{PreparedAppearanceBase, PreparedBaseKind};
use crate::image_processing::{AllAdjustments, Crop, MAX_MASKS};
use crate::lut_processing::Lut;
use crate::mask_generation::{CheckedMaskError, generate_mask_bitmap_checked};
use crate::open_creative_snapshot::CapturedCreativeAssets;
use crate::raw_processing::RawCancel;
use image::{DynamicImage, GenericImageView, GrayImage};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::{Arc, atomic::Ordering};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FitPreviewSpec {
    max_edge: u32,
}
impl FitPreviewSpec {
    pub(crate) fn new(max_edge: u32) -> Result<Self, ComposeError> {
        if max_edge == 0 || max_edge > 1920 {
            return Err(ComposeError::InvalidFit);
        }
        Ok(Self { max_edge })
    }
}

#[derive(Debug)]
pub(crate) enum ComposeError {
    Cancelled,
    InvalidFit,
    InvalidGeometry,
    TooManyMasks(usize),
    Mask(CheckedMaskError),
    MaskOrderMismatch,
    ProfileAlreadyApplied,
    Stage { name: &'static str, message: String },
}
impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fit composition: {self:?}")
    }
}
impl std::error::Error for ComposeError {}

/// Owns pixels and every request buffer; borrows the actual base and captured
/// assets so both owners remain alive through a subsequent borrowed H5 request.
/// This is a CPU request package, not evidence that GPU rendering succeeded.
pub(crate) struct ComposedFit<'a, 'owner> {
    pixels: DynamicImage,
    masks: Vec<GrayImage>,
    uniforms: AllAdjustments,
    lut: Option<Arc<Lut>>,
    as_shot: AsShot,
    kind: PreparedBaseKind,
    scale_for_gpu: f32,
    unscaled_crop_offset: (f32, f32),
    _base: &'a PreparedAppearanceBase<'owner>,
    _assets: &'a CapturedCreativeAssets,
}
impl ComposedFit<'_, '_> {
    pub(crate) fn pixels(&self) -> &DynamicImage {
        &self.pixels
    }
    pub(crate) fn as_shot(&self) -> AsShot {
        self.as_shot
    }
    pub(crate) fn kind(&self) -> PreparedBaseKind {
        self.kind
    }
    pub(crate) fn scale_for_gpu(&self) -> f32 {
        self.scale_for_gpu
    }
    pub(crate) fn unscaled_crop_offset(&self) -> (f32, f32) {
        self.unscaled_crop_offset
    }
    pub(crate) fn request(&self) -> RenderRequest<'_> {
        RenderRequest {
            adjustments: self.uniforms,
            mask_bitmaps: &self.masks,
            lut: self.lut.clone(),
            roi: None,
            dcp_textures: None,
        }
    }
}

/// Pure fitted CPU storage. No base/source/request/asset authority or LUT is
/// retained. A subsequent request must supply its current validated uniforms.
pub(crate) struct OwnedPreviewPixels {
    pixels: DynamicImage,
    masks: Vec<GrayImage>,
    scale_for_gpu: f32,
    unscaled_crop_offset: (f32, f32),
}
impl ComposedFit<'_, '_> {
    pub(crate) fn into_preview_pixels(self) -> OwnedPreviewPixels {
        OwnedPreviewPixels {
            pixels: self.pixels,
            masks: self.masks,
            scale_for_gpu: self.scale_for_gpu,
            unscaled_crop_offset: self.unscaled_crop_offset,
        }
    }
}
impl OwnedPreviewPixels {
    pub(crate) fn pixels(&self) -> &DynamicImage {
        &self.pixels
    }

    pub(crate) fn retained_capacity_bytes(&self) -> Option<usize> {
        use std::mem::size_of;
        let pixels = match &self.pixels {
            DynamicImage::ImageLuma8(v) => v.as_raw().capacity(),
            DynamicImage::ImageLumaA8(v) => v.as_raw().capacity(),
            DynamicImage::ImageRgb8(v) => v.as_raw().capacity(),
            DynamicImage::ImageRgba8(v) => v.as_raw().capacity(),
            DynamicImage::ImageLuma16(v) => v.as_raw().capacity().checked_mul(size_of::<u16>())?,
            DynamicImage::ImageLumaA16(v) => v.as_raw().capacity().checked_mul(size_of::<u16>())?,
            DynamicImage::ImageRgb16(v) => v.as_raw().capacity().checked_mul(size_of::<u16>())?,
            DynamicImage::ImageRgba16(v) => v.as_raw().capacity().checked_mul(size_of::<u16>())?,
            DynamicImage::ImageRgb32F(v) => v.as_raw().capacity().checked_mul(size_of::<f32>())?,
            DynamicImage::ImageRgba32F(v) => v.as_raw().capacity().checked_mul(size_of::<f32>())?,
            _ => return None,
        };
        self.masks
            .iter()
            .try_fold(pixels, |n, mask| n.checked_add(mask.as_raw().capacity()))
    }

    pub(crate) fn within_retention_budget(&self, budget: usize) -> bool {
        self.masks.is_empty() && self.retained_capacity_bytes().is_some_and(|n| n <= budget)
    }

    pub(crate) fn request<'a>(
        &'a self,
        assets: &CapturedCreativeAssets,
        as_shot: AsShot,
    ) -> Result<RenderRequest<'a>, ComposeError> {
        let render = crate::image_processing::resolve_render_options(assets.settings(), true);
        let uniforms = crate::image_processing::get_all_adjustments_from_json(
            assets.adjustments(),
            true,
            as_shot,
            render,
        );
        if uniforms.mask_count as usize != self.masks.len() {
            return Err(ComposeError::MaskOrderMismatch);
        }
        if uniforms.global.has_dcp != 0 {
            return Err(ComposeError::ProfileAlreadyApplied);
        }
        Ok(RenderRequest {
            adjustments: uniforms,
            mask_bitmaps: &self.masks,
            lut: assets.lut().cloned(),
            roi: None,
            dcp_textures: None,
        })
    }

    pub(crate) fn geometry(&self) -> (f32, (f32, f32)) {
        (self.scale_for_gpu, self.unscaled_crop_offset)
    }
}

fn check_cancel(cancel: &RawCancel) -> Result<(), ComposeError> {
    if let Some((tracker, generation)) = cancel {
        if tracker.load(Ordering::SeqCst) != *generation {
            return Err(ComposeError::Cancelled);
        }
    }
    Ok(())
}

/// Checks both sides, including an error return. Existing monolithic kernels
/// are not preemptible; cancelled local results are discarded after return.
fn stage<T, E: std::fmt::Display>(
    cancel: &RawCancel,
    name: &'static str,
    work: impl FnOnce() -> Result<T, E>,
) -> Result<T, ComposeError> {
    check_cancel(cancel)?;
    let result = work();
    check_cancel(cancel)?;
    result.map_err(|error| ComposeError::Stage {
        name,
        message: error.to_string(),
    })
}

struct FitGeometry {
    crop: Option<Crop>,
    full_w: f64,
    working_w: u32,
    working_h: u32,
    working_scale: f64,
    needs_resize: bool,
    spatial_adjustments: Value,
}
impl FitGeometry {
    /// Same crop-aware round/clamp/max1 arithmetic as the normal preview;
    /// stage caches and effects are deliberately absent from this planner.
    fn new(
        warped_w: u32,
        warped_h: u32,
        adjustments: &Value,
        fit: FitPreviewSpec,
    ) -> Result<Self, ComposeError> {
        Self::for_output(warped_w, warped_h, adjustments, Some(fit.max_edge))
    }
    fn for_output(
        warped_w: u32,
        warped_h: u32,
        adjustments: &Value,
        preview_dim: Option<u32>,
    ) -> Result<Self, ComposeError> {
        if warped_w == 0 || warped_h == 0 {
            return Err(ComposeError::InvalidGeometry);
        }
        let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0);
        let (oriented_w, oriented_h) = if orientation_steps % 2 == 1 {
            (warped_h as f64, warped_w as f64)
        } else {
            (warped_w as f64, warped_h as f64)
        };
        let crop: Option<Crop> = serde_json::from_value(adjustments["crop"].clone()).ok();
        let (full_w, full_h) = crop
            .as_ref()
            .and_then(|c| {
                let (x, y) = (c.x.round().max(0.0), c.y.round().max(0.0));
                let (w, h) = (c.width.round(), c.height.round());
                (w > 0.0 && h > 0.0 && x < oriented_w && y < oriented_h)
                    .then(|| ((oriented_w - x).min(w), (oriented_h - y).min(h)))
            })
            .unwrap_or((oriented_w, oriented_h));
        let target_scale =
            preview_dim.map_or(1.0, |limit| (limit as f64 / full_w.max(full_h)).min(1.0));
        let (working_w, working_h) = if target_scale < 1.0 {
            (
                ((warped_w as f64 * target_scale).round() as u32).max(1),
                ((warped_h as f64 * target_scale).round() as u32).max(1),
            )
        } else {
            (warped_w, warped_h)
        };
        let mut result = Self {
            crop,
            full_w,
            working_w,
            working_h,
            working_scale: 1.0,
            needs_resize: target_scale < 1.0,
            spatial_adjustments: Value::Null,
        };
        result.use_output_width(working_w, warped_w, adjustments, preview_dim);
        Ok(result)
    }
    // downscale_f32_image itself preserves aspect/rounds dimensions. Use the
    // ACTUAL returned width, exactly as compute_transformed_preview does.
    fn use_working_width(
        &mut self,
        width: u32,
        warped_w: u32,
        adjustments: &Value,
        preview_dim: u32,
    ) {
        self.use_output_width(width, warped_w, adjustments, Some(preview_dim));
    }
    fn use_output_width(
        &mut self,
        width: u32,
        warped_w: u32,
        adjustments: &Value,
        preview_dim: Option<u32>,
    ) {
        let working_scale = width as f64 / warped_w.max(1) as f64;
        let max_dim = preview_dim.map_or(f64::INFINITY, f64::from);
        self.working_scale = working_scale;
        self.spatial_adjustments = serde_json::json!({
            "orientationSteps": adjustments["orientationSteps"],
            "rotation": adjustments["rotation"],
            "flipHorizontal": adjustments["flipHorizontal"],
            "flipVertical": adjustments["flipVertical"],
            "crop": self.crop.as_ref().map(|c| Crop {
                x: c.x * working_scale,
                y: c.y * working_scale,
                width: (c.width * working_scale).min(max_dim),
                height: (c.height * working_scale).min(max_dim),
            }),
        });
    }
    fn unscaled_offset(&self) -> (f32, f32) {
        self.crop
            .as_ref()
            .map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32))
    }
    fn scale_for_gpu(&self, preview_width: u32) -> f32 {
        if self.full_w > 0.0 {
            (preview_width as f64 / self.full_w) as f32
        } else {
            1.0
        }
    }
}

/// Only a full-resolution UNPATCHED profile/pristine base is accepted here.
/// Default RAW tone belongs solely to the mask reference, never display pixels.
fn mask_reference(
    base: &PreparedAppearanceBase<'_>,
    adjustments: &Value,
    cancel: &RawCancel,
) -> Result<DynamicImage, ComposeError> {
    stage(cancel, "mask reference", || {
        let reference =
            crate::mask_generation::build_full_warped_image(base.pixels(), true, adjustments)
                .into_owned();
        if reference.width() == 0 || reference.height() == 0 {
            return Err("Empty mask reference");
        }
        Ok(reference)
    })
}

pub(crate) fn compose_fit<'a, 'owner>(
    base: &'a PreparedAppearanceBase<'owner>,
    assets: &'a CapturedCreativeAssets,
    fit: FitPreviewSpec,
    cancel: RawCancel,
) -> Result<ComposedFit<'a, 'owner>, ComposeError> {
    FitPreviewSpec::new(fit.max_edge)?;
    compose_output(base, assets, Some(fit.max_edge), None, cancel)
}

/// Full-accuracy editor/analysis/export composition. This is not a placeholder
/// or a widened FitPreviewSpec; no downscale/crop cap is imposed here.
pub(crate) fn compose_full_accuracy<'a, 'owner>(
    base: &'a PreparedAppearanceBase<'owner>,
    assets: &'a CapturedCreativeAssets,
    cancel: RawCancel,
) -> Result<ComposedFit<'a, 'owner>, ComposeError> {
    compose_output(base, assets, None, None, cancel)
}
// Dependency contract ONLY for the existing mask/patch/map-free fitted entry.
// compose_output's CPU stages consume geometry/spatial/profile/effect inputs.
// Fresh uniforms and the current captured LUT are rebuilt by OwnedPreviewPixels::request.
// Unknown schema fields bypass retention: future CPU inputs never become GPU-only by default.
const PREVIEW_FRESH_RENDER_FIELDS: &[&str] = &[
    "baselineExposure",
    "blacks",
    "brightness",
    "centré",
    "centréX",
    "centréY",
    "clarity",
    "chromaticAberrationBlueYellow",
    "chromaticAberrationRedCyan",
    "colorCalibration",
    "colorGrading",
    "colorNoiseReduction",
    "contrast",
    "curves",
    "pointCurves",
    "parametricCurve",
    "curveMode",
    "dehaze",
    "exposure",
    "flareAmount",
    "glowAmount",
    "grainAmount",
    "grainRoughness",
    "grainSize",
    "halationAmount",
    "highlights",
    "hsl",
    "pointColour",
    "hue",
    "lumaNoiseReduction",
    "lutData",
    "lutIntensity",
    "lutName",
    "lutPath",
    "lutSize",
    "lutIsSceneReferred",
    "saturation",
    "shadows",
    "sharpness",
    "sharpnessThreshold",
    "showClipping",
    "structure",
    "temperature",
    "tint",
    "toneMapper",
    "vibrance",
    "vignetteCenterX",
    "vignetteCenterY",
    "vignetteAmount",
    "vignetteFeather",
    "vignetteMidpoint",
    "vignetteRoundness",
    "whiteBalance",
    "whites",
];
const PREVIEW_RETAINED_CPU_FIELDS: &[&str] = &[
    "aiDenoiseEnabled",
    "aiDenoiseAmount",
    "aiDenoiseDetail",
    "aiPatches",
    "aspectRatio",
    "crop",
    "flipHorizontal",
    "flipVertical",
    "lensBlurAmount",
    "lensBlurDiffusion",
    "lensBlurShape",
    "lensBlurDepthMap",
    "lensBlurEnabled",
    "lensBlurMaxDepth",
    "lensBlurMaxFade",
    "lensBlurMinDepth",
    "lensBlurMinFade",
    "relightEnabled",
    "relightNormalMap",
    "relightLights",
    "relightAmbient",
    "relightSoftness",
    "relightShine",
    "relightShadows",
    "relightShadowSoftness",
    "fogEnabled",
    "fogDepthMap",
    "fogAmount",
    "fogStart",
    "fogDensity",
    "fogHeight",
    "fogVariation",
    "fogGlow",
    "fogTemperature",
    "fogTint",
    "lensCorrectionMode",
    "masks",
    "orientationSteps",
    "profile",
    "rotation",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewInputRole {
    Cpu,
    FreshRender,
    Unclassified,
}
fn preview_input_role(key: &str) -> PreviewInputRole {
    if crate::cache_utils::GEOMETRY_KEYS.contains(&key)
        || PREVIEW_RETAINED_CPU_FIELDS.contains(&key)
        || key == "sectionVisibility"
        || key == "activeTools"
    {
        PreviewInputRole::Cpu
    } else if PREVIEW_FRESH_RENDER_FIELDS.contains(&key) {
        PreviewInputRole::FreshRender
    } else {
        PreviewInputRole::Unclassified
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewProjectionError {
    NonObject,
    Unclassified,
}
pub(crate) fn preview_cpu_inputs(adjustments: &Value) -> Result<Value, PreviewProjectionError> {
    let object = adjustments
        .as_object()
        .ok_or(PreviewProjectionError::NonObject)?;
    let mut projected = serde_json::Map::new();
    for (key, value) in object {
        if key == "activeTools" {
            let Some(tools) = value.as_object() else {
                return Err(PreviewProjectionError::Unclassified);
            };
            let mut cpu = serde_json::Map::new();
            for (tool, active) in tools {
                match tool.as_str() {
                    "lensBlur" | "relight" | "fog" | "noiseReduction" => {
                        cpu.insert(tool.clone(), active.clone());
                    }
                    "basic"
                    | "curves"
                    | "whiteBalance"
                    | "colorPresence"
                    | "hue"
                    | "colorGrading"
                    | "colorMixer"
                    | "colorCalibration"
                    | "sharpening"
                    | "presence"
                    | "chromaticAberration"
                    | "creative"
                    | "lut"
                    | "vignette"
                    | "grain" => {}
                    _ => return Err(PreviewProjectionError::Unclassified),
                }
            }
            projected.insert(key.clone(), Value::Object(cpu));
            continue;
        }
        if key == "sectionVisibility" {
            // Only effects visibility is read by the CPU effect/map path. Keep
            // its exact representation; basic/color/curves/details are fresh uniforms.
            let value = if let Some(sections) = value.as_object() {
                if sections.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "basic" | "color" | "curves" | "details" | "effects"
                    )
                }) {
                    return Err(PreviewProjectionError::Unclassified);
                }
                let mut cpu = serde_json::Map::new();
                if let Some(effects) = sections.get("effects") {
                    cpu.insert("effects".into(), effects.clone());
                }
                // Legacy details gates AI denoise only when it can affect pixels.
                if crate::ai_denoise::is_requested(adjustments) {
                    if let Some(details) = sections.get("details") {
                        cpu.insert("details".into(), details.clone());
                    }
                }
                if cpu.is_empty() {
                    continue;
                }
                Value::Object(cpu)
            } else {
                value.clone()
            };
            projected.insert(key.clone(), value);
            continue;
        }
        match preview_input_role(key) {
            PreviewInputRole::Cpu => {
                projected.insert(key.clone(), value.clone());
            }
            PreviewInputRole::FreshRender => {}
            PreviewInputRole::Unclassified => return Err(PreviewProjectionError::Unclassified),
        }
    }
    let mut projected = Value::Object(projected);
    projected.sort_all_objects();
    Ok(projected)
}

// Exactly unity has no post-effect resize, just like None. Do not canonicalize
// invalid values into a valid request or round any valid non-unit divisor.
pub(crate) fn canonical_preview_divisor(divisor: Option<f32>) -> Result<Option<f32>, ComposeError> {
    if divisor.is_some_and(|v| !v.is_finite() || v < 1.0) {
        return Err(ComposeError::InvalidGeometry);
    }
    Ok(divisor.filter(|v| *v != 1.0))
}

/// Accurate editor output resolution (including zoom); separate from the
/// <=1920 placeholder contract. Preserve the old post-effect interactive resize.
pub(crate) fn compose_accurate_preview<'a, 'owner>(
    base: &'a PreparedAppearanceBase<'owner>,
    assets: &'a CapturedCreativeAssets,
    max_edge: u32,
    interactive_divisor: Option<f32>,
    cancel: RawCancel,
) -> Result<ComposedFit<'a, 'owner>, ComposeError> {
    if max_edge == 0 {
        return Err(ComposeError::InvalidGeometry);
    }
    let interactive_divisor = canonical_preview_divisor(interactive_divisor)?;
    let final_edge = interactive_divisor
        .filter(|v| *v > 1.0)
        .map(|v| ((max_edge as f32 / v) as u32).max(1));
    compose_output(base, assets, Some(max_edge), final_edge, cancel)
}
fn compose_output<'a, 'owner>(
    base: &'a PreparedAppearanceBase<'owner>,
    assets: &'a CapturedCreativeAssets,
    preview_dim: Option<u32>,
    final_edge: Option<u32>,
    cancel: RawCancel,
) -> Result<ComposedFit<'a, 'owner>, ComposeError> {
    check_cancel(&cancel)?;
    let adjustments = assets.adjustments();
    let masks = assets.masks();
    if masks.len() > MAX_MASKS {
        return Err(ComposeError::TooManyMasks(masks.len()));
    }
    if base.pixels().width() == 0 || base.pixels().height() == 0 {
        return Err(ComposeError::InvalidGeometry);
    }
    let patched = stage(&cancel, "patches", || {
        crate::image_loader::composite_patches_on_image(base.pixels(), adjustments)
    })?;
    let warped = stage(&cancel, "geometry warp", || {
        Ok::<_, std::convert::Infallible>(
            crate::image_processing::apply_geometry_warp(Cow::Owned(patched), adjustments)
                .into_owned(),
        )
    })?;
    let mut geometry = match preview_dim {
        Some(limit) if limit <= 1920 => FitGeometry::new(
            warped.width(),
            warped.height(),
            adjustments,
            FitPreviewSpec::new(limit)?,
        )?,
        other => FitGeometry::for_output(warped.width(), warped.height(), adjustments, other)?,
    };
    let warped_w = warped.width();
    let working = stage(&cancel, "fit resize", || {
        Ok::<_, std::convert::Infallible>(if geometry.needs_resize {
            crate::image_processing::downscale_f32_image(
                &warped,
                geometry.working_w,
                geometry.working_h,
            )
        } else {
            warped
        })
    })?;
    match preview_dim {
        Some(limit) => geometry.use_working_width(working.width(), warped_w, adjustments, limit),
        None => geometry.use_output_width(working.width(), warped_w, adjustments, None),
    }
    // Actual captured map Arcs; no map lookup/decode and no sampled-pixel model
    // cache. Fresh scratch is discarded at the end of this single composition.
    let mut scratch = crate::relight::LocalRelightScratch::default();
    let relit = stage(&cancel, "relight", || {
        Ok::<_, std::convert::Infallible>(match assets.relight_map() {
            Some(map) => crate::relight::apply_relight_captured(
                Cow::Owned(working),
                adjustments,
                map,
                &mut scratch,
            )
            .into_owned(),
            None => working,
        })
    })?;
    drop(scratch);
    let fogged = stage(&cancel, "fog", || {
        Ok::<_, std::convert::Infallible>(match assets.fog_map() {
            Some(map) => {
                crate::fog::apply_fog_captured(Cow::Owned(relit), adjustments, map).into_owned()
            }
            None => relit,
        })
    })?;
    let blurred = stage(&cancel, "lens blur", || {
        Ok::<_, std::convert::Infallible>(match assets.lens_blur_map() {
            Some(map) => {
                crate::lens_blur::apply_lens_blur_captured(Cow::Owned(fogged), adjustments, map)
                    .into_owned()
            }
            None => fogged,
        })
    })?;
    let pixels = stage(&cancel, "spatial transform", || {
        Ok::<_, std::convert::Infallible>(
            crate::adjustment_utils::apply_spatial_transformations(
                Cow::Owned(blurred),
                &geometry.spatial_adjustments,
            )
            .0
            .into_owned(),
        )
    })?;
    let pixels = if let Some(edge) = final_edge {
        stage(&cancel, "interactive resize", || {
            let (w, h) = pixels.dimensions();
            let (out_w, out_h) = if w > h {
                (edge, (edge as f32 * (h as f32 / w as f32)) as u32)
            } else {
                ((edge as f32 * (w as f32 / h as f32)) as u32, edge)
            };
            Ok::<_, std::convert::Infallible>(crate::image_processing::downscale_f32_image(
                &pixels, out_w, out_h,
            ))
        })?
    } else {
        pixels
    };
    let scale_for_gpu = geometry.scale_for_gpu(pixels.width());
    let unscaled_crop_offset = geometry.unscaled_offset();
    let scaled_crop_offset = (
        unscaled_crop_offset.0 * scale_for_gpu,
        unscaled_crop_offset.1 * scale_for_gpu,
    );
    let reference = if masks.iter().any(|mask| mask.requires_warped_image()) {
        Some(mask_reference(base, adjustments, &cancel)?)
    } else {
        None
    };
    let mut bitmaps = Vec::with_capacity(masks.len());
    for mask in masks {
        check_cancel(&cancel)?;
        let bitmap = generate_mask_bitmap_checked(
            mask,
            pixels.width(),
            pixels.height(),
            scale_for_gpu,
            scaled_crop_offset,
            reference.as_ref(),
        );
        check_cancel(&cancel)?;
        let bitmap = bitmap
            .map_err(ComposeError::Mask)?
            .ok_or(ComposeError::MaskOrderMismatch)?;
        bitmaps.push(bitmap);
    }
    let uniforms = stage(&cancel, "uniforms", || {
        let render = crate::image_processing::resolve_render_options(assets.settings(), true);
        Ok::<_, std::convert::Infallible>(crate::image_processing::get_all_adjustments_from_json(
            adjustments,
            true,
            base.as_shot(),
            render,
        ))
    })?;
    if uniforms.mask_count as usize != bitmaps.len() {
        return Err(ComposeError::MaskOrderMismatch);
    }
    if uniforms.global.has_dcp != 0 {
        return Err(ComposeError::ProfileAlreadyApplied);
    }
    check_cancel(&cancel)?;
    Ok(ComposedFit {
        pixels,
        masks: bitmaps,
        uniforms,
        lut: assets.lut().cloned(),
        as_shot: base.as_shot(),
        kind: base.kind(),
        scale_for_gpu,
        unscaled_crop_offset,
        _base: base,
        _assets: assets,
    })
}

#[cfg(test)]
mod composition_tests {
    use super::*;
    use base64::Engine as _;
    use image::{ImageBuffer, Luma, Rgb, Rgb32FImage};
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn preview_pixels_budget_and_unsupported_are_uncached() {
        let mut data = Vec::<f32>::with_capacity((64 * 1024 * 1024) / 4 + 1);
        data.extend_from_slice(&[0.2, 0.3, 0.4]);
        let pixels = OwnedPreviewPixels {
            pixels: DynamicImage::ImageRgb32F(Rgb32FImage::from_raw(1, 1, data).unwrap()),
            masks: Vec::new(),
            scale_for_gpu: 1.0,
            unscaled_crop_offset: (0.0, 0.0),
        };
        let capacity = pixels.retained_capacity_bytes().unwrap();
        assert!(capacity > 64 * 1024 * 1024);
        assert!(!pixels.within_retention_budget(64 * 1024 * 1024));
        assert!(pixels.within_retention_budget(capacity));
        assert!(!pixels.within_retention_budget(capacity - 1));
        let with_mask = OwnedPreviewPixels {
            pixels: rgb(2, 2),
            masks: vec![GrayImage::new(2, 2)],
            scale_for_gpu: 1.0,
            unscaled_crop_offset: (0.0, 0.0),
        };
        assert!(!with_mask.within_retention_budget(usize::MAX));
        let plain = OwnedPreviewPixels {
            pixels: rgb(2, 2),
            masks: Vec::new(),
            scale_for_gpu: 1.0,
            unscaled_crop_offset: (0.0, 0.0),
        };
        let bytes = plain.retained_capacity_bytes().unwrap();
        assert!(plain.within_retention_budget(bytes));
        assert!(!plain.within_retention_budget(bytes - 1));
    }

    fn rgb(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(w, h, |x, y| {
            Rgb([x as f32 / w as f32, y as f32 / h as f32, 1.25])
        }))
    }
    fn inline(image: DynamicImage) -> String {
        let mut buffer = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(buffer.into_inner())
    }
    fn capture(adjustments: &Value) -> CapturedCreativeAssets {
        crate::open_creative_snapshot::capture_creative_assets(
            adjustments,
            &crate::app_settings::AppSettings::default(),
            None,
        )
        .unwrap()
    }

    // BEGIN deterministic strict-effect test context (test-only).
    fn with_single_thread_effect_pool(work: impl FnOnce() + Send) {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        pool.install(move || {
            assert_eq!(rayon::current_num_threads(), 1);
            work();
        });
        // Installed work has completed; pool Drop only requests worker shutdown.
    }
    // END deterministic strict-effect test context (test-only).

    // BEGIN B2-COVERAGE-TEST-ADDITIONS
    #[test]
    fn l08_actual_compose_fit_patches_once_and_preserves_base() {
        let owner = crate::image_loader::composition_test_owner();
        let source = owner.retained_source();
        let (pristine, _) = source
            .render_pristine(crate::image_loader::AppearancePass::AccurateOne, None)
            .unwrap();
        let pristine_before = pristine.as_bytes().to_vec();
        let base = owner
            .render_unpatched_base(crate::image_loader::AppearancePass::AccurateOne, None)
            .unwrap();
        assert_eq!(base.pixels().as_bytes(), pristine.as_bytes());
        assert!(Arc::ptr_eq(base.retained_source(), &source));
        let base_before = base.pixels().as_bytes().to_vec();
        let adjustments = json!({"aiPatches":[{"id":"partial-red","name":"partial-red","visible":true,"patchData":{"offsetX":0,"offsetY":0,"color":inline(DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1,1,Rgb([255,0,0])))),"mask":inline(DynamicImage::ImageLuma8(GrayImage::from_pixel(1,1,Luma([128])))),"isSrgbEncoded":true}}]});
        let assets = crate::open_creative_snapshot::capture_creative_assets(
            &adjustments,
            owner.settings(),
            None,
        )
        .unwrap();
        let fit = compose_fit(&base, &assets, FitPreviewSpec::new(1024).unwrap(), None).unwrap();
        let once =
            crate::image_loader::composite_patches_on_image(base.pixels(), assets.adjustments())
                .unwrap();
        let twice =
            crate::image_loader::composite_patches_on_image(&once, assets.adjustments()).unwrap();
        assert_eq!(fit.pixels().dimensions(), base.pixels().dimensions());
        assert_eq!(fit.pixels().as_bytes(), once.as_bytes());
        assert_ne!(fit.pixels().as_bytes(), base.pixels().as_bytes());
        assert_ne!(fit.pixels().as_bytes(), twice.as_bytes());
        assert_eq!(fit.scale_for_gpu(), 1.0);
        assert_eq!(fit.unscaled_crop_offset(), (0.0, 0.0));
        assert_eq!(base.pixels().as_bytes(), base_before);
        assert_eq!(pristine.as_bytes(), pristine_before);
        assert!(Arc::ptr_eq(base.retained_source(), &source));
    }

    // Direct transcription of the pre-existing pure cold preview sequence in
    // lib.rs232–342. Expected pixels never call FitGeometry or compose_fit.
    // Actual resizer width is used; no ratio is inferred from the final crop.
    fn old_cold_geometry_sequence(
        base: &DynamicImage,
        adjustments: &Value,
        preview_dim: u32,
    ) -> (DynamicImage, f64, f32, (f32, f32), (f32, f32)) {
        let warped = crate::image_processing::apply_geometry_warp(Cow::Borrowed(base), adjustments);
        let (warped_w, warped_h) = warped.dimensions();
        let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0);
        let (oriented_w, oriented_h) = if orientation_steps % 2 == 1 {
            (warped_h as f64, warped_w as f64)
        } else {
            (warped_w as f64, warped_h as f64)
        };
        let crop: Option<Crop> = serde_json::from_value(adjustments["crop"].clone()).ok();
        let (full_w, full_h) = crop
            .as_ref()
            .and_then(|c| {
                let (x, y) = (c.x.round().max(0.0), c.y.round().max(0.0));
                let (w, h) = (c.width.round(), c.height.round());
                (w > 0.0 && h > 0.0 && x < oriented_w && y < oriented_h)
                    .then(|| ((oriented_w - x).min(w), (oriented_h - y).min(h)))
            })
            .unwrap_or((oriented_w, oriented_h));
        let target_scale = (preview_dim as f64 / full_w.max(full_h)).min(1.0);
        let working = if target_scale < 1.0 {
            let working_w = ((warped_w as f64 * target_scale).round() as u32).max(1);
            let working_h = ((warped_h as f64 * target_scale).round() as u32).max(1);
            crate::image_processing::downscale_f32_image(&warped, working_w, working_h)
        } else {
            warped.into_owned()
        };
        let working_scale = working.width() as f64 / warped_w.max(1) as f64;
        let max_dim = preview_dim as f64;
        let spatial = json!({"orientationSteps":adjustments["orientationSteps"],"rotation":adjustments["rotation"],"flipHorizontal":adjustments["flipHorizontal"],"flipVertical":adjustments["flipVertical"],"crop":crop.as_ref().map(|c|Crop{x:c.x*working_scale,y:c.y*working_scale,width:(c.width*working_scale).min(max_dim),height:(c.height*working_scale).min(max_dim)})});
        let preview =
            crate::adjustment_utils::apply_spatial_transformations(Cow::Owned(working), &spatial)
                .0
                .into_owned();
        let scale_for_gpu = if full_w > 0.0 {
            (preview.width() as f64 / full_w) as f32
        } else {
            1.0
        };
        let unscaled = crop.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));
        let scaled = (unscaled.0 * scale_for_gpu, unscaled.1 * scale_for_gpu);
        (preview, working_scale, scale_for_gpu, unscaled, scaled)
    }

    #[test]
    fn l09_actual_odd_geometry_matches_old_cold_sequence() {
        let owner = crate::image_loader::composition_patterned_test_owner();
        let base = owner
            .render_unpatched_base(crate::image_loader::AppearancePass::AccurateOne, None)
            .unwrap();
        let before = base.pixels().as_bytes().to_vec();
        assert_eq!(base.pixels().dimensions(), (520, 512));
        let adjustments = json!({"transformVertical":9.0,"transformHorizontal":-6.0,"orientationSteps":1,"flipHorizontal":true,"flipVertical":true,"rotation":7.0,"crop":{"x":13.25,"y":17.5,"width":311.75,"height":273.25},"masks":[{"id":"offset-linear","name":"offset-linear","visible":true,"invert":false,"opacity":100.0,"adjustments":{},"subMasks":[{"id":"line","type":"linear","visible":true,"invert":false,"opacity":100.0,"mode":"additive","parameters":{"startX":31.0,"startY":53.0,"endX":211.0,"endY":97.0,"range":73.0}}]}]});
        let params = crate::image_processing::get_geometry_params_from_json(&adjustments);
        assert!(!crate::image_processing::is_geometry_identity(&params));
        let warped = crate::image_processing::apply_geometry_warp(
            Cow::Borrowed(base.pixels()),
            &adjustments,
        );
        assert_ne!(
            warped.as_bytes(),
            base.pixels().as_bytes(),
            "Fixed perspective must actually change pixels"
        );
        let assets = crate::open_creative_snapshot::capture_creative_assets(
            &adjustments,
            owner.settings(),
            None,
        )
        .unwrap();
        let actual = compose_fit(&base, &assets, FitPreviewSpec::new(97).unwrap(), None).unwrap();
        let (expected, working_scale, gpu_scale, unscaled, scaled) =
            old_cold_geometry_sequence(base.pixels(), assets.adjustments(), 97);
        assert_eq!(actual.pixels().dimensions(), expected.dimensions());
        assert_eq!(actual.pixels().as_bytes(), expected.as_bytes());
        // The intermediate width is not exposed by ComposedFit. Whole output
        // equality above and offset-sensitive masks below cover its use; this
        // scalar comes only from the original pure cold sequence.
        assert_eq!(working_scale, 161.0 / 520.0);
        assert_eq!(actual.scale_for_gpu(), gpu_scale);
        assert_eq!(actual.unscaled_crop_offset(), unscaled);
        assert_eq!(unscaled, (13.25, 17.5));
        assert_eq!(
            scaled,
            (
                13.25 * actual.scale_for_gpu(),
                17.5 * actual.scale_for_gpu()
            )
        );
        let expected_mask = generate_mask_bitmap_checked(
            &assets.masks()[0],
            expected.width(),
            expected.height(),
            gpu_scale,
            scaled,
            None,
        )
        .unwrap()
        .unwrap();
        let wrong_offset = generate_mask_bitmap_checked(
            &assets.masks()[0],
            expected.width(),
            expected.height(),
            gpu_scale,
            (0.0, 0.0),
            None,
        )
        .unwrap()
        .unwrap();
        let request = actual.request();
        assert_eq!(request.mask_bitmaps.len(), 1);
        assert_eq!(request.mask_bitmaps[0].as_raw(), expected_mask.as_raw());
        assert_ne!(
            expected_mask.as_raw(),
            wrong_offset.as_raw(),
            "Mask must distinguish a dropped scaled crop offset"
        );
        let mut no_warp = adjustments.clone();
        no_warp["transformVertical"] = json!(0.0);
        no_warp["transformHorizontal"] = json!(0.0);
        let missing_warp = old_cold_geometry_sequence(base.pixels(), &no_warp, 97).0;
        assert_ne!(expected.as_bytes(), missing_warp.as_bytes());
        let wrong_order = crate::adjustment_utils::apply_spatial_transformations(
            Cow::Borrowed(base.pixels()),
            &adjustments,
        )
        .0;
        let wrong_order = crate::image_processing::apply_geometry_warp(wrong_order, &adjustments);
        let wrong_order = crate::image_processing::downscale_f32_image(
            &wrong_order,
            expected.width(),
            expected.height(),
        );
        assert_ne!(
            expected.as_bytes(),
            wrong_order.as_bytes(),
            "Fixed source must distinguish spatial-before-warp order"
        );
        assert_eq!(base.pixels().as_bytes(), before);
    }

    #[test]
    fn l10_active_lens_blur_matches_old_and_captured_kernels() {
        with_single_thread_effect_pool(l10_active_lens_blur_matches_old_and_captured_kernels_body);
    }

    fn l10_active_lens_blur_matches_old_and_captured_kernels_body() {
        let source = DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(128, 96, |x, y| {
            Rgb([
                if (x / 2 + y / 2) % 2 == 0 { 0.1 } else { 0.9 },
                0.05 + 0.75 * x as f32 / 127.0,
                if x >= 64 { 1.2 } else { 0.03 },
            ])
        }));
        let before = source.as_bytes().to_vec();
        let depth = inline(DynamicImage::ImageLuma8(GrayImage::from_fn(
            8,
            8,
            |x, y| Luma([(16 + (19 * x + 13 * y) % 193) as u8]),
        )));
        let normal = inline(DynamicImage::ImageRgba8(image::RgbaImage::from_fn(
            8,
            8,
            |x, y| {
                image::Rgba([
                    (128 + 3 * x) as u8,
                    (128 + 3 * y) as u8,
                    0,
                    (180 + 5 * x) as u8,
                ])
            },
        )));
        let adjustments = json!({"lensBlurEnabled":true,"lensBlurDepthMap":depth,"lensBlurAmount":100.0,"lensBlurDiffusion":25.0,"lensBlurMinDepth":100.0,"lensBlurMaxDepth":100.0,"lensBlurMinFade":0.0,"lensBlurMaxFade":0.0,"relightEnabled":true,"relightNormalMap":normal,"relightAmbient":12.0,"relightLights":[{"type":"directional","visible":true}],"fogEnabled":true,"fogDepthMap":depth,"fogAmount":30.0});
        let assets = capture(&adjustments);
        let depth_before = assets.lens_blur_map().unwrap().as_raw().clone();
        let fog_before = assets.fog_map().unwrap().as_raw().clone();
        let normal_before = assets.relight_map().unwrap().as_raw().clone();
        let adjustments_before = adjustments.clone();
        let max_radius =
            (adjustments["lensBlurAmount"].as_f64().unwrap() as f32 / 100.0) * 128.0 * 0.012;
        let diffusion =
            (adjustments["lensBlurDiffusion"].as_f64().unwrap() as f32 / 100.0).clamp(0.0, 1.0);
        assert_eq!(max_radius, 1.536f32);
        assert!(max_radius >= 0.35);
        assert_eq!(diffusion, 0.25);
        assert!(
            diffusion > 0.0,
            "Sparse-CoC early-return condition diffusion<=0 is deliberately false"
        );
        let mut disabled = adjustments.clone();
        disabled["lensBlurEnabled"] = json!(false);
        let disabled_output =
            crate::lens_blur::apply_lens_blur(Cow::Borrowed(&source), &disabled).into_owned();
        assert_eq!(disabled_output.as_bytes(), source.as_bytes());
        let old =
            crate::lens_blur::apply_lens_blur(Cow::Borrowed(&source), &adjustments).into_owned();
        let captured = crate::lens_blur::apply_lens_blur_captured(
            Cow::Borrowed(&source),
            assets.adjustments(),
            assets.lens_blur_map().unwrap(),
        )
        .into_owned();
        assert_ne!(
            old.as_bytes(),
            disabled_output.as_bytes(),
            "Fixed active blur must change input; no tune-after-run"
        );
        assert_eq!(captured.as_bytes(), old.as_bytes());
        // Normal relight uses its real cache. Execute this entire cohort
        // serially, as mandated by the existing sentinel tests.
        let relit = crate::relight::apply_relight(Cow::Borrowed(&source), &adjustments);
        let preblur = crate::fog::apply_fog(relit, &adjustments).into_owned();
        let old_combined =
            crate::lens_blur::apply_lens_blur(Cow::Borrowed(&preblur), &adjustments).into_owned();
        let mut scratch = crate::relight::LocalRelightScratch::default();
        let relit = crate::relight::apply_relight_captured(
            Cow::Borrowed(&source),
            assets.adjustments(),
            assets.relight_map().unwrap(),
            &mut scratch,
        );
        let fogged =
            crate::fog::apply_fog_captured(relit, assets.adjustments(), assets.fog_map().unwrap());
        let captured_combined = crate::lens_blur::apply_lens_blur_captured(
            fogged,
            assets.adjustments(),
            assets.lens_blur_map().unwrap(),
        )
        .into_owned();
        assert_ne!(
            old_combined.as_bytes(),
            preblur.as_bytes(),
            "Combined blur must also be active"
        );
        assert_eq!(captured_combined.as_bytes(), old_combined.as_bytes());
        assert_eq!(source.as_bytes(), before);
        assert_eq!(assets.lens_blur_map().unwrap().dimensions(), (8, 8));
        assert_eq!(assets.lens_blur_map().unwrap().as_raw(), &depth_before);
        assert_eq!(assets.fog_map().unwrap().as_raw(), &fog_before);
        assert_eq!(assets.relight_map().unwrap().as_raw(), &normal_before);
        assert_eq!(adjustments, adjustments_before);
    }
    // END B2-COVERAGE-TEST-ADDITIONS

    #[test]
    fn l01_single_patch_literal_and_source_immutable() {
        let image = DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(1, 1, Rgb([0.0; 3])));
        let before = image.as_bytes().to_vec();
        let color = inline(DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
            1,
            1,
            Rgb([255, 0, 0]),
        )));
        let mask = inline(DynamicImage::ImageLuma8(GrayImage::from_pixel(
            1,
            1,
            Luma([128]),
        )));
        let adjustments = json!({"aiPatches":[{"visible":true,"patchData":{"offsetX":0,"offsetY":0,"color":color,"mask":mask,"isSrgbEncoded":true}}]});
        let output = crate::image_loader::composite_patches_on_image(&image, &adjustments)
            .unwrap()
            .to_rgb32f();
        assert_eq!(
            output.get_pixel(0, 0).0.map(f32::to_bits),
            [1056997505, 0, 0]
        );
        assert_eq!(image.as_bytes(), before);
    }

    #[test]
    fn l03_literal_fit_crop_and_actual_width_rounding() {
        for (adjustments, edge, dims, unscaled, scaled) in [
            (json!({}), 6, (6, 4), (0.0, 0.0), (0.0, 0.0)),
            (
                json!({"crop":{"x":2.0,"y":1.0,"width":8.0,"height":6.0}}),
                4,
                (4, 3),
                (2.0, 1.0),
                (1.0, 0.5),
            ),
        ] {
            let original = rgb(12, 8);
            let mut geometry =
                FitGeometry::new(12, 8, &adjustments, FitPreviewSpec::new(edge).unwrap()).unwrap();
            let working = crate::image_processing::downscale_f32_image(
                &original,
                geometry.working_w,
                geometry.working_h,
            );
            geometry.use_working_width(working.width(), 12, &adjustments, edge);
            let (pixels, _) = crate::adjustment_utils::apply_spatial_transformations(
                Cow::Owned(working),
                &geometry.spatial_adjustments,
            );
            assert_eq!(pixels.dimensions(), dims);
            assert_eq!(geometry.scale_for_gpu(pixels.width()), 0.5);
            assert_eq!(geometry.working_scale, 0.5);
            assert_eq!(geometry.unscaled_offset(), unscaled);
            assert_eq!((unscaled.0 * 0.5, unscaled.1 * 0.5), scaled);
        }
        // Odd geometry compares the real resizer's returned width, not planned
        // bounds. Rotation/flips/crop retain the existing spatial implementation.
        let adjustments = json!({"orientationSteps":1,"flipHorizontal":true,"flipVertical":true,"rotation":7.0,"crop":{"x":1.25,"y":2.5,"width":9.0,"height":13.0}});
        let mut geometry =
            FitGeometry::new(17, 11, &adjustments, FitPreviewSpec::new(7).unwrap()).unwrap();
        let working = crate::image_processing::downscale_f32_image(
            &rgb(17, 11),
            geometry.working_w,
            geometry.working_h,
        );
        geometry.use_working_width(working.width(), 17, &adjustments, 7);
        assert_eq!(geometry.working_scale, working.width() as f64 / 17.0);
        assert_eq!(geometry.spatial_adjustments["orientationSteps"], 1);
        assert_eq!(
            geometry.spatial_adjustments["crop"]["x"],
            1.25 * geometry.working_scale
        );
    }

    #[test]
    fn l04_captured_maps_match_actual_effect_kernels() {
        with_single_thread_effect_pool(l04_captured_maps_match_actual_effect_kernels_body);
    }

    fn l04_captured_maps_match_actual_effect_kernels_body() {
        let normal = inline(DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([128, 128, 0, 200]),
        )));
        let depth = inline(DynamicImage::ImageLuma8(GrayImage::from_fn(
            8,
            8,
            |x, y| Luma([((x + y) * 16) as u8]),
        )));
        let adjustments = json!({"relightEnabled":true,"relightNormalMap":normal,"relightAmbient":12.0,"relightShadows":true,"relightLights":[{"type":"directional","visible":true}],"fogEnabled":true,"fogDepthMap":depth,"fogAmount":30.0,"lensBlurEnabled":true,"lensBlurDepthMap":depth,"lensBlurAmount":25.0});
        let assets = capture(&adjustments);
        let source = rgb(24, 16);
        let before = source.as_bytes().to_vec();
        // Separate old/new operations and their combined ordering use the
        // ACTUAL kernels, not a second implementation of the effect formulas.
        let fog = crate::fog::apply_fog(Cow::Borrowed(&source), &adjustments).into_owned();
        let captured_fog = crate::fog::apply_fog_captured(
            Cow::Borrowed(&source),
            assets.adjustments(),
            assets.fog_map().unwrap(),
        )
        .into_owned();
        assert_eq!(fog.as_bytes(), captured_fog.as_bytes());
        let blur =
            crate::lens_blur::apply_lens_blur(Cow::Borrowed(&source), &adjustments).into_owned();
        let captured_blur = crate::lens_blur::apply_lens_blur_captured(
            Cow::Borrowed(&source),
            assets.adjustments(),
            assets.lens_blur_map().unwrap(),
        )
        .into_owned();
        assert_eq!(blur.as_bytes(), captured_blur.as_bytes());
        let old = crate::relight::apply_relight(Cow::Borrowed(&source), &adjustments);
        let old = crate::fog::apply_fog(old, &adjustments);
        let old = crate::lens_blur::apply_lens_blur(old, &adjustments).into_owned();
        let mut scratch = crate::relight::LocalRelightScratch::default();
        let new = crate::relight::apply_relight_captured(
            Cow::Borrowed(&source),
            assets.adjustments(),
            assets.relight_map().unwrap(),
            &mut scratch,
        );
        let new =
            crate::fog::apply_fog_captured(new, assets.adjustments(), assets.fog_map().unwrap());
        let new = crate::lens_blur::apply_lens_blur_captured(
            new,
            assets.adjustments(),
            assets.lens_blur_map().unwrap(),
        )
        .into_owned();
        assert_eq!(old.as_bytes(), new.as_bytes());
        assert_eq!(source.as_bytes(), before);
    }

    fn mask(id: &str, kind: &str, visible: bool) -> crate::mask_generation::MaskDefinition {
        serde_json::from_value(json!({"id":id,"name":id,"visible":true,"invert":false,"opacity":100.0,"adjustments":{},"subMasks":[{"id":"child","type":kind,"visible":visible,"invert":false,"opacity":50.0,"mode":"additive","parameters":{}}]})).unwrap()
    }
    #[test]
    fn l05_checked_mask_combines_and_rejects_unresolved_visible() {
        let resolved = mask("resolved", "all", true);
        let old =
            crate::mask_generation::generate_mask_bitmap(&resolved, 3, 2, 1.0, (0.0, 0.0), None)
                .unwrap();
        let new = generate_mask_bitmap_checked(&resolved, 3, 2, 1.0, (0.0, 0.0), None)
            .unwrap()
            .unwrap();
        assert_eq!(old.as_raw(), new.as_raw());
        assert_eq!(new.as_raw(), &[127; 6]);
        for kind in ["unknown", "ai-subject", "color", "luminance"] {
            let invalid = mask("bad", kind, true);
            assert!(matches!(
                generate_mask_bitmap_checked(&invalid, 3, 2, 1.0, (0.0, 0.0), None),
                Err(CheckedMaskError::UnresolvedVisibleSubmask { .. })
            ));
            let hidden = mask("hidden", kind, false);
            assert_eq!(
                generate_mask_bitmap_checked(&hidden, 3, 2, 1.0, (0.0, 0.0), None)
                    .unwrap()
                    .unwrap()
                    .as_raw(),
                &[0; 6]
            );
        }
    }

    #[test]
    fn l05_colour_reference_is_full_unpatched_and_mask_tone_only() {
        let owner = crate::image_loader::composition_test_owner();
        let base = owner
            .render_unpatched_base(crate::image_loader::AppearancePass::AccurateOne, None)
            .unwrap();
        let original = base.pixels().as_bytes().to_vec();
        let mut adjustments = json!({"masks":[{"id":"colour","name":"colour","visible":true,"invert":false,"opacity":100.0,"adjustments":{},"subMasks":[{"id":"pick","type":"color","visible":true,"invert":false,"opacity":100.0,"mode":"additive","parameters":{"targetX":0.0,"targetY":0.0}}]}]});
        let first = mask_reference(&base, &adjustments, &None).unwrap();
        adjustments["aiPatches"] = json!([{"id":"red","name":"red","visible":true,"patchData":{"offsetX":0,"offsetY":0,"color":inline(DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1,1,Rgb([255,0,0])))),"mask":inline(DynamicImage::ImageLuma8(GrayImage::from_pixel(1,1,Luma([255])))),"isSrgbEncoded":true}}]);
        let assets = capture(&adjustments);
        let second = mask_reference(&base, assets.adjustments(), &None).unwrap();
        assert_eq!(first.dimensions(), base.pixels().dimensions());
        assert_eq!(first.as_bytes(), second.as_bytes());
        let patched =
            crate::image_loader::composite_patches_on_image(base.pixels(), assets.adjustments())
                .unwrap();
        let wrong =
            crate::mask_generation::build_full_warped_image(&patched, true, assets.adjustments());
        assert_ne!(second.as_bytes(), wrong.as_bytes());
        let fit = compose_fit(&base, &assets, FitPreviewSpec::new(32).unwrap(), None).unwrap();
        assert_eq!(fit.request().mask_bitmaps.len(), 1);
        assert_eq!(fit.request().adjustments.mask_count, 1);
        assert_eq!(base.pixels().as_bytes(), original);
    }

    #[test]
    fn l06_cancellation_wins_stage_errors_and_discards_results() {
        let tracker = Arc::new(AtomicUsize::new(1));
        let cancel = Some((Arc::clone(&tracker), 1));
        assert!(matches!(
            stage::<(), _>(&cancel, "controlled stage", || {
                tracker.store(2, Ordering::SeqCst);
                Err("stage also failed")
            }),
            Err(ComposeError::Cancelled)
        ));
        let mut entered = false;
        assert!(matches!(
            stage::<(), &str>(&cancel, "must not run", || {
                entered = true;
                Ok(())
            }),
            Err(ComposeError::Cancelled)
        ));
        assert!(!entered);
    }

    #[test]
    fn l07_actual_owned_fit_request_and_32_order() {
        assert!(FitPreviewSpec::new(0).is_err());
        assert!(FitPreviewSpec::new(1921).is_err());
        let owner = crate::image_loader::composition_test_owner();
        let base = owner
            .render_unpatched_base(crate::image_loader::AppearancePass::AccurateOne, None)
            .unwrap();
        let original = base.pixels().as_bytes().to_vec();
        let masks: Vec<_> = (0..32).map(|i| json!({"id":format!("m{i}"),"name":"m","visible":true,"invert":false,"opacity":100.0,"adjustments":{"exposure":i as f64 / 10.0},"subMasks":[{"id":"all","type":"all","visible":true,"invert":false,"opacity":i as f64 + 1.0,"mode":"additive","parameters":{}}]})).collect();
        let assets = capture(&json!({"masks":masks}));
        let fit = compose_fit(&base, &assets, FitPreviewSpec::new(16).unwrap(), None).unwrap();
        let request = fit.request();
        assert_eq!(request.adjustments.mask_count, 32);
        assert_eq!(request.adjustments.global.has_dcp, 0);
        assert!(request.dcp_textures.is_none());
        assert!(request.roi.is_none());
        assert!(
            request
                .mask_bitmaps
                .windows(2)
                .all(|m| m[0].get_pixel(0, 0)[0] < m[1].get_pixel(0, 0)[0])
        );
        assert_eq!(base.pixels().as_bytes(), original);
        let mut too_many = serde_json::to_value(assets.masks())
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        too_many.push(too_many[0].clone());
        assert!(
            crate::open_creative_snapshot::capture_creative_assets(
                &json!({"masks":too_many}),
                &crate::app_settings::AppSettings::default(),
                None
            )
            .is_err()
        );
    }
}

/// Uncropped editor reference preserves its historical effects -> resize ->
/// lens warp -> coarse rotation/flip order. It is separate from fit/full display
/// composition; all assets and the reference still belong to this exact request.
pub(crate) fn compose_uncropped_accuracy<'a, 'owner>(
    base: &'a PreparedAppearanceBase<'owner>,
    assets: &'a CapturedCreativeAssets,
    cancel: RawCancel,
) -> Result<ComposedFit<'a, 'owner>, ComposeError> {
    check_cancel(&cancel)?;
    let adjustments = assets.adjustments();
    let patched = stage(&cancel, "patches", || {
        crate::image_loader::composite_patches_on_image(base.pixels(), adjustments)
    })?;
    let mut scratch = crate::relight::LocalRelightScratch::default();
    let relit = stage(&cancel, "relight", || {
        Ok::<_, std::convert::Infallible>(match assets.relight_map() {
            Some(map) => crate::relight::apply_relight_captured(
                Cow::Owned(patched),
                adjustments,
                map,
                &mut scratch,
            )
            .into_owned(),
            None => patched,
        })
    })?;
    drop(scratch);
    let fogged = stage(&cancel, "fog", || {
        Ok::<_, std::convert::Infallible>(match assets.fog_map() {
            Some(map) => {
                crate::fog::apply_fog_captured(Cow::Owned(relit), adjustments, map).into_owned()
            }
            None => relit,
        })
    })?;
    let blurred = stage(&cancel, "lens blur", || {
        Ok::<_, std::convert::Infallible>(match assets.lens_blur_map() {
            Some(map) => {
                crate::lens_blur::apply_lens_blur_captured(Cow::Owned(fogged), adjustments, map)
                    .into_owned()
            }
            None => fogged,
        })
    })?;
    let edge = assets
        .settings()
        .editor_preview_resolution
        .filter(|n| *n > 0)
        .unwrap_or(1920);
    let resized = stage(&cancel, "uncropped resize", || {
        Ok::<_, std::convert::Infallible>(crate::image_processing::downscale_f32_image(
            &blurred, edge, edge,
        ))
    })?;
    let scale = resized.width() as f32 / base.pixels().width().max(1) as f32;
    let mut params = crate::image_processing::get_geometry_params_from_json(adjustments);
    params.lens_vignette_amount *= 0.4;
    let warped = stage(&cancel, "uncropped warp", || {
        Ok::<_, std::convert::Infallible>(crate::image_processing::warp_image_geometry(
            &resized, params,
        ))
    })?;
    let pixels = stage(&cancel, "uncropped orientation", || {
        Ok::<_, std::convert::Infallible>(
            crate::image_processing::apply_flip(
                crate::image_processing::apply_coarse_rotation(
                    Cow::Owned(warped),
                    adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8,
                ),
                adjustments["flipHorizontal"].as_bool().unwrap_or(false),
                adjustments["flipVertical"].as_bool().unwrap_or(false),
            )
            .into_owned(),
        )
    })?;
    let reference = if assets.masks().iter().any(|m| m.requires_warped_image()) {
        Some(mask_reference(base, adjustments, &cancel)?)
    } else {
        None
    };
    let mut masks = Vec::with_capacity(assets.masks().len());
    for mask in assets.masks() {
        check_cancel(&cancel)?;
        let result = generate_mask_bitmap_checked(
            mask,
            pixels.width(),
            pixels.height(),
            scale,
            (0.0, 0.0),
            reference.as_ref(),
        );
        check_cancel(&cancel)?;
        masks.push(
            result
                .map_err(ComposeError::Mask)?
                .ok_or(ComposeError::MaskOrderMismatch)?,
        );
    }
    let render = crate::image_processing::resolve_render_options(assets.settings(), true);
    let mut uniforms = crate::image_processing::get_all_adjustments_from_json(
        adjustments,
        true,
        base.as_shot(),
        render,
    );
    uniforms.global.show_clipping = 0;
    if uniforms.global.has_dcp != 0 {
        return Err(ComposeError::ProfileAlreadyApplied);
    }
    if uniforms.mask_count as usize != masks.len() {
        return Err(ComposeError::MaskOrderMismatch);
    }
    check_cancel(&cancel)?;
    Ok(ComposedFit {
        pixels,
        masks,
        uniforms,
        lut: assets.lut().cloned(),
        as_shot: base.as_shot(),
        kind: base.kind(),
        scale_for_gpu: scale,
        unscaled_crop_offset: (0.0, 0.0),
        _base: base,
        _assets: assets,
    })
}

#[cfg(test)]
mod preview_dependency_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preview_dependency_schema_and_cpu_read_sweep() {
        // The real current frontend declaration, not a replica fixture/schema.
        let schema = include_str!("../../src/utils/adjustments.ts");
        let body = schema
            .split("export interface Adjustments {")
            .nth(1)
            .unwrap()
            .split("\n}\n")
            .next()
            .unwrap();
        let mut fields = 0;
        for line in body.lines() {
            if !line.starts_with("  ") || line.starts_with("   ") {
                continue;
            }
            let Some((name, _)) = line.trim().split_once(':') else {
                continue;
            };
            let name = name.trim_end_matches('?');
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            assert_ne!(
                preview_input_role(name),
                PreviewInputRole::Unclassified,
                "unclassified schema field: {name}"
            );
            fields += 1;
        }
        assert!(fields >= 100); // A broken extraction cannot silently pass an empty sweep.
        for key in crate::cache_utils::GEOMETRY_KEYS {
            assert_eq!(preview_input_role(key), PreviewInputRole::Cpu, "{key}");
        }
        // Guard literal top-level CPU reads in the real production helpers.
        // Nested patch/mask/light payloads remain whole CPU inputs and uncached.
        let sources = [
            include_str!("adjustment_utils.rs")
                .split("pub fn apply_spatial_transformations")
                .nth(1)
                .unwrap()
                .split("\n}\n")
                .next()
                .unwrap(),
            include_str!("image_processing.rs")
                .split("pub fn get_geometry_params_from_json")
                .nth(1)
                .unwrap()
                .split("\n}\n")
                .next()
                .unwrap(),
            include_str!("relight.rs")
                .split("fn apply_relight_with_map")
                .nth(1)
                .unwrap()
                .split("\n}\n")
                .next()
                .unwrap(),
            include_str!("fog.rs")
                .split("fn apply_fog_with_map")
                .nth(1)
                .unwrap()
                .split("\n}\n")
                .next()
                .unwrap(),
            include_str!("lens_blur.rs")
                .split("fn apply_lens_blur_with_map")
                .nth(1)
                .unwrap()
                .split("\n}\n")
                .next()
                .unwrap(),
        ];
        let mut reads = 0;
        for source in sources {
            let compact: String = source.split_whitespace().collect();
            for prefix in ["adjustments[\"", "adjustments.get(\""] {
                for tail in compact.split(prefix).skip(1) {
                    let key = tail.split('"').next().unwrap();
                    assert_eq!(
                        preview_input_role(key),
                        PreviewInputRole::Cpu,
                        "CPU read must be retained: {key}"
                    );
                    reads += 1;
                }
            }
        }
        assert!(reads > 20);
        // Fog's scalar accessor indexes adjustments[key], so check its actual calls too.
        let fog = include_str!("fog.rs")
            .split("fn apply_fog_with_map")
            .nth(1)
            .unwrap()
            .split("\n}\n")
            .next()
            .unwrap();
        for tail in fog.split("get(\"").skip(1) {
            let key = tail.split('"').next().unwrap();
            if key != "effects" {
                assert_eq!(preview_input_role(key), PreviewInputRole::Cpu, "{key}");
            }
        }
    }

    #[test]
    fn preview_projection_preserves_every_cpu_field_and_rejects_unknown() {
        assert!(canonical_preview_divisor(None).unwrap().is_none());
        assert!(canonical_preview_divisor(Some(1.0)).unwrap().is_none());
        let next = f32::from_bits(1.0f32.to_bits() + 1);
        assert_eq!(
            canonical_preview_divisor(Some(next))
                .unwrap()
                .unwrap()
                .to_bits(),
            next.to_bits()
        );
        for invalid in [
            0.0,
            -0.0,
            f32::from_bits(1.0f32.to_bits() - 1),
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert!(matches!(
                canonical_preview_divisor(Some(invalid)),
                Err(ComposeError::InvalidGeometry)
            ));
        }
        for key in PREVIEW_RETAINED_CPU_FIELDS
            .iter()
            .chain(crate::cache_utils::GEOMETRY_KEYS)
        {
            let mut input = json!({});
            input[*key] = json!({"exact":[1,2,3],"nested":{"value":0.123456789}});
            assert_eq!(preview_cpu_inputs(&input).unwrap()[*key], input[*key]);
            let before = preview_cpu_inputs(&input).unwrap();
            input[*key]["exact"][1] = json!(4);
            assert_ne!(before, preview_cpu_inputs(&input).unwrap(), "{key}");
        }
        for key in PREVIEW_FRESH_RENDER_FIELDS {
            let mut input = json!({"crop":{"x":1,"y":2,"width":30,"height":20}});
            let before = preview_cpu_inputs(&input).unwrap();
            input[*key] = json!({"arbitrary":[1,2,3]});
            assert_eq!(before, preview_cpu_inputs(&input).unwrap(), "{key}");
        }
        let base = json!({"sectionVisibility":{"effects":true,"basic":true,"color":true,"curves":true,"details":true}});
        let mut edited = base.clone();
        for key in ["basic", "color", "curves", "details"] {
            edited["sectionVisibility"][key] = json!(false);
            assert_eq!(
                preview_cpu_inputs(&base).unwrap(),
                preview_cpu_inputs(&edited).unwrap()
            );
        }
        edited["sectionVisibility"]["effects"] = json!(false);
        assert_ne!(
            preview_cpu_inputs(&base).unwrap(),
            preview_cpu_inputs(&edited).unwrap()
        );
        assert_eq!(
            preview_cpu_inputs(&json!({"futureCpuInput":1})),
            Err(PreviewProjectionError::Unclassified)
        );
        assert_eq!(
            preview_cpu_inputs(&json!({"sectionVisibility":{"futureSection":true}})),
            Err(PreviewProjectionError::Unclassified)
        );
    }
}

#[cfg(test)]
mod point_colour_projection_tests {
    use super::*;
    #[test]
    fn pc04_point_is_fresh_gpu_input_without_unknown_bypass() {
        let before = serde_json::json!({"crop":{"x":2,"y":3,"width":10,"height":11}});
        let mut after = before.clone();
        after["pointColour"] = serde_json::json!({"v":1,"centerHue":100,"widthDegrees":40,"hue":80,"saturation":0,"luminance":0});
        assert_eq!(
            preview_cpu_inputs(&before).unwrap(),
            preview_cpu_inputs(&after).unwrap()
        );
        let a = crate::image_processing::get_all_adjustments_from_json(
            &before,
            false,
            AsShot::reference(),
            crate::image_processing::RenderOptions::default(),
        );
        let b = crate::image_processing::get_all_adjustments_from_json(
            &after,
            false,
            AsShot::reference(),
            crate::image_processing::RenderOptions::default(),
        );
        assert_ne!(bytemuck::bytes_of(&a), bytemuck::bytes_of(&b));
        after["futureCpuPointColour"] = serde_json::json!(1);
        assert_eq!(
            preview_cpu_inputs(&after),
            Err(PreviewProjectionError::Unclassified)
        );
    }
}
