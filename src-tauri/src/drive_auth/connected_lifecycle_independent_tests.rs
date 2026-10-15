//! Independent C3 lifecycle cases: real private reducers, watcher and synthetic HTTP.
//! No successful Code handoff or external/provider entry. Fixed synthetic identity
//! mapping is fixture custody only, not authentication or application readiness.

use super::*;
use crate::drive_auth::account::session_test_support::{FixtureSpec, SyntheticLabel};
use crate::drive_auth::registration::NativeRegistration;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

const A: &[u8] = br#"{"user":{"permissionId":"c3_independent_A"}}"#;
const B: &[u8] = br#"{"user":{"permissionId":"c3_independent_B"}}"#;
const BOTH: &[ApprovedScope] = &[ApprovedScope::AppData, ApprovedScope::File];
const FILE: &[ApprovedScope] = &[ApprovedScope::File];

fn catalog() -> NativeRegistrationCatalog {
    NativeRegistrationCatalog::from_native_records(vec![
        NativeRegistration::from_native_configuration(
            "primary",
            "c3-independent.apps.googleusercontent.com",
            "987",
            BOTH,
        )
        .unwrap(),
        NativeRegistration::from_native_configuration(
            "other-project",
            "c3-independent-other.apps.googleusercontent.com",
            "654",
            BOTH,
        )
        .unwrap(),
    ])
    .unwrap()
}
fn selection(id: &str) -> RegistrationSelection {
    RegistrationSelection::new(id, BOTH).unwrap()
}
fn raw_start() -> NativeStart {
    NativeStart::new(
        "c3-raw.apps.googleusercontent.com".into(),
        "raw".into(),
        "321".into(),
        FILE.to_vec(),
    )
    .unwrap()
}
fn raw(inner: &Inner) -> MutexGuard<'_, State> {
    inner.state.lock().unwrap_or_else(|p| p.into_inner())
}
async fn wait_state(inner: &Inner, predicate: impl Fn(&State) -> bool) {
    let mut changed = inner.changed.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if predicate(&raw(inner)) {
                return;
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .expect("bounded actual state notification");
}
async fn reserve(owner: &SessionController, id: &str) -> Work {
    let mut work = owner
        .inner
        .reserve_configured(selection(id), Instant::now())
        .unwrap();
    ensure_watcher(&owner.inner, &mut work)
        .await
        .expect("real watcher readiness");
    owner.inner.offer(work.key, Instant::now()).unwrap();
    // Reducer-only: no listener, supervisor, callback or Code exists.
    owner
        .inner
        .acknowledge(work.key, BrowserHandoff::HandedOff, Instant::now())
        .unwrap();
    assert!(owner.inner.exchanging(work.key, Instant::now()));
    work
}
fn input(
    work: &Work,
    peer: &Peer,
    label: SyntheticLabel,
    granted: &[ApprovedScope],
    issued: Instant,
    seconds: u64,
) -> VerificationInput {
    VerificationInput::Fixture {
        spec: Box::new(
            FixtureSpec::new(
                label,
                ExchangeAttempt {
                    id: work.key.id,
                    generation: work.key.generation,
                },
                granted,
                issued,
                seconds,
            )
            .unwrap(),
        ),
        header: Pending::Marker {
            key: work.key,
            expires: issued + Duration::from_secs(seconds),
            scopes: granted.to_vec(),
        },
        address: peer.address,
    }
}
async fn candidate(
    owner: &SessionController,
    work: &Work,
    label: SyntheticLabel,
    granted: &[ApprovedScope],
    issued: Instant,
    seconds: u64,
) -> VerifiedAccountCandidate {
    let (body, bearer) = match label {
        SyntheticLabel::A => (A, "Bearer synthetic-session-A"),
        SyntheticLabel::B => (B, "Bearer synthetic-session-B"),
    };
    let peer = Peer::new(200, body, false);
    let value = input(work, &peer, label, granted, issued, seconds);
    let (context, deadline) = owner
        .inner
        .start_verification(work.key, &value, Instant::now())
        .unwrap();
    assert_eq!(deadline, work.deadline);
    let (keep, cancelled) = oneshot::channel();
    let result = tokio::time::timeout(Duration::from_secs(3), value.verify(context, cancelled))
        .await
        .unwrap()
        .expect("actual restricted HTTP verifier");
    drop(keep);
    check_request(&peer, bearer);
    assert_eq!(result.granted_scopes(), granted);
    assert_eq!(
        result.credential_expires_at(),
        issued + Duration::from_secs(seconds)
    );
    result
}
async fn adopt(
    owner: &SessionController,
    label: SyntheticLabel,
    granted: &[ApprovedScope],
    seconds: u64,
) -> OwnedMetadata {
    let work = reserve(owner, "primary").await;
    let proof = candidate(owner, &work, label, granted, Instant::now(), seconds).await;
    owner
        .inner
        .verification_completed(work.key, proof, Instant::now());
    assert_eq!(verification(owner), VerificationOutcome::AdoptedSessionOnly);
    let before = metadata(&owner.inner).expect("whole sealed candidate retained");
    owner.inner.reaped(work.key); // Synthetic reservation, no listener/supervisor.
    assert!(metadata(&owner.inner).as_ref() == Some(&before));
    before
}
fn verification(owner: &SessionController) -> VerificationOutcome {
    owner
        .verification_snapshot()
        .unwrap()
        .last_terminal
        .unwrap()
        .outcome
}
#[derive(PartialEq, Eq)]
struct OwnedMetadata {
    id: Uuid,
    epoch: u64,
    key: AccountKey,
    grants: Vec<ApprovedScope>,
    missing: Vec<ApprovedScope>,
    adopted: Instant,
    deadline: Instant,
    credential_expiry: Instant,
}
// Raw State intentionally bypasses expiry reducers; scheduled tests call this
// BEFORE any getter/shared lock can manufacture an expiry observation.
fn metadata(inner: &Inner) -> Option<OwnedMetadata> {
    let state = raw(inner);
    state.connected.as_ref().map(|owner| {
        let stamp = state.identity.as_ref().unwrap();
        assert_eq!(&stamp.key, owner.candidate.account_key());
        assert_eq!(stamp.id, owner.timer.id);
        assert_eq!(state.connection_epoch, owner.timer.epoch);
        OwnedMetadata {
            id: owner.timer.id,
            epoch: owner.timer.epoch,
            key: owner.candidate.account_key().clone(),
            grants: owner.candidate.granted_scopes().to_vec(),
            missing: owner.candidate.missing_requested_scopes().to_vec(),
            adopted: owner.adopted,
            deadline: owner.timer.deadline,
            credential_expiry: owner.candidate.credential_expires_at(),
        }
    })
}
async fn close(owner: &SessionController, expected: Result<(), ControllerError>) {
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), owner.shutdown())
            .await
            .unwrap(),
        expected
    );
    let state = raw(&owner.inner);
    assert!(state.slot.is_none());
    assert!(watcher_finished(&state));
    assert!(state.connected.is_none());
}
fn gate() -> (WatcherTestGate, oneshot::Receiver<()>, oneshot::Sender<()>) {
    let (entered, seen) = oneshot::channel();
    let (release, wait) = oneshot::channel();
    (
        WatcherTestGate {
            entered,
            release: wait,
        },
        seen,
        release,
    )
}
async fn seen(receiver: oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
}
// Own finite one-request peer: no author Server/Peer or success stub. Both I/O
// loops and the response gate observe stop and a 4s absolute budget; Drop joins.
struct Peer {
    address: SocketAddr,
    halt: Arc<AtomicBool>,
    allow: Arc<AtomicBool>,
    captured: Arc<Mutex<Option<Vec<u8>>>>,
    worker: Option<JoinHandle<()>>,
}
impl Peer {
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
impl Drop for Peer {
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

fn check_request(peer: &Peer, bearer: &str) {
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

#[tokio::test]
async fn independent_whole_owner_partial_replacement_and_reconnect_winners() {
    let owner = SessionController::with_catalog(catalog());
    let first = adopt(&owner, SyntheticLabel::A, BOTH, 60).await;
    assert_eq!(first.key.project_number.value(), 987);
    assert_eq!(first.key.drive_permission_id.as_str(), "c3_independent_A");
    assert_eq!(first.grants, BOTH);
    assert!(first.missing.is_empty());
    let partial = adopt(&owner, SyntheticLabel::A, FILE, 60).await;
    assert_eq!(partial.key, first.key);
    assert_ne!(partial.id, first.id);
    assert_eq!(partial.epoch, first.epoch + 1);
    assert_eq!(partial.grants, FILE);
    assert_eq!(partial.missing, &[ApprovedScope::AppData]);
    assert_eq!(raw(&owner.inner).watcher_test.starts, 1);

    let peer = Peer::new(403, b"SYNTHETIC_BODY_NOT_REFLECTED", false);
    let mut work = reserve(&owner, "primary").await;
    let value = input(&work, &peer, SyntheticLabel::A, FILE, Instant::now(), 30);
    tokio::time::timeout(
        Duration::from_secs(3),
        continue_configured(&owner.inner, work.key, value, &mut work.stop),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        verification(&owner),
        VerificationOutcome::Failed(AccountError::Forbidden)
    );
    assert!(metadata(&owner.inner).as_ref() == Some(&partial));
    owner.inner.reaped(work.key);
    let peer = Peer::new(200, A, true);
    let mut work = reserve(&owner, "primary").await;
    let value = input(&work, &peer, SyntheticLabel::A, BOTH, Instant::now(), 30);
    let mut operation = Box::pin(continue_configured(
        &owner.inner,
        work.key,
        value,
        &mut work.stop,
    ));
    tokio::select! { biased; result = &mut operation => panic!("held reconnect unexpectedly completed: {result:?}"), _ = peer.wait_request() => {} }
    assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
    tokio::time::timeout(Duration::from_secs(2), operation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verification(&owner), VerificationOutcome::Cancelled);
    assert!(metadata(&owner.inner).as_ref() == Some(&partial));
    owner.inner.reaped(work.key);

    for cancel_first in [true, false] {
        let incumbent = metadata(&owner.inner).unwrap();
        let work = reserve(&owner, "primary").await;
        let proof = candidate(&owner, &work, SyntheticLabel::A, FILE, Instant::now(), 60).await;
        if cancel_first {
            assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
        }
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now());
        if cancel_first {
            assert_eq!(verification(&owner), VerificationOutcome::Cancelled);
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        } else {
            assert_eq!(
                owner.cancel(work.key.id),
                Ok(CancelDisposition::AlreadyTerminal)
            );
            assert_eq!(
                verification(&owner),
                VerificationOutcome::AdoptedSessionOnly
            );
            let replacement = metadata(&owner.inner).unwrap();
            assert_ne!(replacement.id, incumbent.id);
            assert_eq!(replacement.epoch, incumbent.epoch + 1);
        }
        let kept = metadata(&owner.inner).unwrap();
        owner.inner.reaped(work.key);
        assert!(metadata(&owner.inner).as_ref() == Some(&kept));
    }
    let incumbent = metadata(&owner.inner).unwrap();
    let work = reserve(&owner, "primary").await;
    let proof = candidate(&owner, &work, SyntheticLabel::A, FILE, Instant::now(), 60).await;
    let barrier = Barrier::new(3);
    let disposition = thread::scope(|scope| {
        let done = scope.spawn(|| {
            barrier.wait();
            owner
                .inner
                .verification_completed(work.key, proof, Instant::now());
        });
        let cancel = scope.spawn(|| {
            barrier.wait();
            owner.cancel(work.key.id).unwrap()
        });
        barrier.wait();
        done.join().unwrap();
        cancel.join().unwrap()
    });
    match disposition {
        CancelDisposition::Cancelled => {
            assert_eq!(verification(&owner), VerificationOutcome::Cancelled);
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        }
        CancelDisposition::AlreadyTerminal => {
            assert_eq!(
                verification(&owner),
                VerificationOutcome::AdoptedSessionOnly
            );
            assert_eq!(metadata(&owner.inner).unwrap().epoch, incumbent.epoch + 1);
        }
        CancelDisposition::NotCurrent => panic!("current attempt owns this race"),
    }
    owner.inner.reaped(work.key);
    close(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_different_and_expired_identity_require_explicit_disconnect() {
    for mode in 0..3 {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = adopt(&owner, SyntheticLabel::A, FILE, 60).await;
        if mode == 2 {
            // Explicit future-clock reducer check, not a scheduled timer claim.
            expire_connection(&mut raw(&owner.inner), incumbent.deadline);
            assert!(metadata(&owner.inner).is_none());
            assert_eq!(
                owner.connection_snapshot().unwrap().status,
                ConnectionStatus::Expired
            );
        }
        let work = reserve(
            &owner,
            if mode == 1 {
                "other-project"
            } else {
                "primary"
            },
        )
        .await;
        let label = if mode == 1 {
            SyntheticLabel::A
        } else {
            SyntheticLabel::B
        };
        let proof = candidate(&owner, &work, label, FILE, Instant::now(), 60).await;
        let credential_expiry = proof.credential_expires_at();
        let authorization = owner.snapshot().unwrap().last_authorization;
        assert_eq!(
            authorization,
            Some(TerminalRecord {
                attempt_id: work.key.id,
                generation: work.key.generation,
                outcome: AuthorizationOutcome::CompletedPendingVerification,
            })
        );
        let previous_verification = owner.verification_snapshot().unwrap().last_terminal;
        assert_eq!(
            previous_verification.unwrap().outcome,
            VerificationOutcome::AdoptedSessionOnly
        );
        let publication_started = Instant::now();
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now());
        let publication_finished = Instant::now();
        let proposal = owner
            .switch_proposal()
            .unwrap()
            .expect("exact pending switch");
        let auth_pending = owner.snapshot().unwrap();
        assert_eq!(auth_pending.phase, Phase::AwaitingSwitch);
        assert_eq!(auth_pending.attempt_id, Some(work.key.id));
        assert_eq!(auth_pending.generation, work.key.generation);
        assert_eq!(auth_pending.last_authorization, authorization);
        let verification_pending = owner.verification_snapshot().unwrap();
        assert_eq!(
            verification_pending.phase,
            VerificationPhase::AwaitingSwitch
        );
        assert_eq!(verification_pending.attempt_id, Some(work.key.id));
        assert_eq!(verification_pending.generation, work.key.generation);
        assert_eq!(verification_pending.last_terminal, previous_verification);
        assert_eq!(proposal.attempt_id, work.key.id);
        assert_eq!(proposal.generation, work.key.generation);
        assert!(!proposal.decision_id.is_nil());
        assert_eq!(
            proposal.registration_id,
            if mode == 1 {
                "other-project"
            } else {
                "primary"
            }
        );
        assert_eq!(
            proposal.candidate_account.project_number.value(),
            if mode == 1 { 654 } else { 987 }
        );
        assert_eq!(
            proposal.candidate_account.drive_permission_id.as_str(),
            if mode == 1 {
                "c3_independent_A"
            } else {
                "c3_independent_B"
            }
        );
        assert_eq!(proposal.incumbent_account, incumbent.key);
        assert_eq!(proposal.incumbent_account.project_number.value(), 987);
        assert_eq!(
            proposal.incumbent_account.drive_permission_id.as_str(),
            "c3_independent_A"
        );
        assert_ne!(proposal.candidate_account, proposal.incumbent_account);
        assert_eq!(proposal.actual_grants, FILE);
        assert_eq!(proposal.baseline_connection_epoch, incumbent.epoch);
        // This fixture issued a 60s credential BEFORE publication. Its original
        // expiry therefore caps the proposal before publication's own 60s cap.
        assert!(credential_expiry <= publication_started + Duration::from_secs(60));
        assert_eq!(proposal.deadline, work.deadline.min(credential_expiry));
        assert!(proposal.deadline <= publication_finished + Duration::from_secs(60));
        let observed_again = owner.switch_proposal().unwrap().unwrap();
        assert_eq!(observed_again.attempt_id, proposal.attempt_id);
        assert_eq!(observed_again.generation, proposal.generation);
        assert_eq!(observed_again.decision_id, proposal.decision_id);
        assert_eq!(observed_again.candidate_account, proposal.candidate_account);
        assert_eq!(observed_again.incumbent_account, proposal.incumbent_account);
        assert_eq!(observed_again.registration_id, proposal.registration_id);
        assert_eq!(observed_again.actual_grants, proposal.actual_grants);
        assert_eq!(
            observed_again.baseline_connection_epoch,
            proposal.baseline_connection_epoch
        );
        assert_eq!(observed_again.deadline, proposal.deadline);
        {
            let state = raw(&owner.inner);
            let stamp = state.identity.as_ref().unwrap();
            assert_eq!(stamp.key, incumbent.key);
            assert_eq!(stamp.id, incumbent.id);
        }
        if mode != 2 {
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        } else {
            assert!(metadata(&owner.inner).is_none());
            assert_eq!(
                owner.connection_snapshot().unwrap().status,
                ConnectionStatus::Expired
            );
        }
        assert_eq!(raw(&owner.inner).connection_epoch, incumbent.epoch);
        assert_eq!(
            owner.decide_switch(
                proposal.attempt_id,
                proposal.generation,
                proposal.decision_id,
                SwitchDecision::Decline,
            ),
            Ok(SwitchDisposition::Declined)
        );
        let declined = Some(VerificationTerminalRecord {
            attempt_id: work.key.id,
            generation: work.key.generation,
            outcome: VerificationOutcome::Declined,
        });
        assert_eq!(
            owner.verification_snapshot().unwrap().last_terminal,
            declined
        );
        assert!(owner.switch_proposal().unwrap().is_none());
        assert_eq!(owner.snapshot().unwrap().last_authorization, authorization);
        assert_eq!(
            owner.decide_switch(
                proposal.attempt_id,
                proposal.generation,
                proposal.decision_id,
                SwitchDecision::Decline,
            ),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert_eq!(
            owner.verification_snapshot().unwrap().last_terminal,
            declined
        );
        assert_eq!(owner.snapshot().unwrap().last_authorization, authorization);
        {
            let state = raw(&owner.inner);
            let stamp = state.identity.as_ref().unwrap();
            assert_eq!(stamp.key, incumbent.key);
            assert_eq!(stamp.id, incumbent.id);
            assert_eq!(state.connection_epoch, incumbent.epoch);
        }
        if mode != 2 {
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        } else {
            assert!(metadata(&owner.inner).is_none());
            assert_eq!(
                owner.connection_snapshot().unwrap().status,
                ConnectionStatus::Expired
            );
        }
        owner.inner.reaped(work.key);
        let old_epoch = raw(&owner.inner).connection_epoch;
        owner
            .invalidate(if mode == 1 {
                DisconnectOrSwitch::SwitchAccount
            } else {
                DisconnectOrSwitch::Disconnect
            })
            .unwrap();
        {
            let state = raw(&owner.inner);
            assert!(state.identity.is_none());
            assert!(state.connected.is_none());
            assert_eq!(state.connection_epoch, old_epoch + 1);
        }
        let replacement = adopt(&owner, SyntheticLabel::B, FILE, 60).await;
        assert_eq!(
            replacement.key.drive_permission_id.as_str(),
            "c3_independent_B"
        );
        assert_eq!(replacement.epoch, old_epoch + 2);
        close(&owner, Ok(())).await;
    }
}

#[tokio::test]
async fn independent_exact_retention_and_admission_clock_channels() {
    const {
        assert!(CONNECTION_RETENTION.as_secs() == 3600);
    }
    for (seconds, invalid_clock) in [(40, false), (7200, false), (7200, true)] {
        let owner = SessionController::with_catalog(catalog());
        let work = reserve(&owner, "primary").await;
        let issued = Instant::now();
        let proof = candidate(&owner, &work, SyntheticLabel::A, FILE, issued, seconds).await;
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now());
        assert_eq!(
            verification(&owner),
            VerificationOutcome::AdoptedSessionOnly
        );
        let connected = metadata(&owner.inner).unwrap();
        assert_eq!(
            connected.credential_expiry,
            issued + Duration::from_secs(seconds)
        );
        assert_eq!(
            connected.deadline,
            (issued + Duration::from_secs(seconds))
                .min(connected.adopted + Duration::from_secs(3600))
        );
        let history = {
            let state = raw(&owner.inner);
            (state.last, state.last_verification)
        };
        owner.inner.reaped(work.key);
        if seconds == 7200 && !invalid_clock {
            expire_connection(&mut raw(&owner.inner), work.deadline);
            assert!(
                metadata(&owner.inner).as_ref() == Some(&connected),
                "expired authorization admission alone cannot end adopted session"
            );
        }
        let boundary = if invalid_clock {
            connected.adopted - Duration::from_nanos(1)
        } else {
            connected.deadline
        };
        expire_connection(&mut raw(&owner.inner), boundary);
        assert!(metadata(&owner.inner).is_none());
        {
            let state = raw(&owner.inner);
            assert_eq!(state.identity.as_ref().unwrap().id, connected.id);
            assert_eq!((state.last, state.last_verification), history);
        }
        close(&owner, Ok(())).await;
    }
}

#[tokio::test]
async fn independent_scheduled_replacement_expiry_idle_reuse_and_raw_pending() {
    let owner = SessionController::with_catalog(catalog());
    raw(&owner.inner).watcher_test.retention = Some(Duration::from_millis(900));
    let old = adopt(&owner, SyntheticLabel::A, BOTH, 60).await;
    tokio::time::sleep(Duration::from_millis(60)).await; // Let actual watcher observe the first timer.
    raw(&owner.inner).watcher_test.retention = Some(Duration::from_millis(1900));
    let replacement = adopt(&owner, SyntheticLabel::A, FILE, 60).await;
    assert_ne!(old.id, replacement.id);
    let history = {
        let state = raw(&owner.inner);
        (state.last, state.last_verification)
    };
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        old.deadline + Duration::from_millis(100),
    ))
    .await;
    // This raw observation must precede every expiry-running getter/Inner.lock.
    assert!(
        metadata(&owner.inner).as_ref() == Some(&replacement),
        "old deadline cannot erase replacement"
    );
    {
        let state = raw(&owner.inner);
        assert!(state.watcher == WatcherState::Running);
        assert_eq!(state.watcher_test.starts, 1);
    }
    // A real locally polled production raw pending watchdog shares the Slot but
    // cannot verify/adopt/replace the separately owned connection.
    let mut work = owner.inner.reserve(raw_start(), Instant::now()).unwrap();
    owner.inner.offer(work.key, Instant::now()).unwrap();
    owner
        .inner
        .acknowledge(work.key, BrowserHandoff::HandedOff, Instant::now())
        .unwrap();
    assert!(owner.inner.exchanging(work.key, Instant::now()));
    let marker = Pending::Marker {
        key: work.key,
        expires: Instant::now() + Duration::from_secs(10),
        scopes: FILE.to_vec(),
    };
    let mut pending = Box::pin(continue_pending(
        &owner.inner,
        work.key,
        marker,
        &mut work.stop,
    ));
    tokio::select! { biased; _ = &mut pending => panic!("raw watchdog ended before cancellation"),
    _ = tokio::time::sleep_until(tokio::time::Instant::from_std(replacement.deadline + Duration::from_millis(100))) => {} }
    {
        let state = raw(&owner.inner); // MUST be before public/shared expiry reducers.
        assert!(
            state.connected.is_none(),
            "actual scheduled watcher removed credentials"
        );
        assert_eq!(state.identity.as_ref().unwrap().id, replacement.id);
        assert!(state.watcher == WatcherState::Running);
        assert_eq!(state.watcher_test.starts, 1);
        assert_eq!(
            state.slot.as_ref().unwrap().phase,
            Phase::AccountVerificationPending
        );
        assert!(state.slot.as_ref().unwrap().pending.is_some());
        assert_eq!(state.last_verification, history.1);
        assert_eq!(
            state.last.unwrap().outcome,
            AuthorizationOutcome::CompletedPendingVerification
        );
    }
    owner.cancel(work.key.id).unwrap();
    tokio::time::timeout(Duration::from_secs(1), pending)
        .await
        .unwrap()
        .unwrap();
    owner.inner.reaped(work.key); // Synthetic raw reservation; local watchdog ended.
    owner.invalidate(DisconnectOrSwitch::Disconnect).unwrap();
    let fresh = adopt(&owner, SyntheticLabel::B, FILE, 60).await;
    assert_eq!(fresh.key.drive_permission_id.as_str(), "c3_independent_B");
    assert_eq!(
        raw(&owner.inner).watcher_test.starts,
        1,
        "idle watcher is reused"
    );
    close(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_actual_lazy_startup_cancellation_failure_and_unpolled_guard() {
    let owner = SessionController::with_catalog(catalog());
    assert!(raw(&owner.inner).watcher == WatcherState::Dormant);
    assert_eq!(raw(&owner.inner).watcher_test.starts, 0);
    let ticket = tokio::time::timeout(Duration::from_secs(2), owner.begin(raw_start()))
        .await
        .unwrap()
        .unwrap();
    assert!(raw(&owner.inner).watcher == WatcherState::Dormant);
    drop(ticket);
    wait_state(&owner.inner, |s| s.slot.is_none()).await; // Actual supervisor cleanup.
    let (startup, entered, release) = gate();
    raw(&owner.inner).watcher_test.startup = Some(startup);
    let mut beginning = Box::pin(owner.begin_configured(selection("primary")));
    tokio::select! { biased; _ = &mut beginning => panic!("startup gate must precede ticket"), _ = seen(entered) => {} }
    let key = raw(&owner.inner).slot.as_ref().unwrap().key;
    assert_eq!(owner.cancel(key.id), Ok(CancelDisposition::Cancelled));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), beginning)
            .await
            .unwrap()
            .err(),
        Some(StartError::Terminated)
    );
    wait_state(&owner.inner, |s| s.slot.is_none()).await;
    assert!(
        !raw(&owner.inner).closed,
        "ordinary cancellation does not close healthy watcher"
    );
    release.send(()).unwrap();
    wait_state(&owner.inner, |s| s.watcher == WatcherState::Running).await;
    let ticket = tokio::time::timeout(
        Duration::from_secs(2),
        owner.begin_configured(selection("primary")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(raw(&owner.inner).watcher_test.starts, 1);
    drop(ticket);
    wait_state(&owner.inner, |s| s.slot.is_none()).await;
    close(&owner, Ok(())).await;

    let owner = SessionController::with_catalog(catalog());
    let work = owner
        .inner
        .reserve_configured(selection("primary"), Instant::now())
        .unwrap();
    let guard = owner
        .inner
        .prepare_watcher(work.key, work.startup_deadline)
        .unwrap()
        .unwrap();
    let exact_never_polled_future = watcher_task(guard);
    drop(exact_never_polled_future); // Actual armed guard, not fabricated finalization.
    {
        let state = raw(&owner.inner);
        assert!(state.closed);
        assert_eq!(state.fault, Some(ControllerError::WatcherFailure));
        assert!(state.watcher == WatcherState::Stopped);
        assert!(state.connected.is_none());
    }
    owner.inner.reaped(work.key); // No spawned watcher/listener/supervisor.
    close(&owner, Err(ControllerError::WatcherFailure)).await;

    let owner = SessionController::with_catalog(catalog());
    raw(&owner.inner).watcher_test.fail_start = true;
    let mut work = owner
        .inner
        .reserve_configured(selection("primary"), Instant::now())
        .unwrap();
    let startup_failure = ensure_watcher(&owner.inner, &mut work).await;
    assert!(matches!(
        startup_failure,
        Err(AuthorizationOutcome::WorkerFailure | AuthorizationOutcome::Cancelled)
    ));
    // Internal failure closes the owner and signals Slot.stop; the helper can
    // observe that stop before its watcher error. The stored fault is exact.
    wait_state(&owner.inner, |state| state.watcher == WatcherState::Stopped).await;
    {
        let state = raw(&owner.inner);
        assert!(state.closed);
        assert_eq!(state.fault, Some(ControllerError::WatcherFailure));
        assert!(state.connected.is_none());
        assert!(state.identity.is_none());
        assert_eq!(state.last.unwrap().outcome, AuthorizationOutcome::Closed);
        let slot = state.slot.as_ref().unwrap();
        assert!(slot.key == work.key);
        assert_eq!(slot.phase, Phase::Draining);
        assert!(slot.pending.is_none());
        assert_eq!(state.watcher_test.starts, 1);
    }
    owner.inner.reaped(work.key); // Synthetic Slot; actual failed watcher has finalized.
    close(&owner, Err(ControllerError::WatcherFailure)).await;

    let owner = SessionController::with_catalog(catalog());
    let (startup, entered, release) = gate();
    raw(&owner.inner).watcher_test.startup = Some(startup);
    let mut work = owner
        .inner
        .reserve_configured(selection("primary"), Instant::now())
        .unwrap();
    work.startup_deadline = Instant::now() + Duration::from_millis(120);
    let mut starting = Box::pin(ensure_watcher(&owner.inner, &mut work));
    tokio::select! { biased; _ = &mut starting => panic!("held startup gate"), _ = seen(entered) => {} }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), starting)
            .await
            .unwrap(),
        Err(AuthorizationOutcome::TimedOut)
    );
    owner.inner.reaped(work.key); // No listener/supervisor ever started.
    let mut closing = Box::pin(owner.shutdown());
    tokio::select! { biased; _ = &mut closing => panic!("started gated watcher must fence cleanup"), _ = tokio::time::sleep(Duration::from_millis(20)) => {} }
    release.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), closing)
            .await
            .unwrap(),
        Err(ControllerError::WatcherFailure)
    );
}

