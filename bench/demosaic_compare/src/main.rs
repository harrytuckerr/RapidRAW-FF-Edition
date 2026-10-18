//! Renders a raw file through RapidRaw's current demosaic path and dumps the
//! result as linear camera RGB (no white balance, no colour matrix), so it can
//! be compared against another demosaic with identical colour processing.
//!
//! Mirrors `develop_internal` in src-tauri/src/raw_processing.rs:
//! white level forced to u32::MAX before rawler's Rescale step, Quality
//! demosaic, then RapidRaw's own rescale back to 0..1.
//!
//! Usage: xtrans-current <input.RAF> <out_prefix>
//! Writes <out_prefix>.f32 (little-endian RGB f32, row-major) and <out_prefix>.json.

use anyhow::{Result, anyhow};
use rawler::{
    decoders::RawDecodeParams,
    imgop::develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
    rawimage::{RawImage, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use std::io::Write;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err(anyhow!("usage: xtrans-current <input.RAF> <out_prefix>"));
    }
    let (input, out_prefix) = (&args[1], &args[2]);

    let bytes = std::fs::read(input)?;
    let source = RawSource::new_from_slice(&bytes);
    let decoder = rawler::get_decoder(&source)?;
    let mut raw: RawImage = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    let white = raw.whitelevel.0.first().cloned().unwrap_or(u16::MAX as u32) as f32;
    let black = raw.blacklevel.levels.first().map(|r| r.as_f32()).unwrap_or(0.0);
    let (cfa_name, cfa_w, cfa_h) = match &raw.photometric {
        RawPhotometricInterpretation::Cfa(c) => (c.cfa.name.clone(), c.cfa.width, c.cfa.height),
        _ => (String::from("none"), 0, 0),
    };
    let wb = raw.wb_coeffs;
    let sensor = (raw.width, raw.height);
    let active_area = format!("{:?}", raw.active_area);
    let crop_area = format!("{:?}", raw.crop_area);

    // Same quirk as RapidRaw: rawler rescales against u32::MAX, not the real white level.
    for level in raw.whitelevel.0.iter_mut() {
        *level = u32::MAX;
    }

    let mut developer = RawDevelop::default();
    developer.steps = vec![
        ProcessingStep::Rescale,
        ProcessingStep::Demosaic,
        ProcessingStep::CropActiveArea,
        ProcessingStep::CropDefault,
    ];
    developer.demosaic_algorithm = DemosaicAlgorithm::Quality;

    let started = Instant::now();
    let intermediate = developer.develop_intermediate(&raw)?;
    let develop_ms = started.elapsed().as_millis();

    let rescale = (u32::MAX as f32 - black) / (white - black).max(1.0);
    let dim = intermediate.dim();
    let pixels = match intermediate {
        Intermediate::ThreeColor(p) => p,
        _ => return Err(anyhow!("expected a three-colour image after demosaic")),
    };

    let mut buf = Vec::with_capacity(dim.w * dim.h * 12);
    for px in &pixels.data {
        for c in px {
            buf.extend_from_slice(&(c * rescale).to_le_bytes());
        }
    }
    std::fs::File::create(format!("{out_prefix}.f32"))?.write_all(&buf)?;

    let meta = serde_json::json!({
        "input": input,
        "make": raw.make,
        "model": raw.model,
        "width": dim.w,
        "height": dim.h,
        "sensor_width": sensor.0,
        "sensor_height": sensor.1,
        "black": black,
        "white": white,
        "wb_coeffs": wb,
        "cfa_name": cfa_name,
        "cfa_size": [cfa_w, cfa_h],
        "active_area": active_area,
        "crop_area": crop_area,
        "develop_ms": develop_ms,
    });
    std::fs::write(format!("{out_prefix}.json"), serde_json::to_string_pretty(&meta)?)?;
    println!("{}", serde_json::to_string_pretty(&meta)?);
    Ok(())
}
