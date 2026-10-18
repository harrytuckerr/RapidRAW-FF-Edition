use memmap2::{Mmap, MmapOptions};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use anyhow::Result;
use chrono::{DateTime, Utc};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageBuffer, Luma};
use rayon::prelude::*;
use regex::regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::Disks;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::AppState;
use crate::PendingMetadata;
#[cfg(target_os = "android")]
use crate::android_integration::*;
use crate::app_settings::*;
use crate::as_shot::as_shot;
use crate::baseline_exposure::BaselineOptions;
use crate::exif_processing;
use crate::formats::{
    ImageContentIssue, is_raw_file, is_supported_image_file, validate_image_content,
};
use crate::gpu_processing;
use crate::image_loader;
use crate::image_processing::GpuContext;
use crate::image_processing::{
    Crop, ImageFlag, ImageMetadata, apply_coarse_rotation, apply_cpu_default_raw_processing,
    apply_crop, apply_exposure, apply_flip, apply_geometry_warp, apply_rotation,
    auto_results_to_json, default_adjustments, effective_baseline_exposure,
    get_all_adjustments_from_json, perform_auto_analysis, resolve_render_options,
};
use crate::mask_generation::MaskDefinition;
use crate::preset_converter;
use crate::tagging::COLOR_TAG_PREFIX;

fn resolve_thumbnail_cache_dir(app_handle: &AppHandle) -> std::result::Result<PathBuf, String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");
    if !thumb_cache_dir.exists() {
        fs::create_dir_all(&thumb_cache_dir).map_err(|e| e.to_string())?;
    }
    Ok(thumb_cache_dir)
}

fn emit_thumbnail_cache_setup_error(app_handle: &AppHandle, path: &str, reason: &str) {
    let _ = app_handle.emit(
        "thumbnail-generation-error",
        serde_json::json!({ "path": path, "reason": reason }),
    );
}

