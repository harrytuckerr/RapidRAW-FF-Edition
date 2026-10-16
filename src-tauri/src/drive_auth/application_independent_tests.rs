//! Independent fixed facade contracts; positive DTOs are metadata, never proofs.
use super::*;
use crate::drive::model::{ProviderId, UnsignedDecimal};
use crate::drive_auth::session::BrowserHandoff;
use serde_json::{Value, json};
use std::future::Future;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const V1: &str = r#"{"schemaVersion":1}"#;
const NIL: &str = "00000000-0000-0000-0000-000000000000";
const DECISION: &str = "12345678-1234-5678-9abc-123456789abc";
const START: &str =
    r#"{"schemaVersion":1,"registrationId":"probe-native","capabilities":["stateLibrary"]}"#;

type WireMethod = fn(&DriveApplication, &str) -> Result<String, FacadeError>;
type PendingBegin<'a> = Pin<Box<dyn Future<Output = Result<String, FacadeError>> + 'a>>;

fn document(result: Result<String, FacadeError>) -> Value {
    serde_json::from_str(&result.expect("bounded facade result")).expect("valid wire JSON")
}
fn registration(
    id: &str,
    client: &str,
    project: &str,
    scopes: &[ApprovedScope],
) -> NativeRegistration {
    NativeRegistration::from_native_configuration(id, client, project, scopes).unwrap()
}
fn owner() -> DriveApplication {
    DriveApplication::from_native_records(vec![registration(
        "probe-native",
        "independent-native.apps.googleusercontent.com",
        "991",
        &[ApprovedScope::File, ApprovedScope::AppData],
    )])
    .unwrap()
}
fn cancel_request(id: &str) -> String {
    json!({"schemaVersion":1,"attemptId":id}).to_string()
}
fn decision_request(id: &str, generation: &str, decision: &str) -> String {
    json!({"schemaVersion":1,"attemptId":id,"generation":generation,"decisionId":DECISION,"decision":decision}).to_string()
}
fn metadata_key(project: u64, permission: &str) -> AccountKey {
    AccountKey {
        project_number: UnsignedDecimal::new(project),
        drive_permission_id: ProviderId::try_from(permission.to_owned()).unwrap(),
    }
}
fn metadata_proposal(deadline: Instant) -> SwitchProposalSnapshot {
    SwitchProposalSnapshot {
        attempt_id: Uuid::nil(),
        generation: 9_007_199_254_740_995,
        decision_id: Uuid::parse_str(DECISION).unwrap(),
        candidate_account: metadata_key(u64::MAX, "fixture_candidate"),
        incumbent_account: metadata_key(991, "fixture_incumbent"),
        registration_id: "probe-native".into(),
        actual_grants: vec![ApprovedScope::File],
        baseline_connection_epoch: u64::MAX,
        deadline,
    }
}
async fn settle(app: &DriveApplication) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let state = document(app.status(V1));
            if state["attemptId"].is_null() {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("actual native Slot cleanup")
}
async fn drain(app: &DriveApplication) {
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), app.shutdown())
            .await
            .expect("actual native shutdown"),
        CleanupResult::CleanupComplete { fault: None }
    );
}
async fn reserve(app: &DriveApplication) -> (BrowserLaunchTicket, PendingBegin<'_>) {
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut operation: PendingBegin<'_> =
        Box::pin(app.begin_with_sink(START, move |ticket| async move {
            assert!(send.send(ticket).is_ok());
            std::future::pending::<Result<LaunchOutcome, LaunchError>>().await
        }));
    let ticket=tokio::time::timeout(Duration::from_secs(3),async {
        tokio::select! { biased; _=&mut operation=>panic!("independent retained-ticket sink ended"), result=receive=>result.expect("real ticket delivery") }
    }).await.expect("bounded actual ticket admission");
    (ticket, operation)
}
async fn failed_handoff(ticket: BrowserLaunchTicket) -> Result<LaunchOutcome, LaunchError> {
    ticket
        .finish(BrowserHandoff::Failed)
        .map_err(LaunchError::Ticket)?;
    Err(LaunchError::LaunchFailed)
}

