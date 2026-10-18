use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{
    DynamicImage, GenericImageView, GrayImage, ImageBuffer, ImageEncoder, ImageFormat, Luma,
    imageops,
};
use jxl_encoder::{
    LosslessConfig, LossyConfig, PixelLayout,
    api::{calibrated_jxl_quality, quality_to_distance},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;
use tauri::Manager;

use crate::AppState;
use crate::baseline_exposure::BaselineOptions;
use crate::exif_processing;
use crate::file_management::{
    generate_filename_from_template, parse_virtual_path, read_file_mapped,
};
use crate::formats::is_raw_file;
use crate::image_loader::{
    composite_patches_on_image, load_and_composite_with_profiles, load_base_image_from_bytes,
};
use crate::image_processing::{
    AllAdjustments, Crop, GpuContext, RenderOptions, RenderOutputPrecision, RenderRequest,
    downscale_f32_image, get_all_adjustments_from_json, get_or_init_gpu_context,
    process_and_get_dynamic_image, process_and_get_dynamic_image_with_precision,
    resolve_render_options_from_handle,
};
use crate::lut_processing::{
    convert_image_to_cube_lut, generate_identity_lut_image, get_or_load_lut,
};
use crate::mask_generation::{MaskDefinition, build_warped_image_for_masks, generate_mask_bitmap};

use crate::as_shot::as_shot;
use crate::cache_utils::{calculate_full_job_hash, calculate_transform_hash};
use crate::{
    apply_all_transformations, generate_transformed_preview, get_cached_or_generate_mask,
    hydrate_adjustments, load_settings,
};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(try_from = "u8", into = "u8")]
pub enum TiffBitDepth {
    Eight = 8,
    #[default]
    Sixteen = 16,
}

impl TryFrom<u8> for TiffBitDepth {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            8 => Ok(Self::Eight),
            16 => Ok(Self::Sixteen),
            _ => Err(format!(
                "Invalid TIFF bit depth '{}'; expected 8 or 16.",
                value
            )),
        }
    }
}

impl From<TiffBitDepth> for u8 {
    fn from(value: TiffBitDepth) -> Self {
        value as u8
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    LongEdge,
    ShortEdge,
    Width,
    Height,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResizeOptions {
    pub mode: ResizeMode,
    pub value: u32,
    pub dont_enlarge: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::enum_variant_names)]
pub enum BorderBasis {
    #[default]
    LongEdge,
    ShortEdge,
    EachEdge,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BorderOptions {
    #[serde(default)]
    pub basis: BorderBasis,
    pub horizontal_percent: f32,
    pub vertical_percent: f32,
    pub color: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PadOptions {
    pub ratio_width: f32,
    pub ratio_height: f32,
    pub color: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub jpeg_quality: u8,
    #[serde(default)]
    pub tiff_bit_depth: TiffBitDepth,
    pub resize: Option<ResizeOptions>,
    #[serde(default)]
    pub border: Option<BorderOptions>,
    #[serde(default)]
    pub pad: Option<PadOptions>,
    pub keep_metadata: bool,
    #[serde(default)]
    pub preserve_timestamps: bool,
    pub strip_gps: bool,
    pub filename_template: Option<String>,
    pub watermark: Option<WatermarkSettings>,
    #[serde(default)]
    pub export_masks: bool,
    #[serde(default)]
    pub preserve_folders: bool,
    #[serde(default)]
    pub destination_type: Option<String>,
    #[serde(default)]
    pub subfolder: Option<String>,
}

#[derive(Clone)]
pub(crate) enum ExportAdjustmentsMode {
    UseSidecars {
        active_path: Option<String>,
        active_adjustments: Option<Value>,
    },
    GlobalOverride(Value),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkSettings {
    pub path: String,
    pub anchor: WatermarkAnchor,
    pub scale: f32,
    pub spacing: f32,
    pub opacity: f32,
}

fn apply_watermark(
    base_image: &mut DynamicImage,
    watermark_settings: &WatermarkSettings,
) -> Result<(), String> {
    let watermark_img = image::open(&watermark_settings.path)
        .map_err(|e| format!("Failed to open watermark image: {}", e))?;

    let (base_w, base_h) = base_image.dimensions();
    let base_min_dim = base_w.min(base_h) as f32;

    let watermark_scale_factor =
        (base_min_dim * (watermark_settings.scale / 100.0)) / watermark_img.width().max(1) as f32;
    let new_wm_w = (watermark_img.width() as f32 * watermark_scale_factor).round() as u32;
    let new_wm_h = (watermark_img.height() as f32 * watermark_scale_factor).round() as u32;

    if new_wm_w == 0 || new_wm_h == 0 {
        return Ok(());
    }

    let scaled_watermark =
        watermark_img.resize_exact(new_wm_w, new_wm_h, image::imageops::FilterType::Lanczos3);
    let opacity_factor = (watermark_settings.opacity / 100.0).clamp(0.0, 1.0);

    let spacing_pixels = (base_min_dim * (watermark_settings.spacing / 100.0)) as i64;
    let (wm_w, wm_h) = scaled_watermark.dimensions();

    let x = match watermark_settings.anchor {
        WatermarkAnchor::TopLeft | WatermarkAnchor::CenterLeft | WatermarkAnchor::BottomLeft => {
            spacing_pixels
        }
        WatermarkAnchor::TopCenter | WatermarkAnchor::Center | WatermarkAnchor::BottomCenter => {
            (base_w as i64 - wm_w as i64) / 2
        }
        WatermarkAnchor::TopRight | WatermarkAnchor::CenterRight | WatermarkAnchor::BottomRight => {
            base_w as i64 - wm_w as i64 - spacing_pixels
        }
    };

    let y = match watermark_settings.anchor {
        WatermarkAnchor::TopLeft | WatermarkAnchor::TopCenter | WatermarkAnchor::TopRight => {
            spacing_pixels
        }
        WatermarkAnchor::CenterLeft | WatermarkAnchor::Center | WatermarkAnchor::CenterRight => {
            (base_h as i64 - wm_h as i64) / 2
        }
        WatermarkAnchor::BottomLeft
        | WatermarkAnchor::BottomCenter
        | WatermarkAnchor::BottomRight => base_h as i64 - wm_h as i64 - spacing_pixels,
    };

    if matches!(
        base_image,
        DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_)
    ) {
        let mut base_rgba = base_image.to_rgba16();
        let mut watermark_rgba = scaled_watermark.to_rgba16();
        for pixel in watermark_rgba.pixels_mut() {
            pixel[3] = (pixel[3] as f32 * opacity_factor).round() as u16;
        }
        image::imageops::overlay(&mut base_rgba, &watermark_rgba, x, y);
        *base_image = DynamicImage::ImageRgba16(base_rgba);
    } else if matches!(
        base_image,
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_)
    ) {
        let mut base_rgba = base_image.to_rgba32f();
        let mut watermark_rgba = scaled_watermark.to_rgba32f();
        for pixel in watermark_rgba.pixels_mut() {
            pixel[3] *= opacity_factor;
        }
        image::imageops::overlay(&mut base_rgba, &watermark_rgba, x, y);
        *base_image = DynamicImage::ImageRgba32F(base_rgba);
    } else {
        let mut base_rgba = base_image.to_rgba8();
        let mut watermark_rgba = scaled_watermark.to_rgba8();
        for pixel in watermark_rgba.pixels_mut() {
            pixel[3] = (pixel[3] as f32 * opacity_factor).round() as u8;
        }
        image::imageops::overlay(&mut base_rgba, &watermark_rgba, x, y);
        *base_image = DynamicImage::ImageRgba8(base_rgba);
    }

    Ok(())
}

const PAD_RATIO_EPSILON: f64 = 0.001;
const MAX_PAD_DIMENSION: u32 = 65_535;
const MAX_BORDER_PERCENT: f64 = 100.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExportGeometry {
    pub canvas_w: u32,
    pub canvas_h: u32,
    pub photo_w: u32,
    pub photo_h: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub border_x: u32,
    pub border_y: u32,
}

impl ExportGeometry {
    fn has_border(&self) -> bool {
        self.border_x > 0 || self.border_y > 0
    }

