//! Desktop termination ownership. Cleanup evidence precedes scheduling, sealing
//! precedes the single OS action. No OS success or hard cleanup deadline claimed.
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, ExitRequestApi};

use super::application::{CleanupResult, DriveApplication, FacadeError};

pub(crate) struct DesktopState {
    pub(super) application: Arc<DriveApplication>,
    coordinator: Arc<Coordinator>,
}
impl DesktopState {
    pub(crate) fn empty() -> Result<Self, FacadeError> {
        Ok(Self {
            application: Arc::new(DriveApplication::from_native_records(Vec::new())?),
            coordinator: Arc::new(Coordinator::new()),
        })
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        let observed = self.coordinator.snapshot();
        if observed.phase != Phase::Blocked && observed.fault.is_none() {
            return observed;
        }
        // The coordinator returned a COPY and released its mutex. Reconcile
        // observed fatal state before publication, never under that mutex.
        // This does not detect an unobserved future poison or prove cleanup.
        let closing = self.application.mark_closing();
        self.coordinator.closure_fault(closing.fault);
        self.coordinator.snapshot()
    }

    fn prepare_request(&self, intent: Intent) -> Result<Admission, RequestError> {
        if intent
            == (Intent::Quit {
                code: tauri::RESTART_EXIT_CODE,
            })
        {
            // Preserve validation-before-observation: this Unavailable is not
            // evidence of a fatal coordinator fault and must not close A.
            return self.coordinator.request(intent);
        }
        let admission = self.coordinator.request(intent);
        if self.snapshot().phase == Phase::Blocked {
            return Err(RequestError::Unavailable);
        }
        admission
    }

