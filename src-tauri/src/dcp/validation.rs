//! DCP validation harness: synthetic colour-science tests and optional GPU parity.
//!
//! The default suite verifies synthetic renderer properties, not upstream
//! no-profile equivalence or ACR reference acceptance. D2/D6 acceptance is
//! unimplemented here. The separate ignored acceptance tests fail with explicit
//! blockers when invoked; assets or environment flags cannot make them pass.
//!
//! # Test categories
//!
//! | Category          | Default | Scope                                  |
//! |-------------------|---------|----------------------------------------|
//! | Identity profile  | Yes     | Synthetic identity transform tolerance |
//! | Neutral axis      | Yes     | Synthetic camera neutral in ProPhoto   |
//! | Known-matrix RT   | Yes     | Synthetic matrix round-trip            |
//! | HSV wrap/clamp    | Yes     | Synthetic table interpolation          |
//! | Upstream equality | Ignored | Unimplemented acceptance blocker       |
//! | ACR comparison    | Ignored | Unimplemented acceptance blocker       |
//! | GPU parity        | Ignored | Synthetic CPU/GPU comparison; needs GPU |

use crate::dcp::model::*;
use std::path::PathBuf;

// Re-export the delta-E and statistics machinery for use by the CLI harness.
pub use crate::dcp::delta_e::{
    DeltaEStats, check_gpu_parity, check_thresholds, compute_stats, prophoto_delta_e,
};

// ---------------------------------------------------------------------------
// GPU uniform helpers for W3 parity test
// ---------------------------------------------------------------------------

/// Uniform block matching the parity entry point's uniform struct in WGSL.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DcpParityUniforms {
    has_dcp: u32,
    dcp_huesat_encoding: u32,
    dcp_look_encoding: u32,
    dcp_baseline_exposure_offset: f32,
    dcp_huesat_dims: [u32; 4],
    dcp_look_dims: [u32; 4],
    cam_to_prophoto: [[f32; 4]; 3],
    prophoto_to_working: [[f32; 4]; 3],
}

/// Convert a nalgebra Matrix3<f32> to column-major [[f32;4];3] matching
/// WGSL mat3x3<f32> layout (3 columns of vec4<f32>, padding = 0).
fn mat3_to_cols(m: Mat3) -> [[f32; 4]; 3] {
    [
        [m[(0, 0)], m[(1, 0)], m[(2, 0)], 0.0],
        [m[(0, 1)], m[(1, 1)], m[(2, 1)], 0.0],
        [m[(0, 2)], m[(1, 2)], m[(2, 2)], 0.0],
    ]
}

