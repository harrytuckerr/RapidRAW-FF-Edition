//! Independent fixed-outcome C2 probes, migrated to C3 connected ownership.
//! Synthetic localhost only.
//! No successful Code callback is acknowledged. Reducer fixtures start no
//! listener/supervisor; every candidate comes from the real account transport.

use super::*;
use crate::drive_auth::account::session_test_support::{FixtureSpec, SyntheticLabel};
use crate::drive_auth::registration::NativeRegistration;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

const A_BODY: &[u8] = br#"{"user":{"permissionId":"independent_A"}}"#;
const B_BODY: &[u8] = br#"{"user":{"permissionId":"independent_B"}}"#;

fn native_catalog() -> NativeRegistrationCatalog {
    NativeRegistrationCatalog::from_native_records(vec![
        NativeRegistration::from_native_configuration(
            "independent",
            "independent-c2.apps.googleusercontent.com",
            "987",
            &[ApprovedScope::AppData, ApprovedScope::File],
        )
        .unwrap(),
        NativeRegistration::from_native_configuration(
            "limited",
            "independent-limited.apps.googleusercontent.com",
            "988",
            &[ApprovedScope::File],
        )
        .unwrap(),
    ])
    .unwrap()
}

fn select(scopes: &[ApprovedScope]) -> RegistrationSelection {
    RegistrationSelection::new("independent", scopes).unwrap()
}

fn raw_start() -> NativeStart {
    NativeStart::new(
        "independent-raw.apps.googleusercontent.com".into(),
        "raw".into(),
        "989".into(),
        vec![ApprovedScope::File],
    )
    .unwrap()
}

async fn reducer(scopes: &[ApprovedScope]) -> (SessionController, Work) {
    let owner = SessionController::with_catalog(native_catalog());
    let mut work = owner
        .inner
        .reserve_configured(select(scopes), Instant::now())
        .unwrap();
    ensure_watcher(&owner.inner, &mut work)
        .await
        .expect("actual production watcher readiness");
    owner.inner.offer(work.key, Instant::now()).unwrap();
    // No worker, callback or code exists in this reducer-only reservation.
    owner
        .inner
        .acknowledge(work.key, BrowserHandoff::HandedOff, Instant::now())
        .unwrap();
    assert!(owner.inner.exchanging(work.key, Instant::now()));
    (owner, work)
}

fn fixture(
    key: Key,
    address: SocketAddr,
    label: SyntheticLabel,
    actual: &[ApprovedScope],
    issued: Instant,
    seconds: u64,
) -> VerificationInput {
    VerificationInput::Fixture {
        spec: Box::new(
            FixtureSpec::new(
                label,
                ExchangeAttempt {
                    id: key.id,
                    generation: key.generation,
                },
                actual,
                issued,
                seconds,
            )
            .unwrap(),
        ),
        header: Pending::Marker {
            key,
            expires: issued + Duration::from_secs(seconds),
            scopes: actual.to_vec(),
        },
        address,
    }
}

// Own finite one-request peer: no author Server/Peer or success stub. Both I/O
// loops and the response gate observe stop and a 4s absolute budget; Drop joins.
struct AboutPeer {
    address: SocketAddr,
    halt: Arc<AtomicBool>,
    allow: Arc<AtomicBool>,
    captured: Arc<Mutex<Option<Vec<u8>>>>,
    worker: Option<JoinHandle<()>>,
}
impl AboutPeer {
    fn new(status: u16, body: &[u8], held: bool) -> Self {
        assert!(body.len() <= 1024);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let halt = Arc::new(AtomicBool::new(false));
        let allow = Arc::new(AtomicBool::new(!held));
        let captured = Arc::new(Mutex::new(None));
        let (stop, release, request) =
            (Arc::clone(&halt), Arc::clone(&allow), Arc::clone(&captured));
        let mut response = format!("HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        response.extend_from_slice(body);
        let worker = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(4);
            while !stop.load(Ordering::Acquire) && Instant::now() < end {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => return,
                };
                stream.set_nonblocking(true).unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 1024];
                while !stop.load(Ordering::Acquire) && Instant::now() < end {
                    match stream.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => {
                            if bytes.len() + n > 16 * 1024 {
                                return;
                            }
                            bytes.extend_from_slice(&buf[..n]);
                            if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => return,
                    }
                }
                if !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    return;
                }
                *request.lock().unwrap() = Some(bytes);
                while !release.load(Ordering::Acquire)
                    && !stop.load(Ordering::Acquire)
                    && Instant::now() < end
                {
                    thread::sleep(Duration::from_millis(2));
                }
                let mut offset = 0;
                while offset < response.len()
                    && !stop.load(Ordering::Acquire)
                    && Instant::now() < end
                {
                    match stream.write(&response[offset..]) {
                        Ok(0) => return,
                        Ok(n) => offset += n,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => return,
                    }
                }
                return;
            }
        });
        Self {
            address,
            halt,
            allow,
            captured,
            worker: Some(worker),
        }
    }
    async fn wait_request(&self) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if self.captured.lock().unwrap().is_some() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("bounded actual request capture");
    }
    fn wire(&self) -> String {
        String::from_utf8(
            self.captured
                .lock()
                .unwrap()
                .clone()
                .expect("captured request"),
        )
        .unwrap()
    }
}
impl Drop for AboutPeer {
    fn drop(&mut self) {
        self.halt.store(true, Ordering::Release);
        self.allow.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                assert!(result.is_ok(), "own peer joined without panic");
            }
        }
    }
}