fn compute_thumbnail_cache_hash(path_str: &str, adjustments_bytes: &[u8]) -> Option<String> {
    let (source_path, _) = parse_virtual_path(path_str);

    let img_mod_time = fs::metadata(&source_path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    let mut hasher = blake3::Hasher::new();
    hasher.update(path_str.as_bytes());
    hasher.update(&img_mod_time.to_le_bytes());
    hasher.update(adjustments_bytes);
    Some(hasher.finalize().to_hex().to_string())
}

struct ImageFileMetadata {
    is_edited: bool,
    tags: Option<Vec<String>>,
    rating: u8,
    flag: Option<ImageFlag>,
    is_raw: bool,
}

fn resolve_image_metadata(
    image_path: &Path,
    sidecar_path: &Path,
    enable_xmp_sync: bool,
    settings: &AppSettings,
) -> ImageFileMetadata {
    let mut metadata = crate::exif_processing::load_sidecar(sidecar_path);

    if enable_xmp_sync && sync_metadata_from_xmp(image_path, &mut metadata) {
        let _ = crate::exif_processing::write_sidecar(sidecar_path, image_path, &metadata);
    }

    let is_raw = crate::formats::is_raw_file(image_path);
    let tm_override = crate::image_processing::resolve_tonemapper_override(settings, is_raw);
    let is_edited =
        crate::image_processing::is_image_edited(&metadata.adjustments, is_raw, tm_override);
    ImageFileMetadata {
        is_edited,
        tags: metadata.tags,
        rating: metadata.rating,
        flag: metadata.flag,
        is_raw,
    }
}

fn emit_image_metadata_loaded(
    app_handle: &AppHandle,
    path: &str,
    rating: u8,
    flag: Option<ImageFlag>,
    is_edited: bool,
    tags: &Option<Vec<String>>,
) {
    let _ = app_handle.emit(
        "image-metadata-loaded",
        serde_json::json!({ "path": path, "rating": rating, "flag": flag, "is_edited": is_edited, "tags": tags }),
    );
}

fn enqueue_metadata(
    app_handle: &AppHandle,
    virtual_path: String,
    image_path: PathBuf,
    sidecar_path: PathBuf,
) {
    let state = app_handle.state::<crate::AppState>();
    let manager = &state.metadata_manager;

    let mut pending = manager.pending.lock().unwrap();
    if !pending.insert(sidecar_path.clone()) {
        return;
    }
    drop(pending);

    manager.queue.lock().unwrap().push_back(PendingMetadata {
        virtual_path,
        image_path,
        sidecar_path,
    });
    manager.cvar.notify_one();
}

// Not compute-heavy — these threads mostly block waiting on iCloud to
// materialize a file, not burning CPU — so a small fixed pool is enough and
// doesn't need a user-facing setting the way thumbnail_worker_threads does.
const METADATA_WORKER_THREADS: usize = 4;

pub fn start_metadata_workers(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<crate::AppState>();
    let manager = state.metadata_manager.clone();

    for _ in 0..METADATA_WORKER_THREADS {
        let app_clone = app_handle.clone();
        let manager_clone = manager.clone();

        std::thread::spawn(move || {
            loop {
                let item = {
                    let mut queue = manager_clone.queue.lock().unwrap();
                    while queue.is_empty() {
                        queue = manager_clone.cvar.wait(queue).unwrap();
                    }
                    queue.pop_front().unwrap()
                };

                let settings = load_settings(app_clone.clone()).unwrap_or_default();
                let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

                let metadata = resolve_image_metadata(
                    &item.image_path,
                    &item.sidecar_path,
                    enable_xmp_sync,
                    &settings,
                );

                emit_image_metadata_loaded(
                    &app_clone,
                    &item.virtual_path,
                    metadata.rating,
                    metadata.flag,
                    metadata.is_edited,
                    &metadata.tags,
                );

                manager_clone
                    .pending
                    .lock()
                    .unwrap()
                    .remove(&item.sidecar_path);
            }
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub adjustments: Value,
    #[serde(rename = "includeMasks", skip_serializing_if = "Option::is_none")]
    pub include_masks: Option<bool>,
    #[serde(
        rename = "includeCropTransform",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_crop_transform: Option<bool>,
    #[serde(rename = "presetType", skip_serializing_if = "Option::is_none")]
    pub preset_type: Option<String>,
    /// Whether the preset was saved with "Include Profile". Absent for presets
    /// saved before this flag existed; their adjustments are left as stored.
    #[serde(rename = "includeProfile", skip_serializing_if = "Option::is_none")]
    pub include_profile: Option<bool>,
}

#[derive(Serialize)]
struct ExportPresetFile<'a> {
    creator: &'a str,
    presets: &'a [PresetItem],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PresetFolder {
    pub id: String,
    pub name: String,
    pub children: Vec<Preset>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PresetItem {
    Preset(Preset),
    Folder(PresetFolder),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PresetFile {
    pub presets: Vec<PresetItem>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportFailure {
    pub file_name: String,
    pub error: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportResult {
    /// Number of imported top-level presets or folders, excluding the existing library.
    pub imported: usize,
    pub presets: Vec<PresetItem>,
    pub failures: Vec<PresetImportFailure>,
}

#[derive(Debug)]
pub enum ReadFileError {
    Io(std::io::Error),
    Locked,
    Empty,
    NotFound,
    Invalid,
}

impl fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadFileError::Io(err) => write!(f, "IO error: {}", err),
            ReadFileError::Locked => write!(f, "File is locked"),
            ReadFileError::Empty => write!(f, "File is empty"),
            ReadFileError::NotFound => write!(f, "File not found"),
            ReadFileError::Invalid => write!(f, "Invalid file"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageFile {
    pub path: String,
    modified: u64,
    is_edited: bool,
    rating: u8,
    flag: Option<ImageFlag>,
    tags: Option<Vec<String>>,
    exif: Option<HashMap<String, String>>,
    is_virtual_copy: bool,
    is_cloud_placeholder: bool,
    is_raw: bool,
    group_id: Option<String>,
}

fn make_group_key(source_path: &Path) -> String {
    let parent = source_path.parent().unwrap_or(Path::new(""));
    let stem = source_path.file_stem().unwrap_or_default();
    format!("{}/{}", parent.to_string_lossy(), stem.to_string_lossy())
}

fn assign_group_ids(files: &mut [ImageFile], settings: &crate::app_settings::AppSettings) {
    let require_matching_exif = settings.require_matching_exif.unwrap_or(false);
    let group_edited_files = settings.group_edited_files.unwrap_or(true);

    #[derive(Clone)]
    struct Candidate {
        index: usize,
        source_path: PathBuf,
        key: String,
    }

    let candidates: Vec<Candidate> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_virtual_copy && (group_edited_files || !file.is_edited))
        .map(|(index, file)| {
            let (source_path, _) = parse_virtual_path(&file.path);
            let key = make_group_key(&source_path);
            Candidate {
                index,
                source_path,
                key,
            }
        })
        .collect();

    let mut stem_groups: HashMap<String, Vec<Candidate>> = HashMap::new();
    for candidate in candidates {
        stem_groups
            .entry(candidate.key.clone())
            .or_default()
            .push(candidate);
    }

    if require_matching_exif {
        let groupable_paths: Vec<PathBuf> = stem_groups
            .values()
            .filter(|candidates| candidates.len() >= 2)
            .flat_map(|candidates| candidates.iter().map(|c| c.source_path.clone()))
            .collect();
        let exif_dates: HashMap<PathBuf, Option<chrono::DateTime<chrono::Utc>>> = groupable_paths
            .par_iter()
            .map(|p| {
                (
                    p.clone(),
                    crate::exif_processing::try_get_exif_creation_date(p),
                )
            })
            .collect();

        stem_groups.retain(|_, candidates| {
            if candidates.len() < 2 {
                return false;
            }
            let first = exif_dates
                .get(&candidates[0].source_path)
                .copied()
                .flatten();
            if first.is_none() {
                return false;
            }
            candidates
                .iter()
                .skip(1)
                .all(|c| exif_dates.get(&c.source_path).copied().flatten() == first)
        });
    } else {
        stem_groups.retain(|_, candidates| candidates.len() >= 2);
    }

    for (key, candidates) in stem_groups {
        for candidate in candidates {
            files[candidate.index].group_id = Some(key.clone());
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettings {
    pub filename_template: String,
    pub organize_by_date: bool,
    pub date_folder_format: String,
    pub delete_after_import: bool,
}

pub fn parse_sidecar_name(file_name: &str) -> Option<(&str, Option<&str>)> {
    let base = file_name.strip_suffix(".rrdata")?;
    if base.len() >= 7 && base.as_bytes()[base.len() - 7] == b'.' {
        let id = &base[base.len() - 6..];
        if id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
            return Some((&base[..base.len() - 7], Some(id)));
        }
    }
    Some((base, None))
}

pub fn parse_virtual_path(virtual_path: &str) -> (PathBuf, PathBuf) {
    let (source_path_str, copy_id) = if let Some((base, id)) = virtual_path.rsplit_once("?vc=") {
        (base.to_string(), Some(id.to_string()))
    } else {
        (virtual_path.to_string(), None)
    };

    let source_path = PathBuf::from(source_path_str);

    let sidecar_filename = if let Some(id) = copy_id {
        format!(
            "{}.{}.rrdata",
            source_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            id
        )
    } else {
        format!(
            "{}.rrdata",
            source_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    };

    let sidecar_path = source_path.with_file_name(sidecar_filename);
    (source_path, sidecar_path)
}

#[tauri::command]
pub async fn read_exif_for_paths(
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let is_hdd = state
        .thumbnail_manager
        .rotational_disk
        .load(Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        let process_path = |virtual_path: &String| {
            let (source_path, _) = parse_virtual_path(virtual_path);
            let source_path_str = source_path.to_string_lossy().to_string();

            let map = if let Some(sidecar_exif) =
                crate::exif_processing::read_rrexif_sidecar(&source_path)
            {
                sidecar_exif
            } else if is_cloud_placeholder(&source_path) {
                HashMap::new()
            } else if let Ok(mmap) = read_file_mapped(&source_path) {
                crate::exif_processing::read_exif_data(&source_path_str, &mmap)
            } else if let Ok(bytes) = fs::read(&source_path) {
                crate::exif_processing::read_exif_data(&source_path_str, &bytes)
            } else {
                HashMap::new()
            };

            if map.is_empty() {
                None
            } else {
                Some((virtual_path.clone(), map))
            }
        };

        let exif_data: HashMap<String, HashMap<String, String>> = if is_hdd {
            paths.iter().filter_map(process_path).collect()
        } else {
            paths.par_iter().filter_map(process_path).collect()
        };

        Ok(exif_data)
    })
    .await
    .unwrap_or_else(|e| Err(format!("Task failed: {}", e)))
}

#[tauri::command]
pub async fn update_exif_fields(
    paths: Vec<String>,
    updates: HashMap<String, String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.par_iter().for_each(|path| {
            let original_path = Path::new(&path);
            let primary_path = crate::exif_processing::get_primary_sidecar_path(original_path);
            let temp_metadata = crate::exif_processing::load_sidecar(&primary_path);

            let mut exif_data = temp_metadata.exif.unwrap_or_else(|| {
                if let Some(existing) = crate::exif_processing::read_rrexif_sidecar(original_path) {
                    existing
                } else if let Ok(mmap) = read_file_mapped(original_path) {
                    crate::exif_processing::read_exif_data_from_bytes(path, &mmap)
                } else if let Ok(bytes) = fs::read(original_path) {
                    crate::exif_processing::read_exif_data_from_bytes(path, &bytes)
                } else {
                    HashMap::new()
                }
            });

            for (k, v) in &updates {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    exif_data.remove(k);
                } else {
                    exif_data.insert(k.clone(), trimmed.to_string());
                }
            }

            let mut final_metadata = crate::exif_processing::load_sidecar(&primary_path);

            final_metadata.exif = Some(exif_data);
            let _ = crate::exif_processing::write_sidecar(
                &primary_path,
                original_path,
                &final_metadata,
            );
        });
        Ok(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn match_disk_kind(disks: &Disks, canonical: &Path) -> Option<bool> {
    let mut best_match: Option<(&Path, bool)> = None;

    for disk in disks.list() {
        let mount_point = disk.mount_point();
        if canonical.starts_with(mount_point) {
            let is_longer_match = best_match
                .map(|(current, _)| mount_point.as_os_str().len() > current.as_os_str().len())
                .unwrap_or(true);
            if is_longer_match {
                best_match = Some((mount_point, disk.kind() == sysinfo::DiskKind::HDD));
            }
        }
    }

    best_match.map(|(_, is_hdd)| is_hdd)
}

fn update_rotational_disk_flag(path: &str, app_handle: &AppHandle) {
    let state = app_handle.state::<crate::AppState>();
    let Ok(canonical) = Path::new(path).canonicalize() else {
        return;
    };

    let cached_match = {
        let cache = state.disks_cache.lock().unwrap();
        cache
            .as_ref()
            .and_then(|disks| match_disk_kind(disks, &canonical))
    };

    match cached_match {
        Some(is_hdd) => {
            state
                .thumbnail_manager
                .rotational_disk
                .store(is_hdd, Ordering::Relaxed);
        }
        None => {
            if !state.disks_cache_refreshing.swap(true, Ordering::Relaxed) {
                let refresh_app_handle = app_handle.clone();
                thread::spawn(move || {
                    let disks = Disks::new_with_refreshed_list();
                    let state = refresh_app_handle.state::<crate::AppState>();
                    *state.disks_cache.lock().unwrap() = Some(disks);
                    state.disks_cache_refreshing.store(false, Ordering::Relaxed);
                });
            }
        }
    }
}

#[tauri::command]
pub fn list_images_in_dir(path: String, app_handle: AppHandle) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    update_rotational_disk_flag(&path, &app_handle);

    let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut images = Vec::new();
    let mut sidecars_by_filename: HashMap<String, Vec<Option<String>>> = HashMap::new();

    for entry in entries.filter_map(Result::ok) {
        let entry_path = entry.path();
        let file_name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|os| os.to_string_lossy().into_owned());

        if let Some((source_filename, copy_id)) = parse_sidecar_name(&file_name) {
            sidecars_by_filename
                .entry(source_filename.to_string())
                .or_default()
                .push(copy_id.map(str::to_string));
        } else if is_supported_image_file(&file_name) {
            images.push((file_name, entry_path));
        }
    }

    let tasks: Vec<_> = images
        .into_iter()
        .map(|(file_name, path_buf)| {
            let sidecars = sidecars_by_filename
                .remove(&file_name)
                .unwrap_or_else(|| vec![None]);
            let path_str = path_buf.to_string_lossy().into_owned();
            (path_str, file_name, path_buf, sidecars)
        })
        .collect();

    let mut result_list: Vec<ImageFile> = tasks
        .into_par_iter()
        .flat_map(|(path_str, file_name, path_buf, sidecars)| {
            let modified = fs::metadata(&path_buf)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_cloud_placeholder = is_cloud_placeholder(&path_buf);

            let mut file_results = Vec::with_capacity(sidecars.len());

            for copy_id_opt in sidecars {
                let (virtual_path, is_virtual_copy, sidecar_filename) = match copy_id_opt {
                    Some(id) => (
                        format!("{}?vc={}", path_str, id),
                        true,
                        format!("{}.{}.rrdata", file_name, id),
                    ),
                    None => (path_str.clone(), false, format!("{}.rrdata", file_name)),
                };

                let sidecar_path = path_buf.with_file_name(sidecar_filename);

                let xmp_is_placeholder = enable_xmp_sync
                    && resolve_xmp_path(&path_buf)
                        .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

                let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                    || xmp_is_placeholder
                {
                    enqueue_metadata(
                        &app_handle,
                        virtual_path.clone(),
                        path_buf.clone(),
                        sidecar_path.clone(),
                    );
                    ImageFileMetadata {
                        is_edited: false,
                        tags: None,
                        rating: 0,
                        flag: None,
                        is_raw: crate::formats::is_raw_file(&path_buf),
                    }
                } else {
                    resolve_image_metadata(&path_buf, &sidecar_path, enable_xmp_sync, &settings)
                };

                file_results.push(ImageFile {
                    path: virtual_path,
                    modified,
                    is_edited: metadata.is_edited,
                    tags: metadata.tags,
                    exif: None,
                    is_virtual_copy,
                    is_raw: metadata.is_raw,
                    group_id: None,
                    rating: metadata.rating,
                    flag: metadata.flag,
                    is_cloud_placeholder,
                });
            }

            file_results
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[tauri::command]
pub fn list_images_recursive(
    path: String,
    app_handle: AppHandle,
) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    update_rotational_disk_flag(&path, &app_handle);

    let root_path = Path::new(&path);
    let mut images = Vec::new();

    let mut sidecars_by_path: HashMap<PathBuf, Vec<Option<String>>> = HashMap::new();

    for entry in WalkDir::new(root_path).into_iter().filter_map(Result::ok) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }

        let file_name = entry_path.file_name().unwrap_or_default().to_string_lossy();
        if let Some((source_filename, copy_id)) = parse_sidecar_name(&file_name) {
            if let Some(parent) = entry_path.parent() {
                sidecars_by_path
                    .entry(parent.join(source_filename))
                    .or_default()
                    .push(copy_id.map(str::to_string));
            }
        } else if is_supported_image_file(entry_path.to_string_lossy().as_ref()) {
            images.push(entry_path.to_path_buf());
        }
    }

    let tasks: Vec<_> = images
        .into_iter()
        .map(|path_buf| {
            let sidecars = sidecars_by_path
                .remove(&path_buf)
                .unwrap_or_else(|| vec![None]);
            let path_str = path_buf.to_string_lossy().into_owned();
            let file_name = path_buf
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (path_str, file_name, path_buf, sidecars)
        })
        .collect();

    let mut result_list: Vec<ImageFile> = tasks
        .into_par_iter()
        .flat_map(|(path_str, file_name, path_buf, sidecars)| {
            let modified = fs::metadata(&path_buf)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_cloud_placeholder = is_cloud_placeholder(&path_buf);

            let mut file_results = Vec::with_capacity(sidecars.len());

            for copy_id_opt in sidecars {
                let (virtual_path, is_virtual_copy, sidecar_filename) = match copy_id_opt {
                    Some(id) => (
                        format!("{}?vc={}", path_str, id),
                        true,
                        format!("{}.{}.rrdata", file_name, id),
                    ),
                    None => (path_str.clone(), false, format!("{}.rrdata", file_name)),
                };

                let sidecar_path = path_buf.with_file_name(sidecar_filename);

                let xmp_is_placeholder = enable_xmp_sync
                    && resolve_xmp_path(&path_buf)
                        .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

                let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                    || xmp_is_placeholder
                {
                    enqueue_metadata(
                        &app_handle,
                        virtual_path.clone(),
                        path_buf.clone(),
                        sidecar_path.clone(),
                    );
                    ImageFileMetadata {
                        is_edited: false,
                        tags: None,
                        rating: 0,
                        flag: None,
                        is_raw: crate::formats::is_raw_file(&path_buf),
                    }
                } else {
                    resolve_image_metadata(&path_buf, &sidecar_path, enable_xmp_sync, &settings)
                };

                file_results.push(ImageFile {
                    path: virtual_path,
                    modified,
                    is_edited: metadata.is_edited,
                    tags: metadata.tags,
                    exif: None,
                    is_virtual_copy,
                    is_raw: metadata.is_raw,
                    group_id: None,
                    rating: metadata.rating,
                    flag: metadata.flag,
                    is_cloud_placeholder,
                });
            }

            file_results
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AlbumItem {
    Album {
        id: String,
        name: String,
        icon: Option<String>,
        images: Vec<String>,
    },
    Group {
        id: String,
        name: String,
        icon: Option<String>,
        children: Vec<AlbumItem>,
    },
}

fn get_albums_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let albums_dir = data_dir.join("albums");
    if !albums_dir.exists() {
        fs::create_dir_all(&albums_dir).map_err(|e| e.to_string())?;
    }
    Ok(albums_dir.join("albums.json"))
}

pub fn sort_album_tree(items: &mut [AlbumItem]) {
    items.sort_by(|a, b| {
        let get_sort_key = |item: &AlbumItem| match item {
            AlbumItem::Group { name, .. } => (0, name.to_lowercase()),
            AlbumItem::Album { name, .. } => (1, name.to_lowercase()),
        };

        let key_a = get_sort_key(a);
        let key_b = get_sort_key(b);

        key_a.cmp(&key_b)
    });

    for item in items.iter_mut() {
        if let AlbumItem::Group { children, .. } = item {
            sort_album_tree(children);
        }
    }
}

#[tauri::command]
pub fn get_albums(app_handle: AppHandle) -> Result<Vec<AlbumItem>, String> {
    let path = get_albums_path(&app_handle)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut items: Vec<AlbumItem> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    sort_album_tree(&mut items);
    Ok(items)
}

#[tauri::command]
pub fn save_albums(mut tree: Vec<AlbumItem>, app_handle: AppHandle) -> Result<(), String> {
    let path = get_albums_path(&app_handle)?;
    sort_album_tree(&mut tree);
    let json_string = serde_json::to_string_pretty(&tree).map_err(|e| e.to_string())?;
    fs::write(path, json_string).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_to_album(
    album_id: String,
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut tree = get_albums(app_handle.clone())?;

    fn add_recursive(items: &mut [AlbumItem], target_id: &str, paths_to_add: &Vec<String>) -> bool {
        for item in items.iter_mut() {
            #[allow(clippy::collapsible_match)]
            match item {
                AlbumItem::Album { id, images, .. } if id == target_id => {
                    for p in paths_to_add {
                        if !images.contains(p) {
                            images.push(p.clone());
                        }
                    }
                    return true;
                }
                AlbumItem::Group { children, .. } => {
                    if add_recursive(children, target_id, paths_to_add) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    if add_recursive(&mut tree, &album_id, &paths) {
        save_albums(tree, app_handle)?;
    }
    Ok(())
}

fn sync_album_path_changes(
    app_handle: &AppHandle,
    renames: Option<&HashMap<String, String>>,
    deletions: Option<&HashSet<String>>,
    folder_rename: Option<(&str, &str)>,
) {
    if let Ok(mut tree) = get_albums(app_handle.clone()) {
        let mut changed = false;

        fn process_nodes(
            nodes: &mut [AlbumItem],
            renames: Option<&HashMap<String, String>>,
            deletions: Option<&HashSet<String>>,
            folder_rename: Option<(&str, &str)>,
            changed: &mut bool,
        ) {
            for node in nodes.iter_mut() {
                match node {
                    AlbumItem::Album { images, .. } => {
                        let mut new_images = Vec::new();

                        for img in images.drain(..) {
                            let mut current_img = img;

                            if let Some((old_folder, new_folder)) = folder_rename {
                                let img_path = Path::new(&current_img);
                                let old_path = Path::new(old_folder);
                                if let Ok(stripped) = img_path.strip_prefix(old_path) {
                                    let new_img_path = Path::new(new_folder).join(stripped);
                                    current_img = new_img_path.to_string_lossy().into_owned();
                                    *changed = true;
                                }
                            }

                            if let Some(r) = renames {
                                if let Some(new_path) = r.get(&current_img) {
                                    current_img = new_path.clone();
                                    *changed = true;
                                } else if let Some((base_path, vc_id)) =
                                    current_img.rsplit_once("?vc=")
                                    && let Some(new_base) = r.get(base_path)
                                {
                                    current_img = format!("{}?vc={}", new_base, vc_id);
                                    *changed = true;
                                }
                            }

                            let mut is_deleted = false;
                            if let Some(d) = deletions {
                                if d.contains(&current_img) {
                                    is_deleted = true;
                                } else {
                                    let img_path = Path::new(&current_img);
                                    for del_path_str in d {
                                        let del_path = Path::new(del_path_str);
                                        if img_path.starts_with(del_path) {
                                            is_deleted = true;
                                            break;
                                        }

                                        if let Some((base_path, _)) =
                                            current_img.rsplit_once("?vc=")
                                            && base_path == del_path_str
                                        {
                                            is_deleted = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !is_deleted {
                                new_images.push(current_img);
                            } else {
                                *changed = true;
                            }
                        }
                        *images = new_images;
                    }
                    AlbumItem::Group { children, .. } => {
                        process_nodes(children, renames, deletions, folder_rename, changed);
                    }
                }
            }
        }

        process_nodes(&mut tree, renames, deletions, folder_rename, &mut changed);

        if changed {
            let _ = save_albums(tree, app_handle.clone());
        }
    }
}

#[tauri::command]
pub fn get_album_images(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    let mut result_list: Vec<ImageFile> = paths
        .into_par_iter()
        .filter_map(|virtual_path| {
            let (source_path, sidecar_path) = parse_virtual_path(&virtual_path);
            if !source_path.exists() {
                return None;
            }

            let modified = fs::metadata(&source_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_virtual_copy = virtual_path.contains("?vc=");
            let is_cloud_placeholder = is_cloud_placeholder(&source_path);

            let xmp_is_placeholder = enable_xmp_sync
                && resolve_xmp_path(&source_path)
                    .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

            let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                || xmp_is_placeholder
            {
                enqueue_metadata(
                    &app_handle,
                    virtual_path.clone(),
                    source_path.clone(),
                    sidecar_path.clone(),
                );
                ImageFileMetadata {
                    is_edited: false,
                    tags: None,
                    rating: 0,
                    flag: None,
                    is_raw: crate::formats::is_raw_file(&source_path),
                }
            } else {
                resolve_image_metadata(&source_path, &sidecar_path, enable_xmp_sync, &settings)
            };

            Some(ImageFile {
                path: virtual_path.clone(),
                modified,
                is_edited: metadata.is_edited,
                tags: metadata.tags,
                exif: None,
                is_virtual_copy,
                is_raw: metadata.is_raw,
                group_id: None,
                rating: metadata.rating,
                flag: metadata.flag,
                is_cloud_placeholder,
            })
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FolderNode {
    pub name: String,
    pub path: String,
    pub children: Vec<FolderNode>,
    pub is_dir: bool,
    pub image_count: usize,
    pub has_subdirs: bool,
    pub modified: u64,
    pub created: u64,
}

fn has_subdirs(path: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with('.') {
                    return true;
                }
            }
        }
    }
    false
}

fn scan_dir_lazy(
    path: &Path,
    expanded_folders: &HashSet<&str>,
    show_image_counts: bool,
    prefetch_one_level: bool,
) -> Result<(Vec<FolderNode>, usize), std::io::Error> {
    let mut children_folders = Vec::new();
    let mut current_dir_image_count = 0;

    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Could not scan directory '{}': {}", path.display(), e);
            return Ok((Vec::new(), 0));
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let current_path = entry.path();
        let (file_type, modified, created) = match entry.metadata() {
            Ok(meta) => {
                let ft = meta.file_type();
                let mod_time = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let cre_time = meta.created().unwrap_or(mod_time);

                (
                    ft,
                    mod_time
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    cre_time
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                )
            }
            Err(_) => continue,
        };

        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if name_str.starts_with('.') {
            continue;
        }

        if file_type.is_dir() {
            let path_str = current_path.to_string_lossy().into_owned();
            let is_expanded = expanded_folders.contains(path_str.as_str());

            let should_scan = is_expanded || prefetch_one_level;
            let next_prefetch = is_expanded;

            let (grand_children, sub_dir_own_images) = if should_scan {
                scan_dir_lazy(
                    &current_path,
                    expanded_folders,
                    show_image_counts,
                    next_prefetch,
                )?
            } else {
                let count = if show_image_counts {
                    WalkDir::new(&current_path)
                        .into_iter()
                        .filter_map(Result::ok)
                        .filter(|e| {
                            e.file_type().is_file()
                                && crate::formats::is_supported_image_file(e.path())
                        })
                        .count()
                } else {
                    0
                };
                (Vec::new(), count)
            };

            let has_any_subdirs = if should_scan {
                grand_children.iter().any(|c| c.is_dir)
            } else {
                has_subdirs(&current_path)
            };

            let grand_children_sum: usize = grand_children.iter().map(|c| c.image_count).sum();
            let total_child_count = sub_dir_own_images + grand_children_sum;

            children_folders.push(FolderNode {
                name: name_str.into_owned(),
                path: path_str,
                children: grand_children,
                is_dir: true,
                image_count: total_child_count,
                has_subdirs: has_any_subdirs,
                modified,
                created,
            });
        } else if show_image_counts
            && file_type.is_file()
            && crate::formats::is_supported_image_file(&current_path)
        {
            current_dir_image_count += 1;
        }
    }

    children_folders.sort_by_key(|a| a.name.to_lowercase());

    Ok((children_folders, current_dir_image_count))
}

fn get_folder_tree_sync(
    path: String,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<FolderNode, String> {
    let root_path = Path::new(&path);
    if !root_path.is_dir() {
        return Err(format!("Directory does not exist: {}", path));
    }

    let (modified, created) = root_path
        .metadata()
        .map(|m| {
            let mod_time = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let cre_time = m.created().unwrap_or(mod_time);
            (
                mod_time
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                cre_time
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            )
        })
        .unwrap_or((0, 0));

    let expanded_set: HashSet<&str> = expanded_folders.iter().map(|s| s.as_str()).collect();

    let (children, own_count) = scan_dir_lazy(root_path, &expanded_set, show_image_counts, true)
        .map_err(|e| e.to_string())?;

    let children_sum: usize = children.iter().map(|c| c.image_count).sum();
    let has_subdirs = children.iter().any(|c| c.is_dir);

    let name = match root_path.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => {
            let trimmed = path.trim_end_matches(&['/', '\\'][..]);
            if trimmed.is_empty() {
                path.clone()
            } else {
                trimmed.to_string()
            }
        }
    };

    Ok(FolderNode {
        name,
        path: path.clone(),
        children,
        is_dir: true,
        image_count: own_count + children_sum,
        has_subdirs,
        modified,
        created,
    })
}

#[tauri::command]
pub async fn get_folder_children(
    path: String,
    show_image_counts: bool,
) -> Result<Vec<FolderNode>, String> {
    match tauri::async_runtime::spawn_blocking(move || {
        let root_path = Path::new(&path);
        if !root_path.is_dir() {
            return Err(format!("Directory does not exist: {}", path));
        }
        let empty_set = HashSet::new();
        let (children, _) = scan_dir_lazy(root_path, &empty_set, show_image_counts, false)
            .map_err(|e| e.to_string())?;

        Ok(children)
    })
    .await
    {
        Ok(Ok(children)) => Ok(children),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

#[tauri::command]
pub async fn get_folder_tree(
    path: String,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<FolderNode, String> {
    match tauri::async_runtime::spawn_blocking(move || {
        get_folder_tree_sync(path, expanded_folders, show_image_counts)
    })
    .await
    {
        Ok(Ok(folder_node)) => Ok(folder_node),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Failed to execute folder tree task: {}", e)),
    }
}

#[tauri::command]
pub async fn get_pinned_folder_trees(
    paths: Vec<String>,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<Vec<FolderNode>, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let results: Vec<Result<FolderNode, String>> = paths
            .par_iter()
            .map(|path| {
                get_folder_tree_sync(path.clone(), expanded_folders.clone(), show_image_counts)
            })
            .collect();

        let mut folder_nodes = Vec::new();
        for result in results {
            match result {
                Ok(node) => folder_nodes.push(node),
                Err(e) => log::warn!("Failed to get tree for pinned folder: {}", e),
            }
        }
        folder_nodes
    })
    .await;

    match result {
        Ok(nodes) => Ok(nodes),
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

#[cfg(target_os = "macos")]
pub fn is_cloud_placeholder(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    const SF_DATALESS: u32 = 0x4000_0000;

    let c_path = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::lstat(c_path.as_ptr(), &mut stat_buf) };
    ret == 0 && (stat_buf.st_flags & SF_DATALESS) != 0
}

#[cfg(not(target_os = "macos"))]
pub fn is_cloud_placeholder(_path: &Path) -> bool {
    false
}

pub fn read_file_mapped(path: &Path) -> Result<Mmap, ReadFileError> {
    if !path.is_file() {
        return Err(ReadFileError::Invalid);
    }
    if !path.exists() {
        return Err(ReadFileError::NotFound);
    }
    if path.metadata().map_err(ReadFileError::Io)?.len() == 0 {
        return Err(ReadFileError::Empty);
    }
    let file = fs::File::open(path).map_err(ReadFileError::Io)?;
    if file.try_lock_shared().is_err() {
        return Err(ReadFileError::Locked);
    }
    let mmap = unsafe {
        MmapOptions::new()
            .len(file.metadata().map_err(ReadFileError::Io)?.len() as usize)
            .map(&file)
            .map_err(ReadFileError::Io)?
    };
    Ok(mmap)
}

fn find_embedded_jpeg(exif: &exif::Exif, ifd: exif::In) -> Option<&[u8]> {
    let offset = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, ifd)?
        .value
        .get_uint(0)? as usize;
    let len = exif
        .get_field(exif::Tag::JPEGInterchangeFormatLength, ifd)?
        .value
        .get_uint(0)? as usize;
    exif.buf().get(offset..offset + len)
}

fn apply_exif_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn exif_embedded_preview(exif: &exif::Exif) -> Option<DynamicImage> {
    let (jpeg_bytes, ifd) = find_embedded_jpeg(exif, exif::In::PRIMARY)
        .map(|b| (b, exif::In::PRIMARY))
        .or_else(|| {
            find_embedded_jpeg(exif, exif::In::THUMBNAIL).map(|b| (b, exif::In::THUMBNAIL))
        })?;

    let img = image::load_from_memory_with_format(jpeg_bytes, image::ImageFormat::Jpeg).ok()?;

    let orientation = exif
        .get_field(exif::Tag::Orientation, ifd)
        .and_then(|f| f.value.get_uint(0))
        .unwrap_or(1);

    Some(apply_exif_orientation(img, orientation))
}

fn try_load_embedded_raw_preview(source_path: &Path, target_res: u32) -> Option<DynamicImage> {
    let mmap = read_file_mapped(source_path).ok()?;

    let preview = match exif_processing::read_exif(&mmap) {
        Some(exif) => exif_embedded_preview(&exif)?,
        None => {
            image_loader::safe_embedded_preview_fallback(&mmap, &source_path.to_string_lossy())?
        }
    };

    (preview.width().max(preview.height()) >= (target_res as f32 * 0.95) as u32).then_some(preview)
}

fn thumbnail_metadata(path_str: &str, app_handle: &AppHandle) -> Option<ImageMetadata> {
    let (source_path, sidecar_path) = parse_virtual_path(path_str);
    if is_cloud_placeholder(&sidecar_path) {
        enqueue_metadata(
            app_handle,
            path_str.to_string(),
            source_path.clone(),
            sidecar_path.clone(),
        );
        None
    } else {
        fs::read_to_string(&sidecar_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    }
}

pub fn generate_thumbnail_data(
    path_str: &str,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    app_handle: &AppHandle,
) -> anyhow::Result<DynamicImage> {
    let metadata = thumbnail_metadata(path_str, app_handle);
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    generate_thumbnail_from_metadata(
        path_str,
        gpu_context,
        preloaded_image,
        app_handle,
        metadata,
        &settings,
    )
}

fn generate_thumbnail_from_metadata(
    path_str: &str,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    app_handle: &AppHandle,
    metadata: Option<ImageMetadata>,
    settings: &AppSettings,
) -> anyhow::Result<DynamicImage> {
    let (source_path, _) = parse_virtual_path(path_str);
    let source_path_str = source_path.to_string_lossy().to_string();
    let is_raw = is_raw_file(&source_path_str);

    let adjustments = metadata
        .as_ref()
        .map_or(serde_json::Value::Null, |m| m.adjustments.clone());

    // A preloaded upstream RGB image cannot substitute for camera-native profile development.
    let profile_registry = app_handle.state::<AppState>().profile_registry.clone();
    let preloaded_image =
        if crate::dcp::pipeline::has_selected_profile(&adjustments).map_err(anyhow::Error::msg)? {
            None
        } else {
            preloaded_image
        };
    let always_decode_raw = settings.always_decode_raw_thumbnails.unwrap_or(false);

    if is_raw && adjustments.is_null() && preloaded_image.is_none() && !always_decode_raw {
        let target_res = settings.medium_thumbnail_resolution.unwrap_or(1280);
        if let Some(preview) = try_load_embedded_raw_preview(&source_path, target_res) {
            return Ok(preview);
        }
    }

    if let (Some(context), Some(meta)) = (gpu_context, metadata)
        && !meta.adjustments.is_null()
    {
        let state = app_handle.state::<AppState>();
        let target_res = settings.medium_thumbnail_resolution.unwrap_or(1280);

        let base_cache_hash = crate::cache_utils::calculate_thumbnail_base_hash(&meta.adjustments);

        let crop_data: Option<Crop> = serde_json::from_value(meta.adjustments["crop"].clone()).ok();

        let cached_base: Option<(Arc<DynamicImage>, f32)> = {
            let cache = state.thumbnail_geometry_cache.lock().unwrap();
            if let Some((cached_hash, img, scale)) = cache.get(path_str) {
                let mut sufficient_resolution = true;
                if let Some(c) = &crop_data
                    && c.width > 0.0
                    && c.height > 0.0
                {
                    let final_crop_max_dim =
                        (c.width as f32 * *scale).max(c.height as f32 * *scale);
                    if final_crop_max_dim < (target_res as f32 * 0.95) {
                        sufficient_resolution = false;
                    }
                }

                if *cached_hash == base_cache_hash && sufficient_resolution {
                    Some((Arc::clone(img), *scale))
                } else {
                    None
                }
            } else {
                None
            }
        };

        let (processing_base_arc, total_scale) = if let Some((arc_img, scale)) = cached_base {
            (arc_img, scale)
        } else {
            let mut raw_scale_factor = 1.0f32;

            let composite_image = if let Some(img) = preloaded_image {
                image_loader::prepare_base_image_in_worker(
                    app_handle,
                    img,
                    &source_path_str,
                    &adjustments,
                    &|| Ok(()),
                )?
            } else {
                let mmap_guard;
                let vec_guard;

                let file_slice: &[u8] = match read_file_mapped(&source_path) {
                    Ok(mmap) => {
                        mmap_guard = Some(mmap);
                        mmap_guard.as_ref().unwrap()
                    }
                    Err(e) => {
                        if preloaded_image.is_none() {
                            log::warn!("Fallback read for {}: {}", source_path_str, e);
                        }
                        let bytes = fs::read(&source_path).map_err(|io_err| {
                            anyhow::anyhow!(
                                "Fallback read failed for {}: {}",
                                source_path_str,
                                io_err
                            )
                        })?;
                        vec_guard = Some(bytes);
                        vec_guard.as_ref().unwrap()
                    }
                };

                let img = image_loader::load_and_composite_with_profiles_in_worker(
                    app_handle,
                    &|| Ok(()),
                    file_slice,
                    &source_path_str,
                    &adjustments,
                    true,
                    &settings,
                    &profile_registry,
                    None,
                )?;

                if is_raw {
                    raw_scale_factor = crate::raw_processing::get_fast_demosaic_scale_factor(
                        file_slice,
                        img.width(),
                        img.height(),
                    );
                }
                img
            };

            let warped_image =
                apply_geometry_warp(Cow::Borrowed(&composite_image), &meta.adjustments);

            let relit_image = crate::relight::apply_relight(warped_image, &meta.adjustments);
            let fogged_image = crate::fog::apply_fog(relit_image, &meta.adjustments);
            let blurred_image = crate::lens_blur::apply_lens_blur(fogged_image, &meta.adjustments);

            let orientation_steps =
                meta.adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
            let coarse_rotated_image = apply_coarse_rotation(blurred_image, orientation_steps);

            let (full_w, full_h) = coarse_rotated_image.dimensions();

            let mut processing_dim = target_res;
            if let Some(c) = &crop_data
                && c.width > 0.0
                && c.height > 0.0
            {
                let crop_max_dim_loaded = c.width.max(c.height) * raw_scale_factor as f64;
                let full_max_dim = full_w.max(full_h) as f64;
                if crop_max_dim_loaded > 0.0 {
                    processing_dim = ((target_res as f64 * full_max_dim / crop_max_dim_loaded)
                        .round() as u32)
                        .min(full_w.max(full_h));
                }
            }

            let (base, gpu_scale) = if full_w > processing_dim || full_h > processing_dim {
                let base = crate::image_processing::downscale_f32_image(
                    &coarse_rotated_image,
                    processing_dim,
                    processing_dim,
                );
                let scale = if full_w > 0 {
                    base.width() as f32 / full_w as f32
                } else {
                    1.0
                };
                (base, scale)
            } else {
                (coarse_rotated_image.into_owned(), 1.0)
            };

            let total_scale = gpu_scale * raw_scale_factor;

            let mut cache = state.thumbnail_geometry_cache.lock().unwrap();
            if cache.len() >= 8 {
                let key_to_remove = cache.keys().next().cloned();
                if let Some(key) = key_to_remove {
                    cache.remove(&key);
                }
            }

            let base_arc = Arc::new(base);
            cache.insert(
                path_str.to_string(),
                (base_cache_hash, Arc::clone(&base_arc), total_scale),
            );

            (base_arc, total_scale)
        };

        let rotation_degrees = meta.adjustments["rotation"].as_f64().unwrap_or(0.0) as f32;
        let flip_horizontal = meta.adjustments["flipHorizontal"]
            .as_bool()
            .unwrap_or(false);
        let flip_vertical = meta.adjustments["flipVertical"].as_bool().unwrap_or(false);

        let flipped_image = apply_flip(
            Cow::Borrowed(&*processing_base_arc),
            flip_horizontal,
            flip_vertical,
        );
        let rotated_image = apply_rotation(flipped_image, rotation_degrees);

        let scaled_crop_json = if let Some(c) = &crop_data {
            serde_json::to_value(Crop {
                x: c.x * total_scale as f64,
                y: c.y * total_scale as f64,
                width: c.width * total_scale as f64,
                height: c.height * total_scale as f64,
            })
            .unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        let cropped_preview = apply_crop(rotated_image, &scaled_crop_json);
        let (preview_w, preview_h) = cropped_preview.dimensions();
        let unscaled_crop_offset = crop_data.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));

        let mask_definitions: Vec<MaskDefinition> = meta
            .adjustments
            .get("masks")
            .and_then(|m| serde_json::from_value(m.clone()).ok())
            .unwrap_or_else(Vec::new);

        let mask_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = mask_definitions
            .iter()
            .filter_map(|def| {
                crate::get_cached_or_generate_mask(
                    &state,
                    path_str,
                    def,
                    preview_w,
                    preview_h,
                    total_scale,
                    (
                        unscaled_crop_offset.0 * total_scale,
                        unscaled_crop_offset.1 * total_scale,
                    ),
                    &meta.adjustments,
                )
            })
            .collect();

        let gpu_adjustments = get_all_adjustments_from_json(
            &meta.adjustments,
            is_raw,
            as_shot(&source_path_str),
            resolve_render_options(&settings, is_raw),
        );
        let lut_path = meta.adjustments["lutPath"].as_str();
        let lut = lut_path.and_then(|p| {
            let mut cache = state.lut_cache.lock().unwrap();
            if let Some(cached_lut) = cache.get(p) {
                return Some(cached_lut.clone());
            }
            if let Ok(loaded_lut) = crate::lut_processing::parse_lut_file(p) {
                let arc_lut = Arc::new(loaded_lut);
                cache.insert(p.to_string(), arc_lut.clone());
                return Some(arc_lut);
            }
            None
        });

        let mut hasher = DefaultHasher::new();
        path_str.hash(&mut hasher);
        meta.adjustments.to_string().hash(&mut hasher);
        let unique_hash = hasher.finish();

        if let Ok(processed_image) = gpu_processing::process_and_get_dynamic_image(
            context,
            &state,
            cropped_preview.as_ref(),
            unique_hash,
            gpu_processing::RenderRequest {
                adjustments: gpu_adjustments,
                mask_bitmaps: &mask_bitmaps,
                lut,
                roi: None,
                dcp_textures: None,
            },
            "generate_thumbnail_data",
        ) {
            return Ok(processed_image);
        } else {
            return Ok(cropped_preview.into_owned());
        }
    }

    let mut final_image = if let Some(img) = preloaded_image {
        image_loader::prepare_base_image_in_worker(
            app_handle,
            img,
            &source_path_str,
            &adjustments,
            &|| Ok(()),
        )?
    } else {
        match read_file_mapped(&source_path) {
            Ok(mmap) => image_loader::load_and_composite_with_profiles_in_worker(
                app_handle,
                &|| Ok(()),
                &mmap,
                &source_path_str,
                &adjustments,
                true,
                &settings,
                &profile_registry,
                None,
            )?,
            Err(e) => {
                log::warn!("Fallback read for {}: {}", source_path_str, e);
                let bytes = fs::read(&source_path)?;
                image_loader::load_and_composite_with_profiles_in_worker(
                    app_handle,
                    &|| Ok(()),
                    &bytes,
                    &source_path_str,
                    &adjustments,
                    true,
                    &settings,
                    &profile_registry,
                    None,
                )?
            }
        }
    };

    if adjustments.is_null() {
        let render = resolve_render_options(&settings, is_raw);
        final_image = apply_exposure(
            final_image,
            effective_baseline_exposure(&adjustments, as_shot(&source_path_str), render.baseline),
        );
        let use_agx = render.tonemapper_override == Some(1);

        if use_agx {
            if !is_raw {
                final_image = crate::image_processing::apply_srgb_to_linear(final_image);
            }
            crate::image_processing::apply_cpu_agx_tonemap(&mut final_image);
        } else if is_raw {
            apply_cpu_default_raw_processing(&mut final_image);
        }
    }

    let fallback_orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    Ok(apply_coarse_rotation(Cow::Owned(final_image), fallback_orientation_steps).into_owned())
}

fn encode_thumbnail(image: &DynamicImage, target_width: u32) -> Result<Vec<u8>> {
    let thumbnail = crate::image_processing::downscale_f32_image(image, target_width, target_width);
    let mut buf = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, 75);
    encoder.encode_image(&thumbnail.to_rgb8())?;
    Ok(buf.into_inner())
}

fn thumbnail_metadata_key(
    metadata: &Option<ImageMetadata>,
    path: &str,
    settings: &AppSettings,
) -> (u8, bool, Vec<u8>) {
    metadata.as_ref().map_or((0, false, Vec::new()), |meta| {
        let raw = crate::formats::is_raw_file(path);
        let tm = crate::image_processing::resolve_tonemapper_override(settings, raw);
        (
            meta.rating,
            crate::image_processing::is_image_edited(&meta.adjustments, raw, tm),
            serde_json::to_vec(&meta.adjustments).unwrap_or_default(),
        )
    })
}

fn generate_single_thumbnail_and_cache(
    path_str: &str,
    thumb_cache_dir: &Path,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    force_regenerate: bool,
    app_handle: &AppHandle,
    settings: &AppSettings,
) -> Option<(String, String, u8, bool)> {
    let metadata = thumbnail_metadata(path_str, app_handle);
    generate_single_thumbnail_from_metadata(
        path_str,
        thumb_cache_dir,
        gpu_context,
        preloaded_image,
        force_regenerate,
        app_handle,
        settings,
        metadata,
    )
}

fn generate_single_thumbnail_from_metadata(
    path_str: &str,
    thumb_cache_dir: &Path,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    force_regenerate: bool,
    app_handle: &AppHandle,
    settings: &AppSettings,
    metadata: Option<ImageMetadata>,
) -> Option<(String, String, u8, bool)> {
    let (source_path, _) = parse_virtual_path(path_str);
    let (rating, is_edited, adjustments_bytes) =
        thumbnail_metadata_key(&metadata, path_str, settings);
    let cache_hash = compute_thumbnail_cache_hash(path_str, &adjustments_bytes)?;

    let small_path = thumb_cache_dir.join(format!("{}_small.jpg", cache_hash));
    let medium_path = thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash));

    if !force_regenerate && small_path.exists() && medium_path.exists() {
        return Some((
            small_path.to_string_lossy().into_owned(),
            medium_path.to_string_lossy().into_owned(),
            rating,
            is_edited,
        ));
    }

    if is_cloud_placeholder(&source_path) {
        return None;
    }

    let target_width_small = settings.small_thumbnail_resolution.unwrap_or(480);
    let target_width_medium = settings.medium_thumbnail_resolution.unwrap_or(1280);

    if let Ok(thumb_image) = generate_thumbnail_from_metadata(
        path_str,
        gpu_context,
        preloaded_image,
        app_handle,
        metadata,
        settings,
    ) && let (Ok(small_data), Ok(medium_data)) = (
        encode_thumbnail(&thumb_image, target_width_small),
        encode_thumbnail(&thumb_image, target_width_medium),
    ) {
        // The rendered pixels and key share the same captured metadata. A
        // superseded job may leave a correctly keyed old cache entry, never
        // new-adjustment pixels under an older adjustment key.
        if compute_thumbnail_cache_hash(path_str, &adjustments_bytes).as_deref()
            != Some(cache_hash.as_str())
        {
            return None;
        }
        fs::write(&small_path, &small_data).ok()?;
        fs::write(&medium_path, &medium_data).ok()?;
        return Some((
            small_path.to_string_lossy().into_owned(),
            medium_path.to_string_lossy().into_owned(),
            rating,
            is_edited,
        ));
    }
    None
}

fn prefetch_source_file(path_str: &str) {
    let (source_path, _) = parse_virtual_path(path_str);
    if let Ok(mut file) = std::fs::File::open(&source_path) {
        let _ = std::io::copy(&mut file, &mut std::io::sink());
    }
}

fn emit_managed_thumbnail_progress(state: &AppState, app_handle: &AppHandle) {
    let (current, total) = state.thumbnail_manager.progress();
    let _ = app_handle.emit(
        "thumbnail-progress",
        serde_json::json!({"current":current,"total":total}),
    );
}

pub fn start_thumbnail_workers(app_handle: tauri::AppHandle) {
    let manager = app_handle.state::<AppState>().thumbnail_manager.clone();
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let thread_count = settings.thumbnail_worker_threads.unwrap_or(4).clamp(1, 16);
    for _ in 0..thread_count {
        let app = app_handle.clone();
        let manager = Arc::clone(&manager);
        std::thread::spawn(move || {
            loop {
                let work = manager.next();
                // Lease remains active until all renderer/output values have dropped.
                // A panic/error retires the lease and preserves any newer dirty work.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let state = app.state::<AppState>();
                    let metadata =
                        match manager.current(&work, || thumbnail_metadata(&work.path, &app)) {
                            Some(metadata) => metadata,
                            None => return,
                        };
                    let settings = load_settings(app.clone()).unwrap_or_default();
                    let snapshot_key = thumbnail_metadata_key(&metadata, &work.path, &settings);
                    let cache_dir = match resolve_thumbnail_cache_dir(&app) {
                        Ok(dir) => dir,
                        Err(error) => {
                            manager.current(&work, || {
                                emit_thumbnail_cache_setup_error(&app, &work.path, &error)
                            });
                            return;
                        }
                    };
                    if manager.rotational_disk.load(Ordering::Relaxed) {
                        let _io = manager.io_gate.lock().unwrap();
                        prefetch_source_file(&work.path);
                    }
                    if manager.current(&work, || ()).is_none() {
                        return;
                    }
                    if !manager.wait_for_render_admission(&work) {
                        return;
                    }
                    let gpu = gpu_processing::get_or_init_gpu_context(&state, &app).ok();
                    let output = generate_single_thumbnail_from_metadata(
                        &work.path,
                        &cache_dir,
                        gpu.as_ref(),
                        None,
                        work.force,
                        &app,
                        &settings,
                        metadata,
                    );
                    manager.current(&work, || {
                        // Also reject metadata changed by legacy independent file
                        // operations. Autosave writes share the publication fence.
                        let current = thumbnail_metadata(&work.path, &app);
                        if thumbnail_metadata_key(&current, &work.path, &settings) != snapshot_key {
                            return;
                        }
                        match &output {
                            Some((small, medium, rating, edited)) => emit_thumbnail_generated(
                                &app, &work.path, small, medium, *rating, *edited,
                            ),
                            None => emit_thumbnail_cache_setup_error(
                                &app,
                                &work.path,
                                "Thumbnail generation failed",
                            ),
                        }
                    });
                }));
                if result.is_err() {
                    log::error!("Thumbnail worker panicked; work retired");
                }
                drop(work);
                emit_managed_thumbnail_progress(&app.state::<AppState>(), &app);
            }
        });
    }
}

#[tauri::command]
pub fn update_thumbnail_queue(
    paths: Vec<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let mut paths = paths;
    if state
        .thumbnail_manager
        .rotational_disk
        .load(Ordering::Relaxed)
    {
        paths.sort();
        paths.reverse();
    }
    state.thumbnail_manager.request_library(paths);
    emit_managed_thumbnail_progress(&state, &app_handle);
    Ok(())
}

pub fn add_to_thumbnail_queue(state: &AppState, count: usize, app_handle: &AppHandle) {
    let mut tracker = state.thumbnail_progress.lock().unwrap();
    tracker.total += count;
    let current = tracker.completed;
    let total = tracker.total;
    drop(tracker);

    let _ = app_handle.emit(
        "thumbnail-progress",
        serde_json::json!({ "current": current, "total": total }),
    );
}

pub fn increment_thumbnail_progress(state: &AppState, app_handle: &AppHandle) {
    let mut tracker = state.thumbnail_progress.lock().unwrap();
    tracker.completed += 1;
    let current = tracker.completed;
    let total = tracker.total;

    if current >= total {
        tracker.total = 0;
        tracker.completed = 0;
        drop(tracker);

        let _ = app_handle.emit(
            "thumbnail-progress",
            serde_json::json!({ "current": 0, "total": 0 }),
        );
        let _ = app_handle.emit("thumbnail-generation-complete", true);
    } else {
        drop(tracker);
        let _ = app_handle.emit(
            "thumbnail-progress",
            serde_json::json!({ "current": current, "total": total }),
        );
    }
}

fn emit_thumbnail_generated(
    app_handle: &AppHandle,
    path: &str,
    small_thumbnail_path: &str,
    medium_thumbnail_path: &str,
    rating: u8,
    is_edited: bool,
) {
    let _ = app_handle.emit(
        "thumbnail-generated",
        serde_json::json!({
            "path": path,
            "thumbnailPath": small_thumbnail_path,
            "previewPath": medium_thumbnail_path,
            "rating": rating,
            "is_edited": is_edited
        }),
    );
}

pub fn resolve_lens_params_in_adjustments(
    adjustments: &mut Value,
    exif_data: &Option<HashMap<String, String>>,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) {
    if let Some(map) = adjustments.as_object_mut() {
        let mode = map
            .get("lensCorrectionMode")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        if mode == "auto" {
            if let Some(exif) = exif_data {
                let exif_maker = exif.get("Make").map(|s| s.as_str()).unwrap_or("");
                let exif_model = exif.get("LensModel").map(|s| s.as_str()).unwrap_or("");
                let exif_camera_model = exif.get("Model").map(|s| s.as_str()).unwrap_or("");
                if let Some(db) = lens_db {
                    if let Some((detected_maker, detected_model)) =
                        crate::lens_correction::find_best_lens_match(
                            db,
                            exif_maker,
                            exif_model,
                            exif_camera_model,
                        )
                    {
                        map.insert(
                            "lensMaker".to_string(),
                            serde_json::to_value(&detected_maker).unwrap(),
                        );
                        map.insert(
                            "lensModel".to_string(),
                            serde_json::to_value(&detected_model).unwrap(),
                        );
                    } else {
                        map.remove("lensMaker");
                        map.remove("lensModel");
                    }
                }
            } else {
                map.remove("lensMaker");
                map.remove("lensModel");
            }
        }

        if let Some(db) = lens_db {
            let has_valid_lens = match (
                map.get("lensMaker").and_then(|v| v.as_str()),
                map.get("lensModel").and_then(|v| v.as_str()),
            ) {
                (Some(maker), Some(model)) if !maker.is_empty() && !model.is_empty() => {
                    let mut focal_length = 50.0;
                    let mut aperture = None;
                    let mut distance = None;

                    if let Some(exif) = exif_data {
                        if let Some(fl_str) = exif
                            .get("FocalLength")
                            .or(exif.get("FocalLengthIn35mmFilm"))
                            && let Ok(fl) = fl_str.replace(" mm", "").trim().parse::<f32>()
                        {
                            focal_length = fl;
                        }
                        if let Some(ap_str) = exif.get("ApertureValue").or(exif.get("FNumber"))
                            && let Ok(ap) = ap_str.replace("f/", "").trim().parse::<f32>()
                        {
                            aperture = Some(ap);
                        }
                        if let Some(dist_str) = exif.get("SubjectDistance")
                            && let Ok(dist) = dist_str.replace(" m", "").trim().parse::<f32>()
                        {
                            distance = Some(dist);
                        }
                    }

                    if let Some(params) = crate::lens_correction::resolve_lens_params(
                        db,
                        maker,
                        model,
                        focal_length,
                        aperture,
                        distance,
                    ) {
                        map.insert(
                            "lensDistortionParams".to_string(),
                            serde_json::to_value(params).unwrap(),
                        );
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !has_valid_lens {
                map.remove("lensDistortionParams");
            }
        }
    }
}

#[tauri::command]
pub fn get_supported_file_types() -> Result<serde_json::Value, String> {
    let raw_extensions: Vec<&str> = crate::formats::RAW_EXTENSIONS
        .iter()
        .map(|(ext, _)| *ext)
        .collect();
    let non_raw_extensions: Vec<&str> = crate::formats::NON_RAW_EXTENSIONS.to_vec();

    Ok(serde_json::json!({
        "raw": raw_extensions,
        "nonRaw": non_raw_extensions
    }))
}

#[tauri::command]
pub fn create_folder(path: String) -> Result<(), String> {
    let path_obj = Path::new(&path);
    if let (Some(parent), Some(new_folder_name_os)) = (path_obj.parent(), path_obj.file_name())
        && let Some(new_folder_name) = new_folder_name_os.to_str()
        && parent.exists()
    {
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            if let Ok(entry) = entry
                && entry.file_name().to_string_lossy().to_lowercase()
                    == new_folder_name.to_lowercase()
            {
                return Err("A folder with that name already exists.".to_string());
            }
        }
    }
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_folder(path: String, new_name: String, app_handle: AppHandle) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err("Path is not a directory.".to_string());
    }
    if let Some(parent) = p.parent() {
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            if let Ok(entry) = entry
                && entry.file_name().to_string_lossy().to_lowercase() == new_name.to_lowercase()
                && entry.path() != p
            {
                return Err("A folder with that name already exists.".to_string());
            }
        }
        let new_path = parent.join(&new_name);
        fs::rename(p, &new_path).map_err(|e| e.to_string())?;

        let new_folder_str = new_path.to_string_lossy().into_owned();
        sync_album_path_changes(&app_handle, None, None, Some((&path, &new_folder_str)));

        Ok(())
    } else {
        Err("Could not determine parent directory.".to_string())
    }
}

#[tauri::command]
pub fn delete_folder(path: String, app_handle: AppHandle) -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        if let Err(trash_error) = trash::delete(&path) {
            log::warn!(
                "Failed to move folder to trash: {}. Falling back to permanent delete.",
                trash_error
            );
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    }

    let mut deletions = HashSet::new();
    deletions.insert(path);
    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

#[tauri::command]
pub fn duplicate_file(
    path: String,
    target_album_id: Option<String>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let (source_path, source_sidecar_path) = parse_virtual_path(&path);
    if !source_path.is_file() {
        return Err("Source path is not a file.".to_string());
    }

    let parent = source_path
        .parent()
        .ok_or("Could not get parent directory")?;
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Could not get file stem")?;
    let extension = source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut counter = 1;
    let mut dest_path;
    loop {
        let new_stem = if counter == 1 {
            format!("{}_copy", stem)
        } else {
            format!("{}_copy_{}", stem, counter - 1)
        };
        dest_path = parent.join(format!("{}.{}", new_stem, extension));
        if !dest_path.exists() {
            break;
        }
        counter += 1;
    }

    fs::copy(&source_path, &dest_path).map_err(|e| e.to_string())?;

    if source_sidecar_path.exists()
        && let Some(dest_str) = dest_path.to_str()
    {
        let (_, dest_sidecar_path) = parse_virtual_path(dest_str);
        fs::copy(&source_sidecar_path, &dest_sidecar_path).map_err(|e| e.to_string())?;
    }

    let mut source_rrexif_name = source_path.file_name().unwrap().to_os_string();
    source_rrexif_name.push(".rrexif");
    let source_rrexif = source_path.with_file_name(source_rrexif_name);

    if source_rrexif.exists() {
        let mut dest_rrexif_name = dest_path.file_name().unwrap().to_os_string();
        dest_rrexif_name.push(".rrexif");
        let dest_rrexif = dest_path.with_file_name(dest_rrexif_name);
        let _ = fs::copy(&source_rrexif, &dest_rrexif);
    }

    let dest_path_str = dest_path.to_string_lossy().into_owned();

    if let Some(album_id) = target_album_id {
        let _ = add_to_album(album_id, vec![dest_path_str.clone()], app_handle);
    }

    Ok(dest_path_str)
}

fn find_all_associated_files(source_image_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut associated_files = vec![source_image_path.to_path_buf()];

    let mut rrexif_name = source_image_path
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    rrexif_name.push(".rrexif");
    let rrexif_path = source_image_path.with_file_name(rrexif_name);

    if rrexif_path.exists() {
        associated_files.push(rrexif_path);
    }

    let parent_dir = source_image_path
        .parent()
        .ok_or("Could not determine parent directory")?;
    let source_filename = source_image_path
        .file_name()
        .ok_or("Could not get source filename")?
        .to_string_lossy();

    let primary_sidecar_name = format!("{}.rrdata", source_filename);
    let virtual_copy_prefix = format!("{}.", source_filename);

    if let Ok(entries) = fs::read_dir(parent_dir) {
        for entry in entries.filter_map(Result::ok) {
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }

            let entry_os_filename = entry.file_name();
            let entry_filename = entry_os_filename.to_string_lossy();

            if entry_filename == primary_sidecar_name
                || (entry_filename.starts_with(&virtual_copy_prefix)
                    && entry_filename.ends_with(".rrdata"))
            {
                associated_files.push(entry_path);
            }
        }
    }

    Ok(associated_files)
}

#[tauri::command]
pub fn copy_files(source_paths: Vec<String>, destination_folder: String) -> Result<(), String> {
    let dest_path = Path::new(&destination_folder);
    if !dest_path.is_dir() {
        return Err(format!(
            "Destination is not a folder: {}",
            destination_folder
        ));
    }

    let unique_source_images: HashSet<PathBuf> = source_paths
        .iter()
        .map(|p| parse_virtual_path(p).0)
        .collect();

    let mut operations_to_perform = Vec::new();

    for source_image_path in &unique_source_images {
        let all_files_to_copy = find_all_associated_files(source_image_path)?;

        let source_parent = source_image_path
            .parent()
            .ok_or("Could not get parent directory")?;

        if source_parent == dest_path {
            let stem = source_image_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Could not get file stem")?;
            let extension = source_image_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            let mut counter = 1;
            let new_base_path = loop {
                let new_stem = format!("{}_copy_{}", stem, counter);
                let temp_path = source_parent.join(format!("{}.{}", new_stem, extension));
                if !temp_path.exists() {
                    break temp_path;
                }
                counter += 1;
            };
            let new_filename = new_base_path.file_name().unwrap().to_string_lossy();

            for original_file in all_files_to_copy {
                let original_full_filename = original_file.file_name().unwrap().to_string_lossy();
                let source_base_filename = source_image_path.file_name().unwrap().to_string_lossy();
                let new_dest_filename =
                    original_full_filename.replacen(&*source_base_filename, &new_filename, 1);

                let final_dest_path = dest_path.join(new_dest_filename);
                operations_to_perform.push((original_file, final_dest_path));
            }
        } else {
            for file_to_copy in all_files_to_copy {
                if let Some(file_name) = file_to_copy.file_name() {
                    let dest_file_path = dest_path.join(file_name);

                    if dest_file_path.exists() {
                        return Err(format!(
                            "Copy aborted: File already exists at destination: {}",
                            dest_file_path.display()
                        ));
                    }

                    operations_to_perform.push((file_to_copy, dest_file_path));
                }
            }
        }
    }

    for (source, dest) in operations_to_perform {
        fs::copy(&source, &dest)
            .map_err(|e| format!("Copy failed for {}: {}", source.display(), e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn move_files(
    source_paths: Vec<String>,
    destination_folder: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    let dest_path = Path::new(&destination_folder);
    if !dest_path.is_dir() {
        return Err(format!(
            "Destination is not a folder: {}",
            destination_folder
        ));
    }

    let unique_source_images: HashSet<PathBuf> = source_paths
        .iter()
        .map(|p| parse_virtual_path(p).0)
        .collect();

    let mut operations_to_perform = Vec::new();
    let mut renames = HashMap::new();

    for source_image_path in &unique_source_images {
        let source_parent = source_image_path
            .parent()
            .ok_or("Could not get parent directory")?;

        if source_parent == dest_path {
            return Err("Cannot move files into the same folder they are already in.".to_string());
        }

        let all_files_to_move = find_all_associated_files(source_image_path)?;

        for file_to_move in &all_files_to_move {
            if let Some(file_name) = file_to_move.file_name() {
                let dest_file_path = dest_path.join(file_name);

                if dest_file_path.exists() {
                    return Err(format!(
                        "Move aborted: File already exists at destination: {}",
                        dest_file_path.display()
                    ));
                }

                operations_to_perform.push((file_to_move.clone(), dest_file_path));
            }
        }

        let dest_image_path = dest_path.join(source_image_path.file_name().unwrap());
        renames.insert(
            source_image_path.to_string_lossy().into_owned(),
            dest_image_path.to_string_lossy().into_owned(),
        );
    }

    for (source, dest) in operations_to_perform {
        if fs::rename(&source, &dest).is_err() {
            fs::copy(&source, &dest)
                .map_err(|e| format!("Move failed during copy for {}: {}", source.display(), e))?;

            if let Err(e) = fs::remove_file(&source) {
                log::warn!(
                    "Moved file successfully, but failed to delete original {}: {}",
                    source.display(),
                    e
                );
            }
        }
    }

    sync_album_path_changes(&app_handle, Some(&renames), None, None);

    Ok(())
}

#[tauri::command]
pub fn save_metadata_and_update_thumbnail(
    path: String,
    adjustments: Value,
    app_handle: AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    state.thumbnail_manager.edit(&path, || {
        let (source_path, sidecar_path) = parse_virtual_path(&path);

        let mut metadata =
            crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path);

        let mut final_adjustments = adjustments;
        {
            let lens_db_guard = state.lens_db.lock().unwrap();
            resolve_lens_params_in_adjustments(
                &mut final_adjustments,
                &metadata.exif,
                lens_db_guard.as_deref(),
            );
        }

        metadata.adjustments = final_adjustments;

        crate::exif_processing::write_sidecar(&sidecar_path, &source_path, &metadata)
            .map_err(|e| e.to_string())?;

        if let Ok(settings) = load_settings(app_handle.clone())
            && settings.enable_xmp_sync.unwrap_or(false)
        {
            let create_if_missing = settings.create_xmp_if_missing.unwrap_or(false);
            sync_metadata_to_xmp(&source_path, &metadata, create_if_missing);
        }

        Ok(())
    })?;
    emit_managed_thumbnail_progress(&state, &app_handle);
    Ok(())
}

#[tauri::command]
pub async fn apply_adjustments_to_paths(
    paths: Vec<String>,
    adjustments: Value,
    app_handle: AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        let lens_db = app_handle
            .state::<AppState>()
            .lens_db
            .lock()
            .unwrap()
            .clone();

        paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);

            let mut existing_metadata =
                crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path);

            let mut new_adjustments = existing_metadata.adjustments;
            if new_adjustments.is_null() {
                new_adjustments = default_adjustments();
            }

            if let (Some(new_map), Some(pasted_map)) =
                (new_adjustments.as_object_mut(), adjustments.as_object())
            {
                for (k, v) in pasted_map {
                    new_map.insert(k.clone(), v.clone());
                }
            }

            resolve_lens_params_in_adjustments(
                &mut new_adjustments,
                &existing_metadata.exif,
                lens_db.as_deref(),
            );

            existing_metadata.adjustments = new_adjustments;

            let _ = crate::exif_processing::write_sidecar(
                &sidecar_path,
                &source_path,
                &existing_metadata,
            );

            if enable_xmp_sync {
                sync_metadata_to_xmp(&source_path, &existing_metadata, create_xmp_if_missing);
            }
        });

        regenerate_thumbnails(&paths, &app_handle, &settings);
    });

    Ok(())
}

#[tauri::command]
pub async fn reset_adjustments_for_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);

            let mut existing_metadata = crate::exif_processing::load_sidecar(&sidecar_path);

            existing_metadata.adjustments = default_adjustments();

            let _ = crate::exif_processing::write_sidecar(
                &sidecar_path,
                &source_path,
                &existing_metadata,
            );

            if enable_xmp_sync {
                sync_metadata_to_xmp(&source_path, &existing_metadata, create_xmp_if_missing);
            }
        });

        regenerate_thumbnails(&paths, &app_handle, &settings);
    });

    Ok(())
}

pub(crate) fn regenerate_thumbnails(
    paths: &[String],
    app_handle: &AppHandle,
    settings: &AppSettings,
) {
    let state = app_handle.state::<AppState>();
    let thumb_cache_dir = match resolve_thumbnail_cache_dir(app_handle) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!("Unable to initialize thumbnail cache directory: {}", e);
            for path in paths {
                emit_thumbnail_cache_setup_error(app_handle, path, &e);
            }
            for _ in 0..paths.len() {
                increment_thumbnail_progress(&state, app_handle);
            }
            return;
        }
    };

    let gpu_context = gpu_processing::get_or_init_gpu_context(&state, app_handle).ok();

    paths.par_iter().for_each(|path_str| {
        let result = generate_single_thumbnail_and_cache(
            path_str,
            &thumb_cache_dir,
            gpu_context.as_ref(),
            None,
            true,
            app_handle,
            settings,
        );

        if let Some((small_path, medium_path, rating, is_edited)) = result {
            emit_thumbnail_generated(
                app_handle,
                path_str,
                &small_path,
                &medium_path,
                rating,
                is_edited,
            );
        }

        increment_thumbnail_progress(&state, app_handle);
    });
}

#[tauri::command]
pub async fn apply_auto_lens_correction_to_paths(
    paths: Vec<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
    let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);
    let lens_db = app_handle
        .state::<AppState>()
        .lens_db
        .lock()
        .unwrap()
        .clone();

    let sidecar_paths = paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        sidecar_paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);
            let mut existing_metadata =
                crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path);

            if existing_metadata.adjustments.is_null() {
                existing_metadata.adjustments = default_adjustments();
            }

            if let Some(obj) = existing_metadata.adjustments.as_object_mut() {
                obj.insert("lensCorrectionMode".to_string(), serde_json::json!("auto"));
                obj.insert("lensDistortionEnabled".to_string(), serde_json::json!(true));
                obj.insert("lensTcaEnabled".to_string(), serde_json::json!(true));
                obj.insert("lensVignetteEnabled".to_string(), serde_json::json!(true));
            }

            resolve_lens_params_in_adjustments(
                &mut existing_metadata.adjustments,
                &existing_metadata.exif,
                lens_db.as_deref(),
            );

            let _ = crate::exif_processing::write_sidecar(
                &sidecar_path,
                &source_path,
                &existing_metadata,
            );

            if enable_xmp_sync {
                sync_metadata_to_xmp(&source_path, &existing_metadata, create_xmp_if_missing);
            }
        });
    })
    .await
    .map_err(|e| e.to_string())?;

    add_to_thumbnail_queue(&app_handle.state::<AppState>(), paths.len(), &app_handle);
    tauri::async_runtime::spawn_blocking(move || {
        regenerate_thumbnails(&paths, &app_handle, &settings)
    });

    Ok(())
}

