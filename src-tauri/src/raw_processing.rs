use crate::as_shot::AsShot;
use crate::image_processing::apply_orientation;
use crate::white_balance::WhiteBalance;
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageBuffer, Rgba};
use rawler::{
    decoders::{Decoder, Orientation, RawDecodeParams},
    imgop::{
        develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
        xyz::Illuminant,
    },
    rawimage::{RawImage, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use rayon::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Camera metadata captured from RawImage before it is dropped,
/// needed by the DCP profile stage (§4.1).
#[derive(Clone, Debug)]
pub struct RawImageMeta {
    pub wb_coeffs: [f32; 4],
    pub make: String,
    pub model: String,
    pub clean_model: String,
}

/// Cancellation uses the existing native load generation; no closure is stored.
pub(crate) type RawCancel = Option<(Arc<AtomicUsize>, usize)>;

#[derive(Clone, Copy, Debug)]
pub(crate) enum PreparedQuality {
    CurrentFull,
    LegacyFast,
    AccurateXtrans1 {
        max_workers: usize,
        scratch_budget_bytes: usize,
    },
}

/// One owned decoder result, immutable after capture. No application-level
/// mosaic clone, source-byte borrow or decoder survives in this owner.
pub(crate) struct PreparedRaw {
    input: DevelopmentInput,
    as_shot: AsShot,
}

struct DevelopmentInput {
    raw: RawImage,
    original_whitelevel: rawler::rawimage::WhiteLevel,
    original_wb: [f32; 4],
    orientation: Orientation,
    meta: RawImageMeta,
    to_white_one: f32,
}

struct DevelopmentPolicy {
    developer: RawDevelop,
    is_linear_format: bool,
    apply_ungamma: bool,
    fast_demosaic: bool,
    has_dcp_profile: bool,
}

impl PreparedRaw {
    pub(crate) fn fast_demosaic_scale_factor(
        &self,
        decoded_width: u32,
        decoded_height: u32,
    ) -> f32 {
        let max_orig = (self.input.raw.width as f32).max(self.input.raw.height as f32);
        let max_comp = (decoded_width as f32).max(decoded_height as f32);
        if max_orig > 0.0 {
            let ratio = max_comp / max_orig;
            if ratio > 0.1 && ratio < 0.35 {
                return 0.25;
            } else if (0.35..0.75).contains(&ratio) {
                return 0.5;
            }
        }
        1.0
    }

    pub(crate) fn as_shot(&self) -> AsShot {
        self.as_shot
    }

    pub(crate) fn is_native_xtrans(&self) -> bool {
        is_native_xtrans(&self.input.raw)
    }

    /// Decoded camera identity only for genuine single-plane RGB CFA mosaics
    /// (X-Trans or Bayer). Linear/rendered/4-colour data never qualifies, and
    /// no EXIF, extension or filename fact is consulted.
    pub(crate) fn camera_native_cfa_meta(&self) -> Option<&RawImageMeta> {
        let raw = &self.input.raw;
        (raw.cpp == 1
            && matches!(&raw.photometric, RawPhotometricInterpretation::Cfa(config)
                if config.cfa.is_rgb()))
        .then_some(&self.input.meta)
    }
}

fn check_raw_cancel(cancel: &RawCancel) -> Result<()> {
    if let Some((tracker, generation)) = cancel
        && tracker.load(Ordering::SeqCst) != *generation
    {
        return Err(anyhow!("Load cancelled"));
    }
    Ok(())
}

fn is_native_xtrans(raw: &RawImage) -> bool {
    raw.cpp == 1
        && matches!(&raw.photometric, RawPhotometricInterpretation::Cfa(config)
            if config.cfa.width == 6 && config.cfa.height == 6 && config.cfa.is_rgb())
}

/// Run one decode. Optional extra AsShot capture is exclusive to the new
/// prepared API: old wrappers retain their original metadata/error surface.
fn decode_raw(
    file_bytes: &[u8],
    cancel: &RawCancel,
    capture_as_shot: bool,
) -> Result<(RawImage, Orientation, Option<AsShot>)> {
    check_raw_cancel(cancel)?;
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;
    check_raw_cancel(cancel)?;
    let raw = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;
    let orientation = metadata_orientation(decoder.as_ref(), &source)?;
    let as_shot =
        capture_as_shot.then(|| capture_as_shot_metadata(&raw, decoder.as_ref(), file_bytes));
    Ok((raw, orientation, as_shot))
}

impl DevelopmentInput {
    fn capture(mut raw: RawImage, orientation: Orientation, file_bytes: &[u8]) -> Self {
        let original_whitelevel = raw.whitelevel.clone();
        let original_wb = raw.wb_coeffs;
        let original_white_level =
            raw.whitelevel.0.first().cloned().unwrap_or(u16::MAX as u32) as f32;
        let original_black_level = raw
            .blacklevel
            .levels
            .first()
            .map(|r| r.as_f32())
            .unwrap_or(0.0);
        let denominator = (original_white_level - original_black_level).max(1.0);
        let to_white_one = (u32::MAX as f32 - original_black_level) / denominator;
        for level in raw.whitelevel.0.iter_mut() {
            *level = u32::MAX;
        }
        raw.wb_coeffs =
            crate::multi_exposure::neutralize_wb_if_multiexposure(raw.wb_coeffs, file_bytes);
        let meta = RawImageMeta {
            wb_coeffs: raw.wb_coeffs,
            make: raw.make.clone(),
            model: raw.model.clone(),
            clean_model: raw.clean_model.clone(),
        };
        Self {
            raw,
            original_whitelevel,
            original_wb,
            orientation,
            meta,
            to_white_one,
        }
    }
}

pub(crate) fn prepare_raw(file_bytes: &[u8], cancel: &RawCancel) -> Result<PreparedRaw> {
    let (raw, orientation, as_shot) = decode_raw(file_bytes, cancel, true)?;
    let as_shot = as_shot.ok_or_else(|| anyhow!("Prepared RAW AsShot capture missing"))?;
    let input = DevelopmentInput::capture(raw, orientation, file_bytes);
    check_raw_cancel(cancel)?;
    Ok(PreparedRaw { input, as_shot })
}

// Test-only entry counts per actual development quality: CurrentFull,
// LegacyFast, AccurateXtrans1. Read on the same worker thread that develops.
#[cfg(test)]
thread_local! {
    pub(crate) static PREPARED_DEVELOPMENTS: std::cell::Cell<[usize; 3]> =
        const { std::cell::Cell::new([0; 3]) };
}

/// Full-quality, unoriented development from the same captured decode.
/// No implicit retry to CurrentFull/JPEG on an eligible accurate-path error.
pub(crate) fn develop_prepared_raw(
    prepared: &PreparedRaw,
    quality: PreparedQuality,
    _highlight_compression: f32,
    linear_mode: &str,
    has_dcp_profile: bool,
    cancel: RawCancel,
) -> Result<(DynamicImage, Orientation, RawImageMeta)> {
    #[cfg(test)]
    PREPARED_DEVELOPMENTS.with(|counts| {
        let mut next = counts.get();
        next[match quality {
            PreparedQuality::CurrentFull => 0,
            PreparedQuality::LegacyFast => 1,
            PreparedQuality::AccurateXtrans1 { .. } => 2,
        }] += 1;
        counts.set(next);
    });
    check_raw_cancel(&cancel)?;
    let policy = development_policy(
        &prepared.input.raw,
        matches!(quality, PreparedQuality::LegacyFast),
        linear_mode,
        has_dcp_profile,
    )?;
    let intermediate = develop_captured(&prepared.input, &policy, quality, &cancel)?;
    finish_development(
        intermediate,
        &policy,
        prepared.input.orientation,
        prepared.input.meta.clone(),
        prepared.input.to_white_one,
        &cancel,
    )
}

pub fn develop_raw_image(
    file_bytes: &[u8],
    fast_demosaic: bool,
    highlight_compression: f32,
    linear_mode: String,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let (developed_image, orientation, _meta) = develop_internal(
        file_bytes,
        fast_demosaic,
        highlight_compression,
        linear_mode,
        false, // no DCP profile
        cancel_token,
    )?;
    Ok(apply_orientation(developed_image, orientation))
}

/// Like `develop_raw_image` but returns camera metadata for DCP profile
/// rendering and strips `Calibrate` when a profile is active (§3.1).
#[allow(dead_code)]
pub fn develop_raw_image_with_meta(
    file_bytes: &[u8],
    fast_demosaic: bool,
    highlight_compression: f32,
    linear_mode: String,
    has_dcp_profile: bool,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<(DynamicImage, Orientation, RawImageMeta)> {
    develop_internal(
        file_bytes,
        fast_demosaic,
        highlight_compression,
        linear_mode,
        has_dcp_profile,
        cancel_token,
    )
}

fn metadata_orientation(decoder: &dyn Decoder, source: &RawSource) -> Result<Orientation> {
    let metadata = decoder.raw_metadata(source, &RawDecodeParams::default())?;
    Ok(metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal))
}

pub fn extract_embedded_preview(file_bytes: &[u8]) -> Option<DynamicImage> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let preview = decoder
        .full_image(&source, &RawDecodeParams::default())
        .ok()??;
    let orientation =
        metadata_orientation(decoder.as_ref(), &source).unwrap_or(Orientation::Normal);
    Some(apply_orientation(preview, orientation))
}

fn is_linear_raw_format(raw_image: &RawImage) -> bool {
    matches!(
        raw_image.photometric,
        RawPhotometricInterpretation::LinearRaw
    )
}

#[inline]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(3.0)
    }
}

