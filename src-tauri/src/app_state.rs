use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

use image::{DynamicImage, GrayImage};
use serde::{Deserialize, Serialize};
use sysinfo::Disks;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use wgpu::{Texture, TextureView};

use crate::ai_processing::AiState;
use crate::as_shot::AsShot;
use crate::cache_utils::DecodedImageCache;
use crate::camera_tethering::CameraSession;
use crate::dcp::registry::ProfileRegistry;
use crate::gpu_processing::GpuProcessor;
use crate::image_processing::GpuContext;
use crate::launch_request::ExternalEditSession;
use crate::lens_correction::LensDatabase;
use crate::lut_processing::Lut;

pub struct AiTaskToken {
    cancelled: AtomicBool,
    notify: Notify,
}

impl AiTaskToken {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub async fn wait_for_cancel(&self) {
        let notified = self.notify.notified();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

impl Default for AiTaskToken {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AiTaskGuard<'a> {
    active_ai_tasks: &'a Mutex<HashMap<String, Arc<AiTaskToken>>>,
    task_id: String,
    pub token: Arc<AiTaskToken>,
}

impl<'a> AiTaskGuard<'a> {
    pub fn new(
        active_ai_tasks: &'a Mutex<HashMap<String, Arc<AiTaskToken>>>,
        task_id: String,
    ) -> Self {
        let token = Arc::new(AiTaskToken::new());
        active_ai_tasks
            .lock()
            .unwrap()
            .insert(task_id.clone(), Arc::clone(&token));
        Self {
            active_ai_tasks,
            task_id,
            token,
        }
    }
}

impl<'a> Drop for AiTaskGuard<'a> {
    fn drop(&mut self) {
        let mut tasks = self.active_ai_tasks.lock().unwrap();
        if let Some(existing) = tasks.get(&self.task_id)
            && Arc::ptr_eq(existing, &self.token)
        {
            tasks.remove(&self.task_id);
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
    pub fullscreen: bool,
}

#[derive(Clone)]
pub struct LoadedImage {
    pub path: String,
    pub image: Arc<DynamicImage>,
    pub is_raw: bool,
    /// Camera metadata for DCP profile rendering (§4.1, W4).
    pub raw_meta: Option<crate::raw_processing::RawImageMeta>,
    /// Raw file bytes, available only for RAW images. Retained so the image
    /// can be re-developed with DCP profile (Calibrate stripped) when a
    /// profile is selected after initial load (W4).
    pub raw_bytes: Option<Arc<Vec<u8>>>,
    /// Accurate pristine development retains source and its injected policy as
    /// one descriptor. Legacy loading never installs this inactive owner.
    pub(crate) prepared_source: Option<Arc<crate::image_loader::RetainedPreparedSource>>,
    pub(crate) open_identity: Option<crate::progressive_open::SourceIdentity>,
    pub as_shot: AsShot,
    pub raw_linear_mode: String,
    pub raw_highlight_compression: f32,
}

#[derive(Clone)]
pub struct CachedPreview {
    pub image: Arc<DynamicImage>,
    pub small_image: Arc<DynamicImage>,
    pub transform_hash: u64,
    pub scale: f32,
    pub unscaled_crop_offset: (f32, f32),
    pub preview_dim: u32,
    pub interactive_divisor: f32,
}

pub struct GpuImageCache {
    pub texture: Texture,
    pub texture_view: TextureView,
    pub gf_coeffs_view: TextureView,
    pub gf_dehaze_view: TextureView,
    pub is_raw: u32,
    pub width: u32,
    pub height: u32,
    pub transform_hash: u64,
}

pub struct GpuProcessorState {
    pub processor: GpuProcessor,
    pub width: u32,
    pub height: u32,
}

pub struct PreviewJob {
    pub(crate) thumbnail_activity: Arc<ThumbnailPreviewActivity>,
    pub(crate) request: Arc<crate::image_loader::ConsumerRequest>,
    pub adjustments: serde_json::Value,
    pub is_interactive: bool,
    pub target_resolution: Option<u32>,
    pub roi: Option<(f32, f32, f32, f32)>,
    pub request_analytics: bool,
    pub compute_waveform: bool,
    pub active_waveform_channel: Option<String>,
    pub responder: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
}

pub struct AnalyticsJob {
    pub(crate) request: Option<Arc<crate::image_loader::ConsumerRequest>>,
    pub path: String,
    pub image: Arc<DynamicImage>,
    pub compute_waveform: bool,
    pub active_waveform_channel: Option<String>,
}

pub struct AnalyticsConfig {
    pub(crate) request: Option<Arc<crate::image_loader::ConsumerRequest>>,
    pub path: String,
    pub compute_waveform: bool,
    pub active_waveform_channel: Option<String>,
    pub sender: Sender<AnalyticsJob>,
}

pub struct ThumbnailProgressTracker {
    pub total: usize,
    pub completed: usize,
}

// One bounded scheduler shared by library and durable-save thumbnail requests.
// Lock order is publication -> jobs. No decode, rendering, encoding or cache
// writes occur under either guard. The publication guard serializes the small
// durable sidecar transaction with event admission; it is never held while
// waiting for foreground activity. Saturation never waits for thumbnail capacity.
const THUMBNAIL_QUEUE_LIMIT: usize = 500;
const THUMBNAIL_EDIT_IDLE: std::time::Duration = std::time::Duration::from_millis(350);

#[derive(Default)]
struct ThumbnailJobs {
    paths: HashMap<String, ThumbnailEntry>,
    queue: VecDeque<String>,
    completed: usize,
    foreground_previews: usize,
    // None preserves immediate admission before any foreground preview exists.
    preview_quiet_until: Option<std::time::Instant>,
}
impl ThumbnailJobs {
    fn complete_path(&mut self, path: &str) {
        self.paths.remove(path);
        self.completed += 1;
        if self.paths.is_empty() {
            // A drained or cancelled batch cannot contribute to the next epoch.
            self.completed = 0;
        }
    }
}
struct ThumbnailEntry {
    generation: u64,
    active: Option<u64>,
    pending: bool,
    force: bool,
    ready_at: std::time::Instant,
}
pub(crate) struct ThumbnailPreviewActivity {
    manager: Arc<ThumbnailManager>,
}
impl Drop for ThumbnailPreviewActivity {
    fn drop(&mut self) {
        let mut jobs = self.manager.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.foreground_previews -= 1;
        if jobs.foreground_previews == 0 {
            jobs.preview_quiet_until = Some(std::time::Instant::now() + THUMBNAIL_EDIT_IDLE);
        }
        drop(jobs);
        self.manager.cvar.notify_all();
    }
}

pub(crate) struct ThumbnailWork {
    manager: Arc<ThumbnailManager>,
    pub(crate) path: String,
    generation: u64,
    pub(crate) force: bool,
}
impl Drop for ThumbnailWork {
    fn drop(&mut self) {
        let _publication = self
            .manager
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut jobs = self.manager.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = jobs.paths.get_mut(&self.path)
            && entry.active == Some(self.generation)
        {
            entry.active = None;
            if !entry.pending {
                jobs.complete_path(&self.path);
            }
        }
        drop(jobs);
        self.manager.cvar.notify_all();
    }
}

pub struct ThumbnailManager {
    jobs: Mutex<ThumbnailJobs>,
    publication: Mutex<()>,
    pub cvar: Condvar,
    pub rotational_disk: AtomicBool,
    pub io_gate: Mutex<()>,
}
impl ThumbnailManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            jobs: Mutex::new(ThumbnailJobs::default()),
            publication: Mutex::new(()),
            cvar: Condvar::new(),
            rotational_disk: AtomicBool::new(false),
            io_gate: Mutex::new(()),
        })
    }