#[test]
fn independent_all_nine_public_codecs_enforce_exact_bytes_and_decoded_fields() {
    let methods: [WireMethod; 8] = [
        DriveApplication::options,
        DriveApplication::status,
        DriveApplication::verification_status,
        DriveApplication::connection,
        DriveApplication::proposal,
        DriveApplication::cancel,
        DriveApplication::decide,
        DriveApplication::disconnect,
    ];
    let cancel = cancel_request(NIL);
    let decide = decision_request(NIL, "18446744073709551615", "decline");
    for (index, method) in methods.into_iter().enumerate() {
        let raw = match index {
            5 => cancel.as_str(),
            6 => decide.as_str(),
            _ => V1,
        };
        let app = DriveApplication::from_native_records(vec![]).unwrap();
        let exact = format!("{raw}{}", " ".repeat(4096 - raw.len()));
        assert_eq!(exact.len(), 4096);
        assert!(method(&app, &exact).is_ok());
        assert_eq!(
            method(&app, &(exact + " ")),
            Err(FacadeError::InvalidRequest)
        );
        for bad in [
            raw.replace(":1", ":2"),
            raw.replace(":1", ":1.0"),
            raw.replace(":1", ":\"1\""),
            format!("{raw} null"),
            format!("{},\"schema\\u0056ersion\":1}}", &raw[..raw.len() - 1]),
            format!("{},\"clientId\":\"TEST_ONLY\"}}", &raw[..raw.len() - 1]),
        ] {
            assert_eq!(method(&app, &bad), Err(FacadeError::InvalidRequest));
        }
        assert!(method(&app, &raw.replace("schemaVersion", "schema\\u0056ersion")).is_ok());
    }
    let empty = DriveApplication::from_native_records(vec![]).unwrap();
    let exact = format!("{START}{}", " ".repeat(4096 - START.len()));
    assert_eq!(
        futures::executor::block_on(empty.begin(&exact)),
        Err(FacadeError::NotConfigured)
    );
    for bad in [
        exact + " ",
        START.replace(":1", ":null"),
        START
            .replace("registrationId", "registration\\u0049d")
            .replace(
                "\"capabilities\"",
                "\"registrationId\":\"probe-native\",\"capabilities\"",
            ),
        START.replace("\"capabilities\"", "\"url\":\"TEST_ONLY\",\"capabilities\""),
        format!("{START}{{}}"),
    ] {
        assert_eq!(
            futures::executor::block_on(empty.begin(&bad)),
            Err(FacadeError::InvalidRequest)
        );
    }
    for id in [NIL, DECISION] {
        assert!(empty.cancel(&cancel_request(id)).is_ok());
        for generation in ["0", "9007199254740993", "18446744073709551615"] {
            assert_eq!(
                document(empty.decide(&decision_request(id, generation, "accept")))["outcome"],
                "stale"
            );
        }
    }
    for id in [
        "12345678-1234-5678-9ABC-123456789ABC",
        "12345678123456789abc123456789abc",
        "{00000000-0000-0000-0000-000000000000}",
    ] {
        assert_eq!(
            empty.cancel(&cancel_request(id)),
            Err(FacadeError::InvalidRequest)
        );
    }
    for generation in [
        "00",
        "01",
        "-1",
        "+1",
        "1.0",
        "1e1",
        " 1",
        "18446744073709551616",
    ] {
        assert_eq!(
            empty.decide(&decision_request(NIL, generation, "decline")),
            Err(FacadeError::InvalidRequest)
        );
    }
    for raw in [
        cancel.replace("}", ",\"generation\":\"0\"}"),
        cancel.replace(
            "}",
            ",\"attempt\\u0049d\":\"00000000-0000-0000-0000-000000000000\"}",
        ),
    ] {
        assert_eq!(empty.cancel(&raw), Err(FacadeError::InvalidRequest));
    }
    for raw in [
        decide.replace("decline", "Decline"),
        decide.replace("\"18446744073709551615\"", "18446744073709551615"),
        decide.replace("}", ",\"generation\":\"0\"}"),
        decide.replace("}", ",\"accountKey\":{}}"),
        decide.replace(
            "}",
            ",\"decision\\u0049d\":\"00000000-0000-0000-0000-000000000000\"}",
        ),
    ] {
        assert_eq!(empty.decide(&raw), Err(FacadeError::InvalidRequest));
    }
}