const FULL_DECODE_WORKER_THREADS: usize = 2;

async fn update_sidecars_from_images(
    paths: Vec<String>,
    app_handle: AppHandle,
    operation: &'static str,
    fast_decode: bool,
    update: impl Fn(&DynamicImage, &str, &AppSettings, &mut Value) + Send + Sync + 'static,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        let state = app_handle.state::<AppState>();
        let thumb_cache_dir = match resolve_thumbnail_cache_dir(&app_handle) {
            Ok(dir) => dir,
            Err(e) => {
                log::warn!("Unable to initialize thumbnail cache directory: {}", e);
                for path in &paths {
                    emit_thumbnail_cache_setup_error(&app_handle, path, &e);
                }
                for _ in 0..paths.len() {
                    increment_thumbnail_progress(&state, &app_handle);
                }
                return;
            }
        };

        let gpu_context = gpu_processing::get_or_init_gpu_context(&state, &app_handle).ok();

        let process = |path: &String| {
            let loaded_image: Option<DynamicImage> = (|| -> Result<DynamicImage, String> {
                let (source_path, sidecar_path) = parse_virtual_path(path);
                let source_path_str = source_path.to_string_lossy().to_string();

                let file_bytes = fs::read(&source_path).map_err(|e| e.to_string())?;
                let image = image_loader::load_base_image_from_bytes(
                    &file_bytes,
                    &source_path_str,
                    fast_decode,
                    &settings,
                    None,
                )
                .map_err(|e| e.to_string())?;

                let mut existing_metadata = crate::exif_processing::load_sidecar(&sidecar_path);

                if existing_metadata.adjustments.is_null() {
                    existing_metadata.adjustments = default_adjustments();
                }

                update(
                    &image,
                    &source_path_str,
                    &settings,
                    &mut existing_metadata.adjustments,
                );

                let _ = crate::exif_processing::write_sidecar(
                    &sidecar_path,
                    &source_path,
                    &existing_metadata,
                );

                if enable_xmp_sync {
                    sync_metadata_to_xmp(&source_path, &existing_metadata, create_xmp_if_missing);
                }
                Ok(image)
            })()
            .map_err(|e| eprintln!("Failed to apply {} to {}: {}", operation, path, e))
            .ok();

            let result = generate_single_thumbnail_and_cache(
                path,
                &thumb_cache_dir,
                gpu_context.as_ref(),
                loaded_image.as_ref(),
                true,
                &app_handle,
                &settings,
            );

            if let Some((small_path, medium_path, rating, is_edited)) = result {
                emit_thumbnail_generated(
                    &app_handle,
                    path,
                    &small_path,
                    &medium_path,
                    rating,
                    is_edited,
                );
            }

            increment_thumbnail_progress(&state, &app_handle);
        };

        if fast_decode {
            paths.par_iter().for_each(process);
        } else if let Ok(pool) = rayon::ThreadPoolBuilder::new()
            .num_threads(FULL_DECODE_WORKER_THREADS)
            .build()
        {
            pool.install(|| paths.par_iter().for_each(process));
        } else {
            paths.iter().for_each(process);
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_auto_adjustments_to_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_sidecars_from_images(
        paths,
        app_handle,
        "auto adjustments",
        true,
        |image, source_path, settings, adjustments| {
            let auto_results = perform_auto_analysis(
                image,
                effective_baseline_exposure(
                    adjustments,
                    as_shot(source_path),
                    BaselineOptions::from_settings(settings),
                ),
            );
            let auto_adjustments_json = auto_results_to_json(&auto_results);

            let mut active_tools = crate::image_processing::active_tools(adjustments);

            if let (Some(existing_map), Some(auto_map)) = (
                adjustments.as_object_mut(),
                auto_adjustments_json.as_object(),
            ) {
                for (k, v) in auto_map {
                    if k == "activeTools" {
                        if let Some(auto_tools) = v.as_object() {
                            active_tools.extend(auto_tools.clone());
                        }
                        existing_map.insert(
                            k.clone(),
                            serde_json::Value::Object(std::mem::take(&mut active_tools)),
                        );
                    } else {
                        existing_map.insert(k.clone(), v.clone());
                    }
                }
            }
        },
    )
    .await
}

#[tauri::command]
pub async fn apply_auto_straighten_to_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_sidecars_from_images(
        paths,
        app_handle,
        "auto straighten",
        false,
        |image, source_path, _, adjustments| {
            crate::auto_straighten::straighten_adjustments(image, source_path, adjustments)
        },
    )
    .await
}
fn update_metadata_for_paths(
    paths: &[String],
    app_handle: &AppHandle,
    update: impl Fn(&mut ImageMetadata) + Sync,
) {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
    let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

    paths.par_iter().for_each(|path| {
        let (source_path, sidecar_path) = parse_virtual_path(path);

        let mut metadata = crate::exif_processing::load_sidecar(&sidecar_path);

        update(&mut metadata);

        let _ = crate::exif_processing::write_sidecar(&sidecar_path, &source_path, &metadata);

        if enable_xmp_sync {
            sync_metadata_to_xmp(&source_path, &metadata, create_xmp_if_missing);
        }
    });
}

