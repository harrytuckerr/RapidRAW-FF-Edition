//! Native, non-HDR Adobe RGB table evaluation on linear ProPhoto pixels.
//!
//! Source-checked against Adobe SDK mirrored by Google at revision
//! de700ad461e35af50b28b861943a0b0753b10929:
//! https://android.googlesource.com/platform/external/dng_sdk/+/de700ad461e35af50b28b861943a0b0753b10929/source/
//! dng_reference.cpp: RefRGBtoRGBTable3D (tetrahedra, encoded amount, gamut).
//! dng_big_table.cpp/.h: dng_rgb_to_rgb_table_data, SetAmount.
//! dng_color_space.cpp/.h: matrices, spline-toe gamma functions.
//! dng_1d_table.cpp/.h: 4096-interval, non-subsampled transfer tables.
//! dng_1d_function.cpp: inverse iteration; dng_spline.h: cubic segment.
//!
//! This implements the non-HDR/no-mask path. It does not assert ACR parity.
//! `apply` requires bounded ProPhoto input. `apply_non_hdr` is the explicit
//! application boundary: finite channels are clamped to [0,1] for a nonzero
//! Look. SDK dng_render.cpp DoBaseline1DFunction and dng_reference.cpp
//! RefBaselineRGBTone impose clipping before RGBTables. Our boundary is AFTER
//! our existing DCP renderer, not a reproduction of those exposure/tone stages.
//! It clips scene highlights/negative channels; it is NOT HDR handling or proof
//! of scene-referred/ACR equivalence. XMP capability flags do not select HDR.
//! Zero amount bypasses this boundary exactly.
use super::look_xmp::{LookColorSpace, LookGamut, LookTableSource, LookTransfer};
use nalgebra::{Matrix3, Vector3};

const STEPS: usize = 4096;

pub struct RgbLook {
    n: usize,
    samples: Vec<[f32; 3]>, // decoded integer sample values, before /65535
    amount: f32,
    matrices: Option<(Matrix3<f32>, Matrix3<f32>)>,
    gamma: Option<(Vec<f32>, Vec<f32>)>,
    extend: bool,
}

impl RgbLook {
    /// Amount uses UI percent units. Zero bypass is a deliberate application
    /// contract, before SDK min-amount clamping, but still validates the table.
    pub fn new(
        source: &LookTableSource,
        percent: f64,
        supports_amount: bool,
    ) -> Result<Self, String> {
        let table = match source {
            LookTableSource::Decoded(t) => t,
            LookTableSource::Captured { .. } => return Err("Captured RGB Looks are not supported by the native evaluator".into()),
            LookTableSource::Unavailable => return Err("Selected Look RGB table is unavailable or malformed (including unsupported table enums)".into()),
        };
        if !percent.is_finite() || percent < 0.0 || percent > i32::MAX as f64 {
            return Err(
                "Look amount must be a finite nonnegative percentage within the SDK integer range"
                    .into(),
            );
        }
        if !supports_amount && percent != 0.0 && percent != 100.0 {
            return Err("Selected Look does not support adjustable amount".into());
        }
        let n = table.divisions as usize;
        if !(2..=256).contains(&n) || table.samples.len() != n.pow(3) {
            return Err("Malformed RGB Look grid dimensions or sample count".into());
        }
        if !table.min_amount.is_finite()
            || !table.max_amount.is_finite()
            || !(0.0..=1.0).contains(&table.min_amount)
            || table.max_amount < 1.0
        {
            return Err("Malformed RGB Look amount range".into());
        }
        let amount = if percent == 0.0 {
            0.0
        } else {
            ((percent.round() * 0.01).clamp(table.min_amount, table.max_amount)) as f32
        };
        let mut samples = Vec::with_capacity(n.pow(3));
        for r in 0..n {
            for g in 0..n {
                for b in 0..n {
                    let delta = table.samples[(r * n + g) * n + b];
                    samples.push(std::array::from_fn(|c| {
                        let i = [r, g, b][c];
                        ((delta[c] as usize + (i * 65535 + (n >> 1)) / (n - 1)) & 65535) as f32
                    }));
                }
            }
        }
        let matrices = if table.primaries == LookColorSpace::ProPhoto {
            None
        } else {
            let prophoto = matrix_to_pcs(LookColorSpace::ProPhoto);
            let other = matrix_to_pcs(table.primaries);
            let encode = other
                .try_inverse()
                .ok_or("Singular Look primaries matrix")?
                * prophoto;
            let decode = prophoto.try_inverse().ok_or("Singular ProPhoto matrix")? * other;
            Some((encode.cast(), decode.cast()))
        };
        let gamma = if table.transfer == LookTransfer::Linear {
            None
        } else {
            let encode: Vec<_> = (0..=STEPS + 1)
                .map(|i| encode_transfer(table.transfer, i.min(STEPS) as f64 / STEPS as f64) as f32)
                .collect();
            let decode: Vec<_> = (0..=STEPS + 1)
                .map(|i| decode_transfer(table.transfer, i.min(STEPS) as f64 / STEPS as f64) as f32)
                .collect();
            Some((encode, decode))
        };
        Ok(Self {
            n,
            samples,
            amount,
            matrices,
            gamma,
            extend: table.gamut == LookGamut::Extend,
        })
    }

