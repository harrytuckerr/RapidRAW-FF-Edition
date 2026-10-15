//! CLI validation harness for the RapidRAW DCP render path.
//!
//! Renders RAW files through the shared profile pipeline. Reference comparison
//! is not implemented and exits with failure rather than implying acceptance.
//!
//! ## Usage
//!
//! ```text
//! # Render a DCP through a RAW file (no comparison):
//! cargo run --bin validate_dcp -- render \
//!     --dcp "path/to/profile.dcp" \
//!     --raw "path/to/image.RAF" \
//!     --output "render.tif"
//! # Optional native Look: append --look "look.xmp" --amount 100
//! # Amount is percent; nonzero Looks use the explicit non-HDR [0,1] boundary.
//!
//! # Compare against a reference TIFF:
//! cargo run --bin validate_dcp -- compare \
//!     --dcp "path/to/profile.dcp" \
//!     --raw "path/to/image.RAF" \
//!     --reference "path/to/acr_reference.tif"
//!
//! # Run synthetic self-tests (no assets needed):
//! cargo run --bin validate_dcp -- self-test
//! ```
//!
//! Output TIFF pixels are linear sRGB/Rec.709 working RGB, not display-encoded.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "render" => cmd_render(&args),
        "compare" => cmd_compare(&args),
        "self-test" => cmd_self_test(),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("unknown subcommand: {}", args[1]);
            print_usage();
        }
    }
}

fn print_usage() {
    eprintln!(
        "validate_dcp — DCP render validation harness\n\
         \n\
         USAGE:\n\
           cargo run --bin validate_dcp -- <subcommand> [options]\n\
         \n\
         SUBCOMMANDS:\n\
           render      Render a RAW file through a DCP profile, output a TIFF.\n\
           compare     Unavailable; exits with an explicit error.\n\
           self-test   Run synthetic validation tests (no assets needed).\n\
         \n\
         OPTIONS (render / compare):\n\
           --dcp <path>          Path to the .dcp camera profile.\n\
           --raw <path>          Path to the RAW image file.\n\
           --look <path>         Optional native RGB Look XMP (requires matching base).\n\
           --amount <percent>    Look amount, default 100; zero bypasses the Look.\n\
           --reference <path>    (compare only) Path to the ACR reference TIFF.\n\
           --output <path>       (render only) Output TIFF path.\n\
         \n\
         Nonzero Looks use a bounded non-HDR path; no HDR/Adobe parity claim.\n\
         \n\
         See bench/dcp_validation/README.md for the full procedure.\n"
    );
}

// ---- render subcommand ----------------------------------------------------

