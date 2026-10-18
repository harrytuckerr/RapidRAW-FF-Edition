//! Inactive keyed-open ownership. Root must wire commands, legacy/render leases,
//! display retirement and accurate consumer/cache identities before activation.
//! A blocking job is fenced until its outcome owner AND its supervisor finish.
use crate::app_settings::AppSettings;
use crate::app_state::LoadedImage;
use crate::image_loader::AccurateRawPolicy;
use crate::image_processing::ImageMetadata;
use crate::open_creative_snapshot::{
    CapturedCreativeAssets, CreativeCaptureError, capture_creative_assets,
};
use base64::Engine as _;
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, MutexGuard, Weak,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OpenKey {
    ticket: Uuid,
    generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyWire {
    ticket: String,
    generation: String,
}
impl<'de> Deserialize<'de> for OpenKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = KeyWire::deserialize(d)?;
        let ticket = canonical_ticket(&wire.ticket).map_err(serde::de::Error::custom)?;
        let generation =
            canonical_generation(&wire.generation).map_err(serde::de::Error::custom)?;
        Ok(Self { ticket, generation })
    }
}
impl Serialize for OpenKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("OpenKey", 2)?;
        out.serialize_field("ticket", &self.ticket.hyphenated().to_string())?;
        out.serialize_field("generation", &self.generation.to_string())?;
        out.end()
    }
}
fn canonical_ticket(s: &str) -> Result<Uuid, OpenError> {
    if s.len() != 36 {
        return Err(OpenError::InvalidKey);
    }
    let ticket = Uuid::parse_str(s).map_err(|_| OpenError::InvalidKey)?;
    if ticket.hyphenated().to_string() != s {
        return Err(OpenError::InvalidKey);
    }
    Ok(ticket)
}
fn canonical_generation(s: &str) -> Result<u64, OpenError> {
    if s.is_empty() || s.len() > 20 || (s.len() > 1 && s.starts_with('0')) {
        return Err(OpenError::InvalidKey);
    }
    s.bytes().try_fold(0u64, |n, c| {
        if !c.is_ascii_digit() {
            return Err(OpenError::InvalidKey);
        }
        n.checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(c - b'0')))
            .ok_or(OpenError::InvalidKey)
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum OpenError {
    InvalidKey,
    InvalidPath,
    Refining,
    Stale,
    Unavailable,
    AlreadyStarted,
    Cancelled,
    Poisoned,
    Exhausted,
    NoRuntime,
    WorkerFailed,
    SnapshotFailed,
    DevelopmentFailed,
    InvalidPlaceholder,
    CommitFailed,
}
impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OpenError {}
// Stable, payload-free diagnostics; public errors and IPC remain unchanged.
// Opaque errors deliberately expose stage only, never their formatted chain.
pub(crate) fn log_open_failure(stage: &'static str, cause: &'static str) {
    #[cfg(test)]
    OPEN_FAILURE_RECORDS.with(|records| {
        if let Some(records) = records.borrow_mut().as_mut() {
            records.push((stage, cause));
        }
    });
    if matches!(cause, "cancelled" | "stale") {
        log::debug!("[open-failure] stage={stage} cause={cause}");
    } else {
        log::warn!("[open-failure] stage={stage} cause={cause}");
    }
}

pub(crate) fn log_open_error(stage: &'static str, error: &OpenError) {
    let cause = match error {
        OpenError::Cancelled => "cancelled",
        OpenError::Stale => "stale",
        OpenError::Poisoned => "poisoned",
        OpenError::InvalidPlaceholder => "invalid_placeholder",
        OpenError::Unavailable => "unavailable",
        _ => "open_error",
    };
    log_open_failure(stage, cause);
}

#[cfg(test)]
thread_local! {
    static OPEN_FAILURE_RECORDS: std::cell::RefCell<Option<Vec<(&'static str, &'static str)>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn observe_open_failures_for_test() {
    OPEN_FAILURE_RECORDS.with(|records| *records.borrow_mut() = Some(Vec::new()));
}
#[cfg(test)]
pub(crate) fn take_open_failures_for_test() -> Vec<(&'static str, &'static str)> {
    OPEN_FAILURE_RECORDS.with(|records| records.borrow_mut().take().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RenderMode {
    AccurateXtrans1Encoded,
    Legacy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum CancelResult {
    Cancelled,
    Stale,
    AlreadyFinished,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenAdmission {
    #[serde(rename = "openKey")]
    pub(crate) open_key: OpenKey,
    pub(crate) metadata: ImageMetadata,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyedLoadResult {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) metadata: ImageMetadata,
    pub(crate) exif: HashMap<String, String>,
    pub(crate) is_raw: bool,
    pub(crate) as_shot_white_balance: crate::white_balance::WhiteBalance,
    #[serde(rename = "openKey")]
    pub(crate) open_key: OpenKey,
    #[serde(rename = "renderMode")]
    pub(crate) render_mode: RenderMode,
    /// Present only when this genuine fresh-RAW open selected one camera
    /// default base; `metadata` remains the immutable original (unsaved) value.
    #[serde(
        rename = "initialProfileDefault",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) initial_profile_default: Option<InitialProfileDefault>,
}

/// Provenance for an unsaved camera default applied inside one coherent open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialProfileDefault {
    pub(crate) base: String,
}

/// Native identity for future B2d cache/jobs; path+adjustments is insufficient.
/// A fresh profile/creative specification still needs its own consumer binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceIdentity {
    open_key: OpenKey,
    spec_id: Uuid,
}
impl SourceIdentity {
    pub(crate) fn open_key(self) -> OpenKey {
        self.open_key
    }
}
pub(crate) struct OpenSpec {
    identity: SourceIdentity,
    path: Arc<str>,
    metadata: ImageMetadata,
    settings: AppSettings,
    policy: AccurateRawPolicy,
    // None is exclusively the historical metadata-only completion seam.
    creative: Option<Result<CapturedCreativeAssets, CreativeCaptureError>>,
    // Immutable capture evidence: provably missing primary sidecar, no XMP and
    // no virtual copy. It permits, never forces, one optional camera default.
    fresh_source: bool,
    #[cfg(test)]
    drop_witness: Option<SnapshotDropWitness>,
}

/// Only the actual Snapshot worker can construct this package. Weak provenance
/// binds it to that owner without an Inner -> spec -> Inner strong cycle.
pub(crate) struct CapturedSnapshot {
    origin: Weak<Inner>,
    spec: Arc<OpenSpec>,
}

#[cfg(test)]
impl CapturedSnapshot {
    pub(crate) fn observe_retirement_for_test(
        &mut self,
        observed: Arc<std::sync::atomic::AtomicBool>,
        unlocked: Arc<std::sync::atomic::AtomicBool>,
    ) {
        Arc::get_mut(&mut self.spec)
            .expect("unpublished unique captured spec")
            .drop_witness = Some(SnapshotDropWitness {
            origin: self.origin.clone(),
            observed,
            unlocked,
        });
    }
}

#[cfg(test)]
struct SnapshotDropWitness {
    origin: Weak<Inner>,
    observed: Arc<std::sync::atomic::AtomicBool>,
    unlocked: Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(test)]
impl Drop for OpenSpec {
    fn drop(&mut self) {
        if let Some(witness) = &self.drop_witness {
            if let Some(inner) = witness.origin.upgrade() {
                witness
                    .unlocked
                    .store(inner.gate.try_lock().is_ok(), Ordering::SeqCst);
            }
            witness.observed.store(true, Ordering::SeqCst);
        }
    }
}

impl OpenSpec {
    pub(crate) fn identity(&self) -> SourceIdentity {
        self.identity
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }
    pub(crate) fn settings(&self) -> &AppSettings {
        &self.settings
    }
    /// Borrow only this specification's actual managed capture outcome.
    pub(crate) fn captured_creative(&self) -> Result<&CapturedCreativeAssets, OpenError> {
        match &self.creative {
            Some(Ok(assets)) => Ok(assets),
            Some(Err(error)) => {
                log_open_failure("creative.use", error.diagnostic_category());
                Err(OpenError::DevelopmentFailed)
            }
            None => Err(OpenError::Unavailable),
        }
    }

    pub(crate) fn policy(&self) -> AccurateRawPolicy {
        self.policy
    }

    pub(crate) fn initial_profile_default_allowed(&self) -> bool {
        self.fresh_source
    }
}
struct AccurateSource {
    retained: Option<Arc<crate::image_loader::RetainedOpenAppearance>>,
    spec: Arc<OpenSpec>,
    loaded: LoadedImage,
    appearance: Arc<DynamicImage>,
}
/// Holds a counted source lease through actual rendering and publication.
/// Consumers must retain this owner, not detach an image clone from its lease.
pub(crate) struct AccurateSnapshot {
    source: Arc<AccurateSource>,
    lease: RenderLease,
}
impl AccurateSnapshot {
    pub(crate) fn retained(
        &self,
    ) -> Result<&Arc<crate::image_loader::RetainedOpenAppearance>, OpenError> {
        self.is_current()?;
        self.source.retained.as_ref().ok_or(OpenError::Unavailable)
    }
    pub(crate) fn identity(&self) -> SourceIdentity {
        self.source.spec.identity
    }
    pub(crate) fn loaded(&self) -> &LoadedImage {
        &self.source.loaded
    }
    pub(crate) fn initial_appearance(&self) -> &DynamicImage {
        &self.source.appearance
    }
    pub(crate) fn spec(&self) -> &OpenSpec {
        &self.source.spec
    }
    pub(crate) fn is_current(&self) -> Result<(), OpenError> {
        self.lease.check()
    }
    pub(crate) fn allows_native_transform(&self) -> bool {
        false
    }
}
/// Produced only by the native prepared/explicit-legacy adapter, never by IPC.
pub(crate) struct OpenCandidate {
    retained: Option<Arc<crate::image_loader::RetainedOpenAppearance>>,
    spec: Arc<OpenSpec>,
    loaded: LoadedImage,
    appearance: Arc<DynamicImage>,
    exif: HashMap<String, String>,
    mode: RenderMode,
    initial_profile_default: Option<InitialProfileDefault>,
}
impl OpenCandidate {
    /// Provenance only; the paired appearance already holds the selected assets.
    pub(crate) fn with_initial_profile_default(
        mut self,
        initial: Option<InitialProfileDefault>,
    ) -> Self {
        self.initial_profile_default = initial;
        self
    }
    pub(crate) fn from_retained(
        token: crate::image_loader::RetainedPreparedCandidate,
    ) -> Result<Self, OpenError> {
        let (mut candidate, retained) = token.into_parts();
        if candidate.mode != RenderMode::AccurateXtrans1Encoded
            || !Arc::ptr_eq(&candidate.spec, retained.spec())
            || candidate.loaded.open_identity != Some(retained.spec().identity())
            || candidate.loaded.path != retained.spec().path()
            || !candidate
                .loaded
                .prepared_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, retained.source()))
        {
            return Err(OpenError::CommitFailed);
        }
        candidate.retained = Some(retained);
        Ok(candidate)
    }
    #[cfg(test)]
    pub(crate) fn images_for_test(&self) -> (&DynamicImage, &DynamicImage) {
        (&self.loaded.image, &self.appearance)
    }

    pub(crate) fn new(
        spec: Arc<OpenSpec>,
        loaded: LoadedImage,
        appearance: Arc<DynamicImage>,
        exif: HashMap<String, String>,
        mode: RenderMode,
    ) -> Result<Self, OpenError> {
        let identity_ok = match mode {
            RenderMode::AccurateXtrans1Encoded => {
                loaded.is_raw
                    && loaded.prepared_source.is_some()
                    && loaded.open_identity == Some(spec.identity)
            }
            RenderMode::Legacy => {
                loaded.prepared_source.is_none() && loaded.open_identity.is_none()
            }
        };
        if !identity_ok
            || loaded.path != spec.path.as_ref()
            || loaded.image.dimensions() != appearance.dimensions()
        {
            return Err(OpenError::CommitFailed);
        }
        Ok(Self {
            retained: None,
            spec,
            loaded,
            appearance,
            exif,
            mode,
            initial_profile_default: None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Reserved,
    WaitingSnapshot,
    Snapshotting,
    Ready,
    WaitingPixels,
    Refining,
    Accurate,
    Failed,
    Cancelled,
}
impl Phase {
    fn terminal(self) -> bool {
        matches!(self, Self::Accurate | Self::Failed | Self::Cancelled)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    Snapshot,
    Pixels,
}
struct Reservation {
    key: OpenKey,
    path: Arc<str>,
    phase: Phase,
    spec: Option<Arc<OpenSpec>>,
    cancel: Arc<AtomicUsize>,
    offered: bool,
}
struct WorkerState {
    key: OpenKey,
    kind: JobKind,
    cancel: Arc<AtomicUsize>,
    execution_live: bool,
    supervisor_live: bool,
}
struct State {
    generation: u64,
    current: Option<Reservation>,
    accurate: Option<Arc<AccurateSource>>,
    last_terminal: Option<OpenKey>,
    worker: Option<WorkerState>,
    render_leases: usize,
    closed: bool,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparationObservation {
    Read,
    Factory,
    Render,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FitObservation {
    Capture,
    GpuSnapshot,
    GpuOwner,
    Base,
    Composition,
    Render,
    Encoding,
    Event,
    Offer,
    Transport,
}

struct Inner {
    #[cfg(test)]
    after_fit: Mutex<Option<Arc<dyn Fn(FitObservation) + Send + Sync>>>,
    #[cfg(test)]
    after_preparation: Mutex<Option<Arc<dyn Fn(PreparationObservation) + Send + Sync>>>,
    // Per-owner, void observation after the real capture; never supplies a result.
    #[cfg(test)]
    after_capture: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    gate: Mutex<State>,
    changed: Notify,
    policy: AccurateRawPolicy,
}
impl Inner {
    fn close_flags(s: &mut State) {
        s.closed = true;
        if let Some(r) = &s.current {
            r.cancel.store(1, Ordering::SeqCst);
        }
        if let Some(w) = &s.worker {
            w.cancel.store(1, Ordering::SeqCst);
        }
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>, OpenError> {
        match self.gate.lock() {
            Ok(g) => Ok(g),
            Err(p) => {
                let mut g = p.into_inner();
                Self::close_flags(&mut g);
                drop(g);
                self.changed.notify_waiters();
                Err(OpenError::Poisoned)
            }
        }
    }
    fn check(s: &State, key: OpenKey) -> Result<&Reservation, OpenError> {
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        let r = s.current.as_ref().ok_or(OpenError::Stale)?;
        if r.key != key {
            return Err(OpenError::Stale);
        }
        match r.phase {
            Phase::Cancelled => Err(OpenError::Cancelled),
            Phase::Failed => Err(OpenError::Unavailable),
            _ => Ok(r),
        }
    }
    fn cancel(&self, key: OpenKey) -> Result<CancelResult, OpenError> {
        let mut s = self.lock()?;
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        let Some(r) = s.current.as_mut() else {
            return Ok(if s.last_terminal == Some(key) {
                CancelResult::AlreadyFinished
            } else {
                CancelResult::Stale
            });
        };
        if r.key != key {
            return Ok(CancelResult::Stale);
        }
        if r.phase.terminal() {
            return Ok(CancelResult::AlreadyFinished);
        }
        r.cancel.store(1, Ordering::SeqCst);
        r.phase = Phase::Cancelled;
        let retired = r.spec.take();
        s.last_terminal = Some(key);
        drop(s);
        drop(retired);
        self.changed.notify_waiters();
        Ok(CancelResult::Cancelled)
    }
    fn fail(&self, key: OpenKey) {
        let Ok(mut s) = self.lock() else {
            return;
        };
        let Some(r) = s
            .current
            .as_mut()
            .filter(|r| r.key == key && !r.phase.terminal())
        else {
            return;
        };
        r.phase = Phase::Failed;
        r.cancel.store(1, Ordering::SeqCst);
        let retired = r.spec.take();
        s.last_terminal = Some(key);
        drop(s);
        drop(retired);
        self.changed.notify_waiters();
    }
    fn worker_end(&self, key: OpenKey, kind: JobKind, execution: bool, joined: bool) {
        let mut s = match self.gate.lock() {
            Ok(g) => g,
            Err(p) => {
                let mut g = p.into_inner();
                Inner::close_flags(&mut g);
                g
            }
        };
        let Some(w) = s.worker.as_mut().filter(|w| w.key == key && w.kind == kind) else {
            return;
        };
        if execution {
            w.execution_live = false;
        } else {
            w.supervisor_live = false;
        }
        let finished = !w.execution_live && !w.supervisor_live;
        if !execution && !joined {
            // Runtime loss/unpolled supervisor cannot prove a normal join. Fail
            // closed permanently; the execution/outcome owner still fences work.
            w.cancel.store(1, Ordering::SeqCst);
            Self::close_flags(&mut s);
        }
        let retired = if finished { s.worker.take() } else { None };
        drop(s);
        drop(retired);
        self.changed.notify_waiters();
    }
}

pub(crate) struct OpenOwner {
    inner: Arc<Inner>,
}
impl OpenOwner {
    pub(crate) fn new(policy: AccurateRawPolicy) -> Self {
        Self {
            inner: Arc::new(Inner {
                #[cfg(test)]
                after_fit: Mutex::new(None),
                #[cfg(test)]
                after_preparation: Mutex::new(None),
                #[cfg(test)]
                after_capture: Mutex::new(None),
                policy,
                changed: Notify::new(),
                gate: Mutex::new(State {
                    generation: 0,
                    current: None,
                    accurate: None,
                    last_terminal: None,
                    worker: None,
                    render_leases: 0,
                    closed: false,
                }),
            }),
        }
    }
    #[cfg(test)]
    pub(crate) fn observe_fit_for_test(&self, observer: Arc<dyn Fn(FitObservation) + Send + Sync>) {
        *self.inner.after_fit.lock().unwrap() = Some(observer);
    }

    #[cfg(test)]
    pub(crate) fn observe_preparation_for_test(
        &self,
        observer: Arc<dyn Fn(PreparationObservation) + Send + Sync>,
    ) {
        *self.inner.after_preparation.lock().unwrap() = Some(observer);
    }

    #[cfg(test)]
    pub(crate) fn gate_available_for_test(&self) -> bool {
        self.inner.gate.try_lock().is_ok()
    }

    /// Reject new work and cooperatively cancel existing owners. This does not
    /// claim that blocking work or native presentation has already drained.
    pub(crate) fn close(&self) {
        let mut s = match self.inner.gate.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        Inner::close_flags(&mut s);
        let retired = s.current.take();
        let source = s.accurate.take();
        drop(s);
        drop(retired);
        drop(source);
        self.inner.changed.notify_waiters();
    }
    pub(crate) fn reserve(&self, ticket: &str, path: String) -> Result<OpenKey, OpenError> {
        let ticket = canonical_ticket(ticket)?;
        if path.is_empty() || path.len() > 32768 || path.chars().any(char::is_control) {
            return Err(OpenError::InvalidPath);
        }
        let path: Arc<str> = path.into();
        let cancel = Arc::new(AtomicUsize::new(0));
        let mut s = self.inner.lock()?;
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        let Some(generation) = s.generation.checked_add(1) else {
            Inner::close_flags(&mut s);
            drop(s);
            self.inner.changed.notify_waiters();
            return Err(OpenError::Exhausted);
        };
        let key = OpenKey { ticket, generation };
        s.generation = generation;
        if let Some(r) = &s.current {
            r.cancel.store(1, Ordering::SeqCst);
        }
        if let Some(w) = &s.worker {
            w.cancel.store(1, Ordering::SeqCst);
        }
        let old = s.current.replace(Reservation {
            key,
            path,
            phase: Phase::Reserved,
            spec: None,
            cancel,
            offered: false,
        });
        let old_source = s.accurate.take();
        s.last_terminal = None;
        drop(s);
        drop(old);
        drop(old_source);
        self.inner.changed.notify_waiters();
        Ok(key)
    }
    fn operation(&self, key: OpenKey, kind: JobKind) -> Result<OperationGuard, OpenError> {
        let mut s = self.inner.lock()?;
        let r = Inner::check(&s, key)?;
        let expected = match kind {
            JobKind::Snapshot => Phase::Reserved,
            JobKind::Pixels => Phase::Ready,
        };
        if r.phase != expected {
            return Err(OpenError::AlreadyStarted);
        }
        s.current.as_mut().ok_or(OpenError::Stale)?.phase = match kind {
            JobKind::Snapshot => Phase::WaitingSnapshot,
            JobKind::Pixels => Phase::WaitingPixels,
        };
        Ok(OperationGuard {
            inner: Arc::clone(&self.inner),
            key,
            armed: true,
        })
    }
    fn check_completion(&self, key: OpenKey, kind: JobKind) -> Result<(), OpenError> {
        let s = self.inner.lock()?;
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        if kind == JobKind::Pixels && s.current.is_none() && s.last_terminal == Some(key) {
            return Ok(());
        }
        let current = Inner::check(&s, key)?;
        let expected = match kind {
            JobKind::Snapshot => Phase::Ready,
            JobKind::Pixels => Phase::Accurate,
        };
        if current.phase == expected {
            Ok(())
        } else {
            Err(OpenError::Unavailable)
        }
    }
    pub(crate) fn cancel(&self, key: OpenKey) -> Result<CancelResult, OpenError> {
        self.inner.cancel(key)
    }
    pub(crate) fn check_current(&self, key: OpenKey) -> Result<(), OpenError> {
        let s = self.inner.lock()?;
        Inner::check(&s, key).map(|_| ())
    }
    pub(crate) fn check_path(&self, key: OpenKey, path: &str) -> Result<(), OpenError> {
        let s = self.inner.lock()?;
        if Inner::check(&s, key)?.path.as_ref() == path {
            Ok(())
        } else {
            Err(OpenError::Stale)
        }
    }
    pub(crate) fn owns(&self, context: &WorkerContext) -> bool {
        Arc::ptr_eq(&self.inner, &context.inner)
    }
    async fn acquire(&self, key: OpenKey, kind: JobKind) -> Result<WorkerContext, OpenError> {
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut s = self.inner.lock()?;
                let r = Inner::check(&s, key)?;
                let wanted = match kind {
                    JobKind::Snapshot => Phase::WaitingSnapshot,
                    JobKind::Pixels => Phase::WaitingPixels,
                };
                if r.phase != wanted {
                    return Err(OpenError::AlreadyStarted);
                }
                if s.worker.is_none() && s.render_leases == 0 {
                    let r = s.current.as_mut().ok_or(OpenError::Stale)?;
                    r.phase = match kind {
                        JobKind::Snapshot => Phase::Snapshotting,
                        JobKind::Pixels => Phase::Refining,
                    };
                    let context = WorkerContext {
                        inner: Arc::clone(&self.inner),
                        key,
                        kind,
                        path: Arc::clone(&r.path),
                        spec: r.spec.clone(),
                        cancel: Arc::clone(&r.cancel),
                    };
                    s.worker = Some(WorkerState {
                        key,
                        kind,
                        cancel: Arc::clone(&context.cancel),
                        execution_live: true,
                        supervisor_live: true,
                    });
                    return Ok(context);
                }
            }
            notified.await;
        }
    }
    /// One spawned supervisor only after the latest reservation acquires the
    /// sole worker. Caller future drop cancels via OperationGuard, not abort.
    pub(crate) fn execute<T, R, W, C>(
        &self,
        key: OpenKey,
        kind: JobKind,
        work: W,
        complete: C,
    ) -> impl std::future::Future<Output = Result<R, OpenError>> + '_
    where
        T: Send + 'static,
        R: Send + 'static,
        W: FnOnce(&WorkerContext) -> Result<T, OpenError> + Send + 'static,
        C: FnOnce(&WorkerContext, T) -> Result<R, OpenError> + Send + 'static,
    {
        // Claim synchronously: even an unpolled returned future owns cancellation.
        let operation = self.operation(key, kind);
        async move {
            let mut operation = operation?;
            let runtime =
                tokio::runtime::Handle::try_current().map_err(|_| OpenError::NoRuntime)?;
            let context = self.acquire(key, kind).await?;
            let execution = ExecutionGuard {
                inner: Arc::clone(&self.inner),
                key,
                kind,
            };
            let supervisor = SupervisorGuard {
                inner: Arc::clone(&self.inner),
                key,
                kind,
                joined: false,
            };
            let worker_context = context.clone();
            let join = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.spawn_blocking(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        worker_context.check()?;
                        work(&worker_context)
                    }))
                    .unwrap_or(Err(OpenError::WorkerFailed));
                    WorkerOutcome { result, execution }
                })
            }))
            .map_err(|_| OpenError::WorkerFailed)?;
            let (send, receive) = oneshot::channel();
            let future = async move {
                let mut supervisor = supervisor;
                let mut complete = Some(complete);
                let result = match join.await {
                    Ok(outcome) => {
                        supervisor.joined = true;
                        let WorkerOutcome { result, execution } = outcome;
                        let result = match result {
                            Ok(value) => {
                                let complete = complete.take().expect("one completion owner");
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    context.check()?;
                                    complete(&context, value)
                                }))
                                .unwrap_or(Err(OpenError::WorkerFailed))
                            }
                            Err(error) => Err(error),
                        };
                        if result.is_err() {
                            context.inner.fail(context.key);
                        }
                        drop(execution);
                        result
                    }
                    Err(_) => {
                        supervisor.joined = true;
                        context.inner.fail(context.key);
                        Err(OpenError::WorkerFailed)
                    }
                };
                // The execution outcome (including all unadopted heavy values) is
                // gone and the JoinHandle was awaited before releasing this fence.
                drop(complete);
                drop(context);
                drop(supervisor);
                let _ = send.send(result);
            };
            // An unpolled/dropped supervisor fails closed via its captured guard.
            // No Inner->task handle edge or replacement task is retained.
            let spawned =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runtime.spawn(future)))
                    .map_err(|_| OpenError::WorkerFailed)?;
            drop(spawned); // Supervisor lifetime is tracked by guard; this is NOT a join claim.
            let result = receive.await.map_err(|_| OpenError::Unavailable)??;
            self.check_completion(key, kind)?;
            operation.disarm();
            Ok(result)
        }
    }
    pub(crate) async fn wait_legacy_drain(&self) -> Result<(), OpenError> {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.inner.lock()?;
                if state.closed {
                    return Err(OpenError::Unavailable);
                }
                if state.current.is_some() || state.accurate.is_some() || state.worker.is_some() {
                    return Err(OpenError::Refining);
                }
                if state.render_leases == 0 {
                    return Ok(());
                }
            }
            changed.await;
        }
    }
    pub(crate) fn legacy_render_lease(&self) -> Result<RenderLease, OpenError> {
        let mut s = self.inner.lock()?;
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        if s.current.is_some() || s.accurate.is_some() || s.worker.is_some() {
            return Err(OpenError::Refining);
        }
        s.render_leases = s.render_leases.checked_add(1).ok_or(OpenError::Exhausted)?;
        Ok(RenderLease {
            inner: Arc::clone(&self.inner),
            key: None,
            phase: None,
        })
    }
    pub(crate) fn accurate_snapshot(
        &self,
        expected: Option<&OpenKey>,
    ) -> Result<AccurateSnapshot, OpenError> {
        let mut s = self.inner.lock()?;
        if s.closed {
            return Err(OpenError::Unavailable);
        }
        let key = expected.copied().ok_or(
            if s.current.as_ref().is_some_and(|r| !r.phase.terminal()) {
                OpenError::Refining
            } else {
                OpenError::Unavailable
            },
        )?;
        let r = Inner::check(&s, key).map_err(|e| {
            if e == OpenError::Cancelled {
                OpenError::Unavailable
            } else {
                e
            }
        })?;
        if r.phase != Phase::Accurate {
            return Err(OpenError::Refining);
        }
        let source = Arc::clone(s.accurate.as_ref().ok_or(OpenError::Unavailable)?);
        s.render_leases = s.render_leases.checked_add(1).ok_or(OpenError::Exhausted)?;
        Ok(AccurateSnapshot {
            source,
            lease: RenderLease {
                inner: Arc::clone(&self.inner),
                key: Some(key),
                phase: Some(Phase::Accurate),
            },
        })
    }
    pub(crate) fn offer_placeholder(
        &self,
        event: PlaceholderEvent,
    ) -> Result<PlaceholderOffer, OpenError> {
        let event = Arc::new(event); // Large payload created/dropped outside gate.
        let mut s = self.inner.lock()?;
        let r = Inner::check(&s, event.open_key)?;
        if r.phase != Phase::Refining || r.offered {
            return Err(OpenError::Unavailable);
        }
        let leases = s.render_leases.checked_add(1).ok_or(OpenError::Exhausted)?;
        s.current.as_mut().ok_or(OpenError::Stale)?.offered = true;
        s.render_leases = leases;
        let key = event.open_key;
        drop(s);
        Ok(PlaceholderOffer {
            event,
            lease: RenderLease {
                inner: Arc::clone(&self.inner),
                key: Some(key),
                phase: Some(Phase::Refining),
            },
        })
    }
}
impl Drop for OpenOwner {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) struct OperationGuard {
    inner: Arc<Inner>,
    key: OpenKey,
    armed: bool,
}
impl OperationGuard {
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.inner.cancel(self.key);
        }
    }
}
#[derive(Clone)]
pub(crate) struct WorkerContext {
    inner: Arc<Inner>,
    key: OpenKey,
    kind: JobKind,
    path: Arc<str>,
    spec: Option<Arc<OpenSpec>>,
    cancel: Arc<AtomicUsize>,
}
impl WorkerContext {
    pub(crate) fn key(&self) -> OpenKey {
        self.key
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn raw_cancel(&self) -> crate::raw_processing::RawCancel {
        Some((Arc::clone(&self.cancel), 0))
    }
    pub(crate) fn check(&self) -> Result<(), OpenError> {
        if self.cancel.load(Ordering::SeqCst) != 0 {
            return Err(OpenError::Cancelled);
        }
        let s = self.inner.lock()?;
        Inner::check(&s, self.key).map(|_| ())
    }
    pub(crate) fn spec(&self) -> Result<Arc<OpenSpec>, OpenError> {
        self.check()?;
        self.spec.clone().ok_or(OpenError::Unavailable)
    }
    /// Authority for actual current Pixels work, not merely a matching path/key.
    pub(crate) fn pixels_spec(&self) -> Result<Arc<OpenSpec>, OpenError> {
        self.check()?;
        let state = self.inner.lock()?;
        let current = Inner::check(&state, self.key)?;
        let expected = self.spec.as_ref().ok_or(OpenError::Unavailable)?;
        if self.kind != JobKind::Pixels
            || current.phase != Phase::Refining
            || current.path.as_ref() != self.path.as_ref()
            || !current
                .spec
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(s, expected))
            || !state.worker.as_ref().is_some_and(|worker| {
                worker.key == self.key
                    && worker.kind == self.kind
                    && worker.execution_live
                    && worker.supervisor_live
            })
            || self.cancel.load(Ordering::SeqCst) != 0
        {
            return Err(OpenError::Stale);
        }
        let spec = Arc::clone(expected);
        drop(state);
        Ok(spec)
    }