#[inline]
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn recover_clipped_pixel(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);

    if max_c <= 0.50 {
        return (r, g, b);
    }

    let mut cur_r = r;
    let mut cur_g = g;
    let mut cur_b = b;

    let outer_blend = smootherstep(0.50, 1.5, max_c);

    let magenta = (cur_r.min(cur_b) - cur_g).max(0.0);
    if magenta > 0.0 {
        let target_g = cur_r.min(cur_b) * 0.80 + ((cur_r + cur_b) * 0.5) * 0.20;
        let correction = (target_g - cur_g).max(0.0);
        let magenta_weight = smoothstep(0.0, 0.25, magenta / max_c);
        cur_g += correction * outer_blend * magenta_weight;
    }

    let residual = (cur_r.min(cur_b) - cur_g).max(0.0);
    if residual > 0.0 {
        cur_g += residual * outer_blend;
    }

    let new_max = cur_r.max(cur_g).max(cur_b);
    let min_c = cur_r.min(cur_g).min(cur_b);

    let knee = smoothstep(0.50, 1.5, new_max);

    if knee > 0.0 {
        let neutrality = (min_c / new_max.max(1e-5)).clamp(0.0, 1.0);

        let core_burn = smoothstep(0.60, 3.0, new_max);

        let desat = (knee * (neutrality * 0.85 + core_burn * 0.15)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);

        let neutral_value = min_c + (new_max - min_c) * 1.0;

        cur_r = cur_r * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_g = cur_g * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_b = cur_b * (1.0 - smooth_desat) + neutral_value * smooth_desat;
    }

    (cur_r, cur_g, cur_b)
}