#[tauri::command]
pub fn set_color_label_for_paths(
    paths: Vec<String>,
    color: Option<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        let mut tags = metadata.tags.take().unwrap_or_default();
        tags.retain(|tag| !tag.starts_with(COLOR_TAG_PREFIX));

        if let Some(c) = &color
            && !c.is_empty()
        {
            tags.push(format!("{}{}", COLOR_TAG_PREFIX, c));
        }

        if !tags.is_empty() {
            metadata.tags = Some(tags);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn set_rating_for_paths(
    paths: Vec<String>,
    rating: u8,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        metadata.rating = rating;
        if rating > 0 && metadata.flag == Some(ImageFlag::Reject) {
            metadata.flag = None;
        }
    });

    Ok(())
}

#[tauri::command]
pub fn set_flag_for_paths(
    paths: Vec<String>,
    flag: Option<ImageFlag>,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        metadata.flag = flag;
    });

    Ok(())
}

#[tauri::command]
pub fn load_metadata(path: String, app_handle: AppHandle) -> Result<ImageMetadata, String> {
    let settings = load_settings(app_handle).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let mut metadata = crate::exif_processing::load_sidecar(&sidecar_path);

    if enable_xmp_sync && sync_metadata_from_xmp(&source_path, &mut metadata) {
        let _ = crate::exif_processing::write_sidecar(&sidecar_path, &source_path, &metadata);
    }

    Ok(metadata)
}

fn get_presets_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    let presets_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("presets");

    if !presets_dir.exists() {
        fs::create_dir_all(&presets_dir).map_err(|e| e.to_string())?;
    }

    Ok(presets_dir.join("presets.json"))
}

#[tauri::command]
pub fn load_presets(app_handle: AppHandle) -> Result<Vec<PresetItem>, String> {
    let path = get_presets_path(&app_handle)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_presets(presets: Vec<PresetItem>, app_handle: AppHandle) -> Result<(), String> {
    let path = get_presets_path(&app_handle)?;
    let json_string = serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())?;
    fs::write(path, json_string).map_err(|e| e.to_string())
}

fn get_internal_library_root_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    #[cfg(not(target_os = "android"))]
    {
        let library_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("library");

        if !library_dir.exists() {
            fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;
        }
        Ok(library_dir)
    }
    #[cfg(target_os = "android")]
    {
        crate::android_integration::get_android_internal_library_root()
    }
}

#[tauri::command]
pub fn get_or_create_internal_library_root(app_handle: AppHandle) -> Result<String, String> {
    let library_root = get_internal_library_root_path(&app_handle)?;

    Ok(library_root.to_string_lossy().to_string())
}

fn collect_top_level_preset_names(items: &[PresetItem]) -> HashSet<String> {
    items
        .iter()
        .map(|item| match item {
            PresetItem::Preset(p) => p.name.clone(),
            PresetItem::Folder(f) => f.name.clone(),
        })
        .collect()
}

/// Keep document transport separate from parsing: SAF identifiers are not paths,
/// and the provider's display name, not the URI suffix, determines the format.
fn parse_preset_source_with(
    source: &str,
    resolved_name: &mut Option<String>,
    resolve_name: impl FnOnce(&str) -> Result<String, String>,
    read_bytes: impl FnOnce(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<PresetItem>, String> {
    let name = resolve_name(source)?;
    *resolved_name = Some(name.clone());
    if name.trim().is_empty() {
        return Err(
            "The selected document has no file name. Choose a .rrpreset, .xmp or .lrtemplate file."
                .into(),
        );
    }
    let bytes = read_bytes(source)?;
    parse_preset_bytes(&name, &bytes)
}

fn read_preset_source(
    source: &str,
    resolved_name: &mut Option<String>,
) -> Result<Vec<PresetItem>, String> {
    #[cfg(target_os = "android")]
    if crate::android_integration::is_android_content_uri(source) {
        return parse_preset_source_with(
            source,
            resolved_name,
            crate::android_integration::resolve_android_content_uri_name,
            crate::android_integration::read_android_content_uri,
        );
    }

    parse_preset_source_with(
        source,
        resolved_name,
        |path| {
            Ok(Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string()))
        },
        |path| fs::read(path).map_err(|e| format!("Failed to read preset file: {e}")),
    )
}

fn parse_preset_file(file_path: &str) -> Result<Vec<PresetItem>, String> {
    let mut name = None;
    read_preset_source(file_path, &mut name).map_err(|error| {
        let label = import_item_label(file_path, name.as_deref(), 0);
        import_failure_detail(&label, file_path, &error, name.as_deref())
    })
}

fn parse_preset_bytes(file_name: &str, bytes: &[u8]) -> Result<Vec<PresetItem>, String> {
    let content = String::from_utf8(bytes.to_vec())
        .map_err(|e| format!("Preset file is not valid UTF-8: {e}"))?;
    let lower_name = file_name.to_lowercase();
    let is_legacy = lower_name.ends_with(".xmp") || lower_name.ends_with(".lrtemplate");

    if !is_legacy {
        let preset_file: PresetFile = serde_json::from_str(&content).map_err(|e| {
            format!(
                "Failed to parse preset file: {e}. Choose a .rrpreset, .xmp or .lrtemplate file."
            )
        })?;
        return Ok(preset_file.presets);
    }

    let xmp_content = if lower_name.ends_with(".lrtemplate") {
        if let Some(caps) = regex!(r#"(?s)s.xmp = "(.*)""#).captures(&content) {
            caps.get(1)
                .map(|m| m.as_str().replace(r#"\""#, r#"""#))
                .unwrap_or(content)
        } else {
            content
        }
    } else {
        content
    };

    reject_profile_look_as_preset(&xmp_content)?;
    let converted_preset = preset_converter::convert_xmp_to_preset(&xmp_content)?;
    if converted_preset
        .adjustments
        .as_object()
        .is_some_and(|adjustments| adjustments.is_empty())
    {
        return Err("This XMP contains no supported adjustment settings. For a profile Look, use Camera Profile > Import Look (.xmp); otherwise choose an adjustment preset with supported settings.".into());
    }
    Ok(vec![PresetItem::Preset(converted_preset)])
}

/// Profile Looks contain rendering tables, not exposure/colour adjustment presets.
/// Inspect XML attributes without decoding tables or changing the converter.
fn reject_profile_look_as_preset(content: &str) -> Result<(), String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(content);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element) | Event::Empty(element)) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|e| format!("Invalid preset XML: {e}"))?;
                    if attribute.key.as_ref() == b"crs:RGBTable"
                        || (attribute.key.as_ref() == b"crs:PresetType"
                            && attribute
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .map_err(|e| format!("Invalid preset XML: {e}"))?
                                == "Look")
                    {
                        return Err("This XMP is a profile Look, not an adjustment preset. Use Camera Profile > Import Look (.xmp) with its required base DCP.".into());
                    }
                }
            }
            Ok(Event::Eof) => return Ok(()),
            Err(error) => return Err(format!("Invalid preset XML: {error}")),
            _ => {}
        }
        buffer.clear();
    }
}

fn merge_imported_items(
    target: &mut Vec<PresetItem>,
    taken_names: &mut HashSet<String>,
    imported: Vec<PresetItem>,
) {
    for mut imported_item in imported {
        let original_name = match &mut imported_item {
            PresetItem::Preset(p) => {
                p.id = Uuid::new_v4().to_string();
                p.name.clone()
            }
            PresetItem::Folder(f) => {
                f.id = Uuid::new_v4().to_string();
                for child in &mut f.children {
                    child.id = Uuid::new_v4().to_string();
                }
                f.name.clone()
            }
        };

        let mut new_name = original_name.clone();
        let mut counter = 1;
        while taken_names.contains(&new_name) {
            new_name = format!("{} ({})", original_name, counter);
            counter += 1;
        }

        match &mut imported_item {
            PresetItem::Preset(p) => p.name = new_name.clone(),
            PresetItem::Folder(f) => f.name = new_name.clone(),
        }

        taken_names.insert(new_name);
        target.push(imported_item);
    }
}

fn import_preset_file_into_library(
    file_path: &str,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    let imported = parse_preset_file(file_path)?;

    let mut current_presets = load_presets(app_handle.clone())?;
    let mut taken_names = collect_top_level_preset_names(&current_presets);
    merge_imported_items(&mut current_presets, &mut taken_names, imported);

    save_presets(current_presets.clone(), app_handle)?;
    Ok(current_presets)
}

#[tauri::command]
pub fn handle_import_presets_from_file(
    file_path: String,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    import_preset_file_into_library(&file_path, app_handle)
}

#[tauri::command]
pub fn handle_import_legacy_presets_from_file(
    file_path: String,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    import_preset_file_into_library(&file_path, app_handle)
}

fn import_preset_files_with(
    file_paths: &[String],
    mut current_presets: Vec<PresetItem>,
    mut parse: impl FnMut(&str, &mut Option<String>) -> Result<Vec<PresetItem>, String>,
) -> PresetImportResult {
    let mut taken_names = collect_top_level_preset_names(&current_presets);
    let mut failures = Vec::new();
    let mut imported_count = 0;
    for (index, source) in file_paths.iter().enumerate() {
        let mut name = None;
        match parse(source, &mut name) {
            Ok(imported) => {
                imported_count += imported.len();
                merge_imported_items(&mut current_presets, &mut taken_names, imported);
            }
            Err(error) => {
                let label = import_item_label(source, name.as_deref(), index);
                failures.push(PresetImportFailure {
                    file_name: label.clone(),
                    error: import_failure_detail(&label, source, &error, name.as_deref()),
                });
            }
        }
    }
    PresetImportResult {
        imported: imported_count,
        presets: current_presets,
        failures,
    }
}