#[test]
fn independent_task_free_catalog_options_and_scope_rejections_do_not_dispatch() {
    let empty = DriveApplication::from_native_records(vec![]).unwrap();
    assert_eq!(
        empty.options(V1).unwrap(),
        r#"{"schemaVersion":1,"configured":false,"registrations":[]}"#
    );
    assert_eq!(
        futures::executor::block_on(empty.begin(START)),
        Err(FacadeError::NotConfigured)
    );
    let records = (0..8)
        .map(|i| {
            registration(
                &format!("{}{}", i, "q".repeat(127)),
                &format!("probe{i}.apps.googleusercontent.com"),
                "991",
                &[ApprovedScope::File],
            )
        })
        .collect();
    let app = DriveApplication::from_native_records(records).unwrap();
    let text = app.options(V1).unwrap();
    let output: Value = serde_json::from_str(&text).unwrap();
    assert!(text.len() < 4096);
    assert_eq!(output["configured"], true);
    assert_eq!(output["registrations"].as_array().unwrap().len(), 8);
    for item in output["registrations"].as_array().unwrap() {
        assert_eq!(item.as_object().unwrap().len(), 2);
        assert_eq!(item["id"].as_str().unwrap().len(), 128);
        assert_eq!(item["allowedCapabilities"], json!(["photos"]));
    }
    for forbidden in [
        "clientId",
        "projectNumber",
        "token",
        "apps.googleusercontent",
        "https:",
    ] {
        assert!(!text.contains(forbidden));
    }
    let nine = (0..9)
        .map(|i| {
            registration(
                &format!("probe{i}"),
                &format!("probe{i}.apps.googleusercontent.com"),
                "991",
                &[ApprovedScope::File],
            )
        })
        .collect();
    assert!(matches!(
        DriveApplication::from_native_records(nine),
        Err(FacadeError::InvalidRequest)
    ));
    for (id, client) in [
        ("probe-native", "other.apps.googleusercontent.com"),
        ("different", "independent-native.apps.googleusercontent.com"),
    ] {
        assert!(matches!(
            DriveApplication::from_native_records(vec![
                registration(
                    "probe-native",
                    "independent-native.apps.googleusercontent.com",
                    "991",
                    &[ApprovedScope::AppData]
                ),
                registration(id, client, "992", &[ApprovedScope::File])
            ]),
            Err(FacadeError::InvalidRequest)
        ));
    }
    for (choices, expected) in [
        (r#"["stateLibrary"]"#, vec![ApprovedScope::AppData]),
        (r#"["photos"]"#, vec![ApprovedScope::File]),
        (
            r#"["photos","stateLibrary"]"#,
            vec![ApprovedScope::AppData, ApprovedScope::File],
        ),
    ] {
        let raw = format!(
            r#"{{"schemaVersion":1,"registrationId":"probe-native","capabilities":{choices}}}"#
        );
        assert_eq!(
            decode::<BeginRequest>(&raw)
                .unwrap()
                .selection()
                .unwrap()
                .requested_scopes(),
            expected
        );
    }
    let restricted = DriveApplication::from_native_records(vec![registration(
        "probe-native",
        "independent-native.apps.googleusercontent.com",
        "991",
        &[ApprovedScope::AppData],
    )])
    .unwrap();
    for (request, error) in [
        (
            START.replace("probe-native", "unknown"),
            FacadeError::UnknownRegistration,
        ),
        (
            START.replace("stateLibrary", "photos"),
            FacadeError::DisallowedCapabilities,
        ),
        (
            START.replace("stateLibrary", "drive"),
            FacadeError::InvalidRequest,
        ),
        (
            START.replace("[\"stateLibrary\"]", "[]"),
            FacadeError::InvalidRequest,
        ),
        (
            START.replace("[\"stateLibrary\"]", "[\"photos\",\"photos\"]"),
            FacadeError::InvalidRequest,
        ),
        (
            START.replace("probe-native", &"a".repeat(129)),
            FacadeError::InvalidRequest,
        ),
    ] {
        assert_eq!(
            futures::executor::block_on(restricted.begin_with_sink(&request, |_| async {
                panic!("rejected selection dispatched")
            })),
            Err(error)
        );
        assert!(document(restricted.status(V1))["attemptId"].is_null());
    }
}

#[test]
fn independent_metadata_precision_response_limits_and_unverified_labels_are_fixed() {
    let status = Status::from(AuthSnapshot {
        generation: u64::MAX,
        attempt_id: Some(Uuid::nil()),
        phase: Phase::Draining,
        last_authorization: Some(TerminalRecord {
            attempt_id: Uuid::nil(),
            generation: 9_007_199_254_740_995,
            outcome: AuthorizationOutcome::CompletedPendingVerification,
        }),
        granted_subset_pending_verification: vec![ApprovedScope::File],
    });
    assert_eq!(
        document(encode(&status)),
        json!({"schemaVersion":1,"controllerGeneration":"18446744073709551615","attemptId":NIL,"phase":"draining","lastAuthorization":{"attemptId":NIL,"generation":"9007199254740995","outcome":"completedPendingVerification"},"unverifiedGrantedCapabilities":["photos"]})
    );
    let verification = VerificationStatus::from(VerificationSnapshot {
        configured: true,
        generation: u64::MAX,
        attempt_id: None,
        phase: VerificationPhase::Idle,
        last_terminal: Some(VerificationTerminalRecord {
            attempt_id: Uuid::nil(),
            generation: 0,
            outcome: VerificationOutcome::Failed(AccountError::Forbidden),
        }),
    });
    assert_eq!(
        document(encode(&verification)),
        json!({"schemaVersion":1,"configured":true,"controllerGeneration":"18446744073709551615","attemptId":null,"phase":"idle","lastTerminal":{"attemptId":NIL,"generation":"0","outcome":"verificationFailed","cause":"accessDenied"}})
    );
    let connected = Connection::from(ConnectionSnapshot {
        status: ConnectionStatus::ConnectedSessionOnly,
        connection_id: Some(Uuid::nil()),
        epoch: u64::MAX,
        account_key: Some(metadata_key(u64::MAX, "fixture_permission")),
        granted_scopes: vec![ApprovedScope::File],
    });
    assert_eq!(
        document(encode(&connected)),
        json!({"schemaVersion":1,"status":"connectedSessionOnly","connectionId":NIL,"epoch":"18446744073709551615","accountKey":{"projectNumber":"18446744073709551615","drivePermissionId":"fixture_permission"},"grantedCapabilities":["photos"]})
    );
    #[derive(Serialize)]
    struct Payload<'a> {
        marker: &'a str,
    }
    const EMPTY_ENVELOPE: &str = r#"{"schemaVersion":1,"marker":""}"#;
    let exact = "x".repeat(4096 - EMPTY_ENVELOPE.len());
    assert_eq!(encode(&Payload { marker: &exact }).unwrap().len(), 4096);
    assert_eq!(
        encode(&Payload {
            marker: &(exact.clone() + "x")
        }),
        Err(FacadeError::ControllerFault)
    );
    let escaped = "\"".repeat((4096 - EMPTY_ENVELOPE.len()) / 2 + 1);
    assert_eq!(
        encode(&Payload { marker: &escaped }),
        Err(FacadeError::ControllerFault)
    );
    let app = DriveApplication::from_native_records(vec![]).unwrap();
    let error = app
        .status(r#"{"schemaVersion":1,"TEST_ONLY_SECRET_PATH_GRANT":"fake"}"#)
        .unwrap_err();
    assert_eq!(serde_json::to_string(&error).unwrap(), "\"invalidRequest\"");
}

#[test]
fn independent_proposal_clock_is_floored_capped_and_never_renews_binding() {
    let origin = Instant::now();
    let deadline = origin + Duration::from_secs(61);
    for (observed, expected) in [
        (origin, Some(60_000)),
        (origin + Duration::from_millis(60_998), Some(2)),
        (origin + Duration::from_micros(60_998_999), Some(1)),
        (origin + Duration::from_millis(60_999), Some(1)),
        (deadline - Duration::from_nanos(999_999), None),
        (deadline, None),
        (deadline + Duration::from_millis(1), None),
    ] {
        let response = document(encode(&ProposalResponse {
            proposal: Proposal::at(metadata_proposal(deadline), observed),
        }));
        if let Some(ms) = expected {
            assert_eq!(
                response["proposal"],
                json!({"attemptId":NIL,"generation":"9007199254740995","decisionId":DECISION,"candidateAccount":{"projectNumber":"18446744073709551615","drivePermissionId":"fixture_candidate"},"incumbentAccount":{"projectNumber":"991","drivePermissionId":"fixture_incumbent"},"registrationId":"probe-native","actualCapabilities":["photos"],"baselineConnectionEpoch":"18446744073709551615","expiresInMs":ms})
            );
        } else {
            assert!(response["proposal"].is_null());
        }
    }
    assert!(Proposal::at(metadata_proposal(origin - Duration::from_millis(1)), origin).is_none());
}

#[test]
fn independent_closed_error_vocabularies_and_cleanup_proof_classification() {
    for (error, label) in [
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
        assert_eq!(serde_json::to_value(error).unwrap(), json!(label));
    }
    for (phase, label) in [
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
        assert_eq!(
            serde_json::to_value(PhaseWire::from(phase)).unwrap(),
            json!(label)
        );
    }
    for (native, label) in [
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
        assert_eq!(
            serde_json::to_value(AuthorizationWire::from(native)).unwrap(),
            json!(label)
        );
    }
    for (error, expected) in [
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
        assert_eq!(launch_error(error), expected);
    }
    assert_eq!(
        start_error(StartError::Terminated),
        FacadeError::AttemptEnded
    );
    assert_eq!(start_error(StartError::Busy), FacadeError::Busy);
    for (error, label) in [
        (AccountError::Unauthorized, "authRequired"),
        (AccountError::Forbidden, "accessDenied"),
        (AccountError::RateLimited, "unavailable"),
        (AccountError::ClientSetup, "setupFailed"),
        (AccountError::Cancelled, "cancelled"),
        (AccountError::Deadline, "timedOut"),
        (AccountError::AttemptMismatch, "bindingMismatch"),
        (AccountError::ProviderErrorPayload, "invalidResponse"),
    ] {
        assert_eq!(
            serde_json::to_value(account_error(error)).unwrap(),
            json!(label)
        );
    }
    for fault in [
        ControllerError::Poisoned,
        ControllerError::CounterExhausted,
        ControllerError::WatcherFailure,
    ] {
        assert_eq!(controller_error(fault), FacadeError::ControllerFault);
        assert_eq!(
            classify_cleanup(Err(fault), None),
            CleanupResult::CleanupComplete {
                fault: Some(FacadeError::ControllerFault)
            }
        );
    }
    assert_eq!(
        classify_cleanup(
            Err(ControllerError::Closed),
            Some(FacadeError::ControllerFault)
        ),
        CleanupResult::CleanupUnproven {
            fault: FacadeError::Closed
        }
    );
    assert_eq!(
        classify_cleanup(Ok(()), Some(FacadeError::ControllerFault)),
        CleanupResult::CleanupComplete {
            fault: Some(FacadeError::ControllerFault)
        }
    );
    let app = DriveApplication::from_native_records(vec![]).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lock = app.closing.lock().unwrap();
            panic!("independent application gate poison");
        }))
        .is_err()
    );
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