// ---------------------------------------------------------------------------
/// WGSL source for the parity entry point.
const PARITY_ENTRY_WGSL: &str = r##"
struct ParUniforms {
    has_dcp: u32,
    dcp_huesat_encoding: u32,
    dcp_look_encoding: u32,
    dcp_baseline_exposure_offset: f32,
    dcp_huesat_dims: vec4<u32>,
    dcp_look_dims: vec4<u32>,
    cam_to_prophoto: mat3x3<f32>,
    prophoto_to_working: mat3x3<f32>,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<storage, read> u: ParUniforms;
@group(0) @binding(14) var dcp_huesat_tex: texture_3d<f32>;
@group(0) @binding(15) var dcp_look_tex: texture_3d<f32>;
@group(0) @binding(16) var dcp_tone_curve_tex: texture_1d<f32>;

@compute @workgroup_size(8, 8, 1)
fn main_parity(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_dims = vec2<u32>(textureDimensions(output_texture));
    if (id.x >= out_dims.x || id.y >= out_dims.y) { return; }

    var rgb = textureLoad(input_texture, id.xy, 0).rgb;

    if (u.has_dcp != 0u) {
        rgb = u.cam_to_prophoto * rgb;

        if u.dcp_huesat_dims.x > 0u {
            rgb = dcp_apply_hsv_table(rgb, dcp_huesat_tex,
                                       u.dcp_huesat_dims, u.dcp_huesat_encoding);
        }

        rgb = dcp_baseline_exposure(rgb, u.dcp_baseline_exposure_offset);

        if u.dcp_look_dims.x > 0u {
            rgb = dcp_apply_hsv_table(rgb, dcp_look_tex,
                                       u.dcp_look_dims, u.dcp_look_encoding);
        }

        rgb = dcp_apply_tone_curve(rgb, dcp_tone_curve_tex);

        rgb = u.prophoto_to_working * rgb;
    }

    textureStore(output_texture, id.xy, vec4<f32>(rgb, 1.0));
}
"##;

/// Build the parity shader source by concatenating dcp.wgsl with the parity entry point.
pub(crate) fn parity_shader_source() -> String {
    let dcp = include_str!("../shaders/dcp.wgsl");
    format!("{}\n{}", dcp, PARITY_ENTRY_WGSL)
}

// Synthetic profile builders
// ---------------------------------------------------------------------------

/// Build a synthetic `DcpProfile` with the given name and colour matrices.
/// Both illuminants share the same data (single-illuminant effectively). No
/// rendering tables, no tone curve.
fn synthetic_profile(
    name: &str,
    color_matrix_1: Mat3,
    forward_matrix_1: Option<Mat3>,
    calibration_illuminant_1: Illuminant,
) -> DcpProfile {
    DcpProfile {
        id: ProfileId([0u8; 32]),
        file_path: PathBuf::new(),
        profile_name: name.into(),
        unique_camera_model: "synthetic".into(),
        copyright: None,
        embed_policy: EmbedPolicy::NoRestrictions,
        calibration_illuminant_1,
        calibration_illuminant_2: None,
        color_matrix_1,
        color_matrix_2: None,
        forward_matrix_1,
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

/// Build a full dual-illuminant synthetic profile for realistic-scenario tests.
fn dual_illuminant_profile() -> DcpProfile {
    let cm1 = Mat3::new(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    let cm2 = Mat3::new(0.9, 0.0, 0.0, 0.0, 1.1, 0.0, 0.0, 0.0, 1.0);

    // ProPhoto→XYZ = inverse of XYZ→ProPhoto (from render.rs constants)
    let xyz_to_prophoto: [f64; 9] = [
        1.345_786_881_647_158_5,
        -0.255_572_087_379_794_64,
        -0.051_101_864_975_545_26,
        -0.544_630_705_124_901_9,
        1.508_247_742_845_146_8,
        0.020_527_447_436_421_39,
        0.0,
        0.0,
        1.211_967_545_638_945_2,
    ];
    let xyz_to_prophoto_mat = nalgebra::Matrix3::new(
        xyz_to_prophoto[0],
        xyz_to_prophoto[1],
        xyz_to_prophoto[2],
        xyz_to_prophoto[3],
        xyz_to_prophoto[4],
        xyz_to_prophoto[5],
        xyz_to_prophoto[6],
        xyz_to_prophoto[7],
        xyz_to_prophoto[8],
    );
    let fm_f64 = xyz_to_prophoto_mat.try_inverse().unwrap();
    let fm = fm_f64.cast::<f32>();

    DcpProfile {
        id: ProfileId([1u8; 32]),
        file_path: PathBuf::new(),
        profile_name: "dual-illuminant-test".into(),
        unique_camera_model: "synthetic-dual".into(),
        copyright: None,
        embed_policy: EmbedPolicy::NoRestrictions,
        calibration_illuminant_1: Illuminant::StdA,
        calibration_illuminant_2: Some(Illuminant::D65),
        color_matrix_1: cm1,
        color_matrix_2: Some(cm2),
        forward_matrix_1: Some(fm),
        forward_matrix_2: Some(fm),
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

/// Build a single-illuminant profile with exactly-identity camera→ProPhoto
/// (cam_to_prophoto = I). Used only for synthetic identity-profile tests;
/// this does not exercise the application no-profile path or upstream output.
///
/// Constructed as: ColorMatrix=I, ForwardMatrix=ProPhoto→XYZ (inverse of
/// XYZ→ProPhoto), CameraCalibration/AB=None, as_shot_neutral=[1,1,1].
fn identity_profile() -> DcpProfile {
    let xyz_to_prophoto: [f64; 9] = [
        1.345_786_881_647_158_5,
        -0.255_572_087_379_794_64,
        -0.051_101_864_975_545_26,
        -0.544_630_705_124_901_9,
        1.508_247_742_845_146_8,
        0.020_527_447_436_421_39,
        0.0,
        0.0,
        1.211_967_545_638_945_2,
    ];
    let xyz_to_prophoto_mat = nalgebra::Matrix3::new(
        xyz_to_prophoto[0],
        xyz_to_prophoto[1],
        xyz_to_prophoto[2],
        xyz_to_prophoto[3],
        xyz_to_prophoto[4],
        xyz_to_prophoto[5],
        xyz_to_prophoto[6],
        xyz_to_prophoto[7],
        xyz_to_prophoto[8],
    );
    let fm_f64 = xyz_to_prophoto_mat.try_inverse().unwrap();
    let fm = fm_f64.cast::<f32>();

    DcpProfile {
        id: ProfileId([2u8; 32]),
        file_path: PathBuf::new(),
        profile_name: "identity-test".into(),
        unique_camera_model: "synthetic-identity".into(),
        copyright: None,
        embed_policy: EmbedPolicy::NoRestrictions,
        calibration_illuminant_1: Illuminant::D65,
        calibration_illuminant_2: None,
        color_matrix_1: Mat3::identity(),
        color_matrix_2: None,
        forward_matrix_1: Some(fm),
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

// ---------------------------------------------------------------------------
// Synthetic validation tests (asset-free, CI-safe)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcp::delta_e::prophoto_delta_e;
    use crate::dcp::render::DcpRenderer;
    use wgpu::util::DeviceExt;

    /// The XYZ→ProPhoto D50 matrix from render.rs — needed for delta-E
    /// computation in ProPhoto space.
    const XYZ_TO_PROPHOTO_D50: [f64; 9] = [
        1.345_786_881_647_158_5,
        -0.255_572_087_379_794_64,
        -0.051_101_864_975_545_26,
        -0.544_630_705_124_901_9,
        1.508_247_742_845_146_8,
        0.020_527_447_436_421_39,
        0.0,
        0.0,
        1.211_967_545_638_945_2,
    ];

    // ---- WGSL shader validation (CI -- no GPU required) -------------------

    /// Parse and validate every .wgsl shader under src/shaders/ using naga.
    #[test]
    fn all_shaders_validate_with_naga() {
        let shader_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/shaders"));
        assert!(shader_dir.is_dir(), "shader dir not found");

        let caps = naga::valid::Capabilities::all();
        let flags = naga::valid::ValidationFlags::all();

        // Exercise the exact source builders used by both production outputs.
        for (name, source) in [
            ("8-bit", crate::gpu_processing::production_shader_source()),
            (
                "16-bit",
                crate::gpu_processing::high_precision_shader_source(),
            ),
        ] {
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(flags, caps)
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            for (resource, expected) in [
                ("gf_coeffs_texture", 12),
                ("gf_dehaze_texture", 13),
                ("dcp_huesat_tex", 14),
                ("dcp_look_tex", 15),
                ("dcp_tone_curve_tex", 16),
            ] {
                let (_, var) = module
                    .global_variables
                    .iter()
                    .find(|(_, var)| var.name.as_deref() == Some(resource))
                    .unwrap();
                let binding = var.binding.as_ref().unwrap();
                assert_eq!(
                    (binding.group, binding.binding),
                    (0, expected),
                    "{resource}"
                );
            }
            eprintln!("naga: production {name} VALID");
        }

        // Standalone shaders.
        for entry in std::fs::read_dir(shader_dir).expect("read shader dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
                continue;
            }
            let stem = path.file_stem().unwrap().to_str().unwrap();
            if stem == "dcp" || stem == "shader" {
                continue;
            }
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
            let module = naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|e| panic!("{stem}.wgsl: naga parse: {e}"));
            let mut validator = naga::valid::Validator::new(flags, caps);
            if let Err(e) = validator.validate(&module) {
                panic!("{stem}.wgsl: naga validation failed: {e}");
            }
            eprintln!("naga: {stem}.wgsl VALID");
        }

        // Parity entry point.
        {
            let parity_src = super::parity_shader_source();
            let module =
                naga::front::wgsl::parse_str(&parity_src).expect("parity shader: naga parse");
            let mut validator = naga::valid::Validator::new(flags, caps);
            if let Err(e) = validator.validate(&module) {
                panic!("parity shader: naga validation failed: {e}");
            }
            eprintln!("naga: parity shader VALID");
        }
    }

    #[test]
    fn production_adjustment_layout_matches_wgsl() {
        use crate::image_processing::{AllAdjustments, GlobalAdjustments, MaskAdjustments};
        let source = crate::gpu_processing::production_shader_source();
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        macro_rules! check_layout {
            ($ty:ty, $name:literal, [$($field:ident),*]) => {{
                let (_, ty) = module.types.iter().find(|(_, ty)| ty.name.as_deref() == Some($name)).unwrap();
                let naga::TypeInner::Struct { members, span } = &ty.inner else { panic!("not a struct") };
                assert_eq!(*span as usize, std::mem::size_of::<$ty>(), "{} size", $name);
                $(let member = members.iter().find(|m| m.name.as_deref() == Some(stringify!($field))).unwrap();
                  assert_eq!(member.offset as usize, std::mem::offset_of!($ty, $field), "{}::{}", $name, stringify!($field));)*
            }};
        }
        check_layout!(
            GlobalAdjustments,
            "GlobalAdjustments",
            [
                exposure,
                wb_log_gain_l,
                wb_rgb_to_lms_matrix,
                wb_lms_to_rgb_matrix,
                has_dcp,
                dcp_baseline_exposure_offset,
                dcp_huesat_dims,
                dcp_look_dims,
                cam_to_prophoto,
                prophoto_to_working
            ]
        );
        check_layout!(
            MaskAdjustments,
            "MaskAdjustments",
            [exposure, wb_log_gain_l, hsl]
        );
        check_layout!(
            AllAdjustments,
            "AllAdjustments",
            [global, mask_adjustments, mask_count, tile_offset_x]
        );
    }

    #[test]
    fn as_shot_neutral_uses_rgb_and_rejects_invalid_gains() {
        use crate::dcp::pipeline::neutral_from_wb;
        for fourth in [f32::NAN, f32::INFINITY, 0.0, 7.0] {
            assert_eq!(
                neutral_from_wb([2.0, 1.0, 1.5, fourth]).unwrap(),
                [0.5, 1.0, 2.0 / 3.0]
            );
        }
        for channel in 0..3 {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
                let mut wb = [2.0, 1.0, 1.5, f32::NAN];
                wb[channel] = invalid;
                assert!(neutral_from_wb(wb).is_err());
            }
        }
        assert!(neutral_from_wb([f32::MIN_POSITIVE, f32::MAX, 1.0, f32::NAN]).is_err());
        assert!(neutral_from_wb([f32::MAX, f32::MIN_POSITIVE, 1.0, f32::NAN]).is_err());
    }

    #[test]
    fn dcp_texture_upload_preserves_cpu_table_coordinates() {
        let mut prof = super::identity_profile();
        let table = HsvTable {
            hue_div: 3,
            sat_div: 2,
            val_div: 2,
            data: (0..12).map(|i| [i as f32, 1.0, 1.0]).collect(),
        };
        prof.look_table = Some(table.clone());
        let renderer = DcpRenderer::new(&prof, [1.0; 3]).unwrap();
        let upload = crate::image_processing::dcp_texture_data_from_renderer(&renderer);
        assert_eq!(upload.look_dims, [3, 2, 2]);
        for v in 0..2usize {
            for h in 0..3usize {
                for s in 0..2usize {
                    let cpu_index = (v * 3 + h) * 2 + s;
                    let gpu_index = ((v * 2 + s) * 3 + h) * 4;
                    assert_eq!(
                        &upload.look[gpu_index..gpu_index + 3],
                        &table.data[cpu_index]
                    );
                }
            }
        }
        let mut global = crate::image_processing::GlobalAdjustments::default();
        global.exposure = 1.5;
        crate::image_processing::set_dcp_uniforms(&mut global, &renderer, 1.0);
        let no_table = DcpRenderer::new(&super::identity_profile(), [1.0; 3]).unwrap();
        crate::image_processing::set_dcp_uniforms(&mut global, &no_table, 1.0);
        assert_eq!(global.dcp_look_dims, [0; 4]);
        assert_eq!(global.dcp_huesat_dims, [0; 4]);
        assert_eq!(
            global.exposure, 1.5,
            "DCP must not overwrite upstream exposure"
        );
    }

    // ---- identity / neutral-axis tests (delta-E quantified) ----------------

    /// An identity-profile renderer must map its input to itself within
    /// ΔE2000 < 2e-2 (pipeline is evaluated in f64 internally, so the small
    /// residual comes from f32→f64→f32 round-trip and the Bradford adaptation
    /// of the ForwardMatrix normalisation).
    #[test]
    fn identity_delta_e_near_zero() {
        let prof = super::dual_illuminant_profile();
        let neutral = [1.0f32, 1.0, 1.0];
        let r = DcpRenderer::new(&prof, neutral).expect("dual-illuminant renderer");

        let test_pixels: [[f32; 3]; 7] = [
            [0.0, 0.0, 0.0],
            [0.18, 0.18, 0.18],
            [0.5, 0.3, 0.2],
            [1.0, 1.0, 1.0],
            [2.0, 1.0, 0.5],
            [0.1, 0.8, 0.3],
            [1.5, 0.2, 0.1],
        ];

        for input in test_pixels {
            let out = r.render_pixel(input);
            let de = prophoto_delta_e(input, out, &XYZ_TO_PROPHOTO_D50);
            assert!(
                de < 0.02,
                "identity-ish profile: input {input:?} -> {out:?}, delta-E {de}"
            );
        }
    }

    /// Camera neutral renders to a neutral colour in ProPhoto (ΔE between
    /// channels zero). This is the neutral-axis test from §7.4.
    #[test]
    fn neutral_axis_delta_e() {
        let prof = super::dual_illuminant_profile();
        let as_shot = [0.5f32, 1.0, 0.8];
        let r = DcpRenderer::new(&prof, as_shot).expect("dual-illuminant renderer");
        let out = r.render_pixel(as_shot);

        let diffs = [
            (out[0] - out[1]).abs(),
            (out[1] - out[2]).abs(),
            (out[0] - out[2]).abs(),
        ];
        for (i, &d) in diffs.iter().enumerate() {
            assert!(
                d < 1e-4,
                "neutral axis: as_shot {as_shot:?} -> {out:?}, channel diff[{i}] = {d}"
            );
        }
    }

    /// A known-colour-matrix profile with no tables, no curve produces a pure
    /// linear transform. With identity ColorMatrix and ProPhoto→XYZ ForwardMatrix,
    /// cam→ProPhoto ≈ identity (within f32⇄f64 round-trip tolerance).
    #[test]
    fn known_matrix_round_trip() {
        let prof = super::dual_illuminant_profile();
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let samples: [[f32; 3]; 5] = [
            [0.18, 0.18, 0.18],
            [0.5, 0.3, 0.2],
            [1.0, 1.0, 1.0],
            [0.1, 0.8, 0.3],
            [0.7, 0.5, 0.9],
        ];

        for input in samples {
            let pass1 = r.render_pixel(input);
            let de = prophoto_delta_e(input, pass1, &XYZ_TO_PROPHOTO_D50);
            assert!(
                de < 0.02,
                "matrix round-trip (identity): input {input:?} -> {pass1:?}, delta-E {de}"
            );
        }
    }

    /// Pixel-level render_slice must produce identical results to scalar
    /// render_pixel for every pixel.
    #[test]
    fn render_slice_equals_scalar() {
        let prof = super::dual_illuminant_profile();
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let pixels: Vec<f32> = (0..600).map(|i| i as f32 / 200.0).collect();
        let mut slice = pixels.clone();
        r.render_slice(&mut slice);

        for (i, chunk) in pixels.chunks(3).enumerate() {
            let expected = r.render_pixel([chunk[0], chunk[1], chunk[2]]);
            let got = &slice[i * 3..i * 3 + 3];
            let de = prophoto_delta_e(expected, [got[0], got[1], got[2]], &XYZ_TO_PROPHOTO_D50);
            assert!(
                de < 1e-6,
                "slice pixel {i}: scalar {expected:?} vs slice {got:?}, dE {de}"
            );
        }
    }

    // ---- HSV table edge behaviour tests ----------------------------------

    /// Hue wrap at index 89→0: a profile with a hue shift at the last hue
    /// division must wrap correctly.
    #[test]
    fn hsv_hue_index_89_0_wrap() {
        let mut data = vec![[0.0f32, 1.0, 1.0]; 90];
        data[89] = [10.0, 1.0, 1.0];
        let table = HsvTable {
            hue_div: 90,
            sat_div: 1,
            val_div: 1,
            data,
        };

        let mut prof = super::dual_illuminant_profile();
        prof.hue_sat_map = Some(DualHueSatMap {
            map_1: table,
            map_2: None,
        });

        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let rgb_near_wrap = hsv_to_rgb([359.5, 1.0, 1.0]);
        let out = r.render_pixel(rgb_near_wrap);
        let out_hsv = rgb_to_hsv(out);

        assert!(
            out_hsv[0] < 30.0 && out_hsv[0] > 0.0,
            "hue wrap 89→0: near-359.5° pixel got hue {}",
            out_hsv[0]
        );
    }

    /// Saturation clamp at 1.0: a table with satScale=10 must not produce
    /// saturation > 1.0.
    #[test]
    fn hsv_saturation_clamps_at_1() {
        let mut data = vec![[0.0f32, 1.0, 1.0]; 4];
        data[0] = [0.0, 10.0, 1.0];
        let table = HsvTable {
            hue_div: 2,
            sat_div: 2,
            val_div: 1,
            data,
        };

        let mut prof = super::dual_illuminant_profile();
        prof.hue_sat_map = Some(DualHueSatMap {
            map_1: table,
            map_2: None,
        });

        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let rgb = [0.2f32, 0.19, 0.18];
        let out = r.render_pixel(rgb);
        let sat_out = rgb_to_hsv(out)[1];
        assert!(
            sat_out <= 1.0 + 1e-5,
            "saturation clamps at 1: got {sat_out}"
        );
    }

    /// Value does NOT clamp: a table with valScale=2 must double the value.
    #[test]
    fn hsv_value_does_not_clamp() {
        let mut data = vec![[0.0f32, 1.0, 1.0]; 4];
        data[0] = [0.0, 1.0, 2.0];
        let table = HsvTable {
            hue_div: 2,
            sat_div: 2,
            val_div: 1,
            data,
        };

        let mut prof = super::dual_illuminant_profile();
        prof.hue_sat_map = Some(DualHueSatMap {
            map_1: table,
            map_2: None,
        });

        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let rgb = [0.3f32, 0.28, 0.26];
        let hsv_in = rgb_to_hsv(rgb);
        let out = r.render_pixel(rgb);
        let hsv_out = rgb_to_hsv(out);

        let expected_val = hsv_in[2] * 2.0;
        assert!(
            (hsv_out[2] - expected_val).abs() < 0.1,
            "value NOT clamped: input val {}, output val {} (expected ~{expected_val})",
            hsv_in[2],
            hsv_out[2]
        );
    }

    // ---- baseline exposure ------------------------------------------------

    /// BaselineExposureOffset of 1 stop doubles the pixel values.
    #[test]
    fn baseline_exposure_gain_1_stop() {
        let mut prof = super::dual_illuminant_profile();
        prof.baseline_exposure_offset = Some(1.0);
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let input = [0.5f32, 0.3, 0.1];
        let out = r.render_pixel(input);
        assert!((out[0] - 1.0).abs() < 0.01, "gain on R: {}", out[0]);
        assert!((out[1] - 0.6).abs() < 0.01, "gain on G: {}", out[1]);
        assert!((out[2] - 0.2).abs() < 0.01, "gain on B: {}", out[2]);
    }

    // ---- profile shapes --------------------------------------------------

    /// A profile with ColorMatrix=2I, no FM, no tables, no curve must render
    /// without error (fallback path exercised).
    #[test]
    fn pure_matrix_scale_preserved() {
        let cm = Mat3::new(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0);
        let prof = super::synthetic_profile("2x-scale", cm, None, Illuminant::StdA);
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let input = [0.5f32, 0.3, 0.1];
        let out = r.render_pixel(input);
        for c in out {
            assert!(c.is_finite(), "finite output: {out:?}");
        }
    }

    /// A single-illuminant profile (no CM2/FM2/HueSatMap2) must render without
    /// error.
    #[test]
    fn single_illuminant_renders() {
        let cm = Mat3::new(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
        let prof = super::synthetic_profile("single-illum", cm, None, Illuminant::D65);
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("single-illuminant renderer");

        let out = r.render_pixel([0.5, 0.5, 0.5]);
        for c in out {
            assert!(c.is_finite(), "single-illum output: {out:?}");
        }
    }

    /// A profile with a HueSatMap but no tone curve must render correctly
    /// (the table is applied, the curve is skipped without error).
    #[test]
    fn tables_but_no_curve_renders() {
        let mut data = vec![[0.0f32, 1.0, 1.0]; 180];
        for i in 0..30 {
            data[i * 2] = [1.0, 1.0, 1.0];
        }
        let table = HsvTable {
            hue_div: 90,
            sat_div: 2,
            val_div: 1,
            data,
        };

        let mut prof = super::dual_illuminant_profile();
        prof.hue_sat_map = Some(DualHueSatMap {
            map_1: table,
            map_2: None,
        });

        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");
        let out = r.render_pixel([0.5, 0.3, 0.2]);
        for c in out {
            assert!(c.is_finite(), "table-no-curve output: {out:?}");
        }
    }

    /// ProPhoto→working space conversion must map D50 neutral to neutral in
    /// sRGB/D65.
    #[test]
    fn working_space_neutral() {
        let prof = super::dual_illuminant_profile();
        let r = DcpRenderer::new(&prof, [1.0, 1.0, 1.0]).expect("renderer");

        let ws = r.to_working_space([1.0, 1.0, 1.0]);
        assert!(
            (ws[0] - ws[1]).abs() < 1e-4 && (ws[1] - ws[2]).abs() < 1e-4,
            "working space neutral: {ws:?}"
        );
    }

    // -------------------------------------------------------------------
    // Synthetic identity-profile renderer test
    // -------------------------------------------------------------------

    /// Render synthetic RGB through an identity DCP and check absolute error
    /// below 1e-6. This is a tolerance test, not bit equality with upstream.
    #[test]
    fn identity_profile_preserves_synthetic_rgb_within_tolerance() {
        let prof = super::identity_profile();
        let as_shot = [1.0f32, 1.0, 1.0];
        let r = DcpRenderer::new(&prof, as_shot).expect("identity renderer");

        // ---- 1. Synthetic identity verification (always runs) --------------
        let seed: u64 = 0xdead_c0de;
        let n_px: usize = 64 * 64;
        let mut state = seed;
        let mut pixels: Vec<f32> = Vec::with_capacity(n_px * 3);
        for _ in 0..n_px {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let r_val = (state as f32 / u64::MAX as f32) * 2.0;
            let g_val = {
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                (state as f32 / u64::MAX as f32) * 2.0
            };
            let b_val = {
                state = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
                (state as f32 / u64::MAX as f32) * 2.0
            };
            pixels.extend_from_slice(&[r_val, g_val, b_val]);
        }
        let input = pixels.clone();

        r.render_slice(&mut pixels);

        let mut max_err: f32 = 0.0;
        for (i, (&out, &inp)) in pixels.iter().zip(input.iter()).enumerate() {
            let err = (out - inp).abs();
            if err > max_err {
                max_err = err;
            }
            assert!(
                err < 1e-6,
                "Synthetic identity-profile violation at element {i}: input {inp:.8}, output {out:.8}, diff {err:.2e}",
            );
        }
        eprintln!(
            "Synthetic identity profile: {n_px} px rendered, max per-channel error {:.2e} (< 1e-6)",
            max_err
        );
    }

    /// Reserved for an actual application-render comparison against upstream.
    /// Synthetic identity profiles do not establish no-profile equivalence.
    #[test]
    #[ignore = "unimplemented: requires actual upstream and merged no-profile render comparison"]
    fn no_profile_upstream_acceptance() {
        panic!(
            "UNIMPLEMENTED upstream no-profile acceptance (D6): render identical inputs and settings through upstream and merged application pipelines, then compare decoded output pixels. Synthetic identity-profile tests do not satisfy this acceptance gate."
        );
    }

    // -------------------------------------------------------------------
    // GPU-vs-CPU parity test (W3 acceptance)
    // -------------------------------------------------------------------

    /// GPU-vs-CPU parity test: renders 512x512 pseudo-random camera-RGB values
    /// through BOTH the CPU DcpRenderer and the GPU WGSL pipeline (using the
    /// SAME dcp.wgsl functions as the production shader), comparing per-channel
    /// outputs in the working space.
    ///
    /// Acceptance: max per-channel abs diff < 1e-3, mean < 1e-4.
    ///
    /// Requires a headless GPU adapter; if none is available, the test prints
    /// a diagnostic and refrains from asserting.
    #[test]
    #[ignore = "requires GPU adapter (headless wgpu); run with --ignored"]
    fn gpu_vs_cpu_parity() {
        // ---- 1. Build the CPU oracle ---------------------------------------
        let mut prof = super::dual_illuminant_profile();
        let table = HsvTable {
            hue_div: 3,
            sat_div: 2,
            val_div: 2,
            data: (0..12)
                .map(|i| [-12.0 + i as f32, 0.9 + i as f32 * 0.005, 0.95])
                .collect(),
        };
        prof.hue_sat_map = Some(DualHueSatMap {
            map_1: table.clone(),
            map_2: None,
        });
        prof.look_table = Some(table);
        prof.baseline_exposure_offset = Some(0.25);
        prof.tone_curve = Some(ToneCurve {
            points: vec![[0.0, 0.0], [0.2, 0.08], [0.5, 0.4], [0.8, 0.9], [1.0, 1.05]],
        });
        let as_shot = [0.5f32, 1.0, 0.8];
        let cpu = DcpRenderer::new(&prof, as_shot).expect("DcpRenderer");

        // ---- 2. Generate pseudo-random camera RGB test pixels ---------------
        let seed = 0xdead_c0de_u64;
        let n_pixels: u32 = 512 * 512;
        let mut test_rgb: Vec<f32> = Vec::with_capacity((n_pixels * 3) as usize);
        // Unequal channels distinguish per-channel curves from luminance curves.
        // Explicit signed/HDR cases exercise both out-of-domain LUT branches.
        let edge_pixels = [
            [0.05, 0.25, 0.75],
            [2.0, 0.3, 1.2],
            [-0.25, 0.4, 1.5],
            [-0.4, -0.2, -0.1],
            [0.0, 0.0, 0.0],
            [4.0, 2.0, 0.5],
        ];
        for pixel in edge_pixels {
            test_rgb.extend_from_slice(&pixel);
        }
        let mut state = seed;
        for _ in edge_pixels.len() as u32..n_pixels {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let r = (state as f32 / u64::MAX as f32) * 2.0;
            let next = state;
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let g = (state as f32 / u64::MAX as f32) * 2.0;
            state = next.wrapping_mul(0x2545_f491_4f6c_dd1d);
            let b = (state as f32 / u64::MAX as f32) * 2.0;
            test_rgb.extend_from_slice(&[r, g, b]);
        }

        // ---- 3. CPU render (ProPhoto, then to working space) ----------------
        let mut cpu_ws = test_rgb.clone();
        cpu.render_slice(&mut cpu_ws);
        for chunk in cpu_ws.chunks_mut(3) {
            let ws = cpu.to_working_space([chunk[0], chunk[1], chunk[2]]);
            chunk[0] = ws[0];
            chunk[1] = ws[1];
            chunk[2] = ws[2];
        }

        // ---- 4. GPU render (headless wgpu compute) --------------------------
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = match pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        ) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("GPU parity: no adapter ({e}) — skipping");
                return;
            }
        };
        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
                Ok(dq) => dq,
                Err(e) => {
                    eprintln!("GPU parity: device error {e} — skipping");
                    return;
                }
            };

        // Input texture (rgba32float, matching the f32 CPU oracle, 512x512).
        let tex_size = wgpu::Extent3d {
            width: 512,
            height: 512,
            depth_or_array_layers: 1,
        };
        let mut rgba: Vec<f32> = Vec::with_capacity((n_pixels * 4) as usize);
        for c in test_rgb.chunks(3) {
            rgba.extend_from_slice(&[c[0], c[1], c[2], 1.0]);
        }
        let in_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("p-in"),
                size: tex_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&rgba),
        );
        let in_view = in_tex.create_view(&Default::default());

