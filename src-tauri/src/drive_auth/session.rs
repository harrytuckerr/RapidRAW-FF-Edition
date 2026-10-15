//! One native authorization owner, ending at pending account verification.
//!
//! No browser/IPC/account verification, persistence, refresh or Drive readiness.
//! Run `begin`/`shutdown` on a native async runtime. Socket binding and pumping
//! run on one owned blocking worker. Cancellation is cooperative: the slot stays
//! Draining until that worker returns, including after a startup deadline. Drop
//! signals cleanup; it does not synchronously join or abort an OS operation.
//! Secret-bearing owners have no Debug, Display, Clone or serde implementation.
//! Ordinary String drop is not guaranteed zeroization.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::FutureExt;
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

use super::exchange::{ExchangeAttempt, ExchangeResult, exchange_code};
use super::loopback::{Cancellation, LoopbackError, LoopbackSession};
use super::pkce::{ApprovedScope, CallbackOutcome};

const ATTEMPT_LIFETIME: Duration = Duration::from_secs(300);
const STARTUP_LIFETIME: Duration = Duration::from_secs(5);
const HANDOFF_LIFETIME: Duration = Duration::from_secs(10);

/// Trusted native configuration, not authenticated project/account membership.
/// It cannot carry a caller-supplied authorization URL, token or endpoint.
pub struct NativeStart {
    client_id: String,
    registration_id: String,
    project_number: String,
    scopes: Vec<ApprovedScope>,
    #[cfg(test)]
    startup_gate: Option<TestStartupGate>,
}
impl NativeStart {
    pub fn new(
        client_id: String,
        registration_id: String,
        project_number: String,
        scopes: Vec<ApprovedScope>,
    ) -> Result<Self, StartError> {
        let request = Self {
            client_id,
            registration_id,
            project_number,
            scopes,
            #[cfg(test)]
            startup_gate: None,
        };
        if !request.valid() {
            return Err(StartError::InvalidRegistration);
        }
        Ok(request)
    }