    /// Preserve the synchronous sidecar transaction even at thumbnail capacity.
    /// Evict only queued speculative work; otherwise leave a new-path thumbnail
    /// stale until a later demand reads its updated metadata/cache hash. Existing
    /// paths always retain their latest dirty follow-up. There is no overflow list.
    pub(crate) fn edit<T>(
        &self,
        path: &str,
        persist: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let publication = self
            .publication
            .lock()
            .map_err(|_| "Thumbnail publication unavailable")?;
        let jobs = self
            .jobs
            .lock()
            .map_err(|_| "Thumbnail queue unavailable")?;
        let next = match jobs.paths.get(path) {
            Some(entry) => entry
                .generation
                .checked_add(1)
                .ok_or("Thumbnail generation exhausted")?,
            None => 1,
        };
        drop(jobs);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(persist));
        let mut deferred = false;
        if matches!(&result, Ok(Ok(_))) {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|_| "Thumbnail queue unavailable")?;
            if !jobs.paths.contains_key(path) && jobs.paths.len() >= THUMBNAIL_QUEUE_LIMIT {
                // Publication serializes admissions; active leases cannot retire
                // or be evicted while this small metadata transaction completes.
                let victim = jobs
                    .queue
                    .iter()
                    .find(|p| {
                        let e = &jobs.paths[*p];
                        e.pending && !e.force && e.active.is_none()
                    })
                    .cloned();
                if let Some(victim) = victim {
                    jobs.queue.retain(|p| p != &victim);
                    jobs.complete_path(&victim);
                }
            }
            if jobs.paths.contains_key(path) || jobs.paths.len() < THUMBNAIL_QUEUE_LIMIT {
                let entry = jobs
                    .paths
                    .entry(path.to_string())
                    .or_insert(ThumbnailEntry {
                        generation: 0,
                        active: None,
                        pending: false,
                        force: true,
                        ready_at: std::time::Instant::now(),
                    });
                entry.generation = next;
                entry.force = true;
                entry.pending = true;
                entry.ready_at = std::time::Instant::now() + THUMBNAIL_EDIT_IDLE;
                jobs.queue.retain(|p| p != path);
                jobs.queue.push_back(path.to_string());
            } else {
                deferred = true;
            }
        }
        drop(publication);
        self.cvar.notify_all();
        if deferred {
            log::info!(
                "Metadata saved; thumbnail refresh deferred to later demand because the bounded queue contains only forced or active work"
            );
        }
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// One token per logical foreground command. Worker clones keep it alive
    /// after a dropped command future; unrelated/older drops cannot clear peers.
    pub(crate) fn begin_preview(self: &Arc<Self>) -> Arc<ThumbnailPreviewActivity> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.foreground_previews = jobs
            .foreground_previews
            .checked_add(1)
            .expect("foreground preview count exhausted");
        Arc::new(ThumbnailPreviewActivity {
            manager: Arc::clone(self),
        })
    }

    /// Admission immediately before GPU setup/rendering. This releases the jobs
    /// guard while asleep and never holds the publication/metadata gate. Already
    /// admitted rendering may drain if a newer foreground command starts later.
    pub(crate) fn wait_for_render_admission(&self, work: &ThumbnailWork) -> bool {
        if !std::ptr::eq(self, Arc::as_ptr(&work.manager)) {
            return false;
        }
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let Some(entry) = jobs.paths.get(&work.path) else {
                return false;
            };
            if entry.generation != work.generation || entry.active != Some(work.generation) {
                return false;
            }
            if jobs.foreground_previews != 0 {
                jobs = self.cvar.wait(jobs).unwrap_or_else(|e| e.into_inner());
                continue;
            }
            let remaining = jobs
                .preview_quiet_until
                .map(|until| until.saturating_duration_since(std::time::Instant::now()))
                .unwrap_or_default();
            if remaining.is_zero() {
                return true;
            }
            jobs = self
                .cvar
                .wait_timeout(jobs, remaining)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    pub(crate) fn request_library(&self, paths: Vec<String>) {
        let _publication = self.publication.lock().unwrap();
        let mut jobs = self.jobs.lock().unwrap();
        if paths.is_empty() {
            // Navigation clears speculative work, never a durable-save refresh.
            let discard: Vec<_> = jobs
                .paths
                .iter()
                .filter(|(_, e)| !e.force)
                .map(|(p, _)| p.clone())
                .collect();
            for path in discard {
                jobs.queue.retain(|p| p != &path);
                let entry = jobs.paths.get_mut(&path).unwrap();
                entry.pending = false;
                // Active work is invalidated without releasing its worker lease.
                if entry.active.is_some() {
                    entry.generation = entry.generation.saturating_add(1);
                } else {
                    jobs.complete_path(&path);
                }
            }
        } else {
            for path in paths {
                if let Some(entry) = jobs.paths.get_mut(&path) {
                    // A cleared but still-running library request needs one
                    // follow-up. Existing active/dirty work already covers it.
                    if !entry.pending
                        && entry.active.is_some()
                        && entry.active != Some(entry.generation)
                    {
                        entry.pending = true;
                        jobs.queue.push_back(path);
                    }
                    continue;
                }
                if jobs.paths.len() >= THUMBNAIL_QUEUE_LIMIT {
                    break;
                }
                jobs.paths.insert(
                    path.clone(),
                    ThumbnailEntry {
                        generation: 1,
                        active: None,
                        pending: true,
                        force: false,
                        ready_at: std::time::Instant::now(),
                    },
                );
                jobs.queue.push_back(path);
            }
        }
        drop(jobs);
        self.cvar.notify_all();
    }

    pub(crate) fn next(self: &Arc<Self>) -> ThumbnailWork {
        let mut jobs = self.jobs.lock().unwrap();
        loop {
            if jobs.foreground_previews != 0 {
                jobs = self.cvar.wait(jobs).unwrap_or_else(|e| e.into_inner());
                continue;
            }
            let now = std::time::Instant::now();
            let remaining = jobs
                .preview_quiet_until
                .map(|until| until.saturating_duration_since(now))
                .unwrap_or_default();
            if !remaining.is_zero() {
                jobs = self
                    .cvar
                    .wait_timeout(jobs, remaining)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
                continue;
            }
            let index = jobs.queue.iter().rposition(|p| {
                let e = &jobs.paths[p];
                e.pending && e.active.is_none() && e.ready_at <= now
            });
            if let Some(index) = index {
                let path = jobs.queue.remove(index).unwrap();
                let e = jobs.paths.get_mut(&path).unwrap();
                e.pending = false;
                e.active = Some(e.generation);
                return ThumbnailWork {
                    manager: Arc::clone(self),
                    path,
                    generation: e.generation,
                    force: e.force,
                };
            }
            let delay = jobs
                .queue
                .iter()
                .filter_map(|p| {
                    let e = &jobs.paths[p];
                    (e.active.is_none() && e.pending)
                        .then_some(e.ready_at.saturating_duration_since(now))
                })
                .min();
            jobs = if let Some(delay) = delay {
                self.cvar.wait_timeout(jobs, delay).unwrap().0
            } else {
                self.cvar.wait(jobs).unwrap()
            };
        }
    }

    /// The small callback may snapshot metadata or emit an event, NEVER render.
    /// Saves and clear/invalidation are linearized against this publication.
    pub(crate) fn current<T>(&self, work: &ThumbnailWork, f: impl FnOnce() -> T) -> Option<T> {
        if !std::ptr::eq(self, Arc::as_ptr(&work.manager)) {
            return None;
        }
        let _publication = self.publication.lock().ok()?;
        let jobs = self.jobs.lock().ok()?;
        let e = jobs.paths.get(&work.path)?;
        if e.generation != work.generation || e.active != Some(work.generation) {
            return None;
        }
        drop(jobs);
        Some(f())
    }

    pub(crate) fn progress(&self) -> (usize, usize) {
        let jobs = self.jobs.lock().unwrap();
        if jobs.paths.is_empty() {
            (0, 0)
        } else {
            (jobs.completed, jobs.completed + jobs.paths.len())
        }
    }
}

