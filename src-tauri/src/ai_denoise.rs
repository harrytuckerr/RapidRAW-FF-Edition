use crate::app_state::AppState;
use crate::file_management::parse_virtual_path;
use crate::image_loader::ConsumerRequest;
use crate::relight::RelightTap;
use half::f16;
use image::{DynamicImage, GenericImageView, Rgb32FImage, imageops};
use rayon::prelude::*;
use std::borrow::Cow;

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

const MODEL_TAG: &str = "nind-v3";
const FILE_MAGIC: &[u8; 8] = b"RRDN0003";
const DISK_CACHE_LIMIT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MEMORY_CACHE_CAPACITY: usize = 2;

struct DenoisedBase {
    key: String,
    source: String,
    fingerprint: u64,
    image: Arc<Rgb32FImage>,
    runtime_key: Option<String>,
}

#[derive(Default)]
struct DenoiseCache {
    dir: Option<PathBuf>,
    entries: Vec<Arc<DenoisedBase>>,
    pending: Option<(String, Arc<ConsumerRequest>)>,
    clear_epoch: u64,
    next_job: u64,
    epoch: u64,
}

static CACHE: LazyLock<Mutex<DenoiseCache>> = LazyLock::new(Default::default);
static COMPLETED: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);
// Serializes disk publication against clear, never held across inference/awaits.
static DISK_IO: Mutex<()> = Mutex::new(());
// One model working set across editor and existing bounded file workers.
// Admission bounds the explicitly isolated Rayon execution context below.
static INFERENCE: Mutex<()> = Mutex::new(());

static INFERENCE_POOL: LazyLock<Result<rayon::ThreadPool, String>> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .thread_name(|_| "nind-private".into())
        .build()
        .map_err(|e| format!("Cannot create AI inference context: {e}"))
});

/// Never install another pool directly on a Rayon caller: it can execute outer
/// image jobs while waiting and recursively acquire this same admission lock.
/// One ordinary joined bridge calls one private single-worker context instead.
fn with_inference_lane<T: Send>(
    work: impl FnOnce() -> Result<T, String> + Send,
) -> Result<T, String> {
    let _admission = INFERENCE
        .lock()
        .map_err(|_| "AI inference admission poisoned")?;
    std::thread::scope(|scope| {
        let bridge = std::thread::Builder::new()
            .name("nind-bridge".into())
            .spawn_scoped(scope, move || {
                let pool = INFERENCE_POOL.as_ref().map_err(Clone::clone)?;
                pool.install(work)
            })
            .map_err(|e| format!("Cannot create AI inference bridge: {e}"))?;
        bridge
            .join()
            .map_err(|_| "AI inference lane panicked".to_string())?
    })
}

/// A completed immutable result, not a cache lookup promise. Contains no source
/// owner/back-reference; its caller retains the genuine request/file ownership.
#[derive(Clone)]
pub(crate) struct DenoiseLease {
    entry: Arc<DenoisedBase>,
    clear_epoch: u64,
}
impl DenoiseLease {
    fn check_epoch_at(&self, current: u64) -> Result<(), String> {
        if self.clear_epoch != current {
            return Err("AI denoise cache cleared".into());
        }
        Ok(())
    }
    pub(crate) fn check_epoch(&self) -> Result<(), String> {
        self.check_epoch_at(cache().clear_epoch)
    }
    pub(crate) fn check_runtime(&self, key: &str) -> Result<(), String> {
        self.check_epoch()?;
        if self
            .entry
            .runtime_key
            .as_deref()
            .is_some_and(|held| held != key)
        {
            return Err("AI denoise request changed".into());
        }
        Ok(())
    }
}

fn cache() -> std::sync::MutexGuard<'static, DenoiseCache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn initialize_cache_dir(cache_dir: PathBuf) {
    let dir = cache_dir.join("ai_denoise");
    let _ = fs::create_dir_all(&dir);
    cache().dir = Some(dir);
}

pub fn is_enabled(adjustments: &serde_json::Value) -> bool {
    crate::image_processing::is_effect_enabled(adjustments, "noiseReduction", "aiDenoiseEnabled")
}

pub fn is_requested(adjustments: &serde_json::Value) -> bool {
    is_enabled(adjustments)
        && (adjustments["aiDenoiseAmount"].as_f64().unwrap_or(100.0) as f32) > 0.0
}