    fn valid(&self) -> bool {
        let identifier = |s: &str, max| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        };
        identifier(&self.client_id, 256)
            && self.client_id.len() > ".apps.googleusercontent.com".len()
            && self.client_id.ends_with(".apps.googleusercontent.com")
            && identifier(&self.registration_id, 128)
            && !self.project_number.is_empty()
            && self.project_number.len() <= 32
            && !self.project_number.starts_with('0')
            && self.project_number.bytes().all(|b| b.is_ascii_digit())
            && matches!(self.scopes.len(), 1 | 2)
            && (self.scopes.len() == 1 || self.scopes[0] != self.scopes[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerError {
    Closed,
    Poisoned,
    CounterExhausted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    InvalidRegistration,
    Busy,
    Controller(ControllerError),
    Terminated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketError {
    Stale,
    Expired,
    Controller(ControllerError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserHandoff {
    HandedOff,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectOrSwitch {
    Disconnect,
    SwitchAccount,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelDisposition {
    Cancelled,
    NotCurrent,
    AlreadyTerminal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    AwaitingBrowser,
    AwaitingCallback,
    Exchanging,
    AccountVerificationPending,
    Draining,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationOutcome {
    CompletedPendingVerification,
    Denied,
    ProviderRejected,
    Cancelled,
    TimedOut,
    BrowserUnavailable,
    SetupFailure,
    ListenerFailure,
    ExchangeFailure,
    WorkerFailure,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalRecord {
    pub attempt_id: Uuid,
    pub generation: u64,
    pub outcome: AuthorizationOutcome,
}
/// Metadata only. Requested scopes are NOT granted capabilities or identity.
/// Counters remain native u64; a later IPC adapter must use decimal strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSnapshot {
    pub generation: u64,
    pub phase: Phase,
    pub attempt_id: Option<Uuid>,
    pub last_authorization: Option<TerminalRecord>,
    pub granted_subset_pending_verification: Vec<ApprovedScope>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    id: Uuid,
    generation: u64,
}
struct Slot {
    key: Key,
    admitted: Instant,
    deadline: Instant,
    phase: Phase,
    handoff_deadline: Option<Instant>,
    terminal: Option<AuthorizationOutcome>,
    registration: NativeStart,
    stop: watch::Sender<bool>,
    listener: Option<Cancellation>,
    pending: Option<Pending>,
}
struct State {
    closed: bool,
    fault: Option<ControllerError>,
    generation: u64,
    slot: Option<Slot>,
    last: Option<TerminalRecord>,
}
struct Inner {
    state: Mutex<State>,
    changed: watch::Sender<()>,
}
/// Tasks hold Inner, never this owner: task references cannot postpone Drop.
pub struct SessionController {
    inner: Arc<Inner>,
}

// No public or production fake-result constructor. Test markers exercise the
// same adoption reducer without manufacturing account proof or Google traffic.
enum Pending {
    Exchange(ExchangeResult),
    #[cfg(test)]
    Marker {
        key: Key,
        expires: Instant,
        scopes: Vec<ApprovedScope>,
    },
}
impl Pending {
    fn key(&self) -> Key {
        match self {
            Self::Exchange(value) => Key {
                id: value.attempt().id,
                generation: value.attempt().generation,
            },
            #[cfg(test)]
            Self::Marker { key, .. } => *key,
        }
    }
    fn expires(&self) -> Instant {
        match self {
            Self::Exchange(value) => value.tokens_for_native_controller().expires_at(),
            #[cfg(test)]
            Self::Marker { expires, .. } => *expires,
        }
    }
    fn usable(&self, now: Instant) -> bool {
        match self {
            Self::Exchange(value) => value.tokens_for_native_controller().is_usable_at(now),
            #[cfg(test)]
            Self::Marker { expires, .. } => now < *expires,
        }
    }
    fn scopes(&self) -> &[ApprovedScope] {
        match self {
            Self::Exchange(value) => value.tokens_for_native_controller().granted_scopes(),
            #[cfg(test)]
            Self::Marker { scopes, .. } => scopes,
        }
    }
}

impl Default for SessionController {
    fn default() -> Self {
        Self::new()
    }
}
impl SessionController {
    pub fn new() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    closed: false,
                    fault: None,
                    generation: 0,
                    slot: None,
                    last: None,
                }),
                changed,
            }),
        }
    }

    /// Returns only a generated native ticket after the listener worker is ready.
    /// Dropping this future after admission cancels its reservation. No browser
    /// opens here. A second caller cannot cancel or overwrite an incumbent.
    pub async fn begin(&self, request: NativeStart) -> Result<BrowserLaunchTicket, StartError> {
        let work = self.inner.reserve(request, Instant::now())?;
        let key = work.key;
        let mut reservation = BeginReservation {
            inner: Arc::clone(&self.inner),
            key,
            armed: true,
        };
        let (ticket_tx, ticket_rx) = oneshot::channel();
        let inner = Arc::clone(&self.inner);
        // Exactly one supervised task per occupied slot. Its blocking handle is
        // retained inside supervise; shutdown observes the cleanup fence, not an
        // abort flag. No public handle allows callers to abort the supervisor.
        tauri::async_runtime::spawn(supervise(inner, work, ticket_tx));
        let ticket = ticket_rx.await.map_err(|_| StartError::Terminated)?;
        ticket
            .url_for_native_launcher()
            .map_err(|_| StartError::Terminated)?;
        reservation.armed = false;
        Ok(ticket)
    }

    pub fn snapshot(&self) -> Result<AuthSnapshot, ControllerError> {
        let mut state = self.inner.lock()?;
        // Enforce retention at every observation too; a delayed executor must
        // not expose an expired pending capability in a snapshot.
        expire_pending(&mut state, Instant::now());
        Ok(AuthSnapshot {
            generation: state.generation,
            phase: state.slot.as_ref().map_or(
                if state.closed {
                    Phase::Closed
                } else {
                    Phase::Idle
                },
                |slot| slot.phase,
            ),
            attempt_id: state.slot.as_ref().map(|slot| slot.key.id),
            last_authorization: state.last,
            granted_subset_pending_verification: state
                .slot
                .as_ref()
                .and_then(|slot| slot.pending.as_ref())
                .map_or_else(Vec::new, |value| value.scopes().to_vec()),
        })
    }

    pub fn cancel(&self, attempt_id: Uuid) -> Result<CancelDisposition, ControllerError> {
        let mut state = self.inner.lock()?;
        if state.closed {
            return Err(ControllerError::Closed);
        }
        let Some(slot) = state.slot.as_ref().filter(|slot| slot.key.id == attempt_id) else {
            return Ok(CancelDisposition::NotCurrent);
        };
        if slot.phase == Phase::Draining {
            return Ok(CancelDisposition::AlreadyTerminal);
        }
        advance(&mut state)?;
        terminate(&mut state, AuthorizationOutcome::Cancelled);
        self.inner.changed.send_replace(());
        Ok(CancelDisposition::Cancelled)
    }

    /// Both explicit intents invalidate the generation; neither revokes remote
    /// access nor touches local edits. There is no verified session in S1.
    pub fn invalidate(&self, _reason: DisconnectOrSwitch) -> Result<(), ControllerError> {
        let mut state = self.inner.lock()?;
        if state.closed {
            return Err(ControllerError::Closed);
        }
        advance(&mut state)?;
        terminate(&mut state, AuthorizationOutcome::Cancelled);
        self.inner.changed.send_replace(());
        Ok(())
    }

    /// Closes admission and waits for actual worker cleanup outside the lock.
    /// Not hard-time-bounded if an OS blocking call does not return. Dropping
    /// this future does not undo closure or stop the existing cleanup supervisor.
    pub async fn shutdown(&self) -> Result<(), ControllerError> {
        let mut changed = self.inner.changed.subscribe();
        self.inner.close();
        loop {
            let (empty, fault) = {
                // Poison recovery here permits ONLY inspection/destruction,
                // never new work or credential adoption.
                let state = match self.inner.state.lock() {
                    Ok(state) => state,
                    Err(poison) => poison.into_inner(),
                };
                (state.slot.is_none(), state.fault)
            };
            if empty {
                return fault.map_or(Ok(()), Err);
            }
            if changed.changed().await.is_err() {
                return Err(ControllerError::Closed);
            }
        }
    }
}
impl Drop for SessionController {
    fn drop(&mut self) {
        self.inner.close();
    }
}

struct BeginReservation {
    inner: Arc<Inner>,
    key: Key,
    armed: bool,
}
impl Drop for BeginReservation {
    fn drop(&mut self) {
        if self.armed {
            self.inner.cancel_key(self.key);
        }
    }
}

/// One-shot native browser handoff. Never send this URL through React/IPC/logs.
/// The future OS adapter must retain its operation until acknowledgment/drop;
/// cancellation cannot force a browser window to close or undo an OS launch.
pub struct BrowserLaunchTicket {
    inner: Arc<Inner>,
    key: Key,
    url: String,
    ack: Option<oneshot::Sender<BrowserHandoff>>,
}
impl BrowserLaunchTicket {
    pub fn url_for_native_launcher(&self) -> Result<&str, TicketError> {
        self.inner.check_ticket(self.key, Instant::now())?;
        Ok(&self.url)
    }

    pub fn finish(mut self, result: BrowserHandoff) -> Result<(), TicketError> {
        let ack = self.ack.take().ok_or(TicketError::Stale)?;
        self.inner.acknowledge(self.key, result, Instant::now())?;
        if ack.send(result).is_err() {
            self.inner
                .finish(self.key, AuthorizationOutcome::BrowserUnavailable);
            return Err(TicketError::Stale);
        }
        Ok(())
    }
}
impl Drop for BrowserLaunchTicket {
    fn drop(&mut self) {
        if self.ack.take().is_some() {
            self.inner
                .finish(self.key, AuthorizationOutcome::BrowserUnavailable);
        }
    }
}

struct Work {
    key: Key,
    deadline: Instant,
    startup_deadline: Instant,
    bind: BindConfiguration,
    stop: watch::Receiver<bool>,
}
struct BindConfiguration {
    client_id: String,
    scopes: Vec<ApprovedScope>,
    #[cfg(test)]
    startup_gate: Option<TestStartupGate>,
}
struct Ready {
    url: String,
}
type Worker = tauri::async_runtime::JoinHandle<Result<CallbackOutcome, AuthorizationOutcome>>;

impl Inner {
    fn lock(&self) -> Result<MutexGuard<'_, State>, ControllerError> {
        match self.state.lock() {
            Ok(state) => {
                if let Some(error) = state.fault {
                    Err(error)
                } else {
                    Ok(state)
                }
            }
            Err(poison) => {
                let mut state = poison.into_inner();
                fault(&mut state, ControllerError::Poisoned);
                self.changed.send_replace(());
                Err(ControllerError::Poisoned)
            }
        }
    }

    fn reserve(&self, request: NativeStart, now: Instant) -> Result<Work, StartError> {
        if !request.valid() {
            return Err(StartError::InvalidRegistration);
        }
        // Entropy acquisition is outside the commit lock. Even a Busy request
        // cannot hold the incumbent lock across an OS randomness operation.
        let attempt_id = Uuid::new_v4();
        let mut state = self.lock().map_err(StartError::Controller)?;
        if state.closed {
            return Err(StartError::Controller(ControllerError::Closed));
        }
        if state.slot.is_some() {
            return Err(StartError::Busy);
        }
        let deadline = now.checked_add(ATTEMPT_LIFETIME).ok_or_else(|| {
            fault(&mut state, ControllerError::CounterExhausted);
            StartError::Controller(ControllerError::CounterExhausted)
        })?;
        let startup_deadline = capped_deadline(now, STARTUP_LIFETIME, deadline);
        advance(&mut state).map_err(StartError::Controller)?;
        let key = Key {
            id: attempt_id,
            generation: state.generation,
        };
        let (stop, stop_rx) = watch::channel(false);
        #[cfg(test)]
        let mut request = request;
        let bind = BindConfiguration {
            client_id: request.client_id.clone(),
            scopes: request.scopes.clone(),
            #[cfg(test)]
            startup_gate: request.startup_gate.take(),
        };
        state.slot = Some(Slot {
            key,
            admitted: now,
            deadline,
            phase: Phase::Starting,
            handoff_deadline: None,
            terminal: None,
            registration: request,
            stop,
            listener: None,
            pending: None,
        });
        self.changed.send_replace(());
        Ok(Work {
            key,
            deadline,
            startup_deadline,
            bind,
            stop: stop_rx,
        })
    }

    fn close(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                fault(&mut state, ControllerError::Poisoned);
                state
            }
        };
        if !state.closed {
            let _ = advance(&mut state);
            state.closed = true;
            terminate(&mut state, AuthorizationOutcome::Closed);
        }
        self.changed.send_replace(());
    }