fn develop_internal(
    file_bytes: &[u8],
    fast_demosaic: bool,
    _highlight_compression: f32,
    linear_mode: String,
    has_dcp_profile: bool,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<(DynamicImage, Orientation, RawImageMeta)> {
    let (raw, orientation, _) = decode_raw(file_bytes, &cancel_token, false)?;
    // Preserve policy errors (notably selected-DCP linear rejection) before WB
    // metadata work. White-level replacement does not affect policy decisions.
    let policy = development_policy(&raw, fast_demosaic, &linear_mode, has_dcp_profile)?;
    let input = DevelopmentInput::capture(raw, orientation, file_bytes);
    let developed = develop_captured(&input, &policy, PreparedQuality::CurrentFull, &cancel_token)?;
    let rescale_factor = input.to_white_one;
    let raw_meta = input.meta.clone();
    // Old byte wrappers do not retain their decoded mosaic during conversion.
    drop(input);
    finish_development(
        developed,
        &policy,
        orientation,
        raw_meta,
        rescale_factor,
        &cancel_token,
    )
}

fn development_policy(
    raw_image: &RawImage,
    fast_demosaic: bool,
    linear_mode: &str,
    has_dcp_profile: bool,
) -> Result<DevelopmentPolicy> {
    let is_linear_format = is_linear_raw_format(raw_image);

    let (apply_ungamma, apply_calibration) = match linear_mode {
        "gamma" => (true, true),
        "skip_calib" => (false, false),
        "gamma_skip_calib" => (true, false),
        _ => (false, true),
    };

    // DCP profile replaces the camera calibration step (§3.1).
    let keep_calibrate = keep_calibration(is_linear_format, apply_calibration, has_dcp_profile);

    let mut developer = RawDevelop::default();

    if is_linear_format {
        developer.steps.retain(|&step| {
            step != ProcessingStep::SRgb
                && step != ProcessingStep::Demosaic
                && (keep_calibrate || step != ProcessingStep::Calibrate)
        });
    } else if fast_demosaic {
        developer.demosaic_algorithm = DemosaicAlgorithm::Speed;
        developer.steps.retain(|&step| {
            step != ProcessingStep::SRgb && (keep_calibrate || step != ProcessingStep::Calibrate)
        });
    } else {
        developer.steps.retain(|&step| {
            step != ProcessingStep::SRgb && (keep_calibrate || step != ProcessingStep::Calibrate)
        });
    }

    if has_dcp_profile {
        // DcpRenderer owns both camera calibration and white balance.
        developer
            .steps
            .retain(|step| *step != ProcessingStep::WhiteBalance);
        if is_linear_format {
            return Err(anyhow!(
                "DCP profiles require camera-native RAW data; linear RAW input is not supported"
            ));
        }
    }

    Ok(DevelopmentPolicy {
        developer,
        is_linear_format,
        apply_ungamma,
        fast_demosaic,
        has_dcp_profile,
    })
}

fn develop_captured(
    input: &DevelopmentInput,
    policy: &DevelopmentPolicy,
    quality: PreparedQuality,
    cancel: &RawCancel,
) -> Result<Intermediate> {
    check_raw_cancel(cancel)?;
    match quality {
        PreparedQuality::AccurateXtrans1 {
            max_workers,
            scratch_budget_bytes,
        } if !policy.fast_demosaic && is_native_xtrans(&input.raw) => {
            use rawler::imgop::sensor::xtrans::external::{XTransDemosaicOptions, XTransPasses};
            let cancelled = || {
                cancel.as_ref().is_some_and(|(tracker, generation)| {
                    tracker.load(Ordering::SeqCst) != *generation
                })
            };
            let options = XTransDemosaicOptions::new(
                XTransPasses::One,
                input.to_white_one,
                max_workers,
                scratch_budget_bytes,
                &cancelled,
            )?;
            Ok(policy
                .developer
                .develop_intermediate_with_demosaic(&input.raw, &options)?)
        }
        _ => Ok(policy.developer.develop_intermediate(&input.raw)?),
    }
}

fn finish_development(
    mut developed_intermediate: Intermediate,
    policy: &DevelopmentPolicy,
    orientation: Orientation,
    raw_meta: RawImageMeta,
    rescale_factor: f32,
    cancel: &RawCancel,
) -> Result<(DynamicImage, Orientation, RawImageMeta)> {
    let fast_demosaic = policy.fast_demosaic;
    let is_linear_format = policy.is_linear_format;
    let apply_ungamma = policy.apply_ungamma;
    let has_dcp_profile = policy.has_dcp_profile;
    let safe_highlight_compression = 1000.0;

    let clamp_limit = if fast_demosaic {
        1.0
    } else {
        safe_highlight_compression
    };

    let (width, height) = {
        let dim = developed_intermediate.dim();
        (dim.w as u32, dim.h as u32)
    };

    check_raw_cancel(cancel)?;

    match &mut developed_intermediate {
        Intermediate::Monochrome(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                let mut linear_val = *p * rescale_factor;
                if is_linear_format && apply_ungamma {
                    linear_val = srgb_to_linear(linear_val.max(0.0));
                }
                *p = linear_val.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::ThreeColor(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                let mut r = (p[0] * rescale_factor).max(0.0);
                let mut g = (p[1] * rescale_factor).max(0.0);
                let mut b = (p[2] * rescale_factor).max(0.0);

                if is_linear_format && apply_ungamma {
                    r = srgb_to_linear(r.max(0.0));
                    g = srgb_to_linear(g.max(0.0));
                    b = srgb_to_linear(b.max(0.0));
                }

                let (rec_r, rec_g, rec_b) = if has_dcp_profile {
                    // Upstream highlight recovery assumes calibrated RGB, not sensor channels.
                    (r, g, b)
                } else {
                    recover_clipped_pixel(r, g, b)
                };

                p[0] = rec_r.clamp(0.0, clamp_limit);
                p[1] = rec_g.clamp(0.0, clamp_limit);
                p[2] = rec_b.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::FourColor(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                p.iter_mut().for_each(|c| {
                    let mut linear_val = *c * rescale_factor;
                    if is_linear_format && apply_ungamma {
                        linear_val = srgb_to_linear(linear_val.max(0.0));
                    }
                    *c = linear_val.clamp(0.0, clamp_limit);
                });
            });
        }
    }

    check_raw_cancel(cancel)?;

    let dynamic_image = match developed_intermediate {
        Intermediate::ThreeColor(pixels) => {
            DynamicImage::ImageRgba32F(rgb_pixels_to_rgba(&pixels.data, width, height)?)
        }
        Intermediate::Monochrome(pixels) => {
            DynamicImage::ImageRgba32F(mono_pixels_to_rgba(&pixels.data, width, height)?)
        }
        _ => {
            return Err(anyhow!("Unsupported intermediate format for conversion"));
        }
    };

    Ok((dynamic_image, orientation, raw_meta))
}

pub fn read_as_shot(file_bytes: &[u8]) -> Option<AsShot> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    Some(capture_as_shot_metadata(
        &raw_image,
        decoder.as_ref(),
        file_bytes,
    ))
}