#[tokio::test]
async fn independent_current_ticket_busy_cancel_and_late_finish_preserve_authority() {
    let app = owner();
    let (ticket, operation) = reserve(&app).await;
    let current = document(app.status(V1));
    let id = current["attemptId"].as_str().unwrap();
    let generation = current["controllerGeneration"].as_str().unwrap();
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::Busy)
    );
    assert_eq!(document(app.status(V1))["attemptId"], id);
    assert_eq!(
        document(app.cancel(&cancel_request(NIL)))["outcome"],
        "notCurrent"
    );
    assert_eq!(
        document(app.cancel(&cancel_request(id)))["outcome"],
        "cancelled"
    );
    assert_eq!(
        document(app.cancel(&cancel_request(id)))["outcome"],
        "alreadyTerminal"
    );
    let cancelled = document(app.status(V1));
    assert_eq!(cancelled["phase"], "draining");
    assert_eq!(cancelled["lastAuthorization"]["attemptId"], id);
    assert_eq!(cancelled["lastAuthorization"]["generation"], generation);
    assert_eq!(cancelled["lastAuthorization"]["outcome"], "cancelled");
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::Busy)
    );
    assert_eq!(
        ticket.finish(BrowserHandoff::HandedOff),
        Err(TicketError::Stale)
    );
    drop(operation);
    let ended = settle(&app).await;
    assert_eq!(ended["lastAuthorization"], cancelled["lastAuthorization"]);
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::LaunchFailed)
    );
    let fresh = settle(&app).await;
    assert_ne!(fresh["lastAuthorization"]["attemptId"], id);
    assert_eq!(fresh["lastAuthorization"]["outcome"], "browserUnavailable");
    assert_eq!(
        document(app.cancel(&cancel_request(id)))["outcome"],
        "notCurrent"
    );
    assert_eq!(document(app.connection(V1))["status"], "disconnected");
    drain(&app).await;
}