    fn cancel_key(&self, key: Key) {
        if let Ok(mut state) = self.lock()
            && current(&state, key)
            && state
                .slot
                .as_ref()
                .is_some_and(|slot| slot.phase != Phase::Draining)
        {
            let _ = advance(&mut state);
            terminate(&mut state, AuthorizationOutcome::Cancelled);
            self.changed.send_replace(());
        }
    }

    fn finish(&self, key: Key, outcome: AuthorizationOutcome) {
        if let Ok(mut state) = self.lock()
            && current(&state, key)
        {
            let outcome = if state
                .slot
                .as_ref()
                .is_some_and(|slot| Instant::now() >= slot.deadline)
            {
                AuthorizationOutcome::TimedOut
            } else {
                outcome
            };
            terminate(&mut state, outcome);
            self.changed.send_replace(());
        }
    }

    fn install_listener(&self, key: Key, cancel: Cancellation, now: Instant) -> bool {
        let Ok(mut state) = self.lock() else {
            cancel.cancel();
            return false;
        };
        let now = now.max(Instant::now());
        if live(&state, key, Phase::Starting, now) {
            if let Some(slot) = state.slot.as_mut() {
                slot.listener = Some(cancel);
            }
            true
        } else {
            cancel.cancel();
            false
        }
    }

    fn offer(&self, key: Key, now: Instant) -> Result<Instant, AuthorizationOutcome> {
        let mut state = self.lock().map_err(|_| AuthorizationOutcome::Closed)?;
        let now = now.max(Instant::now());
        if !live(&state, key, Phase::Starting, now) {
            return Err(AuthorizationOutcome::TimedOut);
        }
        let slot = state
            .slot
            .as_mut()
            .ok_or(AuthorizationOutcome::WorkerFailure)?;
        let deadline = capped_deadline(now, HANDOFF_LIFETIME, slot.deadline);
        slot.handoff_deadline = Some(deadline);
        slot.phase = Phase::AwaitingBrowser;
        self.changed.send_replace(());
        Ok(deadline)
    }

    fn check_ticket(&self, key: Key, now: Instant) -> Result<(), TicketError> {
        let mut state = self.lock().map_err(TicketError::Controller)?;
        check_ticket_locked(&mut state, key, now.max(Instant::now()))
    }

    fn acknowledge(
        &self,
        key: Key,
        result: BrowserHandoff,
        now: Instant,
    ) -> Result<(), TicketError> {
        let mut state = self.lock().map_err(TicketError::Controller)?;
        check_ticket_locked(&mut state, key, now.max(Instant::now()))?;
        if result == BrowserHandoff::Failed {
            terminate(&mut state, AuthorizationOutcome::BrowserUnavailable);
        } else if let Some(slot) = state.slot.as_mut() {
            slot.phase = Phase::AwaitingCallback;
        }
        self.changed.send_replace(());
        Ok(())
    }

    fn exchanging(&self, key: Key, now: Instant) -> bool {
        let Ok(mut state) = self.lock() else {
            return false;
        };
        let now = now.max(Instant::now());
        if !live(&state, key, Phase::AwaitingCallback, now) {
            return false;
        }
        if let Some(slot) = state.slot.as_mut() {
            slot.phase = Phase::Exchanging;
        }
        self.changed.send_replace(());
        true
    }

    fn adopt_pending(&self, key: Key, value: Pending, now: Instant) -> Option<Instant> {
        let mut state = self.lock().ok()?;
        let now = now.max(Instant::now());
        if !live(&state, key, Phase::Exchanging, now) || value.key() != key || !value.usable(now) {
            return None;
        }
        let slot = state.slot.as_mut()?;
        if value.scopes().is_empty()
            || value.scopes().len() > 2
            || (value.scopes().len() == 2 && value.scopes()[0] == value.scopes()[1])
            || value
                .scopes()
                .iter()
                .any(|scope| !slot.registration.scopes.contains(scope))
        {
            return None;
        }
        let expires = value.expires().min(slot.deadline);
        slot.pending = Some(value);
        slot.phase = Phase::AccountVerificationPending;
        slot.terminal = Some(AuthorizationOutcome::CompletedPendingVerification);
        state.last = Some(TerminalRecord {
            attempt_id: key.id,
            generation: key.generation,
            outcome: AuthorizationOutcome::CompletedPendingVerification,
        });
        self.changed.send_replace(());
        Some(expires)
    }

    fn discard_pending(&self, key: Key) {
        if let Ok(mut state) = self.lock()
            && state.slot.as_ref().is_some_and(|slot| slot.key == key)
        {
            // Retention changes cannot rewrite the once-only authorization result.
            terminate(&mut state, AuthorizationOutcome::TimedOut);
            self.changed.send_replace(());
        }
    }

    fn signal_listener(&self, key: Key) {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        if let Some(slot) = state.slot.as_ref().filter(|slot| slot.key == key)
            && let Some(cancel) = &slot.listener
        {
            cancel.cancel();
        }
    }

    fn reaped(&self, key: Key) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        // Match the original slot even after generation invalidation. Called
        // ONLY after the owned blocking handle and local exchange future ended.
        if state.slot.as_ref().is_some_and(|slot| slot.key == key) {
            state.slot = None;
            self.changed.send_replace(());
        }
    }
}