fn capture_as_shot_metadata(
    raw_image: &RawImage,
    decoder: &dyn Decoder,
    file_bytes: &[u8],
) -> AsShot {
    let is_monochrome = raw_image.cpp == 1
        && !matches!(raw_image.photometric, RawPhotometricInterpretation::Cfa(_));

    AsShot {
        white_balance: (!is_monochrome)
            .then(|| as_shot_white_balance(raw_image, file_bytes))
            .flatten()
            .unwrap_or_else(WhiteBalance::reference),
        exposure: crate::baseline_exposure::read(decoder, file_bytes, is_monochrome),
    }
}

fn as_shot_white_balance(raw_image: &RawImage, file_bytes: &[u8]) -> Option<WhiteBalance> {
    let wb_coeffs =
        crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);
    let neutral = if wb_coeffs[0].is_nan() {
        [1.0; 4]
    } else {
        wb_coeffs.map(|c| 1.0 / c)
    };

    let matrices = &raw_image.color_matrix;
    if let (Some(matrix_a), Some(matrix_d65)) =
        (matrices.get(&Illuminant::A), matrices.get(&Illuminant::D65))
    {
        return WhiteBalance::from_dual_illuminant_camera_neutral(matrix_a, matrix_d65, &neutral);
    }

    let color_matrix = matrices
        .get(&Illuminant::D65)
        .or_else(|| matrices.values().next())?;
    WhiteBalance::from_camera_neutral(color_matrix, &neutral)
}