    fn is_identity(&self) -> bool {
        self.canvas_w == self.photo_w && self.canvas_h == self.photo_h
    }
}

fn calculate_resize_target(
    current_w: u32,
    current_h: u32,
    resize_opts: &ResizeOptions,
) -> (u32, u32) {
    if resize_opts.dont_enlarge {
        let exceeds = match resize_opts.mode {
            ResizeMode::LongEdge => current_w.max(current_h) > resize_opts.value,
            ResizeMode::ShortEdge => current_w.min(current_h) > resize_opts.value,
            ResizeMode::Width => current_w > resize_opts.value,
            ResizeMode::Height => current_h > resize_opts.value,
        };
        if !exceeds {
            return (current_w, current_h);
        }
    }

    let fix_width = match resize_opts.mode {
        ResizeMode::LongEdge => current_w >= current_h,
        ResizeMode::ShortEdge => current_w <= current_h,
        ResizeMode::Width => true,
        ResizeMode::Height => false,
    };

    let value = resize_opts.value;
    if fix_width {
        let h = (value as f32 * (current_h as f32 / current_w as f32)).round() as u32;
        (value, h)
    } else {
        let w = (value as f32 * (current_w as f32 / current_h as f32)).round() as u32;
        (w, value)
    }
}

fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let raw = hex.trim().trim_start_matches('#');
    let normalized: String = match raw.len() {
        3 => raw.chars().flat_map(|c| [c, c]).collect(),
        6 => raw.to_string(),
        8 => raw[..6].to_string(),
        _ => return None,
    };
    Some([
        u8::from_str_radix(&normalized[0..2], 16).ok()?,
        u8::from_str_radix(&normalized[2..4], 16).ok()?,
        u8::from_str_radix(&normalized[4..6], 16).ok()?,
    ])
}

fn fill_color(hex: Option<&str>) -> [u8; 3] {
    hex.and_then(parse_hex_color).unwrap_or([0, 0, 0])
}

fn resize_target(width: u32, height: u32, resize_opts: Option<&ResizeOptions>) -> (u32, u32) {
    match resize_opts {
        Some(opts) => calculate_resize_target(width, height, opts),
        None => (width, height),
    }
}

fn bordered_size(width: u32, height: u32, border_x: u32, border_y: u32) -> (u32, u32) {
    (
        width.saturating_add(border_x.saturating_mul(2)),
        height.saturating_add(border_y.saturating_mul(2)),
    )
}

fn exceeds_max_dimension(label: &str, width: u32, height: u32) -> bool {
    let exceeds = width > MAX_PAD_DIMENSION || height > MAX_PAD_DIMENSION;
    if exceeds {
        log::warn!(
            "{} {}x{} exceeds the maximum supported dimension; skipping.",
            label,
            width,
            height
        );
    }
    exceeds
}

fn calculate_pad_target(current_w: u32, current_h: u32, pad: &PadOptions) -> Option<(u32, u32)> {
    let ratio_w = pad.ratio_width as f64;
    let ratio_h = pad.ratio_height as f64;
    if !ratio_w.is_finite() || !ratio_h.is_finite() || ratio_w <= 0.0 || ratio_h <= 0.0 {
        return None;
    }

    let target_ratio = ratio_w / ratio_h;
    let original_ratio = current_w as f64 / current_h as f64;
    if (original_ratio - target_ratio).abs() < PAD_RATIO_EPSILON {
        return None;
    }

    let (new_w, new_h) = if original_ratio > target_ratio {
        (current_w, (current_w as f64 / target_ratio).round() as u32)
    } else {
        ((current_h as f64 * target_ratio).round() as u32, current_h)
    };

    let new_w = new_w.max(current_w);
    let new_h = new_h.max(current_h);

    if exceeds_max_dimension("Pad target", new_w, new_h)
        || (new_w == current_w && new_h == current_h)
    {
        return None;
    }

    Some((new_w, new_h))
}

fn calculate_border_thickness(
    src_w: u32,
    src_h: u32,
    border: &BorderOptions,
) -> Option<(u32, u32)> {
    let horizontal = border.horizontal_percent as f64;
    let vertical = border.vertical_percent as f64;
    if !horizontal.is_finite() || !vertical.is_finite() {
        return None;
    }

    if horizontal < 0.0
        || vertical < 0.0
        || horizontal > MAX_BORDER_PERCENT
        || vertical > MAX_BORDER_PERCENT
    {
        log::warn!(
            "Border percentages {}/{} are outside 0..={}; skipping border.",
            horizontal,
            vertical,
            MAX_BORDER_PERCENT
        );
        return None;
    }

    let (basis_x, basis_y) = match border.basis {
        BorderBasis::LongEdge => {
            let edge = src_w.max(src_h) as f64;
            (edge, edge)
        }
        BorderBasis::ShortEdge => {
            let edge = src_w.min(src_h) as f64;
            (edge, edge)
        }
        BorderBasis::EachEdge => (src_w as f64, src_h as f64),
    };

    let border_x = (basis_x * horizontal / 100.0).round() as u32;
    let border_y = (basis_y * vertical / 100.0).round() as u32;

    if border_x == 0 && border_y == 0 {
        return None;
    }

    let (bordered_w, bordered_h) = bordered_size(src_w, src_h, border_x, border_y);
    if exceeds_max_dimension("Bordered size", bordered_w, bordered_h) {
        return None;
    }

    Some((border_x, border_y))
}

fn compute_export_geometry(
    src_w: u32,
    src_h: u32,
    settings: &ExportSettings,
) -> Option<ExportGeometry> {
    if src_w == 0 || src_h == 0 {
        return None;
    }

    let border = settings
        .border
        .as_ref()
        .and_then(|border| calculate_border_thickness(src_w, src_h, border));
    let (border_src_x, border_src_y) = border.unwrap_or((0, 0));
    let (bordered_w, bordered_h) = bordered_size(src_w, src_h, border_src_x, border_src_y);

    let pad_target = settings
        .pad
        .as_ref()
        .and_then(|pad| calculate_pad_target(bordered_w, bordered_h, pad));

    if border.is_none() && pad_target.is_none() {
        return None;
    }

    let (canvas_w, canvas_h) = pad_target.unwrap_or((bordered_w, bordered_h));

    Some(compute_fused_geometry(
        src_w,
        src_h,
        border_src_x,
        border_src_y,
        canvas_w,
        canvas_h,
        settings.resize.as_ref(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn compute_fused_geometry(
    src_w: u32,
    src_h: u32,
    border_src_x: u32,
    border_src_y: u32,
    canvas_w: u32,
    canvas_h: u32,
    resize_opts: Option<&ResizeOptions>,
) -> ExportGeometry {
    let (final_w, final_h) = resize_target(canvas_w, canvas_h, resize_opts);
    let (final_w, final_h) = (final_w.max(1), final_h.max(1));

    let photo_w =
        ((src_w as f64 * final_w as f64 / canvas_w as f64).round() as u32).clamp(1, final_w);
    let photo_h =
        ((src_h as f64 * final_h as f64 / canvas_h as f64).round() as u32).clamp(1, final_h);

    let border_x = ((border_src_x as f64 * final_w as f64 / canvas_w as f64).round() as u32)
        .min(final_w.saturating_sub(photo_w) / 2);
    let border_y = ((border_src_y as f64 * final_h as f64 / canvas_h as f64).round() as u32)
        .min(final_h.saturating_sub(photo_h) / 2);

    ExportGeometry {
        canvas_w: final_w,
        canvas_h: final_h,
        photo_w,
        photo_h,
        offset_x: (final_w - photo_w) / 2,
        offset_y: (final_h - photo_h) / 2,
        border_x,
        border_y,
    }
}

fn compute_export_output_size(src_w: u32, src_h: u32, settings: &ExportSettings) -> (u32, u32) {
    match compute_export_geometry(src_w, src_h, settings) {
        Some(geometry) => (geometry.canvas_w, geometry.canvas_h),
        None => resize_target(src_w, src_h, settings.resize.as_ref()),
    }
}

trait FillChannel: image::Primitive {
    fn from_u8(value: u8) -> Self;
}

impl FillChannel for u8 {
    fn from_u8(value: u8) -> Self {
        value
    }
}

impl FillChannel for u16 {
    fn from_u8(value: u8) -> Self {
        value as u16 * 257
    }
}

impl FillChannel for f32 {
    fn from_u8(value: u8) -> Self {
        value as f32 / 255.0
    }
}

trait SolidFill: image::Pixel + 'static {
    fn from_rgb8(color: [u8; 3]) -> Self;
}

impl<T: FillChannel> SolidFill for image::Rgb<T>
where
    image::Rgb<T>: image::Pixel<Subpixel = T> + 'static,
{
    fn from_rgb8([r, g, b]: [u8; 3]) -> Self {
        image::Rgb([T::from_u8(r), T::from_u8(g), T::from_u8(b)])
    }
}

impl<T: FillChannel> SolidFill for image::Rgba<T>
where
    image::Rgba<T>: image::Pixel<Subpixel = T> + 'static,
{
    fn from_rgb8([r, g, b]: [u8; 3]) -> Self {
        image::Rgba([
            T::from_u8(r),
            T::from_u8(g),
            T::from_u8(b),
            T::DEFAULT_MAX_VALUE,
        ])
    }
}

fn fill_border_frame<P>(
    canvas: &mut ImageBuffer<P, Vec<P::Subpixel>>,
    geometry: &ExportGeometry,
    fill: P,
) where
    P: image::Pixel + 'static,
    P::Subpixel: 'static,
{
    let box_x = geometry.offset_x.saturating_sub(geometry.border_x);
    let box_y = geometry.offset_y.saturating_sub(geometry.border_y);
    let box_w = geometry
        .photo_w
        .saturating_add(geometry.border_x.saturating_mul(2));
    let below_photo = geometry.offset_y.saturating_add(geometry.photo_h);
    let right_of_photo = geometry.offset_x.saturating_add(geometry.photo_w);

    let strips = [
        (box_x, box_y, box_w, geometry.border_y),
        (box_x, below_photo, box_w, geometry.border_y),
        (
            box_x,
            geometry.offset_y,
            geometry.border_x,
            geometry.photo_h,
        ),
        (
            right_of_photo,
            geometry.offset_y,
            geometry.border_x,
            geometry.photo_h,
        ),
    ];

    for (x, y, width, height) in strips {
        let x_end = x.saturating_add(width).min(canvas.width());
        let y_end = y.saturating_add(height).min(canvas.height());
        for row in y..y_end {
            for col in x..x_end {
                canvas.put_pixel(col, row, fill);
            }
        }
    }
}

fn place_photo_on_canvas<P>(
    src: &ImageBuffer<P, Vec<P::Subpixel>>,
    geometry: &ExportGeometry,
    outer_fill: P,
    border_fill: Option<P>,
) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P: image::Pixel + 'static,
    P::Subpixel: 'static,
{
    let mut canvas = ImageBuffer::from_pixel(geometry.canvas_w, geometry.canvas_h, outer_fill);

    if let Some(fill) = border_fill
        && geometry.has_border()
    {
        fill_border_frame(&mut canvas, geometry, fill);
    }

    imageops::replace(
        &mut canvas,
        src,
        geometry.offset_x as i64,
        geometry.offset_y as i64,
    );
    canvas
}

fn compose_image_on_canvas(
    image: &DynamicImage,
    geometry: &ExportGeometry,
    pad_color: [u8; 3],
    border_color: [u8; 3],
) -> DynamicImage {
    fn compose<P: SolidFill>(
        src: &ImageBuffer<P, Vec<P::Subpixel>>,
        geometry: &ExportGeometry,
        pad_color: [u8; 3],
        border_color: [u8; 3],
    ) -> ImageBuffer<P, Vec<P::Subpixel>>
    where
        P::Subpixel: 'static,
    {
        place_photo_on_canvas(
            src,
            geometry,
            P::from_rgb8(pad_color),
            Some(P::from_rgb8(border_color)),
        )
    }

    match image {
        DynamicImage::ImageRgb8(buffer) => {
            DynamicImage::ImageRgb8(compose(buffer, geometry, pad_color, border_color))
        }
        DynamicImage::ImageRgba8(buffer) => {
            DynamicImage::ImageRgba8(compose(buffer, geometry, pad_color, border_color))
        }
        DynamicImage::ImageRgb16(buffer) => {
            DynamicImage::ImageRgb16(compose(buffer, geometry, pad_color, border_color))
        }
        DynamicImage::ImageRgba16(buffer) => {
            DynamicImage::ImageRgba16(compose(buffer, geometry, pad_color, border_color))
        }
        DynamicImage::ImageRgb32F(buffer) => {
            DynamicImage::ImageRgb32F(compose(buffer, geometry, pad_color, border_color))
        }
        DynamicImage::ImageRgba32F(buffer) => {
            DynamicImage::ImageRgba32F(compose(buffer, geometry, pad_color, border_color))
        }
        other => DynamicImage::ImageRgba8(compose(
            &other.to_rgba8(),
            geometry,
            pad_color,
            border_color,
        )),
    }
}

fn relative_dir_is_safe(rel_dir: &Path) -> bool {
    rel_dir.components().all(|component| {
        matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    })
}

#[cfg(windows)]
fn component_matches(left: std::path::Component<'_>, right: std::path::Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(not(windows))]
fn component_matches(left: std::path::Component<'_>, right: std::path::Component<'_>) -> bool {
    left == right
}

fn strip_prefix_preserving_source_case(source_path: &Path, base_path: &Path) -> Option<PathBuf> {
    let source_components: Vec<_> = source_path.components().collect();
    let base_components: Vec<_> = base_path.components().collect();

    if base_components.len() > source_components.len() {
        return None;
    }

    if !source_components
        .iter()
        .zip(base_components.iter())
        .all(|(source, base)| component_matches(*source, *base))
    {
        return None;
    }

    Some(source_components[base_components.len()..].iter().collect())
}

fn relative_export_dir_for_preserved_folders(
    source_path: &Path,
    base_origin_folders: &[String],
) -> Option<PathBuf> {
    base_origin_folders
        .iter()
        .filter_map(|base| {
            let base_path = Path::new(base);
            strip_prefix_preserving_source_case(source_path, base_path)
                .map(|rel_path| (base_path.components().count(), rel_path))
        })
        .max_by_key(|(component_count, _)| *component_count)
        .and_then(|(_, rel_path)| {
            let rel_dir = rel_path.parent().unwrap_or_else(|| Path::new(""));
            if relative_dir_is_safe(rel_dir) {
                Some(rel_dir.to_path_buf())
            } else {
                None
            }
        })
}

fn apply_export_geometry(
    mut image: DynamicImage,
    export_settings: &ExportSettings,
) -> Result<(DynamicImage, ExportGeometry), String> {
    let (src_w, src_h) = image.dimensions();

    match compute_export_geometry(src_w, src_h, export_settings) {
        None => {
            let (target_w, target_h) = resize_target(src_w, src_h, export_settings.resize.as_ref());
            if target_w != src_w || target_h != src_h {
                image = image.resize(target_w, target_h, imageops::FilterType::Lanczos3);
            }

            let (width, height) = image.dimensions();
            Ok((
                image,
                ExportGeometry {
                    canvas_w: width,
                    canvas_h: height,
                    photo_w: width,
                    photo_h: height,
                    offset_x: 0,
                    offset_y: 0,
                    border_x: 0,
                    border_y: 0,
                },
            ))
        }
        Some(geometry) => {
            if geometry.photo_w != src_w || geometry.photo_h != src_h {
                image = image.resize_exact(
                    geometry.photo_w,
                    geometry.photo_h,
                    imageops::FilterType::Lanczos3,
                );
            }

            let pad_color = fill_color(export_settings.pad.as_ref().map(|pad| pad.color.as_str()));
            let border_color = fill_color(
                export_settings
                    .border
                    .as_ref()
                    .map(|border| border.color.as_str()),
            );

            Ok((
                compose_image_on_canvas(&image, &geometry, pad_color, border_color),
                geometry,
            ))
        }
    }
}

fn apply_export_geometry_and_watermark(
    image: DynamicImage,
    export_settings: &ExportSettings,
) -> Result<(DynamicImage, ExportGeometry), String> {
    let (mut image, geometry) = apply_export_geometry(image, export_settings)?;

    if let Some(watermark_settings) = &export_settings.watermark {
        apply_watermark(&mut image, watermark_settings)?;
    }
    Ok((image, geometry))
}

fn ensure_export_not_cancelled(cancellation_token: &AtomicBool) -> Result<(), String> {
    if cancellation_token.load(Ordering::SeqCst) {
        Err("Export cancelled".to_string())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportCancellationRequest {
    Requested,
    AlreadyRequested,
    NoActiveTask,
}

struct ExportTaskGuard {
    task_token: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancellation_token: Arc<AtomicBool>,
    app_handle: Option<tauri::AppHandle>,
}

impl ExportTaskGuard {
    fn new(
        task_token: Arc<Mutex<Option<Arc<AtomicBool>>>>,
        cancellation_token: Arc<AtomicBool>,
    ) -> Self {
        Self {
            task_token,
            cancellation_token,
            app_handle: None,
        }
    }

    fn with_app_handle(
        task_token: Arc<Mutex<Option<Arc<AtomicBool>>>>,
        cancellation_token: Arc<AtomicBool>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        let mut guard = Self::new(task_token, cancellation_token);
        guard.app_handle = Some(app_handle);
        guard
    }
}

fn register_export_task(
    task_token: &Mutex<Option<Arc<AtomicBool>>>,
) -> Result<Arc<AtomicBool>, String> {
    let mut active_token = task_token.lock().unwrap();
    if active_token.is_some() {
        return Err("An export is already in progress.".to_string());
    }

    let cancellation_token = Arc::new(AtomicBool::new(false));
    *active_token = Some(Arc::clone(&cancellation_token));
    Ok(cancellation_token)
}

fn request_export_cancellation<F>(
    task_token: &Mutex<Option<Arc<AtomicBool>>>,
    on_requested: F,
) -> ExportCancellationRequest
where
    F: FnOnce(),
{
    let active_token = task_token.lock().unwrap();
    let Some(cancellation_token) = active_token.as_ref() else {
        return ExportCancellationRequest::NoActiveTask;
    };

    if cancellation_token.swap(true, Ordering::SeqCst) {
        ExportCancellationRequest::AlreadyRequested
    } else {
        on_requested();
        ExportCancellationRequest::Requested
    }
}

fn finish_export_task<F>(
    task_token: &Mutex<Option<Arc<AtomicBool>>>,
    cancellation_token: &Arc<AtomicBool>,
    on_finish: F,
) -> bool
where
    F: FnOnce(bool),
{
    let mut active_token = task_token.lock().unwrap();
    let Some(current_token) = active_token.as_ref() else {
        return false;
    };
    if !Arc::ptr_eq(current_token, cancellation_token) {
        return false;
    }

    let cancelled = cancellation_token.load(Ordering::SeqCst);
    *active_token = None;

    on_finish(cancelled);
    true
}

impl Drop for ExportTaskGuard {
    fn drop(&mut self) {
        let app_handle = self.app_handle.clone();
        let _ = finish_export_task(
            &self.task_token,
            &self.cancellation_token,
            |cancelled| match (cancelled, app_handle) {
                (true, Some(app_handle)) => {
                    let _ = app_handle.emit("export-cancelled", ());
                }
                (false, Some(app_handle)) => {
                    let _ = app_handle.emit("export-error", "Export task terminated unexpectedly");
                }
                _ => {}
            },
        );
    }
}

/// Chooses metadata exactly where the legacy pipeline originally read it.
/// Captured variants never invoke the path/settings fallback closures.
#[derive(Clone, Copy)]
enum ExportSourceContext {
    Legacy,
    Captured {
        as_shot: crate::as_shot::AsShot,
        render: RenderOptions,
    },
}

impl ExportSourceContext {
    fn render_options(self, legacy: impl FnOnce() -> RenderOptions) -> RenderOptions {
        match self {
            Self::Legacy => legacy(),
            Self::Captured { render, .. } => render,
        }
    }
    fn as_shot(self, legacy: impl FnOnce() -> crate::as_shot::AsShot) -> crate::as_shot::AsShot {
        match self {
            Self::Legacy => legacy(),
            Self::Captured { as_shot, .. } => as_shot,
        }
    }
}

/// Owns the real one-pass base, selected assets and immutable creative capture.
pub(crate) struct PreparedExportSource {
    appearance: crate::image_loader::PreparedStandaloneAppearance,
    creative: crate::open_creative_snapshot::CapturedCreativeAssets,
    cancel: crate::raw_processing::RawCancel,
}

pub(crate) enum PreparedExportDecision {
    Accurate(PreparedExportSource),
    DecodedLegacy(crate::image_loader::DecodedLegacyAppearance),
    LegacyRequired(crate::image_loader::LegacyPreparedReason),
}

pub(crate) fn prepare_accurate_export_source(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    settings: &crate::app_settings::AppSettings,
    registry: &crate::dcp::registry::ProfileRegistry,
    policy: crate::image_loader::AccurateRawPolicy,
    cancel: crate::raw_processing::RawCancel,
) -> Result<PreparedExportDecision, String> {
    // Missing sidecars mean no edits; retain all non-null input errors.
    let empty = serde_json::json!({});
    let adjustments = if adjustments.is_null() {
        &empty
    } else {
        adjustments
    };
    match crate::image_loader::prepare_open_appearance(
        bytes,
        path,
        adjustments,
        settings,
        registry,
        policy,
        cancel.clone(),
    )
    .map_err(|e| e.to_string())?
    {
        crate::image_loader::PreparedAppearanceDecision::LegacyRequired(reason) => {
            Ok(PreparedExportDecision::LegacyRequired(reason))
        }
        crate::image_loader::PreparedAppearanceDecision::DecodedLegacy(open) => {
            Ok(PreparedExportDecision::DecodedLegacy(open))
        }
        crate::image_loader::PreparedAppearanceDecision::Prepared(open) => {
            finish_prepared_export(open, cancel).map(PreparedExportDecision::Accurate)
        }
    }
}

fn finish_prepared_export(
    open: crate::image_loader::PreparedOpenAppearance,
    cancel: crate::raw_processing::RawCancel,
) -> Result<PreparedExportSource, String> {
    let empty = serde_json::json!({});
    let adjustments = if open.adjustments().is_null() {
        &empty
    } else {
        open.adjustments()
    };
    let creative = crate::open_creative_snapshot::capture_creative_assets(
        adjustments,
        open.settings(),
        cancel.clone(),
    )
    .map_err(|e| e.to_string())?;
    let appearance = open
        .prepare_independent_export(cancel.clone())
        .map_err(|e| e.to_string())?;
    Ok(PreparedExportSource {
        appearance,
        creative,
        cancel,
    })
}

fn checked_export_stage<T>(
    check: &impl Fn() -> Result<(), String>,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    check()?;
    let result = work();
    check()?; // Source/cancellation wins even when work failed.
    result
}

impl PreparedExportSource {
    fn with_composition<T>(
        &self,
        check: &impl Fn() -> Result<(), String>,
        work: impl FnOnce(&crate::open_composition::ComposedFit<'_, '_>) -> Result<T, String>,
    ) -> Result<T, String> {
        checked_export_stage(check, || {
            let base = self.appearance.base();
            let composed = checked_export_stage(check, || {
                crate::open_composition::compose_full_accuracy(
                    &base,
                    &self.creative,
                    self.cancel.clone(),
                )
                .map_err(|e| e.to_string())
            })?;
            checked_export_stage(check, || work(&composed))
        })
    }
}

pub(crate) fn render_composed_export(
    composed: &crate::open_composition::ComposedFit<'_, '_>,
    context: &GpuContext,
    settings: &ExportSettings,
    format: &str,
    check: &impl Fn() -> Result<(), String>,
) -> Result<DynamicImage, String> {
    let mut request = composed.request();
    request.adjustments.global.point_colour.validate()?;
    request.adjustments.global.show_clipping = 0;
    if settings.export_masks {
        request.adjustments.mask_count = 0;
        request.mask_bitmaps = &[];
    }
    let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
    checked_export_stage(check, || {
        renderer.render(
            composed.pixels(),
            request,
            render_output_precision(format, settings),
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_prepared_export_source(
    source: &PreparedExportSource,
    export_settings: &ExportSettings,
    context: &GpuContext,
    _state: &tauri::State<AppState>,
    _app_handle: &tauri::AppHandle,
    output_format: &str,
) -> Result<DynamicImage, String> {
    if output_format.eq_ignore_ascii_case("cube") {
        if crate::dcp::pipeline::has_selected_profile(source.appearance.adjustments())? {
            return Err(
                "Exporting a selected camera profile or Look as a cube LUT is not supported".into(),
            );
        }
        return Err(
            "Prepared pixel export does not handle cube output; use the existing LUT route".into(),
        );
    }
    let check = || {
        if source
            .cancel
            .as_ref()
            .is_some_and(|(tracker, generation)| tracker.load(Ordering::SeqCst) != *generation)
        {
            Err("Export cancelled".into())
        } else {
            Ok(())
        }
    };
    source.with_composition(&check, |composed| {
        let image =
            render_composed_export(composed, context, export_settings, output_format, &check)?;
        checked_export_stage(&check, || {
            apply_export_geometry_and_watermark(image, export_settings).map(|(image, _)| image)
        })
    })
}

/// No editor owner is acquired for an independent file. Decode once and retain
/// the actual decision; only explicit LegacyRequired invokes the old loader.
enum IndependentExportSource {
    Accurate(PreparedExportSource),
    Legacy {
        image: DynamicImage,
        adjustments: Value,
        as_shot: crate::as_shot::AsShot,
        raw_scale: f32,
    },
}
fn independent_export_source(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    settings: &crate::app_settings::AppSettings,
    state: &tauri::State<AppState>,
    check: &impl Fn() -> Result<(), String>,
) -> Result<IndependentExportSource, String> {
    let policy = crate::image_loader::AccurateRawPolicy::new(
        1,
        rawler::imgop::sensor::xtrans::external::XTransPasses::One.scratch_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let decision = checked_export_stage(check, || {
        prepare_accurate_export_source(
            bytes,
            path,
            adjustments,
            settings,
            &state.profile_registry,
            policy,
            None,
        )
    })?;
    match decision {
        PreparedExportDecision::Accurate(source) => Ok(IndependentExportSource::Accurate(source)),
        PreparedExportDecision::DecodedLegacy(owner) => {
            let as_shot = owner.as_shot();
            let mut adjustments = adjustments.clone();
            hydrate_adjustments(state, &mut adjustments);
            let image = checked_export_stage(check, || {
                owner
                    .render_current_with_adjustments(adjustments.clone(), None)
                    .map_err(|e| e.to_string())
            })?;
            Ok(IndependentExportSource::Legacy {
                image,
                adjustments,
                as_shot,
                raw_scale: 1.0,
            })
        }
        PreparedExportDecision::LegacyRequired(_reason) => {
            let mut adjustments = adjustments.clone();
            hydrate_adjustments(state, &mut adjustments);
            let image = checked_export_stage(check, || {
                load_and_composite_with_profiles(
                    bytes,
                    path,
                    &adjustments,
                    false,
                    settings,
                    &state.profile_registry,
                    None,
                )
                .map_err(|e| e.to_string())
            })?;
            let as_shot = checked_export_stage(check, || Ok(as_shot(path)))?;
            Ok(IndependentExportSource::Legacy {
                image,
                adjustments,
                as_shot,
                raw_scale: 1.0,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_image_for_export_pipeline(
    path: &str,
    base_image: &DynamicImage,
    js_adjustments: &Value,
    context: &GpuContext,
    state: &tauri::State<AppState>,
    is_raw: bool,
    debug_tag: &str,
    app_handle: &tauri::AppHandle,
    output_precision: RenderOutputPrecision,
) -> Result<DynamicImage, String> {
    process_image_for_export_pipeline_with_source(
        path,
        base_image,
        js_adjustments,
        context,
        state,
        is_raw,
        debug_tag,
        app_handle,
        output_precision,
        ExportSourceContext::Legacy,
    )
}

#[allow(clippy::too_many_arguments)]
fn process_image_for_export_pipeline_with_source(
    path: &str,
    base_image: &DynamicImage,
    js_adjustments: &Value,
    context: &GpuContext,
    state: &tauri::State<AppState>,
    is_raw: bool,
    debug_tag: &str,
    app_handle: &tauri::AppHandle,
    output_precision: RenderOutputPrecision,
    source: ExportSourceContext,
) -> Result<DynamicImage, String> {
    let (transformed_image, unscaled_crop_offset) =
        apply_all_transformations(Cow::Borrowed(base_image), js_adjustments);
    let (img_w, img_h) = transformed_image.dimensions();

    let mask_definitions: Vec<MaskDefinition> = js_adjustments
        .get("masks")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default();

    let warped_image =
        build_warped_image_for_masks(base_image, is_raw, js_adjustments, &mask_definitions);
    let mask_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = mask_definitions
        .iter()
        .filter_map(|def| {
            generate_mask_bitmap(
                def,
                img_w,
                img_h,
                1.0,
                unscaled_crop_offset,
                warped_image.as_deref(),
            )
        })
        .collect();

    let render = source.render_options(|| resolve_render_options_from_handle(app_handle, is_raw));
    let mut all_adjustments = get_all_adjustments_from_json(
        js_adjustments,
        is_raw,
        source.as_shot(|| as_shot(path)),
        render,
    );
    all_adjustments.global.show_clipping = 0;

    let lut_path = js_adjustments["lutPath"].as_str();
    let lut = lut_path.and_then(|p| get_or_load_lut(state, p).ok());
    if matches!(source, ExportSourceContext::Captured { .. }) {
        let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
        return renderer.render(
            transformed_image.as_ref(),
            RenderRequest {
                adjustments: all_adjustments,
                mask_bitmaps: &mask_bitmaps,
                lut,
                roi: None,
                dcp_textures: None,
            },
            output_precision,
        );
    }

    let unique_hash = calculate_full_job_hash(path, js_adjustments);

    process_and_get_dynamic_image_with_precision(
        context,
        state,
        transformed_image.as_ref(),
        unique_hash,
        RenderRequest {
            adjustments: all_adjustments,
            mask_bitmaps: &mask_bitmaps,
            lut,
            roi: None,
            dcp_textures: None,
        },
        debug_tag,
        output_precision,
    )
}

fn render_output_precision(
    output_format: &str,
    export_settings: &ExportSettings,
) -> RenderOutputPrecision {
    if matches!(output_format.to_lowercase().as_str(), "tif" | "tiff")
        && export_settings.tiff_bit_depth == TiffBitDepth::Sixteen
    {
        RenderOutputPrecision::SixteenBit
    } else {
        RenderOutputPrecision::EightBit
    }
}

fn set_timestamps_from_exif(src: &Path, dst: &Path) {
    let capture_dt = exif_processing::get_creation_date_from_path(src);

    if capture_dt.timestamp() <= 0 {
        let now = filetime::FileTime::now();
        if let Err(e) = filetime::set_file_times(dst, now, now) {
            log::warn!("Could not set timestamps on '{}': {}", dst.display(), e);
        }
        return;
    }

    let ft = filetime::FileTime::from_unix_time(
        capture_dt.timestamp(),
        capture_dt.timestamp_subsec_nanos(),
    );
    if let Err(e) = filetime::set_file_times(dst, ft, ft) {
        log::warn!("Could not set timestamps on '{}': {}", dst.display(), e);
    }
}

fn save_image_with_metadata(
    image: &DynamicImage,
    output_path: &Path,
    source_path_str: &str,
    export_xmp: &exif_processing::ExportXmp,
    export_settings: &ExportSettings,
) -> Result<(), String> {
    save_image_with_metadata_checked(
        image,
        output_path,
        source_path_str,
        export_xmp,
        export_settings,
        &|| Ok(()),
    )
}

fn save_image_with_metadata_checked(
    image: &DynamicImage,
    output_path: &std::path::Path,
    source_path_str: &str,
    export_xmp: &exif_processing::ExportXmp,
    export_settings: &ExportSettings,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let extension = output_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut image_bytes = checked_export_stage(check, || {
        encode_image_to_bytes(
            image,
            &extension,
            export_settings.jpeg_quality,
            export_settings.tiff_bit_depth,
        )
    })?;

    checked_export_stage(check, || {
        exif_processing::write_image_with_metadata(
            &mut image_bytes,
            source_path_str,
            &extension,
            export_settings.keep_metadata,
            export_settings.strip_gps,
            export_xmp,
        )
    })?;

    #[cfg(target_os = "android")]
    {
        let file_name = output_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Missing Android export file name".to_string())?;
        checked_export_stage(check, || {
            crate::android_integration::save_image_bytes_to_android_gallery(
                file_name,
                mime_type_for_extension(&extension),
                &image_bytes,
            )
        })?;
    }

    #[cfg(not(target_os = "android"))]
    checked_export_stage(check, || {
        fs::write(output_path, image_bytes)
            .map_err(|e| format!("Failed to write file to '{}': {}", output_path.display(), e))
    })?;

    Ok(())
}

#[cfg(target_os = "android")]
pub fn mime_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        "jxl" => "image/jxl",
        _ => "application/octet-stream",
    }
}

#[allow(clippy::too_many_arguments)]
fn process_image_for_export(
    path: &str,
    base_image: &DynamicImage,
    js_adjustments: &Value,
    export_settings: &ExportSettings,
    context: &GpuContext,
    state: &tauri::State<AppState>,
    is_raw: bool,
    app_handle: &tauri::AppHandle,
    output_format: &str,
) -> Result<DynamicImage, String> {
    let processed_image = process_image_for_export_pipeline(
        path,
        base_image,
        js_adjustments,
        context,
        state,
        is_raw,
        "process_image_for_export",
        app_handle,
        render_output_precision(output_format, export_settings),
    )?;

    apply_export_geometry_and_watermark(processed_image, export_settings).map(|(image, _)| image)
}

fn build_single_mask_adjustments(all: &AllAdjustments, mask_index: usize) -> AllAdjustments {
    let mut single = AllAdjustments {
        global: all.global,
        mask_adjustments: all.mask_adjustments,
        mask_count: 1,
        tile_offset_x: all.tile_offset_x,
        tile_offset_y: all.tile_offset_y,
        mask_atlas_cols: all.mask_atlas_cols,
    };
    single.mask_adjustments[0] = all.mask_adjustments[mask_index];
    for i in 1..single.mask_adjustments.len() {
        single.mask_adjustments[i] = Default::default();
    }
    single
}

fn encode_grayscale_to_png(bitmap: &GrayImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    bitmap
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Exports are sRGB-encoded; tagging them lets color-managed apps read them correctly.
const SRGB_ICC_PROFILE: &[u8] = include_bytes!("../icc/sRGB-v2-magic.icc");

fn encode_image_to_bytes(
    image: &DynamicImage,
    output_format: &str,
    jpeg_quality: u8,
    tiff_bit_depth: TiffBitDepth,
) -> Result<Vec<u8>, String> {
    let mut image_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut image_bytes);

    match output_format.to_lowercase().as_str() {
        "jxl" => {
            let (width, height) = image.dimensions();
            let has_alpha = image.color().has_alpha();

            let jxl_data = if jpeg_quality == 100 {
                if has_alpha {
                    let rgba = image.to_rgba8();
                    LosslessConfig::new()
                        .encode(rgba.as_raw(), width, height, PixelLayout::Rgba8)
                        .map_err(|e| format!("Failed to encode lossless JXL: {}", e))?
                } else {
                    let rgb = image.to_rgb8();
                    LosslessConfig::new()
                        .encode(rgb.as_raw(), width, height, PixelLayout::Rgb8)
                        .map_err(|e| format!("Failed to encode lossless JXL: {}", e))?
                }
            } else {
                let jxl_quality = calibrated_jxl_quality(jpeg_quality as f32);
                let distance = quality_to_distance(jxl_quality);

                if has_alpha {
                    let rgba = image.to_rgba8();
                    LossyConfig::new(distance)
                        .encode(rgba.as_raw(), width, height, PixelLayout::Rgba8)
                        .map_err(|e| format!("Failed to encode lossy JXL: {}", e))?
                } else {
                    let rgb = image.to_rgb8();
                    LossyConfig::new(distance)
                        .encode(rgb.as_raw(), width, height, PixelLayout::Rgb8)
                        .map_err(|e| format!("Failed to encode lossy JXL: {}", e))?
                }
            };

            return Ok(jxl_data);
        }
        "webp" => {
            let encoder = webp::Encoder::from_image(image)
                .map_err(|_| "Failed to create WebP encoder".to_string())?;
            let webp_mem = encoder.encode(jpeg_quality as f32);
            return Ok(webp_mem.to_vec());
        }
        "jpg" | "jpeg" => {
            let rgb_image = image.to_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(&mut cursor, jpeg_quality);
            encoder
                .set_icc_profile(SRGB_ICC_PROFILE.to_vec())
                .map_err(|e| e.to_string())?;
            rgb_image
                .write_with_encoder(encoder)
                .map_err(|e| e.to_string())?;
        }
        "png" => {
            let image_to_encode = if image.as_rgb32f().is_some() {
                DynamicImage::ImageRgb16(image.to_rgb16())
            } else {
                image.clone()
            };

            let mut encoder = PngEncoder::new(&mut cursor);
            encoder
                .set_icc_profile(SRGB_ICC_PROFILE.to_vec())
                .map_err(|e| e.to_string())?;
            image_to_encode
                .write_with_encoder(encoder)
                .map_err(|e| e.to_string())?;
        }
        "tif" | "tiff" => {
            let image_to_encode = match tiff_bit_depth {
                TiffBitDepth::Eight => DynamicImage::ImageRgb8(image.to_rgb8()),
                TiffBitDepth::Sixteen => DynamicImage::ImageRgb16(image.to_rgb16()),
            };
            image_to_encode
                .write_to(&mut cursor, image::ImageFormat::Tiff)
                .map_err(|e| e.to_string())?;
        }
        "avif" => {
            image
                .write_to(&mut cursor, image::ImageFormat::Avif)
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unsupported file format: {}", output_format)),
    };
    Ok(image_bytes)
}

#[allow(clippy::too_many_arguments)]
fn export_masks_for_image(
    base_image: &DynamicImage,
    js_adjustments: &Value,
    export_settings: &ExportSettings,
    output_path_obj: &std::path::Path,
    source_path_str: &str,
    export_xmp: &exif_processing::ExportXmp,
    context: &Arc<GpuContext>,
    state: &tauri::State<AppState>,
    is_raw: bool,
    app_handle: &tauri::AppHandle,
    cancellation_token: &AtomicBool,
    source: ExportSourceContext,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    ensure_export_not_cancelled(cancellation_token)?;
    check()?;
    let (transformed_image, unscaled_crop_offset) =
        apply_all_transformations(Cow::Borrowed(base_image), js_adjustments);
    ensure_export_not_cancelled(cancellation_token)?;
    check()?;
    let (img_w, img_h) = transformed_image.dimensions();
    let mask_definitions: Vec<MaskDefinition> = js_adjustments
        .get("masks")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default();

    let warped_image =
        build_warped_image_for_masks(base_image, is_raw, js_adjustments, &mask_definitions);
    let mut mask_bitmaps = Vec::with_capacity(mask_definitions.len());
    for definition in &mask_definitions {
        ensure_export_not_cancelled(cancellation_token)?;
        check()?;
        if let Some(bitmap) = generate_mask_bitmap(
            definition,
            img_w,
            img_h,
            1.0,
            unscaled_crop_offset,
            warped_image.as_deref(),
        ) {
            mask_bitmaps.push(bitmap);
        }
        ensure_export_not_cancelled(cancellation_token)?;
        check()?;
    }

    if !mask_bitmaps.is_empty() {
        let render =
            source.render_options(|| resolve_render_options_from_handle(app_handle, is_raw));
        let all_adjustments = get_all_adjustments_from_json(
            js_adjustments,
            is_raw,
            source.as_shot(|| as_shot(source_path_str)),
            render,
        );
        let lut_path = js_adjustments["lutPath"].as_str();
        let lut = lut_path.and_then(|p| get_or_load_lut(state, p).ok());
        let unique_hash = calculate_full_job_hash(source_path_str, js_adjustments);
        let output_dir = output_path_obj.parent().unwrap_or(output_path_obj);
        let stem = output_path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        let extension = output_path_obj
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg");

        for (i, _) in mask_bitmaps.iter().enumerate() {
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;
            let single_adjustments = build_single_mask_adjustments(&all_adjustments, i);
            let full_white_mask = ImageBuffer::from_fn(img_w, img_h, |_, _| Luma([255u8]));
            let single_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = vec![full_white_mask];

            let processed = checked_export_stage(check, || {
                let request = RenderRequest {
                    adjustments: single_adjustments,
                    mask_bitmaps: &single_bitmaps,
                    lut: lut.clone(),
                    roi: None,
                    dcp_textures: None,
                };
                match source {
                    ExportSourceContext::Legacy => process_and_get_dynamic_image_with_precision(
                        context,
                        state,
                        transformed_image.as_ref(),
                        unique_hash,
                        request,
                        "export_mask_image",
                        render_output_precision(extension, export_settings),
                    ),
                    ExportSourceContext::Captured { .. } => {
                        let mut renderer =
                            crate::gpu_processing::IsolatedCreativeRenderer::new(context);
                        renderer.render(
                            transformed_image.as_ref(),
                            request,
                            render_output_precision(extension, export_settings),
                        )
                    }
                }
            })?;
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;

            let (with_options, geometry) =
                apply_export_geometry_and_watermark(processed, export_settings)?;

            let alpha_scaled = imageops::resize(
                &mask_bitmaps[i],
                geometry.photo_w,
                geometry.photo_h,
                imageops::FilterType::Lanczos3,
            );

            let alpha_resized: GrayImage = if geometry.is_identity() {
                alpha_scaled
            } else {
                place_photo_on_canvas(&alpha_scaled, &geometry, Luma([0u8]), None)
            };
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;

            let mask_image_path =
                output_dir.join(format!("{}_mask_{}_image.{}", stem, i, extension));
            let mask_alpha_path = output_dir.join(format!("{}_mask_{}_alpha.png", stem, i));

            save_image_with_metadata_checked(
                &with_options,
                &mask_image_path,
                source_path_str,
                export_xmp,
                export_settings,
                check,
            )?;
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;

            if export_settings.preserve_timestamps {
                checked_export_stage(check, || {
                    set_timestamps_from_exif(Path::new(source_path_str), &mask_image_path);
                    Ok(())
                })?;
            }

            let alpha_bytes =
                checked_export_stage(check, || encode_grayscale_to_png(&alpha_resized))?;
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;
            #[cfg(target_os = "android")]
            {
                let file_name = mask_alpha_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| "Missing Android mask export file name".to_string())?;
                checked_export_stage(check, || {
                    crate::android_integration::save_image_bytes_to_android_gallery(
                        file_name,
                        "image/png",
                        &alpha_bytes,
                    )
                })?;
            }

            #[cfg(not(target_os = "android"))]
            checked_export_stage(check, || {
                fs::write(&mask_alpha_path, alpha_bytes).map_err(|e| e.to_string())
            })?;
            ensure_export_not_cancelled(cancellation_token)?;
            check()?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn export_composed_masks(
    composed: &crate::open_composition::ComposedFit<'_, '_>,
    export_settings: &ExportSettings,
    output_path_obj: &Path,
    source_path_str: &str,
    export_xmp: &exif_processing::ExportXmp,
    context: &GpuContext,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    check()?;
    let request = composed.request();
    let mask_bitmaps = request.mask_bitmaps;
    let all_adjustments = request.adjustments;
    let lut = request.lut;
    let (img_w, img_h) = composed.pixels().dimensions();
    let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
    if !mask_bitmaps.is_empty() {
        let output_dir = output_path_obj.parent().unwrap_or(output_path_obj);
        let stem = output_path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        let extension = output_path_obj
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg");

        for (i, _) in mask_bitmaps.iter().enumerate() {
            check()?;
            let single_adjustments = build_single_mask_adjustments(&all_adjustments, i);
            let full_white_mask = ImageBuffer::from_fn(img_w, img_h, |_, _| Luma([255u8]));
            let single_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = vec![full_white_mask];

            let processed = checked_export_stage(check, || {
                renderer.render(
                    composed.pixels(),
                    RenderRequest {
                        adjustments: single_adjustments,
                        mask_bitmaps: &single_bitmaps,
                        lut: lut.clone(),
                        roi: None,
                        dcp_textures: None,
                    },
                    render_output_precision(extension, export_settings),
                )
            })?;
            check()?;

            let (with_options, geometry) = checked_export_stage(check, || {
                apply_export_geometry_and_watermark(processed, export_settings)
            })?;

            let alpha_scaled = imageops::resize(
                &mask_bitmaps[i],
                geometry.photo_w,
                geometry.photo_h,
                imageops::FilterType::Lanczos3,
            );

            let alpha_resized: GrayImage = if geometry.is_identity() {
                alpha_scaled
            } else {
                place_photo_on_canvas(&alpha_scaled, &geometry, Luma([0u8]), None)
            };
            check()?;

            let mask_image_path =
                output_dir.join(format!("{}_mask_{}_image.{}", stem, i, extension));
            let mask_alpha_path = output_dir.join(format!("{}_mask_{}_alpha.png", stem, i));

            save_image_with_metadata_checked(
                &with_options,
                &mask_image_path,
                source_path_str,
                export_xmp,
                export_settings,
                check,
            )?;
            check()?;

            if export_settings.preserve_timestamps {
                checked_export_stage(check, || {
                    set_timestamps_from_exif(Path::new(source_path_str), &mask_image_path);
                    Ok(())
                })?;
            }

            let alpha_bytes =
                checked_export_stage(check, || encode_grayscale_to_png(&alpha_resized))?;
            check()?;
            #[cfg(target_os = "android")]
            {
                let file_name = mask_alpha_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| "Missing Android mask export file name".to_string())?;
                checked_export_stage(check, || {
                    crate::android_integration::save_image_bytes_to_android_gallery(
                        file_name,
                        "image/png",
                        &alpha_bytes,
                    )
                })?;
            }

            #[cfg(not(target_os = "android"))]
            checked_export_stage(check, || {
                fs::write(&mask_alpha_path, alpha_bytes).map_err(|e| e.to_string())
            })?;
            check()?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_composed_export(
    composed: &crate::open_composition::ComposedFit<'_, '_>,
    export_settings: &ExportSettings,
    output_path: &Path,
    source_path: &str,
    export_xmp: &exif_processing::ExportXmp,
    context: &GpuContext,
    format: &str,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let processed = render_composed_export(composed, context, export_settings, format, check)?;
    let final_image = checked_export_stage(check, || {
        apply_export_geometry_and_watermark(processed, export_settings).map(|(image, _)| image)
    })?;
    save_image_with_metadata_checked(
        &final_image,
        output_path,
        source_path,
        export_xmp,
        export_settings,
        check,
    )?;
    if export_settings.preserve_timestamps {
        checked_export_stage(check, || {
            set_timestamps_from_exif(Path::new(source_path), output_path);
            Ok(())
        })?;
    }
    if export_settings.export_masks {
        export_composed_masks(
            composed,
            export_settings,
            output_path,
            source_path,
            export_xmp,
            context,
            check,
        )?;
    }
    check()
}

fn export_adjustments_as_lut(
    js_adjustments: &Value,
    source_path_str: &str,
    context: &Arc<GpuContext>,
    state: &tauri::State<AppState>,
    app_handle: &tauri::AppHandle,
    cancellation_token: &AtomicBool,
) -> Result<Vec<u8>, String> {
    ensure_export_not_cancelled(cancellation_token)?;
    let lut_size = 33;
    let identity_image = generate_identity_lut_image(lut_size);

    let render = RenderOptions {
        baseline: BaselineOptions::default(),
        ..resolve_render_options_from_handle(app_handle, false)
    };
    let mut all_adjustments =
        get_all_adjustments_from_json(js_adjustments, false, as_shot(source_path_str), render);

    all_adjustments.global.show_clipping = 0;
    all_adjustments.global.vignette_amount = 0.0;
    all_adjustments.global.grain_amount = 0.0;
    all_adjustments.global.sharpness = 0.0;
    all_adjustments.global.clarity = 0.0;
    all_adjustments.global.dehaze = 0.0;
    all_adjustments.global.structure = 0.0;
    all_adjustments.global.centré = 0.0;
    all_adjustments.global.glow_amount = 0.0;
    all_adjustments.global.halation_amount = 0.0;
    all_adjustments.global.flare_amount = 0.0;
    all_adjustments.global.luma_noise_reduction = 0.0;
    all_adjustments.global.color_noise_reduction = 0.0;
    all_adjustments.global.chromatic_aberration_red_cyan = 0.0;
    all_adjustments.global.chromatic_aberration_blue_yellow = 0.0;

    let lut_path = js_adjustments["lutPath"].as_str();
    let lut = lut_path.and_then(|p| get_or_load_lut(state, p).ok());
    let unique_hash = calculate_full_job_hash(source_path_str, js_adjustments);

    let processed_lut = process_and_get_dynamic_image(
        context,
        state,
        &identity_image,
        unique_hash,
        RenderRequest {
            adjustments: all_adjustments,
            mask_bitmaps: &[],
            lut,
            roi: None,
            dcp_textures: None,
        },
        "export_lut",
    )?;
    ensure_export_not_cancelled(cancellation_token)?;

    let cube_lut = convert_image_to_cube_lut(&processed_lut, lut_size)?;
    ensure_export_not_cancelled(cancellation_token)?;
    Ok(cube_lut)
}

fn current_edit_export_target(
    paths: &[String],
    mode: &ExportAdjustmentsMode,
    keyed: bool,
) -> Result<Option<String>, String> {
    match mode {
        ExportAdjustmentsMode::UseSidecars {
            active_path: Some(path),
            active_adjustments,
        } if paths.iter().any(|item| item == path) => {
            if keyed && active_adjustments.is_none() {
                return Err("Keyed export requires captured current adjustments".into());
            }
            Ok(Some(path.clone()))
        }
        _ if keyed => Err("Keyed export requires the exact current virtual path".into()),
        _ => Ok(None),
    }
}
fn with_export_file_bytes<T>(
    path: &Path,
    check: &impl Fn() -> Result<(), String>,
    work: impl FnOnce(&[u8]) -> Result<T, String>,
) -> Result<T, String> {
    checked_export_stage(check, || match read_file_mapped(path) {
        Ok(bytes) => checked_export_stage(check, || work(&bytes)),
        Err(_) => {
            let bytes = checked_export_stage(check, || fs::read(path).map_err(|e| e.to_string()))?;
            checked_export_stage(check, || work(&bytes))
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn export_images_impl(
    paths: Vec<String>,
    output_folder_or_file: String,
    is_explicit_file_path: bool,
    base_origin_folders: Vec<String>,
    export_settings: ExportSettings,
    output_format: String,
    adjustments_mode: ExportAdjustmentsMode,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    completion_tx: Option<tokio::sync::oneshot::Sender<Result<(), usize>>>,
) -> Result<(), String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let current_path =
        current_edit_export_target(&paths, &adjustments_mode, expected_open_key.is_some())?;
    // A cube is a JSON/LUT operation, not a pixel appearance capture. Keep its
    // old validation/asset-read ordering while retaining the real source fence.
    let is_cube = output_format.eq_ignore_ascii_case("cube");
    let cube_source = if is_cube {
        current_path
            .as_deref()
            .map(|path| {
                state
                    .capture_consumer_source(expected_open_key.as_ref(), Some(path))
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .transpose()?
    } else {
        None
    };
    let current_request = if let Some(path) = current_path.as_deref().filter(|_| !is_cube) {
        let adjustments = match &adjustments_mode {
            ExportAdjustmentsMode::UseSidecars {
                active_adjustments, ..
            } => active_adjustments.clone(),
            ExportAdjustmentsMode::GlobalOverride(_) => None,
        };
        Some(state.capture_consumer_request(
            expected_open_key.as_ref(),
            Some(path),
            adjustments,
            settings.clone(),
        )?)
    } else {
        None
    };
    let cancellation_token = register_export_task(&state.export_task_token)?;
    let task_guard = ExportTaskGuard::with_app_handle(
        Arc::clone(&state.export_task_token),
        Arc::clone(&cancellation_token),
        app_handle.clone(),
    );
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    if cancellation_token.load(Ordering::SeqCst) {
        return Ok(());
    }

    let admission_check = || {
        ensure_export_not_cancelled(&cancellation_token)?;
        if let Some(request) = &current_request {
            request.check()?;
        }
        if let Some(source) = &cube_source {
            source.check().map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let context = match checked_export_stage(&admission_check, || {
        get_or_init_gpu_context(&state, &app_handle)
    }) {
        Ok(context) => context,
        Err(_) if cancellation_token.load(Ordering::SeqCst) => return Ok(()),
        Err(error) => return Err(error),
    };

    if cancellation_token.load(Ordering::SeqCst) {
        return Ok(());
    }

    let context = Arc::new(context);
    let progress_counter = Arc::new(AtomicUsize::new(0));

    let available_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();

    let available_ram_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let ram_based_limit = (available_ram_gb / 4.0).floor() as usize;

    let num_threads = if paths.len() == 1 {
        1
    } else {
        available_cores.min(ram_based_limit).clamp(1, 4)
    };

    log::info!(
        "Batch Export: {} cores, {:.1} GB free RAM -> {} threads",
        available_cores,
        available_ram_gb,
        num_threads
    );

    let _export_task = tokio::spawn(async move {
        let _task_guard = task_guard;
        let output_folder_path = std::path::Path::new(&output_folder_or_file);
        let total_paths = paths.len();

        let mut base_path_counts: HashMap<String, usize> = HashMap::new();
        let mut export_items = Vec::with_capacity(total_paths);

        for (i, path_str) in paths.into_iter().enumerate() {
            let (source_path, _) = parse_virtual_path(&path_str);
            let source_str = source_path.to_string_lossy().to_string();
            let count = base_path_counts.entry(source_str.clone()).or_insert(0);
            *count += 1;

            let mut explicit_vc = None;
            if let Some(idx) = path_str.rfind("vc=") {
                let id_str = path_str[idx + 3..].split('&').next().unwrap_or("");
                if let Ok(id) = id_str.parse::<u32>() {
                    explicit_vc = Some(id);
                }
            }
            if explicit_vc.is_none() {
                let lower = path_str.to_lowercase();
                if let Some(idx) = lower.rfind("_vc") {
                    let id_str: String = lower[idx + 3..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(id) = id_str.parse::<u32>() {
                        explicit_vc = Some(id);
                    }
                }
            }
            export_items.push((i, path_str, *count, explicit_vc));
        }

        let used_paths = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(num_threads));
        let mut join_handles = Vec::new();

        for (global_index, image_path_str, appearance_count, explicit_vc) in export_items {
            if cancellation_token.load(Ordering::SeqCst) {
                break;
            }
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            if cancellation_token.load(Ordering::SeqCst) {
                drop(permit);
                break;
            }

            let app_handle_clone = app_handle.clone();
            let context_clone = Arc::clone(&context);
            let progress_counter_clone = Arc::clone(&progress_counter);
            let output_folder_path = output_folder_path.to_path_buf();
            let base_origin_folders = base_origin_folders.clone();
            let export_settings = export_settings.clone();
            let output_format = output_format.clone();
            let settings = settings.clone();
            let cancellation_token_clone = Arc::clone(&cancellation_token);
            let adjustments_mode = adjustments_mode.clone();
            let used_paths_clone = Arc::clone(&used_paths);

            let current_request = current_request
                .as_ref()
                .filter(|request| request.loaded().path == image_path_str)
                .cloned();
            let cube_source = cube_source
                .as_ref()
                .filter(|source| source.loaded().path == image_path_str)
                .cloned();
            let handle = tokio::task::spawn_blocking(move || {
                let check = || {
                    ensure_export_not_cancelled(&cancellation_token_clone)?;
                    if let Some(request) = &current_request {
                        request.check()?;
                    }
                    if let Some(source) = &cube_source {
                        source.check().map_err(|e| e.to_string())?;
                    }
                    Ok(())
                };
                checked_export_stage(&check, || {
                    ensure_export_not_cancelled(&cancellation_token_clone)?;

                    let state = app_handle_clone.state::<AppState>();
                    let (source_path, sidecar_path) = parse_virtual_path(&image_path_str);
                    let source_path_str = source_path.to_string_lossy().to_string();

                    let is_current_edit = current_request.is_some() || cube_source.is_some();

                    let sidecar = crate::exif_processing::load_sidecar(&sidecar_path);
                    let export_xmp = crate::exif_processing::ExportXmp::from_metadata(&sidecar);

                    let mut js_adjustments = match &adjustments_mode {
                        ExportAdjustmentsMode::UseSidecars {
                            active_adjustments, ..
                        } => {
                            if is_current_edit {
                                if let Some(adj) = active_adjustments {
                                    adj.clone()
                                } else {
                                    sidecar.adjustments
                                }
                            } else {
                                sidecar.adjustments
                            }
                        }
                        ExportAdjustmentsMode::GlobalOverride(adj) => adj.clone(),
                    };

                    let is_raw = is_raw_file(&source_path_str);
                    check()?;
                    let original_path = std::path::Path::new(&source_path_str);
                    let file_date = exif_processing::get_creation_date_from_path(original_path);

                    let filename_template = export_settings
                        .filename_template
                        .as_deref()
                        .unwrap_or("{original_filename}_edited");

                    let mut new_stem = generate_filename_from_template(
                        filename_template,
                        original_path,
                        global_index + 1,
                        total_paths,
                        &file_date,
                    );

                    if let Some(vc_id) = explicit_vc {
                        new_stem = format!("{}_VC{:02}", new_stem, vc_id);
                    } else if appearance_count > 1 {
                        new_stem = format!("{}_VC{:02}", new_stem, appearance_count - 1);
                    }

                    let new_filename = format!("{}.{}", new_stem, output_format);

                    check()?;
                    let mut output_path =
                        if export_settings.destination_type.as_deref() == Some("originalFolder") {
                            let mut dir = source_path
                                .parent()
                                .unwrap_or(std::path::Path::new(""))
                                .to_path_buf();
                            if let Some(sub) = &export_settings.subfolder {
                                let mut trimmed = sub.trim();

                                while trimmed.starts_with('/') || trimmed.starts_with('\\') {
                                    trimmed = &trimmed[1..];
                                }

                                if !trimmed.is_empty() {
                                    dir = dir.join(trimmed);
                                }
                            }

                            if let Err(e) = std::fs::create_dir_all(&dir) {
                                return Err(format!(
                                    "Failed to create export subdirectory '{}': {}",
                                    dir.display(),
                                    e
                                ));
                            }

                            dir.join(&new_filename)
                        } else if is_explicit_file_path && total_paths == 1 {
                            output_folder_path.clone()
                        } else if export_settings.preserve_folders {
                            if let Some(rel_dir) = relative_export_dir_for_preserved_folders(
                                source_path.as_path(),
                                &base_origin_folders,
                            ) {
                                let full_dir = output_folder_path.join(rel_dir);
                                if let Err(e) = std::fs::create_dir_all(&full_dir) {
                                    return Err(format!(
                                        "Failed to create export subdirectory '{}': {}",
                                        full_dir.display(),
                                        e
                                    ));
                                }
                                full_dir.join(&new_filename)
                            } else {
                                output_folder_path.join(&new_filename)
                            }
                        } else {
                            output_folder_path.join(&new_filename)
                        };

                    let extension = output_format.to_lowercase();

                    if !(is_explicit_file_path && total_paths == 1) {
                        let mut used = used_paths_clone.lock().unwrap();
                        let parent_dir = output_path
                            .parent()
                            .unwrap_or(std::path::Path::new(""))
                            .to_path_buf();
                        let mut counter = 1;

                        while output_path.exists() || used.contains(&output_path) {
                            let incremented_filename =
                                format!("{}_{}.{}", new_stem, counter, extension);
                            output_path = parent_dir.join(&incremented_filename);
                            counter += 1;
                        }
                        used.insert(output_path.clone());
                    }

                    let result: Result<(), String> = (|| {
                        if extension == "cube" {
                            hydrate_adjustments(&state, &mut js_adjustments);
                            if crate::dcp::pipeline::has_selected_profile(&js_adjustments)? {
                                return Err("Exporting a selected camera profile or Look as a cube LUT is not supported".into());
                            }
                            let cube_bytes = checked_export_stage(&check, || {
                                export_adjustments_as_lut(
                                    &js_adjustments,
                                    &source_path_str,
                                    &context_clone,
                                    &state,
                                    &app_handle_clone,
                                    &cancellation_token_clone,
                                )
                            })?;
                            check()?;
                            #[cfg(target_os = "android")]
                            {
                                let file_name = output_path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .ok_or_else(|| "Missing Android LUT file name".to_string())?;
                                crate::android_integration::save_file_bytes_to_android_downloads(
                                    file_name,
                                    "application/octet-stream",
                                    &cube_bytes,
                                )?;
                            }
                            #[cfg(not(target_os = "android"))]
                            fs::write(&output_path, cube_bytes).map_err(|e| e.to_string())?;
                            check()?;
                            return Ok(());
                        }

                        let actual_output_format = output_path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or(output_format.as_str());
                        if let Some(request) = &current_request
                            && request.is_keyed()
                        {
                            return request.with_full_composition(|composed| {
                                write_composed_export(
                                    composed,
                                    &export_settings,
                                    &output_path,
                                    &source_path_str,
                                    &export_xmp,
                                    &context_clone,
                                    actual_output_format,
                                    &check,
                                )
                            });
                        }
                        let (base_image, captured_as_shot) = if let Some(request) = &current_request
                        {
                            // The legacy lease and exact path remain held through every output.
                            hydrate_adjustments(&state, &mut js_adjustments);
                            let selected_profile =
                                crate::dcp::pipeline::has_selected_profile(&js_adjustments)?;
                            let image = if selected_profile {
                                with_export_file_bytes(
                                    Path::new(&source_path_str),
                                    &check,
                                    |bytes| {
                                        load_and_composite_with_profiles(
                                            bytes,
                                            &source_path_str,
                                            &js_adjustments,
                                            false,
                                            request.settings(),
                                            &state.profile_registry,
                                            None,
                                        )
                                        .map_err(|e| e.to_string())
                                    },
                                )?
                            } else {
                                checked_export_stage(&check, || {
                                    composite_patches_on_image(
                                        &request.loaded().image,
                                        &js_adjustments,
                                    )
                                    .map_err(|e| format!("Failed to composite AI patches: {}", e))
                                })?
                            };
                            (image, request.loaded().as_shot)
                        } else {
                            let source = with_export_file_bytes(
                                Path::new(&source_path_str),
                                &check,
                                |bytes| {
                                    independent_export_source(
                                        bytes,
                                        &source_path_str,
                                        &js_adjustments,
                                        &settings,
                                        &state,
                                        &check,
                                    )
                                },
                            )?;
                            match source {
                                IndependentExportSource::Accurate(source) => {
                                    return source.with_composition(&check, |composed| {
                                        write_composed_export(
                                            composed,
                                            &export_settings,
                                            &output_path,
                                            &source_path_str,
                                            &export_xmp,
                                            &context_clone,
                                            actual_output_format,
                                            &check,
                                        )
                                    });
                                }
                                IndependentExportSource::Legacy {
                                    image,
                                    adjustments,
                                    as_shot,
                                    ..
                                } => {
                                    js_adjustments = adjustments;
                                    (image, as_shot)
                                }
                            }
                        };
                        let render_settings = current_request
                            .as_ref()
                            .map_or(&settings, |request| request.settings());
                        let source_context = ExportSourceContext::Captured {
                            as_shot: captured_as_shot,
                            render: crate::image_processing::resolve_render_options(
                                render_settings,
                                is_raw,
                            ),
                        };
                        check()?;

                        let mut main_export_adjustments = js_adjustments.clone();
                        if export_settings.export_masks
                            && let Some(obj) = main_export_adjustments.as_object_mut()
                        {
                            obj.insert("masks".to_string(), serde_json::json!([]));
                        }

                        let actual_output_format = output_path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or(output_format.as_str());

                        let processed = checked_export_stage(&check, || {
                            process_image_for_export_pipeline_with_source(
                                &source_path_str,
                                &base_image,
                                &main_export_adjustments,
                                &context_clone,
                                &state,
                                is_raw,
                                "process_image_for_export",
                                &app_handle_clone,
                                render_output_precision(actual_output_format, &export_settings),
                                source_context,
                            )
                        })?;
                        let final_image = checked_export_stage(&check, || {
                            apply_export_geometry_and_watermark(processed, &export_settings)
                                .map(|(image, _)| image)
                        })?;
                        ensure_export_not_cancelled(&cancellation_token_clone)?;
                        save_image_with_metadata_checked(
                            &final_image,
                            &output_path,
                            &source_path_str,
                            &export_xmp,
                            &export_settings,
                            &check,
                        )?;
                        ensure_export_not_cancelled(&cancellation_token_clone)?;

                        if export_settings.preserve_timestamps {
                            checked_export_stage(&check, || {
                                set_timestamps_from_exif(Path::new(&source_path_str), &output_path);
                                Ok(())
                            })?;
                        }
                        ensure_export_not_cancelled(&cancellation_token_clone)?;

                        if export_settings.export_masks {
                            export_masks_for_image(
                                &base_image,
                                &js_adjustments,
                                &export_settings,
                                &output_path,
                                &source_path_str,
                                &export_xmp,
                                &context_clone,
                                &state,
                                is_raw,
                                &app_handle_clone,
                                &cancellation_token_clone,
                                source_context,
                                &check,
                            )?;
                        }

                        Ok(())
                    })();

                    if !cancellation_token_clone.load(Ordering::SeqCst) && check().is_ok() {
                        let current_progress =
                            progress_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                        let _ = app_handle_clone.emit(
                            "batch-export-progress",
                            serde_json::json!({
                                "current": current_progress,
                                "total": total_paths,
                                "path": &image_path_str
                            }),
                        );
                    }

                    drop(permit);
                    if cancellation_token_clone.load(Ordering::SeqCst) {
                        Err("Export cancelled".to_string())
                    } else {
                        result
                    }
                })
            });

            join_handles.push(handle);
        }

        // Only the real per-image worker retains a current-source lease now;
        // unrelated independent workers do not extend its lifetime.
        drop(current_request);
        drop(cube_source);
        let mut results = Vec::new();
        for handle in join_handles {
            match handle.await {
                Ok(res) => results.push(res),
                Err(e) => results.push(Err(format!("Thread crashed: {}", e))),
            }
        }

        let errors: Vec<String> = results.into_iter().filter_map(Result::err).collect();
        let error_count = errors.len();
        let export_state = app_handle.state::<AppState>();
        let finalized = finish_export_task(
            &export_state.export_task_token,
            &cancellation_token,
            |cancelled| {
                if cancelled {
                    log::info!("Batch export cancelled and worker cleanup completed");
                    let _ = app_handle.emit("export-cancelled", ());
                    return;
                }

                for error in &errors {
                    log::error!("Export error: {}", error);
                    if total_paths == 1 {
                        let _ = app_handle.emit("export-error", error.clone());
                    }
                }

                if error_count > 0 && total_paths > 1 {
                    let _ = app_handle.emit(
                        "export-error",
                        format!("{error_count} of {total_paths} exports failed"),
                    );
                } else if error_count == 0 {
                    let _ = app_handle.emit(
                        "batch-export-progress",
                        serde_json::json!({ "current": total_paths, "total": total_paths, "path": "" }),
                    );
                    let _ = app_handle.emit("export-complete", ());
                }
            },
        );

        if !finalized {
            log::warn!("Ignoring terminal events from a stale export task");
        }

        if let Some(tx) = completion_tx {
            if error_count > 0 {
                let _ = tx.send(Err(error_count));
            } else {
                let _ = tx.send(Ok(()));
            }
        }
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn export_images(
    paths: Vec<String>,
    output_folder_or_file: String,
    is_explicit_file_path: bool,
    base_origin_folders: Vec<String>,
    export_settings: ExportSettings,
    output_format: String,
    current_edit_path: Option<String>,
    current_edit_adjustments: Option<Value>,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    export_images_impl(
        paths,
        output_folder_or_file,
        is_explicit_file_path,
        base_origin_folders,
        export_settings,
        output_format,
        ExportAdjustmentsMode::UseSidecars {
            active_path: current_edit_path,
            active_adjustments: current_edit_adjustments,
        },
        expected_open_key,
        state,
        app_handle,
        None,
    )
    .await
}

pub async fn run_headless_export(
    session: crate::launch_request::HeadlessExportSession,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    cli_println!("Starting headless export...");
    let state = app_handle.state::<crate::AppState>();

    let source_path = std::path::Path::new(&session.source);
    if !source_path.exists() {
        return Err(format!("Source path does not exist: {}", session.source));
    }

    let mut paths = Vec::new();
    if source_path.is_dir() {
        let images = crate::file_management::list_images_recursive(
            session.source.clone(),
            app_handle.clone(),
        )?;
        paths = images.into_iter().map(|img| img.path).collect();
    } else {
        paths.push(session.source.clone());
    }

    if paths.is_empty() {
        return Err("No supported images found at the source path.".to_string());
    }

    let output_path = std::path::Path::new(&session.output);
    let is_explicit_file_path =
        paths.len() == 1 && output_path.extension().is_some() && !output_path.is_dir();

    if is_explicit_file_path {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create output parent directory: {}", e))?;
        }
    } else {
        std::fs::create_dir_all(&session.output)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    cli_println!("Found {} images to export. Processing...", paths.len());

    let export_settings = ExportSettings {
        jpeg_quality: session.quality,
        tiff_bit_depth: session.tiff_bit_depth,
        resize: None,
        border: None,
        pad: None,
        keep_metadata: session.keep_metadata,
        preserve_timestamps: true,
        strip_gps: false,
        filename_template: None,
        watermark: None,
        export_masks: false,
        preserve_folders: true,
        destination_type: None,
        subfolder: None,
    };

    let mut custom_adjustments = None;
    if let Some(adj_path) = &session.adjustments_override {
        let content = std::fs::read_to_string(adj_path)
            .map_err(|e| format!("Failed to read adjustments file: {}", e))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse adjustments JSON: {}", e))?;
        custom_adjustments = Some(json);
        cli_println!(
            "Loaded custom adjustments to override sidecars from: {}",
            adj_path
        );
    }

    let (tx, rx) = tokio::sync::oneshot::channel();

    let mode = if let Some(adj) = custom_adjustments {
        ExportAdjustmentsMode::GlobalOverride(adj)
    } else {
        ExportAdjustmentsMode::UseSidecars {
            active_path: None,
            active_adjustments: None,
        }
    };

    export_images_impl(
        paths,
        session.output,
        is_explicit_file_path,
        vec![session.source],
        export_settings,
        session.format,
        mode,
        None,
        state.clone(),
        app_handle.clone(),
        Some(tx),
    )
    .await?;

    match rx.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(errors)) => Err(format!("Export completed with {} errors.", errors)),
        Err(_) => Err("Export task panicked or was cancelled.".to_string()),
    }
}

#[tauri::command]
pub fn cancel_export(
    state: tauri::State<AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    match request_export_cancellation(&state.export_task_token, || {
        let _ = app_handle.emit("export-cancelling", ());
    }) {
        ExportCancellationRequest::Requested => {
            log::info!("Export cancellation requested; workers will stop at the next checkpoint");
        }
        ExportCancellationRequest::AlreadyRequested => {
            log::info!("Export cancellation was already requested");
        }
        ExportCancellationRequest::NoActiveTask => {
            return Err("No export task is currently running.".to_string());
        }
    }
    Ok(())
}

fn independent_estimate_source(
    bytes: &[u8],
    path: &str,
    adjustments: &Value,
    settings: &crate::app_settings::AppSettings,
    state: &tauri::State<AppState>,
    check: &impl Fn() -> Result<(), String>,
) -> Result<IndependentExportSource, String> {
    let policy = crate::image_loader::AccurateRawPolicy::new(
        1,
        rawler::imgop::sensor::xtrans::external::XTransPasses::One.scratch_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let decision = checked_export_stage(check, || {
        crate::image_loader::prepare_estimate_appearance(
            bytes,
            path,
            adjustments,
            settings,
            &state.profile_registry,
            policy,
            None,
        )
        .map_err(|e| e.to_string())
    })?;
    match decision {
        crate::image_loader::PreparedAppearanceDecision::Prepared(open) => {
            let source = checked_export_stage(check, || finish_prepared_export(open, None))?;
            Ok(IndependentExportSource::Accurate(source))
        }
        crate::image_loader::PreparedAppearanceDecision::DecodedLegacy(owner) => {
            let as_shot = owner.as_shot();
            let (image, raw_scale) = checked_export_stage(check, || {
                owner.render_estimate_fast(None).map_err(|e| e.to_string())
            })?;
            Ok(IndependentExportSource::Legacy {
                image,
                adjustments: adjustments.clone(),
                as_shot,
                raw_scale,
            })
        }
        crate::image_loader::PreparedAppearanceDecision::LegacyRequired(_reason) => {
            let image = checked_export_stage(check, || {
                load_base_image_from_bytes(bytes, path, true, settings, None)
                    .map_err(|e| e.to_string())
            })?;
            let raw_scale = if is_raw_file(path) {
                checked_export_stage(check, || {
                    Ok(crate::raw_processing::get_fast_demosaic_scale_factor(
                        bytes,
                        image.width(),
                        image.height(),
                    ))
                })?
            } else {
                1.0
            };
            let as_shot = checked_export_stage(check, || Ok(as_shot(path)))?;
            Ok(IndependentExportSource::Legacy {
                image,
                adjustments: adjustments.clone(),
                as_shot,
                raw_scale,
            })
        }
    }
}

fn estimate_rendered_export_size(
    processed_preview: &DynamicImage,
    full_size: (u32, u32),
    output_format: &str,
    export_settings: &ExportSettings,
    check: &impl Fn() -> Result<(), String>,
) -> Result<usize, String> {
    let preview_bytes = checked_export_stage(check, || {
        encode_image_to_bytes(
            processed_preview,
            output_format,
            export_settings.jpeg_quality,
            export_settings.tiff_bit_depth,
        )
    })?;
    let preview_byte_size = preview_bytes.len();
    let (full_w, full_h) = full_size;
    let (final_full_w, final_full_h) = compute_export_output_size(full_w, full_h, export_settings);
    let (processed_preview_w, processed_preview_h) = processed_preview.dimensions();
    let pixel_ratio = if processed_preview_w > 0 && processed_preview_h > 0 {
        (final_full_w as f64 * final_full_h as f64)
            / (processed_preview_w as f64 * processed_preview_h as f64)
    } else {
        1.0
    };
    Ok((preview_byte_size as f64 * pixel_ratio) as usize)
}

#[allow(clippy::too_many_arguments)]
fn estimate_legacy_independent_image(
    original_image: DynamicImage,
    raw_scale_factor: f32,
    mut js_adjustments: Value,
    settings: &crate::app_settings::AppSettings,
    context: &GpuContext,
    state: &tauri::State<AppState>,
    _app_handle: &tauri::AppHandle,
    source_path_str: &str,
    is_raw: bool,
    captured_as_shot: crate::as_shot::AsShot,
    output_format: &str,
    export_settings: &ExportSettings,
) -> Result<usize, String> {
    const ESTIMATE_DIM: u32 = 1280;
    if let Some(crop_val) = js_adjustments.get_mut("crop")
        && let Ok(c) = serde_json::from_value::<Crop>(crop_val.clone())
    {
        *crop_val = serde_json::to_value(Crop {
            x: c.x * raw_scale_factor as f64,
            y: c.y * raw_scale_factor as f64,
            width: c.width * raw_scale_factor as f64,
            height: c.height * raw_scale_factor as f64,
        })
        .unwrap_or(serde_json::Value::Null);
    }

    let (transformed_shrunk_res, unscaled_crop_offset) =
        apply_all_transformations(Cow::Borrowed(&original_image), &js_adjustments);
    let (shrunk_w, shrunk_h) = transformed_shrunk_res.dimensions();

    let preview_base = if shrunk_w > ESTIMATE_DIM || shrunk_h > ESTIMATE_DIM {
        downscale_f32_image(transformed_shrunk_res.as_ref(), ESTIMATE_DIM, ESTIMATE_DIM)
    } else {
        transformed_shrunk_res.into_owned()
    };

    let (preview_w, preview_h) = preview_base.dimensions();
    let gpu_scale = if shrunk_w > 0 {
        preview_w as f32 / shrunk_w as f32
    } else {
        1.0
    };
    let total_scale = gpu_scale * raw_scale_factor;

    let mask_definitions: Vec<MaskDefinition> = js_adjustments
        .get("masks")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default();
    let scaled_crop_offset = (
        unscaled_crop_offset.0 * gpu_scale,
        unscaled_crop_offset.1 * gpu_scale,
    );

    let mask_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = mask_definitions
        .iter()
        .filter_map(|def| {
            get_cached_or_generate_mask(
                &state,
                &source_path_str,
                def,
                preview_w,
                preview_h,
                total_scale,
                scaled_crop_offset,
                &js_adjustments,
            )
        })
        .collect();

    let render = crate::image_processing::resolve_render_options(settings, is_raw);
    let mut all_adjustments =
        get_all_adjustments_from_json(&js_adjustments, is_raw, captured_as_shot, render);
    all_adjustments.global.show_clipping = 0;

    let lut = js_adjustments["lutPath"]
        .as_str()
        .and_then(|p| get_or_load_lut(&state, p).ok());
    let mut renderer = crate::gpu_processing::IsolatedCreativeRenderer::new(context);
    let processed_preview = renderer.render(
        &preview_base,
        RenderRequest {
            adjustments: all_adjustments,
            mask_bitmaps: &mask_bitmaps,
            lut,
            roi: None,
            dcp_textures: None,
        },
        render_output_precision(output_format, export_settings),
    )?;

    let preview_bytes = encode_image_to_bytes(
        &processed_preview,
        &output_format,
        export_settings.jpeg_quality,
        export_settings.tiff_bit_depth,
    )?;
    let single_image_estimated_size = preview_bytes.len();

    let full_w = (shrunk_w as f32 / raw_scale_factor).round() as u32;
    let full_h = (shrunk_h as f32 / raw_scale_factor).round() as u32;

    let (final_full_w, final_full_h) = compute_export_output_size(full_w, full_h, &export_settings);

    let (processed_preview_w, processed_preview_h) = processed_preview.dimensions();
    let pixel_ratio = if processed_preview_w > 0 && processed_preview_h > 0 {
        (final_full_w as f64 * final_full_h as f64)
            / (processed_preview_w as f64 * processed_preview_h as f64)
    } else {
        1.0
    };

    Ok((single_image_estimated_size as f64 * pixel_ratio) as usize)
}

#[tauri::command]
pub async fn estimate_export_sizes(
    paths: Vec<String>,
    export_settings: ExportSettings,
    output_format: String,
    current_edit_path: Option<String>,
    current_edit_adjustments: Option<Value>,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<usize, String> {
    if output_format.eq_ignore_ascii_case("cube") || paths.is_empty() {
        if let Some(key) = expected_open_key.as_ref() {
            let path = current_edit_path
                .as_deref()
                .filter(|path| paths.first().is_some_and(|first| first == path))
                .ok_or("Keyed estimate requires the exact current virtual path")?;
            let source = state
                .capture_consumer_source(Some(key), Some(path))
                .map_err(|e| e.to_string())?;
            source.check().map_err(|e| e.to_string())?;
        }
        return Ok(if paths.is_empty() {
            0
        } else {
            1_050_000 * paths.len()
        });
    }
    let first_path = &paths[0];
    let (source_path, sidecar_path) = parse_virtual_path(first_path);
    let source_path_str = source_path.to_string_lossy().to_string();
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let is_current_edit = current_edit_path.as_deref() == Some(first_path.as_str());
    if expected_open_key.is_some() && (!is_current_edit || current_edit_adjustments.is_none()) {
        return Err(
            "Keyed estimate requires the exact current virtual path and adjustments".into(),
        );
    }
    let request = if is_current_edit {
        Some(state.capture_consumer_request(
            expected_open_key.as_ref(),
            Some(first_path.as_str()),
            current_edit_adjustments.clone(),
            settings.clone(),
        )?)
    } else {
        None
    };
    let check = || {
        if let Some(request) = &request {
            request.check()?;
        }
        Ok(())
    };
    let context = checked_export_stage(&check, || get_or_init_gpu_context(&state, &app_handle))?;
    let is_raw = is_raw_file(&source_path_str);
    let single_image_extrapolated_size = if let Some(request) = &request {
        if request.is_keyed() {
            request.with_full_composition(|composed| {
                let processed = render_composed_export(
                    composed,
                    &context,
                    &export_settings,
                    &output_format,
                    &check,
                )?;
                estimate_rendered_export_size(
                    &processed,
                    composed.pixels().dimensions(),
                    &output_format,
                    &export_settings,
                    &check,
                )
            })?
        } else if current_edit_adjustments.is_some() {
            checked_export_stage(&check, || {
                let settings = request.settings();
                Ok({
                    let loaded_image = request.loaded().clone();
                    let mut adjustments_clone = current_edit_adjustments.clone().unwrap();
                    hydrate_adjustments(&state, &mut adjustments_clone);

                    let new_transform_hash = calculate_transform_hash(&adjustments_clone);
                    let cached_preview_lock = state.cached_preview.lock().unwrap();
                    let preview_dim = settings.editor_preview_resolution.unwrap_or(1920);

                    let (preview_image, scale, unscaled_crop_offset) =
                        if let Some(cached) = &*cached_preview_lock {
                            if cached.transform_hash == new_transform_hash
                                && cached.preview_dim == preview_dim
                            {
                                let img = Arc::clone(&cached.image);
                                let s = cached.scale;
                                let offset = cached.unscaled_crop_offset;
                                drop(cached_preview_lock);
                                let owned_img =
                                    Arc::try_unwrap(img).unwrap_or_else(|arc| (*arc).clone());
                                (owned_img, s, offset)
                            } else {
                                drop(cached_preview_lock);
                                generate_transformed_preview(
                                    &state,
                                    &loaded_image,
                                    &adjustments_clone,
                                    preview_dim,
                                )?
                            }
                        } else {
                            drop(cached_preview_lock);
                            generate_transformed_preview(
                                &state,
                                &loaded_image,
                                &adjustments_clone,
                                preview_dim,
                            )?
                        };

                    let (img_w, img_h) = preview_image.dimensions();
                    let mask_definitions: Vec<MaskDefinition> = adjustments_clone
                        .get("masks")
                        .and_then(|m| serde_json::from_value(m.clone()).ok())
                        .unwrap_or_default();

                    let scaled_crop_offset = (
                        unscaled_crop_offset.0 * scale,
                        unscaled_crop_offset.1 * scale,
                    );

                    let mask_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = mask_definitions
                        .iter()
                        .filter_map(|def| {
                            get_cached_or_generate_mask(
                                &state,
                                &loaded_image.path,
                                def,
                                img_w,
                                img_h,
                                scale,
                                scaled_crop_offset,
                                &adjustments_clone,
                            )
                        })
                        .collect();

                    let render =
                        crate::image_processing::resolve_render_options(request.settings(), is_raw);
                    let mut all_adjustments = get_all_adjustments_from_json(
                        &adjustments_clone,
                        is_raw,
                        loaded_image.as_shot,
                        render,
                    );
                    all_adjustments.global.show_clipping = 0;

                    let lut = adjustments_clone["lutPath"]
                        .as_str()
                        .and_then(|p| get_or_load_lut(&state, p).ok());
                    let unique_hash =
                        calculate_full_job_hash(&loaded_image.path, &adjustments_clone)
                            .wrapping_add(1);

                    let processed_preview = process_and_get_dynamic_image_with_precision(
                        &context,
                        &state,
                        &preview_image,
                        unique_hash,
                        RenderRequest {
                            adjustments: all_adjustments,
                            mask_bitmaps: &mask_bitmaps,
                            lut,
                            roi: None,
                            dcp_textures: None,
                        },
                        "estimate_export_size",
                        render_output_precision(&output_format, &export_settings),
                    )?;

                    let preview_bytes = encode_image_to_bytes(
                        &processed_preview,
                        &output_format,
                        export_settings.jpeg_quality,
                        export_settings.tiff_bit_depth,
                    )?;
                    let preview_byte_size = preview_bytes.len();

                    let (transformed_full_res, _) =
                        apply_all_transformations(&loaded_image.image, &adjustments_clone);
                    let (full_w, full_h) = transformed_full_res.dimensions();

                    let (final_full_w, final_full_h) =
                        compute_export_output_size(full_w, full_h, &export_settings);

                    let (processed_preview_w, processed_preview_h) = processed_preview.dimensions();
                    let pixel_ratio = if processed_preview_w > 0 && processed_preview_h > 0 {
                        (final_full_w as f64 * final_full_h as f64)
                            / (processed_preview_w as f64 * processed_preview_h as f64)
                    } else {
                        1.0
                    };

                    (preview_byte_size as f64 * pixel_ratio) as usize
                })
            })?
        } else {
            return Err("Current-edit estimate requires captured adjustments".into());
        }
    } else {
        // Independent file estimates stay usable during a keyed editor open.
        let metadata = checked_export_stage(&check, || {
            Ok(crate::exif_processing::load_sidecar(&sidecar_path))
        })?;
        let source = with_export_file_bytes(&source_path, &check, |bytes| {
            independent_estimate_source(
                bytes,
                &source_path_str,
                &metadata.adjustments,
                &settings,
                &state,
                &check,
            )
        })?;
        match source {
            IndependentExportSource::Accurate(source) => {
                source.with_composition(&check, |composed| {
                    let processed = render_composed_export(
                        composed,
                        &context,
                        &export_settings,
                        &output_format,
                        &check,
                    )?;
                    estimate_rendered_export_size(
                        &processed,
                        composed.pixels().dimensions(),
                        &output_format,
                        &export_settings,
                        &check,
                    )
                })?
            }
            IndependentExportSource::Legacy {
                image,
                adjustments,
                as_shot,
                raw_scale,
            } => checked_export_stage(&check, || {
                estimate_legacy_independent_image(
                    image,
                    raw_scale,
                    adjustments,
                    &settings,
                    &context,
                    &state,
                    &app_handle,
                    &source_path_str,
                    is_raw,
                    as_shot,
                    &output_format,
                    &export_settings,
                )
            })?,
        }
    };
    check()?;
    Ok(single_image_extrapolated_size * paths.len())
}

#[cfg(test)]
mod prepared_export_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn captured_metadata_and_render_options_never_evaluate_fallbacks() {
        let mut as_shot = crate::as_shot::AsShot::reference();
        as_shot.exposure.dng_baseline = 1.25;
        let render = RenderOptions {
            tonemapper_override: Some(1),
            ..Default::default()
        };
        let source = ExportSourceContext::Captured { as_shot, render };
        assert_eq!(
            source
                .as_shot(|| panic!("must not reopen path"))
                .exposure
                .dng_baseline,
            1.25
        );
        assert_eq!(
            source
                .render_options(|| panic!("must not reread settings"))
                .tonemapper_override,
            Some(1)
        );
    }

    #[test]
    fn legacy_metadata_resolution_remains_lazy_and_once() {
        let calls = Cell::new(0);
        let source = ExportSourceContext::Legacy;
        assert_eq!(calls.get(), 0);
        let _ = source.render_options(|| {
            calls.set(calls.get() + 1);
            RenderOptions::default()
        });
        let _ = source.as_shot(|| {
            calls.set(calls.get() + 1);
            crate::as_shot::AsShot::reference()
        });
        assert_eq!(calls.get(), 2);
    }
}

#[cfg(test)]
mod consumer_export_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn export_target_requires_exact_virtual_path_and_keyed_inputs() {
        let path = "/A.raf?vc=1".to_string();
        let mode = ExportAdjustmentsMode::UseSidecars {
            active_path: Some(path.clone()),
            active_adjustments: Some(serde_json::json!({"exposure": 0.25})),
        };
        assert_eq!(
            current_edit_export_target(&[path.clone()], &mode, true).unwrap(),
            Some(path)
        );
        assert!(current_edit_export_target(&["/A.raf?vc=2".into()], &mode, true).is_err());
        assert!(current_edit_export_target(&["/A.raf".into()], &mode, true).is_err());
        assert_eq!(
            current_edit_export_target(&["/B.raf".into()], &mode, false).unwrap(),
            None
        );
        let absent = ExportAdjustmentsMode::UseSidecars {
            active_path: Some("/A.raf?vc=1".into()),
            active_adjustments: None,
        };
        assert!(current_edit_export_target(&["/A.raf?vc=1".into()], &absent, true).is_err());
        let independent = ExportAdjustmentsMode::GlobalOverride(serde_json::json!({}));
        assert_eq!(
            current_edit_export_target(&["/B.raf".into()], &independent, false).unwrap(),
            None
        );
        assert!(current_edit_export_target(&["/B.raf".into()], &independent, true).is_err());
    }

    #[test]
    fn export_stage_postchecks_success_and_error_and_refuses_stale_entry() {
        let current = Cell::new(true);
        let calls = Cell::new(0);
        let check = || {
            if current.get() {
                Ok(())
            } else {
                Err("source changed".into())
            }
        };
        let error = checked_export_stage(&check, || {
            calls.set(calls.get() + 1);
            current.set(false);
            Err::<(), _>("actual stage error".into())
        })
        .unwrap_err();
        assert_eq!(error, "source changed");
        assert_eq!(calls.get(), 1);
        assert!(
            checked_export_stage(&check, || {
                calls.set(99);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(calls.get(), 1);
        current.set(true);
        let error = checked_export_stage(&check, || {
            current.set(false);
            Ok(42)
        })
        .unwrap_err();
        assert_eq!(error, "source changed");
        current.set(true);
        assert_eq!(checked_export_stage(&check, || Ok(42)).unwrap(), 42);
        assert_eq!(
            checked_export_stage(&check, || Err::<(), _>("actual stage error".into())).unwrap_err(),
            "actual stage error"
        );
    }

    #[test]
    fn standalone_export_borrows_one_real_base_and_keeps_full_oriented_geometry() {
        let owner = crate::image_loader::composition_test_owner();
        let creative = crate::open_creative_snapshot::capture_creative_assets(
            owner.adjustments(),
            owner.settings(),
            None,
        )
        .unwrap();
        let appearance = owner.prepare_independent_export(None).unwrap();
        {
            let first = appearance.base();
            let second = appearance.base();
            assert!(std::ptr::eq(first.pixels(), second.pixels()));
            assert!(Arc::ptr_eq(
                first.retained_source(),
                second.retained_source()
            ));
            assert_eq!(first.pixels().dimensions(), (520, 512));
            assert!(matches!(
                first.kind(),
                crate::image_loader::PreparedBaseKind::NativeXtransWorking(
                    crate::image_loader::AppearancePass::AccurateOne
                )
            ));
        }
        let source = PreparedExportSource {
            appearance,
            creative,
            cancel: None,
        };
        source
            .with_composition(&|| Ok(()), |composed| {
                assert_eq!(composed.pixels().dimensions(), (520, 512));
                assert_eq!(composed.scale_for_gpu(), 1.0);
                assert!(matches!(
                    composed.kind(),
                    crate::image_loader::PreparedBaseKind::NativeXtransWorking(
                        crate::image_loader::AppearancePass::AccurateOne
                    )
                ));
                assert_eq!(composed.request().adjustments.mask_count, 0);
                assert!(composed.request().mask_bitmaps.is_empty());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn export_geometry_and_precision_keep_full_output_and_mask_uniform_selection() {
        let mut settings: ExportSettings = serde_json::from_value(serde_json::json!({
            "jpegQuality": 90, "tiffBitDepth": 16, "resize": null,
            "keepMetadata": true, "stripGps": false,
            "filenameTemplate": null, "watermark": null,
        }))
        .unwrap();
        assert_eq!(
            compute_export_output_size(4000, 3000, &settings),
            (4000, 3000)
        );
        assert!(matches!(
            render_output_precision("tiff", &settings),
            RenderOutputPrecision::SixteenBit
        ));
        settings.tiff_bit_depth = TiffBitDepth::Eight;
        assert!(matches!(
            render_output_precision("tiff", &settings),
            RenderOutputPrecision::EightBit
        ));
        settings.resize = Some(ResizeOptions {
            mode: ResizeMode::LongEdge,
            value: 2000,
            dont_enlarge: true,
        });
        assert_eq!(
            compute_export_output_size(4000, 3000, &settings),
            (2000, 1500)
        );
        let mut all = AllAdjustments::default();
        all.mask_count = 2;
        all.mask_adjustments[0].exposure = 0.25;
        all.mask_adjustments[1].exposure = 0.75;
        let single = build_single_mask_adjustments(&all, 1);
        assert_eq!(single.mask_count, 1);
        assert_eq!(single.mask_adjustments[0].exposure, 0.75);
        assert_eq!(all.mask_adjustments[0].exposure, 0.25);
    }
}