    /// Explicit application adaptation to the SDK non-HDR bounded-input
    /// contract. This does not claim equivalent upstream tone/exposure stages.
    pub fn apply_non_hdr(&self, input: [f32; 3]) -> Result<[f32; 3], String> {
        if self.amount == 0.0 {
            return Ok(input);
        }
        if input.iter().any(|v| !v.is_finite()) {
            return Err("Non-finite RGB Look input".into());
        }
        self.apply(input.map(|v| v.clamp(0.0, 1.0)))
    }

    pub fn apply(&self, input: [f32; 3]) -> Result<[f32; 3], String> {
        if self.amount == 0.0 {
            return Ok(input);
        }
        if input.iter().any(|v| !v.is_finite()) {
            return Err("Non-finite RGB Look input".into());
        }
        if input.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return Err("Non-HDR ProPhoto Look requires input in [0,1]; HDR overrange processing is not implemented".into());
        }
        let mut rgb = input;
        let mut delta = [0.0; 3];
        if let Some((encode, _)) = &self.matrices {
            let v = encode * Vector3::from(input);
            for c in 0..3 {
                rgb[c] = v[c].clamp(0.0, 1.0);
                if self.extend {
                    delta[c] = v[c] - rgb[c];
                }
            }
        }
        if let Some((encode, _)) = &self.gamma {
            rgb = rgb.map(|v| interpolate(encode, v));
        }
        let mapped = self.tetrahedral(rgb);
        rgb = if self.amount == 1.0 {
            mapped
        } else {
            std::array::from_fn(|c| (rgb[c] + self.amount * (mapped[c] - rgb[c])).clamp(0.0, 1.0))
        };
        if let Some((_, decode)) = &self.gamma {
            rgb = rgb.map(|v| interpolate(decode, v));
        }
        if let Some((_, decode)) = &self.matrices {
            let v = decode * Vector3::from(std::array::from_fn::<_, 3, _>(|c| rgb[c] + delta[c]));
            rgb = [
                v[0].clamp(0.0, 1.0),
                v[1].clamp(0.0, 1.0),
                v[2].clamp(0.0, 1.0),
            ];
        }
        Ok(rgb)
    }

    fn tetrahedral(&self, rgb: [f32; 3]) -> [f32; 3] {
        let scaled = rgb.map(|v| v * (self.n - 1) as f32);
        let index = scaled.map(|v| (v as usize).min(self.n - 2));
        let f: [f32; 3] = std::array::from_fn(|c| scaled[c] - index[c] as f32);
        // The SDK's six tetrahedra, including its tie ordering (B, G, R).
        let order = if f[1] >= f[0] {
            if f[2] >= f[1] {
                [2, 1, 0]
            } else if f[2] >= f[0] {
                [1, 2, 0]
            } else {
                [1, 0, 2]
            }
        } else if f[2] >= f[0] {
            [2, 0, 1]
        } else if f[2] >= f[1] {
            [0, 2, 1]
        } else {
            [0, 1, 2]
        };
        let strides = [self.n * self.n, self.n, 1];
        let base = index[0] * strides[0] + index[1] * strides[1] + index[2];
        let vertices = [
            base,
            base + strides[order[0]],
            base + strides[order[0]] + strides[order[1]],
            base + strides.iter().sum::<usize>(),
        ];
        let weights = [
            1.0 - f[order[0]],
            f[order[0]] - f[order[1]],
            f[order[1]] - f[order[2]],
            f[order[2]],
        ];
        std::array::from_fn(|c| {
            (weights[0] * self.samples[vertices[0]][c]
                + weights[1] * self.samples[vertices[1]][c]
                + weights[2] * self.samples[vertices[2]][c]
                + weights[3] * self.samples[vertices[3]][c])
                * (1.0 / 65535.0)
        })
    }
}