#[tokio::test]
async fn independent_watcher_panic_and_owner_drop_signal_actual_finalization() {
    let owner = SessionController::with_catalog(catalog());
    adopt(&owner, SyntheticLabel::A, FILE, 60).await;
    let history = {
        let state = raw(&owner.inner);
        (state.last, state.last_verification)
    };
    let (cleanup, entered, release) = gate();
    {
        let mut state = raw(&owner.inner);
        state.watcher_test.cleanup = Some(cleanup);
        state.watcher_test.panic_next = true;
        state.lifecycle_changed.send_replace(());
    }
    seen(entered).await;
    {
        let state = raw(&owner.inner);
        assert!(state.closed);
        assert!(state.connected.is_none());
        assert!(state.identity.is_none());
        assert!(state.watcher == WatcherState::Finishing);
        assert_eq!(state.fault, Some(ControllerError::WatcherFailure));
        assert_eq!((state.last, state.last_verification), history);
    }
    let mut abandoned = Box::pin(owner.shutdown());
    tokio::select! { biased; _ = &mut abandoned => panic!("actual cleanup gate must fence shutdown"), _ = tokio::time::sleep(Duration::from_millis(20)) => {} }
    drop(abandoned);
    release.send(()).unwrap();
    close(&owner, Err(ControllerError::WatcherFailure)).await;

    let owner = SessionController::with_catalog(catalog());
    adopt(&owner, SyntheticLabel::A, FILE, 60).await;
    let inner = Arc::clone(&owner.inner);
    let weak = Arc::downgrade(&inner);
    drop(owner);
    {
        let state = raw(&inner);
        assert!(state.closed);
        assert!(state.connected.is_none());
        assert!(state.identity.is_none());
    }
    wait_state(&inner, |s| s.watcher == WatcherState::Stopped).await;
    drop(inner);
    tokio::time::timeout(Duration::from_secs(1), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual task has no retained owner-reference cycle");
}

#[tokio::test]
async fn independent_actual_watcher_and_ticket_each_fence_dropped_shutdown() {
    for release_ticket_first in [true, false] {
        let owner = SessionController::with_catalog(catalog());
        let (cleanup, entered, release) = gate();
        raw(&owner.inner).watcher_test.cleanup = Some(cleanup);
        let ticket = tokio::time::timeout(
            Duration::from_secs(2),
            owner.begin_configured(selection("primary")),
        )
        .await
        .unwrap()
        .unwrap();
        let mut abandoned = Box::pin(owner.shutdown());
        tokio::select! { biased; _ = &mut abandoned => panic!("both owned resources are held"), _ = seen(entered) => {} }
        drop(abandoned); // Closure signals/resources remain owned without this future.
        if release_ticket_first {
            drop(ticket);
            wait_state(&owner.inner, |s| s.slot.is_none()).await;
            assert!(raw(&owner.inner).watcher == WatcherState::Finishing);
            let mut closing = Box::pin(owner.shutdown());
            tokio::select! { biased; _ = &mut closing => panic!("watcher still independently held"), _ = tokio::time::sleep(Duration::from_millis(20)) => {} }
            release.send(()).unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), closing)
                    .await
                    .unwrap(),
                Ok(())
            );
        } else {
            release.send(()).unwrap();
            wait_state(&owner.inner, |s| s.watcher == WatcherState::Stopped).await;
            assert!(raw(&owner.inner).slot.is_some());
            let mut closing = Box::pin(owner.shutdown());
            tokio::select! { biased; _ = &mut closing => panic!("launcher ticket still independently held"), _ = tokio::time::sleep(Duration::from_millis(20)) => {} }
            drop(ticket);
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), closing)
                    .await
                    .unwrap(),
                Ok(())
            );
        }
        assert!(raw(&owner.inner).slot.is_none());
        assert!(raw(&owner.inner).watcher == WatcherState::Stopped);
    }
}