fn check_request(peer: &AboutPeer, bearer: &str) {
    let wire = peer.wire();
    let first = wire
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(first.len(), 3);
    assert_eq!(first[0], "GET");
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    assert_eq!(url.path(), "/drive/v3/about");
    assert_eq!(
        url.query_pairs().into_owned().collect::<Vec<_>>(),
        vec![(
            "fields".to_owned(),
            "user(permissionId,displayName,emailAddress),storageQuota".to_owned()
        )]
    );
    let auth = wire
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    assert_eq!(auth, vec![bearer]);
    assert!(wire.ends_with("\r\n\r\n"));
    assert!(!wire.contains("synthetic-session-refresh"));
}

fn assert_verification_terminal(
    owner: &SessionController,
    key: Key,
    expected: VerificationOutcome,
) {
    let auth = owner.snapshot().unwrap();
    assert_eq!(
        auth.last_authorization,
        Some(TerminalRecord {
            attempt_id: key.id,
            generation: key.generation,
            outcome: AuthorizationOutcome::CompletedPendingVerification
        })
    );
    assert_eq!(auth.phase, Phase::Draining);
    assert!(auth.granted_subset_pending_verification.is_empty());
    let verification = owner.verification_snapshot().unwrap();
    assert!(verification.configured);
    assert_eq!(
        verification.last_terminal,
        Some(VerificationTerminalRecord {
            attempt_id: key.id,
            generation: key.generation,
            outcome: expected
        })
    );
    let state = owner.inner.state.lock().unwrap();
    let slot = state.slot.as_ref().unwrap();
    assert!(slot.verification == VerificationState::Finished);
    assert!(slot.pending.is_none());
    assert!(*slot.stop.borrow());
    assert_eq!(
        state.connected.is_some(),
        expected == VerificationOutcome::AdoptedSessionOnly
    );
}

// Compare the complete exposed candidate/connection metadata across Slot reaping.
// No credential extraction, fake proof, watcher state assignment or new test case.
#[derive(PartialEq, Eq)]
struct RetainedMetadata {
    connection_id: Uuid,
    epoch: u64,
    retention_deadline: Instant,
    adopted: Instant,
    account_key: AccountKey,
    granted: Vec<ApprovedScope>,
    missing: Vec<ApprovedScope>,
    credential_expiry: Instant,
}

fn retained_metadata(inner: &Inner) -> Option<RetainedMetadata> {
    let state = inner.state.lock().unwrap();
    let Some(connected) = state.connected.as_ref() else {
        assert!(
            state.identity.is_none(),
            "fresh failed/cancelled fixture has no identity"
        );
        return None;
    };
    let stamp = state.identity.as_ref().expect("adopted identity stamp");
    assert_eq!(&stamp.key, connected.candidate.account_key());
    assert_eq!(stamp.id, connected.timer.id);
    assert_eq!(state.connection_epoch, connected.timer.epoch);
    // Both fixed response bodies contain only permissionId: do not invent display data.
    let display = connected.candidate.display();
    assert!(display.display_name().is_none());
    assert!(display.email_address().is_none());
    assert!(display.quota().is_none());
    Some(RetainedMetadata {
        connection_id: connected.timer.id,
        epoch: connected.timer.epoch,
        retention_deadline: connected.timer.deadline,
        adopted: connected.adopted,
        account_key: connected.candidate.account_key().clone(),
        granted: connected.candidate.granted_scopes().to_vec(),
        missing: connected.candidate.missing_requested_scopes().to_vec(),
        credential_expiry: connected.candidate.credential_expires_at(),
    })
}