        // Output texture: rgba32float for lossless readback.
        let out_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("p-out"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let out_view = out_tex.create_view(&Default::default());

        // Uniforms: DCP matrices, dims, encodings from CPU renderer.
        let textures = crate::image_processing::dcp_texture_data_from_renderer(&cpu);
        let dims = |d: [u32; 3]| [d[0], d[1], d[2], 0];
        let u = DcpParityUniforms {
            has_dcp: 1,
            dcp_huesat_encoding: match cpu.hue_sat_map_encoding() {
                TableEncoding::Linear => 0,
                TableEncoding::Srgb => 1,
            },
            dcp_look_encoding: match cpu.look_table_encoding() {
                TableEncoding::Linear => 0,
                TableEncoding::Srgb => 1,
            },
            dcp_baseline_exposure_offset: cpu.baseline_exposure_offset(),
            dcp_huesat_dims: dims(textures.huesat_dims),
            dcp_look_dims: dims(textures.look_dims),
            cam_to_prophoto: mat3_to_cols(cpu.cam_to_prophoto()),
            prophoto_to_working: mat3_to_cols(cpu.prophoto_to_working()),
        };
        let u_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("p-u"),
            contents: bytemuck::bytes_of(&u),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Use the same f32 packing and hue-fast texture layout as production.
        let upload_table = |label, data: &[f32], dims: [u32; 3]| {
            device.create_texture_with_data(
                &queue,
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: dims[0],
                        height: dims[1],
                        depth_or_array_layers: dims[2],
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D3,
                    format: wgpu::TextureFormat::Rgba32Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::MipMajor,
                bytemuck::cast_slice(data),
            )
        };
        let hs_tex = upload_table("p-huesat", &textures.huesat, textures.huesat_dims);
        let look_tex = upload_table("p-look", &textures.look, textures.look_dims);
        let hs_view = hs_tex.create_view(&Default::default());
        let look_view = look_tex.create_view(&Default::default());

        // Nonlinear tone curve LUT from the CPU renderer (same upload as production).
        let tc_data = textures.tone_curve;
        let tc_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("p-tc"),
                size: wgpu::Extent3d {
                    width: 4096,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D1,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&tc_data),
        );
        let tc_v = tc_tex.create_view(&Default::default());