pub fn cache_epoch() -> u64 {
    cache().epoch
}

impl DenoiseCache {
    fn get(&mut self, key: &str) -> Option<Arc<DenoisedBase>> {
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos);
        self.entries.push(Arc::clone(&entry));
        Some(entry)
    }

    fn insert(&mut self, entry: Arc<DenoisedBase>) -> Vec<Arc<DenoisedBase>> {
        let mut retired = Vec::new();
        if let Some(pos) = self.entries.iter().position(|e| e.key == entry.key) {
            retired.push(self.entries.remove(pos));
        }
        if self.entries.len() >= MEMORY_CACHE_CAPACITY {
            retired.push(self.entries.remove(0));
        }
        self.entries.push(entry);
        retired
    }
}

fn source_string(path: &str) -> String {
    parse_virtual_path(path).0.to_string_lossy().to_string()
}

// All input samples, format, dimensions, selection and the actual model identity.
// No sparse sample, path-only fallback or dimension-resizing compatibility.
fn entry_key(base: &DynamicImage, source: &str, adjustments: &serde_json::Value) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(MODEL_TAG.as_bytes());
    hasher.update(crate::ai_processing::DENOISE_SHA256.as_bytes());
    hasher.update(source.as_bytes());
    hasher.update(&base.width().to_le_bytes());
    hasher.update(&base.height().to_le_bytes());
    hasher.update(format!("{:?}", base.color()).as_bytes());
    let mut profile = adjustments["profile"].clone();
    profile.sort_all_objects();
    hasher.update(profile.to_string().as_bytes());
    hasher.update(base.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn base_fingerprint(key: &str) -> u64 {
    let digest = blake3::hash(key.as_bytes());
    u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("eight digest bytes"),
    )
}

fn entry_path(key: &str) -> Option<PathBuf> {
    cache()
        .dir
        .as_ref()
        .map(|dir| dir.join(format!("{key}.rrdn")))
}

struct EntryHeader {
    source: String,
    fingerprint: u64,
    width: u32,
    height: u32,
}

fn read_header(reader: &mut impl Read) -> Option<EntryHeader> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).ok()?;
    if &magic != FILE_MAGIC {
        return None;
    }
    let mut u32_buf = [0u8; 4];
    let mut u64_buf = [0u8; 8];
    reader.read_exact(&mut u32_buf).ok()?;
    let source_len = u32::from_le_bytes(u32_buf) as usize;
    if source_len > 64 * 1024 {
        return None;
    }
    let mut source = vec![0u8; source_len];
    reader.read_exact(&mut source).ok()?;
    reader.read_exact(&mut u64_buf).ok()?;
    let fingerprint = u64::from_le_bytes(u64_buf);
    reader.read_exact(&mut u32_buf).ok()?;
    let width = u32::from_le_bytes(u32_buf);
    reader.read_exact(&mut u32_buf).ok()?;
    let height = u32::from_le_bytes(u32_buf);
    Some(EntryHeader {
        source: String::from_utf8(source).ok()?,
        fingerprint,
        width,
        height,
    })
}

fn read_from_disk(key: &str, source: &str, dimensions: (u32, u32)) -> Option<DenoisedBase> {
    let path = entry_path(key)?;
    let mut reader = BufReader::new(File::open(&path).ok()?);
    let header = read_header(&mut reader)?;
    if header.source != source
        || (header.width, header.height) != dimensions
        || header.fingerprint != base_fingerprint(key)
        || dimensions.0 == 0
        || dimensions.1 == 0
    {
        return None;
    }
    let len = (header.width as usize)
        .checked_mul(header.height as usize)?
        .checked_mul(6)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, 0);
    reader.read_exact(&mut bytes).ok()?;
    let halves: &[f16] = bytemuck::try_cast_slice(&bytes).ok()?;
    let data: Vec<f32> = halves.iter().map(|v| v.to_f32()).collect();
    if !data.iter().all(|v| v.is_finite()) {
        return None;
    }
    let image = Rgb32FImage::from_raw(header.width, header.height, data)?;

    if let Ok(file) = File::options().write(true).open(&path) {
        let _ = file.set_modified(SystemTime::now());
    }

    Some(DenoisedBase {
        key: key.to_string(),
        source: header.source,
        fingerprint: header.fingerprint,
        image: Arc::new(image),
        runtime_key: None,
    })
}