fn assert_adopted_metadata(
    owner: &SessionController,
    permission_id: &str,
    granted: &[ApprovedScope],
    missing: &[ApprovedScope],
) {
    let metadata = retained_metadata(&owner.inner).expect("whole sealed candidate retained");
    assert_eq!(metadata.account_key.project_number.value(), 987);
    assert_eq!(
        metadata.account_key.drive_permission_id.as_str(),
        permission_id
    );
    assert_eq!(metadata.granted, granted);
    assert_eq!(metadata.missing, missing);
    assert_eq!(metadata.epoch, 1);
    assert!(!metadata.connection_id.is_nil());
    let snapshot = owner.connection_snapshot().unwrap();
    assert_eq!(snapshot.status, ConnectionStatus::ConnectedSessionOnly);
    assert_eq!(snapshot.connection_id, Some(metadata.connection_id));
    assert_eq!(snapshot.epoch, metadata.epoch);
    assert_eq!(snapshot.account_key.as_ref(), Some(&metadata.account_key));
    assert_eq!(snapshot.granted_scopes, granted);
}

fn reap_reducer_preserving_connection(inner: &Inner, key: Key) {
    let before = retained_metadata(inner);
    // Only the synthetic Slot is reaped: reducer() started no listener/supervisor.
    // The real watcher remains owned and is separately closed/awaited below.
    inner.reaped(key);
    assert!(
        retained_metadata(inner) == before,
        "Slot reaping preserves all owner metadata"
    );
}