#[tokio::test]
async fn independent_current_epoch_mapping_faults_and_disconnect_commit_orders() {
    for mode in 0..8 {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = adopt(&owner, SyntheticLabel::A, FILE, 60).await;
        let work = reserve(&owner, "primary").await;
        let proof = candidate(&owner, &work, SyntheticLabel::A, FILE, Instant::now(), 60).await;
        let mut fault_expected = None;
        match mode {
            0 => {
                raw(&owner.inner)
                    .slot
                    .as_mut()
                    .unwrap()
                    .registration
                    .client_id = "changed.apps.googleusercontent.com".into()
            }
            1 => {
                raw(&owner.inner)
                    .slot
                    .as_mut()
                    .unwrap()
                    .expected_connection_epoch += 1
            }
            2 => {
                owner.inner.verification_completed(
                    Key {
                        id: Uuid::nil(),
                        generation: work.key.generation,
                    },
                    proof,
                    Instant::now(),
                );
                assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
                assert_eq!(verification(&owner), VerificationOutcome::Cancelled);
                assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
                owner.inner.reaped(work.key);
                close(&owner, Ok(())).await;
                continue;
            }
            3 => {
                raw(&owner.inner).generation = u64::MAX;
                assert_eq!(
                    owner.cancel(work.key.id),
                    Err(ControllerError::CounterExhausted)
                );
                fault_expected = Some(ControllerError::CounterExhausted);
            }
            4 => {
                let mut state = raw(&owner.inner);
                state.connection_epoch = u64::MAX;
                state.slot.as_mut().unwrap().expected_connection_epoch = u64::MAX;
                fault_expected = Some(ControllerError::CounterExhausted);
            }
            5 => {
                assert!(
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        let _held = owner.inner.state.lock().unwrap();
                        panic!("independent synthetic C3 poison");
                    }))
                    .is_err()
                );
                assert_eq!(owner.snapshot(), Err(ControllerError::Poisoned));
                fault_expected = Some(ControllerError::Poisoned);
            }
            6 => owner.invalidate(DisconnectOrSwitch::Disconnect).unwrap(),
            _ => {}
        }
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now());
        if mode == 7 {
            assert_eq!(
                verification(&owner),
                VerificationOutcome::AdoptedSessionOnly
            );
            owner.invalidate(DisconnectOrSwitch::SwitchAccount).unwrap();
        }
        {
            let state = raw(&owner.inner);
            let outcome = state.last_verification.unwrap().outcome;
            match mode {
                0 | 1 => {
                    assert_eq!(outcome, VerificationOutcome::BindingMismatch);
                    assert!(state.connected.is_some());
                }
                3..=5 => {
                    assert_eq!(state.fault, fault_expected);
                    assert!(state.closed);
                    assert!(state.connected.is_none());
                    assert!(state.identity.is_none());
                    assert_eq!(outcome, VerificationOutcome::Closed);
                }
                6 => {
                    assert_eq!(outcome, VerificationOutcome::Cancelled);
                    assert!(state.connected.is_none());
                }
                7 => {
                    assert_eq!(outcome, VerificationOutcome::AdoptedSessionOnly);
                    assert!(state.connected.is_none());
                }
                _ => unreachable!(),
            }
            assert_eq!(
                state.last.unwrap().outcome,
                AuthorizationOutcome::CompletedPendingVerification
            );
        }
        if mode <= 1 {
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        }
        owner.inner.reaped(work.key); // Only synthetic reservation; watcher joins independently.
        close(&owner, fault_expected.map_or(Ok(()), Err)).await;
    }
}