#[tokio::test]
async fn independent_unpolled_and_sink_owned_drop_differ_from_external_ticket_fence() {
    let app = owner();
    let initial = document(app.status(V1));
    drop(app.begin(START));
    assert_eq!(document(app.status(V1)), initial);
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut operation = Box::pin(app.begin_with_sink(START, |ticket| async move {
        assert!(send.send(()).is_ok());
        std::future::pending::<()>().await;
        drop(ticket);
        Err(LaunchError::Cancelled)
    }));
    tokio::time::timeout(Duration::from_secs(3),async{tokio::select!{biased; _=&mut operation=>panic!("sink-owned ticket unexpectedly finished"),result=receive=>result.unwrap()}}).await.unwrap();
    drop(operation);
    let ended = settle(&app).await;
    assert_eq!(ended["lastAuthorization"]["outcome"], "browserUnavailable");
    let (ticket, pending) = reserve(&app).await;
    drop(pending);
    assert_eq!(document(app.status(V1))["phase"], "awaitingBrowser");
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::Busy)
    );
    let mut cleanup = Box::pin(app.shutdown());
    assert!(
        tokio::time::timeout(Duration::from_millis(60), &mut cleanup)
            .await
            .is_err()
    );
    assert_eq!(document(app.status(V1))["phase"], "draining");
    drop(ticket);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), cleanup)
            .await
            .unwrap(),
        CleanupResult::CleanupComplete { fault: None }
    );
}