fn advance(state: &mut State) -> Result<(), ControllerError> {
    match state.generation.checked_add(1) {
        Some(next) => {
            state.generation = next;
            Ok(())
        }
        None => {
            fault(state, ControllerError::CounterExhausted);
            Err(ControllerError::CounterExhausted)
        }
    }
}
fn fault(state: &mut State, error: ControllerError) {
    state.closed = true;
    state.fault = Some(error);
    terminate(state, AuthorizationOutcome::Closed);
}
fn current(state: &State, key: Key) -> bool {
    !state.closed
        && state.generation == key.generation
        && state.slot.as_ref().is_some_and(|slot| slot.key == key)
}
fn live(state: &State, key: Key, phase: Phase, now: Instant) -> bool {
    current(state, key)
        && state.slot.as_ref().is_some_and(|slot| {
            slot.phase == phase
                && slot.terminal.is_none()
                && now >= slot.admitted
                && now < slot.deadline
        })
}
fn terminate(state: &mut State, outcome: AuthorizationOutcome) {
    if let Some(slot) = state.slot.as_mut() {
        if slot.terminal.is_none() {
            slot.terminal = Some(outcome);
            state.last = Some(TerminalRecord {
                attempt_id: slot.key.id,
                generation: slot.key.generation,
                outcome,
            });
        }
        slot.pending = None;
        slot.phase = Phase::Draining;
        slot.stop.send_replace(true);
        if let Some(cancel) = &slot.listener {
            cancel.cancel();
        }
    }
}
fn expire_pending(state: &mut State, now: Instant) {
    if state.slot.as_ref().is_some_and(|slot| {
        slot.pending
            .as_ref()
            .is_some_and(|value| now >= slot.deadline || !value.usable(now))
    }) {
        terminate(state, AuthorizationOutcome::TimedOut);
    }
}
fn check_ticket_locked(state: &mut State, key: Key, now: Instant) -> Result<(), TicketError> {
    if !current(state, key)
        || !state
            .slot
            .as_ref()
            .is_some_and(|slot| slot.phase == Phase::AwaitingBrowser)
    {
        return Err(TicketError::Stale);
    }
    let slot = state.slot.as_ref().ok_or(TicketError::Stale)?;
    if now < slot.admitted
        || now >= slot.deadline
        || slot.handoff_deadline.is_none_or(|end| now >= end)
    {
        terminate(state, AuthorizationOutcome::TimedOut);
        return Err(TicketError::Expired);
    }
    Ok(())
}
fn capped_deadline(now: Instant, duration: Duration, cap: Instant) -> Instant {
    now.checked_add(duration)
        .map_or(cap, |value| value.min(cap))
}
async fn until(deadline: Instant) {
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
}
async fn stopped(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

fn bind_worker(
    inner: Arc<Inner>,
    key: Key,
    config: BindConfiguration,
    deadline: Instant,
    ready: oneshot::Sender<Ready>,
) -> Result<CallbackOutcome, AuthorizationOutcome> {
    #[cfg(test)]
    if let Some(gate) = config.startup_gate {
        let _ = gate.entered.send(());
        gate.release
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| AuthorizationOutcome::SetupFailure)?;
    }
    let now = Instant::now();
    {
        let state = inner.lock().map_err(|_| AuthorizationOutcome::Closed)?;
        if !live(&state, key, Phase::Starting, now) {
            return Err(AuthorizationOutcome::Cancelled);
        }
    }
    let scopes: Vec<_> = config.scopes.iter().map(|scope| scope.as_str()).collect();
    let lifetime = deadline
        .checked_duration_since(now)
        .ok_or(AuthorizationOutcome::TimedOut)?;
    let mut session = LoopbackSession::bind(&config.client_id, &scopes, lifetime)
        .map_err(|_| AuthorizationOutcome::SetupFailure)?;
    let url = session
        .authorization_url_for_browser()
        .map_err(|_| AuthorizationOutcome::SetupFailure)?
        .to_owned();
    if !inner.install_listener(key, session.cancellation(), Instant::now()) {
        return Err(AuthorizationOutcome::Cancelled);
    }
    // This acknowledgment is on the actual blocking worker immediately before
    // it enters the pump. No fictional helper readiness API is assumed.
    if ready.send(Ready { url }).is_err() {
        return Err(AuthorizationOutcome::Cancelled);
    }
    session.wait_on_worker().map_err(|error| match error {
        LoopbackError::Cancelled => AuthorizationOutcome::Cancelled,
        LoopbackError::TimedOut => AuthorizationOutcome::TimedOut,
        _ => AuthorizationOutcome::ListenerFailure,
    })
}

async fn join_worker(worker: &mut Option<Worker>) -> Result<CallbackOutcome, AuthorizationOutcome> {
    let handle = worker.as_mut().ok_or(AuthorizationOutcome::WorkerFailure)?;
    let result = handle.await;
    drop(worker.take());
    result.map_err(|_| AuthorizationOutcome::WorkerFailure)?
}

async fn receive_ack(
    acknowledgment: &mut Option<oneshot::Receiver<BrowserHandoff>>,
) -> Result<BrowserHandoff, AuthorizationOutcome> {
    let receiver = acknowledgment
        .as_mut()
        .ok_or(AuthorizationOutcome::BrowserUnavailable)?;
    let result = receiver.await;
    drop(acknowledgment.take());
    result.map_err(|_| AuthorizationOutcome::BrowserUnavailable)
}

async fn supervise(inner: Arc<Inner>, work: Work, ticket: oneshot::Sender<BrowserLaunchTicket>) {
    let key = work.key;
    let mut worker = None;
    let mut acknowledgment = None;
    // A panic in orchestration must not detach a started blocking worker and
    // silently free the slot. Catch it, signal cancellation, then join below.
    let result = AssertUnwindSafe(run_attempt(
        &inner,
        work,
        ticket,
        &mut worker,
        &mut acknowledgment,
    ))
    .catch_unwind()
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(outcome)) => inner.finish(key, outcome),
        Err(_) => inner.finish(key, AuthorizationOutcome::WorkerFailure),
    }
    inner.signal_listener(key);
    if worker.is_some() {
        let _ = join_worker(&mut worker).await;
    }
    // A timed-out/cancelled ticket can still belong to an OS launcher. Retain
    // this one cleanup slot until that owner acknowledges or drops its ticket.
    // A late acknowledgment cannot resurrect authorization, but does release
    // its resource fence. We cannot force an external launcher to return.
    if acknowledgment.is_some() {
        let _ = receive_ack(&mut acknowledgment).await;
    }
    inner.reaped(key);
}