    #[cfg(test)]
    pub(crate) fn observe_fit(&self, stage: FitObservation) {
        let observer = self.inner.after_fit.lock().unwrap().clone();
        if let Some(observer) = observer {
            observer(stage);
        }
    }

    #[cfg(test)]
    pub(crate) fn observe_preparation(&self, stage: PreparationObservation) {
        let observer = self.inner.after_preparation.lock().unwrap().clone();
        if let Some(observer) = observer {
            observer(stage);
        }
    }

    fn check_snapshot_worker(&self) -> Result<(), OpenError> {
        self.check()?;
        let s = self.inner.lock()?;
        let r = Inner::check(&s, self.key)?;
        if self.kind != JobKind::Snapshot
            || r.phase != Phase::Snapshotting
            || r.path.as_ref() != self.path.as_ref()
            || !s.worker.as_ref().is_some_and(|w| {
                w.key == self.key && w.kind == self.kind && w.execution_live && w.supervisor_live
            })
            || self.cancel.load(Ordering::SeqCst) != 0
        {
            return Err(OpenError::Stale);
        }
        Ok(())
    }

    pub(crate) fn capture_snapshot(
        &self,
        metadata: ImageMetadata,
        settings: AppSettings,
    ) -> Result<CapturedSnapshot, OpenError> {
        self.capture_snapshot_with_source_state(metadata, settings, false)
    }