#[tokio::test]
async fn independent_closing_is_idempotent_and_shutdown_waits_for_genuine_release() {
    let app = owner();
    let (ticket, operation) = reserve(&app).await;
    let id = document(app.status(V1))["attemptId"]
        .as_str()
        .unwrap()
        .to_owned();
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
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::Closing)
    );
    assert_eq!(
        app.decide(&decision_request(&id, "0", "decline")),
        Err(FacadeError::Closing)
    );
    assert_eq!(
        document(app.cancel(&cancel_request(&id)))["outcome"],
        "alreadyTerminal"
    );
    assert_eq!(
        document(app.disconnect(V1))["outcome"],
        "ownershipInvalidated"
    );
    let mut first = Box::pin(app.shutdown());
    assert!(
        tokio::time::timeout(Duration::from_millis(60), &mut first)
            .await
            .is_err()
    );
    drop(first);
    let mut second = Box::pin(app.shutdown());
    assert!(
        tokio::time::timeout(Duration::from_millis(60), &mut second)
            .await
            .is_err()
    );
    assert_eq!(document(app.status(V1))["phase"], "draining");
    assert_eq!(app.cancel(&cancel_request(&id)), Err(FacadeError::Closed));
    assert_eq!(app.disconnect(V1), Err(FacadeError::Closed));
    drop(operation);
    drop(ticket);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), second)
            .await
            .unwrap(),
        CleanupResult::CleanupComplete { fault: None }
    );
    drain(&app).await;
    assert_eq!(document(app.status(V1))["phase"], "closed");
    assert_eq!(document(app.connection(V1))["status"], "closed");
    assert_eq!(document(app.options(V1))["configured"], true);
}