fn write_to_disk(entry: &DenoisedBase) -> Result<(), String> {
    let path = entry_path(&entry.key).ok_or("AI denoise cache directory is not initialized")?;
    write_to_path(entry, &path)
}

fn write_to_path(entry: &DenoisedBase, path: &Path) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");

    let halves: Vec<f16> = entry
        .image
        .as_raw()
        .par_iter()
        .map(|v| f16::from_f32(*v))
        .collect();

    if halves.iter().any(|v| !v.is_finite()) {
        return Err("AI denoise FP16 cache cannot represent this HDR result".into());
    }
    let mut file = BufWriter::new(File::create(&tmp_path).map_err(|e| e.to_string())?);
    let source = entry.source.as_bytes();
    file.write_all(FILE_MAGIC)
        .and_then(|_| file.write_all(&(source.len() as u32).to_le_bytes()))
        .and_then(|_| file.write_all(source))
        .and_then(|_| file.write_all(&entry.fingerprint.to_le_bytes()))
        .and_then(|_| file.write_all(&entry.image.width().to_le_bytes()))
        .and_then(|_| file.write_all(&entry.image.height().to_le_bytes()))
        .and_then(|_| file.write_all(bytemuck::cast_slice(&halves)))
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())?;
    drop(file);

    fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    evict_disk_cache(&path);
    Ok(())
}

fn evict_disk_cache(keep: &Path) {
    let Some(read_dir) = keep.parent().and_then(|dir| fs::read_dir(dir).ok()) else {
        return;
    };

    let mut entries: Vec<(PathBuf, u64, SystemTime)> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rrdn"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((e.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();

    let mut total: u64 = entries.iter().map(|(_, len, _)| len).sum();
    entries.sort_by_key(|(_, _, modified)| *modified);

    for (path, len, _) in entries {
        if total <= DISK_CACHE_LIMIT_BYTES {
            break;
        }
        if path != keep && fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn load_entry(key: &str, source: &str, dimensions: (u32, u32)) -> Option<Arc<DenoisedBase>> {
    if let Some(entry) = cache().get(key) {
        return (entry.source == source
            && entry.fingerprint == base_fingerprint(key)
            && entry.image.dimensions() == dimensions)
            .then_some(entry);
    }
    let _disk = DISK_IO.lock().unwrap_or_else(|e| e.into_inner());
    let epoch = cache().clear_epoch;
    let entry = Arc::new(read_from_disk(key, source, dimensions)?);
    let retired = {
        let mut guard = cache();
        if guard.clear_epoch != epoch {
            return None;
        }
        guard.insert(Arc::clone(&entry))
    };
    drop(retired);
    Some(entry)
}

fn load_lease(
    key: &str,
    source: &str,
    dimensions: (u32, u32),
) -> Result<Option<DenoiseLease>, String> {
    let epoch = cache().clear_epoch;
    let entry = load_entry(key, source, dimensions);
    if cache().clear_epoch != epoch {
        return Err("AI denoise cache cleared".into());
    }
    Ok(entry.map(|entry| DenoiseLease {
        entry,
        clear_epoch: epoch,
    }))
}

pub fn apply_ai_denoise<'a>(
    base: &'a DynamicImage,
    path: &str,
    adjustments: &serde_json::Value,
) -> Result<Cow<'a, DynamicImage>, String> {
    apply_ai_denoise_with_lease(base, path, adjustments, None)
}