    fn prepare_exit_response(&self, code: Option<i32>) -> ExitResponse {
        let response = self.coordinator.exit_response(code);
        if self.snapshot().phase == Phase::Blocked {
            return if code == Some(tauri::RESTART_EXIT_CODE) {
                ExitResponse::ForeignReserved
            } else {
                ExitResponse::Prevent
            };
        }
        response
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum Intent {
    Quit { code: i32 },
    Restart,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Acknowledgment {
    Accepted,
    AlreadyPending,
    QuitPending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestError {
    Unavailable,
    AlreadyFinalizing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Phase {
    Open,
    Draining,
    Finalizing,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "code", rename_all = "camelCase")]
enum Fault {
    CleanupUnproven(FacadeError),
    SpawnFailed,
    RunnerFailed,
    ScheduleFailed,
    Poisoned,
    ForeignReservedExit,
    UnexpectedExit,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    phase: Phase,
    intent: Option<Intent>,
    closure_fault: Option<FacadeError>,
    cleanup_fault: Option<FacadeError>,
    fault: Option<Fault>,
}
struct State {
    snapshot: Snapshot,
    fence: bool,
    queued: bool,
}
struct Coordinator {
    state: Mutex<State>,
}
struct Admission {
    start: bool,
    acknowledgment: Acknowledgment,
}
impl Coordinator {
    fn new() -> Self {
        Self {
            state: Mutex::new(State {
                snapshot: Snapshot {
                    phase: Phase::Open,
                    intent: None,
                    closure_fault: None,
                    cleanup_fault: None,
                    fault: None,
                },
                fence: false,
                queued: false,
            }),
        }
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                // Inspection/destruction only after poison; no action can seal.
                let mut state = poison.into_inner();
                state.snapshot.phase = Phase::Blocked;
                state.snapshot.fault = Some(Fault::Poisoned);
                state
            }
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.lock().snapshot
    }
    fn request(&self, intent: Intent) -> Result<Admission, RequestError> {
        if intent
            == (Intent::Quit {
                code: tauri::RESTART_EXIT_CODE,
            })
        {
            return Err(RequestError::Unavailable);
        }
        let mut state = self.lock();
        match state.snapshot.phase {
            Phase::Open => {
                state.snapshot.phase = Phase::Draining;
                state.snapshot.intent = Some(intent);
                Ok(Admission {
                    start: true,
                    acknowledgment: Acknowledgment::Accepted,
                })
            }
            Phase::Draining => {
                let acknowledgment = match (state.snapshot.intent, intent) {
                    (Some(Intent::Restart), Intent::Quit { .. }) => {
                        state.snapshot.intent = Some(intent);
                        Acknowledgment::Accepted
                    }
                    (Some(Intent::Quit { .. }), Intent::Restart) => Acknowledgment::QuitPending,
                    _ => Acknowledgment::AlreadyPending,
                };
                Ok(Admission {
                    start: false,
                    acknowledgment,
                })
            }
            Phase::Finalizing => Err(RequestError::AlreadyFinalizing),
            Phase::Blocked => Err(RequestError::Unavailable),
        }
    }
    fn closure_fault(&self, fault: Option<FacadeError>) {
        self.lock().snapshot.closure_fault = fault;
    }
    fn block(&self, fault: Fault) {
        let mut state = self.lock();
        // A dispatched action cannot be recalled, nor may an error enable a retry.
        if state.snapshot.phase != Phase::Finalizing {
            state.snapshot.phase = Phase::Blocked;
        }
        if state.snapshot.fault.is_none() {
            state.snapshot.fault = Some(fault);
        }
    }
    fn cleanup(&self, result: CleanupResult) -> bool {
        let mut state = self.lock();
        if state.snapshot.phase != Phase::Draining || state.queued {
            return false;
        }
        match result {
            CleanupResult::CleanupComplete { fault } => {
                state.snapshot.cleanup_fault = fault;
                state.fence = true;
                state.queued = true;
                true
            }
            CleanupResult::CleanupUnproven { fault } => {
                state.snapshot.phase = Phase::Blocked;
                state.snapshot.fault = Some(Fault::CleanupUnproven(fault));
                false
            }
        }
    }
    fn seal(&self) -> Option<Intent> {
        let mut state = self.lock();
        if state.snapshot.phase != Phase::Draining || !state.fence || !state.queued {
            return None;
        }
        let action = state.snapshot.intent?;
        state.snapshot.phase = Phase::Finalizing;
        Some(action)
    }
    fn exit_response(&self, code: Option<i32>) -> ExitResponse {
        let state = self.lock();
        if state.snapshot.phase == Phase::Finalizing {
            let expected = match state.snapshot.intent {
                Some(Intent::Quit { code }) => Some(code),
                Some(Intent::Restart) => Some(tauri::RESTART_EXIT_CODE),
                None => None,
            };
            if code.is_some() && code == expected {
                return ExitResponse::Allow;
            }
        }
        if code == Some(tauri::RESTART_EXIT_CODE) {
            return ExitResponse::ForeignReserved;
        }
        match state.snapshot.phase {
            Phase::Open | Phase::Draining => ExitResponse::RequestQuit(code.unwrap_or(0)),
            Phase::Finalizing | Phase::Blocked => ExitResponse::Prevent,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum ExitResponse {
    Allow,
    Prevent,
    RequestQuit(i32),
    ForeignReserved,
}

// Created BEFORE spawn/future construction. A failed spawn or never-polled
// future owns this guard already. Once polled it also covers unwind/cancellation.
struct RunnerGuard {
    coordinator: Arc<Coordinator>,
    fault: Fault,
    armed: bool,
}
impl RunnerGuard {
    fn new(coordinator: Arc<Coordinator>) -> Self {
        Self {
            coordinator,
            fault: Fault::SpawnFailed,
            armed: true,
        }
    }
    fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for RunnerGuard {
    fn drop(&mut self) {
        if self.armed {
            self.coordinator.block(self.fault);
        }
    }
}

pub(super) fn request(
    state: &DesktopState,
    app: &AppHandle,
    intent: Intent,
) -> Result<Acknowledgment, RequestError> {
    let admission = state.prepare_request(intent)?;
    if !admission.start {
        return Ok(admission.acknowledgment);
    }
    let guard = RunnerGuard::new(Arc::clone(&state.coordinator));
    // No coordinator lock is held across native invalidation or task startup.
    let closing = state.application.mark_closing();
    state.coordinator.closure_fault(closing.fault);
    let application = Arc::clone(&state.application);
    let scheduler = app.clone();
    let actor = app.clone();
    let future = drain_and_queue(
        guard,
        async move { application.shutdown().await },
        move |finalizer| scheduler.run_on_main_thread(finalizer).map_err(|_| ()),
        move |intent| match intent {
            Intent::Quit { code } => actor.exit(code),
            Intent::Restart => actor.request_restart(),
        },
    );
    if catch_unwind(AssertUnwindSafe(|| {
        // Runtime owns this ONE runner; no abort/JoinHandle is exposed or stored.
        drop(tauri::async_runtime::spawn(future));
    }))
    .is_err()
    {
        state.coordinator.block(Fault::SpawnFailed);
        return Err(RequestError::Unavailable);
    }
    Ok(admission.acknowledgment)
}

async fn drain_and_queue<C, Q, A>(mut guard: RunnerGuard, cleanup: C, queue: Q, action: A)
where
    C: Future<Output = CleanupResult>,
    Q: FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), ()>,
    A: FnOnce(Intent) + Send + 'static,
{
    guard.fault = Fault::RunnerFailed;
    let result = cleanup.await;
    if !guard.coordinator.cleanup(result) {
        guard.disarm();
        return;
    }
    let coordinator = Arc::clone(&guard.coordinator);
    guard.fault = Fault::ScheduleFailed;
    // Queueing is not sealing: normal quit can still replace restart here.
    let finalizer = Box::new(move || {
        if let Some(intent) = coordinator.seal() {
            action(intent);
        }
    });
    if queue(finalizer).is_err() {
        guard.coordinator.block(Fault::ScheduleFailed);
    }
    guard.disarm();
}

pub(crate) fn on_exit_requested(
    state: &DesktopState,
    app: &AppHandle,
    code: Option<i32>,
    api: &ExitRequestApi,
) {
    match state.prepare_exit_response(code) {
        ExitResponse::Allow => {}
        ExitResponse::Prevent => api.prevent_exit(),
        ExitResponse::RequestQuit(code) => {
            api.prevent_exit();
            let _ = request(state, app, Intent::Quit { code });
        }
        ExitResponse::ForeignReserved => {
            // Tauri ignores prevent_exit for this sentinel. Desktop renderer
            // plugin routes are revoked; an unknown native caller is outside
            // the graceful contract, never treated as proven cleanup.
            let closing = state.application.mark_closing();
            state.coordinator.closure_fault(closing.fault);
            state.coordinator.block(Fault::ForeignReservedExit);
        }
    }
}
pub(crate) fn on_exit(state: &DesktopState) {
    if state.snapshot().phase != Phase::Finalizing {
        let closing = state.application.mark_closing();
        state.coordinator.closure_fault(closing.fault);
        state.coordinator.block(Fault::UnexpectedExit);
    }
    // Always return to Tauri's cleanup + restart_on_exit processing. No hard exit.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    fn active(intent: Intent) -> Arc<Coordinator> {
        let coordinator = Arc::new(Coordinator::new());
        assert!(coordinator.request(intent).unwrap().start);
        coordinator
    }
    fn complete() -> CleanupResult {
        CleanupResult::CleanupComplete { fault: None }
    }
    #[test]
    fn first_quit_overrides_restart_but_never_the_first_normal_code() {
        let c = active(Intent::Restart);
        assert_eq!(
            c.request(Intent::Restart).unwrap().acknowledgment,
            Acknowledgment::AlreadyPending
        );
        assert!(!c.request(Intent::Quit { code: 1 }).unwrap().start);
        assert_eq!(
            c.request(Intent::Quit { code: 0 }).unwrap().acknowledgment,
            Acknowledgment::AlreadyPending
        );
        assert_eq!(
            c.request(Intent::Restart).unwrap().acknowledgment,
            Acknowledgment::QuitPending
        );
        assert_eq!(c.snapshot().intent, Some(Intent::Quit { code: 1 }));
        let c = Coordinator::new();
        assert_eq!(c.exit_response(None), ExitResponse::RequestQuit(0));
        c.request(Intent::Quit { code: 0 }).unwrap();
        c.request(Intent::Restart).unwrap();
        assert_eq!(c.snapshot().intent, Some(Intent::Quit { code: 0 }));
        for code in [0, 1, -4, 99] {
            let c = Coordinator::new();
            assert_eq!(c.exit_response(Some(code)), ExitResponse::RequestQuit(code));
            c.request(Intent::Quit { code }).unwrap();
            c.request(Intent::Quit { code: 7 }).unwrap();
            assert_eq!(c.snapshot().intent, Some(Intent::Quit { code }));
        }
    }
    #[test]
    fn real_runner_queues_then_seals_latest_intent_and_dispatches_once() {
        let c = active(Intent::Restart);
        let order = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let cleanup_order = Arc::clone(&order);
        let queue_order = Arc::clone(&order);
        let action_order = Arc::clone(&order);
        futures::executor::block_on(drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            async move {
                cleanup_order.lock().unwrap().push("fence");
                complete()
            },
            move |finalizer| {
                queue_order.lock().unwrap().push("queue");
                tx.send(finalizer).unwrap();
                Ok(())
            },
            move |action| {
                assert_eq!(action, Intent::Quit { code: 1 });
                action_order.lock().unwrap().push("action");
            },
        ));
        assert_eq!(*order.lock().unwrap(), vec!["fence", "queue"]);
        assert_eq!(c.snapshot().phase, Phase::Draining);
        c.request(Intent::Quit { code: 1 }).unwrap();
        rx.recv().unwrap()();
        assert_eq!(*order.lock().unwrap(), vec!["fence", "queue", "action"]);
        assert!(c.seal().is_none());
        assert!(matches!(
            c.request(Intent::Restart),
            Err(RequestError::AlreadyFinalizing)
        ));
        assert_eq!(c.exit_response(Some(1)), ExitResponse::Allow);
        assert_eq!(c.exit_response(None), ExitResponse::Prevent);
        assert_eq!(c.exit_response(Some(0)), ExitResponse::Prevent);
    }
    #[test]
    fn every_static_cleanup_fault_retains_provenance_unproven_never_queues() {
        for fault in [
            FacadeError::InvalidRequest,
            FacadeError::NotConfigured,
            FacadeError::UnknownRegistration,
            FacadeError::DisallowedCapabilities,
            FacadeError::Busy,
            FacadeError::Closing,
            FacadeError::Closed,
            FacadeError::Stale,
            FacadeError::TimedOut,
            FacadeError::Cancelled,
            FacadeError::LaunchFailed,
            FacadeError::WorkerFailure,
            FacadeError::AttemptEnded,
            FacadeError::ControllerFault,
        ] {
            let c = active(Intent::Restart);
            c.closure_fault(Some(FacadeError::ControllerFault));
            assert!(c.cleanup(CleanupResult::CleanupComplete { fault: Some(fault) }));
            assert_eq!(c.snapshot().cleanup_fault, Some(fault));
            assert_eq!(
                c.snapshot().closure_fault,
                Some(FacadeError::ControllerFault)
            );
            assert_eq!(c.seal(), Some(Intent::Restart));
            assert!(!c.cleanup(complete()));
            let c = active(Intent::Restart);
            assert!(!c.cleanup(CleanupResult::CleanupUnproven { fault }));
            assert_eq!(c.snapshot().fault, Some(Fault::CleanupUnproven(fault)));
            assert!(c.seal().is_none());
        }
    }
    #[test]
    fn guard_covers_failed_and_unpolled_startup_and_polled_drop() {
        let c = active(Intent::Restart);
        drop(RunnerGuard::new(Arc::clone(&c)));
        assert_eq!(c.snapshot().fault, Some(Fault::SpawnFailed));
        let c = active(Intent::Restart);
        let future = drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            std::future::pending(),
            |_| panic!("not queued"),
            |_| panic!("not acted"),
        );
        drop(future);
        assert_eq!(c.snapshot().fault, Some(Fault::SpawnFailed));
        let c = active(Intent::Restart);
        futures::executor::block_on(async {
            let mut future = Box::pin(drain_and_queue(
                RunnerGuard::new(Arc::clone(&c)),
                std::future::pending(),
                |_| panic!("not queued"),
                |_| panic!("not acted"),
            ));
            assert!(futures::poll!(&mut future).is_pending());
            drop(future);
        });
        assert_eq!(c.snapshot().fault, Some(Fault::RunnerFailed));
        assert!(c.seal().is_none());
        assert!(matches!(
            c.request(Intent::Quit { code: 0 }),
            Err(RequestError::Unavailable)
        ));
    }
    #[test]
    fn runner_and_scheduler_panics_or_errors_never_become_cleanup_actions() {
        let c = active(Intent::Restart);
        assert!(
            catch_unwind(AssertUnwindSafe(|| futures::executor::block_on(
                drain_and_queue(
                    RunnerGuard::new(Arc::clone(&c)),
                    async { panic!("synthetic runner failure") },
                    |_| Ok(()),
                    |_| panic!("not acted")
                )
            )))
            .is_err()
        );
        assert_eq!(c.snapshot().fault, Some(Fault::RunnerFailed));
        for panic_queue in [false, true] {
            let c = active(Intent::Restart);
            let _ = catch_unwind(AssertUnwindSafe(|| {
                futures::executor::block_on(drain_and_queue(
                    RunnerGuard::new(Arc::clone(&c)),
                    async { complete() },
                    move |_| {
                        if panic_queue {
                            panic!("synthetic scheduling failure");
                        }
                        Err(())
                    },
                    |_| panic!("not acted"),
                ))
            }));
            assert_eq!(c.snapshot().fault, Some(Fault::ScheduleFailed));
            assert_eq!(c.snapshot().phase, Phase::Blocked);
            assert!(c.seal().is_none());
        }
    }
    #[test]
    fn poison_fails_closed_and_queued_but_never_run_does_not_seal() {
        let c = active(Intent::Restart);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _lock = c.state.lock().unwrap();
            panic!("synthetic poison");
        }));
        assert_eq!(c.snapshot().phase, Phase::Blocked);
        assert_eq!(c.snapshot().fault, Some(Fault::Poisoned));
        assert!(!c.cleanup(complete()));
        assert!(c.seal().is_none());
        let c = active(Intent::Restart);
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = Arc::clone(&count);
        futures::executor::block_on(drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            async { complete() },
            |finalizer| {
                drop(finalizer);
                Ok(())
            },
            move |_| {
                count2.fetch_add(1, Ordering::SeqCst);
            },
        ));
        assert_eq!(c.snapshot().phase, Phase::Draining);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn recursive_codes_require_exact_sealed_action_and_foreign_sentinel_is_distinct() {
        let c = Coordinator::new();
        assert_eq!(
            c.exit_response(Some(tauri::RESTART_EXIT_CODE)),
            ExitResponse::ForeignReserved
        );
        assert!(matches!(
            c.request(Intent::Quit {
                code: tauri::RESTART_EXIT_CODE
            }),
            Err(RequestError::Unavailable)
        ));
        assert_eq!(c.snapshot().phase, Phase::Open);
        let c = active(Intent::Restart);
        assert_eq!(
            c.exit_response(Some(tauri::RESTART_EXIT_CODE)),
            ExitResponse::ForeignReserved
        );
        assert!(c.cleanup(complete()));
        assert_eq!(c.seal(), Some(Intent::Restart));
        assert_eq!(
            c.exit_response(Some(tauri::RESTART_EXIT_CODE)),
            ExitResponse::Allow
        );
        assert_eq!(c.exit_response(Some(0)), ExitResponse::Prevent);
        assert_eq!(c.exit_response(None), ExitResponse::Prevent);
        c.block(Fault::ForeignReservedExit);
        assert_eq!(c.snapshot().phase, Phase::Finalizing);
        assert_eq!(c.snapshot().fault, Some(Fault::ForeignReservedExit));
        assert!(c.seal().is_none());
    }
    #[tokio::test]
    async fn actual_empty_facade_shutdown_precedes_queue_without_an_owner_cycle() {
        let state = DesktopState::empty().unwrap();
        let weak = Arc::downgrade(&state.application);
        let c = Arc::clone(&state.coordinator);
        c.request(Intent::Quit { code: 0 }).unwrap();
        assert!(state.application.mark_closing().first);
        let application = Arc::clone(&state.application);
        let count = Arc::new(AtomicUsize::new(0));
        let action_count = Arc::clone(&count);
        drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            async move { application.shutdown().await },
            |finalizer| {
                finalizer();
                Ok(())
            },
            move |intent| {
                assert_eq!(intent, Intent::Quit { code: 0 });
                action_count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(c.snapshot().phase, Phase::Finalizing);
        drop(state);
        assert!(weak.upgrade().is_none());
    }
    #[tokio::test]
    async fn public_session_held_ticket_fence_blocks_the_real_runner_queue() {
        // A intentionally exposes no ticket seam. This is a public S1 fixture,
        // not a claim of a held-ticket DriveApplication integration test.
        use super::super::pkce::ApprovedScope;
        use super::super::session::{NativeStart, SessionController};
        let controller = Arc::new(SessionController::new());
        let start = NativeStart::new(
            "synthetic.apps.googleusercontent.com".into(),
            "synthetic".into(),
            "1".into(),
            vec![ApprovedScope::AppData],
        )
        .unwrap();
        let ticket = tokio::time::timeout(Duration::from_secs(4), controller.begin(start))
            .await
            .unwrap()
            .unwrap();
        let c = active(Intent::Restart);
        let count = Arc::new(AtomicUsize::new(0));
        let queued = Arc::clone(&count);
        let shutdown_owner = Arc::clone(&controller);
        let mut runner = Box::pin(drain_and_queue(
            RunnerGuard::new(Arc::clone(&c)),
            async move {
                shutdown_owner
                    .shutdown()
                    .await
                    .expect("public fixture cleanup");
                complete()
            },
            move |finalizer| {
                queued.fetch_add(1, Ordering::SeqCst);
                finalizer();
                Ok(())
            },
            |_| {},
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut runner)
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(c.snapshot().phase, Phase::Draining);
        drop(ticket);
        tokio::time::timeout(Duration::from_secs(4), runner)
            .await
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(c.snapshot().phase, Phase::Finalizing);
    }
    #[test]
    fn finalizer_winner_and_guard_resource_ownership_are_once_only() {
        let c = active(Intent::Restart);
        let weak = Arc::downgrade(&c);
        let mut guard = RunnerGuard::new(Arc::clone(&c));
        assert!(c.cleanup(complete()));
        c.request(Intent::Quit { code: 1 }).unwrap();
        assert_eq!(c.seal(), Some(Intent::Quit { code: 1 }));
        assert!(c.seal().is_none());
        guard.disarm();
        drop(guard);
        drop(c);
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod fault_admission_tests {
    use super::*;

    const BEGIN: &str =
        r#"{"schemaVersion":1,"registrationId":"synthetic","capabilities":["stateLibrary"]}"#;
    const DECIDE: &str = r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000","generation":"0","decisionId":"00000000-0000-0000-0000-000000000000","decision":"accept"}"#;

    fn poison(state: &DesktopState) {
        // Controlled fault injection only: no ordinary production panic claimed.
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _guard = state.coordinator.state.lock().unwrap();
                panic!("synthetic coordinator poison");
            }))
            .is_err()
        );
        assert!(state.coordinator.state.is_poisoned());
        // Do not observe through coordinator/owner before the tested entry point.
    }
    fn assert_application_closed(state: &DesktopState) {
        // These are real A methods. Closing rejects before runtime/listener/
        // browser work, even though a merely open empty catalog is NotConfigured.
        assert_eq!(
            futures::executor::block_on(state.application.begin(BEGIN)),
            Err(FacadeError::Closing)
        );
        assert_eq!(state.application.decide(DECIDE), Err(FacadeError::Closing));
        assert!(!state.application.mark_closing().first);
    }
    fn assert_no_work(state: &DesktopState) {
        let inner = state.coordinator.lock();
        assert_eq!(inner.snapshot.phase, Phase::Blocked);
        assert!(!inner.fence);
        assert!(!inner.queued);
        drop(inner);
        assert!(state.coordinator.seal().is_none());
    }