struct DenialPeer {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Vec<u8>>>,
}
impl Drop for DenialPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}
impl DenialPeer {
    fn start(ticket: &BrowserLaunchTicket) -> Self {
        let url = reqwest::Url::parse(ticket.url_for_native_launcher().unwrap()).unwrap();
        let redirect = url
            .query_pairs()
            .find(|(k, _)| k == "redirect_uri")
            .unwrap()
            .1
            .into_owned();
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let redirect = reqwest::Url::parse(&redirect).unwrap();
        assert_eq!(redirect.host_str(), Some("127.0.0.1"));
        assert_eq!(redirect.path(), "/oauth2/callback/google");
        let port = redirect.port().unwrap();
        let target = format!("{}?error=access_denied&state={state}", redirect.path());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            let mut stream =
                TcpStream::connect_timeout(&address, Duration::from_millis(500)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            stream.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
            let mut bytes = Vec::with_capacity(2048);
            while !stopped.load(Ordering::Acquire)
                && Instant::now() < deadline
                && bytes.len() < 2048
            {
                let mut part = [0u8; 256];
                let cap = (2048 - bytes.len()).min(part.len());
                match stream.read(&mut part[..cap]) {
                    Ok(0) => break,
                    Ok(n) => bytes.extend_from_slice(&part[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        ) => {}
                    Err(_) => break,
                }
            }
            bytes
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    async fn response(mut self) -> Vec<u8> {
        while !self.thread.as_ref().unwrap().is_finished() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        match self.thread.take().unwrap().join() {
            Ok(bytes) => bytes,
            Err(_) => panic!("independent denial peer failed"),
        }
    }
}
#[tokio::test]
async fn independent_joined_local_denial_before_ack_never_authenticates() {
    let app = owner();
    assert_eq!(
        app.begin_with_sink(START, failed_handoff).await,
        Err(FacadeError::LaunchFailed)
    );
    let failed = settle(&app).await;
    let old = failed["lastAuthorization"]["attemptId"].clone();
    let app_ref = &app;
    let previous = &failed;
    let result = tokio::time::timeout(
        Duration::from_secs(4),
        app.begin_with_sink(START, |ticket| async move {
            let peer = DenialPeer::start(&ticket);
            let response = peer.response().await;
            assert!(response.starts_with(b"HTTP/1.1 200"));
            let pending = document(app_ref.status(V1));
            assert_eq!(pending["phase"], "awaitingBrowser");
            assert_eq!(pending["lastAuthorization"], previous["lastAuthorization"]);
            ticket
                .finish(BrowserHandoff::HandedOff)
                .map_err(LaunchError::Ticket)?;
            Ok(LaunchOutcome::HandedOff)
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        result.unwrap(),
        r#"{"schemaVersion":1,"outcome":"handedOff"}"#
    );
    let ended = settle(&app).await;
    assert_ne!(ended["lastAuthorization"]["attemptId"], old);
    assert_eq!(ended["lastAuthorization"]["outcome"], "denied");
    assert_eq!(ended["unverifiedGrantedCapabilities"], json!([]));
    assert_eq!(document(app.connection(V1))["status"], "disconnected");
    assert!(document(app.verification_status(V1))["lastTerminal"].is_null());
    drain(&app).await;
}