pub(crate) fn apply_ai_denoise_with_lease<'a>(
    base: &'a DynamicImage,
    path: &str,
    adjustments: &serde_json::Value,
    held: Option<&DenoiseLease>,
) -> Result<Cow<'a, DynamicImage>, String> {
    if !is_enabled(adjustments) {
        return Ok(Cow::Borrowed(base));
    }

    let amount =
        (adjustments["aiDenoiseAmount"].as_f64().unwrap_or(100.0) as f32 / 100.0).clamp(0.0, 1.0);
    let detail =
        (adjustments["aiDenoiseDetail"].as_f64().unwrap_or(0.0) as f32 / 100.0).clamp(0.0, 1.0);
    if amount <= 0.0 {
        return Ok(Cow::Borrowed(base));
    }

    let source = source_string(path);
    let key = entry_key(base, &source, adjustments);
    let loaded;
    let lease = match held {
        Some(lease) => lease,
        None => {
            loaded = load_lease(&key, &source, base.dimensions())?.ok_or(
                "AI denoise unavailable for this exact base; complete denoise before export",
            )?;
            &loaded
        }
    };
    lease.check_epoch()?;
    let entry = &lease.entry;
    if entry.key != key
        || entry.source != source
        || entry.fingerprint != base_fingerprint(&key)
        || entry.image.dimensions() != base.dimensions()
    {
        return Err("AI denoise base identity changed".into());
    }
    let denoised = entry.image.as_ref();

    let luma_keep = 1.0 - amount * (1.0 - detail);
    let chroma_keep = 1.0 - amount;

    let mut out = base.to_rgb32f();
    out.par_chunks_exact_mut(3)
        .zip(denoised.as_raw().par_chunks_exact(3))
        .for_each(|(px, dn)| {
            let r = [px[0] - dn[0], px[1] - dn[1], px[2] - dn[2]];
            let r_luma = 0.2126 * r[0] + 0.7152 * r[1] + 0.0722 * r[2];
            for c in 0..3 {
                px[c] = dn[c] + chroma_keep * (r[c] - r_luma) + luma_keep * r_luma;
            }
        });

    lease.check_epoch()?;
    Ok(Cow::Owned(DynamicImage::ImageRgb32F(out)))
}

fn restore_low_frequencies(denoised: &mut Rgb32FImage, source: &Rgb32FImage) {
    const FACTOR: usize = 64;
    let (w, h) = (source.width() as usize, source.height() as usize);
    let (sw, sh) = (w.div_ceil(FACTOR), h.div_ceil(FACTOR));
    if sw < 2 || sh < 2 {
        return;
    }

    let src = source.as_raw();
    let dn = denoised.as_raw();
    let planes: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let mut plane = vec![0.0f32; sw * sh];
            plane
                .par_chunks_exact_mut(sw)
                .enumerate()
                .for_each(|(sy, row)| {
                    let y_range = sy * FACTOR..((sy + 1) * FACTOR).min(h);
                    for (sx, out) in row.iter_mut().enumerate() {
                        let x_range = sx * FACTOR..((sx + 1) * FACTOR).min(w);
                        let mut acc = 0.0f32;
                        for y in y_range.clone() {
                            for x in x_range.clone() {
                                let i = (y * w + x) * 3 + c;
                                acc += src[i] - dn[i];
                            }
                        }
                        *out = acc / (y_range.len() * x_range.len()) as f32;
                    }
                });
            for _ in 0..3 {
                crate::lens_blur::dof_box_filter(&mut plane, sw, sh, 1, 3);
            }
            plane
        })
        .collect();

    denoised
        .par_chunks_exact_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let v = (y as f32 + 0.5) / h as f32;
            for x in 0..w {
                let tap = RelightTap::new((x as f32 + 0.5) / w as f32, v, sw, sh);
                for (c, plane) in planes.iter().enumerate() {
                    row[x * 3 + c] += tap.sample(plane);
                }
            }
        });
}

fn run_model(
    base: &DynamicImage,
    session: &Mutex<ort::session::Session>,
    on_progress: &dyn Fn(f32),
    check: &dyn Fn() -> Result<(), String>,
) -> Result<Rgb32FImage, String> {
    let source = base.to_rgb32f();
    let mut output =
        crate::ai_processing::run_ai_denoise_checked(&source, 0.5, session, on_progress, &|| {
            check().map_err(anyhow::Error::msg)
        })
        .map_err(|e| e.to_string())?
        .into_rgb32f();

    output
        .par_iter_mut()
        .zip(source.as_raw().par_iter())
        .for_each(|(dn, &src)| *dn += src - src.clamp(0.0, 1.0));

    check()?;
    restore_low_frequencies(&mut output, &source);
    check()?;
    if !output.as_raw().iter().all(|v| v.is_finite()) {
        return Err("Nonfinite AI denoise output".into());
    }
    Ok(output)
}

