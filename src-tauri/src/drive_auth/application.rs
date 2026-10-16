//! One native owner and bounded, token-free JSON facade; no app/IPC registration.
//!
//! Native records require separately reviewed provenance. A selector is not
//! provider proof. HandedOff means launcher completion, never sign-in success.
//! Caller Drop requests existing native cleanup; it cannot abort a started OS
//! operation. No credential persistence, account lease, logging or sync exists.

use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Deserializer, Serialize, de};
use uuid::Uuid;

use super::account::AccountError;
use super::browser::{self, LaunchError, LaunchOutcome};
use super::pkce::ApprovedScope;
use super::registration::{
    NativeRegistration, NativeRegistrationCatalog, RegistrationError, RegistrationSelection,
};
use super::session::{
    AuthSnapshot, AuthorizationOutcome, BrowserLaunchTicket, CancelDisposition, ConnectionSnapshot,
    ConnectionStatus, ControllerError, DisconnectOrSwitch, Phase, SessionController, StartError,
    SwitchDecision, SwitchDisposition, SwitchProposalSnapshot, TerminalRecord, TicketError,
    VerificationOutcome, VerificationPhase, VerificationSnapshot, VerificationTerminalRecord,
};
use crate::drive::model::AccountKey;

const WIRE_LIMIT: usize = 4096;

/// Static codes only: no provider, request, URL or native-error string payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FacadeError {
    InvalidRequest,
    NotConfigured,
    UnknownRegistration,
    DisallowedCapabilities,
    Busy,
    Closing,
    Closed,
    Stale,
    TimedOut,
    Cancelled,
    LaunchFailed,
    WorkerFailure,
    AttemptEnded,
    ControllerFault,
}

/// Native-only closure acknowledgement, not evidence of resource cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosingReport {
    pub first: bool,
    pub fault: Option<FacadeError>,
}
/// Native-only fence result. Drop/timeout/unwind is never CleanupComplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupResult {
    CleanupComplete { fault: Option<FacadeError> },
    CleanupUnproven { fault: FacadeError },
}
struct ClosingState {
    closed: bool,
    fault: Option<FacadeError>,
}

/// No Clone/Debug/serde, task handle, public controller, URL or credential access.
pub struct DriveApplication {
    controller: SessionController,
    registrations: Vec<RegistrationOption>,
    closing: Mutex<ClosingState>,
}
impl DriveApplication {
    /// Task-free. No record or runtime is needed for an unconfigured owner.
    pub fn from_native_records(records: Vec<NativeRegistration>) -> Result<Self, FacadeError> {
        // The consuming catalog has no iterator. Preflight its count/duplicate
        // policy using borrowed getters BEFORE copying any selector metadata.
        // Records already enforce field/scope bounds in their private constructor.
        if records.len() > 8 {
            return Err(FacadeError::InvalidRequest);
        }
        for (i, record) in records.iter().enumerate() {
            if records[..i].iter().any(|prior| {
                prior.id() == record.id() || prior.desktop_client_id() == record.desktop_client_id()
            }) {
                return Err(FacadeError::InvalidRequest);
            }
        }
        let registrations = records
            .iter()
            .map(|r| RegistrationOption {
                id: r.id().to_owned(),
                allowed_capabilities: capabilities(r.allowed_scopes()),
            })
            .collect();
        let catalog =
            NativeRegistrationCatalog::from_native_records(records).map_err(registration_error)?;
        Ok(Self {
            controller: SessionController::with_catalog(catalog),
            registrations,
            closing: Mutex::new(ClosingState {
                closed: false,
                fault: None,
            }),
        })
    }

    pub fn options(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        encode(&Options {
            configured: !self.registrations.is_empty(),
            registrations: &self.registrations,
        })
    }
    pub async fn begin(&self, raw: &str) -> Result<String, FacadeError> {
        let ticket = self.prepare_begin(raw).await?;
        handoff_response(browser::hand_off(ticket).await)
    }
    async fn prepare_begin(&self, raw: &str) -> Result<BrowserLaunchTicket, FacadeError> {
        self.check_open()?;
        let request = decode::<BeginRequest>(raw)?;
        let selection = request.selection()?;
        let ticket = self
            .controller
            .begin_configured(selection)
            .await
            .map_err(start_error)?;
        // On error the genuine ticket is dropped; its native acknowledgement
        // fence remains authoritative. Never launch after observing closure.
        self.check_open()?;
        Ok(ticket)
    }
    pub fn status(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        encode(&Status::from(
            self.controller.snapshot().map_err(controller_error)?,
        ))
    }
    pub fn verification_status(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        encode(&VerificationStatus::from(
            self.controller
                .verification_snapshot()
                .map_err(controller_error)?,
        ))
    }
    pub fn connection(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        encode(&Connection::from(
            self.controller
                .connection_snapshot()
                .map_err(controller_error)?,
        ))
    }
    pub fn proposal(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        let proposal = self
            .controller
            .switch_proposal()
            .map_err(controller_error)?;
        encode(&ProposalResponse {
            proposal: proposal.and_then(|p| Proposal::at(p, Instant::now())),
        })
    }
    pub fn cancel(&self, raw: &str) -> Result<String, FacadeError> {
        let request = decode::<CancelRequest>(raw)?;
        let outcome = self
            .controller
            .cancel(request.attempt_id.0)
            .map_err(controller_error)?;
        encode(&Outcome {
            outcome: CancelWire::from(outcome),
        })
    }
    pub fn decide(&self, raw: &str) -> Result<String, FacadeError> {
        let request = decode::<DecideRequest>(raw)?;
        // Reject new decisions once closure is observed. An already-admitted
        // decision races native invalidate under the existing commit mutex.
        // Do not hold our gate over decide_switch's UUID/OS entropy acquisition.
        self.check_open()?;
        let outcome = self
            .controller
            .decide_switch(
                request.attempt_id.0,
                request.generation.0,
                request.decision_id.0,
                request.decision.native(),
            )
            .map_err(controller_error)?;
        encode(&Outcome {
            outcome: SwitchWire::from(outcome),
        })
    }
    pub fn disconnect(&self, raw: &str) -> Result<String, FacadeError> {
        decode::<EmptyRequest>(raw)?;
        self.controller
            .invalidate(DisconnectOrSwitch::Disconnect)
            .map_err(controller_error)?;
        encode(&Outcome {
            outcome: Acknowledgment::OwnershipInvalidated,
        })
    }
    fn check_open(&self) -> Result<(), FacadeError> {
        let state = self
            .closing
            .lock()
            .map_err(|_| FacadeError::ControllerFault)?;
        if state.closed {
            Err(FacadeError::Closing)
        } else {
            Ok(())
        }
    }
    /// Closes admission synchronously, once; never claims cleanup completion.
    pub fn mark_closing(&self) -> ClosingReport {
        let (mut state, poisoned) = match self.closing.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        if poisoned {
            state.fault = Some(FacadeError::ControllerFault);
        }
        if state.closed {
            return ClosingReport {
                first: false,
                fault: state.fault,
            };
        }
        state.closed = true;
        let fault = self
            .controller
            .invalidate(DisconnectOrSwitch::Disconnect)
            .err()
            .map(controller_error);
        state.fault = state.fault.or(fault);
        ClosingReport {
            first: true,
            fault: state.fault,
        }
    }
    /// May remain pending indefinitely for unproven OS cleanup. Dropping this
    /// future does not reopen admission or abort the existing native supervisors.
    pub async fn shutdown(&self) -> CleanupResult {
        let report = self.mark_closing();
        classify_cleanup(self.controller.shutdown().await, report.fault)
    }
    #[cfg(test)]
    async fn begin_with_sink<F, Fut>(&self, raw: &str, sink: F) -> Result<String, FacadeError>
    where
        F: FnOnce(BrowserLaunchTicket) -> Fut,
        Fut: std::future::Future<Output = Result<LaunchOutcome, LaunchError>>,
    {
        let ticket = self.prepare_begin(raw).await?;
        handoff_response(sink(ticket).await)
    }
}
// Field destruction drops the sole SessionController owner. No replacement task
// is spawned and no async task owns this outer object internally.