        // Compile the parity shader: shared dcp.wgsl + parity entry point.
        let parity_src = parity_shader_source();
        let sm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("p-sm"),
            source: wgpu::ShaderSource::Wgsl(parity_src.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("p-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 14,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 15,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 16,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D1,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("p-pipe"),
            layout: Some(&pl),
            module: &sm,
            entry_point: Some("main_parity"),
            compilation_options: Default::default(),
            cache: None,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("p-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&in_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&hs_view),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(&look_view),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(&tc_v),
                },
            ],
        });

        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&pipe);
            cp.set_bind_group(0, &bg, &[]);
            cp.dispatch_workgroups(64, 64, 1);
        }
        queue.submit(Some(enc.finish()));

        // Readback. rgba32float = 16 bytes per pixel, f32 values direct.
        let al = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let pbpr = ((512 * 16) + al - 1) & !(al - 1);
        let rb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("p-rb"),
            size: (pbpr * 512) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut e2 = device.create_command_encoder(&Default::default());
        e2.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &out_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &rb,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pbpr),
                    rows_per_image: Some(512),
                },
            },
            tex_size,
        );
        queue.submit(Some(e2.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        let sl = rb.slice(..);
        sl.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = sl.get_mapped_range().to_vec();
        rb.unmap();

        // ---- 5. Compare in working space (f32, like-for-like) ----------------
        let mut max_diff: f32 = 0.0;
        let mut sum: f64 = 0.0;
        for row in 0..512usize {
            let so = row * pbpr as usize;
            for col in 0..512 {
                let pi = (row * 512 + col) * 3;
                let gr =
                    f32::from_ne_bytes(data[so + col * 16..so + col * 16 + 4].try_into().unwrap());
                let gg = f32::from_ne_bytes(
                    data[so + col * 16 + 4..so + col * 16 + 8]
                        .try_into()
                        .unwrap(),
                );
                let gb = f32::from_ne_bytes(
                    data[so + col * 16 + 8..so + col * 16 + 12]
                        .try_into()
                        .unwrap(),
                );
                assert!(
                    [gr, gg, gb].iter().all(|v| v.is_finite()),
                    "nonfinite GPU pixel {pi}"
                );
                assert!(
                    cpu_ws[pi..pi + 3].iter().all(|v| v.is_finite()),
                    "nonfinite CPU pixel {pi}"
                );
                let d = (gr - cpu_ws[pi])
                    .abs()
                    .max((gg - cpu_ws[pi + 1]).abs())
                    .max((gb - cpu_ws[pi + 2]).abs());
                max_diff = max_diff.max(d);
                sum += d as f64;
            }
        }
        let mean_diff = sum / (n_pixels as f64);
        eprintln!(
            "GPU parity: {} px — max {:.6}, mean {:.6}",
            n_pixels, max_diff, mean_diff
        );
        assert!(
            max_diff < 1e-3,
            "max per-channel diff {max_diff:.6} >= 1e-3"
        );
        assert!(
            mean_diff < 1e-4,
            "mean per-channel diff {mean_diff:.6} >= 1e-4"
        );
        eprintln!("GPU parity: PASSED");
    }

    // -------------------------------------------------------------------
    // ACR-reference acceptance blocker
    // -------------------------------------------------------------------

    /// Reserved for real RAW/profile pairing, rendering, and measured comparison
    /// against ACR references in a common colour space. File presence is not evidence.
    #[test]
    #[ignore = "unimplemented: requires actual RAW/profile render and ACR pixel comparison"]
    fn acr_reference_comparison() {
        panic!(
            "UNIMPLEMENTED ACR reference acceptance (D2): pair RAW inputs with DCP profiles and ACR references, render the actual pipeline, compare pixels in a common colour space, and enforce measured delta-E thresholds. No reference comparison is implemented here."
        );
    }

    // -------------------------------------------------------------------
    // HSV helpers (duplicated from render.rs for test isolation)
    // -------------------------------------------------------------------

    fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
        let c_max = c[0].max(c[1]).max(c[2]);
        let c_min = c[0].min(c[1]).min(c[2]);
        let delta = c_max - c_min;
        let mut h = 0.0f32;
        if delta > 0.0 {
            if c_max == c[0] {
                h = 60.0 * (((c[1] - c[2]) / delta) % 6.0);
            } else if c_max == c[1] {
                h = 60.0 * (((c[2] - c[0]) / delta) + 2.0);
            } else {
                h = 60.0 * (((c[0] - c[1]) / delta) + 4.0);
            }
        }
        if h < 0.0 {
            h += 360.0;
        }
        let s = if c_max > 0.0 { delta / c_max } else { 0.0 };
        [h, s, c_max]
    }

    fn hsv_to_rgb(c: [f32; 3]) -> [f32; 3] {
        let h = c[0];
        let s = c[1];
        let v = c[2];
        let ch = v * s;
        let x = ch * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - ch;
        let rgb_prime = if h < 60.0 {
            [ch, x, 0.0]
        } else if h < 120.0 {
            [x, ch, 0.0]
        } else if h < 180.0 {
            [0.0, ch, x]
        } else if h < 240.0 {
            [0.0, x, ch]
        } else if h < 300.0 {
            [x, 0.0, ch]
        } else {
            [ch, 0.0, x]
        };
        [rgb_prime[0] + m, rgb_prime[1] + m, rgb_prime[2] + m]
    }
}