fn rgb_pixels_to_rgba(
    data: &[[f32; 3]],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; checked_rgba_sample_count(data.len(), width, height)?];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, p)| dst.copy_from_slice(&[p[0], p[1], p[2], 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

fn mono_pixels_to_rgba(
    data: &[f32],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; checked_rgba_sample_count(data.len(), width, height)?];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, &p)| dst.copy_from_slice(&[p, p, p, 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

fn checked_rgba_sample_count(pixel_count: usize, width: u32, height: u32) -> Result<usize> {
    let dimension_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| anyhow!("Developed image dimensions overflow"))?;
    if dimension_pixels != pixel_count {
        return Err(anyhow!("Developed image size does not match its dimensions"));
    }
    pixel_count
        .checked_mul(4)
        .ok_or_else(|| anyhow!("Developed RGBA image size overflow"))
}

pub fn get_fast_demosaic_scale_factor(
    file_bytes: &[u8],
    decoded_width: u32,
    decoded_height: u32,
) -> f32 {
    let source = RawSource::new_from_slice(file_bytes);
    if let Ok(decoder) = rawler::get_decoder(&source)
        && let Ok(raw_img) = decoder.raw_image(&source, &RawDecodeParams::default(), true)
    {
        let max_orig = (raw_img.width as f32).max(raw_img.height as f32);
        let max_comp = (decoded_width as f32).max(decoded_height as f32);
        if max_orig > 0.0 {
            let ratio = max_comp / max_orig;
            if ratio > 0.1 && ratio < 0.35 {
                return 0.25;
            } else if (0.35..0.75).contains(&ratio) {
                return 0.5;
            }
        }
    }
    1.0
}

// Linear-RAW mode controls must not change CFA development when no profile is selected.
fn keep_calibration(is_linear: bool, linear_calibration: bool, has_dcp: bool) -> bool {
    !has_dcp && (!is_linear || linear_calibration)
}

#[cfg(test)]
mod integration_tests {
    use super::keep_calibration;
    #[test]
    fn linear_skip_calibration_setting_does_not_change_cfa_rendering() {
        assert!(keep_calibration(false, false, false));
        assert!(!keep_calibration(true, false, false));
        assert!(keep_calibration(true, true, false));
        assert!(!keep_calibration(false, true, true));
    }
}

#[cfg(test)]
mod prepared_tests {
    use super::*;
    use rawler::{
        CFA,
        cfa::PlaneColor,
        decoders::Camera,
        imgop::{Dim2, Point, Rect},
        rawimage::{BlackLevel, CFAConfig, RawImageData, WhiteLevel},
    };

    // Synthetic owned decoder output, not a decoder/real-RAW fixture.
    fn fixture(pattern: &str, width: usize, height: usize) -> RawImage {
        let cfa = CFA::new(pattern);
        let colors = PlaneColor::new("RGB");
        let mut matrix = std::collections::HashMap::new();
        matrix.insert(
            Illuminant::D65,
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        );
        let camera = Camera {
            make: "Synthetic".into(),
            model: "Fixture".into(),
            clean_model: "Fixture".into(),
            cfa: cfa.clone(),
            plane_color: colors.clone(),
            color_matrix: matrix,
            ..Camera::default()
        };
        RawImage::new_with_data(
            camera,
            RawImageData::Float(vec![256.0; width * height]),
            width,
            height,
            1,
            [2.0, 1.0, 4.0, f32::NAN],
            RawPhotometricInterpretation::Cfa(CFAConfig::new(&cfa, &colors)),
            Some(BlackLevel::zero(1, 1, 1)),
            Some(WhiteLevel::new(vec![1024])),
            false,
        )
    }

    fn prepared(raw: RawImage) -> PreparedRaw {
        PreparedRaw {
            input: DevelopmentInput::capture(raw, Orientation::Rotate90, b"synthetic"),
            as_shot: AsShot::reference(),
        }
    }

    fn sample_bits(raw: &RawImage) -> Vec<u32> {
        match &raw.data {
            RawImageData::Float(v) => v.iter().map(|v| v.to_bits()).collect(),
            _ => panic!("synthetic float mosaic required"),
        }
    }

    #[test]
    fn capture_retains_complete_source_metadata_and_moves_the_mosaic() {
        let mut raw = fixture("RGGB", 64, 64);
        raw.whitelevel = WhiteLevel::new(vec![1024, 2048, 4096, 8192]);
        raw.blacklevel = BlackLevel::new(&[64u16, 65, 66, 67], 2, 2, 1);
        raw.active_area = Some(Rect::new(Point::new(2, 3), Dim2::new(60, 58)));
        raw.crop_area = Some(Rect::new(Point::new(4, 5), Dim2::new(54, 52)));
        let address = match &raw.data {
            RawImageData::Float(v) => v.as_ptr(),
            _ => unreachable!(),
        };
        let black = raw.blacklevel.clone();
        let matrix = raw.color_matrix.clone();
        let active = raw.active_area;
        let crop = raw.crop_area;
        let prepared = prepared(raw);
        assert_eq!(
            (
                &prepared.input.raw.blacklevel,
                &prepared.input.original_whitelevel
            ),
            (&black, &WhiteLevel::new(vec![1024, 2048, 4096, 8192]))
        );
        assert_eq!(prepared.input.raw.whitelevel.0, vec![u32::MAX; 4]);
        assert_eq!(
            prepared.input.original_wb.map(f32::to_bits),
            [2.0, 1.0, 4.0, f32::NAN].map(f32::to_bits)
        );
        assert_eq!(
            prepared.input.meta.wb_coeffs.map(f32::to_bits),
            prepared.input.original_wb.map(f32::to_bits)
        );
        assert_eq!(prepared.input.raw.color_matrix, matrix);
        assert_eq!(prepared.input.raw.active_area, active);
        assert_eq!(prepared.input.raw.crop_area, crop);
        assert_eq!(prepared.input.orientation, Orientation::Rotate90);
        assert_eq!(prepared.input.meta.model, "Fixture");
        assert_eq!(
            prepared.input.to_white_one.to_bits(),
            ((u32::MAX as f32 - 64.0) / 960.0).to_bits()
        );
        assert!(!prepared.is_native_xtrans());
        assert_eq!(prepared.as_shot().exposure.dng_baseline, 0.0);
        match &prepared.input.raw.data {
            RawImageData::Float(v) => assert_eq!(v.as_ptr(), address),
            _ => panic!("mosaic changed representation"),
        }
    }

    #[test]
    fn policies_preserve_quality_speed_linear_and_dcp_step_ownership() {
        let mut raw = fixture("RGGB", 64, 64);
        let full = development_policy(&raw, false, "skip_calib", false).unwrap();
        assert_eq!(
            full.developer.demosaic_algorithm,
            DemosaicAlgorithm::Quality
        );
        assert_eq!(
            full.developer.steps,
            vec![
                ProcessingStep::Rescale,
                ProcessingStep::Demosaic,
                ProcessingStep::CropActiveArea,
                ProcessingStep::WhiteBalance,
                ProcessingStep::Calibrate,
                ProcessingStep::CropDefault
            ]
        );
        let fast = development_policy(&raw, true, "auto", false).unwrap();
        assert_eq!(fast.developer.demosaic_algorithm, DemosaicAlgorithm::Speed);
        assert_eq!(fast.developer.steps, full.developer.steps);
        let dcp = development_policy(&raw, false, "auto", true).unwrap();
        assert_eq!(
            dcp.developer.steps,
            vec![
                ProcessingStep::Rescale,
                ProcessingStep::Demosaic,
                ProcessingStep::CropActiveArea,
                ProcessingStep::CropDefault
            ]
        );
        raw.photometric = RawPhotometricInterpretation::LinearRaw;
        let linear = development_policy(&raw, false, "gamma_skip_calib", false).unwrap();
        assert!(linear.apply_ungamma);
        assert_eq!(
            linear.developer.steps,
            vec![
                ProcessingStep::Rescale,
                ProcessingStep::CropActiveArea,
                ProcessingStep::WhiteBalance,
                ProcessingStep::CropDefault
            ]
        );
        assert_eq!(
            development_policy(&raw, false, "auto", true)
                .err()
                .unwrap()
                .to_string(),
            "DCP profiles require camera-native RAW data; linear RAW input is not supported"
        );
    }

    #[test]
    fn repeated_bayer_development_borrows_source_and_ignores_xtrans_only_budget() {
        let prepared = prepared(fixture("RGGB", 64, 64));
        let before = sample_bits(&prepared.input.raw);
        let (current, orientation, meta) = develop_prepared_raw(
            &prepared,
            PreparedQuality::CurrentFull,
            2.5,
            "auto",
            false,
            None,
        )
        .unwrap();
        let (other, other_orientation, other_meta) = develop_prepared_raw(
            &prepared,
            PreparedQuality::AccurateXtrans1 {
                max_workers: 0,
                scratch_budget_bytes: 0,
            },
            2.5,
            "auto",
            false,
            None,
        )
        .unwrap();
        assert_eq!((current.width(), current.height()), (64, 64));
        assert_eq!(orientation, Orientation::Rotate90);
        assert_eq!(other_orientation, orientation);
        assert_eq!(meta.model, other_meta.model);
        assert_eq!(
            current
                .to_rgba32f()
                .as_raw()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            other
                .to_rgba32f()
                .as_raw()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(sample_bits(&prepared.input.raw), before);
        assert_eq!(prepared.input.original_whitelevel.0, vec![1024]);
    }

    #[test]
    fn eligible_accurate_failures_do_not_silently_develop_current_quality() {
        let mut raw = fixture("RBGBRGGGRGGBGGBGGRBRGRBGGGBGGRGGRGGB", 512, 512);
        raw.color_matrix.clear();
        let prepared = prepared(raw);
        assert!(prepared.is_native_xtrans());
        let error = develop_prepared_raw(
            &prepared,
            PreparedQuality::AccurateXtrans1 {
                max_workers: 0,
                scratch_budget_bytes: 0,
            },
            2.5,
            "auto",
            false,
            None,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("ScratchBudget"));
        let error = develop_prepared_raw(
            &prepared,
            PreparedQuality::AccurateXtrans1 {
                max_workers: 1,
                scratch_budget_bytes: rawler::imgop::sensor::xtrans::external::XTransPasses::One
                    .scratch_bytes(),
            },
            2.5,
            "auto",
            false,
            None,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("InvalidMetric"));
    }

    #[test]
    fn accurate_camera_output_undoes_interpolation_wb_before_profile_stage() {
        let mut raw = fixture("RBGBRGGGRGGBGGBGGRBRGRBGGGBGGRGGRGGB", 512, 512);
        let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
            unreachable!()
        };
        raw.data = RawImageData::Float(
            (0..512 * 512)
                .map(|i| [128.0, 256.0, 512.0][config.cfa.color_at(i / 512, i % 512)])
                .collect(),
        );
        let prepared = prepared(raw);
        let (image, orientation, meta) = develop_prepared_raw(
            &prepared,
            PreparedQuality::AccurateXtrans1 {
                max_workers: 1,
                scratch_budget_bytes: rawler::imgop::sensor::xtrans::external::XTransPasses::One
                    .scratch_bytes(),
            },
            2.5,
            "auto",
            true,
            None,
        )
        .unwrap();
        assert_eq!(orientation, Orientation::Rotate90);
        assert_eq!(&meta.wb_coeffs[..3], &[2.0, 1.0, 4.0]);
        assert_eq!((image.width(), image.height()), (512, 512));
        for pixel in image.to_rgb32f().pixels() {
            for (actual, expected) in pixel.0.iter().zip([0.125, 0.25, 0.5]) {
                assert!((*actual - expected).abs() <= 1e-5);
            }
        }
        assert_eq!(prepared.input.original_whitelevel.0, vec![1024]);
    }

    #[test]
    fn final_conversion_keeps_existing_full_hdr_and_fast_clamps() {
        let raw = fixture("RGGB", 64, 64);
        for (fast, expected) in [
            (false, vec![0.0, 0.5, 4.0, 1000.0]),
            (true, vec![0.0, 0.5, 1.0, 1.0]),
        ] {
            let policy = development_policy(&raw, fast, "auto", false).unwrap();
            let pixels = rawler::pixarray::PixF32::new_with(vec![-1.0, 0.25, 2.0, 600.0], 4, 1);
            let meta = RawImageMeta {
                wb_coeffs: raw.wb_coeffs,
                make: raw.make.clone(),
                model: raw.model.clone(),
                clean_model: raw.clean_model.clone(),
            };
            let (out, _, _) = finish_development(
                Intermediate::Monochrome(pixels),
                &policy,
                Orientation::Normal,
                meta,
                2.0,
                &None,
            )
            .unwrap();
            assert_eq!(
                out.to_rgba32f().pixels().map(|p| p[0]).collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn parallel_rgb_finish_matches_original_serial_pixel_order() {
        let raw = fixture("RGGB", 64, 64);
        let source = vec![
            [-0.0, 0.25, 0.75],
            [0.9, 0.1, 0.9],
            [1.7, 1.6, 0.1],
            [f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
        ];

        for has_dcp_profile in [false, true] {
            let policy = development_policy(&raw, false, "auto", has_dcp_profile).unwrap();
            let mut serial = source.clone();
            for pixel in &mut serial {
                let mut r = (pixel[0] * 2.0).max(0.0);
                let mut g = (pixel[1] * 2.0).max(0.0);
                let mut b = (pixel[2] * 2.0).max(0.0);
                if policy.is_linear_format && policy.apply_ungamma {
                    r = srgb_to_linear(r.max(0.0));
                    g = srgb_to_linear(g.max(0.0));
                    b = srgb_to_linear(b.max(0.0));
                }
                let (r, g, b) = if has_dcp_profile {
                    (r, g, b)
                } else {
                    recover_clipped_pixel(r, g, b)
                };
                pixel[0] = r.clamp(0.0, 1000.0);
                pixel[1] = g.clamp(0.0, 1000.0);
                pixel[2] = b.clamp(0.0, 1000.0);
            }
            let expected: Vec<u32> = serial
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 1.0])
                .map(f32::to_bits)
                .collect();
            let meta = RawImageMeta {
                wb_coeffs: raw.wb_coeffs,
                make: raw.make.clone(),
                model: raw.model.clone(),
                clean_model: raw.clean_model.clone(),
            };
            let pixels = rawler::pixarray::Color2D::new_with(source.clone(), 4, 1);
            let (actual, _, _) = finish_development(
                Intermediate::ThreeColor(pixels),
                &policy,
                Orientation::Normal,
                meta,
                2.0,
                &None,
            )
            .unwrap();
            let actual: Vec<u32> = actual
                .to_rgba32f()
                .into_raw()
                .into_iter()
                .map(f32::to_bits)
                .collect();
            assert_eq!(actual, expected, "has_dcp_profile={has_dcp_profile}");
        }
    }

    #[test]
    fn parallel_rgb_and_mono_rgba_conversions_match_serial_bits() {
        let rgb = vec![
            [f32::NEG_INFINITY, -0.0, f32::NAN],
            [65504.0, 70000.0, 1.0e-8],
        ];
        let expected_rgb = ImageBuffer::<Rgba<f32>, _>::from_fn(2, 1, |x, _| {
            let p = rgb[x as usize];
            Rgba([p[0], p[1], p[2], 1.0])
        });
        let actual_rgb = rgb_pixels_to_rgba(&rgb, 2, 1).unwrap();
        let bits = |values: &[f32]| values.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(expected_rgb.as_raw()), bits(actual_rgb.as_raw()));

        let mono = vec![f32::NAN, -0.0, f32::INFINITY, 70000.0];
        let expected_mono = ImageBuffer::<Rgba<f32>, _>::from_fn(2, 2, |x, y| {
            let p = mono[(y * 2 + x) as usize];
            Rgba([p, p, p, 1.0])
        });
        let actual_mono = mono_pixels_to_rgba(&mono, 2, 2).unwrap();
        assert_eq!(bits(expected_mono.as_raw()), bits(actual_mono.as_raw()));

        assert!(rgb_pixels_to_rgba(&rgb, 3, 1).is_err());
        assert!(mono_pixels_to_rgba(&mono, 1, 1).is_err());
    }

    #[test]
    fn cancellation_precedes_decode_and_prepared_dispatch() {
        let cancel = Some((Arc::new(AtomicUsize::new(2)), 1));
        assert_eq!(
            prepare_raw(b"invalid", &cancel).err().unwrap().to_string(),
            "Load cancelled"
        );
        let prepared = prepared(fixture("RGGB", 64, 64));
        assert_eq!(
            develop_prepared_raw(
                &prepared,
                PreparedQuality::CurrentFull,
                2.5,
                "auto",
                false,
                cancel
            )
            .err()
            .unwrap()
            .to_string(),
            "Load cancelled"
        );
    }
}
