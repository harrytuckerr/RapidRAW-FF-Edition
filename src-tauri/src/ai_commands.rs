use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose};
use image::{GrayImage, ImageFormat};

use crate::ai_connector;
use crate::ai_processing::{
    AiDepthMaskParameters, AiForegroundMaskParameters, AiSkyMaskParameters,
    AiSubjectMaskParameters, CachedDepthMap, generate_image_embeddings, get_or_init_ai_models,
    run_depth_anything_model, run_sam_decoder, run_sky_seg_model, run_u2netp_model,
};
use crate::app_settings::load_settings;
use crate::app_state::{AiTaskGuard, AppState};
use crate::cache_utils::GEOMETRY_KEYS;
use crate::get_cached_full_warped_image;

#[tauri::command]
pub fn cancel_ai_task(task_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let tasks = state.active_ai_tasks.lock().unwrap();
    if let Some(token) = tasks.get(&task_id) {
        token.cancel();
    }
    Ok(())
}

fn encode_to_base64_png(image: &GrayImage) -> Result<String, String> {
    let mut buf = Cursor::new(Vec::new());
    image
        .write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let base64_str = general_purpose::STANDARD.encode(buf.get_ref());
    Ok(format!("data:image/png;base64,{}", base64_str))
}

fn full_reference_for_ai_consumer(
    state: &tauri::State<'_, AppState>,
    request: &std::sync::Arc<crate::image_loader::ConsumerRequest>,
    js_adjustments: &serde_json::Value,
) -> Result<std::sync::Arc<image::DynamicImage>, String> {
    let result = if request.is_keyed() {
        request.full_reference()
    } else {
        get_cached_full_warped_image(state, js_adjustments)
    };
    request.check()?;
    result
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_foreground_mask(
    js_adjustments: serde_json::Value,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    task_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiForegroundMaskParameters, String> {
    let _guard = task_id
        .as_ref()
        .map(|id| AiTaskGuard::new(&state.active_ai_tasks, id.clone()));
    let cancel_flag = _guard.as_ref().map(|g| &g.token);
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        None,
        Some(js_adjustments.clone()),
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let models_result = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
    request.check()?;
    let warped_image = warped_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let full_mask_result =
        run_u2netp_model(warped_image.as_ref(), &models.u2netp).map_err(|e| e.to_string());
    request.check()?;
    let full_mask_image = full_mask_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let encoded_result = encode_to_base64_png(&full_mask_image);
    request.check()?;
    let base64_data = encoded_result?;
    request.check()?;

    Ok(AiForegroundMaskParameters {
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_sky_mask(
    js_adjustments: serde_json::Value,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    task_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiSkyMaskParameters, String> {
    let _guard = task_id
        .as_ref()
        .map(|id| AiTaskGuard::new(&state.active_ai_tasks, id.clone()));
    let cancel_flag = _guard.as_ref().map(|g| &g.token);
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        None,
        Some(js_adjustments.clone()),
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let models_result = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
    request.check()?;
    let warped_image = warped_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let full_mask_result =
        run_sky_seg_model(warped_image.as_ref(), &models.sky_seg).map_err(|e| e.to_string());
    request.check()?;
    let full_mask_image = full_mask_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let encoded_result = encode_to_base64_png(&full_mask_image);
    request.check()?;
    let base64_data = encoded_result?;
    request.check()?;

    Ok(AiSkyMaskParameters {
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_depth_mask(
    js_adjustments: serde_json::Value,
    path: String,
    min_depth: f32,
    max_depth: f32,
    min_fade: f32,
    max_fade: f32,
    feather: f32,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    task_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiDepthMaskParameters, String> {
    let _guard = task_id
        .as_ref()
        .map(|id| AiTaskGuard::new(&state.active_ai_tasks, id.clone()));
    let cancel_flag = _guard.as_ref().map(|g| &g.token);
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        Some(path.as_str()),
        Some(js_adjustments.clone()),
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let models_result = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let cached_depth = if request.is_keyed() {
        let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
        request.check()?;
        let warped_image = warped_result?;
        if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
            return Err("Task cancelled".to_string());
        }

        let depth_result = run_depth_anything_model(warped_image.as_ref(), &models.depth_anything)
            .map_err(|e| e.to_string());
        request.check()?;
        let depth_image = depth_result?;
        CachedDepthMap {
            path_hash: path_hash.clone(),
            depth_image,
            original_size: (warped_image.width(), warped_image.height()),
        }
    } else {
        let cached = {
            let ai_state_lock = state.ai_state.lock().unwrap();
            let ai_state = ai_state_lock.as_ref().unwrap();
            ai_state
                .depth_map
                .as_ref()
                .filter(|cached| cached.path_hash == path_hash)
                .cloned()
        };
        request.check()?;
        if let Some(cached) = cached {
            cached
        } else {
            let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
            request.check()?;
            let warped_image = warped_result?;
            if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
                return Err("Task cancelled".to_string());
            }

            let depth_result =
                run_depth_anything_model(warped_image.as_ref(), &models.depth_anything)
                    .map_err(|e| e.to_string());
            request.check()?;
            let depth_image = depth_result?;
            let new_cache = CachedDepthMap {
                path_hash: path_hash.clone(),
                depth_image,
                original_size: (warped_image.width(), warped_image.height()),
            };

            request.check()?;
            let cache_copy = new_cache.clone();
            let displaced = {
                let mut ai_state_lock = state.ai_state.lock().unwrap();
                let ai_state = ai_state_lock.as_mut().unwrap();
                std::mem::replace(&mut ai_state.depth_map, Some(cache_copy))
            };
            drop(displaced);
            request.check()?;
            new_cache
        }
    };

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let raw_depth_fullres = image::imageops::resize(
        &cached_depth.depth_image,
        cached_depth.original_size.0,
        cached_depth.original_size.1,
        image::imageops::FilterType::Triangle,
    );
    request.check()?;

    let encoded_result = encode_to_base64_png(&raw_depth_fullres);
    request.check()?;
    let base64_data = encoded_result?;
    request.check()?;

    Ok(AiDepthMaskParameters {
        min_depth,
        max_depth,
        min_fade,
        max_fade,
        feather,
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[tauri::command]
pub async fn generate_full_image_depth_map(
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        None,
        None,
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;
    let models_result = crate::ai_processing::get_or_init_ai_models(
        &app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    // Keep this request's full source, AsShot and captured settings through map generation.
    let loaded_image = request.loaded().clone();
    let source_image =
        crate::mask_generation::build_full_source_image(&loaded_image.image, loaded_image.is_raw);
    request.check()?;

    let depth_result = crate::ai_processing::run_depth_anything_model(
        source_image.as_ref(),
        &models.depth_anything,
    )
    .map_err(|e| e.to_string());
    request.check()?;
    let depth_img = depth_result?;

    let encoded_result =
        crate::effect_maps::encode_source_space_map(&image::DynamicImage::ImageLuma8(depth_img));
    request.check()?;
    let encoded = encoded_result?;
    request.check()?;
    Ok(encoded)
}

#[tauri::command]
pub async fn generate_relight_normal_map(
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        None,
        None,
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;
    let normal_model_result = crate::ai_processing::get_or_init_normal_model(
        &app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string());
    request.check()?;
    let normal_model = normal_model_result?;

    // Keep this request's full source, AsShot and captured settings through map generation.
    let loaded_image = request.loaded().clone();
    let source_image =
        crate::mask_generation::build_full_source_image(&loaded_image.image, loaded_image.is_raw);
    request.check()?;

    let normal_result =
        crate::ai_processing::run_normal_model(source_image.as_ref(), normal_model.as_ref())
            .map_err(|e| e.to_string());
    request.check()?;
    let normal_img = normal_result?;

    let encoded_result =
        crate::effect_maps::encode_source_space_map(&image::DynamicImage::ImageRgba8(normal_img));
    request.check()?;
    let encoded = encoded_result?;
    request.check()?;
    Ok(encoded)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_subject_mask(
    js_adjustments: serde_json::Value,
    path: String,
    start_point: (f64, f64),
    end_point: (f64, f64),
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    skip_refinement: Option<bool>,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    task_id: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiSubjectMaskParameters, String> {
    let _guard = task_id
        .as_ref()
        .map(|id| AiTaskGuard::new(&state.active_ai_tasks, id.clone()));
    let cancel_flag = _guard.as_ref().map(|g| &g.token);
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        Some(path.as_str()),
        Some(js_adjustments.clone()),
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let models_result = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
    request.check()?;
    let warped_image = warped_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let embeddings = if request.is_keyed() {
        let embeddings_result =
            generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
                .map_err(|e| e.to_string());
        request.check()?;
        let mut new_embeddings = embeddings_result?;
        new_embeddings.path_hash = path_hash.clone();
        request.check()?;
        new_embeddings
    } else {
        let cached_embeddings = {
            let ai_state_lock = state.ai_state.lock().unwrap();
            let ai_state = ai_state_lock.as_ref().unwrap();
            ai_state
                .embeddings
                .as_ref()
                .filter(|cached| cached.path_hash == path_hash)
                .cloned()
        };
        request.check()?;
        if let Some(cached_embeddings) = cached_embeddings {
            cached_embeddings
        } else {
            let embeddings_result =
                generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
                    .map_err(|e| e.to_string());
            request.check()?;
            let mut new_embeddings = embeddings_result?;
            new_embeddings.path_hash = path_hash.clone();
            request.check()?;
            let cache_copy = new_embeddings.clone();
            let displaced = {
                let mut ai_state_lock = state.ai_state.lock().unwrap();
                let ai_state = ai_state_lock.as_mut().unwrap();
                std::mem::replace(&mut ai_state.embeddings, Some(cache_copy))
            };
            drop(displaced);
            request.check()?;
            new_embeddings
        }
    };

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let (img_w, img_h) = embeddings.original_size;

    let (coarse_rotated_w, coarse_rotated_h) = if orientation_steps % 2 == 1 {
        (img_h as f64, img_w as f64)
    } else {
        (img_w as f64, img_h as f64)
    };

    let center = (coarse_rotated_w / 2.0, coarse_rotated_h / 2.0);

    let p1 = start_point;
    let p2 = (start_point.0, end_point.1);
    let p3 = end_point;
    let p4 = (end_point.0, start_point.1);

    let angle_rad = (rotation as f64).to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let unrotate = |p: (f64, f64)| {
        let px = p.0 - center.0;
        let py = p.1 - center.1;
        let new_px = px * cos_a + py * sin_a + center.0;
        let new_py = -px * sin_a + py * cos_a + center.1;
        (new_px, new_py)
    };

    let up1 = unrotate(p1);
    let up2 = unrotate(p2);
    let up3 = unrotate(p3);
    let up4 = unrotate(p4);

    let unflip = |p: (f64, f64)| {
        let mut new_px = p.0;
        let mut new_py = p.1;
        if flip_horizontal {
            new_px = coarse_rotated_w - p.0;
        }
        if flip_vertical {
            new_py = coarse_rotated_h - p.1;
        }
        (new_px, new_py)
    };

    let ufp1 = unflip(up1);
    let ufp2 = unflip(up2);
    let ufp3 = unflip(up3);
    let ufp4 = unflip(up4);

    let un_coarse_rotate = |p: (f64, f64)| -> (f64, f64) {
        match orientation_steps {
            0 => p,
            1 => (p.1, img_h as f64 - p.0),
            2 => (img_w as f64 - p.0, img_h as f64 - p.1),
            3 => (img_w as f64 - p.1, p.0),
            _ => p,
        }
    };

    let ucrp1 = un_coarse_rotate(ufp1);
    let ucrp2 = un_coarse_rotate(ufp2);
    let ucrp3 = un_coarse_rotate(ufp3);
    let ucrp4 = un_coarse_rotate(ufp4);

    let min_x = ucrp1.0.min(ucrp2.0).min(ucrp3.0).min(ucrp4.0);
    let min_y = ucrp1.1.min(ucrp2.1).min(ucrp3.1).min(ucrp4.1);
    let max_x = ucrp1.0.max(ucrp2.0).max(ucrp3.0).max(ucrp4.0);
    let max_y = ucrp1.1.max(ucrp2.1).max(ucrp3.1).max(ucrp4.1);

    let unrotated_start_point = (min_x, min_y);
    let unrotated_end_point = (max_x, max_y);

    let mask_result = run_sam_decoder(
        &models.sam_decoder,
        &embeddings,
        unrotated_start_point,
        unrotated_end_point,
        if skip_refinement.unwrap_or(false) {
            None
        } else {
            Some(warped_image.as_ref())
        },
    )
    .map_err(|e| e.to_string());
    request.check()?;
    let mask_bitmap = mask_result?;

    if cancel_flag.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err("Task cancelled".to_string());
    }

    let encoded_result = encode_to_base64_png(&mask_bitmap);
    request.check()?;
    let base64_data = encoded_result?;
    request.check()?;

    Ok(AiSubjectMaskParameters {
        start_x: start_point.0,
        start_y: start_point.1,
        end_x: end_point.0,
        end_y: end_point.1,
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[tauri::command]
pub async fn precompute_ai_subject_mask(
    js_adjustments: serde_json::Value,
    path: String,
    expected_open_key: Option<crate::progressive_open::OpenKey>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let request = state.capture_consumer_request(
        expected_open_key.as_ref(),
        Some(path.as_str()),
        Some(js_adjustments.clone()),
        load_settings(app_handle.clone()).unwrap_or_default(),
    )?;
    let models_result = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string());
    request.check()?;
    let models = models_result?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    let cached = if request.is_keyed() {
        false
    } else {
        let ai_state_lock = state.ai_state.lock().unwrap();
        ai_state_lock
            .as_ref()
            .and_then(|ai_state| ai_state.embeddings.as_ref())
            .is_some_and(|cached_embeddings| cached_embeddings.path_hash == path_hash)
    };
    request.check()?;
    if cached {
        return Ok(());
    }

    let warped_result = full_reference_for_ai_consumer(&state, &request, &js_adjustments);
    request.check()?;
    let warped_image = warped_result?;
    let embeddings_result = generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
        .map_err(|e| e.to_string());
    request.check()?;
    let mut new_embeddings = embeddings_result?;

    new_embeddings.path_hash = path_hash.clone();

    request.check()?;
    if !request.is_keyed() {
        let cache_entry = Some(new_embeddings);
        let displaced = {
            let mut ai_state_lock = state.ai_state.lock().unwrap();
            if let Some(ai_state) = ai_state_lock.as_mut() {
                std::mem::replace(&mut ai_state.embeddings, cache_entry)
            } else {
                cache_entry
            }
        };
        drop(displaced);
        request.check()?;
    }

    // Keyed precomputation intentionally drops its local embedding instead of
    // publishing into the path-only global cache.
    request.check()?;
    Ok(())
}

#[tauri::command]
pub async fn check_ai_connector_status(app_handle: tauri::AppHandle) {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let is_connected = if let Some(address) = settings.ai_connector_address {
        ai_connector::check_status(&address).await.unwrap_or(false)
    } else {
        false
    };
    use tauri::Emitter;
    let _ = app_handle.emit(
        "ai-connector-status-update",
        serde_json::json!({ "connected": is_connected }),
    );
}

#[tauri::command]
pub async fn test_ai_connector_connection(address: String) -> Result<(), String> {
    match ai_connector::check_status(&address).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Server reachable but returned bad health status".to_string()),
        Err(e) => Err(e.to_string()),
    }
}
