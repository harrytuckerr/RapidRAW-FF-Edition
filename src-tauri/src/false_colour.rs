//! Fixed first candidate: linear-srgb-edge-median-v1.
//!
//! Independently authored from the frozen mathematical contract; no external
//! implementation was copied. This module has no active caller. Future native
//! producers must supply unencoded linear sRGB/Rec.709, after calibration or the
//! single selected-profile working conversion. Pixels cannot attest primaries.
//! Eligibility, once-only placement, caches and editor/export wiring belong to
//! the later producer grant; do not call this on a stand-in or filtered output.
//!
//! One immutable input and one output buffer, disjoint rows on the existing Rayon pool, one pass. The update
//! preserves linear Rec.709 luminance in real arithmetic, not encoded luminance
//! or exact f32 luma at arbitrary magnitude. No new clipping or gamma is used.
//! Existing upstream clipping/profile restrictions are not repaired here.

use crate::raw_processing::RawCancel;
use image::{DynamicImage, ImageBuffer};
use rayon::prelude::*;
use std::fmt;
use std::sync::atomic::Ordering;

const WR: f32 = 0.2126;
const WG: f32 = 0.7152;
const WB: f32 = 0.0722;
const EDGE_RATIO: f32 = 1.0 / 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FcsError {
    UnsupportedFormat,
    InvalidGeometry,
    NonFiniteInput,
    ArithmeticOverflow,
    AllocationFailed,
    Cancelled,
}

impl fmt::Display for FcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedFormat => "False-colour input must be RGB32F or RGBA32F",
            Self::InvalidGeometry => "Invalid false-colour image dimensions or storage",
            Self::NonFiniteInput => "Nonfinite false-colour input",
            Self::ArithmeticOverflow => "False-colour arithmetic overflow",
            Self::AllocationFailed => "False-colour output allocation failed",
            Self::Cancelled => "False-colour processing cancelled",
        })
    }
}

impl std::error::Error for FcsError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Checkpoint {
    Entry,
    ValidationRow,
    BeforeAllocation,
    OutputRow,
    Final,
}

struct Work<'a> {
    cancel: &'a RawCancel,
    #[cfg(test)]
    hook: Option<&'a dyn Fn(Checkpoint)>,
    #[cfg(test)]
    force_reserve_failure: bool,
}

impl<'a> Work<'a> {
    fn new(cancel: &'a RawCancel) -> Self {
        Self {
            cancel,
            #[cfg(test)]
            hook: None,
            #[cfg(test)]
            force_reserve_failure: false,
        }
    }

    fn check(&self, _point: Checkpoint) -> Result<(), FcsError> {
        // The private test hook can pause, but cannot decide cancellation or
        // return a fabricated result. The real atomic check always follows.
        #[cfg(test)]
        if let Some(hook) = self.hook {
            hook(_point);
        }
        check_cancel(self.cancel)
    }

    fn output(&self, len: usize) -> Result<Vec<f32>, FcsError> {
        let reserve = len;
        // Capacity overflow deterministically exercises the same std reserve
        // error mapping without attempting a giant successful allocation.
        #[cfg(test)]
        let reserve = if self.force_reserve_failure {
            usize::MAX
        } else {
            reserve
        };
        let mut output = Vec::new();
        output
            .try_reserve_exact(reserve)
            .map_err(|_| FcsError::AllocationFailed)?;
        output.resize(len, 0.0);
        Ok(output)
    }
}

fn check_cancel(cancel: &RawCancel) -> Result<(), FcsError> {
    if let Some((tracker, generation)) = cancel
        && tracker.load(Ordering::SeqCst) != *generation
    {
        return Err(FcsError::Cancelled);
    }
    Ok(())
}

fn checked_len(width: u32, height: u32, channels: usize) -> Result<usize, FcsError> {
    if width == 0 || height == 0 || !matches!(channels, 3 | 4) {
        return Err(FcsError::InvalidGeometry);
    }
    let width = usize::try_from(width).map_err(|_| FcsError::InvalidGeometry)?;
    let height = usize::try_from(height).map_err(|_| FcsError::InvalidGeometry)?;
    width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(channels))
        .filter(|n| {
            n.checked_mul(std::mem::size_of::<f32>())
                .is_some_and(|b| b <= isize::MAX as usize)
        })
        .ok_or(FcsError::InvalidGeometry)
}