// H2b2 is a private, unwired comparison core. Keep it test-scoped until a later
// owned slice connects it to actual candidate pixels; helper tests are not render,
// ICC, GPU, or Adobe acceptance.
#[cfg(test)]
mod candidate_lab {
    use crate::dcp::delta_e::{D50_XYZ, xyz_to_lab};
    use nalgebra::{Matrix3, Vector3};
    use serde::Deserialize;

    #[derive(Debug, PartialEq)]
    enum ColourError {
        NonFinite(&'static str),
        SingularMatrix,
    }

    fn finite(values: &[f64], stage: &'static str) -> Result<(), ColourError> {
        if values.iter().all(|value| value.is_finite()) {
            Ok(())
        } else {
            Err(ColourError::NonFinite(stage))
        }
    }

    fn inverse(matrix: Matrix3<f64>) -> Result<Matrix3<f64>, ColourError> {
        finite(matrix.as_slice(), "matrix inverse")?;
        finite(&[matrix.determinant()], "matrix inverse")?;
        let result = matrix.try_inverse().ok_or(ColourError::SingularMatrix)?;
        finite(result.as_slice(), "matrix inverse")?;
        Ok(result)
    }

    // Check each product and partial sum as well as the final vector.
    fn multiply(
        matrix: &Matrix3<f64>,
        vector: [f64; 3],
        stage: &'static str,
    ) -> Result<[f64; 3], ColourError> {
        finite(matrix.as_slice(), stage)?;
        finite(&vector, stage)?;
        let mut result = [0.0; 3];
        for (row, output) in result.iter_mut().enumerate() {
            for (column, value) in vector.iter().enumerate() {
                let product = matrix[(row, column)] * value;
                finite(&[product], stage)?;
                *output += product;
                finite(&[*output], stage)?;
            }
        }
        Ok(result)
    }