struct PendingGuard {
    request: Arc<ConsumerRequest>,
    epoch: u64,
    job: u64,
}
impl PendingGuard {
    fn check(&self) -> Result<(), String> {
        self.request.check()?;
        if cache().clear_epoch != self.epoch {
            return Err("AI denoise cache cleared".into());
        }
        Ok(())
    }
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        let retired = {
            let mut c = cache();
            if c.pending
                .as_ref()
                .is_some_and(|(_, r)| Arc::ptr_eq(r, &self.request))
            {
                c.pending.take()
            } else {
                None
            }
        };
        drop(retired); // source/asset retirement occurs outside the cache lock
        COMPLETED.notify_waiters();
    }
}

fn emit_progress(app: &tauri::AppHandle, held: &PendingGuard, progress: f32, key: &str) {
    // Unkeyed legacy commands report their own awaited result; never send a
    // path-only asynchronous event that could cross a same-path reopen.
    if let Some(open_key) = held.request.key() {
        if held.check().is_ok() {
            let _ = app.emit("ai-denoise-progress", serde_json::json!({
                "path":held.request.loaded().path, "openKey":open_key,
                "job":held.job.to_string(), "baseKey":key, "profile":held.request.adjustments()["profile"],
                "progress":progress
            }));
        }
    }
}

/// The existing preview command awaits a real matching result. One inference
/// reservation globally; contenders wait without developing/retaining full RGB.
/// A detached captured request survives ordinary preview-request replacement.
/// Different base intent cancels the active tile loop; it drains before reuse.
pub(crate) async fn ensure_generated(
    app: &tauri::AppHandle,
    request: &Arc<ConsumerRequest>,
) -> Result<(), String> {
    if !is_requested(request.adjustments()) {
        let active = cache().pending.as_ref().map(|(_, r)| Arc::clone(r));
        if let Some(active) = active {
            active.cancel();
        }
        return Ok(());
    }
    let intent = request.denoise_request_key()?;
    loop {
        request.check()?;
        let notified = COMPLETED.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let (ready, active) = {
            let c = cache();
            (
                c.entries
                    .iter()
                    .find(|e| e.runtime_key.as_deref() == Some(intent.as_str()))
                    .map(|entry| DenoiseLease {
                        entry: Arc::clone(entry),
                        clear_epoch: c.clear_epoch,
                    }),
                c.pending
                    .as_ref()
                    .map(|(key, r)| (key.clone(), Arc::clone(r))),
            )
        };
        if let Some(lease) = ready {
            return request.retain_denoise_lease(lease);
        }
        if let Some((key, active)) = active {
            if key != intent {
                active.cancel();
            }
            notified.await;
            continue;
        }
        let state = app.state::<AppState>();
        let dedicated = state.capture_consumer_request(
            request.key().as_ref(),
            Some(&request.loaded().path),
            Some(request.adjustments().clone()),
            request.settings().clone(),
        )?;
        request.check()?;
        if dedicated.denoise_request_key()? != intent {
            return Err("AI denoise source changed".into());
        }
        let held = {
            let mut c = cache();
            if c.pending.is_some() {
                continue;
            }
            c.next_job = c
                .next_job
                .checked_add(1)
                .ok_or("AI denoise job counter exhausted")?;
            c.pending = Some((intent.clone(), Arc::clone(&dedicated)));
            PendingGuard {
                request: dedicated,
                epoch: c.clear_epoch,
                job: c.next_job,
            }
        };
        emit_progress(app, &held, 0.0, &intent);
        let job = held.job;
        let result = async {
            let session_result = crate::ai_processing::get_or_init_denoise_model(
                app,
                &state.ai_state,
                &state.ai_init_lock,
            )
            .await;
            held.check()?;
            let session = session_result.map_err(|e| e.to_string())?;
            let app = app.clone();
            let runtime_key = intent.clone();
            tokio::task::spawn_blocking(move || {
                // Guard follows actual blocking lifetime, including unwind.
                let held = held;
                held.check()?;
                let state = app.state::<AppState>();
                with_inference_lane(|| {
                    held.check()?;
                    let base = held.request.denoise_input(&state)?;
                    held.check()?;
                    let lease = produce_base(
                        &base,
                        &held.request.loaded().path,
                        held.request.adjustments(),
                        &session,
                        held.epoch,
                        Some(runtime_key.clone()),
                        &|p| emit_progress(&app, &held, p, &runtime_key),
                        &|| held.check(),
                    )?;
                    emit_progress(&app, &held, 1.0, &runtime_key);
                    held.check()?;
                    Ok(lease)
                })
            })
            .await
            .map_err(|e| format!("AI denoise task failed: {e}"))?
        }
        .await;
        // Real caller cancellation takes precedence even on inference failure.
        request.check()?;
        if let (Err(error), Some(open_key)) = (&result, request.key()) {
            let _ = app.emit(
                "ai-denoise-progress",
                serde_json::json!({
                    "path":request.loaded().path, "openKey":open_key, "job":job.to_string(),
                    "baseKey":intent, "profile":request.adjustments()["profile"],
                    "progress":0.0, "error":error
                }),
            );
        }
        return result.and_then(|lease| request.retain_denoise_lease(lease));
    }
}