async fn run_attempt(
    inner: &Arc<Inner>,
    mut work: Work,
    ticket: oneshot::Sender<BrowserLaunchTicket>,
    worker: &mut Option<Worker>,
    acknowledgment: &mut Option<oneshot::Receiver<BrowserHandoff>>,
) -> Result<(), AuthorizationOutcome> {
    let key = work.key;
    let (ready_tx, ready_rx) = oneshot::channel();
    let worker_inner = Arc::clone(inner);
    let deadline = work.deadline;
    *worker = Some(tauri::async_runtime::spawn_blocking(move || {
        bind_worker(worker_inner, key, work.bind, deadline, ready_tx)
    }));
    let ready = tokio::select! {
        biased;
        _ = stopped(&mut work.stop) => return Err(AuthorizationOutcome::Cancelled),
        _ = until(work.startup_deadline) => return Err(AuthorizationOutcome::TimedOut),
        ready = ready_rx => match ready {
            Ok(ready) => ready,
            Err(_) => {
                let failure = tokio::select! {
                    biased;
                    _ = stopped(&mut work.stop) => AuthorizationOutcome::Cancelled,
                    _ = until(work.startup_deadline) => AuthorizationOutcome::TimedOut,
                    result = join_worker(worker) => match result {
                        Err(outcome) => outcome,
                        Ok(_) => AuthorizationOutcome::WorkerFailure,
                    },
                };
                return Err(failure);
            }
        },
    };
    // Also check the startup subdeadline after readiness: select scheduling is
    // not a monotonic-clock commit check.
    if Instant::now() >= work.startup_deadline {
        return Err(AuthorizationOutcome::TimedOut);
    }
    let ack_deadline = inner.offer(key, Instant::now())?;
    let (ack_tx, ack_rx) = oneshot::channel();
    *acknowledgment = Some(ack_rx);
    let launch = BrowserLaunchTicket {
        inner: Arc::clone(inner),
        key,
        url: ready.url,
        ack: Some(ack_tx),
    };
    if ticket.send(launch).is_err() {
        return Err(AuthorizationOutcome::Cancelled);
    }
    // The already-running worker may finish here. Its JoinHandle retains at
    // most one outcome; no code exchange occurs until positive acknowledgment.
    let ack = tokio::select! {
        biased;
        _ = stopped(&mut work.stop) => return Err(AuthorizationOutcome::Cancelled),
        _ = until(ack_deadline) => return Err(AuthorizationOutcome::TimedOut),
        ack = receive_ack(acknowledgment) => ack?,
    };
    if ack != BrowserHandoff::HandedOff {
        return Err(AuthorizationOutcome::BrowserUnavailable);
    }
    let callback = tokio::select! {
        biased;
        _ = stopped(&mut work.stop) => return Err(AuthorizationOutcome::Cancelled),
        _ = until(deadline) => return Err(AuthorizationOutcome::TimedOut),
        outcome = join_worker(worker) => outcome?,
    };
    // A successful ticket commits AwaitingCallback under the same mutex. An
    // absolute deadline/cancellation that won later cannot commit an exchange.
    if !inner.exchanging(key, Instant::now()) {
        return Err(AuthorizationOutcome::TimedOut);
    }
    let grant = match callback {
        CallbackOutcome::Denied => return Err(AuthorizationOutcome::Denied),
        CallbackOutcome::ProviderRejected => return Err(AuthorizationOutcome::ProviderRejected),
        CallbackOutcome::Code(grant) => grant,
    };
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let operation = exchange_code(
        grant,
        ExchangeAttempt {
            id: key.id,
            generation: key.generation,
        },
        cancel_rx,
    );
    let value = tokio::select! {
        biased;
        _ = stopped(&mut work.stop) => { let _ = cancel_tx.send(()); return Err(AuthorizationOutcome::Cancelled); }
        _ = until(deadline) => { let _ = cancel_tx.send(()); return Err(AuthorizationOutcome::TimedOut); }
        result = operation => result.map_err(|_| AuthorizationOutcome::ExchangeFailure)?,
    };
    // The exchange future is owned locally, never spawned/replayed. Its sender
    // can drop now; only this guarded commit can retain returned native tokens.
    continue_pending(inner, key, Pending::Exchange(value), &mut work.stop).await
}

async fn continue_pending(
    inner: &Inner,
    key: Key,
    value: Pending,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), AuthorizationOutcome> {
    let expires = inner
        .adopt_pending(key, value, Instant::now())
        .ok_or(AuthorizationOutcome::TimedOut)?;
    retain_pending(inner, key, expires, stop).await;
    Ok(())
}

async fn retain_pending(
    inner: &Inner,
    key: Key,
    expires: Instant,
    stop: &mut watch::Receiver<bool>,
) {
    tokio::select! {
        biased;
        _ = stopped(stop) => {}
        _ = until(expires) => {}
    }
    inner.discard_pending(key);
}

#[cfg(test)]
struct TestStartupGate {
    entered: oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Barrier;

