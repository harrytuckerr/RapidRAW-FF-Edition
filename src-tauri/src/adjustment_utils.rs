use image::DynamicImage;
use std::borrow::Cow;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::image_processing::{
    Crop, IntoCowImage, apply_coarse_rotation, apply_crop, apply_flip, apply_geometry_warp,
    apply_rotation,
};

pub fn hydrate_sub_masks(
    sub_masks: &mut Vec<serde_json::Value>,
    cache: &mut HashMap<String, serde_json::Value>,
) {
    for sub_mask in sub_masks {
        let id = sub_mask
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if id.is_empty() {
            continue;
        }

        if let Some(params) = sub_mask
            .get_mut("parameters")
            .and_then(|p| p.as_object_mut())
        {
            let keys_to_check = ["mask_data_base64", "maskDataBase64"];
            for key in keys_to_check {
                if params.contains_key(key) {
                    let val = params.get(key).unwrap();
                    if !val.is_null() {
                        cache.insert(id.clone(), val.clone());
                    } else {
                        if let Some(cached_data) = cache.get(&id) {
                            params.insert(key.to_string(), cached_data.clone());
                        }
                    }
                }
            }
        }
    }
}

pub fn hydrate_adjustments(state: &tauri::State<AppState>, adjustments: &mut serde_json::Value) {
    let mut cache = state.patch_cache.lock().unwrap();

    if let Some(patches) = adjustments
        .get_mut("aiPatches")
        .and_then(|v| v.as_array_mut())
    {
        for patch in patches {
            let id = patch
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if !id.is_empty() {
                let has_data = patch.get("patchData").is_some_and(|v| !v.is_null());

                if has_data {
                    if let Some(data) = patch.get("patchData") {
                        cache.insert(id.clone(), data.clone());
                    }
                } else {
                    if let Some(cached_data) = cache.get(&id) {
                        patch["patchData"] = cached_data.clone();
                    }
                }
            }

            if let Some(sub_masks) = patch.get_mut("subMasks").and_then(|v| v.as_array_mut()) {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }

    if let Some(masks) = adjustments.get_mut("masks").and_then(|v| v.as_array_mut()) {
        for mask_container in masks {
            if let Some(sub_masks) = mask_container
                .get_mut("subMasks")
                .and_then(|v| v.as_array_mut())
            {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }
}

pub fn apply_orientation_and_flip<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> Cow<'a, DynamicImage> {
    let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    let flip_horizontal = adjustments["flipHorizontal"].as_bool().unwrap_or(false);
    let flip_vertical = adjustments["flipVertical"].as_bool().unwrap_or(false);

    let coarse_rotated_image = apply_coarse_rotation(image.into_cow(), orientation_steps);
    apply_flip(coarse_rotated_image, flip_horizontal, flip_vertical)
}

pub fn apply_spatial_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let rotation_degrees = adjustments["rotation"].as_f64().unwrap_or(0.0) as f32;

    let flipped_image = apply_orientation_and_flip(image, adjustments);
    let rotated_image = apply_rotation(flipped_image, rotation_degrees);

    let crop_data: Option<Crop> = serde_json::from_value(adjustments["crop"].clone()).ok();
    let crop_json = serde_json::to_value(crop_data).unwrap_or(serde_json::Value::Null);
    let cropped_image = apply_crop(rotated_image, &crop_json);

    let unscaled_crop_offset = crop_data.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));

    (cropped_image, unscaled_crop_offset)
}

pub fn apply_all_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let start_time = std::time::Instant::now();
    let image = image.into_cow();

    let warped_image = apply_geometry_warp(image, adjustments);
    let relit_image = crate::relight::apply_relight(warped_image, adjustments);
    let fogged_image = crate::fog::apply_fog(relit_image, adjustments);
    let blurred_image = crate::lens_blur::apply_lens_blur(fogged_image, adjustments);

    let (cropped_image, unscaled_crop_offset) =
        apply_spatial_transformations(blurred_image, adjustments);

    let total_duration = start_time.elapsed();
    log::info!("apply_all_transformations took {:.2?}", total_duration);

    (cropped_image, unscaled_crop_offset)
}