/// Shared exact-base production used by the editor and independent file jobs.
/// Caller owns INFERENCE and its real source/assets for this complete call.
fn produce_base(
    base: &DynamicImage,
    path: &str,
    adjustments: &serde_json::Value,
    session: &Mutex<ort::session::Session>,
    epoch: u64,
    runtime_key: Option<String>,
    progress: &dyn Fn(f32),
    check: &dyn Fn() -> Result<(), String>,
) -> Result<DenoiseLease, String> {
    let guarded = || {
        check()?;
        if cache().clear_epoch != epoch {
            return Err("AI denoise cache cleared".into());
        }
        Ok(())
    };
    guarded()?;
    let source = source_string(path);
    let key = entry_key(base, &source, adjustments);
    let image = if let Some(existing) = load_entry(&key, &source, base.dimensions()) {
        Arc::clone(&existing.image)
    } else {
        Arc::new(run_model(base, session, progress, &guarded)?)
    };
    guarded()?;
    let entry = Arc::new(DenoisedBase {
        fingerprint: base_fingerprint(&key),
        key,
        source,
        image,
        runtime_key,
    });
    {
        let _disk = DISK_IO.lock().unwrap_or_else(|e| e.into_inner());
        guarded()?;
        if let Err(error) = write_to_disk(&entry) {
            log::warn!("Failed to persist AI denoise cache: {error}");
        }
        if let Err(error) = guarded() {
            if let Some(path) = entry_path(&entry.key) {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
    }
    guarded()?;
    let retired = {
        let mut c = cache();
        if c.clear_epoch != epoch {
            return Err("AI denoise cache cleared".into());
        }
        let retired = c.insert(Arc::clone(&entry));
        c.epoch = c.epoch.wrapping_add(1);
        retired
    };
    drop(retired);
    guarded()?;
    Ok(DenoiseLease {
        entry,
        clear_epoch: epoch,
    })
}

/// Only called from the existing blocking file/export/thumbnail workers.
/// A cold or evicted saved edit regenerates here, without an editor-open
/// prerequisite or a second algorithm. Source/cancel checks bracket all stages.
pub(crate) fn ensure_worker_base(
    app: &tauri::AppHandle,
    base: &DynamicImage,
    path: &str,
    adjustments: &serde_json::Value,
    check: &(dyn Fn() -> Result<(), String> + Sync),
) -> Result<Option<DenoiseLease>, String> {
    check()?;
    if !is_requested(adjustments) {
        return Ok(None);
    }
    let source = source_string(path);
    let key = entry_key(base, &source, adjustments);
    if let Some(lease) = load_lease(&key, &source, base.dimensions())? {
        check()?;
        return Ok(Some(lease));
    }
    let epoch = cache().clear_epoch;
    with_inference_lane(|| {
        check()?;
        if cache().clear_epoch != epoch {
            return Err("AI denoise cache cleared".into());
        }
        if let Some(lease) = load_lease(&key, &source, base.dimensions())? {
            check()?;
            return Ok(Some(lease));
        }
        let state = app.state::<AppState>();
        let session_result =
            tauri::async_runtime::block_on(crate::ai_processing::get_or_init_denoise_model(
                app,
                &state.ai_state,
                &state.ai_init_lock,
            ));
        check()?;
        let session = session_result.map_err(|e| e.to_string())?;
        produce_base(
            base,
            path,
            adjustments,
            &session,
            epoch,
            None,
            &|_| {},
            check,
        )
        .map(Some)
    })
}

fn cache_files() -> Vec<(PathBuf, u64)> {
    let Some(read_dir) = cache().dir.clone().and_then(|dir| fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    read_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "rrdn" || ext == "tmp")
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.len())))
        .collect()
}