    /// `fresh_source` must come from the same single sidecar read plus XMP
    /// evidence; it never changes the captured metadata or identity.
    pub(crate) fn capture_snapshot_with_source_state(
        &self,
        metadata: ImageMetadata,
        settings: AppSettings,
        fresh_source: bool,
    ) -> Result<CapturedSnapshot, OpenError> {
        self.check_snapshot_worker()?;
        // Missing/malformed sidecars use Null to mean no edits. Normalize only
        // this derived capture input; original admission/spec metadata stays Null.
        let empty_adjustments = serde_json::Value::Object(serde_json::Map::new());
        let adjustments = if metadata.adjustments.is_null() {
            &empty_adjustments
        } else {
            &metadata.adjustments
        };
        let creative = capture_creative_assets(adjustments, &settings, self.raw_cancel());
        #[cfg(test)]
        {
            let observer = self.inner.after_capture.lock().unwrap().clone();
            if let Some(observer) = observer {
                observer();
            }
        }
        // Currentness/cancellation wins even a genuine capture error race.
        self.check_snapshot_worker().inspect_err(|error| {
            log_open_error("creative.postcheck", error);
        })?;
        if let Err(error) = &creative {
            log_open_failure("creative.capture", error.diagnostic_category());
        }
        if matches!(&creative, Err(CreativeCaptureError::Cancelled)) {
            return Err(OpenError::Cancelled);
        }
        let spec = Arc::new(OpenSpec {
            identity: SourceIdentity {
                open_key: self.key,
                spec_id: Uuid::new_v4(),
            },
            path: Arc::clone(&self.path),
            metadata,
            settings,
            policy: self.inner.policy,
            creative: Some(creative),
            fresh_source,
            #[cfg(test)]
            drop_witness: None,
        });
        Ok(CapturedSnapshot {
            origin: Arc::downgrade(&self.inner),
            spec,
        })
    }