// Strict, additive input capture. The normal hydration wrappers above retain
// their established tolerant, AppState-backed behavior unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnapshotHydrationError {
    Invalid(String),
    MissingPayload { domain: &'static str, id: String },
    ConflictingPayload { domain: &'static str, id: String },
    ConflictingAliases { id: String },
    UnknownVisibleMask(String),
    TooManyMasks(usize),
}

impl std::fmt::Display for SnapshotHydrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Strict creative snapshot hydration: {self:?}")
    }
}
impl std::error::Error for SnapshotHydrationError {}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SnapshotPayloadDomain {
    Patch,
    Bitmap,
}
impl SnapshotPayloadDomain {
    fn label(self) -> &'static str {
        match self {
            Self::Patch => "patchData",
            Self::Bitmap => "submask bitmap",
        }
    }
}
type SnapshotPayloads = HashMap<(SnapshotPayloadDomain, String), serde_json::Value>;

fn snapshot_invalid(message: &str) -> SnapshotHydrationError {
    SnapshotHydrationError::Invalid(message.into())
}
fn snapshot_id(value: &serde_json::Value) -> String {
    value["id"].as_str().unwrap_or_default().to_owned()
}
fn snapshot_patch_visible(value: &serde_json::Value) -> bool {
    // Exactly composite_patches_on_image's missing/nonboolean default.
    value["visible"].as_bool().unwrap_or(true)
}
fn snapshot_visible(value: &serde_json::Value) -> Result<bool, SnapshotHydrationError> {
    value["visible"]
        .as_bool()
        .ok_or_else(|| snapshot_invalid("Mask visibility must be boolean"))
}
fn snapshot_seed(
    payloads: &mut SnapshotPayloads,
    domain: SnapshotPayloadDomain,
    id: String,
    value: &serde_json::Value,
) -> Result<(), SnapshotHydrationError> {
    if id.is_empty() {
        return Ok(());
    }
    let key = (domain, id.clone());
    if let Some(old) = payloads.get(&key) {
        if old != value {
            return Err(SnapshotHydrationError::ConflictingPayload {
                domain: domain.label(),
                id,
            });
        }
    } else {
        payloads.insert(key, value.clone());
    }
    Ok(())
}
fn snapshot_image(data: &str, data_url: bool) -> Result<(), SnapshotHydrationError> {
    use base64::Engine as _;
    let raw = if data_url {
        data.split_once(',').map_or(data, |(_, b)| b)
    } else {
        data
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| snapshot_invalid("Invalid visible inline image base64"))?;
    image::load_from_memory(&bytes)
        .map_err(|_| snapshot_invalid("Invalid visible inline image"))?;
    Ok(())
}
fn snapshot_explicit_patch_mask(patch: &serde_json::Value) -> Result<bool, SnapshotHydrationError> {
    match patch["patchData"].get("mask") {
        None | Some(serde_json::Value::Null) => Ok(false),
        Some(serde_json::Value::String(s)) => Ok(!s.is_empty()),
        _ => Err(snapshot_invalid("Patch mask must be an inline string")),
    }
}
fn snapshot_prune_hidden_submasks(values: &mut Vec<serde_json::Value>) {
    // Only structurally malformed AND explicitly hidden entries are no-ops.
    // Valid hidden entries, including their order and parameters, are retained.
    values.retain(|v| {
        v["visible"].as_bool() != Some(false)
            || serde_json::from_value::<crate::mask_generation::SubMask>(v.clone()).is_ok()
    });
}
fn snapshot_visit_submasks(
    value: &mut serde_json::Value,
    mut visit: impl FnMut(&mut serde_json::Value) -> Result<(), SnapshotHydrationError>,
) -> Result<(), SnapshotHydrationError> {
    if let Some(patches) = value.get_mut("aiPatches").and_then(|v| v.as_array_mut()) {
        for patch in patches.iter_mut().filter(|p| snapshot_patch_visible(p)) {
            if snapshot_explicit_patch_mask(patch)? {
                continue;
            }
            let subs = patch
                .get_mut("subMasks")
                .and_then(|v| v.as_array_mut())
                .ok_or_else(|| snapshot_invalid("Visible patch needs inline mask or subMasks"))?;
            snapshot_prune_hidden_submasks(subs);
            if subs.is_empty() {
                return Err(snapshot_invalid(
                    "Visible patch needs nonempty generated subMasks",
                ));
            }
            for sub in subs {
                if snapshot_visible(sub)? {
                    visit(sub)?;
                }
            }
        }
    }
    if let Some(masks) = value.get_mut("masks").and_then(|v| v.as_array_mut()) {
        for mask in masks {
            if !snapshot_visible(mask)? {
                continue;
            }
            let subs = mask
                .get_mut("subMasks")
                .and_then(|v| v.as_array_mut())
                .ok_or_else(|| snapshot_invalid("Visible mask subMasks must be an array"))?;
            snapshot_prune_hidden_submasks(subs);
            for sub in subs {
                if snapshot_visible(sub)? {
                    visit(sub)?;
                }
            }
        }
    }
    Ok(())
}
fn snapshot_bitmap_kind(sub: &serde_json::Value) -> Result<bool, SnapshotHydrationError> {
    // Deserialize the real public shape; hidden entries never reach this path.
    let typed: crate::mask_generation::SubMask = serde_json::from_value(sub.clone())
        .map_err(|e| SnapshotHydrationError::Invalid(format!("Visible submask shape: {e}")))?;
    if typed.mask_type != "all" && !typed.parameters.is_object() {
        return Err(snapshot_invalid(
            "Visible mask parameters must be an object",
        ));
    }
    match typed.mask_type.as_str() {
        "ai-subject" | "ai-foreground" | "ai-sky" | "ai-depth" | "quick-eraser" => Ok(true),
        "radial" | "linear" | "brush" | "clone" | "heal" | "liquify" | "retouch" | "flow"
        | "color" | "luminance" | "all" => Ok(false),
        _ => Err(SnapshotHydrationError::UnknownVisibleMask(typed.mask_type)),
    }
}
fn snapshot_alias(
    sub: &serde_json::Value,
) -> Result<Option<serde_json::Value>, SnapshotHydrationError> {
    let params = sub
        .get("parameters")
        .and_then(|v| v.as_object())
        .ok_or_else(|| snapshot_invalid("Bitmap mask parameters must be an object"))?;
    let a = params.get("mask_data_base64").filter(|v| !v.is_null());
    let b = params.get("maskDataBase64").filter(|v| !v.is_null());
    if let (Some(a), Some(b)) = (a, b) {
        if a != b {
            return Err(SnapshotHydrationError::ConflictingAliases {
                id: snapshot_id(sub),
            });
        }
    }
    let selected = b.or(a);
    if selected.is_some_and(|v| !v.is_string() || v.as_str().is_some_and(str::is_empty)) {
        return Err(snapshot_invalid(
            "Visible bitmap payload must be a nonempty string",
        ));
    }
    Ok(selected.cloned())
}
fn snapshot_validate_ai_parameters(sub: &serde_json::Value) -> Result<(), SnapshotHydrationError> {
    use crate::ai_processing::{
        AiDepthMaskParameters, AiForegroundMaskParameters, AiSkyMaskParameters,
        AiSubjectMaskParameters,
    };
    let p = sub["parameters"].clone();
    let result = match sub["type"].as_str().unwrap_or_default() {
        "ai-subject" | "quick-eraser" => {
            serde_json::from_value::<AiSubjectMaskParameters>(p).map(|_| ())
        }
        "ai-foreground" => serde_json::from_value::<AiForegroundMaskParameters>(p).map(|_| ()),
        "ai-sky" => serde_json::from_value::<AiSkyMaskParameters>(p).map(|_| ()),
        "ai-depth" => serde_json::from_value::<AiDepthMaskParameters>(p).map(|_| ()),
        _ => return Ok(()),
    };
    result.map_err(|e| SnapshotHydrationError::Invalid(format!("Visible bitmap parameters: {e}")))
}