    #[test]
    fn poisoned_open_request_closes_real_admission_without_starting_work() {
        let clean = DesktopState::empty().unwrap();
        assert!(matches!(
            clean.prepare_request(Intent::Quit {
                code: tauri::RESTART_EXIT_CODE
            }),
            Err(RequestError::Unavailable)
        ));
        assert_eq!(clean.snapshot().phase, Phase::Open);
        assert_eq!(
            futures::executor::block_on(clean.application.begin(BEGIN)),
            Err(FacadeError::NotConfigured)
        );

        let state = DesktopState::empty().unwrap();
        poison(&state);
        // Exactly the preparation called by production request before any
        // RunnerGuard/spawn. No AppHandle substitute or manual-close pairing.
        assert!(matches!(
            state.prepare_request(Intent::Restart),
            Err(RequestError::Unavailable)
        ));
        assert_application_closed(&state);
        assert_no_work(&state);
        let snapshot = state.snapshot();
        assert_eq!(snapshot.fault, Some(Fault::Poisoned));
        assert_eq!(snapshot.closure_fault, None); // A was not itself poisoned.
        assert_eq!(snapshot.intent, None);
        assert!(matches!(
            state.prepare_request(Intent::Quit { code: 0 }),
            Err(RequestError::Unavailable)
        ));
        assert_application_closed(&state);
    }