    pub(crate) fn finish_captured_snapshot(
        &self,
        snapshot: CapturedSnapshot,
    ) -> Result<OpenAdmission, OpenError> {
        self.check()?;
        if !Weak::ptr_eq(&snapshot.origin, &Arc::downgrade(&self.inner))
            || snapshot.spec.identity.open_key != self.key
            || snapshot.spec.path.as_ref() != self.path.as_ref()
        {
            return Err(OpenError::Stale);
        }
        self.publish_snapshot(snapshot.spec)
    }

    pub(crate) fn finish_snapshot(
        &self,
        metadata: ImageMetadata,
        settings: AppSettings,
    ) -> Result<OpenAdmission, OpenError> {
        self.check()?;
        let spec = Arc::new(OpenSpec {
            identity: SourceIdentity {
                open_key: self.key,
                spec_id: Uuid::new_v4(),
            },
            path: Arc::clone(&self.path),
            metadata,
            settings,
            policy: self.inner.policy,
            creative: None,
            fresh_source: false,
            #[cfg(test)]
            drop_witness: None,
        });
        self.publish_snapshot(spec)
    }

    fn publish_snapshot(&self, spec: Arc<OpenSpec>) -> Result<OpenAdmission, OpenError> {
        self.check()?;
        if spec.identity.open_key != self.key || spec.path.as_ref() != self.path.as_ref() {
            return Err(OpenError::Stale);
        }
        let admission = OpenAdmission {
            open_key: self.key,
            metadata: spec.metadata.clone(),
        };
        let mut s = self.inner.lock()?;
        let r = Inner::check(&s, self.key)?;
        if self.kind != JobKind::Snapshot
            || r.phase != Phase::Snapshotting
            || !s.worker.as_ref().is_some_and(|w| {
                w.key == self.key && w.kind == self.kind && w.execution_live && w.supervisor_live
            })
            || self.cancel.load(Ordering::SeqCst) != 0
        {
            return Err(OpenError::Stale);
        }
        let r = s.current.as_mut().ok_or(OpenError::Stale)?;
        r.phase = Phase::Ready;
        let retired = r.spec.replace(spec);
        drop(s);
        drop(retired);
        Ok(admission)
    }
    /// Install closure may only acquire the original/cache locks and move values.
    /// It returns displaced owners; this function releases the open gate first.
    pub(crate) fn commit<R>(
        &self,
        candidate: OpenCandidate,
        install: impl FnOnce(&mut Option<LoadedImage>) -> Result<R, OpenError>,
    ) -> Result<(KeyedLoadResult, R), OpenError> {
        let dimensions = candidate.loaded.image.dimensions();
        let result = KeyedLoadResult {
            width: dimensions.0,
            height: dimensions.1,
            metadata: candidate.spec.metadata.clone(),
            exif: candidate.exif,
            is_raw: candidate.loaded.is_raw,
            as_shot_white_balance: candidate.loaded.as_shot.white_balance,
            open_key: self.key,
            render_mode: candidate.mode,
            initial_profile_default: candidate.initial_profile_default,
        };
        let mut install_image = Some(candidate.loaded.clone()); // No cloning/JSON allocation under gate.
        let source = Arc::new(AccurateSource {
            retained: candidate.retained,
            spec: candidate.spec,
            loaded: candidate.loaded,
            appearance: candidate.appearance,
        });
        let mut s = self.inner.lock()?;
        let r = Inner::check(&s, self.key)?;
        let expected = self.spec.as_ref().ok_or(OpenError::Unavailable)?;
        if self.kind != JobKind::Pixels
            || r.phase != Phase::Refining
            || r.path.as_ref() != source.spec.path()
            || !Arc::ptr_eq(expected, &source.spec)
            || !r.spec.as_ref().is_some_and(|p| Arc::ptr_eq(p, expected))
            || !s.worker.as_ref().is_some_and(|w| {
                w.key == self.key && w.kind == self.kind && w.execution_live && w.supervisor_live
            })
            || self.cancel.load(Ordering::SeqCst) != 0
        {
            return Err(OpenError::Stale);
        }
        // Take the already validated reservation before install. No fallible
        // lookup or allocation is allowed after original/cache publication.
        let mut reservation = s.current.take().ok_or(OpenError::Stale)?;
        let retired = match install(&mut install_image) {
            Ok(retired) => retired,
            Err(error) => {
                s.current = Some(reservation);
                drop(s);
                drop(install_image);
                return Err(error);
            }
        };
        let (old_source, retired_reservation) = match result.render_mode {
            RenderMode::AccurateXtrans1Encoded => {
                reservation.phase = Phase::Accurate;
                s.current = Some(reservation);
                (s.accurate.replace(source), None)
            }
            RenderMode::Legacy => (s.accurate.take(), Some(reservation)),
        };
        s.last_terminal = Some(self.key);
        drop(s);
        drop(old_source);
        drop(retired_reservation);
        self.inner.changed.notify_waiters();
        Ok((result, retired))
    }
}
struct WorkerOutcome<T> {
    result: Result<T, OpenError>,
    execution: ExecutionGuard,
}
struct ExecutionGuard {
    inner: Arc<Inner>,
    key: OpenKey,
    kind: JobKind,
}
impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        self.inner.worker_end(self.key, self.kind, true, true);
    }
}
struct SupervisorGuard {
    inner: Arc<Inner>,
    key: OpenKey,
    kind: JobKind,
    joined: bool,
}
impl Drop for SupervisorGuard {
    fn drop(&mut self) {
        self.inner
            .worker_end(self.key, self.kind, false, self.joined);
    }
}
/// Counted actual render/event resource lifetime, not permission to use stale pixels.
pub(crate) struct RenderLease {
    inner: Arc<Inner>,
    key: Option<OpenKey>,
    phase: Option<Phase>,
}
impl RenderLease {
    pub(crate) fn check(&self) -> Result<(), OpenError> {
        let s = self.inner.lock()?;
        if let Some(key) = self.key {
            let r = Inner::check(&s, key)?;
            if Some(r.phase) == self.phase {
                Ok(())
            } else {
                Err(OpenError::Unavailable)
            }
        } else if s.closed || s.current.is_some() {
            Err(OpenError::Stale)
        } else {
            Ok(())
        }
    }
}
impl Drop for RenderLease {
    fn drop(&mut self) {
        let mut s = match self.inner.gate.lock() {
            Ok(g) => g,
            Err(p) => {
                let mut g = p.into_inner();
                Inner::close_flags(&mut g);
                g
            }
        };
        match s.render_leases.checked_sub(1) {
            Some(n) => s.render_leases = n,
            None => Inner::close_flags(&mut s),
        }
        drop(s);
        self.inner.changed.notify_waiters();
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlaceholderEvent {
    schema_version: u8,
    open_key: OpenKey,
    width: u32,
    height: u32,
    mime: &'static str,
    data_base64: String,
}
impl PlaceholderEvent {
    pub(crate) fn from_jpeg(
        key: OpenKey,
        width: u32,
        height: u32,
        fit_limit: u32,
        jpeg: &[u8],
    ) -> Result<Self, OpenError> {
        let limit = fit_limit.min(1920);
        if width == 0
            || height == 0
            || width > limit
            || height > limit
            || jpeg.len() > 2 * 1024 * 1024
            || !jpeg.starts_with(&[0xff, 0xd8, 0xff])
            || !jpeg.ends_with(&[0xff, 0xd9])
        {
            return Err(OpenError::InvalidPlaceholder);
        }
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(jpeg), image::ImageFormat::Jpeg);
        if reader
            .into_dimensions()
            .map_err(|_| OpenError::InvalidPlaceholder)?
            != (width, height)
        {
            return Err(OpenError::InvalidPlaceholder);
        }
        let event = Self {
            schema_version: 1,
            open_key: key,
            width,
            height,
            mime: "image/jpeg",
            data_base64: base64::engine::general_purpose::STANDARD.encode(jpeg),
        };
        if serde_json::to_vec(&event)
            .map_err(|_| OpenError::InvalidPlaceholder)?
            .len()
            > 3 * 1024 * 1024
        {
            return Err(OpenError::InvalidPlaceholder);
        }
        Ok(event)
    }
}
/// Offer admission linearizes under the open gate. Future B2c must retain this
/// counted lease through transport, check immediately before emitting, and drop
/// it on every exit. No event producer or native-display barrier is wired here.
pub(crate) struct PlaceholderOffer {
    event: Arc<PlaceholderEvent>,
    lease: RenderLease,
}
impl PlaceholderOffer {
    pub(crate) fn event(&self) -> Result<&PlaceholderEvent, OpenError> {
        self.lease.check()?;
        Ok(&self.event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{atomic::AtomicBool, mpsc};
    use std::time::Duration;
    const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    fn policy() -> AccurateRawPolicy {
        AccurateRawPolicy::new(
            1,
            rawler::imgop::sensor::xtrans::external::XTransPasses::One.scratch_bytes(),
        )
        .unwrap()
    }
    fn settings() -> AppSettings {
        let mut s = AppSettings::default();
        s.use_apple_raw9 = Some(false);
        s.raw_preprocessing_color_nr = Some(0.0);
        s.raw_preprocessing_sharpening = Some(0.0);
        s
    }
    async fn ready(owner: &OpenOwner, ticket: &str, path: &str) -> OpenKey {
        let key = owner.reserve(ticket, path.into()).unwrap();
        owner
            .execute(
                key,
                JobKind::Snapshot,
                |_| Ok((ImageMetadata::default(), settings())),
                |context, (metadata, settings)| context.finish_snapshot(metadata, settings),
            )
            .await
            .unwrap();
        key
    }
    async fn phase(owner: &OpenOwner, key: OpenKey, expected: Phase) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if owner
                    .inner
                    .gate
                    .lock()
                    .unwrap()
                    .current
                    .as_ref()
                    .is_some_and(|r| r.key == key && r.phase == expected)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn idle(owner: &OpenOwner) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let n = owner.inner.changed.notified();
                tokio::pin!(n);
                n.as_mut().enable();
                if owner.inner.gate.lock().unwrap().worker.is_none() {
                    break;
                }
                n.await;
            }
        })
        .await
        .unwrap();
    }
    #[test]
    fn canonical_wire_rejects_aliases_duplicate_unknown_overflow_and_true_end() {
        let valid = format!(r#"{{"ticket":"{A}","generation":"18446744073709551615"}}"#);
        let key: OpenKey = serde_json::from_str(&valid).unwrap();
        assert_eq!(key.generation, u64::MAX);
        assert_eq!(serde_json::to_string(&key).unwrap(), valid);
        for value in [
            "",
            "00",
            "01",
            "+1",
            "-1",
            "1.0",
            "1\n",
            "18446744073709551616",
        ] {
            assert!(canonical_generation(value).is_err());
        }
        assert_eq!(canonical_generation("0").unwrap(), 0);
        for ticket in [
            A.to_uppercase(),
            A.replace('-', ""),
            format!("{{{A}}}"),
            format!("{A}\n"),
        ] {
            assert!(canonical_ticket(&ticket).is_err());
        }
        for bad in [
            format!(r#"{{"ticket":"{A}","generation":1}}"#),
            format!(r#"{{"ticket":"{A}","generation":"1","other":0}}"#),
            format!(r#"{{"ticket":"{A}","generation":"1","generation":"2"}}"#),
            format!(r#"{{"ticket":"{A}","generation":"1","\u0074icket":"{B}"}}"#),
        ] {
            assert!(serde_json::from_str::<OpenKey>(&bad).is_err());
        }
    }
    #[test]
    fn generations_aba_cancel_once_and_exhaustion_fail_closed() {
        let owner = OpenOwner::new(policy());
        let a1 = owner.reserve(A, "a.dng".into()).unwrap();
        let b = owner.reserve(B, "b.dng".into()).unwrap();
        let a2 = owner.reserve(A, "a.dng".into()).unwrap();
        assert_ne!(a1, a2);
        assert_eq!(a2.generation, 3);
        assert_eq!(owner.cancel(b).unwrap(), CancelResult::Stale);
        assert_eq!(owner.cancel(a2).unwrap(), CancelResult::Cancelled);
        assert_eq!(owner.cancel(a2).unwrap(), CancelResult::AlreadyFinished);
        assert_eq!(owner.check_current(a1), Err(OpenError::Stale));
        owner.inner.gate.lock().unwrap().generation = u64::MAX;
        assert_eq!(owner.reserve(B, "b.dng".into()), Err(OpenError::Exhausted));
        assert_eq!(
            owner.reserve(A, "a.dng".into()),
            Err(OpenError::Unavailable)
        );
    }
    #[tokio::test]
    async fn unpolled_future_and_duplicate_claim_have_precise_ownership() {
        let owner = OpenOwner::new(policy());
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        let first = owner.execute(key, JobKind::Snapshot, |_| Ok(()), |_, ()| Ok(()));
        assert_eq!(
            owner
                .execute(key, JobKind::Snapshot, |_| Ok(()), |_, ()| Ok(()))
                .await,
            Err(OpenError::AlreadyStarted)
        );
        assert_eq!(owner.check_current(key), Ok(()));
        drop(first);
        assert_eq!(owner.cancel(key).unwrap(), CancelResult::AlreadyFinished);
        assert!(owner.inner.gate.lock().unwrap().worker.is_none());
    }
    #[tokio::test]
    async fn counted_legacy_lease_drains_before_any_snapshot_work() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let lease = owner.legacy_render_lease().unwrap();
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        assert!(owner.legacy_render_lease().is_err());
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let runner = Arc::clone(&owner);
        let job = tokio::spawn(async move {
            runner
                .execute(
                    key,
                    JobKind::Snapshot,
                    move |_| {
                        observed.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                    |c, ()| c.finish_snapshot(ImageMetadata::default(), settings()),
                )
                .await
        });
        phase(&owner, key, Phase::WaitingSnapshot).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(lease.check(), Err(OpenError::Stale));
        drop(lease);
        assert!(job.await.unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        idle(&owner).await;
    }
    #[tokio::test]
    async fn deferred_snapshot_latest_wins_without_returning_old_metadata() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let a = owner.reserve(A, "a.dng".into()).unwrap();
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let runner = Arc::clone(&owner);
        let old = tokio::spawn(async move {
            runner
                .execute(
                    a,
                    JobKind::Snapshot,
                    move |_| {
                        let _ = started.send(());
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        let mut m = ImageMetadata::default();
                        m.rating = 4;
                        Ok(m)
                    },
                    |c, m| c.finish_snapshot(m, settings()),
                )
                .await
        });
        started_rx.await.unwrap();
        let b = owner.reserve(B, "b.dng".into()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let runner = Arc::clone(&owner);
        let latest = tokio::spawn(async move {
            runner
                .execute(
                    b,
                    JobKind::Snapshot,
                    move |_| {
                        seen.fetch_add(1, Ordering::SeqCst);
                        Ok(ImageMetadata::default())
                    },
                    |c, m| c.finish_snapshot(m, settings()),
                )
                .await
        });
        phase(&owner, b, Phase::WaitingSnapshot).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        assert!(old.await.unwrap().is_err());
        let admitted = latest.await.unwrap().unwrap();
        assert_eq!(admitted.open_key, b);
        assert_eq!(admitted.metadata.rating, 0);
    }
    #[tokio::test]
    async fn caller_abort_cancels_but_actual_blocking_worker_remains_fenced() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let a = ready(&owner, A, "a.dng").await;
        let active = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&active);
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let runner = Arc::clone(&owner);
        let task = tokio::spawn(async move {
            runner
                .execute(
                    a,
                    JobKind::Pixels,
                    move |context| {
                        assert_eq!(seen.fetch_add(1, Ordering::SeqCst), 0);
                        let _ = started.send(());
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        assert_eq!(context.raw_cancel().unwrap().0.load(Ordering::SeqCst), 1);
                        seen.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    },
                    |_, ()| Ok(()),
                )
                .await
        });
        started_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let b = owner.reserve(B, "b.dng".into()).unwrap();
        assert!(
            owner
                .inner
                .gate
                .lock()
                .unwrap()
                .worker
                .as_ref()
                .is_some_and(|w| w.key == a)
        );
        let seen = Arc::clone(&active);
        let runner = Arc::clone(&owner);
        let next = tokio::spawn(async move {
            runner
                .execute(
                    b,
                    JobKind::Snapshot,
                    move |_| {
                        assert_eq!(seen.load(Ordering::SeqCst), 0);
                        Ok(())
                    },
                    |c, ()| c.finish_snapshot(ImageMetadata::default(), settings()),
                )
                .await
        });
        phase(&owner, b, Phase::WaitingSnapshot).await;
        release.send(()).unwrap();
        assert!(next.await.unwrap().is_ok());
        idle(&owner).await;
    }
    #[tokio::test]
    async fn panic_failure_and_outcome_drop_release_the_real_job() {
        let owner = OpenOwner::new(policy());
        let key = ready(&owner, A, "a.dng").await;
        assert_eq!(
            owner
                .execute::<(), (), _, _>(
                    key,
                    JobKind::Pixels,
                    |_| panic!("synthetic work panic"),
                    |_, ()| Ok(())
                )
                .await,
            Err(OpenError::WorkerFailed)
        );
        idle(&owner).await;
        assert_eq!(owner.check_current(key), Err(OpenError::Unavailable));
        let key = ready(&owner, B, "b.dng").await;
        assert_eq!(
            owner
                .execute::<(), (), _, _>(
                    key,
                    JobKind::Pixels,
                    |_| Err(OpenError::DevelopmentFailed),
                    |_, ()| Ok(())
                )
                .await,
            Err(OpenError::DevelopmentFailed)
        );
        idle(&owner).await;
    }
    #[tokio::test]
    async fn actual_candidate_commit_snapshot_and_legacy_retirement() {
        let owner = OpenOwner::new(policy());
        let target = Arc::new(Mutex::new(None));
        assert!(owner.accurate_snapshot(None).is_err());
        let a = ready(&owner, A, "a.dng").await;
        assert!(matches!(
            owner.accurate_snapshot(Some(&a)),
            Err(OpenError::Refining)
        ));
        let slot = Arc::clone(&target);
        let result = owner
            .execute(
                a,
                JobKind::Pixels,
                |c| crate::image_loader::progressive_test_candidate(c, true),
                move |c, candidate| {
                    let (wire, retired) = c.commit(candidate, |image| {
                        Ok(slot
                            .lock()
                            .unwrap()
                            .replace(image.take().ok_or(OpenError::CommitFailed)?))
                    })?;
                    drop(retired);
                    Ok(wire)
                },
            )
            .await
            .unwrap();
        assert_eq!(result.render_mode, RenderMode::AccurateXtrans1Encoded);
        let snapshot = owner.accurate_snapshot(Some(&a)).unwrap();
        assert!(!snapshot.allows_native_transform());
        assert_eq!(snapshot.identity().open_key(), a);
        assert!(snapshot.loaded().prepared_source.is_some());
        assert_eq!(
            snapshot.loaded().image.dimensions(),
            snapshot.initial_appearance().dimensions()
        );
        assert!(owner.accurate_snapshot(None).is_err());
        assert!(owner.legacy_render_lease().is_err());
        drop(snapshot);
        let b = ready(&owner, B, "b.dng").await;
        let slot = Arc::clone(&target);
        let result = owner
            .execute(
                b,
                JobKind::Pixels,
                |c| crate::image_loader::progressive_test_candidate(c, false),
                move |c, candidate| {
                    let (wire, retired) = c.commit(candidate, |image| {
                        Ok(slot
                            .lock()
                            .unwrap()
                            .replace(image.take().ok_or(OpenError::CommitFailed)?))
                    })?;
                    drop(retired);
                    Ok(wire)
                },
            )
            .await
            .unwrap();
        assert_eq!(result.render_mode, RenderMode::Legacy);
        assert_eq!(result.open_key, b);
        assert!(
            target
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .open_identity
                .is_none()
        );
        assert!(owner.legacy_render_lease().is_ok());
        assert!(owner.accurate_snapshot(Some(&b)).is_err());
        assert_eq!(owner.cancel(b).unwrap(), CancelResult::AlreadyFinished);
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["renderMode"], "Legacy");
        assert!(wire.get("is_raw").is_some());
        assert!(wire.get("isRaw").is_none());
    }
    #[tokio::test]
    async fn stale_candidate_cannot_call_install_and_old_cancel_is_inert() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let a = ready(&owner, A, "a.dng").await;
        let (ready_tx, ready_rx) = oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let installs = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&installs);
        let runner = Arc::clone(&owner);
        let task = tokio::spawn(async move {
            runner
                .execute(
                    a,
                    JobKind::Pixels,
                    move |c| {
                        let candidate = crate::image_loader::progressive_test_candidate(c, true)?;
                        let _ = ready_tx.send(());
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(candidate)
                    },
                    move |c, candidate| {
                        c.commit(candidate, |image| {
                            count.fetch_add(1, Ordering::SeqCst);
                            image.take().ok_or(OpenError::CommitFailed)
                        })
                        .map(|(wire, retired)| {
                            drop(retired);
                            wire
                        })
                    },
                )
                .await
        });
        ready_rx.await.unwrap();
        let b = owner.reserve(B, "b.dng".into()).unwrap();
        assert_eq!(owner.cancel(a).unwrap(), CancelResult::Stale);
        release.send(()).unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(installs.load(Ordering::SeqCst), 0);
        assert_eq!(owner.check_current(b), Ok(()));
    }
    #[tokio::test]
    async fn displaced_destructor_observes_open_gate_unlocked() {
        struct Probe {
            inner: Arc<Inner>,
            observed: Arc<AtomicBool>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                self.observed
                    .store(self.inner.gate.try_lock().is_ok(), Ordering::SeqCst);
            }
        }
        let owner = OpenOwner::new(policy());
        let a = ready(&owner, A, "a.dng").await;
        let observed = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&observed);
        let inner = Arc::clone(&owner.inner);
        owner
            .execute(
                a,
                JobKind::Pixels,
                |c| crate::image_loader::progressive_test_candidate(c, true),
                move |c, candidate| {
                    let (wire, retired) = c.commit(candidate, |image| {
                        Ok((
                            image.take().ok_or(OpenError::CommitFailed)?,
                            Probe {
                                inner,
                                observed: seen,
                            },
                        ))
                    })?;
                    drop(retired);
                    Ok(wire)
                },
            )
            .await
            .unwrap();
        assert!(observed.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn placeholder_bounds_one_offer_and_terminal_stale_rejection() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let key = ready(&owner, A, "a.dng").await;
        let mut bytes = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(16, 8)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let bytes = bytes.into_inner();
        assert!(PlaceholderEvent::from_jpeg(key, 16, 8, 16, &bytes).is_ok());
        assert!(PlaceholderEvent::from_jpeg(key, 15, 8, 16, &bytes).is_err());
        assert!(PlaceholderEvent::from_jpeg(key, 16, 8, 8, &bytes).is_err());
        assert!(PlaceholderEvent::from_jpeg(key, 0, 8, 16, &bytes).is_err());
        assert!(
            PlaceholderEvent::from_jpeg(key, 16, 8, 16, &vec![0; 2 * 1024 * 1024 + 1]).is_err()
        );
        let event = PlaceholderEvent::from_jpeg(key, 16, 8, 16, &bytes).unwrap();
        let wire = serde_json::to_value(&event).unwrap();
        assert_eq!(wire.as_object().unwrap().len(), 6);
        assert_eq!(wire["schemaVersion"], 1);
        assert_eq!(wire["mime"], "image/jpeg");
        let event2 = event.clone();
        let (send, receive) = oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let worker_owner = Arc::clone(&owner);
        let runner = Arc::clone(&owner);
        let job = tokio::spawn(async move {
            runner
                .execute(
                    key,
                    JobKind::Pixels,
                    move |_| {
                        let offer = worker_owner.offer_placeholder(event)?;
                        assert!(worker_owner.offer_placeholder(event2).is_err());
                        let _ = send.send(offer);
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(())
                    },
                    |_, ()| Err::<(), _>(OpenError::DevelopmentFailed),
                )
                .await
        });
        let offer = receive.await.unwrap();
        assert!(offer.event().is_ok());
        owner.cancel(key).unwrap();
        assert!(offer.event().is_err());
        drop(offer);
        release.send(()).unwrap();
        assert!(job.await.unwrap().is_err());
        idle(&owner).await;
    }
    #[tokio::test]
    async fn poison_and_lost_supervisor_fail_closed_without_early_execution_release() {
        let owner = OpenOwner::new(policy());
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = owner.inner.gate.lock().unwrap();
            panic!("poison");
        }));
        assert_eq!(owner.check_current(key), Err(OpenError::Poisoned));
        assert!(owner.reserve(B, "b.dng".into()).is_err());
        // Actual guard methods, not a duplicate state machine; no thread-start claim.
        let owner = OpenOwner::new(policy());
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        let _operation = owner.operation(key, JobKind::Snapshot).unwrap();
        let _context = owner.acquire(key, JobKind::Snapshot).await.unwrap();
        let execution = ExecutionGuard {
            inner: Arc::clone(&owner.inner),
            key,
            kind: JobKind::Snapshot,
        };
        drop(SupervisorGuard {
            inner: Arc::clone(&owner.inner),
            key,
            kind: JobKind::Snapshot,
            joined: false,
        });
        assert!(owner.inner.gate.lock().unwrap().worker.is_some());
        assert!(owner.reserve(B, "b.dng".into()).is_err());
        drop(execution);
        assert!(owner.inner.gate.lock().unwrap().worker.is_none());
    }
    #[tokio::test]
    async fn owner_drop_cancels_without_releasing_live_blocking_ownership() {
        let owner = Arc::new(OpenOwner::new(policy()));
        let key = ready(&owner, A, "a.dng").await;
        let inner = Arc::clone(&owner.inner);
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let runner = Arc::clone(&owner);
        let task = tokio::spawn(async move {
            runner
                .execute(
                    key,
                    JobKind::Pixels,
                    move |context| {
                        let _ = started.send(());
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        assert_eq!(context.raw_cancel().unwrap().0.load(Ordering::SeqCst), 1);
                        Ok(())
                    },
                    |_, ()| Ok(()),
                )
                .await
        });
        started_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        drop(owner);
        {
            let state = inner.gate.lock().unwrap();
            assert!(state.closed);
            assert!(state.worker.is_some());
        }
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let n = inner.changed.notified();
                tokio::pin!(n);
                n.as_mut().enable();
                if inner.gate.lock().unwrap().worker.is_none() {
                    break;
                }
                n.await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn no_runtime_fails_without_starting_or_leaking_a_worker() {
        let owner = OpenOwner::new(policy());
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        let result = futures::executor::block_on(owner.execute(
            key,
            JobKind::Snapshot,
            |_| Ok(()),
            |_, ()| Ok(()),
        ));
        assert_eq!(result, Err(OpenError::NoRuntime));
        assert!(owner.inner.gate.lock().unwrap().worker.is_none());
        assert_eq!(owner.cancel(key).unwrap(), CancelResult::AlreadyFinished);
    }
    #[tokio::test]
    async fn raw_cancel_flag_never_narrows_the_wire_generation() {
        let owner = OpenOwner::new(policy());
        owner.inner.gate.lock().unwrap().generation = u64::MAX - 1;
        let key = owner.reserve(A, "a.dng".into()).unwrap();
        assert_eq!(key.generation, u64::MAX);
        owner
            .execute(
                key,
                JobKind::Snapshot,
                |context| {
                    let (flag, expected) = context.raw_cancel().unwrap();
                    assert_eq!(expected, 0);
                    assert_eq!(flag.load(Ordering::SeqCst), 0);
                    Ok(())
                },
                |c, ()| c.finish_snapshot(ImageMetadata::default(), settings()),
            )
            .await
            .unwrap();
        assert_eq!(owner.cancel(key).unwrap(), CancelResult::Cancelled);
    }
}