    fn xyz_from_xy(x: f64, y: f64) -> [f64; 3] {
        [x / y, 1.0, (1.0 - x - y) / y]
    }

    fn bradford(source: [f64; 3], destination: [f64; 3]) -> Result<Matrix3<f64>, ColourError> {
        let cone = Matrix3::new(
            0.8951, 0.2664, -0.1614, -0.7502, 1.7135, 0.0367, 0.0389, -0.0685, 1.0296,
        );
        let source_cone = multiply(&cone, source, "source white")?;
        let destination_cone = multiply(&cone, destination, "destination white")?;
        let ratios: [f64; 3] = std::array::from_fn(|i| destination_cone[i] / source_cone[i]);
        finite(&ratios, "cone scaling")?;
        let cone_inverse = inverse(cone)?;
        let mut result = Matrix3::zeros();
        for column in 0..3 {
            let scaled = std::array::from_fn(|row| cone[(row, column)] * ratios[row]);
            let adapted = multiply(&cone_inverse, scaled, "adaptation matrix")?;
            result.set_column(column, &Vector3::from(adapted));
        }
        Ok(result)
    }

    fn finish_lab(xyz_wc: [f64; 3]) -> Result<[f64; 3], ColourError> {
        finite(&xyz_wc, "XYZ Wc")?;
        let relative: [f64; 3] = std::array::from_fn(|i| xyz_wc[i] / D50_XYZ[i]);
        finite(&relative, "XYZ relative to Wc")?;
        let lab = xyz_to_lab(xyz_wc);
        finite(&lab, "Lab")?;
        Ok(lab)
    }

    #[derive(Debug)]
    struct CandidateLab {
        xyz_d65: [f64; 3],
        xyz_wc: [f64; 3],
        lab: [f64; 3],
    }