fn matrix_to_pcs(space: LookColorSpace) -> Matrix3<f64> {
    let values = match space {
        LookColorSpace::Srgb => [
            0.4361, 0.3851, 0.1431, 0.2225, 0.7169, 0.0606, 0.0139, 0.0971, 0.7141,
        ],
        LookColorSpace::Adobe => [
            0.6097, 0.2053, 0.1492, 0.3111, 0.6257, 0.0632, 0.0195, 0.0609, 0.7446,
        ],
        LookColorSpace::ProPhoto => [
            0.7977, 0.1352, 0.0313, 0.2880, 0.7119, 0.0001, 0.0, 0.0, 0.8249,
        ],
        LookColorSpace::DisplayP3 => [
            0.5151, 0.2920, 0.1571, 0.2412, 0.6922, 0.0666, -0.0010, 0.0419, 0.7843,
        ],
        LookColorSpace::Rec2020 => [
            0.6735, 0.1657, 0.1251, 0.2791, 0.6753, 0.0456, -0.0019, 0.0300, 0.7971,
        ],
    };
    let mut m = Matrix3::from_row_slice(&values);
    // The common PCS white diagonal cancels in inv(M_target)*M_source.
    // Unit row sums therefore give the same transform as SDK SetMatrixToPCS.
    for row in 0..3 {
        let sum = m[(row, 0)] + m[(row, 1)] + m[(row, 2)];
        for col in 0..3 {
            m[(row, col)] /= sum;
        }
    }
    m
}

fn interpolate(table: &[f32], x: f32) -> f32 {
    let scaled = x * STEPS as f32;
    let i = scaled as usize;
    let fraction = scaled - i as f32;
    table[i] * (1.0 - fraction) + table[i + 1] * fraction
}

fn toe(x: f64, x1: f64, y1: f64, slope1: f64) -> f64 {
    let b = x / x1;
    let c = (x1 - x) / x1;
    (32.0 * x1 * b) * (c * c) + (y1 * (2.0 - b + c) - slope1 * x1 * c) * (b * b)
}

fn encode_transfer(t: LookTransfer, x: f64) -> f64 {
    match t {
        LookTransfer::Linear => x,
        LookTransfer::Srgb => {
            if x <= 0.0031308 {
                12.92 * x
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            }
        }
        LookTransfer::Gamma18 => {
            if x <= 8.2118790552e-4 {
                toe(x, 8.2118790552e-4, 0.019310851, 13.064306598)
            } else {
                x.powf(1.0 / 1.8)
            }
        }
        LookTransfer::Gamma22 => {
            if x <= 0.0034800731 {
                toe(x, 0.0034800731, 0.0763027458, 9.9661890075)
            } else {
                x.powf(1.0 / 2.2)
            }
        }
        LookTransfer::Rec2020 => {
            if x <= 0.0180539685108078 {
                4.5 * x
            } else {
                1.099_296_826_809_443 * x.powf(0.45) - (1.099_296_826_809_443 - 1.0)
            }
        }
    }
}