    fn request() -> NativeStart {
        NativeStart::new(
            "synthetic-desktop.apps.googleusercontent.com".into(),
            "desktop-test".into(),
            "123456789".into(),
            vec![ApprovedScope::AppData, ApprovedScope::File],
        )
        .expect("valid synthetic configuration")
    }
    fn reserved() -> (SessionController, Work) {
        let owner = SessionController::new();
        let work = owner
            .inner
            .reserve(request(), Instant::now())
            .expect("reserve");
        (owner, work)
    }
    fn exchanging(owner: &SessionController, work: &Work) {
        owner.inner.offer(work.key, Instant::now()).expect("offer");
        owner
            .inner
            .acknowledge(work.key, BrowserHandoff::HandedOff, Instant::now())
            .expect("ack");
        assert!(owner.inner.exchanging(work.key, Instant::now()));
    }
    fn marker(work: &Work, expires: Instant) -> Pending {
        Pending::Marker {
            key: work.key,
            expires,
            scopes: vec![ApprovedScope::File],
        }
    }
    fn gate() -> (
        TestStartupGate,
        oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered, seen) = oneshot::channel();
        let (release, held) = std::sync::mpsc::channel();
        (
            TestStartupGate {
                entered,
                release: held,
            },
            seen,
            release,
        )
    }
    async fn idle(owner: &SessionController) {
        let mut changes = owner.inner.changed.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if owner.snapshot().expect("snapshot").attempt_id.is_none() {
                    return;
                }
                changes.changed().await.expect("controller retained");
            }
        })
        .await
        .expect("bounded cleanup");
    }
    async fn ticket(owner: &SessionController) -> BrowserLaunchTicket {
        tokio::time::timeout(Duration::from_secs(3), owner.begin(request()))
            .await
            .expect("bounded startup")
            .expect("native ticket")
    }
    fn callback_address(ticket: &BrowserLaunchTicket) -> (reqwest::Url, String) {
        let url = reqwest::Url::parse(ticket.url_for_native_launcher().expect("live ticket"))
            .expect("native URL");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        (
            reqwest::Url::parse(&pairs["redirect_uri"]).expect("loopback URL"),
            pairs["state"].clone(),
        )
    }
    // Synthetic localhost only. Every connect/read/write is bounded; listener
    // ownership remains with the controller so test panic/drop signals cleanup.
    async fn callback(address: reqwest::Url, state: String, code: bool) -> bool {
        tauri::async_runtime::spawn_blocking(move || {
            let port = address.port().expect("bound port");
            let socket = ([127, 0, 0, 1], port).into();
            let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(1)).expect("loopback connect");
            stream.set_read_timeout(Some(Duration::from_secs(1))).expect("read bound");
            stream.set_write_timeout(Some(Duration::from_secs(1))).expect("write bound");
            let parameter = if code { "code=synthetic-code" } else { "error=access_denied" };
            let wire = format!(
                "GET {}?state={}&{}&provider_extension=synthetic HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
                address.path(), state, parameter, port,
            );
            stream.write_all(wire.as_bytes()).expect("bounded callback write");
            let mut bytes = Vec::new();
            stream.take(4096).read_to_end(&mut bytes).expect("bounded response read");
            bytes.starts_with(b"HTTP/1.1 200")
        }).await.expect("callback worker")
    }

    #[test]
    fn native_registration_bounds_and_requested_scope_set() {
        for bad in [
            "",
            "https://client",
            "client.apps.googleusercontent.com/",
            "a\n.apps.googleusercontent.com",
        ] {
            assert!(matches!(
                NativeStart::new(
                    bad.into(),
                    "r".into(),
                    "12".into(),
                    vec![ApprovedScope::File]
                ),
                Err(StartError::InvalidRegistration)
            ));
        }
        for bad in ["", "0", "012", "1.2", "+12", " 12"] {
            assert!(matches!(
                NativeStart::new(
                    request().client_id,
                    "r".into(),
                    bad.into(),
                    vec![ApprovedScope::File]
                ),
                Err(StartError::InvalidRegistration)
            ));
        }
        for scopes in [
            vec![],
            vec![ApprovedScope::File, ApprovedScope::File],
            vec![ApprovedScope::File; 3],
        ] {
            assert!(matches!(
                NativeStart::new(request().client_id, "r".into(), "12".into(), scopes),
                Err(StartError::InvalidRegistration)
            ));
        }
        assert!(
            NativeStart::new(
                request().client_id,
                "x".repeat(129),
                "12".into(),
                vec![ApprovedScope::File]
            )
            .is_err()
        );
    }

    #[test]
    fn concurrent_admission_has_one_winner_and_busy_preserves_every_field() {
        let owner = SessionController::new();
        let barrier = Barrier::new(3);
        let results = std::thread::scope(|threads| {
            let a = threads.spawn(|| {
                barrier.wait();
                owner.inner.reserve(request(), Instant::now())
            });
            let b = threads.spawn(|| {
                barrier.wait();
                owner.inner.reserve(request(), Instant::now())
            });
            barrier.wait();
            (
                a.join().expect("first thread"),
                b.join().expect("second thread"),
            )
        });
        assert!(matches!(
            (&results.0, &results.1),
            (Ok(_), Err(StartError::Busy)) | (Err(StartError::Busy), Ok(_))
        ));
        let before = owner.snapshot().expect("snapshot");
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Busy)
        ));
        assert_eq!(owner.snapshot().expect("snapshot"), before);
    }

    #[test]
    fn reservation_drop_invalidates_but_does_not_release_unreaped_slot() {
        let (owner, work) = reserved();
        drop(BeginReservation {
            inner: Arc::clone(&owner.inner),
            key: work.key,
            armed: true,
        });
        let snapshot = owner.snapshot().expect("snapshot");
        assert_eq!(snapshot.phase, Phase::Draining);
        assert_eq!(
            snapshot.last_authorization.expect("terminal").outcome,
            AuthorizationOutcome::Cancelled
        );
        assert!(snapshot.generation > work.key.generation);
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Busy)
        ));
        owner.inner.reaped(work.key);
        let next = owner
            .inner
            .reserve(request(), Instant::now())
            .expect("released slot");
        let before = owner.snapshot().expect("snapshot");
        owner.inner.cancel_key(work.key);
        assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::NotCurrent));
        assert_eq!(owner.snapshot().expect("snapshot"), before);
        assert_ne!(next.key.id, work.key.id);
    }

    #[test]
    fn terminal_is_once_and_late_ready_cannot_resurrect() {
        let (owner, work) = reserved();
        owner.inner.finish(work.key, AuthorizationOutcome::Denied);
        let first = owner.snapshot().expect("snapshot").last_authorization;
        for outcome in [
            AuthorizationOutcome::TimedOut,
            AuthorizationOutcome::WorkerFailure,
            AuthorizationOutcome::BrowserUnavailable,
        ] {
            owner.inner.finish(work.key, outcome);
            assert_eq!(
                owner.snapshot().expect("snapshot").last_authorization,
                first
            );
        }
        assert!(owner.inner.offer(work.key, Instant::now()).is_err());
        assert_eq!(
            owner.cancel(work.key.id),
            Ok(CancelDisposition::AlreadyTerminal)
        );
        owner.inner.reaped(work.key);
        let next = owner
            .inner
            .reserve(request(), Instant::now())
            .expect("next");
        owner
            .inner
            .finish(work.key, AuthorizationOutcome::WorkerFailure);
        assert_eq!(
            owner.snapshot().expect("snapshot").attempt_id,
            Some(next.key.id)
        );
        assert_eq!(
            owner.snapshot().expect("snapshot").last_authorization,
            first
        );
    }

    #[test]
    fn absolute_deadline_and_ack_subdeadline_never_renew() {
        let (owner, work) = reserved();
        let near_end = work.deadline - Duration::from_secs(1);
        assert_eq!(owner.inner.offer(work.key, near_end), Ok(work.deadline));
        assert_eq!(
            owner.inner.check_ticket(work.key, work.deadline),
            Err(TicketError::Expired)
        );
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::TimedOut
        );
        let state = owner.inner.state.lock().expect("state");
        assert_eq!(state.slot.as_ref().expect("slot").deadline, work.deadline);
        drop(state);
        let (owner2, work2) = reserved();
        let now = Instant::now();
        let end = owner2.inner.offer(work2.key, now).expect("offer");
        assert!(end <= now + HANDOFF_LIFETIME + Duration::from_secs(1));
        assert!(end < work2.deadline);
        assert_eq!(
            owner2
                .inner
                .acknowledge(work2.key, BrowserHandoff::HandedOff, end),
            Err(TicketError::Expired)
        );
    }

    #[test]
    fn pending_partial_scope_and_expiry_do_not_create_a_second_authorization_result() {
        let (owner, work) = reserved();
        exchanging(&owner, &work);
        let until = owner
            .inner
            .adopt_pending(
                work.key,
                marker(&work, work.deadline + Duration::from_secs(600)),
                Instant::now(),
            )
            .expect("pending");
        assert_eq!(until, work.deadline);
        let pending = owner.snapshot().expect("snapshot");
        assert_eq!(pending.phase, Phase::AccountVerificationPending);
        assert_eq!(
            pending.granted_subset_pending_verification,
            vec![ApprovedScope::File]
        );
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Busy)
        ));
        {
            let mut state = owner.inner.state.lock().expect("state");
            expire_pending(&mut state, until);
            assert!(state.slot.as_ref().expect("slot").pending.is_none());
            assert_eq!(state.last, pending.last_authorization);
        }
        owner.inner.discard_pending(work.key);
        assert_eq!(
            owner.snapshot().expect("snapshot").last_authorization,
            pending.last_authorization
        );
        owner.inner.reaped(work.key);
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Idle);
    }

    #[test]
    fn pending_rejects_wrong_attempt_expiry_and_scope_without_overwriting() {
        let (owner, work) = reserved();
        exchanging(&owner, &work);
        let now = Instant::now();
        let wrong = Pending::Marker {
            key: Key {
                id: Uuid::new_v4(),
                generation: work.key.generation,
            },
            expires: work.deadline,
            scopes: vec![ApprovedScope::File],
        };
        assert!(owner.inner.adopt_pending(work.key, wrong, now).is_none());
        assert!(
            owner
                .inner
                .adopt_pending(work.key, marker(&work, now), now)
                .is_none()
        );
        assert!(
            owner
                .inner
                .adopt_pending(
                    work.key,
                    Pending::Marker {
                        key: work.key,
                        expires: work.deadline,
                        scopes: vec![]
                    },
                    now
                )
                .is_none()
        );
        assert!(
            owner
                .inner
                .adopt_pending(work.key, marker(&work, work.deadline), work.deadline)
                .is_none()
        );
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Exchanging);
        assert!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .is_none()
        );
        let owner2 = SessionController::new();
        let mut only_appdata = request();
        only_appdata.scopes = vec![ApprovedScope::AppData];
        let work2 = owner2
            .inner
            .reserve(only_appdata, Instant::now())
            .expect("appdata admission");
        exchanging(&owner2, &work2);
        assert!(
            owner2
                .inner
                .adopt_pending(work2.key, marker(&work2, work2.deadline), Instant::now())
                .is_none()
        );
        assert_eq!(
            owner2.snapshot().expect("snapshot").phase,
            Phase::Exchanging
        );
    }

    #[test]
    fn completion_races_cancel_disconnect_and_switch_under_one_lock() {
        for reason in [
            None,
            Some(DisconnectOrSwitch::Disconnect),
            Some(DisconnectOrSwitch::SwitchAccount),
        ] {
            let (owner, work) = reserved();
            exchanging(&owner, &work);
            let barrier = Barrier::new(3);
            let value = marker(&work, work.deadline);
            std::thread::scope(|threads| {
                let left = threads.spawn(|| {
                    barrier.wait();
                    owner.inner.adopt_pending(work.key, value, Instant::now())
                });
                let right = threads.spawn(|| {
                    barrier.wait();
                    match reason {
                        None => {
                            owner.cancel(work.key.id).expect("cancel");
                        }
                        Some(reason) => owner.invalidate(reason).expect("invalidate"),
                    }
                });
                barrier.wait();
                let _ = left.join().expect("completion thread");
                right.join().expect("cancellation thread");
            });
            let snapshot = owner.snapshot().expect("snapshot");
            assert_eq!(snapshot.phase, Phase::Draining);
            assert!(snapshot.granted_subset_pending_verification.is_empty());
            assert!(matches!(
                snapshot.last_authorization.expect("one terminal").outcome,
                AuthorizationOutcome::Cancelled
                    | AuthorizationOutcome::CompletedPendingVerification
            ));
            assert!(
                owner
                    .inner
                    .adopt_pending(work.key, marker(&work, work.deadline), Instant::now())
                    .is_none()
            );
            let recorded = snapshot.last_authorization;
            owner.inner.reaped(work.key);
            let next = owner
                .inner
                .reserve(request(), Instant::now())
                .expect("new slot");
            assert!(
                owner
                    .inner
                    .adopt_pending(work.key, marker(&work, work.deadline), Instant::now())
                    .is_none()
            );
            assert_eq!(
                owner.snapshot().expect("snapshot").last_authorization,
                recorded
            );
            assert_eq!(
                owner.snapshot().expect("snapshot").attempt_id,
                Some(next.key.id)
            );
        }
    }

    #[test]
    fn checked_counter_exhaustion_and_poison_close_admission() {
        let owner = SessionController::new();
        owner.inner.state.lock().expect("state").generation = u64::MAX;
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Controller(ControllerError::CounterExhausted))
        ));
        assert_eq!(owner.snapshot(), Err(ControllerError::CounterExhausted));
        let (owner2, work) = reserved();
        let inner = Arc::clone(&owner2.inner);
        assert!(
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                let _guard = inner.state.lock().expect("state");
                panic!("synthetic mutex poison");
            }))
            .is_err()
        );
        assert_eq!(owner2.snapshot(), Err(ControllerError::Poisoned));
        assert!(matches!(
            owner2.inner.reserve(request(), Instant::now()),
            Err(StartError::Controller(ControllerError::Poisoned))
        ));
        let state = inner.state.lock().unwrap_or_else(|p| p.into_inner());
        assert!(state.closed);
        assert_eq!(state.slot.as_ref().expect("fence").phase, Phase::Draining);
        drop(state);
        inner.reaped(work.key); // No worker was launched by this reducer fixture.
    }

    #[tokio::test]
    async fn dropped_begin_during_real_worker_startup_and_late_bind() {
        let owner = SessionController::new();
        let (gate, entered, release) = gate();
        let mut request = request();
        request.startup_gate = Some(gate);
        let mut begin = Box::pin(owner.begin(request));
        tokio::select! {
            biased;
            _ = &mut begin => panic!("ticket before startup released"),
            result = entered => result.expect("worker reached gate"),
            _ = tokio::time::sleep(Duration::from_secs(3)) => panic!("bounded gate"),
        }
        drop(begin);
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Draining);
        assert!(matches!(
            owner.inner.reserve(super::tests::request(), Instant::now()),
            Err(StartError::Busy)
        ));
        release.send(()).expect("release startup");
        idle(&owner).await;
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::Cancelled
        );
    }

    #[tokio::test]
    async fn startup_timeout_keeps_slot_until_actual_worker_join() {
        let owner = SessionController::new();
        let (gate, entered, release) = gate();
        let mut request = request();
        request.startup_gate = Some(gate);
        let mut work = owner
            .inner
            .reserve(request, Instant::now())
            .expect("reserve");
        work.startup_deadline = Instant::now() + Duration::from_millis(25);
        let (tx, rx) = oneshot::channel();
        let supervisor = tauri::async_runtime::spawn(supervise(Arc::clone(&owner.inner), work, tx));
        entered.await.expect("worker gate");
        assert!(
            tokio::time::timeout(Duration::from_secs(1), rx)
                .await
                .expect("startup timeout")
                .is_err()
        );
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Draining);
        assert!(matches!(
            owner.inner.reserve(super::tests::request(), Instant::now()),
            Err(StartError::Busy)
        ));
        release.send(()).expect("release");
        supervisor.await.expect("supervisor cleanup");
        idle(&owner).await;
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::TimedOut
        );
    }

    #[tokio::test]
    async fn real_callback_before_ack_is_retained_but_failure_never_exchanges() {
        let owner = SessionController::new();
        let launch = ticket(&owner).await;
        let (address, state) = callback_address(&launch);
        assert!(callback(address, state, true).await);
        assert_eq!(
            owner.snapshot().expect("snapshot").phase,
            Phase::AwaitingBrowser
        );
        // Never positively acknowledge this synthetic code: no test calls Google.
        launch
            .finish(BrowserHandoff::Failed)
            .expect("failed handoff recorded");
        idle(&owner).await;
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::BrowserUnavailable
        );
    }

    #[tokio::test]
    async fn real_listener_wrong_state_then_acknowledged_denial() {
        let owner = SessionController::new();
        let launch = ticket(&owner).await;
        let (address, state) = callback_address(&launch);
        launch.finish(BrowserHandoff::HandedOff).expect("ack");
        assert!(!callback(address.clone(), "wrong-synthetic-state".into(), false).await);
        assert_eq!(
            owner.snapshot().expect("snapshot").phase,
            Phase::AwaitingCallback
        );
        assert!(callback(address, state, false).await);
        idle(&owner).await;
        let snapshot = owner.snapshot().expect("snapshot");
        assert_eq!(
            snapshot.last_authorization.expect("terminal").outcome,
            AuthorizationOutcome::Denied
        );
        assert!(snapshot.granted_subset_pending_verification.is_empty());
    }

    #[tokio::test]
    async fn real_ticket_drop_and_stale_ticket_are_once_only() {
        let owner = SessionController::new();
        drop(ticket(&owner).await);
        idle(&owner).await;
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::BrowserUnavailable
        );
        let launch = ticket(&owner).await;
        let id = owner.snapshot().expect("snapshot").attempt_id.expect("id");
        owner.cancel(id).expect("cancel");
        let terminal = owner.snapshot().expect("snapshot").last_authorization;
        assert!(matches!(
            launch.url_for_native_launcher(),
            Err(TicketError::Stale)
        ));
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Busy)
        ));
        assert_eq!(
            launch.finish(BrowserHandoff::HandedOff),
            Err(TicketError::Stale)
        );
        idle(&owner).await;
        assert_eq!(
            owner.snapshot().expect("snapshot").last_authorization,
            terminal
        );
        let next = ticket(&owner).await;
        let snapshot = owner.snapshot().expect("snapshot");
        assert_eq!(owner.cancel(id), Ok(CancelDisposition::NotCurrent));
        assert_eq!(owner.snapshot().expect("snapshot"), snapshot);
        drop(next);
        idle(&owner).await;
    }

    #[tokio::test]
    async fn failed_late_ack_and_expired_ticket_hold_the_external_launcher_fence() {
        let owner = SessionController::new();
        let launch = ticket(&owner).await;
        let key = launch.key;
        let end = owner
            .inner
            .state
            .lock()
            .expect("state")
            .slot
            .as_ref()
            .expect("slot")
            .handoff_deadline
            .expect("deadline");
        assert_eq!(
            owner.inner.check_ticket(key, end),
            Err(TicketError::Expired)
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Draining);
        assert!(matches!(
            owner.inner.reserve(request(), Instant::now()),
            Err(StartError::Busy)
        ));
        assert_eq!(
            launch.finish(BrowserHandoff::Failed),
            Err(TicketError::Stale)
        );
        idle(&owner).await;
        assert_eq!(
            owner
                .snapshot()
                .expect("snapshot")
                .last_authorization
                .expect("terminal")
                .outcome,
            AuthorizationOutcome::TimedOut
        );
    }

    #[tokio::test]
    async fn shutdown_waits_for_ticket_and_actual_listener_cleanup_without_lock() {
        let owner = SessionController::new();
        let launch = ticket(&owner).await;
        let (address, _) = callback_address(&launch);
        let mut shutdown = Box::pin(owner.shutdown());
        tokio::select! {
            biased;
            _ = &mut shutdown => panic!("outstanding ticket must fence cleanup"),
            _ = tokio::time::sleep(Duration::from_millis(30)) => {}
        }
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Draining);
        drop(launch);
        tokio::time::timeout(Duration::from_secs(3), &mut shutdown)
            .await
            .expect("bounded cooperative shutdown")
            .expect("shutdown");
        assert_eq!(owner.snapshot().expect("snapshot").phase, Phase::Closed);
        let port = address.port().expect("port");
        // No server retains the listener after join; the private test owns this
        // ephemeral port only for this closure assertion, never a global port.
        assert!(
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(100))
                .is_err()
        );
    }

    #[tokio::test]
    async fn owner_drop_is_not_prevented_by_ticket_or_task_inner_references() {
        let owner = SessionController::new();
        let launch = ticket(&owner).await;
        let inner = Arc::clone(&owner.inner);
        let mut changed = inner.changed.subscribe();
        drop(owner);
        assert!(inner.state.lock().expect("state").closed);
        assert!(matches!(
            launch.url_for_native_launcher(),
            Err(TicketError::Stale)
        ));
        assert!(matches!(
            inner.reserve(request(), Instant::now()),
            Err(StartError::Controller(ControllerError::Closed))
        ));
        drop(launch);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if inner.state.lock().expect("state").slot.is_none() {
                    return;
                }
                changed.changed().await.expect("inner retained");
            }
        })
        .await
        .expect("owner-independent cleanup supervisor");
    }

    #[tokio::test]
    async fn blocking_worker_panic_is_typed_and_handle_reaped() {
        let mut worker: Option<Worker> = Some(tauri::async_runtime::spawn_blocking(|| {
            panic!("synthetic blocking worker failure");
        }));
        assert!(matches!(
            join_worker(&mut worker).await,
            Err(AuthorizationOutcome::WorkerFailure)
        ));
        assert!(worker.is_none());
    }

    #[test]
    fn actual_late_bound_listener_is_cancelled_and_cannot_attach_to_replacement() {
        let (owner, work) = reserved();
        let session = LoopbackSession::bind(
            &request().client_id,
            &[ApprovedScope::File.as_str()],
            Duration::from_secs(2),
        )
        .expect("synthetic local bind");
        let cancellation = session.cancellation();
        owner
            .cancel(work.key.id)
            .expect("cancel before listener installation");
        assert!(
            !owner
                .inner
                .install_listener(work.key, cancellation.clone(), Instant::now())
        );
        // Cancellation belongs to the actual already-bound session. The helper
        // checks it before accepting a socket; no provider or browser is used.
        assert!(matches!(
            session.wait_on_worker(),
            Err(LoopbackError::Cancelled)
        ));
        owner.inner.reaped(work.key);
        let replacement = owner
            .inner
            .reserve(request(), Instant::now())
            .expect("replacement");
        assert!(
            !owner
                .inner
                .install_listener(work.key, cancellation, Instant::now())
        );
        assert_eq!(
            owner.snapshot().expect("snapshot").attempt_id,
            Some(replacement.key.id)
        );
    }

    #[tokio::test]
    async fn real_pending_retention_watchdog_expires_without_rewriting_terminal() {
        let (owner, mut work) = reserved();
        exchanging(&owner, &work);
        let end = Instant::now() + Duration::from_millis(25);
        let expires = owner
            .inner
            .adopt_pending(work.key, marker(&work, end), Instant::now())
            .expect("pending marker");
        let once = owner.snapshot().expect("snapshot").last_authorization;
        tokio::time::timeout(
            Duration::from_secs(1),
            retain_pending(&owner.inner, work.key, expires, &mut work.stop),
        )
        .await
        .expect("bounded watchdog");
        let after = owner.snapshot().expect("snapshot");
        assert_eq!(after.phase, Phase::Draining);
        assert!(after.granted_subset_pending_verification.is_empty());
        assert_eq!(after.last_authorization, once);
        owner.inner.reaped(work.key); // This marker fixture launched no worker.
    }

    #[test]
    fn counter_overflow_during_active_cancellation_fails_closed_and_signals() {
        let owner = SessionController::new();
        owner.inner.state.lock().expect("state").generation = u64::MAX - 1;
        let work = owner
            .inner
            .reserve(request(), Instant::now())
            .expect("last representable admission");
        assert_eq!(
            owner.cancel(work.key.id),
            Err(ControllerError::CounterExhausted)
        );
        assert!(*work.stop.borrow());
        assert_eq!(owner.snapshot(), Err(ControllerError::CounterExhausted));
        let state = owner.inner.state.lock().expect("not poisoned");
        assert!(state.closed);
        assert_eq!(state.generation, u64::MAX);
        assert_eq!(state.slot.as_ref().expect("fence").phase, Phase::Draining);
    }
}

#[cfg(test)]
#[path = "session_independent_tests.rs"]
mod independent_tests;