#[derive(Clone, Copy)]
struct Features {
    y: f32,
    dr: f32,
    db: f32,
}

fn features(pixel: &[f32]) -> Result<Features, FcsError> {
    let r = WR * pixel[0];
    let g = WG * pixel[1];
    let b = WB * pixel[2];
    let rg = r + g;
    let y = rg + b;
    let dr = pixel[0] - pixel[1];
    let db = pixel[2] - pixel[1];
    if [r, g, b, rg, y, dr, db].iter().any(|n| !n.is_finite()) {
        return Err(FcsError::ArithmeticOverflow);
    }
    Ok(Features { y, dr, db })
}

fn accepts_edge(center_y: f32, neighbor_y: f32) -> Result<bool, FcsError> {
    let difference = neighbor_y - center_y;
    let scale = 1.0_f32.max(center_y.abs()).max(neighbor_y.abs());
    let threshold = EDGE_RATIO * scale;
    if [center_y, neighbor_y, difference, scale, threshold]
        .iter()
        .any(|n| !n.is_finite())
    {
        return Err(FcsError::ArithmeticOverflow);
    }
    Ok(difference.abs() <= threshold)
}

fn filtered_rgb(
    source: &[f32],
    width: usize,
    height: usize,
    channels: usize,
    x: usize,
    y: usize,
) -> Result<[f32; 3], FcsError> {
    // Geometry/storage were checked before this private helper is reached.
    let offset = (y * width + x) * channels;
    let pixel = &source[offset..offset + channels];
    let center = features(pixel)?;
    let rows = [y.saturating_sub(1), y, (y + 1).min(height - 1)];
    let cols = [x.saturating_sub(1), x, (x + 1).min(width - 1)];
    let mut reds = [0.0_f32; 9];
    let mut blues = [0.0_f32; 9];
    let mut i = 0;
    for row in rows {
        for col in cols {
            let start = (row * width + col) * channels;
            let neighbor = features(&source[start..start + channels])?;
            let accepted = accepts_edge(center.y, neighbor.y)?;
            reds[i] = if accepted { neighbor.dr } else { center.dr };
            blues[i] = if accepted { neighbor.db } else { center.db };
            i += 1;
        }
    }
    reds.sort_unstable_by(f32::total_cmp);
    blues.sort_unstable_by(f32::total_cmp);
    if reds[4] == center.dr && blues[4] == center.db {
        return Ok([pixel[0], pixel[1], pixel[2]]);
    }
    let a = reds[4] - center.dr;
    let b = blues[4] - center.db;
    let ra = WR * a;
    let bb = WB * b;
    let sum = ra + bb;
    let delta_g = -sum;
    let red_base = pixel[0] + delta_g;
    let red = red_base + a;
    let green = pixel[1] + delta_g;
    let blue_base = pixel[2] + delta_g;
    let blue = blue_base + b;
    if [
        a, b, ra, bb, sum, delta_g, red_base, red, green, blue_base, blue,
    ]
    .iter()
    .any(|n| !n.is_finite())
    {
        return Err(FcsError::ArithmeticOverflow);
    }
    Ok([red, green, blue])
}

fn filter_output_row(
    source: &[f32],
    row: &mut [f32],
    width: usize,
    height: usize,
    channels: usize,
    y: usize,
) -> Result<(), FcsError> {
    for x in 0..width {
        let source_offset = (y * width + x) * channels;
        let output_offset = x * channels;
        let rgb = filtered_rgb(source, width, height, channels, x, y)?;
        row[output_offset..output_offset + 3].copy_from_slice(&rgb);
        if channels == 4 {
            row[output_offset + 3] = source[source_offset + 3];
        }
    }
    Ok(())
}