// Explicit ordinary-mask draft provenance, never inferred from missing data.
// The caller removes only validated entries from its request-local clone.
fn snapshot_is_bitmap_draft(sub: &serde_json::Value) -> Result<bool, SnapshotHydrationError> {
    let Some(state) = sub.get("bitmapState") else { return Ok(false) };
    if state.as_str() != Some("draft") {
        return Err(snapshot_invalid("Invalid bitmap draft state"));
    }
    if snapshot_id(sub).is_empty() || !snapshot_bitmap_kind(sub)?
        || !matches!(sub["type"].as_str(), Some("ai-subject" | "ai-foreground" | "ai-sky" | "ai-depth"))
    {
        return Err(snapshot_invalid("Invalid ordinary bitmap draft"));
    }
    snapshot_visible(sub)?;
    if !sub["opacity"].as_f64().is_some_and(|v| v.is_finite() && (0.0..=100.0).contains(&v)) {
        return Err(snapshot_invalid("Invalid bitmap draft opacity"));
    }
    if snapshot_alias(sub)?.is_some() {
        return Err(snapshot_invalid("Bitmap draft contains completed payload"));
    }
    let params = sub["parameters"].as_object().unwrap();
    let kind = sub["type"].as_str().unwrap();
    for (key, value) in params {
        if matches!(key.as_str(), "maskDataBase64" | "mask_data_base64") {
            // Only absent/null payloads are drafts; even empty strings are errors.
            if !value.is_null() { return Err(snapshot_invalid("Invalid draft payload")) }
            continue;
        }
        let allowed = match key.as_str() {
            "grow" | "feather" => true,
            "startX" | "startY" | "endX" | "endY" => kind == "ai-subject",
            "minDepth" | "maxDepth" | "minFade" | "maxFade" => kind == "ai-depth",
            _ => false,
        };
        if !allowed || !value.as_f64().is_some_and(|v| v.is_finite() && (v as f32).is_finite()) {
            return Err(snapshot_invalid("Invalid or completed bitmap draft parameters"));
        }
    }
    Ok(true)
}