#[tauri::command]
pub fn get_ai_denoise_cache_size() -> u64 {
    cache_files().iter().map(|(_, len)| len).sum()
}

#[tauri::command]
pub fn clear_ai_denoise_cache() -> Result<(), String> {
    let (retired, active) = {
        let mut c = cache();
        c.clear_epoch = c.clear_epoch.wrapping_add(1);
        c.epoch = c.epoch.wrapping_add(1);
        (
            std::mem::take(&mut c.entries),
            c.pending.as_ref().map(|(_, r)| Arc::clone(r)),
        )
    };
    if let Some(active) = active {
        active.cancel();
    }
    drop(retired);
    let _disk = DISK_IO.lock().unwrap_or_else(|e| e.into_inner());
    for (path, _) in cache_files() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
    }
    Ok(())
}

#[cfg(test)]
mod adoption_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nd01_full_input_profile_and_cache_header_identity() {
        let base = DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(
            100,
            100,
            image::Rgb([0.2, 0.3, 0.4]),
        ));
        let settings = json!({"profile":{"base":"base-a","look":"look-a","amount":100}});
        let key = entry_key(&base, "fixture.raf", &settings);
        let mut edited = base.clone();
        edited.as_mut_rgb32f().unwrap().get_pixel_mut(0, 0).0[1] = 0.31; // Outside the former sparse stride.
        assert_ne!(key, entry_key(&edited, "fixture.raf", &settings));
        assert_ne!(
            key,
            entry_key(
                &base,
                "fixture.raf",
                &json!({"profile":{"base":"base-b","look":"look-a","amount":100}})
            )
        );
        assert_ne!(
            key,
            entry_key(&DynamicImage::new_rgb32f(50, 200), "fixture.raf", &settings)
        );
        assert_eq!(
            key,
            entry_key(
                &base,
                "fixture.raf",
                &json!({"exposure":3,"profile":{"amount":100,"look":"look-a","base":"base-a"}})
            )
        );
        let mut malformed = FILE_MAGIC.to_vec();
        malformed.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_header(&mut std::io::Cursor::new(malformed)).is_none());
    }

    #[test]
    fn nd02_missing_enabled_result_is_error_zero_is_identity() {
        let base =
            DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(2, 2, image::Rgb([-0.25, 0.5, 1.5])));
        for edit in [
            json!({}),
            json!({"aiDenoiseEnabled":true,"aiDenoiseAmount":0}),
            json!({"aiDenoiseEnabled":true,"activeTools":{"noiseReduction":false}}),
        ] {
            assert!(matches!(
                apply_ai_denoise(&base, "missing-nd02.raf", &edit).unwrap(),
                Cow::Borrowed(_)
            ));
        }
        assert!(
            apply_ai_denoise(
                &base,
                "missing-nd02.raf",
                &json!({"aiDenoiseEnabled":true,"aiDenoiseAmount":100})
            )
            .is_err()
        );
        let input = json!({"activeTools":{"colorMixer":true,"fog":true},"aiDenoiseEnabled":false});
        let before = crate::open_composition::preview_cpu_inputs(&input).unwrap();
        let mut edit = input.clone();
        edit["activeTools"]["colorMixer"] = json!(false);
        assert_eq!(
            before,
            crate::open_composition::preview_cpu_inputs(&edit).unwrap()
        );
        edit["activeTools"]["fog"] = json!(false);
        assert_ne!(
            before,
            crate::open_composition::preview_cpu_inputs(&edit).unwrap()
        );
        edit["activeTools"]["futureCpuTool"] = json!(true);
        assert!(crate::open_composition::preview_cpu_inputs(&edit).is_err());
    }
    #[test]
    fn nd04_nested_rayon_lane_serializes_and_recovers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for threads in [1, 2] {
            let outer = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let active = AtomicUsize::new(0);
            let peak = AtomicUsize::new(0);
            let completed = AtomicUsize::new(0);
            let results: Vec<_> = outer.install(|| {
                (0..2)
                    .into_par_iter()
                    .map(|_| {
                        with_inference_lane(|| {
                            assert!(outer.current_thread_index().is_none());
                            assert_eq!(rayon::current_num_threads(), 1);
                            let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(running, Ordering::SeqCst);
                            assert_eq!(running, 1);
                            let (sum, plane) = rayon::join(
                                || (0..8).into_par_iter().sum::<u32>(),
                                || {
                                    let mut plane = vec![1.0f32; 16];
                                    crate::lens_blur::dof_box_filter(&mut plane, 4, 4, 1, 1);
                                    plane
                                },
                            );
                            assert_eq!(sum, 28);
                            assert_eq!(plane, vec![1.0f32; 16]);
                            active.fetch_sub(1, Ordering::SeqCst);
                            completed.fetch_add(1, Ordering::SeqCst);
                            Ok(sum)
                        })
                        .unwrap()
                    })
                    .collect()
            });
            assert_eq!(results, vec![28, 28]);
            assert_eq!(completed.load(Ordering::SeqCst), 2);
            assert_eq!(active.load(Ordering::SeqCst), 0);
            assert_eq!(peak.load(Ordering::SeqCst), 1);
        }
        assert!(with_inference_lane::<()>(|| Err("controlled error".into())).is_err());
        assert!(with_inference_lane::<()>(|| panic!("controlled lane panic")).is_err());
        assert_eq!(with_inference_lane(|| Ok(7)).unwrap(), 7);
    }

    #[test]
    fn nd05_result_lease_survives_eviction_and_rejects_clear() {
        let base =
            DynamicImage::ImageRgb32F(Rgb32FImage::from_pixel(2, 2, image::Rgb([-0.25, 0.5, 1.5])));
        let edit = json!({"aiDenoiseEnabled":true,"aiDenoiseAmount":100,"aiDenoiseDetail":0});
        let source = source_string("lease-control.raf");
        let key = entry_key(&base, &source, &edit);
        let entry = Arc::new(DenoisedBase {
            key: key.clone(),
            source: source.clone(),
            fingerprint: base_fingerprint(&key),
            image: Arc::new(Rgb32FImage::from_pixel(
                2,
                2,
                image::Rgb([0.125, 0.25, 0.5]),
            )),
            runtime_key: None,
        });
        let weak = Arc::downgrade(&entry);
        let pixels_weak = Arc::downgrade(&entry.image);
        let epoch = cache().clear_epoch;
        let lease = DenoiseLease {
            entry: Arc::clone(&entry),
            clear_epoch: epoch,
        };
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("occupied.rrdn");
        fs::create_dir(&occupied).unwrap();
        assert!(write_to_path(&entry, &occupied).is_err()); // Real rename failure, no mock writer.
        let mut local = DenoiseCache::default();
        drop(local.insert(Arc::clone(&entry)));
        for key in ["other-b", "other-c"] {
            drop(local.insert(Arc::new(DenoisedBase {
                key: key.into(),
                source: source.clone(),
                fingerprint: 0,
                image: Arc::new(Rgb32FImage::new(1, 1)),
                runtime_key: None,
            })));
        }
        drop(entry);
        assert!(local.get(&key).is_none());
        assert!(weak.upgrade().is_some());
        assert!(pixels_weak.upgrade().is_some());
        let output =
            apply_ai_denoise_with_lease(&base, "lease-control.raf", &edit, Some(&lease)).unwrap();
        assert_eq!(
            output.as_rgb32f().unwrap().as_raw(),
            &vec![
                0.125, 0.25, 0.5, 0.125, 0.25, 0.5, 0.125, 0.25, 0.5, 0.125, 0.25, 0.5
            ]
        );
        let mut changed = base.clone();
        changed.as_mut_rgb32f().unwrap().get_pixel_mut(0, 0).0[0] = 0.0;
        assert!(
            apply_ai_denoise_with_lease(&changed, "lease-control.raf", &edit, Some(&lease))
                .is_err()
        );
        assert!(lease.check_epoch_at(epoch.wrapping_add(1)).is_err()); // Actual clear predicate, no global-cache mutation.
        let expired = DenoiseLease {
            entry: Arc::clone(&lease.entry),
            clear_epoch: epoch.wrapping_sub(1),
        };
        assert!(
            apply_ai_denoise_with_lease(&base, "lease-control.raf", &edit, Some(&expired)).is_err()
        );
        drop(expired);
        drop(lease);
        assert!(weak.upgrade().is_none());
        assert!(pixels_weak.upgrade().is_none());
    }
}