/// Consume one unencoded linear-sRGB image and return one complete filtered
/// image of the same float format. No inference of colour space, camera type,
/// profile eligibility or current generation is made from pixel bytes.
///
/// Input stays immutable while the separate output is computed. Cancellation
/// or any failure drops that local result; a later native owner must still
/// reject stale publication under its own commit guard. This is no hard OOM,
/// process-abort or GPU-completion guarantee.
pub(crate) fn suppress_linear_srgb_v1(
    image: DynamicImage,
    cancel: &RawCancel,
) -> Result<DynamicImage, FcsError> {
    suppress_with_work(image, &Work::new(cancel))
}

fn suppress_with_work(image: DynamicImage, work: &Work<'_>) -> Result<DynamicImage, FcsError> {
    work.check(Checkpoint::Entry)?;
    let (width, height, channels, source) = match image {
        DynamicImage::ImageRgb32F(image) => (image.width(), image.height(), 3, image.into_raw()),
        DynamicImage::ImageRgba32F(image) => (image.width(), image.height(), 4, image.into_raw()),
        _ => return Err(FcsError::UnsupportedFormat),
    };
    let len = checked_len(width, height, channels)?;
    if source.len() != len {
        return Err(FcsError::InvalidGeometry);
    }
    let row_len = width as usize * channels;
    for row in source.chunks_exact(row_len) {
        work.check(Checkpoint::ValidationRow)?;
        if row.iter().any(|n| !n.is_finite()) {
            return Err(FcsError::NonFiniteInput);
        }
    }
    work.check(Checkpoint::BeforeAllocation)?;
    let mut output = work.output(len)?;
    let serial_hook = {
        #[cfg(test)]
        {
            work.hook.is_some()
        }
        #[cfg(not(test))]
        {
            false
        }
    };
    if serial_hook {
        for y in 0..height as usize {
            work.check(Checkpoint::OutputRow)?;
            let row_start = y * row_len;
            filter_output_row(
                &source,
                &mut output[row_start..row_start + row_len],
                width as usize,
                height as usize,
                channels,
                y,
            )?;
        }
    } else {
        let cancel = work.cancel;
        output
            .par_chunks_exact_mut(row_len)
            .enumerate()
            .try_for_each(|(y, row)| {
                check_cancel(cancel)?;
                filter_output_row(&source, row, width as usize, height as usize, channels, y)
            })?;
    }
    let image = if channels == 3 {
        DynamicImage::ImageRgb32F(
            ImageBuffer::from_raw(width, height, output).ok_or(FcsError::InvalidGeometry)?,
        )
    } else {
        DynamicImage::ImageRgba32F(
            ImageBuffer::from_raw(width, height, output).ok_or(FcsError::InvalidGeometry)?,
        )
    };
    work.check(Checkpoint::Final)?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, Rgb32FImage, Rgba, Rgba32FImage};
    use std::cell::Cell;
    use std::sync::{Arc, atomic::AtomicUsize, mpsc};
    use std::time::Duration;

    fn rgb(width: u32, height: u32, pixel: [f32; 3]) -> DynamicImage {
        DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(width, height, Rgb(pixel)))
    }

    fn rgb_data(image: &DynamicImage) -> &[f32] {
        match image {
            DynamicImage::ImageRgb32F(image) => image.as_raw(),
            _ => panic!("expected RGB32F"),
        }
    }

    fn impulse(width: u32, height: u32, positions: &[(u32, u32)]) -> DynamicImage {
        let mut image = Rgb32FImage::from_pixel(width, height, Rgb([0.5; 3]));
        for &(x, y) in positions {
            image.put_pixel(x, y, Rgb([0.58235, 0.48235, 0.43235]));
        }
        DynamicImage::ImageRgb32F(image)
    }

    fn close_rgb(actual: &[f32], expected: &[f32], tolerance: f32) {
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            assert!(
                a.is_finite() && (*a - *b).abs() <= tolerance,
                "{a} versus {b}"
            );
        }
    }

    fn ycc(pixel: &[f32]) -> [f64; 3] {
        let r = f64::from(pixel[0]);
        let g = f64::from(pixel[1]);
        let b = f64::from(pixel[2]);
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        [y, b - y, r - y]
    }

    fn assert_colour_controls(
        reference: &DynamicImage,
        candidate: &DynamicImage,
        endpoints: [usize; 2],
    ) {
        let before = rgb_data(reference);
        let after = rgb_data(candidate);
        close_rgb(after, before, 1e-6);
        for (a, b) in before.chunks_exact(3).zip(after.chunks_exact(3)) {
            let a = ycc(a);
            let b = ycc(b);
            assert!((a[0] - b[0]).abs() <= 1e-5);
            if a[1].hypot(a[2]) > 0.05 {
                assert!(b[1].hypot(b[2]) > 0.0);
                let angle = b[2].atan2(b[1]) - a[2].atan2(a[1]);
                assert!(angle.sin().atan2(angle.cos()).abs().to_degrees() <= 1.0);
            }
        }
        let distance = |data: &[f32]| {
            let a = ycc(&data[endpoints[0] * 3..endpoints[0] * 3 + 3]);
            let b = ycc(&data[endpoints[1] * 3..endpoints[1] * 3 + 3]);
            (a[1] - b[1]).hypot(a[2] - b[2])
        };
        assert!(distance(after) >= 0.97 * distance(before));
    }

    fn field() -> DynamicImage {
        DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(7, 5, |x, y| {
            let dr = (((x * 3 + y * 5) % 7) as i32 - 3) as f32 * 0.04;
            let db = (((x * 5 + y * 2) % 5) as i32 - 2) as f32 * 0.03;
            let g = (0.5 - 0.2126 * dr) - 0.0722 * db;
            Rgb([g + dr, g, g + db])
        }))
    }

    // Small direct reference for finite test vectors, written separately from
    // the production helpers. Reverse visitation always reads original pixels.
    fn reverse_reference(input: &Rgb32FImage) -> Rgb32FImage {
        let w = input.width();
        let h = input.height();
        let mut output = Rgb32FImage::new(w, h);
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let p = input.get_pixel(x, y).0;
                let py = (0.2126 * p[0] + 0.7152 * p[1]) + 0.0722 * p[2];
                let mut differences = Vec::with_capacity(9);
                for dy in -1_i64..=1 {
                    for dx in -1_i64..=1 {
                        let nx = (i64::from(x) + dx).clamp(0, i64::from(w) - 1) as u32;
                        let ny = (i64::from(y) + dy).clamp(0, i64::from(h) - 1) as u32;
                        let q = input.get_pixel(nx, ny).0;
                        let qy = (0.2126 * q[0] + 0.7152 * q[1]) + 0.0722 * q[2];
                        let q = if (qy - py).abs() <= 1.0_f32.max(py.abs()).max(qy.abs()) / 32.0 {
                            q
                        } else {
                            p
                        };
                        differences.push([q[0] - q[1], q[2] - q[1]]);
                    }
                }
                let mut r: Vec<f32> = differences.iter().map(|v| v[0]).collect();
                let mut b: Vec<f32> = differences.iter().map(|v| v[1]).collect();
                r.sort_by(|a, b| a.partial_cmp(b).unwrap());
                b.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let dr = r[4] - (p[0] - p[1]);
                let db = b[4] - (p[2] - p[1]);
                let result = if dr == 0.0 && db == 0.0 {
                    p
                } else {
                    let dg = -(0.2126 * dr + 0.0722 * db);
                    [(p[0] + dg) + dr, p[1] + dg, (p[2] + dg) + db]
                };
                output.put_pixel(x, y, Rgb(result));
            }
        }
        output
    }

    #[test]
    fn k01_flat_bits_formats_and_hdr() {
        for pixel in [
            [0.0; 3],
            [0.2; 3],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [-0.2, 0.3, 1.5],
            [2.0; 3],
            [-0.0; 3],
        ] {
            for input in [
                rgb(5, 3, pixel),
                DynamicImage::ImageRgba32F(Rgba32FImage::from_pixel(
                    5,
                    3,
                    Rgba([pixel[0], pixel[1], pixel[2], -0.0]),
                )),
            ] {
                let bytes = input.as_bytes().to_vec();
                let color = input.color();
                let output = suppress_linear_srgb_v1(input, &None).unwrap();
                assert_eq!((output.width(), output.height()), (5, 3));
                assert_eq!(output.color(), color);
                assert_eq!(output.as_bytes(), bytes);
            }
        }
    }

    #[test]
    fn k02_fixed_isoluminant_impulse() {
        let input = impulse(5, 5, &[(2, 2)]);
        let before = rgb_data(&input).to_vec();
        let output = suppress_linear_srgb_v1(input, &None).unwrap();
        close_rgb(rgb_data(&output), &[0.5; 75], 1e-6);
        for i in 0..25 {
            if i != 12 {
                assert_eq!(
                    &rgb_data(&output)[i * 3..i * 3 + 3],
                    &before[i * 3..i * 3 + 3]
                );
            }
        }
    }

    #[test]
    fn k03_luma_edge_rejects_chroma_replacement() {
        let mut input = Rgb32FImage::from_pixel(5, 5, Rgb([0.5; 3]));
        input.put_pixel(2, 2, Rgb([1.0, 0.0, 0.0]));
        let input = DynamicImage::ImageRgb32F(input);
        let before = input.as_bytes().to_vec();
        let output = suppress_linear_srgb_v1(input, &None).unwrap();
        assert_eq!(output.as_bytes(), before);
    }

    #[test]
    fn k04_legitimate_straight_colour_controls() {
        let edge = DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(8, 5, |x, _| {
            Rgb(if x < 4 {
                [0.58235, 0.48235, 0.43235]
            } else {
                [0.41765, 0.51765, 0.56765]
            })
        }));
        let control = DynamicImage::ImageRgb32F(
            Rgb32FImage::from_raw(3, 1, vec![0.2, 0.2, 0.2, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]).unwrap(),
        );
        for (input, endpoints) in [(edge, [19, 20]), (control, [1, 2])] {
            let output = suppress_linear_srgb_v1(input.clone(), &None).unwrap();
            assert_eq!(output.as_bytes(), input.as_bytes());
            assert_colour_controls(&input, &output, endpoints);
        }
    }

    #[test]
    fn k05_replicated_small_borders_and_gate_endpoints() {
        let left = [0.58235, 0.48235, 0.43235];
        let right = [0.41765, 0.51765, 0.56765];
        for (w, h, data) in [
            (1, 1, left.to_vec()),
            (1, 3, [left, [0.5; 3], right].concat()),
            (3, 1, [left, [0.5; 3], right].concat()),
        ] {
            let input = DynamicImage::ImageRgb32F(Rgb32FImage::from_raw(w, h, data).unwrap());
            let before = input.as_bytes().to_vec();
            assert_eq!(
                suppress_linear_srgb_v1(input, &None).unwrap().as_bytes(),
                before
            );
        }
        for input in [
            impulse(2, 2, &[(0, 0)]),
            impulse(7, 5, &[(0, 0), (3, 2), (6, 4)]),
        ] {
            let output = suppress_linear_srgb_v1(input, &None).unwrap();
            assert!(rgb_data(&output).iter().all(|v| (*v - 0.5).abs() <= 1e-6));
        }
        assert!(accepts_edge(0.0, f32::from_bits(0x3cffffff)).unwrap());
        assert!(accepts_edge(0.0, f32::from_bits(0x3d000000)).unwrap());
        assert!(!accepts_edge(0.0, f32::from_bits(0x3d000001)).unwrap());
        assert!(accepts_edge(32.0, 33.0).unwrap());
        assert!(accepts_edge(-32.0, -33.0).unwrap());
        assert!(!accepts_edge(32.0, 34.0).unwrap());
    }

    #[test]
    fn k06_linear_luma_gray_and_signed_hdr() {
        let gray = DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(11, 7, |x, y| {
            let v = (x as f32 - 3.0) * 0.2 + if y % 2 == 0 { 0.0 } else { 0.5 };
            Rgb([v; 3])
        }));
        let bytes = gray.as_bytes().to_vec();
        assert_eq!(
            suppress_linear_srgb_v1(gray, &None).unwrap().as_bytes(),
            bytes
        );
        for input in [field(), rgb(7, 5, [-0.2, 0.3, 1.5])] {
            let before = rgb_data(&input).to_vec();
            let output = suppress_linear_srgb_v1(input, &None).unwrap();
            for (a, b) in before
                .chunks_exact(3)
                .zip(rgb_data(&output).chunks_exact(3))
            {
                assert!((ycc(a)[0] - ycc(b)[0]).abs() <= 1e-5);
            }
        }
    }

    #[test]
    fn k07_alpha_is_bitwise_and_not_a_weight() {
        let input = impulse(5, 5, &[(2, 2)]);
        let alpha = [-0.0_f32, 0.0, 0.25, 1.0, 2.0, -0.5];
        let rgba = DynamicImage::ImageRgba32F(Rgba32FImage::from_fn(5, 5, |x, y| {
            let i = (y * 5 + x) as usize;
            let p = &rgb_data(&input)[i * 3..i * 3 + 3];
            Rgba([p[0], p[1], p[2], alpha[i % alpha.len()]])
        }));
        let expected = suppress_linear_srgb_v1(input, &None).unwrap();
        let actual = suppress_linear_srgb_v1(rgba, &None).unwrap();
        let DynamicImage::ImageRgba32F(actual) = actual else {
            panic!("RGBA format changed")
        };
        for (i, p) in actual.pixels().enumerate() {
            assert_eq!(p[3].to_bits(), alpha[i % alpha.len()].to_bits());
            for c in 0..3 {
                assert_eq!(p[c].to_bits(), rgb_data(&expected)[i * 3 + c].to_bits());
            }
        }
    }

    #[test]
    fn k08_typed_errors_and_real_reserve_failure() {
        assert_eq!(
            suppress_linear_srgb_v1(DynamicImage::new_rgb8(1, 1), &None).unwrap_err(),
            FcsError::UnsupportedFormat
        );
        assert_eq!(
            suppress_linear_srgb_v1(rgb(0, 1, [0.0; 3]), &None).unwrap_err(),
            FcsError::InvalidGeometry
        );
        let oversized =
            DynamicImage::ImageRgb32F(Rgb32FImage::from_raw(1, 1, vec![0.0; 4]).unwrap());
        assert_eq!(
            suppress_linear_srgb_v1(oversized, &None).unwrap_err(),
            FcsError::InvalidGeometry
        );
        assert_eq!(
            checked_len(u32::MAX, u32::MAX, 4),
            Err(FcsError::InvalidGeometry)
        );
        for nonfinite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for channel in 0..4 {
                let mut p = [0.5; 4];
                p[channel] = nonfinite;
                let input = DynamicImage::ImageRgba32F(Rgba32FImage::from_pixel(1, 1, Rgba(p)));
                assert_eq!(
                    suppress_linear_srgb_v1(input, &None).unwrap_err(),
                    FcsError::NonFiniteInput
                );
            }
        }
        assert_eq!(
            suppress_linear_srgb_v1(rgb(1, 1, [f32::MAX, -f32::MAX, 0.0]), &None).unwrap_err(),
            FcsError::ArithmeticOverflow
        );
        let cancel = None;
        let mut work = Work::new(&cancel);
        work.force_reserve_failure = true;
        assert_eq!(
            suppress_with_work(rgb(1, 1, [0.5; 3]), &work).unwrap_err(),
            FcsError::AllocationFailed
        );
    }

    #[test]
    fn k09_actual_cancellation_checkpoints() {
        let stale = Some((Arc::new(AtomicUsize::new(2)), 1));
        assert_eq!(
            suppress_linear_srgb_v1(rgb(2, 2, [0.5; 3]), &stale).unwrap_err(),
            FcsError::Cancelled
        );
        for (target, occurrence) in [
            (Checkpoint::ValidationRow, 2),
            (Checkpoint::BeforeAllocation, 1),
            (Checkpoint::OutputRow, 2),
            (Checkpoint::Final, 1),
        ] {
            let tracker = Arc::new(AtomicUsize::new(7));
            let token = Some((Arc::clone(&tracker), 7));
            let (reached_tx, reached_rx) = mpsc::sync_channel(1);
            let (resume_tx, resume_rx) = mpsc::sync_channel(1);
            std::thread::scope(|scope| {
                let handle = scope.spawn(move || {
                    let count = Cell::new(0);
                    let hook = |point| {
                        if point == target {
                            count.set(count.get() + 1);
                            if count.get() == occurrence {
                                reached_tx.send(point).unwrap();
                                resume_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                            }
                        }
                    };
                    let mut work = Work::new(&token);
                    work.hook = Some(&hook);
                    suppress_with_work(impulse(5, 5, &[(2, 2)]), &work)
                });
                assert_eq!(
                    reached_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                    target
                );
                tracker.store(8, Ordering::SeqCst);
                resume_tx.send(()).unwrap();
                assert_eq!(handle.join().unwrap().unwrap_err(), FcsError::Cancelled);
            });
        }
    }

    #[test]
    fn k10_immutable_source_reverse_reference_and_repeat() {
        let original = field();
        let before = original.as_bytes().to_vec();
        let DynamicImage::ImageRgb32F(input) = &original else {
            unreachable!()
        };
        let expected = DynamicImage::ImageRgb32F(reverse_reference(input));
        let first = suppress_linear_srgb_v1(original.clone(), &None).unwrap();
        let repeated = suppress_linear_srgb_v1(original.clone(), &None).unwrap();
        assert_eq!(first.as_bytes(), expected.as_bytes());
        assert_eq!(first.as_bytes(), repeated.as_bytes());
        assert_eq!(original.as_bytes(), before);
    }

    #[test]
    fn k11_parallel_rows_match_serial_rgb_rgba_and_boundary_inputs() {
        fn serial_hook(_: Checkpoint) {}

        let cancel = None;
        let serial = Work {
            cancel: &cancel,
            hook: Some(&serial_hook),
            force_reserve_failure: false,
        };
        let assert_parity = |image: DynamicImage| {
            let parallel = suppress_linear_srgb_v1(image.clone(), &cancel);
            let reference = suppress_with_work(image, &serial);
            match (parallel, reference) {
                (Ok(parallel), Ok(reference)) => {
                    assert_eq!(parallel.as_bytes(), reference.as_bytes());
                }
                (Err(parallel), Err(reference)) => assert_eq!(parallel, reference),
                (parallel, reference) => {
                    panic!("parallel/serial result mismatch: {parallel:?} vs {reference:?}")
                }
            }
        };

        for (width, height) in [(1, 1), (2, 7), (11, 5)] {
            assert_parity(DynamicImage::ImageRgb32F(Rgb32FImage::from_fn(
                width,
                height,
                |x, y| {
                    let value = ((x * 7 + y * 11) % 23) as f32 * 0.75 - 4.0;
                    Rgb([value, 1.25 + value * 0.3, 2.0 - value * 1.1])
                },
            )));
            assert_parity(DynamicImage::ImageRgba32F(Rgba32FImage::from_fn(
                width,
                height,
                |x, y| {
                    let value = ((x * 5 + y * 13) % 19) as f32 * 1.5 - 6.0;
                    let alpha = ((x + y) % 4) as f32 / 3.0;
                    Rgba([value, 0.5 - value * 0.2, 3.0 + value * 0.6, alpha])
                },
            )));
        }
        assert_parity(rgb(1, 1, [f32::MAX, -f32::MAX, 0.0]));
        assert_parity(DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(
            2,
            2,
            Rgb([0.5, f32::NAN, 0.25]),
        )));
        assert_parity(DynamicImage::ImageRgb32F(Rgb32FImage::new(0, 2)));
    }
}