fn snapshot_omit_ordinary_bitmap_drafts(value: &mut serde_json::Value) -> Result<(), SnapshotHydrationError> {
    if let Some(masks) = value.get_mut("masks").and_then(|v| v.as_array_mut()) {
        for mask in masks {
            let hidden_parent = mask["visible"].as_bool() == Some(false);
            if let Some(subs) = mask.get_mut("subMasks").and_then(|v| v.as_array_mut()) {
                for index in (0..subs.len()).rev() {
                    let hidden = hidden_parent || subs[index]["visible"].as_bool() == Some(false);
                    let draft = snapshot_is_bitmap_draft(&subs[index]);
                    // Valid hidden drafts are empty too. Other hidden content keeps
                    // the existing visitor/pruning policy, never new draft errors.
                    let omit = if hidden { matches!(draft, Ok(true)) } else { draft? };
                    if omit { subs.remove(index); }
                }
            }
        }
    }
    Ok(())
}

/// Fresh map for this exact snapshot only. No state/cache/path is accepted.
/// Canonical camelCase is written because actual AI parameter serde uses it;
/// a supplied snake alias is preserved and filled with the identical payload.
/// Hidden malformed typed entries may be pruned from this NEW clone only.
pub(crate) fn hydrate_snapshot_adjustments(
    input: &serde_json::Value,
) -> Result<serde_json::Value, SnapshotHydrationError> {
    if !input.is_object() {
        return Err(snapshot_invalid("Adjustments must be an object"));
    }
    let mut value = input.clone();
    for name in ["aiPatches", "masks"] {
        if let Some(v) = value.get(name) {
            if !v.is_null() && !v.is_array() {
                return Err(snapshot_invalid("Mask/patch collection must be an array"));
            }
        }
    }
    snapshot_omit_ordinary_bitmap_drafts(&mut value)?;
    if let Some(masks) = value.get_mut("masks").and_then(|v| v.as_array_mut()) {
        masks.retain(|m| {
            m["visible"].as_bool() != Some(false)
                || serde_json::from_value::<crate::mask_generation::MaskDefinition>(m.clone())
                    .is_ok()
        });
    }
    let mut payloads = SnapshotPayloads::new();
    if let Some(patches) = value.get("aiPatches").and_then(|v| v.as_array()) {
        for patch in patches.iter().filter(|p| snapshot_patch_visible(p)) {
            if let Some(data) = patch.get("patchData").filter(|v| !v.is_null()) {
                if !data.is_object() {
                    return Err(snapshot_invalid("patchData must be an object"));
                }
                snapshot_seed(
                    &mut payloads,
                    SnapshotPayloadDomain::Patch,
                    snapshot_id(patch),
                    data,
                )?;
            }
        }
    }
    if let Some(patches) = value.get_mut("aiPatches").and_then(|v| v.as_array_mut()) {
        for patch in patches.iter_mut().filter(|p| snapshot_patch_visible(p)) {
            if patch
                .get("patchData")
                .is_none_or(serde_json::Value::is_null)
            {
                let id = snapshot_id(patch);
                patch["patchData"] = payloads
                    .get(&(SnapshotPayloadDomain::Patch, id.clone()))
                    .cloned()
                    .ok_or(SnapshotHydrationError::MissingPayload {
                        domain: "patchData",
                        id,
                    })?;
            }
            let color = patch["patchData"]["color"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| snapshot_invalid("Visible patch color is required"))?;
            snapshot_image(color, false)?;
            if snapshot_explicit_patch_mask(patch)? {
                snapshot_image(patch["patchData"]["mask"].as_str().unwrap(), false)?;
            }
        }
    }
    snapshot_visit_submasks(&mut value, |sub| {
        if snapshot_bitmap_kind(sub)? {
            if let Some(data) = snapshot_alias(sub)? {
                snapshot_seed(
                    &mut payloads,
                    SnapshotPayloadDomain::Bitmap,
                    snapshot_id(sub),
                    &data,
                )?;
            }
        }
        Ok(())
    })?;
    snapshot_visit_submasks(&mut value, |sub| {
        if !snapshot_bitmap_kind(sub)? {
            return Ok(());
        }
        let id = snapshot_id(sub);
        let data = match snapshot_alias(sub)? {
            Some(data) => data,
            None => payloads
                .get(&(SnapshotPayloadDomain::Bitmap, id.clone()))
                .cloned()
                .ok_or(SnapshotHydrationError::MissingPayload {
                    domain: "submask bitmap",
                    id,
                })?,
        };
        snapshot_image(data.as_str().unwrap(), true)?;
        let params = sub["parameters"].as_object_mut().unwrap();
        if params.contains_key("mask_data_base64") {
            params.insert("mask_data_base64".into(), data.clone());
        }
        params.insert("maskDataBase64".into(), data);
        snapshot_validate_ai_parameters(sub)
    })?;
    let mut active = 0;
    if let Some(masks) = value.get("masks").and_then(|v| v.as_array()) {
        for mask in masks {
            if snapshot_visible(mask)? {
                let definition =
                    serde_json::from_value::<crate::mask_generation::MaskDefinition>(mask.clone())
                        .map_err(|e| {
                            SnapshotHydrationError::Invalid(format!("Visible mask shape: {e}"))
                        })?;
                if !definition.sub_masks.is_empty() {
                    active += 1;
                }
            }
        }
    }
    if active > crate::image_processing::MAX_MASKS {
        return Err(SnapshotHydrationError::TooManyMasks(active));
    }
    Ok(value)
}