fn cmd_render(args: &[String]) {
    if let Err(e) = render_raw(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn render_raw(args: &[String]) -> Result<(), String> {
    use rapidraw_lib::dcp::{
        look_xmp::parse_look_xmp,
        parser::parse_dcp,
        pipeline::render_selected_profile,
        registry::{LookMetadata, ProfileEntry, ProfileRegistry, ProfileSource},
    };
    let dcp_path = parse_flag(args, "--dcp").ok_or("--dcp is required")?;
    let raw_path = parse_flag(args, "--raw")
        .ok_or("--raw is required (render does not use synthetic swatches)")?;
    let output = parse_flag(args, "--output").ok_or("--output is required")?;
    let (look_path, amount) = parse_look_options(args)?;
    let profiles = parse_dcp(Path::new(&dcp_path)).map_err(|e| e.to_string())?;
    if profiles.len() != 1 {
        return Err("Validation render requires a DCP containing exactly one profile".into());
    }
    let profile = &profiles[0];
    let registry = ProfileRegistry::new();
    registry.insert_dcp(ProfileEntry {
        id: profile.id,
        file_path: Path::new(&dcp_path).into(),
        profile_name: profile.profile_name.clone(),
        unique_camera_model: profile.unique_camera_model.clone(),
        source: ProfileSource::Managed,
        size: 0,
        mtime_ms: 0,
    });
    let look_id = if let Some(path) = look_path {
        let look = parse_look_xmp(Path::new(&path))
            .map_err(|e| format!("Cannot parse selected Look '{path}': {e}"))?;
        registry.register_look(LookMetadata {
            uuid: look.uuid.clone(),
            name: look.name,
            group: look.group,
            required_base_profile: look.required_base_profile,
            camera_model_restriction: look.camera_model_restriction,
            file_path: look.file_path,
        });
        eprintln!(
            "Native Look at {amount}%: nonzero amount clamps finite pre-Look ProPhoto channels to [0,1]; HDR/scene-referred fidelity is unverified."
        );
        Some(look.uuid)
    } else {
        None
    };
    let adjustments = serde_json::json!({"profile":{
        "base": profile.id.to_hex(), "look": look_id, "amount": amount,
    }});
    let bytes = std::fs::read(&raw_path).map_err(|e| format!("Cannot read RAW: {e}"))?;
    let image = render_selected_profile(
        &bytes,
        &adjustments,
        &registry,
        false,
        2.5,
        "auto".into(),
        None,
    )?
    .ok_or("No selected profile was rendered")?;
    image
        .save_with_format(&output, image::ImageFormat::Tiff)
        .map_err(|e| format!("Cannot write TIFF: {e}"))?;
    eprintln!(
        "Rendered actual RAW: {}x{} linear working RGB -> {output}. No Adobe parity assertion.",
        image.width(),
        image.height()
    );
    Ok(())
}

fn cmd_compare(_args: &[String]) {
    eprintln!(
        "error: reference comparison is not implemented; no Adobe parity result is available"
    );
    std::process::exit(1);
}

// ---- self-test subcommand -------------------------------------------------

fn cmd_self_test() {
    eprintln!("validate_dcp self-test — synthetic validation");
    eprintln!("Run 'cargo test --all-features' for the full test suite.");
    eprintln!();

    use rapidraw_lib::dcp::delta_e::cie_de2000;

    // Verify Sharma pair 1.
    let lab1 = [50.0, 2.6772, -79.7751];
    let lab2 = [50.0, 0.0, -82.7485];
    let de = cie_de2000(lab1, lab2);
    eprintln!("Sharma pair 1: expected 2.0425, got {:.4}", de);

    if (de - 2.0425).abs() < 1e-3 {
        eprintln!("PASS: CIEDE2000 self-check matches published value.");
    } else {
        eprintln!("FAIL: CIEDE2000 self-check mismatch (got {de}, expected 2.0425)");
        std::process::exit(1);
    }

    // Verify symmetry.
    let d1 = cie_de2000([50.0, 10.0, 20.0], [55.0, 12.0, 18.0]);
    let d2 = cie_de2000([55.0, 12.0, 18.0], [50.0, 10.0, 20.0]);
    if (d1 - d2).abs() < 1e-12 {
        eprintln!("PASS: CIEDE2000 symmetric.");
    } else {
        eprintln!("FAIL: CIEDE2000 not symmetric.");
        std::process::exit(1);
    }

    eprintln!();
    eprintln!("self-test: all checks passed.");
    eprintln!();
    eprintln!("For the full suite including:");
    eprintln!("  - Identity/neutral-axis delta-E tests");
    eprintln!("  - HSV table wrap/clamp behaviour");
    eprintln!("  - Known-matrix round-trips");
    eprintln!("  - render_slice vs scalar parity");
    eprintln!();
    eprintln!("Run: cargo test --all-features dcp::validation");
}

// ---- helpers --------------------------------------------------------------

fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

/// Parse optional Look arguments strictly: a misspelled/missing amount must
/// not accidentally render at the default, and amount without a Look is an error.
fn parse_look_options(args: &[String]) -> Result<(Option<String>, f64), String> {
    let value = |flag: &str| -> Result<Option<String>, String> {
        let positions: Vec<_> = args
            .iter()
            .enumerate()
            .filter(|(_, arg)| arg.as_str() == flag)
            .collect();
        if positions.len() > 1 {
            return Err(format!("{flag} may only be specified once"));
        }
        let Some((index, _)) = positions.first() else {
            return Ok(None);
        };
        args.get(index + 1)
            .filter(|v| !v.is_empty() && !v.starts_with("--"))
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{flag} requires a value"))
    };
    let look = value("--look")?;
    let amount = value("--amount")?;
    if amount.is_some() && look.is_none() {
        return Err("--amount requires --look".into());
    }
    let amount = amount
        .map(|v| {
            v.parse::<f64>()
                .map_err(|_| "--amount must be a numeric percentage".to_string())
        })
        .transpose()?
        .unwrap_or(100.0);
    if !amount.is_finite() || amount < 0.0 || amount > i32::MAX as f64 {
        return Err(
            "--amount must be a finite nonnegative percentage within the SDK integer range".into(),
        );
    }
    Ok((look, amount))
}

#[cfg(test)]
mod cli_tests {
    use super::parse_look_options;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn look_amount_is_percent_with_default_and_exact_zero() {
        assert_eq!(parse_look_options(&args(&[])).unwrap(), (None, 100.0));
        assert_eq!(
            parse_look_options(&args(&["--look", "test.xmp"])).unwrap(),
            (Some("test.xmp".into()), 100.0)
        );
        assert_eq!(
            parse_look_options(&args(&["--look", "test.xmp", "--amount", "0"]))
                .unwrap()
                .1,
            0.0
        );
        assert_eq!(
            parse_look_options(&args(&["--look", "test.xmp", "--amount", "150"]))
                .unwrap()
                .1,
            150.0
        );
    }
    #[test]
    fn malformed_look_options_do_not_silently_default() {
        for values in [
            vec!["--look"],
            vec!["--look", "--output", "out.tif"],
            vec!["--amount", "100"],
            vec!["--look", "test.xmp", "--amount"],
            vec!["--look", "test.xmp", "--amount", "NaN"],
            vec!["--look", "test.xmp", "--amount", "-1"],
            vec!["--look", "test.xmp", "--amount", "invalid"],
            vec!["--look", "a.xmp", "--look", "b.xmp"],
        ] {
            assert!(parse_look_options(&args(&values)).is_err(), "{values:?}");
        }
    }
}