pub struct PendingMetadata {
    pub virtual_path: String,
    pub image_path: PathBuf,
    pub sidecar_path: PathBuf,
}

pub struct MetadataManager {
    pub queue: Mutex<VecDeque<PendingMetadata>>,
    pub cvar: Condvar,
    pub pending: Mutex<HashSet<PathBuf>>,
}

impl MetadataManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            cvar: Condvar::new(),
            pending: Mutex::new(HashSet::new()),
        })
    }
}

pub type ThumbnailGeometryEntry = (u64, Arc<DynamicImage>, f32);
pub type TransformedImageCache = (u64, Arc<DynamicImage>, f32, (f32, f32));
pub type TransformedPreview = (Arc<DynamicImage>, f32, (f32, f32));

pub struct AppState {
    pub(crate) preview_request:
        Mutex<Option<std::sync::Weak<crate::image_loader::ConsumerRequest>>>,
    pub(crate) progressive_open: crate::progressive_open::OpenOwner,
    pub window_setup_complete: AtomicBool,
    pub gpu_crash_flag_path: Mutex<Option<PathBuf>>,
    pub original_image: Mutex<Option<LoadedImage>>,
    pub cached_preview: Mutex<Option<CachedPreview>>,
    pub gpu_context: Mutex<Option<GpuContext>>,
    pub gpu_image_cache: Mutex<Option<GpuImageCache>>,
    pub gpu_processor: Mutex<Option<GpuProcessorState>>,
    pub ai_state: Mutex<Option<AiState>>,
    pub ai_init_lock: TokioMutex<()>,
    pub active_ai_tasks: Mutex<HashMap<String, Arc<AiTaskToken>>>,
    pub export_task_token: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    pub hdr_result: Arc<Mutex<Option<DynamicImage>>>,
    pub panorama_result: Arc<Mutex<Option<DynamicImage>>>,
    pub focus_stack_result: Arc<Mutex<Option<DynamicImage>>>,
    pub denoise_result: Arc<Mutex<Option<DynamicImage>>>,
    pub indexing_task_handle: Mutex<Option<JoinHandle<()>>>,
    pub lut_cache: Mutex<HashMap<String, Arc<Lut>>>,
    pub initial_file_path: Mutex<Option<String>>,
    pub pending_edit_session: Mutex<Option<ExternalEditSession>>,
    pub thumbnail_cancellation_token: Arc<AtomicBool>,
    pub thumbnail_progress: Mutex<ThumbnailProgressTracker>,
    pub preview_worker_tx: Mutex<Option<Sender<PreviewJob>>>,
    pub analytics_worker_tx: Mutex<Option<Sender<AnalyticsJob>>>,
    pub mask_cache: Mutex<HashMap<u64, GrayImage>>,
    pub patch_cache: Mutex<HashMap<String, serde_json::Value>>,
    pub geometry_cache: Mutex<HashMap<u64, DynamicImage>>,
    pub thumbnail_geometry_cache: Mutex<HashMap<String, ThumbnailGeometryEntry>>,
    pub lens_db: Mutex<Option<Arc<LensDatabase>>>,
    pub load_image_generation: Arc<AtomicUsize>,
    pub full_warped_cache: Mutex<Option<(u64, Arc<DynamicImage>)>>,
    pub patched_cache: Mutex<Option<(u64, Arc<DynamicImage>)>>,
    pub patched_warped_cache: Mutex<Option<(u64, Arc<DynamicImage>)>>,
    pub working_cache: Mutex<Option<(u64, Arc<DynamicImage>)>>,
    pub effects_cache: Mutex<Option<(u64, Arc<DynamicImage>)>>,
    pub full_transformed_cache: Mutex<Option<TransformedImageCache>>,
    pub decoded_image_cache: Mutex<DecodedImageCache>,
    pub thumbnail_manager: Arc<ThumbnailManager>,
    pub metadata_manager: Arc<MetadataManager>,
    pub disks_cache: Mutex<Option<Disks>>,
    pub disks_cache_refreshing: AtomicBool,
    pub camera_session: Mutex<CameraSession>,
    pub profile_registry: Arc<ProfileRegistry>,
}