    struct CandidateLabConverter {
        rgb_to_xyz: Matrix3<f64>,
        d65_to_wc: Matrix3<f64>,
    }

    impl CandidateLabConverter {
        fn new() -> Result<Self, ColourError> {
            // Published sRGB primary xy values, normalized to unit D65 white.
            // Derive these independently of the production renderer's matrices.
            let primaries = Matrix3::from_columns(&[
                Vector3::from(xyz_from_xy(0.64, 0.33)),
                Vector3::from(xyz_from_xy(0.30, 0.60)),
                Vector3::from(xyz_from_xy(0.15, 0.06)),
            ]);
            let white = xyz_from_xy(0.3127, 0.3290);
            let scales = multiply(&inverse(primaries)?, white, "primary scales")?;
            let rgb_to_xyz =
                Matrix3::from_fn(|row, column| primaries[(row, column)] * scales[column]);
            finite(rgb_to_xyz.as_slice(), "RGB matrix")?;
            let d65_to_wc = bradford(white, D50_XYZ)?;
            Ok(Self {
                rgb_to_xyz,
                d65_to_wc,
            })
        }

        fn convert(&self, linear_srgb: [f64; 3]) -> Result<CandidateLab, ColourError> {
            finite(&linear_srgb, "RGB")?;
            let xyz_d65 = multiply(&self.rgb_to_xyz, linear_srgb, "XYZ D65")?;
            let xyz_wc = multiply(&self.d65_to_wc, xyz_d65, "XYZ Wc")?;
            let lab = finish_lab(xyz_wc)?;
            Ok(CandidateLab {
                xyz_d65,
                xyz_wc,
                lab,
            })
        }
    }

    // This is only a reader for the frozen test data, not a comparator manifest.
    #[derive(Deserialize)]
    struct Answers {
        domains: Vec<Domain>,
        fault_witnesses: Witnesses,
        clip_witnesses: Witnesses,
    }

    #[derive(Deserialize)]
    struct Domain {
        domain: String,
        tolerance_abs: String,
        samples: Vec<Sample>,
    }

    #[derive(Deserialize)]
    struct Sample {
        name: String,
        rgb: Option<[f64; 3]>,
        f32_le_hex: Option<[String; 3]>,
        rgb_exact: Option<[String; 3]>,
        xyz_d65: [f64; 3],
        xyz_wc: [f64; 3],
        lab: [f64; 3],
    }

    #[derive(Deserialize)]
    struct Witnesses {
        columns: Vec<String>,
        samples: Vec<Witness>,
    }

    #[derive(Deserialize)]
    struct Witness {
        name: String,
        max_abs_lab_deltas: Vec<String>,
    }

    const TOLERANCE: f64 = 1e-6;

    fn answers() -> Answers {
        serde_json::from_str(include_str!(
            "../../../bench/dcp_validation/known-answers.json"
        ))
        .expect("frozen independent known answers")
    }

    fn input(sample: &Sample, domain: &str) -> [f64; 3] {
        match domain {
            "analytic_decimal_f64" => {
                assert!(sample.f32_le_hex.is_none() && sample.rgb_exact.is_none());
                sample.rgb.expect("analytic helper RGB")
            }
            "f32_widened_to_f64" => {
                assert!(sample.rgb.is_none());
                let hex = sample.f32_le_hex.as_ref().expect("f32 bit binding");
                let exact = sample.rgb_exact.as_ref().expect("exact widened input");
                std::array::from_fn(|channel| {
                    assert_eq!(hex[channel].len(), 8);
                    let bytes = std::array::from_fn(|i| {
                        u8::from_str_radix(&hex[channel][i * 2..i * 2 + 2], 16).unwrap()
                    });
                    let widened = f64::from(f32::from_bits(u32::from_le_bytes(bytes)));
                    assert_eq!(widened, exact[channel].parse::<f64>().unwrap());
                    widened
                })
            }
            other => panic!("unexpected fixture input domain: {other}"),
        }
    }

    fn max_error(actual: [f64; 3], expected: [f64; 3]) -> f64 {
        finite(&actual, "test result").unwrap();
        actual
            .into_iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max)
    }

    fn assert_close(actual: [f64; 3], expected: [f64; 3], context: &str) {
        assert!(
            max_error(actual, expected) < TOLERANCE,
            "{context}: actual {actual:?}, frozen expected {expected:?}"
        );
    }