fn classify_cleanup(
    result: Result<(), ControllerError>,
    prior: Option<FacadeError>,
) -> CleanupResult {
    match result {
        Ok(()) => CleanupResult::CleanupComplete { fault: prior },
        Err(
            ControllerError::Poisoned
            | ControllerError::CounterExhausted
            | ControllerError::WatcherFailure,
        ) => CleanupResult::CleanupComplete {
            fault: Some(FacadeError::ControllerFault),
        },
        Err(ControllerError::Closed) => CleanupResult::CleanupUnproven {
            fault: FacadeError::Closed,
        },
    }
}
fn handoff_response(result: Result<LaunchOutcome, LaunchError>) -> Result<String, FacadeError> {
    match result.map_err(launch_error)? {
        LaunchOutcome::HandedOff => encode(&Outcome {
            outcome: Acknowledgment::HandedOff,
        }),
    }
}

// Typed structs reject decoded duplicate fields (including escaped aliases).
// There is no intermediate Value tree. The byte bound precedes serde parsing.
fn decode<T: de::DeserializeOwned>(raw: &str) -> Result<T, FacadeError> {
    if raw.len() > WIRE_LIMIT {
        return Err(FacadeError::InvalidRequest);
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = T::deserialize(&mut decoder).map_err(|_| FacadeError::InvalidRequest)?;
    decoder.end().map_err(|_| FacadeError::InvalidRequest)?;
    Ok(value)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Versioned<'a, T> {
    schema_version: u8,
    #[serde(flatten)]
    body: &'a T,
}
fn encode<T: Serialize>(body: &T) -> Result<String, FacadeError> {
    let encoded = serde_json::to_string(&Versioned {
        schema_version: 1,
        body,
    })
    .map_err(|_| FacadeError::ControllerFault)?;
    if encoded.len() > WIRE_LIMIT {
        Err(FacadeError::ControllerFault)
    } else {
        Ok(encoded)
    }
}
struct Version;
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match u8::deserialize(decoder)? {
            1 => Ok(Self),
            _ => Err(de::Error::custom("invalid version")),
        }
    }
}
struct WireUuid(Uuid);
impl<'de> Deserialize<'de> for WireUuid {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        let parsed = Uuid::parse_str(&text).map_err(|_| de::Error::custom("invalid identifier"))?;
        if text.len() != 36 || parsed.hyphenated().to_string() != text {
            return Err(de::Error::custom("noncanonical identifier"));
        }
        Ok(Self(parsed))
    }
}
struct WireDecimal(u64);
impl<'de> Deserialize<'de> for WireDecimal {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        if text.is_empty()
            || text.len() > 20
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(de::Error::custom("noncanonical counter"));
        }
        text.parse()
            .map(Self)
            .map_err(|_| de::Error::custom("counter overflow"))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum Capability {
    StateLibrary,
    Photos,
}
impl Capability {
    fn native(self) -> ApprovedScope {
        match self {
            Self::StateLibrary => ApprovedScope::AppData,
            Self::Photos => ApprovedScope::File,
        }
    }
}
fn capabilities(scopes: &[ApprovedScope]) -> Vec<Capability> {
    // Deterministic presentation of the actual set, never requested-scope union.
    [ApprovedScope::AppData, ApprovedScope::File]
        .into_iter()
        .filter(|scope| scopes.contains(scope))
        .map(|scope| match scope {
            ApprovedScope::AppData => Capability::StateLibrary,
            ApprovedScope::File => Capability::Photos,
        })
        .collect()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EmptyRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BeginRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
    registration_id: String,
    capabilities: Vec<Capability>,
}
impl BeginRequest {
    fn selection(self) -> Result<RegistrationSelection, FacadeError> {
        if !matches!(self.capabilities.len(), 1 | 2)
            || (self.capabilities.len() == 2 && self.capabilities[0] == self.capabilities[1])
        {
            return Err(FacadeError::InvalidRequest);
        }
        let scopes: Vec<_> = [Capability::StateLibrary, Capability::Photos]
            .into_iter()
            .filter(|scope| self.capabilities.contains(scope))
            .map(Capability::native)
            .collect();
        RegistrationSelection::new(&self.registration_id, &scopes).map_err(registration_error)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CancelRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
    attempt_id: WireUuid,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Decision {
    Accept,
    Decline,
}
impl Decision {
    fn native(self) -> SwitchDecision {
        match self {
            Self::Accept => SwitchDecision::Accept,
            Self::Decline => SwitchDecision::Decline,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DecideRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
    attempt_id: WireUuid,
    generation: WireDecimal,
    decision_id: WireUuid,
    decision: Decision,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationOption {
    id: String,
    allowed_capabilities: Vec<Capability>,
}
#[derive(Serialize)]
struct Options<'a> {
    configured: bool,
    registrations: &'a [RegistrationOption],
}
#[derive(Serialize)]
struct Outcome<T> {
    outcome: T,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Acknowledgment {
    HandedOff,
    OwnershipInvalidated,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum PhaseWire {
    Idle,
    Starting,
    AwaitingBrowser,
    AwaitingCallback,
    Exchanging,
    AccountVerificationPending,
    VerifyingAccount,
    AwaitingSwitch,
    Draining,
    Closed,
}
impl From<Phase> for PhaseWire {
    fn from(value: Phase) -> Self {
        match value {
            Phase::Idle => Self::Idle,
            Phase::Starting => Self::Starting,
            Phase::AwaitingBrowser => Self::AwaitingBrowser,
            Phase::AwaitingCallback => Self::AwaitingCallback,
            Phase::Exchanging => Self::Exchanging,
            Phase::AccountVerificationPending => Self::AccountVerificationPending,
            Phase::VerifyingAccount => Self::VerifyingAccount,
            Phase::AwaitingSwitch => Self::AwaitingSwitch,
            Phase::Draining => Self::Draining,
            Phase::Closed => Self::Closed,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum AuthorizationWire {
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
impl From<AuthorizationOutcome> for AuthorizationWire {
    fn from(value: AuthorizationOutcome) -> Self {
        match value {
            AuthorizationOutcome::CompletedPendingVerification => {
                Self::CompletedPendingVerification
            }
            AuthorizationOutcome::Denied => Self::Denied,
            AuthorizationOutcome::ProviderRejected => Self::ProviderRejected,
            AuthorizationOutcome::Cancelled => Self::Cancelled,
            AuthorizationOutcome::TimedOut => Self::TimedOut,
            AuthorizationOutcome::BrowserUnavailable => Self::BrowserUnavailable,
            AuthorizationOutcome::SetupFailure => Self::SetupFailure,
            AuthorizationOutcome::ListenerFailure => Self::ListenerFailure,
            AuthorizationOutcome::ExchangeFailure => Self::ExchangeFailure,
            AuthorizationOutcome::WorkerFailure => Self::WorkerFailure,
            AuthorizationOutcome::Closed => Self::Closed,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum VerificationPhaseWire {
    Idle,
    Running,
    AwaitingSwitch,
    Draining,
    Closed,
}
impl From<VerificationPhase> for VerificationPhaseWire {
    fn from(value: VerificationPhase) -> Self {
        match value {
            VerificationPhase::Idle => Self::Idle,
            VerificationPhase::Running => Self::Running,
            VerificationPhase::AwaitingSwitch => Self::AwaitingSwitch,
            VerificationPhase::Draining => Self::Draining,
            VerificationPhase::Closed => Self::Closed,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum ConnectionWire {
    Disconnected,
    ConnectedSessionOnly,
    Expired,
    Closed,
}
impl From<ConnectionStatus> for ConnectionWire {
    fn from(value: ConnectionStatus) -> Self {
        match value {
            ConnectionStatus::Disconnected => Self::Disconnected,
            ConnectionStatus::ConnectedSessionOnly => Self::ConnectedSessionOnly,
            ConnectionStatus::Expired => Self::Expired,
            ConnectionStatus::Closed => Self::Closed,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum CancelWire {
    Cancelled,
    NotCurrent,
    AlreadyTerminal,
}
impl From<CancelDisposition> for CancelWire {
    fn from(value: CancelDisposition) -> Self {
        match value {
            CancelDisposition::Cancelled => Self::Cancelled,
            CancelDisposition::NotCurrent => Self::NotCurrent,
            CancelDisposition::AlreadyTerminal => Self::AlreadyTerminal,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum SwitchWire {
    AdoptedSessionOnly,
    Declined,
    Stale,
    AlreadyResolved,
}
impl From<SwitchDisposition> for SwitchWire {
    fn from(value: SwitchDisposition) -> Self {
        match value {
            SwitchDisposition::AdoptedSessionOnly => Self::AdoptedSessionOnly,
            SwitchDisposition::Declined => Self::Declined,
            SwitchDisposition::Stale => Self::Stale,
            SwitchDisposition::AlreadyResolved => Self::AlreadyResolved,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthorizationTerminal {
    attempt_id: String,
    generation: String,
    outcome: AuthorizationWire,
}
impl From<TerminalRecord> for AuthorizationTerminal {
    fn from(value: TerminalRecord) -> Self {
        Self {
            attempt_id: value.attempt_id.to_string(),
            generation: value.generation.to_string(),
            outcome: value.outcome.into(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    controller_generation: String,
    attempt_id: Option<String>,
    phase: PhaseWire,
    last_authorization: Option<AuthorizationTerminal>,
    unverified_granted_capabilities: Vec<Capability>,
}
impl From<AuthSnapshot> for Status {
    fn from(value: AuthSnapshot) -> Self {
        Self {
            controller_generation: value.generation.to_string(),
            attempt_id: value.attempt_id.map(|id| id.to_string()),
            phase: value.phase.into(),
            last_authorization: value.last_authorization.map(Into::into),
            unverified_granted_capabilities: capabilities(
                &value.granted_subset_pending_verification,
            ),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum VerificationWire {
    AdoptedSessionOnly,
    Declined,
    Cancelled,
    TimedOut,
    Closed,
    WorkerFailure,
    BindingMismatch,
    VerificationFailed,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum VerificationCause {
    AuthRequired,
    AccessDenied,
    Unavailable,
    SetupFailed,
    Cancelled,
    TimedOut,
    BindingMismatch,
    InvalidResponse,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationTerminal {
    attempt_id: String,
    generation: String,
    outcome: VerificationWire,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<VerificationCause>,
}
impl From<VerificationTerminalRecord> for VerificationTerminal {
    fn from(value: VerificationTerminalRecord) -> Self {
        let (outcome, cause) = match value.outcome {
            VerificationOutcome::AdoptedSessionOnly => (VerificationWire::AdoptedSessionOnly, None),
            VerificationOutcome::Declined => (VerificationWire::Declined, None),
            VerificationOutcome::Cancelled => (VerificationWire::Cancelled, None),
            VerificationOutcome::TimedOut => (VerificationWire::TimedOut, None),
            VerificationOutcome::Closed => (VerificationWire::Closed, None),
            VerificationOutcome::WorkerFailure => (VerificationWire::WorkerFailure, None),
            VerificationOutcome::BindingMismatch => (VerificationWire::BindingMismatch, None),
            VerificationOutcome::Failed(error) => (
                VerificationWire::VerificationFailed,
                Some(account_error(error)),
            ),
        };
        Self {
            attempt_id: value.attempt_id.to_string(),
            generation: value.generation.to_string(),
            outcome,
            cause,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationStatus {
    configured: bool,
    controller_generation: String,
    attempt_id: Option<String>,
    phase: VerificationPhaseWire,
    last_terminal: Option<VerificationTerminal>,
}
impl From<VerificationSnapshot> for VerificationStatus {
    fn from(value: VerificationSnapshot) -> Self {
        Self {
            configured: value.configured,
            controller_generation: value.generation.to_string(),
            attempt_id: value.attempt_id.map(|id| id.to_string()),
            phase: value.phase.into(),
            last_terminal: value.last_terminal.map(Into::into),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountMetadata {
    project_number: String,
    drive_permission_id: String,
}
impl From<AccountKey> for AccountMetadata {
    fn from(value: AccountKey) -> Self {
        Self {
            project_number: value.project_number.value().to_string(),
            drive_permission_id: value.drive_permission_id.as_str().to_owned(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    status: ConnectionWire,
    connection_id: Option<String>,
    epoch: String,
    account_key: Option<AccountMetadata>,
    granted_capabilities: Vec<Capability>,
}
impl From<ConnectionSnapshot> for Connection {
    fn from(value: ConnectionSnapshot) -> Self {
        Self {
            status: value.status.into(),
            connection_id: value.connection_id.map(|id| id.to_string()),
            epoch: value.epoch.to_string(),
            account_key: value.account_key.map(Into::into),
            granted_capabilities: capabilities(&value.granted_scopes),
        }
    }
}
#[derive(Serialize)]
struct ProposalResponse {
    proposal: Option<Proposal>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Proposal {
    attempt_id: String,
    generation: String,
    decision_id: String,
    candidate_account: AccountMetadata,
    incumbent_account: AccountMetadata,
    registration_id: String,
    actual_capabilities: Vec<Capability>,
    baseline_connection_epoch: String,
    expires_in_ms: u64,
}
impl Proposal {
    fn at(value: SwitchProposalSnapshot, now: Instant) -> Option<Self> {
        let ms = value
            .deadline
            .saturating_duration_since(now)
            .as_millis()
            .min(60_000) as u64;
        if ms == 0 {
            return None;
        }
        Some(Self {
            attempt_id: value.attempt_id.to_string(),
            generation: value.generation.to_string(),
            decision_id: value.decision_id.to_string(),
            candidate_account: value.candidate_account.into(),
            incumbent_account: value.incumbent_account.into(),
            registration_id: value.registration_id,
            actual_capabilities: capabilities(&value.actual_grants),
            baseline_connection_epoch: value.baseline_connection_epoch.to_string(),
            expires_in_ms: ms,
        })
    }
}
fn controller_error(error: ControllerError) -> FacadeError {
    match error {
        ControllerError::Closed => FacadeError::Closed,
        ControllerError::Poisoned
        | ControllerError::CounterExhausted
        | ControllerError::WatcherFailure => FacadeError::ControllerFault,
    }
}
fn registration_error(error: RegistrationError) -> FacadeError {
    match error {
        RegistrationError::NotConfigured => FacadeError::NotConfigured,
        RegistrationError::UnknownRegistration => FacadeError::UnknownRegistration,
        RegistrationError::DisallowedScopes => FacadeError::DisallowedCapabilities,
        RegistrationError::InvalidId
        | RegistrationError::InvalidClientId
        | RegistrationError::InvalidProjectNumber
        | RegistrationError::InvalidAllowedScopes
        | RegistrationError::InvalidRequestedScopes
        | RegistrationError::TooManyRecords
        | RegistrationError::DuplicateId
        | RegistrationError::DuplicateClientId => FacadeError::InvalidRequest,
    }
}
fn start_error(error: StartError) -> FacadeError {
    match error {
        StartError::InvalidRegistration => FacadeError::InvalidRequest,
        StartError::Registration(error) => registration_error(error),
        StartError::Busy => FacadeError::Busy,
        StartError::Controller(error) => controller_error(error),
        StartError::Terminated => FacadeError::AttemptEnded,
    }
}
fn launch_error(error: LaunchError) -> FacadeError {
    match error {
        LaunchError::InvalidDestination | LaunchError::LaunchFailed => FacadeError::LaunchFailed,
        LaunchError::Cancelled => FacadeError::Cancelled,
        LaunchError::WorkerFailure => FacadeError::WorkerFailure,
        LaunchError::Ticket(TicketError::Stale) => FacadeError::Stale,
        LaunchError::Ticket(TicketError::Expired) => FacadeError::TimedOut,
        LaunchError::Ticket(TicketError::Controller(error)) => controller_error(error),
    }
}
fn account_error(error: AccountError) -> VerificationCause {
    match error {
        AccountError::Unauthorized => VerificationCause::AuthRequired,
        AccountError::Forbidden => VerificationCause::AccessDenied,
        AccountError::Transport | AccountError::RateLimited | AccountError::Server => {
            VerificationCause::Unavailable
        }
        AccountError::ClientSetup => VerificationCause::SetupFailed,
        AccountError::Cancelled => VerificationCause::Cancelled,
        AccountError::TokenExpired | AccountError::Deadline => VerificationCause::TimedOut,
        AccountError::ClockRegression
        | AccountError::AttemptMismatch
        | AccountError::InvalidRegistration(_)
        | AccountError::InvalidRequestedScopes
        | AccountError::InvalidGrantedScopes => VerificationCause::BindingMismatch,
        AccountError::InvalidBearerCredential
        | AccountError::HeaderLimit
        | AccountError::InvalidHeaders
        | AccountError::ContentEncoding
        | AccountError::BodyLimit
        | AccountError::ParseLimit(_)
        | AccountError::Redirect
        | AccountError::UnexpectedStatus
        | AccountError::MissingUser
        | AccountError::MissingPermissionId
        | AccountError::InvalidPermissionId
        | AccountError::MalformedResponse
        | AccountError::InvalidDisplayMetadata
        | AccountError::InvalidQuota
        | AccountError::ProviderErrorPayload => VerificationCause::InvalidResponse,
    }
}

#[cfg(test)]
mod tests {
    use super::super::account::{ParseLimit, RegistrationField};
    use super::super::session::BrowserHandoff;
    use super::*;
    use crate::drive::model::{ProviderId, UnsignedDecimal};
    use serde_json::{Value, json};
    use std::time::Duration;

    const EMPTY: &str = r#"{"schemaVersion":1}"#;
    const BEGIN: &str =
        r#"{"schemaVersion":1,"registrationId":"desktop","capabilities":["stateLibrary"]}"#;
    const ID: &str = "00000000-0000-0000-0000-000000000000";
    const OTHER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    fn uid() -> Uuid {
        Uuid::parse_str(ID).unwrap()
    }
    fn record(id: &str, client: &str, scopes: &[ApprovedScope]) -> NativeRegistration {
        NativeRegistration::from_native_configuration(id, client, "123", scopes).unwrap()
    }
    fn configured() -> DriveApplication {
        DriveApplication::from_native_records(vec![record(
            "desktop",
            "synthetic.apps.googleusercontent.com",
            &[ApprovedScope::AppData, ApprovedScope::File],
        )])
        .unwrap()
    }
    fn value(encoded: Result<String, FacadeError>) -> Value {
        serde_json::from_str(&encoded.unwrap()).unwrap()
    }
    fn wire<T: Serialize>(v: &T) -> Value {
        serde_json::to_value(v).unwrap()
    }
    fn identity(project: u64, id: &str) -> AccountKey {
        AccountKey {
            project_number: UnsignedDecimal::new(project),
            drive_permission_id: ProviderId::try_from(id.to_owned()).unwrap(),
        }
    }
    fn proposal_at(deadline: Instant) -> SwitchProposalSnapshot {
        SwitchProposalSnapshot {
            attempt_id: uid(),
            generation: u64::MAX,
            decision_id: Uuid::parse_str(OTHER).unwrap(),
            candidate_account: identity(7, "syntheticB"),
            incumbent_account: identity(5, "syntheticA"),
            registration_id: "desktop".into(),
            actual_grants: vec![ApprovedScope::File],
            baseline_connection_epoch: 9,
            deadline,
        }
    }
    async fn failed_ticket(ticket: BrowserLaunchTicket) -> Result<LaunchOutcome, LaunchError> {
        ticket
            .finish(BrowserHandoff::Failed)
            .map_err(LaunchError::Ticket)?;
        Err(LaunchError::LaunchFailed)
    }
    async fn idle(app: &DriveApplication) {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if value(app.status(EMPTY))["attemptId"].is_null() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("native cleanup deadline");
    }
    async fn shutdown(app: &DriveApplication) {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(4), app.shutdown())
                .await
                .expect("native shutdown deadline"),
            CleanupResult::CleanupComplete { fault: None }
        );
    }

    #[test]
    fn strict_json_rejects_alias_duplicates_unknown_types_and_bounds() {
        for raw in [
            "",
            "null",
            "[]",
            "{}",
            r#"{"schemaVersion":0}"#,
            r#"{"schemaVersion":2}"#,
            r#"{"schemaVersion":1.0}"#,
            r#"{"schemaVersion":"1"}"#,
            r#"{"schemaVersion":null}"#,
            r#"{"schemaVersion":true}"#,
            r#"{"schemaVersion":1,"schemaVersion":1}"#,
            r#"{"schemaVersion":1,"schema\u0056ersion":1}"#,
            r#"{"schemaVersion":1,"extra":0}"#,
            r#"{"schemaVersion":1}{}"#,
            r#"{"schemaVersion":NaN}"#,
        ] {
            assert!(matches!(
                decode::<EmptyRequest>(raw),
                Err(FacadeError::InvalidRequest)
            ));
        }
        assert!(decode::<EmptyRequest>(r#" {"schema\u0056ersion":1} "#).is_ok());
        let exact = format!("{EMPTY}{}", " ".repeat(WIRE_LIMIT - EMPTY.len()));
        assert!(decode::<EmptyRequest>(&exact).is_ok());
        assert!(matches!(
            decode::<EmptyRequest>(&(exact + " ")),
            Err(FacadeError::InvalidRequest)
        ));
        let deep = format!(
            r#"{{"schemaVersion":{}1{}}}"#,
            "[".repeat(140),
            "]".repeat(140)
        );
        assert!(matches!(
            decode::<EmptyRequest>(&deep),
            Err(FacadeError::InvalidRequest)
        ));
        for raw in [
            r#"{"schemaVersion":1,"registrationId":"desktop","capabilities":["photos"],"registration\u0049d":"desktop"}"#,
            r#"{"schemaVersion":1,"registrationId":"desktop","capabilities":["photos"],"clientId":"synthetic"}"#,
            r#"{"schemaVersion":1,"registrationId":"desktop","capabilities":{"photos":true}}"#,
            r#"{"schemaVersion":1,"registrationId":"desktop","capabilities":["photos","drive"]}"#,
        ] {
            assert!(matches!(
                decode::<BeginRequest>(raw),
                Err(FacadeError::InvalidRequest)
            ));
        }
    }

    #[test]
    fn canonical_identifiers_and_decimal_counters_preserve_precision() {
        for id in [ID, OTHER] {
            assert!(decode::<WireUuid>(&format!("\"{id}\"")).is_ok());
        }
        for id in [
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
            "00000000000000000000000000000000",
            "{00000000-0000-0000-0000-000000000000}",
            "urn:uuid:00000000-0000-0000-0000-000000000000",
            "",
            "x",
        ] {
            assert!(matches!(
                decode::<WireUuid>(&format!("\"{id}\"")),
                Err(FacadeError::InvalidRequest)
            ));
        }
        for (text, expected) in [
            ("0", 0),
            ("9007199254740993", 9_007_199_254_740_993),
            ("18446744073709551615", u64::MAX),
        ] {
            assert_eq!(
                decode::<WireDecimal>(&format!("\"{text}\"")).unwrap().0,
                expected
            );
        }
        for raw in [
            "0",
            "null",
            "\"\"",
            "\"00\"",
            "\"01\"",
            "\"+1\"",
            "\"-1\"",
            "\"1.0\"",
            "\"1e2\"",
            "\" 1\"",
            "\"18446744073709551616\"",
        ] {
            assert!(matches!(
                decode::<WireDecimal>(raw),
                Err(FacadeError::InvalidRequest)
            ));
        }
        let request = format!(
            r#"{{"schemaVersion":1,"attemptId":"{ID}","generation":"18446744073709551615","decisionId":"{OTHER}","decision":"accept"}}"#
        );
        let decoded = decode::<DecideRequest>(&request).unwrap();
        assert_eq!(decoded.generation.0, u64::MAX);
        assert!(matches!(decoded.decision.native(), SwitchDecision::Accept));
        assert!(decode::<DecideRequest>(&request.replace("accept", "decline")).is_ok());
        for bad in [
            request.replace("accept", "Accept"),
            request.replace("\"18446744073709551615\"", "0"),
            request.replace("\"decision\":", "\"generation\":\"0\",\"decision\":"),
        ] {
            assert!(matches!(
                decode::<DecideRequest>(&bad),
                Err(FacadeError::InvalidRequest)
            ));
        }
        let cancel = format!(r#"{{"schemaVersion":1,"attemptId":"{ID}","generation":"0"}}"#);
        assert!(matches!(
            decode::<CancelRequest>(&cancel),
            Err(FacadeError::InvalidRequest)
        ));
    }

    #[test]
    fn capabilities_are_explicit_unique_and_normalized() {
        for (caps, expected) in [
            (r#"["stateLibrary"]"#, vec![ApprovedScope::AppData]),
            (r#"["photos"]"#, vec![ApprovedScope::File]),
            (
                r#"["photos","stateLibrary"]"#,
                vec![ApprovedScope::AppData, ApprovedScope::File],
            ),
            (
                r#"["stateLibrary","photos"]"#,
                vec![ApprovedScope::AppData, ApprovedScope::File],
            ),
        ] {
            let raw =
                format!(r#"{{"schemaVersion":1,"registrationId":"._-","capabilities":{caps}}}"#);
            let selected = decode::<BeginRequest>(&raw).unwrap().selection().unwrap();
            assert_eq!(selected.id(), "._-");
            assert_eq!(selected.requested_scopes(), expected);
        }
        for caps in [
            "[]",
            r#"["photos","photos"]"#,
            r#"["photos","stateLibrary","photos"]"#,
        ] {
            let raw = format!(
                r#"{{"schemaVersion":1,"registrationId":"desktop","capabilities":{caps}}}"#
            );
            assert!(matches!(
                decode::<BeginRequest>(&raw).unwrap().selection(),
                Err(FacadeError::InvalidRequest)
            ));
        }
        for id in ["".to_owned(), " ".into(), "a".repeat(129)] {
            let raw = json!({"schemaVersion":1,"registrationId":id,"capabilities":["photos"]})
                .to_string();
            assert!(matches!(
                decode::<BeginRequest>(&raw).unwrap().selection(),
                Err(FacadeError::InvalidRequest)
            ));
        }
        assert_eq!(
            wire(&capabilities(&[ApprovedScope::File])),
            json!(["photos"])
        );
    }

    #[test]
    fn catalog_options_are_bounded_task_free_and_do_not_expose_configuration() {
        let empty = DriveApplication::from_native_records(vec![]).unwrap();
        assert_eq!(
            empty.options(EMPTY).unwrap(),
            r#"{"schemaVersion":1,"configured":false,"registrations":[]}"#
        );
        let records = (0..8)
            .map(|i| {
                record(
                    &format!("{i}{}", "x".repeat(127)),
                    &format!("synthetic-{i}.apps.googleusercontent.com"),
                    &[ApprovedScope::File, ApprovedScope::AppData],
                )
            })
            .collect();
        let app = DriveApplication::from_native_records(records).unwrap();
        let encoded = app.options(EMPTY).unwrap();
        assert!(encoded.len() <= WIRE_LIMIT);
        let v: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(v["registrations"].as_array().unwrap().len(), 8);
        assert_eq!(
            v["registrations"][0]["allowedCapabilities"],
            json!(["stateLibrary", "photos"])
        );
        for forbidden in [
            "client",
            "project",
            "apps.googleusercontent",
            "token",
            "https:",
        ] {
            assert!(!encoded.contains(forbidden));
        }
        let nine = (0..9)
            .map(|i| {
                record(
                    &i.to_string(),
                    &format!("s{i}.apps.googleusercontent.com"),
                    &[ApprovedScope::AppData],
                )
            })
            .collect();
        assert!(matches!(
            DriveApplication::from_native_records(nine),
            Err(FacadeError::InvalidRequest)
        ));
        for (id, client) in [
            ("same", "b.apps.googleusercontent.com"),
            ("other", "a.apps.googleusercontent.com"),
        ] {
            assert!(matches!(
                DriveApplication::from_native_records(vec![
                    record(
                        "same",
                        "a.apps.googleusercontent.com",
                        &[ApprovedScope::AppData]
                    ),
                    record(id, client, &[ApprovedScope::AppData])
                ]),
                Err(FacadeError::InvalidRequest)
            ));
        }
    }

    #[test]
    fn static_error_mapping_is_exhaustive_and_redacted() {
        use RegistrationError as R;
        for (native, expected) in [
            (R::InvalidId, FacadeError::InvalidRequest),
            (R::InvalidClientId, FacadeError::InvalidRequest),
            (R::InvalidProjectNumber, FacadeError::InvalidRequest),
            (R::InvalidAllowedScopes, FacadeError::InvalidRequest),
            (R::InvalidRequestedScopes, FacadeError::InvalidRequest),
            (R::TooManyRecords, FacadeError::InvalidRequest),
            (R::DuplicateId, FacadeError::InvalidRequest),
            (R::DuplicateClientId, FacadeError::InvalidRequest),
            (R::NotConfigured, FacadeError::NotConfigured),
            (R::UnknownRegistration, FacadeError::UnknownRegistration),
            (R::DisallowedScopes, FacadeError::DisallowedCapabilities),
        ] {
            assert_eq!(registration_error(native), expected);
            assert_eq!(start_error(StartError::Registration(native)), expected);
        }
        for (native, expected) in [
            (ControllerError::Closed, FacadeError::Closed),
            (ControllerError::Poisoned, FacadeError::ControllerFault),
            (
                ControllerError::CounterExhausted,
                FacadeError::ControllerFault,
            ),
            (
                ControllerError::WatcherFailure,
                FacadeError::ControllerFault,
            ),
        ] {
            assert_eq!(controller_error(native), expected);
            assert_eq!(start_error(StartError::Controller(native)), expected);
            assert_eq!(
                launch_error(LaunchError::Ticket(TicketError::Controller(native))),
                expected
            );
        }
        for (native, expected) in [
            (StartError::InvalidRegistration, FacadeError::InvalidRequest),
            (StartError::Busy, FacadeError::Busy),
            (StartError::Terminated, FacadeError::AttemptEnded),
        ] {
            assert_eq!(start_error(native), expected);
        }
        for (native, expected) in [
            (LaunchError::InvalidDestination, FacadeError::LaunchFailed),
            (LaunchError::LaunchFailed, FacadeError::LaunchFailed),
            (LaunchError::Cancelled, FacadeError::Cancelled),
            (LaunchError::WorkerFailure, FacadeError::WorkerFailure),
            (LaunchError::Ticket(TicketError::Stale), FacadeError::Stale),
            (
                LaunchError::Ticket(TicketError::Expired),
                FacadeError::TimedOut,
            ),
        ] {
            assert_eq!(launch_error(native), expected);
        }
        use AccountError as A;
        for (native, expected) in [
            (A::Unauthorized, "authRequired"),
            (A::Forbidden, "accessDenied"),
            (A::Transport, "unavailable"),
            (A::RateLimited, "unavailable"),
            (A::Server, "unavailable"),
            (A::ClientSetup, "setupFailed"),
            (A::Cancelled, "cancelled"),
            (A::TokenExpired, "timedOut"),
            (A::Deadline, "timedOut"),
            (A::ClockRegression, "bindingMismatch"),
            (A::AttemptMismatch, "bindingMismatch"),
            (A::InvalidRequestedScopes, "bindingMismatch"),
            (A::InvalidGrantedScopes, "bindingMismatch"),
            (A::InvalidBearerCredential, "invalidResponse"),
            (A::HeaderLimit, "invalidResponse"),
            (A::InvalidHeaders, "invalidResponse"),
            (A::ContentEncoding, "invalidResponse"),
            (A::BodyLimit, "invalidResponse"),
            (A::Redirect, "invalidResponse"),
            (A::UnexpectedStatus, "invalidResponse"),
            (A::MissingUser, "invalidResponse"),
            (A::MissingPermissionId, "invalidResponse"),
            (A::InvalidPermissionId, "invalidResponse"),
            (A::MalformedResponse, "invalidResponse"),
            (A::InvalidDisplayMetadata, "invalidResponse"),
            (A::InvalidQuota, "invalidResponse"),
            (A::ProviderErrorPayload, "invalidResponse"),
        ] {
            assert_eq!(wire(&account_error(native)), json!(expected));
        }
        for field in [
            RegistrationField::RegistrationId,
            RegistrationField::ClientId,
            RegistrationField::ProjectNumber,
            RegistrationField::AttemptIdentity,
        ] {
            assert_eq!(
                wire(&account_error(A::InvalidRegistration(field))),
                json!("bindingMismatch")
            );
        }
        for limit in [
            ParseLimit::Depth,
            ParseLimit::Work,
            ParseLimit::Fields,
            ParseLimit::Key,
        ] {
            assert_eq!(
                wire(&account_error(A::ParseLimit(limit))),
                json!("invalidResponse")
            );
        }
        for (error, code) in [
            (FacadeError::InvalidRequest, "invalidRequest"),
            (FacadeError::NotConfigured, "notConfigured"),
            (FacadeError::UnknownRegistration, "unknownRegistration"),
            (
                FacadeError::DisallowedCapabilities,
                "disallowedCapabilities",
            ),
            (FacadeError::Busy, "busy"),
            (FacadeError::Closing, "closing"),
            (FacadeError::Closed, "closed"),
            (FacadeError::Stale, "stale"),
            (FacadeError::TimedOut, "timedOut"),
            (FacadeError::Cancelled, "cancelled"),
            (FacadeError::LaunchFailed, "launchFailed"),
            (FacadeError::WorkerFailure, "workerFailure"),
            (FacadeError::AttemptEnded, "attemptEnded"),
            (FacadeError::ControllerFault, "controllerFault"),
        ] {
            assert_eq!(wire(&error), json!(code));
        }
    }

    #[test]
    fn native_phase_and_outcome_wire_names_are_fixed() {
        for (v, s) in [
            (Phase::Idle, "idle"),
            (Phase::Starting, "starting"),
            (Phase::AwaitingBrowser, "awaitingBrowser"),
            (Phase::AwaitingCallback, "awaitingCallback"),
            (Phase::Exchanging, "exchanging"),
            (
                Phase::AccountVerificationPending,
                "accountVerificationPending",
            ),
            (Phase::VerifyingAccount, "verifyingAccount"),
            (Phase::AwaitingSwitch, "awaitingSwitch"),
            (Phase::Draining, "draining"),
            (Phase::Closed, "closed"),
        ] {
            assert_eq!(wire(&PhaseWire::from(v)), json!(s));
        }
        for (v, s) in [
            (
                AuthorizationOutcome::CompletedPendingVerification,
                "completedPendingVerification",
            ),
            (AuthorizationOutcome::Denied, "denied"),
            (AuthorizationOutcome::ProviderRejected, "providerRejected"),
            (AuthorizationOutcome::Cancelled, "cancelled"),
            (AuthorizationOutcome::TimedOut, "timedOut"),
            (
                AuthorizationOutcome::BrowserUnavailable,
                "browserUnavailable",
            ),
            (AuthorizationOutcome::SetupFailure, "setupFailure"),
            (AuthorizationOutcome::ListenerFailure, "listenerFailure"),
            (AuthorizationOutcome::ExchangeFailure, "exchangeFailure"),
            (AuthorizationOutcome::WorkerFailure, "workerFailure"),
            (AuthorizationOutcome::Closed, "closed"),
        ] {
            assert_eq!(wire(&AuthorizationWire::from(v)), json!(s));
        }
        for (v, s) in [
            (VerificationPhase::Idle, "idle"),
            (VerificationPhase::Running, "running"),
            (VerificationPhase::AwaitingSwitch, "awaitingSwitch"),
            (VerificationPhase::Draining, "draining"),
            (VerificationPhase::Closed, "closed"),
        ] {
            assert_eq!(wire(&VerificationPhaseWire::from(v)), json!(s));
        }
        for (v, s) in [
            (ConnectionStatus::Disconnected, "disconnected"),
            (
                ConnectionStatus::ConnectedSessionOnly,
                "connectedSessionOnly",
            ),
            (ConnectionStatus::Expired, "expired"),
            (ConnectionStatus::Closed, "closed"),
        ] {
            assert_eq!(wire(&ConnectionWire::from(v)), json!(s));
        }
        for (v, s) in [
            (CancelDisposition::Cancelled, "cancelled"),
            (CancelDisposition::NotCurrent, "notCurrent"),
            (CancelDisposition::AlreadyTerminal, "alreadyTerminal"),
        ] {
            assert_eq!(wire(&CancelWire::from(v)), json!(s));
        }
        for (v, s) in [
            (SwitchDisposition::AdoptedSessionOnly, "adoptedSessionOnly"),
            (SwitchDisposition::Declined, "declined"),
            (SwitchDisposition::Stale, "stale"),
            (SwitchDisposition::AlreadyResolved, "alreadyResolved"),
        ] {
            assert_eq!(wire(&SwitchWire::from(v)), json!(s));
        }
        for (v, s) in [
            (
                VerificationOutcome::AdoptedSessionOnly,
                "adoptedSessionOnly",
            ),
            (VerificationOutcome::Declined, "declined"),
            (VerificationOutcome::Cancelled, "cancelled"),
            (VerificationOutcome::TimedOut, "timedOut"),
            (VerificationOutcome::Closed, "closed"),
            (VerificationOutcome::WorkerFailure, "workerFailure"),
            (VerificationOutcome::BindingMismatch, "bindingMismatch"),
        ] {
            let wire = wire(&VerificationTerminal::from(VerificationTerminalRecord {
                attempt_id: uid(),
                generation: 1,
                outcome: v,
            }));
            assert_eq!(wire["outcome"], json!(s));
            assert!(wire.get("cause").is_none());
        }
        assert_eq!(
            wire(&VerificationTerminal::from(VerificationTerminalRecord {
                attempt_id: uid(),
                generation: 1,
                outcome: VerificationOutcome::Failed(AccountError::Unauthorized)
            }))["cause"],
            json!("authRequired")
        );
    }

    #[test]
    fn metadata_mapping_preserves_counters_and_never_invents_proof_or_countdown() {
        let status = Status::from(AuthSnapshot {
            generation: u64::MAX,
            attempt_id: Some(uid()),
            phase: Phase::Draining,
            last_authorization: Some(TerminalRecord {
                attempt_id: uid(),
                generation: 9_007_199_254_740_993,
                outcome: AuthorizationOutcome::Cancelled,
            }),
            granted_subset_pending_verification: vec![ApprovedScope::File],
        });
        let v = value(encode(&status));
        assert_eq!(v["controllerGeneration"], "18446744073709551615");
        assert_eq!(v["lastAuthorization"]["generation"], "9007199254740993");
        assert_eq!(v["unverifiedGrantedCapabilities"], json!(["photos"]));
        let verification = VerificationStatus::from(VerificationSnapshot {
            configured: true,
            generation: u64::MAX,
            attempt_id: None,
            phase: VerificationPhase::Idle,
            last_terminal: Some(VerificationTerminalRecord {
                attempt_id: uid(),
                generation: 0,
                outcome: VerificationOutcome::Declined,
            }),
        });
        assert_eq!(
            value(encode(&verification))["lastTerminal"]["generation"],
            "0"
        );
        // This is public metadata mapping, NOT a sealed candidate or adoption.
        let connection = Connection::from(ConnectionSnapshot {
            status: ConnectionStatus::ConnectedSessionOnly,
            connection_id: Some(uid()),
            epoch: u64::MAX,
            account_key: Some(identity(u64::MAX, "synthetic_id")),
            granted_scopes: vec![ApprovedScope::File],
        });
        let v = value(encode(&connection));
        assert_eq!(v["status"], "connectedSessionOnly");
        assert_eq!(v["epoch"], "18446744073709551615");
        assert_eq!(v["accountKey"]["projectNumber"], "18446744073709551615");
        assert_eq!(v["grantedCapabilities"], json!(["photos"]));
        for key in [
            "token",
            "url",
            "email",
            "clientId",
            "expiresAt",
            "expiresInMs",
            "credentials",
            "verifier",
        ] {
            assert!(v.get(key).is_none());
        }
        #[derive(Serialize)]
        struct Large {
            value: String,
        }
        let overhead = encode(&Large {
            value: String::new(),
        })
        .unwrap()
        .len();
        assert_eq!(
            encode(&Large {
                value: "x".repeat(WIRE_LIMIT - overhead)
            })
            .unwrap()
            .len(),
            WIRE_LIMIT
        );
        assert_eq!(
            encode(&Large {
                value: "x".repeat(WIRE_LIMIT - overhead + 1)
            }),
            Err(FacadeError::ControllerFault)
        );
    }

    #[test]
    fn proposal_mapping_preserves_tuple_binding_and_saturates_without_renewal() {
        let now = Instant::now();
        for duration in [Duration::ZERO, Duration::from_nanos(999_999)] {
            assert!(Proposal::at(proposal_at(now + duration), now).is_none());
        }
        assert!(Proposal::at(proposal_at(now), now + Duration::from_millis(1)).is_none());
        for (duration, expected) in [
            (Duration::from_millis(1), 1),
            (Duration::from_micros(1_999), 1),
            (Duration::from_secs(60), 60_000),
            (Duration::from_secs(90), 60_000),
        ] {
            let v = value(encode(&ProposalResponse {
                proposal: Proposal::at(proposal_at(now + duration), now),
            }));
            let p = &v["proposal"];
            assert_eq!(p["expiresInMs"], expected);
            assert_eq!(p["attemptId"], ID);
            assert_eq!(p["decisionId"], OTHER);
            assert_eq!(p["generation"], "18446744073709551615");
            assert_eq!(p["baselineConnectionEpoch"], "9");
            assert_eq!(
                p["candidateAccount"],
                json!({"projectNumber":"7","drivePermissionId":"syntheticB"})
            );
            assert_eq!(
                p["incumbentAccount"],
                json!({"projectNumber":"5","drivePermissionId":"syntheticA"})
            );
            assert_eq!(p["registrationId"], "desktop");
            assert_eq!(p["actualCapabilities"], json!(["photos"]));
        }
        let deadline = now + Duration::from_secs(60);
        assert_eq!(
            Proposal::at(proposal_at(deadline), now + Duration::from_secs(59))
                .unwrap()
                .expires_in_ms,
            1000
        );
        assert_eq!(
            encode(&ProposalResponse { proposal: None }).unwrap(),
            r#"{"schemaVersion":1,"proposal":null}"#
        );
    }

    #[test]
    fn task_free_getters_cancel_decision_and_disconnect_map_actual_public_owner() {
        let app = DriveApplication::from_native_records(vec![]).unwrap();
        assert_eq!(
            value(app.status(EMPTY)),
            json!({"schemaVersion":1,"controllerGeneration":"0","attemptId":null,"phase":"idle","lastAuthorization":null,"unverifiedGrantedCapabilities":[]})
        );
        assert_eq!(
            value(app.verification_status(EMPTY)),
            json!({"schemaVersion":1,"configured":false,"controllerGeneration":"0","attemptId":null,"phase":"idle","lastTerminal":null})
        );
        assert_eq!(
            value(app.connection(EMPTY)),
            json!({"schemaVersion":1,"status":"disconnected","connectionId":null,"epoch":"0","accountKey":null,"grantedCapabilities":[]})
        );
        assert_eq!(value(app.proposal(EMPTY))["proposal"], Value::Null);
        let cancel = json!({"schemaVersion":1,"attemptId":ID}).to_string();
        assert_eq!(value(app.cancel(&cancel))["outcome"], "notCurrent");
        let decide=json!({"schemaVersion":1,"attemptId":ID,"generation":"0","decisionId":OTHER,"decision":"accept"}).to_string();
        assert_eq!(value(app.decide(&decide))["outcome"], "stale");
        assert_eq!(
            value(app.disconnect(EMPTY))["outcome"],
            "ownershipInvalidated"
        );
        assert_eq!(value(app.connection(EMPTY))["epoch"], "1");
        assert_eq!(
            app.mark_closing(),
            ClosingReport {
                first: true,
                fault: None
            }
        );
        assert_eq!(
            app.mark_closing(),
            ClosingReport {
                first: false,
                fault: None
            }
        );
        assert_eq!(app.decide(&decide), Err(FacadeError::Closing));
        assert!(app.status(EMPTY).is_ok());
    }

    #[test]
    fn cleanup_classification_never_promotes_unproven_closure_or_poisoned_admission() {
        assert_eq!(
            classify_cleanup(Ok(()), None),
            CleanupResult::CleanupComplete { fault: None }
        );
        assert_eq!(
            classify_cleanup(Ok(()), Some(FacadeError::ControllerFault)),
            CleanupResult::CleanupComplete {
                fault: Some(FacadeError::ControllerFault)
            }
        );
        for fault in [
            ControllerError::Poisoned,
            ControllerError::CounterExhausted,
            ControllerError::WatcherFailure,
        ] {
            assert_eq!(
                classify_cleanup(Err(fault), None),
                CleanupResult::CleanupComplete {
                    fault: Some(FacadeError::ControllerFault)
                }
            );
        }
        assert_eq!(
            classify_cleanup(Err(ControllerError::Closed), None),
            CleanupResult::CleanupUnproven {
                fault: FacadeError::Closed
            }
        );
        let app = DriveApplication::from_native_records(vec![]).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = app.closing.lock().unwrap();
            panic!("synthetic closing poison");
        }));
        assert_eq!(app.check_open(), Err(FacadeError::ControllerFault));
        assert_eq!(
            app.mark_closing(),
            ClosingReport {
                first: true,
                fault: Some(FacadeError::ControllerFault)
            }
        );
        assert_eq!(
            app.mark_closing(),
            ClosingReport {
                first: false,
                fault: Some(FacadeError::ControllerFault)
            }
        );
    }

    #[test]
    fn empty_unknown_and_disallowed_selection_fail_without_runtime_or_sink() {
        let empty = DriveApplication::from_native_records(vec![]).unwrap();
        let sink = |_: BrowserLaunchTicket| async {
            panic!("sink must not run");
        };
        assert_eq!(
            futures::executor::block_on(empty.begin_with_sink(BEGIN, sink)),
            Err(FacadeError::NotConfigured)
        );
        let app = DriveApplication::from_native_records(vec![record(
            "desktop",
            "synthetic.apps.googleusercontent.com",
            &[ApprovedScope::AppData],
        )])
        .unwrap();
        for (raw, expected) in [
            (
                BEGIN.replace("desktop", "unknown"),
                FacadeError::UnknownRegistration,
            ),
            (
                BEGIN.replace("stateLibrary", "photos"),
                FacadeError::DisallowedCapabilities,
            ),
            (
                BEGIN.replace("stateLibrary", "drive"),
                FacadeError::InvalidRequest,
            ),
        ] {
            assert_eq!(
                futures::executor::block_on(app.begin_with_sink(&raw, sink)),
                Err(expected)
            );
        }
        assert_eq!(value(app.status(EMPTY))["attemptId"], Value::Null);
        app.mark_closing();
        assert_eq!(
            futures::executor::block_on(app.begin_with_sink(BEGIN, sink)),
            Err(FacadeError::Closing)
        );
    }

    #[tokio::test]
    async fn unpolled_and_polled_caller_drop_keep_native_ticket_cleanup() {
        let app = configured();
        let unpolled = app.begin_with_sink(BEGIN, failed_ticket);
        drop(unpolled);
        assert_eq!(value(app.status(EMPTY))["attemptId"], Value::Null);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let mut operation = Box::pin(app.begin_with_sink(BEGIN, |ticket| async move {
            ready_tx.send(()).unwrap();
            std::future::pending::<()>().await;
            drop(ticket);
            Err(LaunchError::Cancelled)
        }));
        tokio::time::timeout(Duration::from_secs(4),async {
            tokio::select! { biased; _=&mut operation => panic!("held sink completed"), result=ready_rx => result.unwrap() }
        }).await.unwrap();
        assert_eq!(value(app.status(EMPTY))["phase"], "awaitingBrowser");
        drop(operation);
        idle(&app).await;
        assert_eq!(
            value(app.status(EMPTY))["lastAuthorization"]["outcome"],
            "browserUnavailable"
        );
        shutdown(&app).await;
    }

    #[tokio::test]
    async fn duplicate_begin_busy_cancel_and_late_completion_do_not_replace_attempt() {
        let app = configured();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let mut operation = Box::pin(app.begin_with_sink(BEGIN, |ticket| async move {
            ready_tx.send(()).unwrap();
            release_rx.await.unwrap();
            ticket
                .finish(BrowserHandoff::HandedOff)
                .map_err(LaunchError::Ticket)?;
            Ok(LaunchOutcome::HandedOff)
        }));
        tokio::time::timeout(Duration::from_secs(4),async {
            tokio::select! { biased; _=&mut operation=>panic!("held sink completed"),r=ready_rx=>r.unwrap() }
        }).await.unwrap();
        let snapshot = value(app.status(EMPTY));
        let id = snapshot["attemptId"].as_str().unwrap().to_owned();
        assert_eq!(
            app.begin_with_sink(BEGIN, failed_ticket).await,
            Err(FacadeError::Busy)
        );
        assert_eq!(value(app.status(EMPTY))["attemptId"], id);
        assert_eq!(
            value(app.cancel(&json!({"schemaVersion":1,"attemptId":ID}).to_string()))["outcome"],
            "notCurrent"
        );
        let cancel = json!({"schemaVersion":1,"attemptId":id}).to_string();
        assert_eq!(value(app.cancel(&cancel))["outcome"], "cancelled");
        assert_eq!(value(app.cancel(&cancel))["outcome"], "alreadyTerminal");
        assert_eq!(
            app.begin_with_sink(BEGIN, failed_ticket).await,
            Err(FacadeError::Busy)
        );
        release_tx.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(4), operation)
                .await
                .unwrap(),
            Err(FacadeError::Stale)
        );
        idle(&app).await;
        assert_eq!(
            value(app.status(EMPTY))["lastAuthorization"]["outcome"],
            "cancelled"
        );
        shutdown(&app).await;
    }

    #[tokio::test]
    async fn actual_local_denial_before_synthetic_handoff_never_reaches_exchange() {
        use std::io::{Read, Write};
        use std::net::{SocketAddr, TcpStream};
        let app = configured();
        let outcome=tokio::time::timeout(Duration::from_secs(5),app.begin_with_sink(BEGIN,|ticket|async move {
            let url=reqwest::Url::parse(ticket.url_for_native_launcher().unwrap()).unwrap();
            let redirect=url.query_pairs().find(|(k,_)|k=="redirect_uri").unwrap().1.into_owned();
            let state=url.query_pairs().find(|(k,_)|k=="state").unwrap().1.into_owned();
            let redirect=reqwest::Url::parse(&redirect).unwrap();let port=redirect.port().unwrap();
            let target=format!("{}?state={state}&error=access_denied",redirect.path());
            // Only a denial; no Code or token/account endpoint is invoked.
            // This joined peer is bounded by connect/read/write timeouts.
            tokio::task::spawn_blocking(move|| {
                let address=SocketAddr::from(([127,0,0,1],port));
                let mut stream=TcpStream::connect_timeout(&address,Duration::from_secs(1)).unwrap();
                stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
                stream.set_write_timeout(Some(Duration::from_millis(500))).unwrap();
                stream.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                let mut response=[0;512];let _=stream.read(&mut response);
            }).await.unwrap();
            ticket.finish(BrowserHandoff::HandedOff).map_err(LaunchError::Ticket)?;Ok(LaunchOutcome::HandedOff)
        })).await.unwrap();
        assert_eq!(
            outcome.unwrap(),
            r#"{"schemaVersion":1,"outcome":"handedOff"}"#
        );
        idle(&app).await;
        assert_eq!(
            value(app.status(EMPTY))["lastAuthorization"]["outcome"],
            "denied"
        );
        assert_eq!(value(app.connection(EMPTY))["status"], "disconnected");
        shutdown(&app).await;
    }

    #[tokio::test]
    async fn closing_rejects_admission_and_shutdown_waits_for_held_ticket_ack() {
        let app = configured();
        let ticket = tokio::time::timeout(Duration::from_secs(4), app.prepare_begin(BEGIN))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            app.mark_closing(),
            ClosingReport {
                first: true,
                fault: None
            }
        );
        assert_eq!(
            app.begin_with_sink(BEGIN, failed_ticket).await,
            Err(FacadeError::Closing)
        );
        let mut cleanup = Box::pin(app.shutdown());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut cleanup)
                .await
                .is_err()
        );
        assert_eq!(value(app.status(EMPTY))["phase"], "draining");
        drop(ticket);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(4), cleanup)
                .await
                .unwrap(),
            CleanupResult::CleanupComplete { fault: None }
        );
        shutdown(&app).await;
    }

    #[tokio::test]
    async fn failed_handoff_and_disconnect_allow_only_fresh_native_attempts() {
        let app = configured();
        assert_eq!(
            tokio::time::timeout(
                Duration::from_secs(4),
                app.begin_with_sink(BEGIN, failed_ticket)
            )
            .await
            .unwrap(),
            Err(FacadeError::LaunchFailed)
        );
        idle(&app).await;
        let old = value(app.status(EMPTY))["lastAuthorization"]["attemptId"].clone();
        assert_eq!(
            value(app.disconnect(EMPTY))["outcome"],
            "ownershipInvalidated"
        );
        let ticket = tokio::time::timeout(Duration::from_secs(4), app.prepare_begin(BEGIN))
            .await
            .unwrap()
            .unwrap();
        assert_ne!(value(app.status(EMPTY))["attemptId"], old);
        let stale = json!({"schemaVersion":1,"attemptId":old}).to_string();
        assert_eq!(value(app.cancel(&stale))["outcome"], "notCurrent");
        drop(ticket);
        idle(&app).await;
        shutdown(&app).await;
    }
}

#[cfg(test)]
#[path = "application_independent_tests.rs"]
mod application_independent_tests;