/// Displaced resources from a keyed commit. Call retire only after the open,
/// original and every cache guard have been released. No GPU operation here.
pub(crate) struct RetiredOpenResources {
    original: Option<LoadedImage>,
    decoded: Option<DecodedImageCache>,
    preview: Option<CachedPreview>,
    gpu: Option<GpuImageCache>,
    warped: Option<(u64, Arc<DynamicImage>)>,
    patched: Option<(u64, Arc<DynamicImage>)>,
    patched_warped: Option<(u64, Arc<DynamicImage>)>,
    working: Option<(u64, Arc<DynamicImage>)>,
    effects: Option<(u64, Arc<DynamicImage>)>,
    transformed: Option<TransformedImageCache>,
    masks: HashMap<u64, GrayImage>,
    patches: HashMap<String, serde_json::Value>,
    geometry: HashMap<u64, DynamicImage>,
    denoise: Option<DynamicImage>,
    hdr: Option<DynamicImage>,
    panorama: Option<DynamicImage>,
}
impl RetiredOpenResources {
    pub(crate) fn retire(self) {
        let Self {
            original,
            decoded,
            preview,
            gpu,
            warped,
            patched,
            patched_warped,
            working,
            effects,
            transformed,
            masks,
            patches,
            geometry,
            denoise,
            hdr,
            panorama,
        } = self;
        drop((
            original,
            decoded,
            preview,
            gpu,
            warped,
            patched,
            patched_warped,
            working,
            effects,
            transformed,
            masks,
            patches,
            geometry,
            denoise,
            hdr,
            panorama,
        ));
    }
}
impl AppState {
    /// Called within the keyed commit gate, or for a keyless clear only after
    /// original-image admission is closed and actual legacy leases have drained.
    /// Acquire every fallible lock before moving anything; poison cannot leave a partial install.
    /// On error the caller's image slot stays owned outside all lock guards.
    pub(crate) fn install_keyed_source(
        &self,
        image: &mut Option<LoadedImage>,
    ) -> Result<RetiredOpenResources, crate::progressive_open::OpenError> {
        if image.is_none() {
            return Err(crate::progressive_open::OpenError::CommitFailed);
        }
        self.replace_native_source(image, true)
    }
    pub(crate) fn clear_native_source(
        &self,
    ) -> Result<RetiredOpenResources, crate::progressive_open::OpenError> {
        self.replace_native_source(&mut None, false)
    }
    fn replace_native_source(
        &self,
        image: &mut Option<LoadedImage>,
        retire_decoded: bool,
    ) -> Result<RetiredOpenResources, crate::progressive_open::OpenError> {
        let acquire = || -> Result<_, crate::progressive_open::OpenError> {
            use crate::progressive_open::OpenError;
            let original = self
                .original_image
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let decoded = self
                .decoded_image_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let preview = self
                .cached_preview
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let gpu = self
                .gpu_image_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let warped = self
                .full_warped_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let patched = self.patched_cache.lock().map_err(|_| OpenError::Poisoned)?;
            let patched_warped = self
                .patched_warped_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let working = self.working_cache.lock().map_err(|_| OpenError::Poisoned)?;
            let effects = self.effects_cache.lock().map_err(|_| OpenError::Poisoned)?;
            let transformed = self
                .full_transformed_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let masks = self.mask_cache.lock().map_err(|_| OpenError::Poisoned)?;
            let patches = self.patch_cache.lock().map_err(|_| OpenError::Poisoned)?;
            let geometry = self
                .geometry_cache
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let denoise = self
                .denoise_result
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            let hdr = self.hdr_result.lock().map_err(|_| OpenError::Poisoned)?;
            let panorama = self
                .panorama_result
                .lock()
                .map_err(|_| OpenError::Poisoned)?;
            Ok((
                original,
                decoded,
                preview,
                gpu,
                warped,
                patched,
                patched_warped,
                working,
                effects,
                transformed,
                masks,
                patches,
                geometry,
                denoise,
                hdr,
                panorama,
            ))
        };
        let (
            mut original,
            mut decoded,
            mut preview,
            mut gpu,
            mut warped,
            mut patched,
            mut patched_warped,
            mut working,
            mut effects,
            mut transformed,
            mut masks,
            mut patches,
            mut geometry,
            mut denoise,
            mut hdr,
            mut panorama,
        ) = acquire()?;
        let image = image.take();
        Ok(RetiredOpenResources {
            original: std::mem::replace(&mut *original, image),
            decoded: if retire_decoded {
                Some(decoded.take_for_retirement())
            } else {
                None
            },
            preview: preview.take(),
            gpu: gpu.take(),
            warped: warped.take(),
            patched: patched.take(),
            patched_warped: patched_warped.take(),
            working: working.take(),
            effects: effects.take(),
            transformed: transformed.take(),
            masks: std::mem::take(&mut *masks),
            patches: std::mem::take(&mut *patches),
            geometry: std::mem::take(&mut *geometry),
            denoise: denoise.take(),
            hdr: hdr.take(),
            panorama: panorama.take(),
        })
    }
}