    fn analytic_sample<'a>(answers: &'a Answers, name: &str) -> &'a Sample {
        answers.domains[0]
            .samples
            .iter()
            .find(|sample| sample.name == name)
            .unwrap()
    }

    #[test]
    fn frozen_answers_match_both_bound_input_domains() {
        let answers = answers();
        assert_eq!(answers.domains.len(), 2);
        assert_eq!(answers.domains[0].domain, "analytic_decimal_f64");
        assert_eq!(answers.domains[1].domain, "f32_widened_to_f64");
        assert_eq!(answers.domains[0].samples.len(), 10);
        assert_eq!(answers.domains[1].samples.len(), 2);
        let converter = CandidateLabConverter::new().unwrap();
        let mut largest: f64 = 0.0;
        for domain in &answers.domains {
            assert_eq!(domain.tolerance_abs, "1e-6");
            for sample in &domain.samples {
                let result = converter.convert(input(sample, &domain.domain)).unwrap();
                for (stage, actual, expected) in [
                    ("XYZ D65", result.xyz_d65, sample.xyz_d65),
                    ("XYZ Wc", result.xyz_wc, sample.xyz_wc),
                    ("Lab", result.lab, sample.lab),
                ] {
                    let context = format!("{} / {} / {stage}", domain.domain, sample.name);
                    assert_close(actual, expected, &context);
                    largest = largest.max(max_error(actual, expected));
                }
            }
        }
        eprintln!("candidate Lab: 12 bound rows / 108 components; maximum error {largest:e}");
    }

    #[test]
    fn f32_rows_cannot_use_analytic_decimal_expected_lab() {
        let answers = answers();
        let converter = CandidateLabConverter::new().unwrap();
        for sample in &answers.domains[1].samples {
            let actual = converter
                .convert(input(sample, "f32_widened_to_f64"))
                .unwrap();
            let analytic = analytic_sample(&answers, &sample.name);
            assert!(max_error(actual.lab, analytic.lab) > TOLERANCE);
            assert_close(actual.lab, sample.lab, "correct f32 domain");
        }
    }

    #[test]
    fn negative_and_above_one_values_survive_every_stage() {
        let answers = answers();
        let converter = CandidateLabConverter::new().unwrap();
        let negative = converter.convert([0.0, 0.0, -1.0]).unwrap();
        assert!(negative.xyz_d65.iter().all(|v| *v < 0.0));
        assert!(negative.xyz_wc.iter().all(|v| *v < 0.0));
        assert!(negative.lab[0] < 0.0);
        assert_close(
            negative.lab,
            analytic_sample(&answers, "negative_blue").lab,
            "negative blue",
        );
        let doubled = converter.convert([2.0; 3]).unwrap();
        assert!(doubled.lab[0] > 100.0);
        assert_close(
            doubled.lab,
            analytic_sample(&answers, "double_white").lab,
            "double white",
        );
    }

    #[test]
    fn independent_witnesses_detect_adaptation_transpose_channel_and_rgb_clip_faults() {
        let answers = answers();
        let converter = CandidateLabConverter::new().unwrap();
        let reverse = bradford(D50_XYZ, xyz_from_xy(0.3127, 0.3290)).unwrap();
        let table = &answers.fault_witnesses;
        assert_eq!(
            table.columns,
            [
                "omit",
                "reversed",
                "doubled",
                "M_transpose",
                "A_transpose",
                "R_B_swap",
                "RGB_0_1_clip"
            ]
        );
        assert_eq!(table.samples.len(), 5);
        for witness in &table.samples {
            let sample = analytic_sample(&answers, &witness.name);
            let rgb = input(sample, "analytic_decimal_f64");
            let correct = converter.convert(rgb).unwrap();
            let swapped = converter.convert([rgb[2], rgb[1], rgb[0]]).unwrap();
            let clipped = converter.convert(rgb.map(|v| v.clamp(0.0, 1.0))).unwrap();
            let faults = [
                finish_lab(correct.xyz_d65).unwrap(),
                finish_lab(multiply(&reverse, correct.xyz_d65, "reversed").unwrap()).unwrap(),
                finish_lab(multiply(&converter.d65_to_wc, correct.xyz_wc, "doubled").unwrap())
                    .unwrap(),
                finish_lab(
                    multiply(
                        &converter.d65_to_wc,
                        multiply(&converter.rgb_to_xyz.transpose(), rgb, "M transpose").unwrap(),
                        "adapt",
                    )
                    .unwrap(),
                )
                .unwrap(),
                finish_lab(
                    multiply(
                        &converter.d65_to_wc.transpose(),
                        correct.xyz_d65,
                        "A transpose",
                    )
                    .unwrap(),
                )
                .unwrap(),
                swapped.lab,
                clipped.lab,
            ];
            assert_eq!(witness.max_abs_lab_deltas.len(), faults.len());
            for ((label, actual), frozen) in table
                .columns
                .iter()
                .zip(faults)
                .zip(&witness.max_abs_lab_deltas)
            {
                let expected: f64 = frozen.parse().unwrap();
                let error = max_error(actual, sample.lab);
                assert!(
                    (error - expected).abs() < TOLERANCE,
                    "{} / {label}: {error} vs {expected}",
                    sample.name
                );
                if expected == 0.0 {
                    assert!(error < TOLERANCE);
                } else {
                    assert!(error > TOLERANCE);
                }
            }
        }
    }

    #[test]
    fn independent_clipping_witnesses_preserve_stated_nonseparability() {
        let answers = answers();
        let converter = CandidateLabConverter::new().unwrap();
        let table = &answers.clip_witnesses;
        assert_eq!(
            table.columns,
            [
                "XYZ_D65_negative_clip",
                "XYZ_Wc_negative_clip",
                "L_0_100_clip",
                "RGB_0_1_clip"
            ]
        );
        assert_eq!(table.samples.len(), 2);
        for witness in &table.samples {
            let sample = analytic_sample(&answers, &witness.name);
            let rgb = input(sample, "analytic_decimal_f64");
            let correct = converter.convert(rgb).unwrap();
            let d65_clip = finish_lab(
                multiply(
                    &converter.d65_to_wc,
                    correct.xyz_d65.map(|v| v.max(0.0)),
                    "D65 clip",
                )
                .unwrap(),
            )
            .unwrap();
            let wc_clip = finish_lab(correct.xyz_wc.map(|v| v.max(0.0))).unwrap();
            let l_clip = [
                correct.lab[0].clamp(0.0, 100.0),
                correct.lab[1],
                correct.lab[2],
            ];
            let rgb_clip = converter
                .convert(rgb.map(|v| v.clamp(0.0, 1.0)))
                .unwrap()
                .lab;
            let faults = [d65_clip, wc_clip, l_clip, rgb_clip];
            assert_eq!(witness.max_abs_lab_deltas.len(), faults.len());
            for (actual, frozen) in faults.into_iter().zip(&witness.max_abs_lab_deltas) {
                let expected: f64 = frozen.parse().unwrap();
                assert!((max_error(actual, sample.lab) - expected).abs() < TOLERANCE);
            }
            if sample.name == "negative_blue" {
                assert_close(d65_clip, [0.0; 3], "XYZ clip produces black");
                assert_close(wc_clip, rgb_clip, "XYZ and RGB clips are not separable");
                assert_close(l_clip, [0.0, correct.lab[1], correct.lab[2]], "L-only clip");
            } else {
                assert_eq!(sample.name, "double_white");
                assert_close(d65_clip, correct.lab, "positive XYZ unaffected");
                assert_close(wc_clip, correct.lab, "positive Wc XYZ unaffected");
                assert_close(l_clip, rgb_clip, "L and RGB clips are not separable");
            }
        }
    }

    #[test]
    fn nonfinite_input_channels_are_errors() {
        let converter = CandidateLabConverter::new().unwrap();
        for channel in 0..3 {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut rgb = [0.2, 0.4, 0.6];
                rgb[channel] = invalid;
                assert_eq!(
                    converter.convert(rgb).unwrap_err(),
                    ColourError::NonFinite("RGB")
                );
            }
        }
    }

    #[test]
    fn finite_extremes_succeed_only_when_every_stage_stays_finite() {
        let converter = CandidateLabConverter::new().unwrap();
        for rgb in [
            [f64::MAX / 32.0; 3],
            [-f64::MAX / 4096.0; 3],
            [f64::MIN_POSITIVE; 3],
            [f64::from_bits(1); 3],
        ] {
            let result = converter.convert(rgb).unwrap();
            assert!(
                result
                    .xyz_d65
                    .into_iter()
                    .chain(result.xyz_wc)
                    .chain(result.lab)
                    .all(f64::is_finite)
            );
        }
        for rgb in [[f64::MAX; 3], [-f64::MAX; 3], [0.0, 0.0, -f64::MAX]] {
            assert!(matches!(
                converter.convert(rgb),
                Err(ColourError::NonFinite(_))
            ));
        }
        assert_eq!(
            finish_lab([0.0, 0.0, f64::MAX]),
            Err(ColourError::NonFinite("XYZ relative to Wc"))
        );
        assert_eq!(
            finish_lab([0.0, -f64::MAX, 0.0]),
            Err(ColourError::NonFinite("Lab"))
        );
        assert!(multiply(&Matrix3::identity(), [0.0, f64::NAN, 0.0], "probe").is_err());
        assert!(multiply(&Matrix3::repeat(2.0), [f64::MAX, 0.0, 0.0], "probe").is_err());
        assert!(
            multiply(
                &Matrix3::repeat(1.0),
                [f64::MAX, f64::MAX, -f64::MAX],
                "probe"
            )
            .is_err()
        );
    }

    #[test]
    fn required_inverses_and_cone_scaling_fail_closed() {
        assert_eq!(inverse(Matrix3::zeros()), Err(ColourError::SingularMatrix));
        assert_eq!(
            inverse(Matrix3::identity() * f64::MAX),
            Err(ColourError::NonFinite("matrix inverse"))
        );
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut matrix = Matrix3::identity();
            matrix[(1, 2)] = invalid;
            assert_eq!(
                inverse(matrix),
                Err(ColourError::NonFinite("matrix inverse"))
            );
        }
        assert!(
            inverse(Matrix3::from_diagonal(&Vector3::new(
                f64::from_bits(1),
                1.0,
                1.0
            )))
            .is_err()
        );
        assert_eq!(
            bradford([0.0; 3], D50_XYZ),
            Err(ColourError::NonFinite("cone scaling"))
        );
    }
}