// BEGIN C3a additive captured-owner tests (six fixed groups).
#[cfg(test)]
mod captured_snapshot_tests {
    use super::*;
    use base64::Engine as _;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::sync::{atomic::AtomicBool, mpsc};
    use std::time::Duration;

    const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    fn owner() -> OpenOwner {
        OpenOwner::new(
            AccurateRawPolicy::new(
                1,
                rawler::imgop::sensor::xtrans::external::XTransPasses::One.scratch_bytes(),
            )
            .unwrap(),
        )
    }
    fn settings() -> AppSettings {
        let mut value = AppSettings::default();
        value.use_apple_raw9 = Some(false);
        value.raw_preprocessing_color_nr = Some(0.0);
        value.raw_preprocessing_sharpening = Some(0.0);
        value
    }
    fn parent() -> PathBuf {
        let path = std::env::temp_dir().join(format!("c3a-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap(); // Retained private synthetic evidence.
        path
    }
    fn inline_map() -> String {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([128, 128, 128, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes.get_ref())
    }
    fn assets_input() -> (Value, PathBuf) {
        let cube = parent().join("held.cube");
        std::fs::write(&cube, b"LUT_3D_SIZE 1\n0.25 0.5 1\n").unwrap();
        (
            json!({"exposure":1.25,"lutPath":cube.to_str().unwrap(),
            "fogEnabled":true,"fogAmount":50.0,"fogDepthMap":inline_map(),
            "masks":[{"visible":false,"id":"hidden"}]}),
            cube,
        )
    }
    fn metadata(adjustments: Value) -> ImageMetadata {
        let mut value = ImageMetadata::default();
        value.rating = 3;
        value.adjustments = adjustments;
        value
    }
    fn spec(owner: &OpenOwner) -> Arc<OpenSpec> {
        owner
            .inner
            .gate
            .lock()
            .unwrap()
            .current
            .as_ref()
            .unwrap()
            .spec
            .clone()
            .unwrap()
    }
    async fn idle(owner: &OpenOwner) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let changed = owner.inner.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if owner.inner.gate.lock().unwrap().worker.is_none() {
                    break;
                }
                changed.await;
            }
        })
        .await
        .unwrap();
    }
    async fn phase(owner: &OpenOwner, key: OpenKey, expected: Phase) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if owner
                    .inner
                    .gate
                    .lock()
                    .unwrap()
                    .current
                    .as_ref()
                    .is_some_and(|r| r.key == key && r.phase == expected)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn capture(
        owner: &OpenOwner,
        key: OpenKey,
        value: ImageMetadata,
    ) -> Result<OpenAdmission, OpenError> {
        tokio::time::timeout(
            Duration::from_secs(3),
            owner.execute(
                key,
                JobKind::Snapshot,
                move |c| c.capture_snapshot(value, settings()),
                |c, captured| c.finish_captured_snapshot(captured),
            ),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn captured_spec_keeps_one_metadata_settings_lut_map_snapshot() {
        let owner = owner();
        let (input, cube) = assets_input();
        let original = metadata(input);
        let source = parent().join("unread-source.dng");
        let path = source.to_str().unwrap().to_string();
        let (_, sidecar) = crate::file_management::parse_virtual_path(&path);
        std::fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let key = owner.reserve(A, path.clone()).unwrap();
        let saved_settings = settings();
        let mut caller_settings = saved_settings.clone();
        let admission = tokio::time::timeout(
            Duration::from_secs(3),
            owner.execute(
                key,
                JobKind::Snapshot,
                move |c| crate::image_loader::capture_keyed_snapshot(c, saved_settings),
                |c, captured| c.finish_captured_snapshot(captured),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let held = spec(&owner);
        let creative = held.creative.as_ref().unwrap().as_ref().unwrap();
        let lut = Arc::clone(creative.lut().unwrap());
        let map = Arc::clone(creative.fog_map().unwrap());
        assert_eq!(lut.data, vec![0.25, 0.5, 1.0]);
        assert_eq!(map.dimensions(), (2, 2));
        assert!(map.as_raw().iter().all(|v| *v == 128));
        assert_eq!(
            serde_json::to_value(&admission.metadata).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let mut caller_metadata = original.clone();
        caller_metadata.rating = 5;
        caller_metadata.adjustments["exposure"] = json!(99.0);
        caller_metadata.adjustments["fogDepthMap"] = json!("malformed later inline bytes");
        caller_settings.raw_preprocessing_color_nr = Some(8.0);
        std::fs::write(&sidecar, serde_json::to_vec(&caller_metadata).unwrap()).unwrap();
        std::fs::write(&cube, b"malformed later bytes").unwrap();
        std::fs::remove_file(&cube).unwrap();
        assert_eq!(held.metadata.rating, 3);
        assert_eq!(held.metadata.adjustments["exposure"], json!(1.25));
        assert_eq!(held.settings.raw_preprocessing_color_nr, Some(0.0));
        assert_eq!(creative.settings().raw_preprocessing_color_nr, Some(0.0));
        assert_eq!(
            held.metadata.adjustments["masks"].as_array().unwrap().len(),
            1
        );
        assert!(
            creative.adjustments()["masks"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(Arc::ptr_eq(&lut, creative.lut().unwrap()));
        assert!(Arc::ptr_eq(&map, creative.fog_map().unwrap()));
        assert_eq!(lut.data, vec![0.25, 0.5, 1.0]);
        assert!(map.as_raw().iter().all(|v| *v == 128));
        assert!(!source.exists()); // Snapshot never needed source pixels.
        assert_eq!(caller_settings.raw_preprocessing_color_nr, Some(8.0));
        idle(&owner).await;
    }

    #[tokio::test]
    async fn strict_capture_failure_is_deferred_without_empty_success() {
        for (input, expected) in [
            (json!({}), "success"),
            (json!({"fogEnabled":true,"fogAmount":50.0}), "fog"),
            (
                json!({"lutPath":parent().join("missing.cube").to_str().unwrap()}),
                "lut",
            ),
        ] {
            let owner = owner();
            let key = owner.reserve(A, "not-read.dng".into()).unwrap();
            let original = metadata(input);
            let admission = capture(&owner, key, original.clone()).await.unwrap();
            assert_eq!(
                serde_json::to_value(admission.metadata).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            let held = spec(&owner);
            assert_eq!(held.metadata.adjustments, original.adjustments);
            match (expected, held.creative.as_ref()) {
                ("success", Some(Ok(assets))) => assert_eq!(assets.adjustments(), &json!({})),
                ("fog", Some(Err(CreativeCaptureError::MissingEffectMap("fogDepthMap")))) => {}
                ("lut", Some(Err(CreativeCaptureError::Lut(message)))) => {
                    assert!(!message.is_empty())
                }
                _ => panic!("actual capture outcome was lost or changed"),
            }
            assert_eq!(
                owner
                    .inner
                    .gate
                    .lock()
                    .unwrap()
                    .current
                    .as_ref()
                    .unwrap()
                    .phase,
                Phase::Ready
            );
            idle(&owner).await;
        }

        // BEGIN I1 default-input boundary additions.
        let direct = owner();
        let key = direct.reserve(A, "direct-null.dng".into()).unwrap();
        let original = metadata(Value::Null);
        let expected = serde_json::to_value(&original).unwrap();
        let admission = capture(&direct, key, original).await.unwrap();
        assert_eq!(serde_json::to_value(admission.metadata).unwrap(), expected);
        let held = spec(&direct);
        assert!(held.metadata.adjustments.is_null());
        assert_eq!(serde_json::to_value(&held.metadata).unwrap(), expected);
        let assets = held.creative.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(assets.adjustments(), &json!({}));
        assert_eq!(assets.settings().raw_preprocessing_color_nr, Some(0.0));
        idle(&direct).await;

        for malformed in [false, true] {
            let owner = owner();
            let source = parent().join("absent-source.dng");
            let path = source.to_str().unwrap().to_string();
            let (_, sidecar) = crate::file_management::parse_virtual_path(&path);
            if malformed {
                std::fs::write(&sidecar, b"{not-json").unwrap();
            }
            let key = owner.reserve(A, path).unwrap();
            let admission = tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    key,
                    JobKind::Snapshot,
                    |c| crate::image_loader::capture_keyed_snapshot(c, settings()),
                    |c, package| c.finish_captured_snapshot(package),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            let expected = serde_json::to_value(ImageMetadata::default()).unwrap();
            assert_eq!(serde_json::to_value(admission.metadata).unwrap(), expected);
            let held = spec(&owner);
            assert_eq!(serde_json::to_value(&held.metadata).unwrap(), expected);
            assert!(held.metadata.adjustments.is_null());
            assert_eq!(held.settings.raw_preprocessing_color_nr, Some(0.0));
            let assets = held.creative.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(assets.adjustments(), &json!({}));
            assert_eq!(assets.settings().raw_preprocessing_color_nr, Some(0.0));
            assert!(!source.exists());
            if malformed {
                assert_eq!(std::fs::read(&sidecar).unwrap(), b"{not-json");
            } else {
                assert!(!sidecar.exists());
            }
            idle(&owner).await;
        }
        // Every non-null malformed value still reaches the strict capture API.
        for input in [
            json!([]),
            json!("no edits"),
            json!(0),
            json!(false),
            json!({"masks":[{"visible":true}]}),
        ] {
            let owner = owner();
            let key = owner.reserve(A, "invalid-view.dng".into()).unwrap();
            let original = metadata(input);
            let expected = serde_json::to_value(&original).unwrap();
            let admission = capture(&owner, key, original).await.unwrap();
            assert_eq!(serde_json::to_value(admission.metadata).unwrap(), expected);
            let held = spec(&owner);
            assert_eq!(serde_json::to_value(&held.metadata).unwrap(), expected);
            assert!(matches!(
                held.creative.as_ref(),
                Some(Err(CreativeCaptureError::Hydration(_)))
            ));
            idle(&owner).await;
        }
        let invalid_map = owner();
        let key = invalid_map.reserve(A, "invalid-map.dng".into()).unwrap();
        let original = metadata(json!({"fogEnabled":true,"fogDepthMap":"invalid"}));
        let expected = serde_json::to_value(&original).unwrap();
        let admission = capture(&invalid_map, key, original).await.unwrap();
        assert_eq!(serde_json::to_value(admission.metadata).unwrap(), expected);
        let held = spec(&invalid_map);
        assert_eq!(serde_json::to_value(&held.metadata).unwrap(), expected);
        assert!(matches!(
            held.creative.as_ref(),
            Some(Err(CreativeCaptureError::InvalidEffectMap {
                field: "fogDepthMap",
                ..
            }))
        ));
        idle(&invalid_map).await;
        // END I1 default-input boundary additions.
    }

    #[tokio::test]
    async fn cancel_pre_post_and_capture_error_precedence() {
        let pre = owner();
        let key = pre.reserve(A, "pre.dng".into()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        *pre.inner.after_capture.lock().unwrap() = Some(Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        pre.cancel(key).unwrap();
        assert!(matches!(
            capture(&pre, key, metadata(json!({}))).await,
            Err(OpenError::Cancelled)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for input in [json!({}), json!({"fogEnabled":true})] {
            let owner = Arc::new(owner());
            let key = owner.reserve(A, "post.dng".into()).unwrap();
            let (entered, entered_rx) = oneshot::channel();
            let entered = Mutex::new(Some(entered));
            let (release, release_rx) = mpsc::channel();
            let release_rx = Mutex::new(release_rx);
            *owner.inner.after_capture.lock().unwrap() = Some(Arc::new(move || {
                entered.lock().unwrap().take().unwrap().send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }));
            let runner = Arc::clone(&owner);
            let job = tokio::spawn(async move { capture(&runner, key, metadata(input)).await });
            tokio::time::timeout(Duration::from_secs(2), entered_rx)
                .await
                .unwrap()
                .unwrap();
            owner.cancel(key).unwrap();
            assert!(owner.inner.gate.lock().unwrap().worker.is_some());
            release.send(()).unwrap();
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(3), job)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(OpenError::Cancelled)
            ));
            idle(&owner).await;
            let state = owner.inner.gate.lock().unwrap();
            assert_eq!(state.current.as_ref().unwrap().phase, Phase::Cancelled);
            assert!(state.current.as_ref().unwrap().spec.is_none());
        }
    }

    #[tokio::test]
    async fn foreign_owner_and_stale_generation_cannot_publish_capture() {
        // The package is made by a genuine managed worker, never a test constructor.
        let donor = owner();
        let donor_key = donor.reserve(A, "same.dng".into()).unwrap();
        let (send, receive) = mpsc::channel();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            donor.execute(
                donor_key,
                JobKind::Snapshot,
                move |c| {
                    send.send(c.capture_snapshot(metadata(json!({})), settings())?)
                        .unwrap_or_else(|_| panic!("capture receiver lost"));
                    Ok(())
                },
                |_, ()| Err::<(), _>(OpenError::SnapshotFailed),
            ),
        )
        .await
        .unwrap();
        assert_eq!(result, Err(OpenError::SnapshotFailed));
        let foreign = receive.recv_timeout(Duration::from_secs(2)).unwrap();
        let target = owner();
        let target_key = target.reserve(A, "same.dng".into()).unwrap();
        assert_eq!(target_key, donor_key); // Coincident wire identity is insufficient.
        let rejected = tokio::time::timeout(
            Duration::from_secs(3),
            target.execute(
                target_key,
                JobKind::Snapshot,
                move |_| Ok(foreign),
                |c, captured| c.finish_captured_snapshot(captured),
            ),
        )
        .await
        .unwrap();
        assert!(matches!(rejected, Err(OpenError::Stale)));
        assert!(
            target
                .inner
                .gate
                .lock()
                .unwrap()
                .current
                .as_ref()
                .unwrap()
                .spec
                .is_none()
        );

        let owner = owner();
        let old = owner.reserve(A, "same.dng".into()).unwrap();
        let (send, receive) = mpsc::channel();
        assert_eq!(
            tokio::time::timeout(
                Duration::from_secs(3),
                owner.execute(
                    old,
                    JobKind::Snapshot,
                    move |c| {
                        send.send(c.capture_snapshot(metadata(json!({})), settings())?)
                            .unwrap_or_else(|_| panic!("capture receiver lost"));
                        Ok(())
                    },
                    |_, ()| Err::<(), _>(OpenError::SnapshotFailed)
                )
            )
            .await
            .unwrap(),
            Err(OpenError::SnapshotFailed)
        );
        let old_package = receive.recv_timeout(Duration::from_secs(2)).unwrap();
        owner.reserve(B, "other.dng".into()).unwrap();
        let fresh = owner.reserve(A, "same.dng".into()).unwrap();
        assert_ne!(old, fresh);
        let rejected = tokio::time::timeout(
            Duration::from_secs(3),
            owner.execute(
                fresh,
                JobKind::Snapshot,
                move |_| Ok(old_package),
                |c, captured| c.finish_captured_snapshot(captured),
            ),
        )
        .await
        .unwrap();
        assert!(matches!(rejected, Err(OpenError::Stale)));
        let valid = owner.reserve(A, "same.dng".into()).unwrap();
        assert_eq!(
            capture(&owner, valid, metadata(json!({})))
                .await
                .unwrap()
                .open_key,
            valid
        );
        idle(&owner).await;
    }

    #[tokio::test]
    async fn held_capture_drains_once_and_retires_outside_gate() {
        for mode in ["caller-drop", "supersede", "close"] {
            let owner = Arc::new(owner());
            let key = owner.reserve(A, "held.dng".into()).unwrap();
            let (input, _) = assets_input();
            let dropped = Arc::new(AtomicBool::new(false));
            let unlocked = Arc::new(AtomicBool::new(false));
            let witness_dropped = Arc::clone(&dropped);
            let witness_unlocked = Arc::clone(&unlocked);
            let (entered, entered_rx) = oneshot::channel();
            let (release, release_rx) = mpsc::channel();
            let runner = Arc::clone(&owner);
            let mut job = Some(tokio::spawn(async move {
                runner
                    .execute(
                        key,
                        JobKind::Snapshot,
                        move |c| {
                            let mut package = c.capture_snapshot(metadata(input), settings())?;
                            Arc::get_mut(&mut package.spec).unwrap().drop_witness =
                                Some(SnapshotDropWitness {
                                    origin: Arc::downgrade(&c.inner),
                                    observed: witness_dropped,
                                    unlocked: witness_unlocked,
                                });
                            let assets = package.spec.creative.as_ref().unwrap().as_ref().unwrap();
                            entered
                                .send((
                                    Arc::downgrade(&package.spec),
                                    Arc::downgrade(assets.lut().unwrap()),
                                    Arc::downgrade(assets.fog_map().unwrap()),
                                ))
                                .unwrap_or_else(|_| panic!("observer lost"));
                            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                            Ok(package)
                        },
                        |c, package| c.finish_captured_snapshot(package),
                    )
                    .await
            }));
            let (held_spec, held_lut, held_map) =
                tokio::time::timeout(Duration::from_secs(2), entered_rx)
                    .await
                    .unwrap()
                    .unwrap();
            assert!(
                held_spec.upgrade().is_some()
                    && held_lut.upgrade().is_some()
                    && held_map.upgrade().is_some()
            );
            if mode == "caller-drop" {
                job.as_ref().unwrap().abort();
            } else if mode == "close" {
                owner.close();
            }
            if mode == "close" {
                assert!(matches!(
                    owner.reserve(B, "closed.dng".into()),
                    Err(OpenError::Unavailable)
                ));
                assert!(
                    owner
                        .inner
                        .gate
                        .lock()
                        .unwrap()
                        .worker
                        .as_ref()
                        .is_some_and(|w| w.key == key)
                );
                release.send(()).unwrap();
                assert!(
                    tokio::time::timeout(Duration::from_secs(3), job.take().unwrap())
                        .await
                        .unwrap()
                        .unwrap()
                        .is_err()
                );
            } else {
                if mode == "caller-drop" {
                    // Await the caller cancellation before reserving the next work.
                    match tokio::time::timeout(Duration::from_secs(3), job.take().unwrap())
                        .await
                        .unwrap()
                    {
                        Err(error) => assert!(error.is_cancelled()),
                        Ok(_) => panic!("caller was not cancelled"),
                    }
                }
                let b = owner.reserve(B, "b.dng".into()).unwrap();
                let calls = Arc::new(AtomicUsize::new(0));
                let seen = Arc::clone(&calls);
                let runner = Arc::clone(&owner);
                let waiting = tokio::spawn(async move {
                    runner
                        .execute(
                            b,
                            JobKind::Snapshot,
                            move |c| {
                                seen.fetch_add(1, Ordering::SeqCst);
                                c.capture_snapshot(metadata(json!({})), settings())
                            },
                            |c, package| c.finish_captured_snapshot(package),
                        )
                        .await
                });
                phase(&owner, b, Phase::WaitingSnapshot).await;
                let newest = owner.reserve(A, "held.dng".into()).unwrap();
                let seen = Arc::clone(&calls);
                let runner = Arc::clone(&owner);
                let latest = tokio::spawn(async move {
                    runner
                        .execute(
                            newest,
                            JobKind::Snapshot,
                            move |c| {
                                assert_eq!(seen.fetch_add(1, Ordering::SeqCst), 0);
                                c.capture_snapshot(metadata(json!({})), settings())
                            },
                            |c, package| c.finish_captured_snapshot(package),
                        )
                        .await
                });
                phase(&owner, newest, Phase::WaitingSnapshot).await;
                assert_eq!(calls.load(Ordering::SeqCst), 0);
                assert!(
                    owner
                        .inner
                        .gate
                        .lock()
                        .unwrap()
                        .worker
                        .as_ref()
                        .is_some_and(|w| w.key == key)
                );
                assert!(!dropped.load(Ordering::SeqCst));
                release.send(()).unwrap();
                if mode == "supersede" {
                    assert!(
                        tokio::time::timeout(Duration::from_secs(3), job.take().unwrap())
                            .await
                            .unwrap()
                            .unwrap()
                            .is_err()
                    );
                }
                assert!(
                    tokio::time::timeout(Duration::from_secs(3), waiting)
                        .await
                        .unwrap()
                        .unwrap()
                        .is_err()
                );
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(3), latest)
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap()
                        .open_key,
                    newest
                );
                assert_eq!(calls.load(Ordering::SeqCst), 1);
            }
            idle(&owner).await;
            assert!(dropped.load(Ordering::SeqCst));
            assert!(unlocked.load(Ordering::SeqCst));
            assert!(held_spec.upgrade().is_none());
            assert!(held_lut.upgrade().is_none());
            assert!(held_map.upgrade().is_none());
        }
        // A published spec can legitimately outlive supersession through a held
        // Arc. Its actual final destruction must still occur outside the gate.
        let owner = owner();
        let key = owner.reserve(A, "published.dng".into()).unwrap();
        let (input, _) = assets_input();
        let dropped = Arc::new(AtomicBool::new(false));
        let unlocked = Arc::new(AtomicBool::new(false));
        let seen_drop = Arc::clone(&dropped);
        let seen_unlocked = Arc::clone(&unlocked);
        tokio::time::timeout(
            Duration::from_secs(3),
            owner.execute(
                key,
                JobKind::Snapshot,
                move |c| {
                    let mut package = c.capture_snapshot(metadata(input), settings())?;
                    Arc::get_mut(&mut package.spec).unwrap().drop_witness =
                        Some(SnapshotDropWitness {
                            origin: Arc::downgrade(&c.inner),
                            observed: seen_drop,
                            unlocked: seen_unlocked,
                        });
                    Ok(package)
                },
                |c, package| c.finish_captured_snapshot(package),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let held = spec(&owner);
        let assets = held.creative.as_ref().unwrap().as_ref().unwrap();
        let lut = Arc::downgrade(assets.lut().unwrap());
        let map = Arc::downgrade(assets.fog_map().unwrap());
        owner.reserve(B, "next.dng".into()).unwrap();
        assert!(!dropped.load(Ordering::SeqCst));
        assert!(lut.upgrade().is_some() && map.upgrade().is_some());
        drop(held);
        assert!(dropped.load(Ordering::SeqCst));
        assert!(unlocked.load(Ordering::SeqCst));
        assert!(lut.upgrade().is_none() && map.upgrade().is_none());
        idle(&owner).await;
    }

    #[tokio::test]
    async fn metadata_only_helper_and_historical_tests_remain_exact() {
        let owner = owner();
        let input = metadata(
            json!({"lutPath":parent().join("absent.cube").to_str().unwrap(),
            "fogEnabled":true}),
        );
        let original = serde_json::to_value(&input).unwrap();
        let key = owner.reserve(A, "metadata-only.dng".into()).unwrap();
        let admission = tokio::time::timeout(
            Duration::from_secs(3),
            owner.execute(
                key,
                JobKind::Snapshot,
                move |_| Ok(input),
                |c, input| c.finish_snapshot(input, settings()),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(serde_json::to_value(admission.metadata).unwrap(), original);
        let held = spec(&owner);
        assert!(held.creative.is_none());
        assert_eq!(held.metadata.adjustments["fogEnabled"], json!(true));
        assert_eq!(held.settings.raw_preprocessing_color_nr, Some(0.0));
        idle(&owner).await;
    }
}
// END C3a additive captured-owner tests.