    #[test]
    fn first_status_observation_closes_before_publishing_fatal_state() {
        let clean = DesktopState::empty().unwrap();
        assert_eq!(clean.snapshot().phase, Phase::Open);
        assert_eq!(
            futures::executor::block_on(clean.application.begin(BEGIN)),
            Err(FacadeError::NotConfigured)
        );
        let state = DesktopState::empty().unwrap();
        poison(&state);
        // Commands' lifecycle status calls this exact owner snapshot method.
        let first = state.snapshot();
        assert_eq!(first.phase, Phase::Blocked);
        assert_eq!(first.fault, Some(Fault::Poisoned));
        assert_eq!(first.closure_fault, None);
        assert_application_closed(&state);
        for _ in 0..2 {
            let again = state.snapshot();
            assert_eq!(again.phase, first.phase);
            assert_eq!(again.fault, first.fault);
            assert_eq!(again.closure_fault, first.closure_fault);
            assert_application_closed(&state);
        }
        assert_no_work(&state);
    }

    #[test]
    fn native_exit_fault_observation_prevents_and_closes_without_new_drain() {
        for code in [None, Some(0), Some(1)] {
            let state = DesktopState::empty().unwrap();
            poison(&state);
            // Same preparation used by on_exit_requested, before touching its
            // real ExitRequestApi/AppHandle. No OS event-loop proof is claimed.
            assert_eq!(state.prepare_exit_response(code), ExitResponse::Prevent);
            assert_application_closed(&state);
            assert_no_work(&state);
            assert_eq!(state.prepare_exit_response(code), ExitResponse::Prevent);
            assert_application_closed(&state);
        }
        let state = DesktopState::empty().unwrap();
        // Controlled mid-admission boundary, before the first runner starts.
        assert!(state.prepare_request(Intent::Restart).unwrap().start);
        state.coordinator.block(Fault::RunnerFailed);
        assert_eq!(state.prepare_exit_response(None), ExitResponse::Prevent);
        assert_application_closed(&state);
        assert_no_work(&state);
        assert!(
            !state
                .coordinator
                .cleanup(CleanupResult::CleanupComplete { fault: None })
        );
        assert!(state.coordinator.seal().is_none());
        assert_eq!(state.snapshot().fault, Some(Fault::RunnerFailed));
        assert_eq!(state.snapshot().intent, Some(Intent::Restart));
        assert_application_closed(&state);
    }
}

#[cfg(test)]
#[path = "lifecycle_independent_tests.rs"]
mod lifecycle_independent_tests;