/// Whole counted source owner, never an image clone standing in for admission.
pub(crate) enum ConsumerSource {
    Accurate(crate::progressive_open::AccurateSnapshot),
    Legacy {
        loaded: LoadedImage,
        lease: crate::progressive_open::RenderLease,
        generation: (Arc<AtomicUsize>, usize),
    },
}
impl ConsumerSource {
    pub(crate) fn loaded(&self) -> &LoadedImage {
        match self {
            Self::Accurate(source) => source.loaded(),
            Self::Legacy { loaded, .. } => loaded,
        }
    }
    pub(crate) fn check(&self) -> Result<(), crate::progressive_open::OpenError> {
        match self {
            Self::Accurate(source) => source.is_current(),
            Self::Legacy {
                lease, generation, ..
            } => {
                lease.check()?;
                if generation.0.load(Ordering::SeqCst) != generation.1 {
                    Err(crate::progressive_open::OpenError::Stale)
                } else {
                    Ok(())
                }
            }
        }
    }
    pub(crate) fn key(&self) -> Option<crate::progressive_open::OpenKey> {
        match self {
            Self::Accurate(source) => Some(source.identity().open_key()),
            Self::Legacy { .. } => None,
        }
    }
}
impl AppState {
    pub(crate) fn capture_consumer_source(
        &self,
        expected: Option<&crate::progressive_open::OpenKey>,
        path: Option<&str>,
    ) -> Result<ConsumerSource, crate::progressive_open::OpenError> {
        capture_consumer_source_parts(
            &self.progressive_open,
            &self.original_image,
            &self.load_image_generation,
            expected,
            path,
        )
    }
}

impl AppState {
    pub(crate) fn capture_consumer_request(
        &self,
        expected: Option<&crate::progressive_open::OpenKey>,
        path: Option<&str>,
        adjustments: Option<serde_json::Value>,
        settings: crate::app_settings::AppSettings,
    ) -> Result<Arc<crate::image_loader::ConsumerRequest>, String> {
        let source = self
            .capture_consumer_source(expected, path)
            .map_err(|e| e.to_string())?;
        crate::image_loader::ConsumerRequest::capture(
            source,
            adjustments,
            settings,
            &self.profile_registry,
        )
        .map_err(|e| e.to_string())
    }
}