fn decode_transfer(t: LookTransfer, y: f64) -> f64 {
    let toe_limit = match t {
        LookTransfer::Gamma18 => 0.019310851,
        LookTransfer::Gamma22 => 0.0763027458,
        _ => 0.0,
    };
    if y > 0.0 && y < toe_limit {
        // SDK dng_1d_function::EvaluateInverse secant iteration.
        let (mut x0, mut y0, mut x1, mut y1) =
            (0.0, encode_transfer(t, 0.0), 1.0, encode_transfer(t, 1.0));
        for _ in 0..30 {
            if (y1 - y0).abs() < 1e-10 {
                break;
            }
            let x2 = (x1 + (y - y1) * (x1 - x0) / (y1 - y0)).clamp(0.0, 1.0);
            (x0, y0, x1, y1) = (x1, y1, x2, encode_transfer(t, x2));
        }
        return x1;
    }
    match t {
        LookTransfer::Linear => y,
        LookTransfer::Srgb => {
            if y <= 0.0031308 * 12.92 {
                y / 12.92
            } else {
                ((y + 0.055) / 1.055).powf(2.4)
            }
        }
        LookTransfer::Gamma18 => y.powf(1.8),
        LookTransfer::Gamma22 => y.powf(2.2),
        LookTransfer::Rec2020 => {
            if y <= 4.5 * 0.0180539685108078 {
                y / 4.5
            } else {
                ((y + (1.099_296_826_809_443 - 1.0)) / 1.099_296_826_809_443).powf(1.0 / 0.45)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcp::look_xmp::CobaltRgbTable;
    fn table(map: impl Fn([usize; 3]) -> [u16; 3]) -> CobaltRgbTable {
        let mut samples = Vec::new();
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    let p = [r, g, b];
                    let value = map(p);
                    samples.push(std::array::from_fn(|c| {
                        value[c].wrapping_sub((p[c] * 65535 + 1) as u16)
                    }));
                }
            }
        }
        CobaltRgbTable {
            divisions: 2,
            samples,
            primaries: LookColorSpace::ProPhoto,
            transfer: LookTransfer::Linear,
            gamut: LookGamut::Clip,
            min_amount: 0.0,
            max_amount: 2.0,
        }
    }
    fn identity() -> CobaltRgbTable {
        table(|p| p.map(|c| (c * 65535) as u16))
    }
    fn evaluator(t: CobaltRgbTable, percent: f64) -> RgbLook {
        RgbLook::new(&LookTableSource::Decoded(t), percent, true).unwrap()
    }
    fn near(a: [f32; 3], b: [f32; 3], tol: f32) {
        for c in 0..3 {
            assert!((a[c] - b[c]).abs() < tol, "{a:?} != {b:?}");
        }
    }

    #[test]
    #[ignore = "requires locally owned Cobalt Look XMP files"]
    fn supplied_looks_native_evaluation_smoke() {
        let dir = std::env::var("RAPIDRAW_LOOKS_DIR").expect("set RAPIDRAW_LOOKS_DIR explicitly");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("xmp") {
                continue;
            }
            let look = crate::dcp::look_xmp::parse_look_xmp(&path).unwrap();
            let mut changed = false;
            for amount in [0.0, 50.0, 100.0, 150.0] {
                if !look.supports_amount && amount != 0.0 && amount != 100.0 {
                    continue;
                }
                let renderer = RgbLook::new(&look.table, amount, look.supports_amount).unwrap();
                for r in [0.0, 0.01, 0.18, 0.5, 1.0] {
                    for g in [0.0, 0.01, 0.18, 0.5, 1.0] {
                        for b in [0.0, 0.01, 0.18, 0.5, 1.0] {
                            let input = [r, g, b];
                            let output = renderer.apply(input).unwrap();
                            assert!(output.iter().all(|v| v.is_finite()), "{}", path.display());
                            if amount == 0.0 {
                                assert_eq!(output, input);
                            } else if output.iter().zip(input).any(|(a, b)| (*a - b).abs() > 1e-4) {
                                changed = true;
                            }
                        }
                    }
                }
            }
            assert!(
                changed,
                "{} did not affect any sampled pixels",
                path.display()
            );
            count += 1;
        }
        assert_eq!(count, 17, "expected the supplied 17-Look corpus");
        eprintln!(
            "Native evaluation smoke: {count} Looks, 125 pixels at each supported amount; no Adobe reference comparison"
        );
    }

    #[test]
    fn tetrahedra_all_six_orders_and_ties_are_not_trilinear() {
        let look = evaluator(
            table(|p| if p == [1, 1, 1] { [65535; 3] } else { [0; 3] }),
            100.0,
        );
        for p in [
            [0.2, 0.4, 0.7],
            [0.2, 0.7, 0.4],
            [0.4, 0.2, 0.7],
            [0.4, 0.7, 0.2],
            [0.7, 0.2, 0.4],
            [0.7, 0.4, 0.2],
            [0.4, 0.4, 0.4],
        ] {
            let expected = p.into_iter().fold(1.0f32, f32::min);
            near(look.apply(p).unwrap(), [expected; 3], 1e-6);
        }
    }

    #[test]
    fn zero_amount_is_exact_bypass_even_below_minimum_and_outside_domain() {
        let mut t = table(|_| [65535; 3]);
        t.min_amount = 0.2;
        let look = evaluator(t, 0.0);
        assert_eq!(look.apply([-0.2, 1.8, 0.123]).unwrap(), [-0.2, 1.8, 0.123]);
    }

    #[test]
    fn amount_rounds_then_clamps_and_blends_in_encoded_space() {
        let mut t = table(|_| [65535; 3]);
        t.transfer = LookTransfer::Gamma18;
        let look = evaluator(t.clone(), 50.0);
        let expected = ((0.25f64.powf(1.0 / 1.8) + 1.0) * 0.5).powf(1.8) as f32;
        near(look.apply([0.25; 3]).unwrap(), [expected; 3], 2e-6);
        assert!((expected - 0.625).abs() > 0.01); // would be linear-space mixing
        assert_eq!(evaluator(t.clone(), 12.49).amount, 0.12);
        assert_eq!(evaluator(t.clone(), 12.5).amount, 0.13);
        t.min_amount = 0.2;
        t.max_amount = 1.5;
        assert_eq!(evaluator(t.clone(), 1.0).amount, 0.2);
        assert_eq!(evaluator(t, 200.0).amount, 1.5);
    }

    #[test]
    fn explicit_non_hdr_boundary_clips_but_zero_bypasses() {
        let look = evaluator(identity(), 100.0);
        near(
            look.apply_non_hdr([-0.00001, 1.00001, 0.3]).unwrap(),
            [0.0, 1.0, 0.3],
            1e-6,
        );
        near(
            look.apply_non_hdr([-0.2, 3.0, 0.5]).unwrap(),
            [0.0, 1.0, 0.5],
            1e-6,
        );
        assert!(look.apply_non_hdr([f32::NAN, 0.2, 0.3]).is_err());
        let bypass = evaluator(identity(), 0.0);
        assert_eq!(
            bypass.apply_non_hdr([-0.2, 3.0, 0.5]).unwrap(),
            [-0.2, 3.0, 0.5]
        );
    }

    #[test]
    fn amount_extrapolation_clips_before_decoding() {
        let look = evaluator(table(|_| [65535, 0, 65535]), 200.0);
        assert_eq!(look.apply([0.2, 0.4, 0.6]).unwrap(), [1.0, 0.0, 1.0]);
    }

    #[test]
    fn sdk_gamma_toes_and_transfer_tables() {
        assert!((encode_transfer(LookTransfer::Gamma18, 1e-9) / 1e-9 - 32.0).abs() < 0.001);
        assert!(
            (encode_transfer(LookTransfer::Gamma18, 0.0001) - 0.0001f64.powf(1.0 / 1.8)).abs()
                > 0.001
        );
        for transfer in [
            LookTransfer::Linear,
            LookTransfer::Srgb,
            LookTransfer::Gamma18,
            LookTransfer::Gamma22,
            LookTransfer::Rec2020,
        ] {
            let mut t = identity();
            t.transfer = transfer;
            let look = evaluator(t, 100.0);
            near(look.apply([0.2, 0.4, 0.7]).unwrap(), [0.2, 0.4, 0.7], 2e-6);
            for x in [0.0, 0.0001, 0.001, 0.2, 0.7, 1.0] {
                let restored = decode_transfer(transfer, encode_transfer(transfer, x));
                assert!(
                    (restored - x).abs() < 1e-8,
                    "{transfer:?}: {x} -> {restored}"
                );
            }
            if let Some((encode, decode)) = &look.gamma {
                assert_eq!(encode.len(), 4098);
                assert_eq!(decode.len(), 4098);
            }
        }
    }

    #[test]
    fn declared_primaries_and_gamut_extension() {
        for primaries in [
            LookColorSpace::Srgb,
            LookColorSpace::Adobe,
            LookColorSpace::DisplayP3,
            LookColorSpace::Rec2020,
            LookColorSpace::ProPhoto,
        ] {
            let mut t = identity();
            t.primaries = primaries;
            near(
                evaluator(t.clone(), 100.0).apply([0.4, 0.35, 0.3]).unwrap(),
                [0.4, 0.35, 0.3],
                2e-6,
            );
            if primaries == LookColorSpace::Srgb {
                let clipped = evaluator(t.clone(), 100.0).apply([0.8, 0.1, 0.1]).unwrap();
                t.gamut = LookGamut::Extend;
                near(
                    evaluator(t, 100.0).apply([0.8, 0.1, 0.1]).unwrap(),
                    [0.8, 0.1, 0.1],
                    2e-6,
                );
                assert!((clipped[0] - 0.8).abs() > 0.01);
            }
        }
    }

    #[test]
    fn unsupported_or_malformed_selections_error_including_zero_amount() {
        for source in [
            LookTableSource::Unavailable,
            LookTableSource::Captured {
                cube_path: "missing.cube".into(),
            },
        ] {
            for amount in [0.0, 100.0] {
                assert!(RgbLook::new(&source, amount, true).is_err());
            }
        }
        for amount in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
            assert!(RgbLook::new(&LookTableSource::Decoded(identity()), amount, true).is_err());
        }
        let mut t = identity();
        t.divisions = 1;
        assert!(RgbLook::new(&LookTableSource::Decoded(t), 100.0, true).is_err());
        assert!(RgbLook::new(&LookTableSource::Decoded(identity()), 50.0, false).is_err());
        let look = evaluator(identity(), 100.0);
        assert!(
            look.apply([1.1, 0.1, 0.2])
                .unwrap_err()
                .contains("overrange")
        );
        assert!(look.apply([f32::NAN, 0.1, 0.2]).is_err());
    }
}