async fn finish_reducer(inner: &Arc<Inner>) {
    let mut changed = inner.changed.subscribe();
    inner.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            {
                let state = inner.state.lock().unwrap();
                assert!(
                    state.slot.is_none(),
                    "synthetic Slot reaped after verifier ended"
                );
                if state.watcher == WatcherState::Stopped {
                    return;
                }
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .expect("bounded actual watcher finalization, including owner Drop");
}

async fn real_candidate(
    owner: &SessionController,
    work: &Work,
    peer: &AboutPeer,
    label: SyntheticLabel,
    actual: &[ApprovedScope],
) -> VerifiedAccountCandidate {
    let value = fixture(work.key, peer.address, label, actual, Instant::now(), 30);
    let (context, deadline) = owner
        .inner
        .start_verification(work.key, &value, Instant::now())
        .unwrap();
    assert_eq!(deadline, work.deadline);
    let (keep, cancelled) = oneshot::channel();
    let candidate = tokio::time::timeout(Duration::from_secs(3), value.verify(context, cancelled))
        .await
        .unwrap()
        .expect("real bounded account response");
    drop(keep);
    assert_eq!(candidate.granted_scopes(), actual);
    candidate
}

async fn vacant(inner: &Arc<Inner>) {
    let mut changed = inner.changed.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if inner.state.lock().unwrap().slot.is_none() {
                return;
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .expect("actual supervisor cleanup");
}

// Only error callbacks are constructed. No Code parameter exists in this client.
async fn deny(ticket: &BrowserLaunchTicket) -> bool {
    let native = reqwest::Url::parse(ticket.url_for_native_launcher().unwrap()).unwrap();
    let pairs = native
        .query_pairs()
        .into_owned()
        .collect::<std::collections::BTreeMap<_, _>>();
    let redirect = reqwest::Url::parse(&pairs["redirect_uri"]).unwrap();
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    let address: SocketAddr = ([127, 0, 0, 1], redirect.port().unwrap()).into();
    let request = format!(
        "GET {}?state={}&error=access_denied HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        redirect.path(),
        pairs["state"],
        address.port()
    );
    tokio::task::spawn_blocking(move || {
        let mut client = TcpStream::connect_timeout(&address, Duration::from_millis(500)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client.write_all(request.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        client.take(4096).read_to_end(&mut bytes).unwrap();
        bytes.starts_with(b"HTTP/1.1 200")
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn independent_public_admission_modes_and_actual_ticket_cleanup_fence() {
    for owner in [
        SessionController::new(),
        SessionController::with_catalog(
            NativeRegistrationCatalog::from_native_records(vec![]).unwrap(),
        ),
    ] {
        assert_eq!(
            owner
                .begin_configured(select(&[ApprovedScope::File]))
                .await
                .err(),
            Some(StartError::Registration(RegistrationError::NotConfigured))
        );
        assert_eq!(owner.snapshot().unwrap().generation, 0);
        assert!(!owner.verification_snapshot().unwrap().configured);
        assert!(
            owner
                .verification_snapshot()
                .unwrap()
                .last_terminal
                .is_none()
        );
    }
    let owner = SessionController::with_catalog(native_catalog());
    for (id, scope, error) in [
        (
            "Independent",
            ApprovedScope::File,
            RegistrationError::UnknownRegistration,
        ),
        (
            "limited",
            ApprovedScope::AppData,
            RegistrationError::DisallowedScopes,
        ),
    ] {
        assert_eq!(
            owner
                .begin_configured(RegistrationSelection::new(id, &[scope]).unwrap())
                .await
                .err(),
            Some(StartError::Registration(error))
        );
        assert_eq!(owner.snapshot().unwrap().generation, 0);
    }
    for configured in [true, false] {
        let ticket = tokio::time::timeout(Duration::from_secs(3), async {
            if configured {
                owner.begin_configured(select(&[ApprovedScope::File])).await
            } else {
                owner.begin(raw_start()).await
            }
        })
        .await
        .unwrap()
        .unwrap();
        let before = owner.snapshot().unwrap();
        assert_eq!(owner.begin(raw_start()).await.err(), Some(StartError::Busy));
        assert_eq!(
            owner
                .begin_configured(select(&[ApprovedScope::File]))
                .await
                .err(),
            Some(StartError::Busy)
        );
        assert_eq!(owner.snapshot().unwrap(), before);
        assert_eq!(
            owner
                .inner
                .state
                .lock()
                .unwrap()
                .slot
                .as_ref()
                .unwrap()
                .configured_selection
                .is_some(),
            configured
        );
        assert!(deny(&ticket).await);
        ticket
            .finish(if configured {
                BrowserHandoff::Failed
            } else {
                BrowserHandoff::HandedOff
            })
            .unwrap();
        vacant(&owner.inner).await;
        assert_eq!(
            owner
                .snapshot()
                .unwrap()
                .last_authorization
                .unwrap()
                .outcome,
            if configured {
                AuthorizationOutcome::BrowserUnavailable
            } else {
                AuthorizationOutcome::Denied
            }
        );
        assert!(
            owner
                .verification_snapshot()
                .unwrap()
                .last_terminal
                .is_none()
        );
    }
    let ticket = tokio::time::timeout(
        Duration::from_secs(3),
        owner.begin_configured(select(&[ApprovedScope::File])),
    )
    .await
    .unwrap()
    .unwrap();
    let mut closing = Box::pin(owner.shutdown());
    tokio::select! { biased; _ = &mut closing => panic!("held launcher ticket must fence shutdown"),
    _ = tokio::time::sleep(Duration::from_millis(20)) => {} }
    assert_eq!(owner.snapshot().unwrap().phase, Phase::Draining);
    assert!(
        owner
            .verification_snapshot()
            .unwrap()
            .last_terminal
            .is_none()
    );
    drop(ticket);
    tokio::time::timeout(Duration::from_secs(3), closing)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn independent_actual_continuation_success_partial_grants_and_typed_failures() {
    for (label, scope, status, body, expected, bearer) in [
        (
            SyntheticLabel::A,
            ApprovedScope::File,
            200,
            A_BODY,
            VerificationOutcome::AdoptedSessionOnly,
            "Bearer synthetic-session-A",
        ),
        (
            SyntheticLabel::B,
            ApprovedScope::AppData,
            200,
            B_BODY,
            VerificationOutcome::AdoptedSessionOnly,
            "Bearer synthetic-session-B",
        ),
        (
            SyntheticLabel::A,
            ApprovedScope::File,
            200,
            &b"{}"[..],
            VerificationOutcome::Failed(AccountError::MissingUser),
            "Bearer synthetic-session-A",
        ),
        (
            SyntheticLabel::B,
            ApprovedScope::File,
            403,
            &b"SYNTHETIC_UNREFLECTED"[..],
            VerificationOutcome::Failed(AccountError::Forbidden),
            "Bearer synthetic-session-B",
        ),
    ] {
        let peer = AboutPeer::new(status, body, false);
        let (owner, mut work) = reducer(&[ApprovedScope::File, ApprovedScope::AppData]).await;
        let value = fixture(work.key, peer.address, label, &[scope], Instant::now(), 30);
        tokio::time::timeout(
            Duration::from_secs(3),
            continue_configured(&owner.inner, work.key, value, &mut work.stop),
        )
        .await
        .unwrap()
        .unwrap();
        check_request(&peer, bearer);
        assert_verification_terminal(&owner, work.key, expected);
        if expected == VerificationOutcome::AdoptedSessionOnly {
            let (permission, missing) = if scope == ApprovedScope::File {
                ("independent_A", ApprovedScope::AppData)
            } else {
                ("independent_B", ApprovedScope::File)
            };
            assert_adopted_metadata(&owner, permission, &[scope], &[missing]);
        } else {
            assert!(retained_metadata(&owner.inner).is_none());
        }
        let recorded = owner.verification_snapshot().unwrap();
        owner.inner.verification_failed(
            work.key,
            VerificationOutcome::WorkerFailure,
            Instant::now(),
        );
        assert_eq!(owner.verification_snapshot().unwrap(), recorded);
        assert_eq!(
            owner.cancel(work.key.id),
            Ok(CancelDisposition::AlreadyTerminal)
        );
        reap_reducer_preserving_connection(&owner.inner, work.key);
        assert_eq!(
            owner.verification_snapshot().unwrap().phase,
            VerificationPhase::Idle
        );
        assert_eq!(
            owner.verification_snapshot().unwrap().last_terminal,
            recorded.last_terminal
        );
        finish_reducer(&owner.inner).await;
    }
}

#[tokio::test]
async fn independent_preflight_rejects_identity_scope_and_configuration_without_terminal() {
    for defect in 0..7 {
        let peer = AboutPeer::new(200, A_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File]).await;
        let mut value = fixture(
            work.key,
            peer.address,
            SyntheticLabel::A,
            &[ApprovedScope::File],
            Instant::now(),
            30,
        );
        if let VerificationInput::Fixture {
            header: Pending::Marker { key, scopes, .. },
            ..
        } = &mut value
        {
            match defect {
                0 => key.id = Uuid::nil(),
                1 => key.generation += 1,
                2 => scopes.clear(),
                3 => *scopes = vec![ApprovedScope::File, ApprovedScope::File],
                4 => *scopes = vec![ApprovedScope::AppData],
                _ => {}
            }
        }
        if defect >= 5 {
            let mut state = owner.inner.state.lock().unwrap();
            let slot = state.slot.as_mut().unwrap();
            if defect == 5 {
                slot.registration.project_number = "988".into();
            } else {
                slot.configured_selection = None;
            }
        }
        assert_eq!(
            owner
                .inner
                .start_verification(work.key, &value, Instant::now())
                .err(),
            Some(AuthorizationOutcome::ExchangeFailure)
        );
        assert!(owner.snapshot().unwrap().last_authorization.is_none());
        assert!(
            owner
                .verification_snapshot()
                .unwrap()
                .last_terminal
                .is_none()
        );
        assert!(peer.captured.lock().unwrap().is_none());
        owner
            .inner
            .finish(work.key, AuthorizationOutcome::ExchangeFailure);
        assert!(
            owner
                .verification_snapshot()
                .unwrap()
                .last_terminal
                .is_none()
        );
        owner.inner.reaped(work.key);
        finish_reducer(&owner.inner).await;
    }
}

#[tokio::test]
async fn independent_real_candidates_recheck_current_mapping_and_exact_attempt() {
    for defect in 0..4 {
        let peer = AboutPeer::new(200, A_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File, ApprovedScope::AppData]).await;
        let candidate = real_candidate(
            &owner,
            &work,
            &peer,
            SyntheticLabel::A,
            &[ApprovedScope::File],
        )
        .await;
        assert_eq!(
            candidate.missing_requested_scopes(),
            &[ApprovedScope::AppData]
        );
        {
            let mut state = owner.inner.state.lock().unwrap();
            let slot = state.slot.as_mut().unwrap();
            match defect {
                0 => slot.registration.client_id = "other.apps.googleusercontent.com".into(),
                1 => slot.registration.project_number = "988".into(),
                2 => slot.registration.scopes = vec![ApprovedScope::File],
                _ => slot.configured_selection = None,
            }
        }
        owner
            .inner
            .verification_completed(work.key, candidate, Instant::now());
        assert_verification_terminal(&owner, work.key, VerificationOutcome::BindingMismatch);
        owner.inner.reaped(work.key);
        finish_reducer(&owner.inner).await;
    }
    for wrong_generation in [false, true] {
        let peer = AboutPeer::new(200, B_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File]).await;
        let candidate = real_candidate(
            &owner,
            &work,
            &peer,
            SyntheticLabel::B,
            &[ApprovedScope::File],
        )
        .await;
        let before = owner.verification_snapshot().unwrap();
        let wrong = if wrong_generation {
            Key {
                id: work.key.id,
                generation: work.key.generation + 1,
            }
        } else {
            Key {
                id: Uuid::nil(),
                generation: work.key.generation,
            }
        };
        owner
            .inner
            .verification_completed(wrong, candidate, Instant::now());
        assert_eq!(owner.verification_snapshot().unwrap(), before);
        owner.cancel(work.key.id).unwrap();
        assert_verification_terminal(&owner, work.key, VerificationOutcome::Cancelled);
        owner.inner.reaped(work.key);
        finish_reducer(&owner.inner).await;
    }
    let peer = AboutPeer::new(200, A_BODY, false);
    let (left, left_work) = reducer(&[ApprovedScope::File]).await;
    let foreign = real_candidate(
        &left,
        &left_work,
        &peer,
        SyntheticLabel::A,
        &[ApprovedScope::File],
    )
    .await;
    let (right, right_work) = reducer(&[ApprovedScope::File]).await;
    let unconsumed = fixture(
        right_work.key,
        peer.address,
        SyntheticLabel::B,
        &[ApprovedScope::File],
        Instant::now(),
        30,
    );
    right
        .inner
        .start_verification(right_work.key, &unconsumed, Instant::now())
        .unwrap();
    right
        .inner
        .verification_completed(right_work.key, foreign, Instant::now());
    assert_verification_terminal(&right, right_work.key, VerificationOutcome::BindingMismatch);
    left.cancel(left_work.key.id).unwrap();
    left.inner.reaped(left_work.key);
    right.inner.reaped(right_work.key);
    finish_reducer(&left.inner).await;
    finish_reducer(&right.inner).await;
}

#[tokio::test]
async fn independent_real_completion_cancel_orders_race_and_replacement_are_once_only() {
    for cancel_first in [true, false] {
        let peer = AboutPeer::new(200, A_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File]).await;
        let candidate = real_candidate(
            &owner,
            &work,
            &peer,
            SyntheticLabel::A,
            &[ApprovedScope::File],
        )
        .await;
        let authorization = owner.snapshot().unwrap().last_authorization;
        if cancel_first {
            assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
        }
        owner
            .inner
            .verification_completed(work.key, candidate, Instant::now());
        if !cancel_first {
            assert_eq!(
                owner.cancel(work.key.id),
                Ok(CancelDisposition::AlreadyTerminal)
            );
        }
        assert_verification_terminal(
            &owner,
            work.key,
            if cancel_first {
                VerificationOutcome::Cancelled
            } else {
                VerificationOutcome::AdoptedSessionOnly
            },
        );
        assert_eq!(owner.snapshot().unwrap().last_authorization, authorization);
        if !cancel_first {
            assert_adopted_metadata(&owner, "independent_A", &[ApprovedScope::File], &[]);
        }
        reap_reducer_preserving_connection(&owner.inner, work.key);
        finish_reducer(&owner.inner).await;
    }
    let peer = AboutPeer::new(200, B_BODY, false);
    let (owner, work) = reducer(&[ApprovedScope::File]).await;
    let candidate = real_candidate(
        &owner,
        &work,
        &peer,
        SyntheticLabel::B,
        &[ApprovedScope::File],
    )
    .await;
    let barrier = Barrier::new(3);
    let cancelled = thread::scope(|scope| {
        let complete = scope.spawn(|| {
            barrier.wait();
            owner
                .inner
                .verification_completed(work.key, candidate, Instant::now());
        });
        let cancel = scope.spawn(|| {
            barrier.wait();
            owner.cancel(work.key.id).unwrap()
        });
        barrier.wait();
        complete.join().unwrap();
        cancel.join().unwrap()
    });
    assert_verification_terminal(
        &owner,
        work.key,
        match cancelled {
            CancelDisposition::Cancelled => VerificationOutcome::Cancelled,
            CancelDisposition::AlreadyTerminal => VerificationOutcome::AdoptedSessionOnly,
            CancelDisposition::NotCurrent => panic!("race belongs to current reservation"),
        },
    );
    if cancelled == CancelDisposition::AlreadyTerminal {
        assert_adopted_metadata(&owner, "independent_B", &[ApprovedScope::File], &[]);
    }
    reap_reducer_preserving_connection(&owner.inner, work.key);
    finish_reducer(&owner.inner).await;
    let peer = AboutPeer::new(200, A_BODY, false);
    let (owner, work) = reducer(&[ApprovedScope::File]).await;
    let late = real_candidate(
        &owner,
        &work,
        &peer,
        SyntheticLabel::A,
        &[ApprovedScope::File],
    )
    .await;
    owner.cancel(work.key.id).unwrap();
    owner.inner.reaped(work.key);
    let replacement = owner
        .inner
        .reserve_configured(select(&[ApprovedScope::File]), Instant::now())
        .unwrap();
    owner
        .inner
        .finish(replacement.key, AuthorizationOutcome::Denied);
    let auth = owner.snapshot().unwrap();
    let verification = owner.verification_snapshot().unwrap();
    owner
        .inner
        .verification_completed(work.key, late, Instant::now());
    owner
        .inner
        .verification_failed(work.key, VerificationOutcome::WorkerFailure, Instant::now());
    assert_eq!(owner.snapshot().unwrap(), auth);
    assert_eq!(owner.verification_snapshot().unwrap(), verification);
    assert_eq!(
        auth.last_authorization.unwrap().outcome,
        AuthorizationOutcome::Denied
    );
    owner.inner.reaped(replacement.key);
    finish_reducer(&owner.inner).await;
}

#[tokio::test]
async fn independent_original_token_and_snapshot_expiry_do_not_renew_or_rewrite() {
    for token_boundary in [false, true] {
        let peer = AboutPeer::new(200, A_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File]).await;
        let candidate = real_candidate(
            &owner,
            &work,
            &peer,
            SyntheticLabel::A,
            &[ApprovedScope::File],
        )
        .await;
        let at = if token_boundary {
            candidate.credential_expires_at()
        } else {
            work.deadline
        };
        owner.inner.verification_completed(work.key, candidate, at);
        assert_verification_terminal(
            &owner,
            work.key,
            if token_boundary {
                VerificationOutcome::Failed(AccountError::TokenExpired)
            } else {
                VerificationOutcome::TimedOut
            },
        );
        assert_eq!(
            owner
                .inner
                .state
                .lock()
                .unwrap()
                .slot
                .as_ref()
                .unwrap()
                .deadline,
            work.deadline
        );
        owner.inner.reaped(work.key);
        finish_reducer(&owner.inner).await;
    }
    let peer = AboutPeer::new(200, A_BODY, false);
    let (owner, work) = reducer(&[ApprovedScope::File]).await;
    let expired = fixture(
        work.key,
        peer.address,
        SyntheticLabel::A,
        &[ApprovedScope::File],
        Instant::now() - Duration::from_secs(3),
        1,
    );
    assert_eq!(
        owner
            .inner
            .start_verification(work.key, &expired, Instant::now())
            .err(),
        Some(AuthorizationOutcome::ExchangeFailure)
    );
    assert!(
        owner
            .verification_snapshot()
            .unwrap()
            .last_terminal
            .is_none()
    );
    assert!(owner.snapshot().unwrap().last_authorization.is_none());
    let live = fixture(
        work.key,
        peer.address,
        SyntheticLabel::A,
        &[ApprovedScope::File],
        Instant::now(),
        30,
    );
    owner
        .inner
        .start_verification(work.key, &live, Instant::now())
        .unwrap();
    owner
        .inner
        .state
        .lock()
        .unwrap()
        .slot
        .as_mut()
        .unwrap()
        .deadline = Instant::now() - Duration::from_millis(1);
    assert_eq!(
        owner
            .verification_snapshot()
            .unwrap()
            .last_terminal
            .unwrap()
            .outcome,
        VerificationOutcome::TimedOut
    );
    assert_verification_terminal(&owner, work.key, VerificationOutcome::TimedOut);
    assert!(peer.captured.lock().unwrap().is_none());
    owner.inner.reaped(work.key);
    finish_reducer(&owner.inner).await;
}

#[tokio::test]
async fn independent_stalled_continuation_cancel_close_drop_and_absolute_deadline() {
    for action in 0..4 {
        let peer = AboutPeer::new(200, A_BODY, true);
        let (owner, mut work) = reducer(&[ApprovedScope::File]).await;
        let inner = Arc::clone(&owner.inner);
        let mut owner = Some(owner);
        let original = if action == 3 {
            Instant::now() + Duration::from_millis(300)
        } else {
            work.deadline
        };
        inner.state.lock().unwrap().slot.as_mut().unwrap().deadline = original;
        let value = fixture(
            work.key,
            peer.address,
            SyntheticLabel::A,
            &[ApprovedScope::File],
            Instant::now(),
            30,
        );
        let mut operation = Box::pin(continue_configured(&inner, work.key, value, &mut work.stop));
        tokio::select! { biased; result = &mut operation => panic!("held actual response completed: {result:?}"),
        _ = peer.wait_request() => {} }
        let recorded = inner.state.lock().unwrap().last;
        match action {
            0 => {
                owner.as_ref().unwrap().cancel(work.key.id).unwrap();
            }
            1 => inner.close(),
            2 => drop(owner.take()),
            _ => {}
        }
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap()
            .unwrap();
        {
            let state = inner.state.lock().unwrap();
            assert_eq!(state.last, recorded);
            assert_eq!(
                state.last.unwrap().outcome,
                AuthorizationOutcome::CompletedPendingVerification
            );
            let outcome = state.last_verification.unwrap().outcome;
            match action {
                0 => assert_eq!(outcome, VerificationOutcome::Cancelled),
                1 | 2 => assert_eq!(outcome, VerificationOutcome::Closed),
                _ => assert!(matches!(
                    outcome,
                    VerificationOutcome::TimedOut
                        | VerificationOutcome::Failed(AccountError::Deadline)
                )),
            }
            let slot = state.slot.as_ref().unwrap();
            assert_eq!(slot.phase, Phase::Draining);
            assert!(slot.pending.is_none());
            assert_eq!(slot.deadline, original);
            assert!(*slot.stop.borrow());
        }
        assert_eq!(
            inner.reserve(raw_start(), Instant::now()).err(),
            Some(if action == 1 || action == 2 {
                StartError::Controller(ControllerError::Closed)
            } else {
                StartError::Busy
            })
        );
        inner.reaped(work.key); // No listener/supervisor; owned verifier future ended above.
        finish_reducer(&inner).await;
    }
}

#[tokio::test]
async fn independent_faults_after_authorization_drop_candidates_and_keep_cleanup_fence() {
    for action in 0..3 {
        let peer = AboutPeer::new(200, A_BODY, false);
        let (owner, work) = reducer(&[ApprovedScope::File]).await;
        let candidate = real_candidate(
            &owner,
            &work,
            &peer,
            SyntheticLabel::A,
            &[ApprovedScope::File],
        )
        .await;
        let authorization = owner.snapshot().unwrap().last_authorization;
        match action {
            0 => owner
                .inner
                .finish(work.key, AuthorizationOutcome::WorkerFailure),
            1 => {
                assert!(
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        let _held = owner.inner.state.lock().unwrap();
                        panic!("independent C2 synthetic poison");
                    }))
                    .is_err()
                );
                assert_eq!(
                    owner.verification_snapshot(),
                    Err(ControllerError::Poisoned)
                );
            }
            _ => {
                owner.inner.state.lock().unwrap().generation = u64::MAX;
                assert_eq!(
                    owner.cancel(work.key.id),
                    Err(ControllerError::CounterExhausted)
                );
            }
        }
        owner
            .inner
            .verification_completed(work.key, candidate, Instant::now());
        {
            let state = owner
                .inner
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            assert_eq!(state.last, authorization);
            assert_eq!(
                state.last_verification.unwrap().outcome,
                if action == 0 {
                    VerificationOutcome::WorkerFailure
                } else {
                    VerificationOutcome::Closed
                }
            );
            assert_eq!(state.slot.as_ref().unwrap().phase, Phase::Draining);
            assert!(state.slot.as_ref().unwrap().pending.is_none());
            assert!(*work.stop.borrow());
        }
        owner.inner.reaped(work.key); // Candidate-producing reducer fixture started no worker.
        let shutdown = tokio::time::timeout(Duration::from_secs(1), owner.shutdown())
            .await
            .unwrap();
        assert_eq!(
            shutdown,
            match action {
                0 => Ok(()),
                1 => Err(ControllerError::Poisoned),
                _ => Err(ControllerError::CounterExhausted),
            }
        );
    }
}