#[tauri::command]
pub fn handle_import_presets_from_files(
    file_paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<PresetImportResult, String> {
    let current_presets = load_presets(app_handle.clone())?;
    let result = import_preset_files_with(&file_paths, current_presets, read_preset_source);
    if result.imported > 0 {
        save_presets(result.presets.clone(), app_handle)?;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "preset_import_tests.rs"]
mod preset_import_tests;

#[tauri::command]
pub fn handle_export_presets_to_file(
    presets_to_export: Vec<PresetItem>,
    file_path: String,
) -> Result<(), String> {
    let preset_file = ExportPresetFile {
        creator: "Anonymous",
        presets: &presets_to_export,
    };

    let json_string = serde_json::to_string_pretty(&preset_file)
        .map_err(|e| format!("Failed to serialize presets: {}", e))?;
    fs::write(file_path, json_string).map_err(|e| format!("Failed to write preset file: {}", e))
}

#[tauri::command]
pub fn save_community_preset(
    name: String,
    adjustments: Value,
    app_handle: AppHandle,
    include_masks: Option<bool>,
    include_crop_transform: Option<bool>,
    preset_type: Option<String>,
) -> Result<(), String> {
    let mut current_presets = load_presets(app_handle.clone())?;

    let community_folder_name = "Community";
    let community_folder_id = match current_presets.iter_mut().find(|item| {
        if let PresetItem::Folder(f) = item {
            f.name == community_folder_name
        } else {
            false
        }
    }) {
        Some(PresetItem::Folder(folder)) => folder.id.clone(),
        _ => {
            let new_folder_id = Uuid::new_v4().to_string();
            let new_folder = PresetItem::Folder(PresetFolder {
                id: new_folder_id.clone(),
                name: community_folder_name.to_string(),
                children: Vec::new(),
            });
            current_presets.insert(0, new_folder);
            new_folder_id
        }
    };

    let new_preset = Preset {
        id: Uuid::new_v4().to_string(),
        name,
        adjustments,
        include_masks,
        include_crop_transform,
        preset_type: preset_type.or(Some("style".to_string())),
        include_profile: None,
    };

    if let Some(PresetItem::Folder(folder)) = current_presets.iter_mut().find(|item| {
        if let PresetItem::Folder(f) = item {
            f.id == community_folder_id
        } else {
            false
        }
    }) {
        folder.children.retain(|p| p.name != new_preset.name);
        folder.children.push(new_preset);
    }

    save_presets(current_presets, app_handle)
}

#[tauri::command]
pub fn clear_all_sidecars(root_path: String) -> Result<usize, String> {
    if !Path::new(&root_path).exists() {
        return Err(format!("Root path does not exist: {}", root_path));
    }

    let mut deleted_count = 0;
    let walker = WalkDir::new(root_path).into_iter();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && let Some(extension) = path.extension()
            && (extension == "rrdata" || extension == "rrexif")
        {
            if fs::remove_file(path).is_ok() {
                deleted_count += 1;
            } else {
                eprintln!("Failed to delete sidecar file: {:?}", path);
            }
        }
    }

    Ok(deleted_count)
}

#[tauri::command]
pub fn clear_thumbnail_cache(app_handle: AppHandle) -> Result<(), String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");

    if thumb_cache_dir.exists() {
        fs::remove_dir_all(&thumb_cache_dir)
            .map_err(|e| format!("Failed to remove thumbnail cache: {}", e))?;
    }

    fs::create_dir_all(&thumb_cache_dir)
        .map_err(|e| format!("Failed to recreate thumbnail cache directory: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let (source_path, _) = parse_virtual_path(&path);

    #[cfg(target_os = "windows")]
    {
        let source_path_str = source_path.to_string_lossy().to_string();
        Command::new("explorer")
            .args(["/select,", &source_path_str])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        let source_path_str = source_path.to_string_lossy().to_string();
        Command::new("open")
            .args(["-R", &source_path_str])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = source_path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Could not get parent directory".into());
        }
    }

    #[cfg(target_os = "android")]
    {
        return Err("Show in File Manager is not natively supported via CLI on Android.".into());
    }

    #[cfg(target_os = "ios")]
    {
        return Err("Show in File Manager is not supported on iOS.".into());
    }

    Ok(())
}

#[tauri::command]
pub fn delete_files_from_disk(paths: Vec<String>, app_handle: AppHandle) -> Result<(), String> {
    let mut files_to_trash = HashSet::new();
    let mut deletions = HashSet::new();

    for path_str in paths {
        let (source_path, sidecar_path) = parse_virtual_path(&path_str);
        deletions.insert(path_str.clone());

        if path_str.contains("?vc=") {
            if sidecar_path.exists() {
                files_to_trash.insert(sidecar_path);
            }
        } else {
            if source_path.exists() {
                match find_all_associated_files(&source_path) {
                    Ok(associated_files) => {
                        for file in associated_files {
                            files_to_trash.insert(file);
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Could not find associated files for {}: {}",
                            source_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    if files_to_trash.is_empty() {
        return Ok(());
    }

    let final_paths_to_delete: Vec<PathBuf> = files_to_trash.into_iter().collect();

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    if let Err(trash_error) = trash::delete_all(&final_paths_to_delete) {
        log::warn!(
            "Failed to move files to trash: {}. Falling back to permanent delete.",
            trash_error
        );
        for path in final_paths_to_delete {
            if path.is_file() {
                if let Err(e) = fs::remove_file(&path) {
                    log::warn!("Failed to delete file {}: {}", path.display(), e);
                }
            } else if path.is_dir()
                && let Err(e) = fs::remove_dir_all(&path)
            {
                log::warn!("Failed to delete directory {}: {}", path.display(), e);
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    for path in final_paths_to_delete {
        if path.is_file() {
            if let Err(e) = fs::remove_file(&path) {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        } else if path.is_dir() {
            if let Err(e) = fs::remove_dir_all(&path) {
                log::warn!("Failed to delete directory {}: {}", path.display(), e);
            }
        }
    }

    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

fn deletion_stem_for(filename: &str) -> Option<&str> {
    let image_filename = if filename.ends_with(".rrdata") {
        let without_rrdata = filename.trim_end_matches(".rrdata");
        if let Some(dot_pos) = without_rrdata.rfind('.') {
            let suffix = &without_rrdata[dot_pos + 1..];
            if suffix.len() == 6 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
                &without_rrdata[..dot_pos]
            } else {
                without_rrdata
            }
        } else {
            without_rrdata
        }
    } else if filename.ends_with(".rrexif") {
        filename.trim_end_matches(".rrexif")
    } else if is_supported_image_file(filename) {
        filename
    } else {
        return None;
    };
    Path::new(image_filename)
        .file_stem()
        .and_then(|s| s.to_str())
}

#[tauri::command]
pub fn delete_files_with_associated(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut stems_to_delete = HashSet::new();
    let mut parent_dirs = HashSet::new();
    let mut deletions = HashSet::new();

    for path_str in &paths {
        deletions.insert(path_str.clone());
        let (source_path, _) = parse_virtual_path(path_str);
        if let Some(stem) = source_path.file_stem().and_then(|s| s.to_str()) {
            stems_to_delete.insert(stem.to_string());
        }
        if let Some(parent) = source_path.parent() {
            parent_dirs.insert(parent.to_path_buf());
        }
    }

    if stems_to_delete.is_empty() {
        return Ok(());
    }

    let mut files_to_trash = HashSet::new();

    for parent_dir in parent_dirs {
        if let Ok(entries) = fs::read_dir(parent_dir) {
            for entry in entries.filter_map(Result::ok) {
                let entry_path = entry.path();
                if !entry_path.is_file() {
                    continue;
                }

                let entry_filename = entry.file_name();
                let entry_filename_str = entry_filename.to_string_lossy();

                if let Some(stem) = deletion_stem_for(&entry_filename_str)
                    && stems_to_delete.contains(stem)
                {
                    files_to_trash.insert(entry_path);
                }
            }
        }
    }

    if files_to_trash.is_empty() {
        return Ok(());
    }

    let final_paths_to_delete: Vec<PathBuf> = files_to_trash.into_iter().collect();

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    if let Err(trash_error) = trash::delete_all(&final_paths_to_delete) {
        log::warn!(
            "Failed to move files to trash: {}. Falling back to permanent delete.",
            trash_error
        );
        for path in final_paths_to_delete {
            if path.is_file()
                && let Err(e) = fs::remove_file(&path)
            {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    for path in final_paths_to_delete {
        if path.is_file() {
            if let Err(e) = fs::remove_file(&path) {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        }
    }

    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

pub fn get_thumb_cache_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");
    if !thumb_cache_dir.exists() {
        fs::create_dir_all(&thumb_cache_dir).map_err(|e| e.to_string())?;
    }
    Ok(thumb_cache_dir)
}

pub fn get_cache_key_hash(path_str: &str) -> Option<String> {
    let (_, sidecar_path) = parse_virtual_path(path_str);

    let adjustments_bytes = if let Ok(content) = fs::read_to_string(&sidecar_path) {
        if let Ok(meta) = serde_json::from_str::<ImageMetadata>(&content) {
            serde_json::to_vec(&meta.adjustments).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    compute_thumbnail_cache_hash(path_str, &adjustments_bytes)
}

pub fn get_cached_or_generate_thumbnail_image(
    path_str: &str,
    app_handle: &AppHandle,
    gpu_context: Option<&GpuContext>,
) -> Result<DynamicImage> {
    let thumb_cache_dir = get_thumb_cache_dir(app_handle).map_err(|e| anyhow::anyhow!(e))?;
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let target_width_small = settings.small_thumbnail_resolution.unwrap_or(480);
    let target_width_medium = settings.medium_thumbnail_resolution.unwrap_or(1280);

    if let Some(cache_hash) = get_cache_key_hash(path_str) {
        let cache_path = thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash));

        if cache_path.exists() {
            if let Ok(image) = image::open(&cache_path) {
                return Ok(image);
            }
            eprintln!(
                "Could not open cached thumbnail, regenerating: {:?}",
                cache_path
            );
        }

        let thumb_image = generate_thumbnail_data(path_str, gpu_context, None, app_handle)?;
        if let (Ok(small_data), Ok(medium_data)) = (
            encode_thumbnail(&thumb_image, target_width_small),
            encode_thumbnail(&thumb_image, target_width_medium),
        ) {
            let _ = fs::write(
                thumb_cache_dir.join(format!("{}_small.jpg", cache_hash)),
                &small_data,
            );
            let _ = fs::write(
                thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash)),
                &medium_data,
            );
        }

        Ok(thumb_image)
    } else {
        generate_thumbnail_data(path_str, gpu_context, None, app_handle)
    }
}

/// Filesystem steps used to publish an imported file. Tests replace individual
/// steps to inject failures; production uses `ImportStagingOps::default()`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct ImportStagingOps<'a> {
    write: &'a dyn Fn(&mut fs::File, &[u8]) -> std::io::Result<()>,
    before_publish: &'a dyn Fn(&Path),
    persist: &'a dyn Fn(&Path, &Path) -> std::io::Result<()>,
    remove: &'a dyn Fn(&Path) -> std::io::Result<()>,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn write_and_sync(file: &mut fs::File, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn no_publish_hook(_: &Path) {}

/// Never replaces an existing target. tempfile first tries an atomic
/// no-replace rename; where unsupported it hard-links and then unlinks the
/// temporary name, ignoring unlink errors (handled by the caller).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn persist_without_replacing(temp: &Path, dest: &Path) -> std::io::Result<()> {
    tempfile::TempPath::try_from_path(temp)?
        .persist_noclobber(dest)
        .map_err(|err| {
            // The caller owns cleanup of the temporary file.
            let _ = err.path.keep();
            err.error
        })
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn remove_path(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)
}

impl Default for ImportStagingOps<'static> {
    fn default() -> Self {
        ImportStagingOps {
            write: &write_and_sync,
            before_publish: &no_publish_hook,
            persist: &persist_without_replacing,
            remove: &remove_path,
        }
    }
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn import_display_basename(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

/// Accepts bytes that were actually read only when they are nonempty and do not
/// contradict the original display name. Other content is left to the decoders.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn validate_import_read(
    display_name: &str,
    read: Result<Vec<u8>, String>,
    source_label: &str,
) -> Result<Vec<u8>, String> {
    let name = import_display_basename(display_name);
    let bytes = read.map_err(|e| {
        let detail = if source_label.is_empty() {
            e
        } else {
            e.replace(source_label, "the selected item")
        };
        format!("Could not read “{}”: {}", name, detail)
    })?;

    match validate_image_content(display_name, &bytes) {
        Ok(()) => Ok(bytes),
        Err(ImageContentIssue::Empty) => Err(format!(
            "No image data was read for “{}”. Select the original file and try again.",
            name
        )),
        Err(issue) => Err(issue.open_error_message(display_name)),
    }
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct PublishedImport {
    path: PathBuf,
    cleanup_warning: Option<String>,
}

/// Writes `bytes` to a hidden temporary file in `dest_dir` and publishes it as
/// `file_name` without replacing anything. On failure no final file is created,
/// existing files are untouched and only the owned temporary file is removed.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn publish_import_bytes(
    dest_dir: &Path,
    file_name: &str,
    bytes: &[u8],
    ops: &ImportStagingOps,
) -> Result<PublishedImport, String> {
    let dest = dest_dir.join(file_name);
    let name = import_display_basename(file_name);

    if fs::symlink_metadata(&dest).is_ok() {
        return Err(format!(
            "File already exists at destination: {}",
            dest.display()
        ));
    }

    let (mut file, temp_path) = tempfile::Builder::new()
        .prefix(".rapidraw-import-")
        .suffix(".tmp")
        .tempfile_in(dest_dir)
        .and_then(|temp| temp.keep().map_err(|e| e.error))
        .map_err(|e| format!("Could not stage “{}”: {}", name, e))?;

    let temp_name = import_display_basename(&temp_path.to_string_lossy());
    // Removes the owned temporary file. Only success or NotFound proves it is
    // gone; any other error (including an unreadable directory) is reported.
    let remove_temp = || match (ops.remove)(&temp_path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Some(e),
        _ => None,
    };
    let with_cleanup = |primary: String| -> String {
        match remove_temp() {
            None => primary,
            Some(e) => format!(
                "{} The temporary file “{}” could not be removed: {}",
                primary, temp_name, e
            ),
        }
    };

    let written = (ops.write)(&mut file, bytes);
    drop(file);
    if let Err(e) = written {
        return Err(with_cleanup(format!("Could not write “{}”: {}", name, e)));
    }

    (ops.before_publish)(&dest);

    if let Err(e) = (ops.persist)(&temp_path, &dest) {
        let primary = if e.kind() == std::io::ErrorKind::AlreadyExists {
            format!("File already exists at destination: {}", dest.display())
        } else {
            format!("Could not publish “{}”: {}", name, e)
        };
        return Err(with_cleanup(primary));
    }

    // A rename leaves no temporary name (NotFound); the hard-link fallback can
    // leave it behind. The published file is complete either way, so a cleanup
    // failure is a warning, never a failed import.
    let cleanup_warning = remove_temp().map(|e| {
        format!(
            "Imported “{}”, but the temporary file “{}” could not be removed: {}",
            name, temp_name, e
        )
    });

    Ok(PublishedImport {
        path: dest,
        cleanup_warning,
    })
}

/// Android content-URI import of one item: validates the original display name
/// against the bytes actually read, before any EXIF parsing (`creation_date`),
/// naming or folder creation, then publishes those same bytes without replacing
/// existing files.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn import_content_bytes(
    resolved_name: &str,
    read: Result<Vec<u8>, String>,
    source_label: &str,
    creation_date: &dyn Fn(&str, &[u8]) -> DateTime<Utc>,
    destination_folder: &str,
    settings: &ImportSettings,
    sequence: usize,
    total: usize,
    ops: &ImportStagingOps,
) -> Result<PathBuf, String> {
    let source_bytes = validate_import_read(resolved_name, read, source_label)?;
    let source_name_path = Path::new(resolved_name);
    let file_date = creation_date(resolved_name, &source_bytes);

    let mut final_dest_folder = PathBuf::from(destination_folder);
    if settings.organize_by_date {
        let date_format_str = settings
            .date_folder_format
            .replace("YYYY", "%Y")
            .replace("MM", "%m")
            .replace("DD", "%d");
        let subfolder = file_date.format(&date_format_str).to_string();
        final_dest_folder.push(subfolder);
    }

    fs::create_dir_all(&final_dest_folder)
        .map_err(|e| format!("Failed to create destination folder: {}", e))?;

    let new_stem = generate_filename_from_template(
        &settings.filename_template,
        source_name_path,
        sequence,
        total,
        &file_date,
    );
    let extension = source_name_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let new_filename = format!("{}.{}", new_stem, extension);

    let published = publish_import_bytes(&final_dest_folder, &new_filename, &source_bytes, ops)?;
    if let Some(warning) = &published.cleanup_warning {
        log::warn!("{}", warning);
    }

    Ok(published.path)
}

/// Highest count JavaScript represents exactly (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE_JS_INTEGER: u64 = 9_007_199_254_740_991;
const IMPORT_DETAIL_MAX_CHARS: usize = 256;
const IMPORT_SUMMARY_MAX_CHARS: usize = 1024;
const IMPORT_SUMMARY_MAX_DETAILS: usize = 3;

/// Process-wide flag: at most one native import batch runs at a time.
static IMPORT_RUN_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Owns the single-active import flag; releases it on drop, including unwind.
struct ImportRunGuard {
    flag: &'static AtomicBool,
}

impl ImportRunGuard {
    fn try_acquire(flag: &'static AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| ImportRunGuard { flag })
    }
}

impl Drop for ImportRunGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

fn import_count_is_js_safe(count: usize) -> bool {
    u64::try_from(count).is_ok_and(|count| count <= MAX_SAFE_JS_INTEGER)
}

fn is_canonical_run_id(run_id: &str) -> bool {
    Uuid::parse_str(run_id).is_ok_and(|id| id.hyphenated().to_string() == run_id)
}

fn validate_import_request(
    source_paths: &[String],
    destination_folder: &str,
    run_id: &str,
) -> Result<(), String> {
    if source_paths.is_empty() {
        return Err("No files were selected for import.".to_string());
    }
    if destination_folder.trim().is_empty() {
        return Err("No destination folder was selected for import.".to_string());
    }
    if !is_canonical_run_id(run_id) {
        return Err("The import request has an invalid run ID.".to_string());
    }
    if !import_count_is_js_safe(source_paths.len()) {
        return Err("Too many files were selected for import.".to_string());
    }
    Ok(())
}

/// Validates a request, then claims the single-active flag. Neither step emits
/// events or touches files.
fn admit_import_run(
    source_paths: &[String],
    destination_folder: &str,
    run_id: &str,
    flag: &'static AtomicBool,
) -> Result<ImportRunGuard, String> {
    validate_import_request(source_paths, destination_folder, run_id)?;
    ImportRunGuard::try_acquire(flag).ok_or_else(|| "An import is already running.".to_string())
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ImportStartPayload {
    run_id: String,
    total: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ImportProgressPayload {
    run_id: String,
    total: u64,
    current: u64,
    path: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ImportItemErrorPayload {
    run_id: String,
    total: u64,
    failed: u64,
    error_message: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ImportCompletePayload {
    run_id: String,
    total: u64,
    imported: u64,
    failed: u64,
    error_message: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ImportEvent {
    Start(ImportStartPayload),
    Progress(ImportProgressPayload),
    ItemError(ImportItemErrorPayload),
    Complete(ImportCompletePayload),
}

fn emit_import_event(app_handle: &AppHandle, event: ImportEvent) {
    let (name, result) = match &event {
        ImportEvent::Start(p) => ("import-start", app_handle.emit("import-start", p)),
        ImportEvent::Progress(p) => ("import-progress", app_handle.emit("import-progress", p)),
        ImportEvent::ItemError(p) => ("import-error", app_handle.emit("import-error", p)),
        ImportEvent::Complete(p) => ("import-complete", app_handle.emit("import-complete", p)),
    };
    // Transport failures are reported but never change the recorded outcomes.
    if let Err(e) = result {
        log::warn!("Could not emit {} for an import run: {}", name, e);
    }
}

/// Replaces each run of control characters with one space and keeps at most
/// `max_chars` Unicode scalar values.
fn bound_display_text(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut kept = 0;
    let mut in_control = false;
    for c in text.chars() {
        let c = if c.is_control() {
            if in_control {
                continue;
            }
            in_control = true;
            ' '
        } else {
            in_control = false;
            c
        };
        if kept == max_chars {
            break;
        }
        out.push(c);
        kept += 1;
    }
    out
}

/// Replaces every `content://` token (ASCII case-insensitive) with a placeholder.
fn redact_content_uris(text: &str) -> String {
    replace_content_uri_tokens(text, None)
}

/// URI punctuation is not a safe terminator: consume the entire non-whitespace
/// token, including any attached grant/query suffix or closing punctuation.
/// Substitute a label only for an exact whole source token, never its prefix.
fn replace_content_uri_tokens(text: &str, source_label: Option<(&str, &str)>) -> String {
    const SCHEME: &str = "content://";
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    while let Some(offset) = lower[rest..].find(SCHEME) {
        let start = rest + offset;
        out.push_str(&text[rest..start]);
        let end = text[start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |len| start + len);
        let replacement = source_label
            .filter(|(source, _)| *source == &text[start..end])
            .map_or("[content URI]", |(_, label)| label);
        out.push_str(replacement);
        rest = end;
    }
    out.push_str(&text[rest..]);
    out
}

/// A bounded display name for an item: the resolved URI name or local basename,
/// never a raw content URI; `Item N` when no usable name is available.
fn import_item_label(source: &str, resolved_name: Option<&str>, index: usize) -> String {
    let name = match resolved_name {
        Some(name) if redact_content_uris(name) != name => None,
        Some(name) => Path::new(name).file_name(),
        None if redact_content_uris(source) != source => None,
        None => Path::new(source.rsplit_once("?vc=").map_or(source, |(base, _)| base)).file_name(),
    };
    name.map(|name| bound_display_text(&name.to_string_lossy(), IMPORT_DETAIL_MAX_CHARS))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("Item {}", index + 1))
}

/// Retain both ends of a long basename, including its extension, while reserving
/// at least 190 of the 256 detail scalars for the failure cause and action.
fn import_detail_label(label: &str) -> String {
    const MAX_LABEL_CHARS: usize = 64;
    const SUFFIX_CHARS: usize = 16;
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label.to_string();
    }
    let prefix: String = label
        .chars()
        .take(MAX_LABEL_CHARS - SUFFIX_CHARS - 1)
        .collect();
    let suffix: String = label.chars().rev().take(SUFFIX_CHARS).collect();
    format!("{}…{}", prefix, suffix.chars().rev().collect::<String>())
}

/// `bounded label: reason`, compacting known repeated names before bounding the
/// reason. Redact URI tokens before replacing local source/name strings so an
/// exact source prefix cannot leave a grant suffix behind. Keep the full known
/// resolved basename until normalization is complete; `label` may already be
/// bounded for progress and is only a fallback for this internal context.
fn import_failure_detail(
    label: &str,
    source: &str,
    error: &str,
    resolved_name: Option<&str>,
) -> String {
    let label = resolved_name
        .filter(|name| redact_content_uris(name) == *name)
        .and_then(|name| Path::new(name).file_name())
        .map(|name| bound_display_text(&name.to_string_lossy(), usize::MAX))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| bound_display_text(&redact_content_uris(label), usize::MAX));
    let long_label = label.chars().count() > 64;
    let reason_label = if long_label {
        "the selected item"
    } else {
        label.as_str()
    };
    let redacted = replace_content_uri_tokens(error, Some((source, reason_label)));
    let mut reason = bound_display_text(&redacted, usize::MAX);
    if !source.is_empty() {
        let clean_source = bound_display_text(source, usize::MAX);
        if !clean_source.trim().is_empty() {
            reason = reason.replace(&clean_source, reason_label);
        }
        let basename = import_display_basename(source);
        let basename = bound_display_text(&basename, usize::MAX);
        if basename.chars().count() > 64 {
            reason = reason.replace(&basename, "the selected item");
        }
    }
    if long_label && !label.is_empty() {
        reason = reason.replace(&label, "the selected item");
    }
    let display_label = import_detail_label(&label);
    let reason_budget = IMPORT_DETAIL_MAX_CHARS - display_label.chars().count() - 2;
    format!(
        "{}: {}",
        display_label,
        bound_display_text(&reason, reason_budget)
    )
}

/// Empty when every item imported; otherwise a bounded summary naming up to the
/// first three failures.
fn import_summary(total: u64, imported: u64, failed: u64, details: &[String]) -> String {
    if failed == 0 {
        return String::new();
    }
    let mut summary = format!(
        "{} of {} imported; {} failed: {}",
        imported,
        total,
        failed,
        details.join("; ")
    );
    let further = failed.saturating_sub(details.len() as u64);
    if further > 0 {
        summary.push_str(&format!(" +{} further failures", further));
    }
    bound_display_text(&summary, IMPORT_SUMMARY_MAX_CHARS)
}

type ImportBatchItem<'a> = dyn FnMut(usize, &str, &mut Option<String>) -> Result<(), String> + 'a;

/// Runs every item once and reports one start, progress per attempt, an error
/// per failed item and exactly one completion built from the actual outcomes.
fn run_import_batch(
    run_id: &str,
    source_paths: &[String],
    item: &mut ImportBatchItem<'_>,
    emit: &mut dyn FnMut(ImportEvent),
) -> ImportCompletePayload {
    let total = source_paths.len() as u64;
    let mut imported: u64 = 0;
    let mut failed: u64 = 0;
    let mut details: Vec<String> = Vec::new();

    emit(ImportEvent::Start(ImportStartPayload {
        run_id: run_id.to_string(),
        total,
    }));

    for (i, source) in source_paths.iter().enumerate() {
        emit(ImportEvent::Progress(ImportProgressPayload {
            run_id: run_id.to_string(),
            total,
            current: i as u64,
            path: import_item_label(source, None, i),
        }));

        let mut display_name = None;
        match item(i, source, &mut display_name) {
            Ok(()) => imported += 1,
            Err(e) => {
                failed += 1;
                let label = import_item_label(source, display_name.as_deref(), i);
                let detail = import_failure_detail(&label, source, &e, display_name.as_deref());
                if details.len() < IMPORT_SUMMARY_MAX_DETAILS {
                    details.push(detail.clone());
                }
                emit(ImportEvent::ItemError(ImportItemErrorPayload {
                    run_id: run_id.to_string(),
                    total,
                    failed,
                    error_message: detail,
                }));
            }
        }
    }

    emit(ImportEvent::Progress(ImportProgressPayload {
        run_id: run_id.to_string(),
        total,
        current: total,
        path: String::new(),
    }));

    let complete = ImportCompletePayload {
        run_id: run_id.to_string(),
        total,
        imported,
        failed,
        error_message: import_summary(total, imported, failed, &details),
    };
    emit(ImportEvent::Complete(complete.clone()));
    complete
}

/// The real per-item operation used by `import_files`, including its existing
/// failure log line.
fn import_item_operation<'a>(
    destination_folder: &'a str,
    settings: &'a ImportSettings,
    total_files: usize,
) -> impl FnMut(usize, &str, &mut Option<String>) -> Result<(), String> + 'a {
    move |i, source_path_str, display_name| {
        let result = import_one_item(
            source_path_str,
            i,
            total_files,
            destination_folder,
            settings,
            display_name,
        );
        if let Err(e) = &result {
            eprintln!("Failed to import {}: {}", source_path_str, e);
        }
        result
    }
}

/// Imports one source item. This is the former per-item closure body of
/// `import_files`, moved unchanged apart from recording the URI display name
/// that the content-URI route already resolved.
#[cfg_attr(not(target_os = "android"), allow(unused_variables))]
fn import_one_item(
    source_path_str: &str,
    i: usize,
    total_files: usize,
    destination_folder: &str,
    settings: &ImportSettings,
    display_name: &mut Option<String>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    if is_android_content_uri(source_path_str) {
        let resolved_name = resolve_android_content_uri_name(source_path_str)?;
        *display_name = Some(resolved_name.clone());
        import_content_bytes(
            &resolved_name,
            read_android_content_uri(source_path_str),
            source_path_str,
            &exif_processing::get_creation_date_from_bytes,
            &destination_folder,
            &settings,
            i + 1,
            total_files,
            &ImportStagingOps::default(),
        )?;

        if settings.delete_after_import {
            log::info!(
                "Skipping delete_after_import for Android content URI source: {}",
                source_path_str
            );
        }

        return Ok(());
    }

    let (source_path, source_sidecar) = parse_virtual_path(source_path_str);
    if !source_path.exists() {
        return Err(format!("Source file not found: {}", source_path_str));
    }

    let file_date = exif_processing::get_creation_date_from_path(&source_path);

    let mut final_dest_folder = PathBuf::from(&destination_folder);
    if settings.organize_by_date {
        let date_format_str = settings
            .date_folder_format
            .replace("YYYY", "%Y")
            .replace("MM", "%m")
            .replace("DD", "%d");
        let subfolder = file_date.format(&date_format_str).to_string();
        final_dest_folder.push(subfolder);
    }

    fs::create_dir_all(&final_dest_folder)
        .map_err(|e| format!("Failed to create destination folder: {}", e))?;

    let new_stem = generate_filename_from_template(
        &settings.filename_template,
        &source_path,
        i + 1,
        total_files,
        &file_date,
    );
    let extension = source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let new_filename = format!("{}.{}", new_stem, extension);
    let dest_file_path = final_dest_folder.join(new_filename);

    if dest_file_path.exists() {
        return Err(format!(
            "File already exists at destination: {}",
            dest_file_path.display()
        ));
    }

    fs::copy(&source_path, &dest_file_path).map_err(|e| e.to_string())?;
    if source_sidecar.exists()
        && let Some(dest_str) = dest_file_path.to_str()
    {
        let (_, dest_sidecar) = parse_virtual_path(dest_str);
        fs::copy(&source_sidecar, &dest_sidecar).map_err(|e| e.to_string())?;
    }

    let mut source_rrexif_name = source_path.file_name().unwrap().to_os_string();
    source_rrexif_name.push(".rrexif");
    let source_rrexif = source_path.with_file_name(source_rrexif_name);

    if source_rrexif.exists() {
        let mut dest_rrexif_name = dest_file_path.file_name().unwrap().to_os_string();
        dest_rrexif_name.push(".rrexif");
        let dest_rrexif = dest_file_path.with_file_name(dest_rrexif_name);
        let _ = fs::copy(&source_rrexif, &dest_rrexif);
    }

    if settings.delete_after_import {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            if let Err(trash_error) = trash::delete(&source_path) {
                log::warn!(
                    "Failed to trash source file {}: {}. Deleting permanently.",
                    source_path.display(),
                    trash_error
                );
                fs::remove_file(&source_path).map_err(|e| e.to_string())?;
            }
            if source_sidecar.exists()
                && let Err(trash_error) = trash::delete(&source_sidecar)
            {
                log::warn!(
                    "Failed to trash source sidecar {}: {}. Deleting permanently.",
                    source_sidecar.display(),
                    trash_error
                );
                fs::remove_file(&source_sidecar).map_err(|e| e.to_string())?;
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            fs::remove_file(&source_path).map_err(|e| e.to_string())?;
            if source_sidecar.exists() {
                fs::remove_file(&source_sidecar).map_err(|e| e.to_string())?;
            }
            if source_rrexif.exists() {
                let _ = fs::remove_file(&source_rrexif);
            }
        }
    }

    Ok(())
}

/// Queues one import batch. Resolving only means the batch was admitted and
/// queued; outcomes arrive through run-tagged `import-*` events.
#[tauri::command]
pub async fn import_files(
    source_paths: Vec<String>,
    destination_folder: String,
    settings: ImportSettings,
    run_id: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    let guard = admit_import_run(
        &source_paths,
        &destination_folder,
        &run_id,
        &IMPORT_RUN_ACTIVE,
    )?;

    tauri::async_runtime::spawn_blocking(move || {
        // Held until the terminal event has been attempted.
        let _guard = guard;
        let mut item = import_item_operation(&destination_folder, &settings, source_paths.len());
        let mut emit = |event: ImportEvent| emit_import_event(&app_handle, event);
        run_import_batch(&run_id, &source_paths, &mut item, &mut emit);
    });

    Ok(())
}

pub fn generate_filename_from_template(
    template: &str,
    original_path: &std::path::Path,
    sequence: usize,
    total: usize,
    file_date: &DateTime<Utc>,
) -> String {
    let stem = original_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let sequence_str = format!(
        "{:0width$}",
        sequence,
        width = total.to_string().len().max(1)
    );
    let local_date = file_date.with_timezone(&chrono::Local);
    let stem_prefix = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    let filename_number = &stem[stem_prefix.len()..];

    let mut result = template.to_string();
    result = result.replace("{original_filename}", stem);
    result = result.replace("{original_filename_number}", filename_number);
    result = result.replace("{sequence}", &sequence_str);
    result = result.replace("{YYYY}", &local_date.format("%Y").to_string());
    result = result.replace("{MM}", &local_date.format("%m").to_string());
    result = result.replace("{DD}", &local_date.format("%d").to_string());
    result = result.replace("{hh}", &local_date.format("%H").to_string());
    result = result.replace("{mm}", &local_date.format("%M").to_string());

    result
}

#[tauri::command]
pub fn rename_files(
    paths: Vec<String>,
    name_template: String,
    app_handle: AppHandle,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut operations: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut final_new_paths = Vec::with_capacity(paths.len());
    let mut renames = HashMap::new();

    for (i, path_str) in paths.iter().enumerate() {
        let (original_path, _) = parse_virtual_path(path_str);
        if !original_path.exists() {
            return Err(format!("File not found: {}", path_str));
        }

        let parent = original_path
            .parent()
            .ok_or("Could not get parent directory")?;
        let extension = original_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let file_date = exif_processing::get_creation_date_from_path(&original_path);

        let new_stem = generate_filename_from_template(
            &name_template,
            &original_path,
            i + 1,
            paths.len(),
            &file_date,
        );
        let new_filename = format!("{}.{}", new_stem, extension);
        let new_path = parent.join(new_filename);

        if new_path.exists() && new_path != original_path {
            return Err(format!(
                "A file with the name {} already exists.",
                new_path.display()
            ));
        }

        if operations.iter().any(|(other_original, other_new)| {
            *other_new == new_path && *other_original != original_path
        }) {
            return Err(format!(
                "More than one file would be renamed to {}.",
                new_path.display()
            ));
        }

        operations.push((original_path, new_path));
    }

    let mut sidecar_operations: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (original_path, new_path) in &operations {
        let parent = original_path
            .parent()
            .ok_or("Could not get parent directory")?;
        let original_filename_str = original_path.file_name().unwrap().to_string_lossy();
        let new_filename_str = new_path.file_name().unwrap().to_string_lossy();

        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.filter_map(Result::ok) {
                let entry_path = entry.path();
                let entry_os_filename = entry.file_name();
                let entry_filename = entry_os_filename.to_string_lossy();

                if entry_filename.starts_with(&format!("{}.", original_filename_str))
                    && entry_filename.ends_with(".rrdata")
                {
                    let new_sidecar_filename =
                        entry_filename.replacen(&*original_filename_str, &new_filename_str, 1);
                    let new_sidecar_path = parent.join(new_sidecar_filename);
                    sidecar_operations.push((entry_path, new_sidecar_path));
                } else if entry_filename == format!("{}.rrdata", original_filename_str) {
                    let mut new_sidecar_name = new_path.file_name().unwrap().to_os_string();
                    new_sidecar_name.push(".rrdata");
                    let new_sidecar_path = new_path.with_file_name(new_sidecar_name);

                    sidecar_operations.push((entry_path, new_sidecar_path));
                }
            }
        }

        let mut old_rrexif_name = original_path.file_name().unwrap().to_os_string();
        old_rrexif_name.push(".rrexif");
        let old_rrexif = original_path.with_file_name(old_rrexif_name);

        if old_rrexif.exists() {
            let mut new_rrexif_name = new_path.file_name().unwrap().to_os_string();
            new_rrexif_name.push(".rrexif");
            let new_rrexif = new_path.with_file_name(new_rrexif_name);
            sidecar_operations.push((old_rrexif, new_rrexif));
        }
    }
    operations.extend(sidecar_operations);

    for (old_path, new_path) in operations {
        if let Err(e) = fs::rename(&old_path, &new_path) {
            log::warn!(
                "Failed to rename {} to {}: {}",
                old_path.display(),
                new_path.display(),
                e
            );
            continue;
        }

        let old_str = old_path.to_string_lossy().into_owned();
        let new_str = new_path.to_string_lossy().into_owned();

        renames.insert(old_str, new_str.clone());

        if is_supported_image_file(&new_path) {
            final_new_paths.push(new_str);
        }
    }

    sync_album_path_changes(&app_handle, Some(&renames), None, None);

    Ok(final_new_paths)
}

#[tauri::command]
pub fn create_virtual_copy(
    source_virtual_path: String,
    target_album_id: Option<String>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let (source_path, source_sidecar_path) = parse_virtual_path(&source_virtual_path);

    let new_copy_id = Uuid::new_v4().to_string()[..6].to_string();
    let new_virtual_path = format!("{}?vc={}", source_path.to_string_lossy(), new_copy_id);
    let (_, new_sidecar_path) = parse_virtual_path(&new_virtual_path);

    let default_json =
        serde_json::to_string_pretty(&ImageMetadata::default()).map_err(|e| e.to_string())?;

    {
        let _layout = crate::exif_processing::SIDECAR_LAYOUT_LOCK.lock().unwrap();

        if source_sidecar_path.exists() {
            fs::copy(&source_sidecar_path, &new_sidecar_path)
                .map_err(|e| format!("Failed to copy sidecar file: {}", e))?;
        } else {
            fs::write(&new_sidecar_path, &default_json).map_err(|e| e.to_string())?;
        }

        // The library hides the original when only virtual copy sidecars exist.
        let primary_sidecar_path = crate::exif_processing::get_primary_sidecar_path(&source_path);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&primary_sidecar_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(default_json.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }

    if let Some(album_id) = target_album_id {
        let _ = add_to_album(album_id, vec![new_virtual_path.clone()], app_handle);
    }

    Ok(new_virtual_path)
}

pub fn extract_xmp_rating(content: &str) -> Option<i8> {
    if let Some(idx) = content.find("xmp:Rating=\"") {
        let start = idx + 12;
        let end = content[start..].find('"').map(|i| start + i)?;
        return content[start..end].parse().ok();
    }
    if let Some(idx) = content.find("<xmp:Rating>") {
        let start = idx + 12;
        let end = content[start..].find('<').map(|i| start + i)?;
        return content[start..end].parse().ok();
    }
    None
}

const XMP_REJECTED_RATING: i8 = -1;

pub fn xmp_rating(metadata: &ImageMetadata) -> String {
    if metadata.flag == Some(ImageFlag::Reject) {
        XMP_REJECTED_RATING.to_string()
    } else {
        metadata.rating.to_string()
    }
}

pub fn xmp_label(tags: &[String]) -> Option<String> {
    tags.iter()
        .rev()
        .find_map(|t| t.strip_prefix(COLOR_TAG_PREFIX))
        .map(|color| {
            let mut c = color.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
}

pub fn xmp_subject_bag<S: AsRef<str>>(items: &[S]) -> String {
    let mut bag = String::from("<dc:subject>\n    <rdf:Bag>\n");
    for item in items {
        bag.push_str(&format!("     <rdf:li>{}</rdf:li>\n", item.as_ref()));
    }
    bag.push_str("    </rdf:Bag>\n   </dc:subject>");
    bag
}

pub fn extract_xmp_label(content: &str) -> Option<String> {
    if let Some(idx) = content.find("xmp:Label=\"") {
        let start = idx + 11;
        let end = content[start..].find('"').map(|i| start + i)?;
        return Some(content[start..end].to_string());
    }
    if let Some(idx) = content.find("<xmp:Label>") {
        let start = idx + 11;
        let end = content[start..].find('<').map(|i| start + i)?;
        return Some(content[start..end].to_string());
    }
    None
}

pub fn extract_xmp_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(start_idx) = content.find("<dc:subject>")
        && let Some(end_idx) = content[start_idx..].find("</dc:subject>")
    {
        let subject_block = &content[start_idx..start_idx + end_idx];
        let mut current_idx = 0;
        while let Some(li_start) = subject_block[current_idx..].find("<rdf:li>") {
            let val_start = current_idx + li_start + 8;
            if let Some(li_end) = subject_block[val_start..].find("</rdf:li>") {
                tags.push(subject_block[val_start..val_start + li_end].to_string());
                current_idx = val_start + li_end + 9;
            } else {
                break;
            }
        }
    }
    tags
}

pub fn resolve_xmp_path(image_path: &Path) -> Option<PathBuf> {
    let xmp_path = image_path.with_extension("xmp");
    let xmp_path_upper = image_path.with_extension("XMP");
    if xmp_path.exists() {
        Some(xmp_path)
    } else if xmp_path_upper.exists() {
        Some(xmp_path_upper)
    } else {
        None
    }
}

pub fn sync_metadata_from_xmp(source_path: &Path, metadata: &mut ImageMetadata) -> bool {
    let actual_xmp = resolve_xmp_path(source_path);

    let mut changed = false;

    if let Some(xmp_file) = actual_xmp
        && let Ok(content) = fs::read_to_string(&xmp_file)
    {
        let xmp_rating = extract_xmp_rating(&content);

        if xmp_rating == Some(XMP_REJECTED_RATING) && metadata.flag.is_none() {
            metadata.flag = Some(ImageFlag::Reject);
            changed = true;
        }

        if metadata.rating == 0
            && let Some(rating) = xmp_rating.and_then(|r| u8::try_from(r).ok())
            && rating != 0
        {
            metadata.rating = rating;
            if !metadata.adjustments.is_object() {
                metadata.adjustments = default_adjustments();
            }
            if let Some(obj) = metadata.adjustments.as_object_mut() {
                obj.insert("rating".to_string(), serde_json::json!(rating));
            }
            changed = true;
        }

        let xmp_label = extract_xmp_label(&content);
        let xmp_tags = extract_xmp_tags(&content);

        let mut current_tags = metadata.tags.clone().unwrap_or_default();
        let original_len = current_tags.len();
        let had_no_tags = metadata.tags.is_none();

        for tag in xmp_tags {
            if !current_tags.contains(&tag) {
                current_tags.push(tag);
            }
        }

        if let Some(label) = xmp_label {
            let label_tag = format!("{}{}", COLOR_TAG_PREFIX, label.to_lowercase());
            if !current_tags.contains(&label_tag) {
                current_tags.retain(|t| !t.starts_with(COLOR_TAG_PREFIX));
                current_tags.push(label_tag);
            }
        }

        if current_tags.len() != original_len || (had_no_tags && !current_tags.is_empty()) {
            metadata.tags = Some(current_tags);
            changed = true;
        }
    }
    changed
}

pub fn sync_metadata_to_xmp(source_path: &Path, metadata: &ImageMetadata, create_if_missing: bool) {
    let xmp_path = source_path.with_extension("xmp");
    let xmp_path_upper = source_path.with_extension("XMP");

    let mut actual_xmp = if xmp_path.exists() {
        Some(xmp_path.clone())
    } else if xmp_path_upper.exists() {
        Some(xmp_path_upper.clone())
    } else {
        None
    };

    if actual_xmp.is_none() {
        if !create_if_missing {
            return;
        }
        let skeleton = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="RapidRAW">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;
        if let Err(e) = fs::write(&xmp_path, skeleton) {
            log::error!("Failed to create skeleton XMP: {}", e);
            return;
        }
        actual_xmp = Some(xmp_path);
    }

    if let Some(xmp_file) = actual_xmp
        && let Ok(mut content) = fs::read_to_string(&xmp_file)
    {
        let rating_str = xmp_rating(metadata);
        let re_rating_attr = regex!(r#"xmp:Rating\s*=\s*"[^"]*""#);
        let re_rating_tag = regex!(r#"<xmp:Rating\s*>[^<]*</xmp:Rating>"#);

        if re_rating_attr.is_match(&content) {
            content = re_rating_attr
                .replace(&content, format!("xmp:Rating=\"{}\"", rating_str))
                .to_string();
        } else if re_rating_tag.is_match(&content) {
            content = re_rating_tag
                .replace(&content, format!("<xmp:Rating>{}</xmp:Rating>", rating_str))
                .to_string();
        } else if let Some(last_index) = content.rfind("</rdf:Description>") {
            let (start, end) = content.split_at(last_index);
            content = format!("{} <xmp:Rating>{}</xmp:Rating>\n{}", start, rating_str, end);
        }

        let current_tags = metadata.tags.clone().unwrap_or_default();
        let label = xmp_label(&current_tags);
        let normal_tags: Vec<String> = current_tags
            .into_iter()
            .filter(|t| !t.starts_with(COLOR_TAG_PREFIX))
            .collect();

        if let Some(lbl) = label {
            let re_label_attr = regex!(r#"xmp:Label\s*=\s*"[^"]*""#);
            let re_label_tag = regex!(r#"<xmp:Label\s*>[^<]*</xmp:Label>"#);

            if re_label_attr.is_match(&content) {
                content = re_label_attr
                    .replace(&content, format!("xmp:Label=\"{}\"", lbl))
                    .to_string();
            } else if re_label_tag.is_match(&content) {
                content = re_label_tag
                    .replace(&content, format!("<xmp:Label>{}</xmp:Label>", lbl))
                    .to_string();
            } else if let Some(last_index) = content.rfind("</rdf:Description>") {
                let (start, end) = content.split_at(last_index);
                content = format!("{} <xmp:Label>{}</xmp:Label>\n{}", start, lbl, end);
            }
        } else {
            let re_label_attr = regex!(r#"\s*xmp:Label\s*=\s*"[^"]*""#);
            let re_label_tag = regex!(r#"\s*<xmp:Label\s*>[^<]*</xmp:Label>"#);
            content = re_label_attr.replace_all(&content, "").to_string();
            content = re_label_tag.replace_all(&content, "").to_string();
        }

        let re_subject = regex!(r#"(?s)<dc:subject>\s*<rdf:Bag>.*?</rdf:Bag>\s*</dc:subject>"#);
        if normal_tags.is_empty() {
            content = re_subject.replace_all(&content, "").to_string();
        } else {
            let bag = xmp_subject_bag(&normal_tags);

            if re_subject.is_match(&content) {
                content = re_subject.replace(&content, bag).to_string();
            } else if let Some(last_index) = content.rfind("</rdf:Description>") {
                let (start, end) = content.split_at(last_index);
                content = format!("{} {}\n  {}", start, bag, end);
            }
        }

        let _ = fs::write(&xmp_file, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const RAF_JPEG_MESSAGE: &str = "has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.";

    /// A complete JPEG produced by the real encoder, not a marker literal.
    fn encoded_jpeg() -> Vec<u8> {
        let image = image::RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb([(x * 8) as u8, (y * 10) as u8, 128])
        });
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .unwrap();
        bytes
    }

    fn import_settings(template: &str, organize_by_date: bool) -> ImportSettings {
        ImportSettings {
            filename_template: template.to_string(),
            organize_by_date,
            date_folder_format: "YYYY/MM-DD".to_string(),
            delete_after_import: false,
        }
    }

    thread_local! {
        static DATE_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    /// Fixed date standing in for Android-only EXIF parsing; counts calls so
    /// tests can show rejected content never reaches it.
    fn fixed_date(_: &str, _: &[u8]) -> DateTime<Utc> {
        DATE_CALLS.with(|calls| calls.set(calls.get() + 1));
        DateTime::parse_from_rfc3339("2026-10-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn date_calls() -> usize {
        DATE_CALLS.with(Cell::get)
    }

    fn import(
        library: &Path,
        name: &str,
        read: Result<Vec<u8>, String>,
        settings: &ImportSettings,
        ops: &ImportStagingOps,
    ) -> Result<PathBuf, String> {
        import_content_bytes(
            name,
            read,
            "content://provider/item/1",
            &fixed_date,
            &library.to_string_lossy(),
            settings,
            1,
            1,
            ops,
        )
    }

    /// Every file under `dir`, including hidden temporary files.
    fn all_files(dir: &Path) -> Vec<PathBuf> {
        WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .collect()
    }

    fn failing_write(file: &mut fs::File, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        file.write_all(&bytes[..bytes.len() / 2])?;
        Err(std::io::Error::other("disk full (injected)"))
    }

    fn failing_remove(_: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("permission denied (injected)"))
    }

    #[test]
    fn jpeg_named_raf_is_not_imported_in_any_case_or_template() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let jpeg = encoded_jpeg();
        for name in ["DSCF5389.RAF", "dscf5389.raf", "DSCF5389.RaF"] {
            for settings in [
                import_settings("{original_filename}", false),
                import_settings("renamed_{sequence}", true),
            ] {
                let err = import(
                    &library,
                    name,
                    Ok(jpeg.clone()),
                    &settings,
                    &ImportStagingOps::default(),
                )
                .unwrap_err();
                assert_eq!(err, format!("“{name}” {RAF_JPEG_MESSAGE}"));
            }
        }
        // Rejected before EXIF parsing and folder creation: no final,
        // temporary or date folder.
        assert_eq!(date_calls(), 0);
        assert!(!library.exists());
        assert!(all_files(root.path()).is_empty());
    }

    #[test]
    fn same_jpeg_named_jpg_is_imported_byte_identical() {
        let root = tempfile::tempdir().unwrap();
        let jpeg = encoded_jpeg();
        for (organize, template) in [(false, "{original_filename}"), (true, "copy_{sequence}")] {
            let library = root.path().join(format!("library-{organize}"));
            let path = import(
                &library,
                "DSCF5389.jpg",
                Ok(jpeg.clone()),
                &import_settings(template, organize),
                &ImportStagingOps::default(),
            )
            .unwrap();
            assert_eq!(fs::read(&path).unwrap(), jpeg);
            assert_eq!(all_files(&library), vec![path]);
        }
    }

    #[test]
    fn empty_read_and_read_failure_are_distinct_and_write_nothing() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let settings = import_settings("{original_filename}", false);
        let ops = ImportStagingOps::default();

        let empty = import(&library, "x.RAF", Ok(Vec::new()), &settings, &ops).unwrap_err();
        assert_eq!(
            empty,
            "No image data was read for “x.RAF”. Select the original file and try again."
        );

        let failed = import(
            &library,
            "x.RAF",
            Err(
                "Failed to open InputStream for Android content URI: content://provider/item/1"
                    .to_string(),
            ),
            &settings,
            &ops,
        )
        .unwrap_err();
        assert_eq!(
            failed,
            "Could not read “x.RAF”: Failed to open InputStream for Android content URI: the selected item"
        );

        assert_eq!(date_calls(), 0);
        assert!(!library.exists());
    }

    #[test]
    fn existing_target_is_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("DSCF5389.jpg");
        fs::write(&existing, b"original").unwrap();

        let err = import(
            root.path(),
            "DSCF5389.jpg",
            Ok(encoded_jpeg()),
            &import_settings("{original_filename}", false),
            &ImportStagingOps::default(),
        )
        .unwrap_err();
        assert!(
            err.starts_with("File already exists at destination:"),
            "{err}"
        );
        assert_eq!(fs::read(&existing).unwrap(), b"original");
        assert_eq!(all_files(root.path()), vec![existing]);
    }

    #[test]
    fn collision_created_after_staging_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let raced = |dest: &Path| fs::write(dest, b"raced").unwrap();
        let ops = ImportStagingOps {
            before_publish: &raced,
            ..ImportStagingOps::default()
        };

        let err = import(
            root.path(),
            "DSCF5389.jpg",
            Ok(encoded_jpeg()),
            &import_settings("{original_filename}", false),
            &ops,
        )
        .unwrap_err();
        let dest = root.path().join("DSCF5389.jpg");
        assert_eq!(
            err,
            format!("File already exists at destination: {}", dest.display())
        );
        assert_eq!(fs::read(&dest).unwrap(), b"raced");
        assert_eq!(all_files(root.path()), vec![dest]);
    }

    #[test]
    fn write_failure_leaves_no_final_or_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let ops = ImportStagingOps {
            write: &failing_write,
            ..ImportStagingOps::default()
        };
        let err = import(
            root.path(),
            "DSCF5389.jpg",
            Ok(encoded_jpeg()),
            &import_settings("{original_filename}", false),
            &ops,
        )
        .unwrap_err();
        assert_eq!(err, "Could not write “DSCF5389.jpg”: disk full (injected)");
        assert!(all_files(root.path()).is_empty());
    }

    #[test]
    fn cleanup_failure_is_reported_after_the_primary_error() {
        let root = tempfile::tempdir().unwrap();
        let ops = ImportStagingOps {
            write: &failing_write,
            remove: &failing_remove,
            ..ImportStagingOps::default()
        };
        let err = import(
            root.path(),
            "DSCF5389.jpg",
            Ok(encoded_jpeg()),
            &import_settings("{original_filename}", false),
            &ops,
        )
        .unwrap_err();
        assert!(
            err.starts_with(
                "Could not write “DSCF5389.jpg”: disk full (injected) The temporary file “.rapidraw-import-"
            ),
            "{err}"
        );
        assert!(err.ends_with(".tmp” could not be removed: permission denied (injected)"));

        // The leftover is hidden and never listed as an image; no final file exists.
        let leftovers = all_files(root.path());
        assert_eq!(leftovers.len(), 1);
        assert!(!is_supported_image_file(&leftovers[0]));
        assert!(!root.path().join("DSCF5389.jpg").exists());
    }

    /// Simulates tempfile's hard-link fallback, which can leave the temporary name.
    fn link_without_unlink(temp: &Path, dest: &Path) -> std::io::Result<()> {
        fs::hard_link(temp, dest)
    }

    #[test]
    fn hard_link_fallback_leftover_is_removed_or_reported() {
        let jpeg = encoded_jpeg();

        let root = tempfile::tempdir().unwrap();
        let ops = ImportStagingOps {
            persist: &link_without_unlink,
            ..ImportStagingOps::default()
        };
        let published = publish_import_bytes(root.path(), "a.jpg", &jpeg, &ops).unwrap();
        assert!(published.cleanup_warning.is_none());
        assert_eq!(all_files(root.path()), vec![published.path.clone()]);
        assert_eq!(fs::read(&published.path).unwrap(), jpeg);

        let root = tempfile::tempdir().unwrap();
        let ops = ImportStagingOps {
            persist: &link_without_unlink,
            remove: &failing_remove,
            ..ImportStagingOps::default()
        };
        let published = publish_import_bytes(root.path(), "a.jpg", &jpeg, &ops).unwrap();
        let warning = published.cleanup_warning.unwrap();
        assert!(
            warning.starts_with("Imported “a.jpg”, but the temporary file “.rapidraw-import-"),
            "{warning}"
        );
        assert_eq!(fs::read(&published.path).unwrap(), jpeg);
    }

    /// Removes like production and records each result's error kind.
    fn recording_remove(
        calls: &std::cell::RefCell<Vec<Option<std::io::ErrorKind>>>,
    ) -> impl Fn(&Path) -> std::io::Result<()> + '_ {
        move |path| {
            let result = fs::remove_file(path);
            calls
                .borrow_mut()
                .push(result.as_ref().err().map(std::io::Error::kind));
            result
        }
    }

    #[test]
    fn renamed_temporary_reports_not_found_as_clean() {
        let root = tempfile::tempdir().unwrap();
        let jpeg = encoded_jpeg();
        let calls = std::cell::RefCell::new(Vec::new());
        let remove = recording_remove(&calls);
        let ops = ImportStagingOps {
            remove: &remove,
            ..ImportStagingOps::default()
        };

        let published = publish_import_bytes(root.path(), "a.jpg", &jpeg, &ops).unwrap();
        // The real no-replace persist renamed the temporary file away, so the
        // direct removal attempt saw NotFound and that is not a warning.
        assert_eq!(*calls.borrow(), vec![Some(std::io::ErrorKind::NotFound)]);
        assert!(published.cleanup_warning.is_none());
        assert_eq!(all_files(root.path()), vec![published.path.clone()]);
        assert_eq!(fs::read(&published.path).unwrap(), jpeg);
    }

    /// Restores a directory's permissions on drop, including on panic.
    #[cfg(unix)]
    struct RestorePermissions<'a>(&'a Path);

    #[cfg(unix)]
    impl Drop for RestorePermissions<'_> {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unknown_temporary_status_after_hard_link_publication_is_a_warning() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        fs::create_dir(&library).unwrap();
        let jpeg = encoded_jpeg();

        // Publishes by hard link, then makes the directory unsearchable so the
        // temporary file's status can no longer be read or changed.
        let _restore = RestorePermissions(&library);
        let lock_after_link = |temp: &Path, dest: &Path| {
            fs::hard_link(temp, dest)?;
            fs::set_permissions(dest.parent().unwrap(), fs::Permissions::from_mode(0o000))
        };
        let ops = ImportStagingOps {
            persist: &lock_after_link,
            ..ImportStagingOps::default()
        };
        let result = publish_import_bytes(&library, "a.jpg", &jpeg, &ops);
        fs::set_permissions(&library, fs::Permissions::from_mode(0o755)).unwrap();

        let published = result.expect("a complete published file is still a success");
        let warning = published
            .cleanup_warning
            .expect("unverified cleanup must warn");
        assert!(
            warning.starts_with("Imported “a.jpg”, but the temporary file “.rapidraw-import-"),
            "{warning}"
        );
        assert_eq!(fs::read(&published.path).unwrap(), jpeg);
        let files = all_files(&library);
        assert_eq!(files.len(), 2, "final file and the reported temporary file");
        assert!(files.contains(&published.path));
    }

    #[test]
    fn already_missing_temporary_is_not_a_cleanup_failure_after_an_error() {
        let root = tempfile::tempdir().unwrap();
        let remove_then_missing = |path: &Path| -> std::io::Result<()> {
            fs::remove_file(path)?;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        };
        let ops = ImportStagingOps {
            write: &failing_write,
            remove: &remove_then_missing,
            ..ImportStagingOps::default()
        };
        let err = publish_import_bytes(root.path(), "a.jpg", &encoded_jpeg(), &ops)
            .err()
            .unwrap();
        assert_eq!(err, "Could not write “a.jpg”: disk full (injected)");
        assert!(all_files(root.path()).is_empty());
    }

    #[test]
    fn unsupported_publication_fails_without_final_or_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let unsupported = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        };
        let ops = ImportStagingOps {
            persist: &unsupported,
            ..ImportStagingOps::default()
        };
        let err = publish_import_bytes(root.path(), "a.jpg", &encoded_jpeg(), &ops)
            .err()
            .unwrap();
        assert!(err.starts_with("Could not publish “a.jpg”: "), "{err}");
        assert!(!err.contains("temporary file"), "{err}");
        assert!(all_files(root.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_destination_fails_without_output() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let attempted = Cell::new(false);
        let write = |file: &mut fs::File, bytes: &[u8]| {
            attempted.set(true);
            write_and_sync(file, bytes)
        };
        let ops = ImportStagingOps {
            write: &write,
            ..ImportStagingOps::default()
        };
        let result = publish_import_bytes(root.path(), "a.jpg", &encoded_jpeg(), &ops);
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();

        let err = result.err().expect("read-only destination must fail");
        assert!(err.starts_with("Could not stage “a.jpg”:"), "{err}");
        assert!(!attempted.get());
        assert!(all_files(root.path()).is_empty());
    }

    fn env_path(var: &str) -> PathBuf {
        PathBuf::from(std::env::var(var).unwrap_or_else(|_| panic!("set {var}")))
    }

    fn assert_imported_byte_identical(source: &Path, names: &[&str]) {
        use sha2::{Digest, Sha256};
        let bytes = fs::read(source).unwrap();
        let before = hex::encode(Sha256::digest(&bytes));
        let root = tempfile::tempdir().unwrap();
        for name in names {
            let library = root.path().join(name.replace('.', "_"));
            let path = import(
                &library,
                name,
                Ok(bytes.clone()),
                &import_settings("{original_filename}", false),
                &ImportStagingOps::default(),
            )
            .unwrap();
            let after = hex::encode(Sha256::digest(fs::read(&path).unwrap()));
            assert_eq!(after, before, "{name}");
            println!("{name}: imported sha256 {after}");
        }
        assert_eq!(
            hex::encode(Sha256::digest(fs::read(source).unwrap())),
            before
        );
    }

    #[test]
    #[ignore = "needs RAPIDRAW_R2A_FIXTURE_DIR from scripts/integration-fixtures.py"]
    fn synthetic_dng_imports_byte_identical_including_raf_alias() {
        let dng = env_path("RAPIDRAW_R2A_FIXTURE_DIR").join("synthetic-camera.dng");
        assert_imported_byte_identical(&dng, &["synthetic-camera.dng", "synthetic-camera.RAF"]);
    }

    #[test]
    #[ignore = "needs RAPIDRAW_R2A_NATIVE_RAF pointing at a genuine RAF"]
    fn native_raf_imports_byte_identical() {
        assert_imported_byte_identical(&env_path("RAPIDRAW_R2A_NATIVE_RAF"), &["DSCF1893.RAF"]);
    }
}

#[cfg(test)]
mod r3_terminal_tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    const RUN_ID: &str = "3f2b8c1e-5d4a-4f6b-9a7c-2e1d0b9f8a76";

    fn settings() -> ImportSettings {
        ImportSettings {
            filename_template: "{original_filename}".to_string(),
            organize_by_date: false,
            date_folder_format: "YYYY/MM-DD".to_string(),
            delete_after_import: false,
        }
    }

    /// A fresh flag per test so tests never contend on the process-wide one.
    fn test_flag() -> &'static AtomicBool {
        Box::leak(Box::new(AtomicBool::new(false)))
    }

    /// Runs the real batch driver with the real per-item operation on local files.
    fn run_local_batch(
        sources: &[String],
        destination: &Path,
    ) -> (Vec<ImportEvent>, ImportCompletePayload) {
        let settings = settings();
        let destination = destination.to_string_lossy().into_owned();
        let mut item = import_item_operation(&destination, &settings, sources.len());
        let mut events = Vec::new();
        let complete =
            run_import_batch(RUN_ID, sources, &mut item, &mut |event| events.push(event));
        (events, complete)
    }

    fn write_source(dir: &Path, name: &str, bytes: &[u8]) -> String {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn missing_source(dir: &Path, name: &str) -> String {
        dir.join(name).to_string_lossy().into_owned()
    }

    fn progress_currents(events: &[ImportEvent]) -> Vec<u64> {
        events
            .iter()
            .filter_map(|e| match e {
                ImportEvent::Progress(p) => Some(p.current),
                _ => None,
            })
            .collect()
    }

    fn count_kind(events: &[ImportEvent], kind: fn(&ImportEvent) -> bool) -> usize {
        events.iter().filter(|e| kind(e)).count()
    }

    #[test]
    fn single_failure_ends_with_an_error_terminal() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let sources = vec![missing_source(root.path(), "missing.RAF")];

        let (events, complete) = run_local_batch(&sources, &library);

        let detail = "missing.RAF: Source file not found: missing.RAF".to_string();
        let expected_complete = ImportCompletePayload {
            run_id: RUN_ID.to_string(),
            total: 1,
            imported: 0,
            failed: 1,
            error_message: format!("0 of 1 imported; 1 failed: {detail}"),
        };
        assert_eq!(
            events,
            vec![
                ImportEvent::Start(ImportStartPayload {
                    run_id: RUN_ID.to_string(),
                    total: 1,
                }),
                ImportEvent::Progress(ImportProgressPayload {
                    run_id: RUN_ID.to_string(),
                    total: 1,
                    current: 0,
                    path: "missing.RAF".to_string(),
                }),
                ImportEvent::ItemError(ImportItemErrorPayload {
                    run_id: RUN_ID.to_string(),
                    total: 1,
                    failed: 1,
                    error_message: detail,
                }),
                ImportEvent::Progress(ImportProgressPayload {
                    run_id: RUN_ID.to_string(),
                    total: 1,
                    current: 1,
                    path: String::new(),
                }),
                ImportEvent::Complete(expected_complete.clone()),
            ]
        );
        assert_eq!(complete, expected_complete);
    }

    #[test]
    fn mixed_batch_counts_each_outcome_once_and_keeps_the_good_copy() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let good_bytes = b"not decoded by import; copied byte-for-byte".to_vec();
        let sources = vec![
            write_source(root.path(), "DSCF0001.JPG", &good_bytes),
            missing_source(root.path(), "missing.RAF"),
        ];

        let (events, complete) = run_local_batch(&sources, &library);

        assert_eq!(
            (complete.total, complete.imported, complete.failed),
            (2, 1, 1)
        );
        assert_eq!(
            complete.error_message,
            "1 of 2 imported; 1 failed: missing.RAF: Source file not found: missing.RAF"
        );
        assert_eq!(
            count_kind(&events, |e| matches!(e, ImportEvent::Start(_))),
            1
        );
        assert_eq!(
            count_kind(&events, |e| matches!(e, ImportEvent::ItemError(_))),
            1
        );
        assert_eq!(
            count_kind(&events, |e| matches!(e, ImportEvent::Complete(_))),
            1
        );
        assert!(matches!(events.last(), Some(ImportEvent::Complete(_))));
        assert_eq!(progress_currents(&events), vec![0, 1, 2]);

        assert_eq!(fs::read(library.join("DSCF0001.JPG")).unwrap(), good_bytes);
        assert_eq!(fs::read(&sources[0]).unwrap(), good_bytes);
    }

    #[test]
    fn all_good_batch_completes_with_an_empty_message() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let sources = vec![
            write_source(root.path(), "a.JPG", b"first"),
            write_source(root.path(), "b.RAF", b"second"),
        ];

        let (events, complete) = run_local_batch(&sources, &library);

        assert_eq!(
            complete,
            ImportCompletePayload {
                run_id: RUN_ID.to_string(),
                total: 2,
                imported: 2,
                failed: 0,
                error_message: String::new(),
            }
        );
        assert_eq!(
            count_kind(&events, |e| matches!(e, ImportEvent::ItemError(_))),
            0
        );
        assert_eq!(fs::read(library.join("a.JPG")).unwrap(), b"first");
        assert_eq!(fs::read(library.join("b.RAF")).unwrap(), b"second");
        // delete_after_import is false: sources remain.
        assert!(sources.iter().all(|s| Path::new(s).exists()));
    }

    #[test]
    fn existing_destination_is_a_counted_failure_and_left_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("a.JPG"), b"existing").unwrap();
        let sources = vec![write_source(root.path(), "a.JPG", b"incoming")];

        let (_, complete) = run_local_batch(&sources, &library);

        assert_eq!((complete.imported, complete.failed), (0, 1));
        assert!(
            complete.error_message.starts_with(
                "0 of 1 imported; 1 failed: a.JPG: File already exists at destination"
            ),
            "{}",
            complete.error_message
        );
        assert_eq!(fs::read(library.join("a.JPG")).unwrap(), b"existing");
    }

    #[test]
    fn many_failures_are_summarised_with_bounded_safe_details() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let long_name = format!("{}.RAF", "L".repeat(300));
        let sources = vec![
            "content://com.android.providers.media.documents/document/image%3A42".to_string(),
            missing_source(root.path(), "bad\u{7}\nname.RAF"),
            missing_source(root.path(), &long_name),
            missing_source(root.path(), "fourth.RAF"),
            missing_source(root.path(), "fifth.RAF"),
        ];

        let (events, complete) = run_local_batch(&sources, &library);

        assert_eq!(
            (complete.total, complete.imported, complete.failed),
            (5, 0, 5)
        );
        let message = &complete.error_message;
        assert!(
            message.starts_with(
                "0 of 5 imported; 5 failed: Item 1: Source file not found: Item 1; bad name.RAF: "
            ),
            "{message}"
        );
        assert!(message.ends_with(" +2 further failures"), "{message}");
        assert!(message.chars().count() <= IMPORT_SUMMARY_MAX_CHARS);
        assert!(!message.chars().any(char::is_control));
        assert!(!message.contains("fourth.RAF") && !message.contains("fifth.RAF"));

        let mut failed_counts = Vec::new();
        for event in &events {
            let json = match event {
                ImportEvent::Start(p) => serde_json::to_string(p),
                ImportEvent::Progress(p) => serde_json::to_string(p),
                ImportEvent::ItemError(p) => {
                    failed_counts.push(p.failed);
                    assert!(p.error_message.chars().count() <= IMPORT_DETAIL_MAX_CHARS);
                    assert!(!p.error_message.chars().any(char::is_control));
                    serde_json::to_string(p)
                }
                ImportEvent::Complete(p) => serde_json::to_string(p),
            }
            .unwrap();
            assert!(!json.contains("content://"), "{json}");
        }
        assert_eq!(failed_counts, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn resolved_uri_name_labels_failures_and_uri_text_is_redacted() {
        let sources = vec!["content://provider/item/7".to_string()];
        let mut item = |_: usize, _: &str, name: &mut Option<String>| -> Result<(), String> {
            *name = Some("DSCF5389.RAF".to_string());
            Err(
                "read failed for content://provider/item/7 via CONTENT://other/grant?x=1"
                    .to_string(),
            )
        };
        let mut events = Vec::new();
        let complete = run_import_batch(RUN_ID, &sources, &mut item, &mut |e| events.push(e));

        assert_eq!(
            complete.error_message,
            "0 of 1 imported; 1 failed: DSCF5389.RAF: read failed for DSCF5389.RAF via [content URI]"
        );
        // Before the attempt the URI name is unknown, so progress uses `Item N`.
        assert!(matches!(
            &events[1],
            ImportEvent::Progress(p) if p.path == "Item 1"
        ));
    }

    #[test]
    fn wire_payloads_use_the_contract_field_names() {
        let keys = |value: serde_json::Value| -> Vec<String> {
            let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };
        let id = RUN_ID.to_string();
        assert_eq!(
            keys(
                serde_json::to_value(ImportStartPayload {
                    run_id: id.clone(),
                    total: 1
                })
                .unwrap()
            ),
            ["runId", "total"]
        );
        assert_eq!(
            keys(
                serde_json::to_value(ImportProgressPayload {
                    run_id: id.clone(),
                    total: 1,
                    current: 0,
                    path: String::new()
                })
                .unwrap()
            ),
            ["current", "path", "runId", "total"]
        );
        assert_eq!(
            keys(
                serde_json::to_value(ImportItemErrorPayload {
                    run_id: id.clone(),
                    total: 1,
                    failed: 1,
                    error_message: "x".to_string()
                })
                .unwrap()
            ),
            ["errorMessage", "failed", "runId", "total"]
        );
        assert_eq!(
            keys(
                serde_json::to_value(ImportCompletePayload {
                    run_id: id,
                    total: 1,
                    imported: 1,
                    failed: 0,
                    error_message: String::new()
                })
                .unwrap()
            ),
            ["errorMessage", "failed", "imported", "runId", "total"]
        );
    }

    #[test]
    fn invalid_requests_are_rejected_without_claiming_the_run() {
        let flag = test_flag();
        let one = vec!["/tmp/a.RAF".to_string()];
        let cases: Vec<(Vec<String>, &str, &str)> = vec![
            (Vec::new(), "/tmp/lib", RUN_ID),
            (one.clone(), "   ", RUN_ID),
            (one.clone(), "/tmp/lib", ""),
            (one.clone(), "/tmp/lib", "not-a-uuid"),
            (
                one.clone(),
                "/tmp/lib",
                "3F2B8C1E-5D4A-4F6B-9A7C-2E1D0B9F8A76",
            ),
            (one.clone(), "/tmp/lib", "3f2b8c1e5d4a4f6b9a7c2e1d0b9f8a76"),
            (
                one.clone(),
                "/tmp/lib",
                "{3f2b8c1e-5d4a-4f6b-9a7c-2e1d0b9f8a76}",
            ),
        ];
        for (sources, destination, run_id) in cases {
            assert!(admit_import_run(&sources, destination, run_id, flag).is_err());
            assert!(!flag.load(Ordering::SeqCst), "{run_id:?} {destination:?}");
        }

        let guard = admit_import_run(&one, "/tmp/lib", RUN_ID, flag).unwrap();
        assert!(flag.load(Ordering::SeqCst));
        drop(guard);
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn counts_beyond_javascript_safe_integers_are_rejected() {
        assert!(import_count_is_js_safe(1));
        if let Ok(max) = usize::try_from(MAX_SAFE_JS_INTEGER) {
            assert!(import_count_is_js_safe(max));
            assert!(!import_count_is_js_safe(max + 1));
        }
    }

    #[test]
    fn busy_run_rejects_admission_until_the_worker_releases() {
        let flag = test_flag();
        let sources = vec!["/tmp/a.RAF".to_string()];
        let guard = admit_import_run(&sources, "/tmp/lib", RUN_ID, flag).unwrap();

        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let worker_sources = sources.clone();
        let worker = std::thread::spawn(move || {
            // Mirrors import_files: the guard lives until the terminal attempt.
            let _guard = guard;
            let mut item = |_: usize, _: &str, _: &mut Option<String>| -> Result<(), String> {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            };
            run_import_batch(RUN_ID, &worker_sources, &mut item, &mut |_| {})
        });

        started_rx.recv().unwrap();
        let second = "0d6f1c52-7a3e-4b8f-9c21-5e4d3b2a1f00";
        assert_eq!(
            admit_import_run(&sources, "/tmp/lib", second, flag).err(),
            Some("An import is already running.".to_string())
        );

        release_tx.send(()).unwrap();
        let complete = worker.join().unwrap();
        assert_eq!((complete.imported, complete.failed), (1, 0));
        assert!(!flag.load(Ordering::SeqCst));
        assert!(admit_import_run(&sources, "/tmp/lib", second, flag).is_ok());
    }

    #[test]
    fn concurrent_admissions_accept_exactly_one() {
        let flag = test_flag();
        let threads = 8;
        let barrier = std::sync::Arc::new(Barrier::new(threads));
        let admitted = std::sync::Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let barrier = barrier.clone();
                let admitted = admitted.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let guard = ImportRunGuard::try_acquire(flag);
                    if guard.is_some() {
                        admitted.fetch_add(1, Ordering::SeqCst);
                    }
                    // Hold any guard until every thread has tried.
                    barrier.wait();
                    drop(guard);
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(admitted.load(Ordering::SeqCst), 1);
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn guard_is_released_when_the_worker_panics() {
        let flag = test_flag();
        let result = std::panic::catch_unwind(|| {
            let _guard = ImportRunGuard::try_acquire(flag).unwrap();
            panic!("injected worker panic");
        });
        assert!(result.is_err());
        assert!(!flag.load(Ordering::SeqCst));
        assert!(ImportRunGuard::try_acquire(flag).is_some());
    }

    #[test]
    fn display_text_helpers_bound_and_redact() {
        assert_eq!(
            redact_content_uris("see CONTENT://a/b?x=1, then content://c"),
            "see [content URI] then [content URI]"
        );
        assert_eq!(bound_display_text("a\u{0}\u{1b}b\tc", 10), "a b c");
        assert_eq!(bound_display_text("ééééé", 3), "ééé");
        assert_eq!(
            import_item_label("/x/y/photo.RAF?vc=abc", None, 0),
            "photo.RAF"
        );
        assert_eq!(import_item_label("content://p/doc/1", None, 4), "Item 5");
        assert_eq!(import_item_label("/x/..", None, 1), "Item 2");
    }

    #[test]
    fn long_names_keep_actual_causes_and_actions_even_when_repeated() {
        for name in [
            format!("{}.RAF", "L".repeat(251)),
            format!("{}.RAF", "界".repeat(251)),
        ] {
            for source in [
                format!("/selected/{name}"),
                "content://synthetic/document/7".to_string(),
            ] {
                let label = import_item_label(&source, Some(&name), 0);
                let read_error = validate_import_read(
                    &name,
                    Err(format!(
                        "{name}: Permission denied; select a readable original"
                    )),
                    &source,
                )
                .unwrap_err();
                // Real R1 signature validation, not a RAW decode or image route.
                let misnamed_error =
                    validate_import_read(&name, Ok(vec![0xff, 0xd8, 0xff]), &source).unwrap_err();
                for (error, required) in [
                    (
                        format!("Source file not found: {source}"),
                        vec!["Source file not found"],
                    ),
                    (
                        format!("File already exists at destination: /library/{name}"),
                        vec!["File already exists at destination"],
                    ),
                    (
                        read_error,
                        vec!["Permission denied", "select a readable original"],
                    ),
                    (
                        misnamed_error,
                        vec![
                            "has a JPEG signature",
                            "Select the original RAW file and try again.",
                        ],
                    ),
                ] {
                    let detail = import_failure_detail(&label, &source, &error, Some(&name));
                    assert!(
                        detail.chars().count() <= IMPORT_DETAIL_MAX_CHARS,
                        "{detail}"
                    );
                    assert!(!detail.chars().any(char::is_control));
                    assert!(detail.starts_with(&name.chars().take(8).collect::<String>()));
                    assert!(
                        detail.contains("…") && detail.contains(".RAF: "),
                        "{detail}"
                    );
                    for part in required {
                        assert!(detail.contains(part), "missing {part:?}: {detail}");
                    }
                }
            }
        }
    }

    #[test]
    fn real_driver_long_missing_and_collision_details_keep_reasons() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        fs::create_dir_all(&library).unwrap();
        let missing = format!("{}.RAF", "M".repeat(251));
        let collision = format!("{}.JPG", "界".repeat(83));
        let sources = vec![
            missing_source(root.path(), &missing),
            write_source(root.path(), &collision, b"incoming"),
        ];
        fs::write(library.join(&collision), b"existing").unwrap();
        let (events, complete) = run_local_batch(&sources, &library);
        assert_eq!(
            (complete.total, complete.imported, complete.failed),
            (2, 0, 2)
        );
        assert!(complete.error_message.contains("Source file not found"));
        assert!(
            complete
                .error_message
                .contains("File already exists at destination")
        );
        assert!(complete.error_message.chars().count() <= IMPORT_SUMMARY_MAX_CHARS);
        assert_eq!(progress_currents(&events), vec![0, 1, 2]);
        for event in events {
            if let ImportEvent::ItemError(error) = event {
                assert!(error.error_message.chars().count() <= IMPORT_DETAIL_MAX_CHARS);
            }
        }
        assert_eq!(fs::read(library.join(collision)).unwrap(), b"existing");
        assert_eq!(fs::read(&sources[1]).unwrap(), b"incoming");
    }

    #[test]
    fn every_uri_punctuation_variant_redacts_the_fake_grant_and_path_suffix() {
        for punctuation in [
            ",", ";", "(", ")", "[", "]", "{", "}", "<", ">", "\"", "'", "#", "@", ":",
        ] {
            for scheme in ["content://", "CONTENT://", "CoNtEnT://"] {
                let uri =
                    format!("{scheme}other/document/a{punctuation}PRIVATE_PATH?grant=TEST_ONLY");
                let reason = format!("Permission denied for {uri} select a readable original");
                let redacted = redact_content_uris(&reason);
                assert_eq!(
                    redacted,
                    "Permission denied for [content URI] select a readable original"
                );
                for private in ["TEST_ONLY", "grant=", "PRIVATE_PATH", "other/document"] {
                    assert!(!redacted.contains(private), "{redacted}");
                }
                let label = import_item_label("content://synthetic/item/1", Some(&uri), 0);
                assert_eq!(label, "Item 1");
            }
        }
    }

    #[test]
    fn display_payloads_redact_multiple_unicode_control_and_source_prefix_uri_suffixes() {
        let source = "content://synthetic/item/1".to_string();
        let sources = vec![source.clone()];
        let error = format!(
            "Permission\u{7}\n denied for {source},PRIVATE_PATH?grant=TEST_ONLY_A via CoNtEnT://other/写真[部]\u{0}?grant=TEST_ONLY_B; select a readable original"
        );
        let mut item = |_: usize, _: &str, name: &mut Option<String>| -> Result<(), String> {
            *name = Some("photo.RAF".to_string());
            Err(error.clone())
        };
        let mut events = Vec::new();
        let complete = run_import_batch(RUN_ID, &sources, &mut item, &mut |e| events.push(e));
        assert_eq!((complete.imported, complete.failed), (0, 1));
        for detail in events.iter().filter_map(|event| match event {
            ImportEvent::ItemError(error) => Some(error.error_message.as_str()),
            ImportEvent::Complete(complete) => Some(complete.error_message.as_str()),
            _ => None,
        }) {
            assert!(
                detail.contains("Permission") && detail.contains("denied"),
                "{detail}"
            );
            assert!(detail.contains("select a readable original"), "{detail}");
            assert_eq!(detail.matches("[content URI]").count(), 2, "{detail}");
            assert!(!detail.chars().any(char::is_control));
            for private in [
                "TEST_ONLY",
                "grant=",
                "PRIVATE_PATH",
                "other/",
                "写真",
                "部",
                "content://",
                "CONTENT://",
                "CoNtEnT://",
            ] {
                assert!(!detail.contains(private), "{detail}");
            }
        }
    }

    #[test]
    fn oversized_resolved_names_keep_actual_raf_error_through_the_batch_driver() {
        for (name, suffix) in [
            (
                format!("{}{}.RAF", "A".repeat(500), "B".repeat(500)),
                "BBBBBBBBBBBB.RAF",
            ),
            (
                format!("{}{}.RAF", "撮".repeat(300), "影".repeat(300)),
                "影影影影影影影影影影影影.RAF",
            ),
        ] {
            let sources = vec!["content://synthetic/item/7".to_string()];
            let actual_error = ImageContentIssue::RafHasJpegSignature.open_error_message(&name);
            let mut item =
                |_: usize, _: &str, resolved: &mut Option<String>| -> Result<(), String> {
                    *resolved = Some(name.clone());
                    Err(actual_error.clone())
                };
            let mut events = Vec::new();
            let complete = run_import_batch(RUN_ID, &sources, &mut item, &mut |e| events.push(e));
            assert_eq!(
                (complete.total, complete.imported, complete.failed),
                (1, 0, 1)
            );
            assert_eq!(progress_currents(&events), vec![0, 1]);
            let errors: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    ImportEvent::ItemError(error) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1);
            assert_eq!((errors[0].total, errors[0].failed), (1, 1));
            let detail = &errors[0].error_message;
            let (label, _) = detail.split_once(": ").expect("bounded label and reason");
            assert!(label.chars().count() <= 64, "{detail}");
            assert!(
                label.starts_with(&name.chars().take(8).collect::<String>()),
                "{detail}"
            );
            assert!(label.contains('…') && label.ends_with(suffix), "{detail}");
            assert!(detail.chars().count() <= IMPORT_DETAIL_MAX_CHARS);
            assert!(complete.error_message.chars().count() <= IMPORT_SUMMARY_MAX_CHARS);
            assert!(complete.error_message.contains(detail));
            for message in [detail.as_str(), complete.error_message.as_str()] {
                assert!(message.contains("has a JPEG signature"), "{message}");
                assert!(
                    message.contains("Select the original RAW file and try again."),
                    "{message}"
                );
                assert!(
                    !message.contains("content://") && !message.contains("synthetic/item"),
                    "{message}"
                );
                assert!(!message.chars().any(char::is_control));
            }
        }
    }

    #[test]
    fn oversized_resolved_read_name_repetitions_do_not_consume_the_reason_budget() {
        let name = format!("{}{}.RAF", "A".repeat(500), "B".repeat(500));
        let sources = vec!["content://synthetic/item/7,PRIVATE_PATH?grant=TEST_ONLY".to_string()];
        let actual_error = validate_import_read(
            &name,
            Err(format!(
                "{name}: Permission denied; select a readable original"
            )),
            &sources[0],
        )
        .unwrap_err();
        let mut item = |_: usize, _: &str, resolved: &mut Option<String>| -> Result<(), String> {
            *resolved = Some(name.clone());
            Err(actual_error.clone())
        };
        let mut events = Vec::new();
        let complete = run_import_batch(RUN_ID, &sources, &mut item, &mut |e| events.push(e));
        assert_eq!(
            (complete.total, complete.imported, complete.failed),
            (1, 0, 1)
        );
        let mut details = Vec::new();
        for event in &events {
            if let ImportEvent::ItemError(error) = event {
                assert!(error.error_message.chars().count() <= IMPORT_DETAIL_MAX_CHARS);
                details.push(error.error_message.as_str());
            }
        }
        assert_eq!(details.len(), 1);
        assert!(complete.error_message.chars().count() <= IMPORT_SUMMARY_MAX_CHARS);
        for message in [details[0], complete.error_message.as_str()] {
            assert!(message.contains("Permission denied"), "{message}");
            assert!(message.contains("select a readable original"), "{message}");
            assert!(message.contains("BBBBBBBBBBBB.RAF: "), "{message}");
            for private in [
                "TEST_ONLY",
                "grant=",
                "PRIVATE_PATH",
                "synthetic/item",
                "content://",
            ] {
                assert!(!message.contains(private), "{message}");
            }
        }
    }
}

#[cfg(test)]
mod ps1_preset_profile_tests {
    use super::*;
    use serde_json::json;

    const BASE_ID: &str = "4b80ed63b210e5738cde04430af1511c3e12e2bb53bf2471b4bb0750dceb87f3";
    const LOOK_ID: &str = "6e4b9a8c-2f61-4d3e-9b7a-1c0d5e8f2a34";

    fn preset(extra: Value) -> Value {
        let mut preset = json!({
            "id": "preset-1",
            "name": "Fuji base + Look",
            "adjustments": {
                "exposure": 0.25,
                "profile": { "base": BASE_ID, "look": LOOK_ID, "amount": 37 }
            },
            "includeMasks": false,
            "includeCropTransform": false,
            "presetType": "style"
        });
        for (key, value) in extra.as_object().unwrap() {
            preset[key] = value.clone();
        }
        json!({ "preset": preset })
    }

    /// Round-trips through the exact types `save_presets`/`load_presets` use.
    fn save_and_reload(items: &Value) -> Result<Value, String> {
        let saved: Vec<PresetItem> =
            serde_json::from_value(items.clone()).map_err(|e| e.to_string())?;
        let written = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
        let reloaded: Vec<PresetItem> =
            serde_json::from_str(&written).map_err(|e| e.to_string())?;
        serde_json::to_value(&reloaded).map_err(|e| e.to_string())
    }

    #[test]
    fn explicit_profile_capture_survives_save_and_reopen() {
        for flag in [true, false] {
            let items = json!([preset(json!({ "includeProfile": flag }))]);
            assert_eq!(
                save_and_reload(&items).unwrap(),
                items,
                "includeProfile={flag}"
            );
        }
    }

    #[test]
    fn legacy_preset_without_flag_stays_unchanged() {
        let items = json!([preset(json!({}))]);
        let reloaded = save_and_reload(&items).unwrap();
        assert_eq!(reloaded, items);
        assert!(reloaded[0]["preset"].get("includeProfile").is_none());
        assert_eq!(
            reloaded[0]["preset"]["adjustments"]["profile"],
            json!({ "base": BASE_ID, "look": LOOK_ID, "amount": 37 })
        );
    }

    #[test]
    fn folder_tree_and_preset_file_keep_each_flag() {
        let tree = json!([
            preset(json!({ "includeProfile": true })),
            {
                "folder": {
                    "id": "folder-1",
                    "name": "Fuji",
                    "children": [
                        preset(json!({ "id": "child-true", "includeProfile": true }))["preset"],
                        preset(json!({ "id": "child-false", "includeProfile": false }))["preset"],
                        preset(json!({ "id": "child-legacy" }))["preset"],
                    ]
                }
            }
        ]);
        assert_eq!(save_and_reload(&tree).unwrap(), tree);

        // Import/export files use PresetFile.
        let file = json!({ "presets": tree });
        let parsed: PresetFile = serde_json::from_value(file.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), file);
    }

    #[test]
    fn malformed_flag_is_rejected_not_dropped() {
        for bad in [json!("yes"), json!(1), json!({}), json!([true])] {
            let items = json!([preset(json!({ "includeProfile": bad }))]);
            assert!(
                save_and_reload(&items).is_err(),
                "accepted includeProfile={bad}"
            );
        }
        // JSON null is the same as an absent optional flag.
        let reloaded =
            save_and_reload(&json!([preset(json!({ "includeProfile": null }))])).unwrap();
        assert!(reloaded[0]["preset"].get("includeProfile").is_none());
    }

    #[test]
    fn converted_xmp_preset_does_not_claim_profile_capture() {
        let xmp = r#"<x:xmpmeta><rdf:Description crs:Exposure2012="+0.50" crs:Contrast2012="+10">
            <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Converted</rdf:li></rdf:Alt></crs:Name>
            </rdf:Description></x:xmpmeta>"#;
        let converted = crate::preset_converter::convert_xmp_to_preset(xmp).unwrap();
        let value = serde_json::to_value(PresetItem::Preset(converted)).unwrap();
        assert!(value["preset"].get("includeProfile").is_none(), "{value}");
        assert!(
            value["preset"]["adjustments"].get("profile").is_none(),
            "{value}"
        );
    }
}

#[cfg(test)]
mod thumbnail_snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_key_preserves_captured_metadata_after_sidecar_change() {
        let dir = std::env::temp_dir().join(format!("thumbnail-snapshot-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let source = dir.join("image.png");
        fs::write(&source, b"source identity fixture; no decoder invocation").unwrap();
        let path = source.to_str().unwrap();
        let (_, sidecar) = parse_virtual_path(path);
        let mut a = ImageMetadata::default();
        a.adjustments = serde_json::json!({"exposure":1.0});
        a.rating = 2;
        fs::write(&sidecar, serde_json::to_vec(&a).unwrap()).unwrap();
        let captured: Option<ImageMetadata> =
            Some(serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap());
        let settings = AppSettings::default();
        let key_a = thumbnail_metadata_key(&captured, path, &settings);
        let hash_a = compute_thumbnail_cache_hash(path, &key_a.2).unwrap();
        let mut c = a.clone();
        c.adjustments = serde_json::json!({"exposure":3.0});
        c.rating = 4;
        fs::write(&sidecar, serde_json::to_vec(&c).unwrap()).unwrap();
        let current = Some(serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap());
        let key_c = thumbnail_metadata_key(&current, path, &settings);
        assert_eq!(captured.as_ref().unwrap().adjustments["exposure"], 1.0);
        assert_eq!(key_a, thumbnail_metadata_key(&captured, path, &settings));
        assert_ne!(key_a, key_c);
        assert_ne!(
            hash_a,
            compute_thumbnail_cache_hash(path, &key_c.2).unwrap()
        );
        assert_eq!(
            hash_a,
            compute_thumbnail_cache_hash(path, &key_a.2).unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