pub(crate) fn capture_consumer_source_parts(
    owner: &crate::progressive_open::OpenOwner,
    original: &Mutex<Option<LoadedImage>>,
    load_generation: &Arc<AtomicUsize>,
    expected: Option<&crate::progressive_open::OpenKey>,
    path: Option<&str>,
) -> Result<ConsumerSource, crate::progressive_open::OpenError> {
    use crate::progressive_open::OpenError;
    let source = if expected.is_some() {
        ConsumerSource::Accurate(owner.accurate_snapshot(expected)?)
    } else {
        let lease = owner.legacy_render_lease()?;
        let generation = (
            Arc::clone(&load_generation),
            load_generation.load(Ordering::SeqCst),
        );
        let loaded = original.lock().map_err(|_| OpenError::Poisoned)?.clone();
        lease.check()?;
        let loaded = loaded.ok_or(OpenError::Unavailable)?;
        if loaded.open_identity.is_some() || loaded.prepared_source.is_some() {
            return Err(OpenError::Stale);
        }
        ConsumerSource::Legacy {
            loaded,
            lease,
            generation,
        }
    };
    if path.is_some_and(|path| path != source.loaded().path) {
        return Err(OpenError::Stale);
    }
    source.check()?;
    Ok(source)
}

pub(crate) fn replace_preview_request(
    slot: &Mutex<Option<std::sync::Weak<crate::image_loader::ConsumerRequest>>>,
    request: &Arc<crate::image_loader::ConsumerRequest>,
) -> Result<(), String> {
    let previous = {
        let mut latest = slot.lock().map_err(|_| "Preview admission poisoned")?;
        latest
            .replace(Arc::downgrade(&request))
            .and_then(|old| old.upgrade())
    };
    if let Some(previous) = previous {
        previous.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod thumbnail_bounds_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct SavedFile(std::path::PathBuf);
    impl SavedFile {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("thumbnail-bounds-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path.join("image.rrdata"))
        }
        fn save(&self, manager: &ThumbnailManager, value: &str) {
            manager
                .edit("image", || {
                    std::fs::write(&self.0, value).map_err(|e| e.to_string())
                })
                .unwrap();
        }
    }
    impl Drop for SavedFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    #[test]
    fn autosave_abc_keeps_one_active_and_latest_followup() {
        let manager = ThumbnailManager::new();
        let file = SavedFile::new();
        file.save(&manager, "A");
        let (held_tx, held_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let worker_manager = Arc::clone(&manager);
        let worker = std::thread::spawn(move || {
            let work = worker_manager.next();
            assert!(work.force);
            held_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(
                worker_manager
                    .current(&work, || panic!("stale A published"))
                    .is_none()
            );
            drop(work);
        });
        held_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        file.save(&manager, "B");
        file.save(&manager, "C");
        manager.request_library(vec!["image".into()]);
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "C");
        {
            let jobs = manager.jobs.lock().unwrap();
            assert_eq!(jobs.paths.len(), 1);
            assert_eq!(jobs.queue.len(), 1);
            assert_eq!(jobs.paths["image"].active, Some(1));
            assert_eq!(jobs.paths["image"].generation, 3);
        }
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        let latest = manager.next();
        assert!(latest.force);
        assert_eq!(latest.generation, 3);
        assert_eq!(
            manager.current(&latest, || std::fs::read_to_string(&file.0).unwrap()),
            Some("C".into())
        );
        drop(latest);
        assert_eq!(manager.progress(), (0, 0));
        assert!(manager.jobs.lock().unwrap().queue.is_empty());
    }

    #[test]
    fn library_upgrade_preserves_force_and_clear_keeps_dirty() {
        let manager = ThumbnailManager::new();
        manager.request_library(vec!["image".into(), "image".into()]);
        let old = manager.next();
        assert!(!old.force);
        manager.edit("image", || Ok(())).unwrap();
        manager.request_library(vec![]);
        manager.request_library(vec!["image".into()]);
        assert!(manager.current(&old, || ()).is_none());
        assert_eq!(manager.jobs.lock().unwrap().queue.len(), 1);
        drop(old);
        let latest = manager.next();
        assert!(latest.force);
        assert!(manager.current(&latest, || ()).is_some());
        drop(latest);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn queue_capacity_backpressure_resumes_without_dropping_save() {
        let manager = ThumbnailManager::new();
        manager.request_library((0..1000).map(|n| n.to_string()).collect());
        assert_eq!(manager.jobs.lock().unwrap().paths.len(), 500);
        assert_eq!(manager.jobs.lock().unwrap().queue.len(), 500);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (persisted_tx, persisted_rx) = mpsc::sync_channel(1);
        let other = Arc::clone(&manager);
        let save = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            other
                .edit("saved", || {
                    persisted_tx.send(()).unwrap();
                    Ok(())
                })
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // New metadata-first policy: persistence completes without freeing a slot.
        persisted_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        save.join().unwrap();
        {
            let jobs = manager.jobs.lock().unwrap();
            assert_eq!(jobs.paths.len(), 500);
            assert_eq!(jobs.queue.len(), 500);
            assert!(!jobs.paths.contains_key("0"));
            assert!(jobs.paths["saved"].force);
            assert_eq!(jobs.paths.values().filter(|e| e.force).count(), 1);
            assert!(jobs.paths.values().all(|e| e.active.is_none()));
        }
        manager.request_library(vec![]);
        let work = manager.next();
        assert_eq!(work.path, "saved");
        assert!(work.force);
        drop(work);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn failed_save_does_not_replace_successful_dirty_work() {
        let manager = ThumbnailManager::new();
        let file = SavedFile::new();
        file.save(&manager, "A");
        let error = manager
            .edit::<()>("image", || Err("disk unavailable".into()))
            .unwrap_err();
        assert_eq!(error, "disk unavailable");
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "A");
        let work = manager.next();
        assert_eq!(work.generation, 1);
        assert!(manager.current(&work, || ()).is_some());
        drop(work);
        assert!(
            manager
                .edit::<()>("new", || Err("write failed".into()))
                .is_err()
        );
        assert_eq!(manager.progress(), (0, 0));
        // A failed empty-epoch reservation must not manufacture completed work.
        file.save(&manager, "B");
        assert_eq!(manager.progress(), (0, 1));
        assert!(
            manager
                .edit::<()>("other", || Err("write failed".into()))
                .is_err()
        );
        assert_eq!(manager.progress(), (0, 1));
        drop(manager.next());
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn dropped_or_panicked_worker_releases_path_and_wakes_followup() {
        let manager = ThumbnailManager::new();
        manager.edit("image", || Ok(())).unwrap();
        let work = manager.next();
        manager.edit("image", || Ok(())).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = work;
            panic!("controlled renderer failure");
        }));
        assert!(result.is_err());
        let latest = manager.next();
        assert_eq!(latest.generation, 2);
        drop(latest);
        assert_eq!(manager.progress(), (0, 0));
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), String> = manager.edit("new", || panic!("controlled persist panic"));
        }));
        assert!(failed.is_err());
        assert_eq!(manager.progress(), (0, 0));
        manager.edit("new", || Ok(())).unwrap();
        drop(manager.next());
    }

    #[test]
    fn library_clear_invalidates_active_publication_and_allows_requeue() {
        let manager = ThumbnailManager::new();
        manager.request_library(vec!["image".into()]);
        let work = manager.next();
        manager.request_library(vec![]);
        assert!(
            manager
                .current(&work, || panic!("cancelled event"))
                .is_none()
        );
        manager.request_library(vec!["image".into()]);
        assert_eq!(manager.jobs.lock().unwrap().paths.len(), 1);
        assert_eq!(manager.jobs.lock().unwrap().queue.len(), 1);
        drop(work);
        let next = manager.next();
        assert!(manager.current(&next, || ()).is_some());
        drop(next);
        assert_eq!(manager.progress(), (0, 0));
        // Both new admission routes start with zero prior completed work.
        manager.request_library(vec!["second-epoch".into()]);
        assert_eq!(manager.progress(), (0, 1));
        let second = manager.next();
        assert_eq!(manager.progress(), (0, 1));
        drop(second);
        assert_eq!(manager.progress(), (0, 0));
        manager.edit("third-epoch", || Ok(())).unwrap();
        assert_eq!(manager.progress(), (0, 1));
        drop(manager.next());
        assert_eq!(manager.progress(), (0, 0));
        // Clearing queued work also ends an epoch, including its completed count.
        manager.request_library(vec!["queued-a".into(), "queued-b".into()]);
        drop(manager.next());
        assert_eq!(manager.progress(), (1, 2));
        manager.request_library(vec![]);
        assert_eq!(manager.progress(), (0, 0));
        manager.request_library(vec!["after-clear".into()]);
        assert_eq!(manager.progress(), (0, 1));
        drop(manager.next());
    }

    #[test]
    fn saturated_library_eviction_preserves_active_lease() {
        let manager = ThumbnailManager::new();
        manager.request_library((0..500).map(|n| n.to_string()).collect());
        let active = manager.next();
        assert_eq!(active.path, "499");
        manager.edit("saved", || Ok(())).unwrap();
        {
            let jobs = manager.jobs.lock().unwrap();
            assert_eq!(jobs.paths.len(), 500);
            assert!(!jobs.paths.contains_key("0"));
            assert_eq!(jobs.paths["499"].active, Some(active.generation));
            assert!(jobs.paths["saved"].force);
            assert_eq!(
                jobs.paths.values().filter(|e| e.active.is_some()).count(),
                1
            );
        }
        assert!(manager.current(&active, || ()).is_some());
        manager.request_library(vec![]);
        drop(active);
        let saved = manager.next();
        assert_eq!(saved.path, "saved");
        assert!(saved.force);
        drop(saved);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn saturated_forced_and_active_queue_persists_without_overflow() {
        let manager = ThumbnailManager::new();
        let file = SavedFile::new();
        manager.request_library(vec!["active".into()]);
        let active = manager.next();
        for n in 0..499 {
            manager.edit(&format!("forced-{n}"), || Ok(())).unwrap();
        }
        // Failed persistence cannot evict a forced job or manufacture a refresh.
        assert!(
            manager
                .edit::<()>("failed", || Err("disk failed".into()))
                .is_err()
        );
        file.save(&manager, "C");
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "C");
        {
            let jobs = manager.jobs.lock().unwrap();
            assert_eq!(jobs.paths.len(), 500);
            assert_eq!(jobs.queue.len(), 499);
            assert!(!jobs.paths.contains_key("image"));
            assert!(!jobs.paths.contains_key("failed"));
            assert_eq!(jobs.paths["active"].active, Some(active.generation));
            assert_eq!(jobs.paths.values().filter(|e| e.force).count(), 499);
        }
        // The same active path must still accept and retain its latest follow-up.
        manager.edit("active", || Ok(())).unwrap();
        manager.edit("active", || Ok(())).unwrap();
        assert!(manager.current(&active, || ()).is_none());
        {
            let jobs = manager.jobs.lock().unwrap();
            assert_eq!(jobs.paths.len(), 500);
            assert_eq!(jobs.queue.len(), 500);
            assert_eq!(jobs.paths["active"].generation, 3);
            assert!(jobs.paths["active"].pending);
        }
        // Now every slot is forced (one is also active): a later save still
        // persists immediately and cannot create an overflow entry.
        file.save(&manager, "D");
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "D");
        assert_eq!(manager.jobs.lock().unwrap().paths.len(), 500);
        assert!(!manager.jobs.lock().unwrap().paths.contains_key("image"));
        // No overflow list: later normal demand can admit the saved path after
        // one real lease retires, then reads the already-durable latest sidecar.
        let retired = manager.next();
        assert!(retired.path.starts_with("forced-"));
        drop(retired);
        manager.request_library(vec!["image".into()]);
        let lazy = manager.next();
        assert_eq!(lazy.path, "image");
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "D");
        drop(lazy);
        drop(active);
        assert!(manager.jobs.lock().unwrap().paths["active"].pending);
        assert!(manager.jobs.lock().unwrap().paths.len() <= 500);
    }

    #[test]
    fn preview_tokens_defer_workers_and_last_drop_resumes_without_save() {
        let manager = ThumbnailManager::new();
        manager.request_library(vec!["image".into()]);
        let first = manager.begin_preview();
        let worker_retention = Arc::clone(&first);
        let second = manager.begin_preview();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (admitted_tx, admitted_rx) = mpsc::sync_channel(1);
        let other = Arc::clone(&manager);
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let work = other.next();
            assert!(other.wait_for_render_admission(&work));
            admitted_tx.send(work.path.clone()).unwrap();
            drop(work);
        });
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(first);
        drop(second);
        assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 1);
        assert!(admitted_rx.recv_timeout(Duration::from_millis(30)).is_err());
        // The queued/blocking worker clone, not the earlier command's Drop,
        // controls the final release. No new edit is used to wake the worker.
        drop(worker_retention);
        assert_eq!(
            admitted_rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            "image"
        );
        worker.join().unwrap();
        assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 0);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn claimed_thumbnail_wait_releases_locks_and_rejects_superseded_work() {
        let manager = ThumbnailManager::new();
        manager.request_library(vec!["image".into()]);
        let work = manager.next();
        let preview = manager.begin_preview();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (admitted_tx, admitted_rx) = mpsc::sync_channel(1);
        let other = Arc::clone(&manager);
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let admitted = other.wait_for_render_admission(&work);
            drop(work);
            admitted_tx.send(admitted).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(admitted_rx.recv_timeout(Duration::from_millis(30)).is_err());
        let file = SavedFile::new();
        file.save(&manager, "C");
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), "C");
        assert!(!admitted_rx.recv_timeout(Duration::from_secs(3)).unwrap());
        worker.join().unwrap();
        assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 1);
        drop(preview);
        let latest = manager.next();
        assert!(latest.force);
        assert!(manager.wait_for_render_admission(&latest));
        drop(latest);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn preview_activity_error_and_unwind_release_only_their_own_count() {
        fn fail(manager: &Arc<ThumbnailManager>) -> Result<(), &'static str> {
            let _activity = manager.begin_preview();
            Err("controlled preview error")
        }
        let manager = ThumbnailManager::new();
        let survivor = manager.begin_preview();
        assert_eq!(fail(&manager), Err("controlled preview error"));
        assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 1);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _activity = manager.begin_preview();
            panic!("controlled preview unwind");
        }));
        assert!(panic.is_err());
        assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 1);
        manager.request_library(vec!["image".into()]);
        drop(survivor);
        let work = manager.next();
        assert!(manager.wait_for_render_admission(&work));
        drop(work);
        assert_eq!(manager.progress(), (0, 0));
    }

    #[test]
    fn preview_quiet_period_blocks_back_to_back_gaps_for_claim_and_render() {
        for already_claimed in [false, true] {
            let manager = ThumbnailManager::new();
            manager.request_library(vec!["image".into()]);
            assert!(manager.jobs.lock().unwrap().preview_quiet_until.is_none());
            let held = if already_claimed {
                let work = manager.next();
                assert!(manager.wait_for_render_admission(&work));
                Some(work)
            } else {
                None
            };
            let first = manager.begin_preview();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let worker_barrier = Arc::clone(&barrier);
            let (attempt_tx, attempt_rx) = mpsc::sync_channel(1);
            let (done_tx, done_rx) = mpsc::sync_channel(1);
            let other = Arc::clone(&manager);
            let worker = std::thread::spawn(move || {
                worker_barrier.wait();
                attempt_tx.send(()).unwrap();
                let work = match held {
                    Some(work) => {
                        assert!(other.wait_for_render_admission(&work));
                        work
                    }
                    // Deliberately do NOT call the second gate in this branch:
                    // a faulty next() quiet gate cannot be hidden by it.
                    None => other.next(),
                };
                let admitted_at = std::time::Instant::now();
                done_tx.send(admitted_at).unwrap();
                drop(work);
            });
            barrier.wait();
            attempt_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
            drop(first);
            let first_until = manager.jobs.lock().unwrap().preview_quiet_until.unwrap();
            assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
            // An actual response-to-next-command gap shorter than the quiet
            // window must not start either a new claim or held rendering.
            let second = manager.begin_preview();
            assert!(std::time::Instant::now() < first_until);
            let old_remaining = first_until.saturating_duration_since(std::time::Instant::now());
            assert!(
                done_rx
                    .recv_timeout(old_remaining + Duration::from_millis(10))
                    .is_err()
            );
            assert_eq!(manager.jobs.lock().unwrap().foreground_previews, 1);
            drop(second);
            let last_until = manager.jobs.lock().unwrap().preview_quiet_until.unwrap();
            assert!(last_until > first_until);
            assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
            let admitted_at = done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(admitted_at >= last_until);
            worker.join().unwrap();
            assert_eq!(manager.progress(), (0, 0));
            // No edit, save, queue mutation or timer thread was needed to resume.
        }
    }
}
