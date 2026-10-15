//! Independent C4 fixed cases. Real restricted loopback verification and watcher;
//! synthetic reducer reservations have no browser, worker or supervisor. They are
//! reaped only after their continuation ends. No Code handoff/provider entry.
use super::*;
use crate::drive_auth::account::session_test_support::{FixtureSpec, SyntheticLabel};
use crate::drive_auth::registration::NativeRegistration;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

const BOTH: &[ApprovedScope] = &[ApprovedScope::AppData, ApprovedScope::File];
const FILE: &[ApprovedScope] = &[ApprovedScope::File];
const APP: &[ApprovedScope] = &[ApprovedScope::AppData];

fn catalog() -> NativeRegistrationCatalog {
    NativeRegistrationCatalog::from_native_records(vec![
        NativeRegistration::from_native_configuration(
            "one",
            "c4-independent.apps.googleusercontent.com",
            "101",
            BOTH,
        )
        .unwrap(),
        NativeRegistration::from_native_configuration(
            "two",
            "c4-other.apps.googleusercontent.com",
            "202",
            BOTH,
        )
        .unwrap(),
    ])
    .unwrap()
}
fn select(id: &str) -> RegistrationSelection {
    RegistrationSelection::new(id, BOTH).unwrap()
}
fn raw(inner: &Inner) -> MutexGuard<'_, State> {
    inner.state.lock().unwrap_or_else(|p| p.into_inner())
}
async fn observed(inner: &Inner, predicate: impl Fn(&State) -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if predicate(&raw(inner)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("bounded raw-state observation, no lazy getter");
}
async fn reserved(owner: &SessionController, id: &str) -> Work {
    let mut work = owner
        .inner
        .reserve_configured(select(id), Instant::now())
        .unwrap();
    ensure_watcher(&owner.inner, &mut work)
        .await
        .expect("actual watcher readiness");
    owner.inner.offer(work.key, Instant::now()).unwrap();
    owner
        .inner
        .acknowledge(work.key, BrowserHandoff::HandedOff, Instant::now())
        .unwrap();
    assert!(owner.inner.exchanging(work.key, Instant::now()));
    work
}

// Own one-request bounded nonblocking peer. All loops observe stop/absolute 3s;
// Drop joins even during an assertion unwind. No author Peer/helper is reused.
struct Peer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    request: Arc<Mutex<Option<Vec<u8>>>>,
    worker: Option<JoinHandle<()>>,
}
impl Peer {
    fn new(label: SyntheticLabel) -> Self {
        let body: &[u8] = match label {
            SyntheticLabel::A => br#"{"user":{"permissionId":"independent_c4_A"}}"#,
            SyntheticLabel::B => br#"{"user":{"permissionId":"independent_c4_B"}}"#,
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let request = Arc::new(Mutex::new(None));
        let halt = Arc::clone(&stop);
        let capture = Arc::clone(&request);
        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n", body.len()).into_bytes();
        response.extend_from_slice(body);
        let worker = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(3);
            while !halt.load(Ordering::Acquire) && Instant::now() < end {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(_) => return,
                };
                stream.set_nonblocking(true).unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 1024];
                while !halt.load(Ordering::Acquire) && Instant::now() < end {
                    match stream.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => {
                            if bytes.len() + n > 8192 {
                                return;
                            }
                            bytes.extend_from_slice(&buf[..n]);
                            if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1))
                        }
                        Err(_) => return,
                    }
                }
                if !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                    return;
                }
                *capture.lock().unwrap() = Some(bytes);
                let mut offset = 0;
                while offset < response.len()
                    && !halt.load(Ordering::Acquire)
                    && Instant::now() < end
                {
                    match stream.write(&response[offset..]) {
                        Ok(0) => return,
                        Ok(n) => offset += n,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1))
                        }
                        Err(_) => return,
                    }
                }
                return;
            }
        });
        Self {
            address,
            stop,
            request,
            worker: Some(worker),
        }
    }
    fn assert_request(&self, label: SyntheticLabel) {
        let bytes = self
            .request
            .lock()
            .unwrap()
            .clone()
            .expect("actual HTTP request");
        let wire = String::from_utf8(bytes).unwrap();
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
        let headers = wire
            .lines()
            .filter_map(|v| v.split_once(':'))
            .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.trim())
            .collect::<Vec<_>>();
        assert_eq!(
            headers,
            vec![match label {
                SyntheticLabel::A => "Bearer synthetic-session-A",
                SyntheticLabel::B => "Bearer synthetic-session-B",
            }]
        );
        assert!(wire.ends_with("\r\n\r\n"));
        assert!(!wire.contains("refresh"));
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                assert!(result.is_ok(), "own bounded peer joined");
            }
        }
    }
}
fn input(
    work: &Work,
    peer: &Peer,
    label: SyntheticLabel,
    scopes: &[ApprovedScope],
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
                scopes,
                issued,
                seconds,
            )
            .unwrap(),
        ),
        header: Pending::Marker {
            key: work.key,
            expires: issued + Duration::from_secs(seconds),
            scopes: scopes.to_vec(),
        },
        address: peer.address,
    }
}
async fn sealed(
    owner: &SessionController,
    work: &Work,
    label: SyntheticLabel,
    scopes: &[ApprovedScope],
    issued: Instant,
    seconds: u64,
    start: bool,
) -> VerifiedAccountCandidate {
    let peer = Peer::new(label);
    let value = input(work, &peer, label, scopes, issued, seconds);
    let context = if start {
        let (context, deadline) = owner
            .inner
            .start_verification(work.key, &value, Instant::now())
            .unwrap();
        assert_eq!(deadline, work.deadline);
        context
    } else {
        let state = raw(&owner.inner);
        owner
            .inner
            .configured_context(state.slot.as_ref().unwrap())
            .unwrap()
    };
    let (held, cancelled) = oneshot::channel();
    let proof = tokio::time::timeout(Duration::from_secs(3), value.verify(context, cancelled))
        .await
        .unwrap()
        .expect("actual restricted verifier");
    drop(held);
    peer.assert_request(label);
    assert_eq!(proof.granted_scopes(), scopes);
    assert_eq!(
        proof.credential_expires_at(),
        issued + Duration::from_secs(seconds)
    );
    proof
}
#[derive(PartialEq, Eq)]
struct Metadata {
    key: AccountKey,
    id: Uuid,
    epoch: u64,
    grants: Vec<ApprovedScope>,
    missing: Vec<ApprovedScope>,
    adopted: Instant,
    deadline: Instant,
    expiry: Instant,
}
fn metadata(inner: &Inner) -> Option<Metadata> {
    let state = raw(inner);
    state.connected.as_ref().map(|value| Metadata {
        key: value.candidate.account_key().clone(),
        id: value.timer.id,
        epoch: value.timer.epoch,
        grants: value.candidate.granted_scopes().to_vec(),
        missing: value.candidate.missing_requested_scopes().to_vec(),
        adopted: value.adopted,
        deadline: value.timer.deadline,
        expiry: value.candidate.credential_expires_at(),
    })
}
async fn initial(owner: &SessionController) -> Metadata {
    let work = reserved(owner, "one").await;
    let proof = sealed(
        owner,
        &work,
        SyntheticLabel::A,
        BOTH,
        Instant::now(),
        900,
        true,
    )
    .await;
    assert!(matches!(
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now()),
        VerificationCommit::Finished
    ));
    let value = metadata(&owner.inner).unwrap();
    assert_eq!(value.key.project_number.value(), 101);
    assert_eq!(value.key.drive_permission_id.as_str(), "independent_c4_A");
    assert_eq!(
        terminal(owner).outcome,
        VerificationOutcome::AdoptedSessionOnly
    );
    owner.inner.reaped(work.key); // No listener/supervisor was started by reserved().
    value
}
async fn proposed(
    owner: &SessionController,
    id: &str,
    label: SyntheticLabel,
    scopes: &[ApprovedScope],
) -> (Work, SwitchProposalSnapshot) {
    let work = reserved(owner, id).await;
    let proof = sealed(owner, &work, label, scopes, Instant::now(), 900, true).await;
    assert!(matches!(
        owner
            .inner
            .verification_completed(work.key, proof, Instant::now()),
        VerificationCommit::AwaitingSwitch { .. }
    ));
    let proposal = owner
        .switch_proposal()
        .unwrap()
        .expect("one nonterminal proposal");
    (work, proposal)
}
fn terminal(owner: &SessionController) -> VerificationTerminalRecord {
    owner
        .verification_snapshot()
        .unwrap()
        .last_terminal
        .unwrap()
}
fn decision(
    owner: &SessionController,
    p: &SwitchProposalSnapshot,
    value: SwitchDecision,
) -> Result<SwitchDisposition, ControllerError> {
    owner.decide_switch(p.attempt_id, p.generation, p.decision_id, value)
}
fn same_proposal(a: &SwitchProposalSnapshot, b: &SwitchProposalSnapshot) {
    assert_eq!(a.attempt_id, b.attempt_id);
    assert_eq!(a.generation, b.generation);
    assert_eq!(a.decision_id, b.decision_id);
    assert_eq!(a.candidate_account, b.candidate_account);
    assert_eq!(a.incumbent_account, b.incumbent_account);
    assert_eq!(a.registration_id, b.registration_id);
    assert_eq!(a.actual_grants, b.actual_grants);
    assert_eq!(a.baseline_connection_epoch, b.baseline_connection_epoch);
    assert_eq!(a.deadline, b.deadline);
}
async fn cleanup(owner: &SessionController, expected: Result<(), ControllerError>) {
    owner.inner.close();
    let key = {
        let state = raw(&owner.inner);
        state.slot.as_ref().map(|v| {
            assert_eq!(v.phase, Phase::Draining);
            v.key
        })
    };
    if let Some(key) = key {
        owner.inner.reaped(key);
    } // Synthetic reservations only; any directly driven continuation was joined first.
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
    assert!(state.identity.is_none());
}

#[tokio::test]
async fn independent_exact_proposal_partial_accept_and_late_sealed_completion() {
    let owner = SessionController::with_catalog(catalog());
    let incumbent = initial(&owner).await;
    let previous = terminal(&owner);
    let (work, p) = proposed(&owner, "two", SyntheticLabel::B, FILE).await;
    let authorization = owner.snapshot().unwrap().last_authorization;
    assert_eq!(
        authorization,
        Some(TerminalRecord {
            attempt_id: work.key.id,
            generation: work.key.generation,
            outcome: AuthorizationOutcome::CompletedPendingVerification
        })
    );
    assert_eq!(owner.snapshot().unwrap().phase, Phase::AwaitingSwitch);
    assert_eq!(
        owner.verification_snapshot().unwrap().phase,
        VerificationPhase::AwaitingSwitch
    );
    assert_eq!(terminal(&owner), previous);
    assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
    assert_eq!(p.attempt_id, work.key.id);
    assert_eq!(p.generation, work.key.generation);
    assert!(!p.decision_id.is_nil());
    assert_eq!(p.registration_id, "two");
    assert_eq!(p.candidate_account.project_number.value(), 202);
    assert_eq!(
        p.candidate_account.drive_permission_id.as_str(),
        "independent_c4_B"
    );
    assert_eq!(p.incumbent_account, incumbent.key);
    assert_eq!(p.actual_grants, FILE);
    assert_eq!(p.baseline_connection_epoch, incumbent.epoch);
    assert_eq!(
        owner
            .inner
            .reserve_configured(select("one"), Instant::now())
            .err(),
        Some(StartError::Busy)
    );
    for (id, generation, decision_id) in [
        (Uuid::nil(), p.generation, p.decision_id),
        (p.attempt_id, p.generation + 1, p.decision_id),
        (p.attempt_id, p.generation, Uuid::nil()),
    ] {
        assert_eq!(
            owner.decide_switch(id, generation, decision_id, SwitchDecision::Accept),
            Ok(SwitchDisposition::Stale)
        );
        same_proposal(&p, &owner.switch_proposal().unwrap().unwrap());
        assert_eq!(terminal(&owner), previous);
    }
    // A second independently HTTP-sealed candidate and a late error cannot steal
    // the already-published proposal, its decision ID, or either terminal channel.
    let late = sealed(
        &owner,
        &work,
        SyntheticLabel::A,
        APP,
        Instant::now(),
        900,
        false,
    )
    .await;
    assert!(matches!(
        owner
            .inner
            .verification_completed(work.key, late, Instant::now()),
        VerificationCommit::Finished
    ));
    owner.inner.verification_failed(
        work.key,
        VerificationOutcome::Failed(AccountError::MalformedResponse),
        Instant::now(),
    );
    same_proposal(&p, &owner.switch_proposal().unwrap().unwrap());
    assert_eq!(terminal(&owner), previous);
    let before = Instant::now();
    assert_eq!(
        decision(&owner, &p, SwitchDecision::Accept),
        Ok(SwitchDisposition::AdoptedSessionOnly)
    );
    let after = Instant::now();
    let adopted = metadata(&owner.inner).unwrap();
    assert_eq!(adopted.key, p.candidate_account);
    assert_ne!(adopted.id, incumbent.id);
    assert_eq!(adopted.epoch, incumbent.epoch + 1);
    assert_eq!(adopted.grants, FILE);
    assert_eq!(adopted.missing, APP);
    assert!(adopted.adopted >= before && adopted.adopted <= after);
    assert_eq!(adopted.deadline, adopted.expiry);
    assert!(adopted.deadline <= before + Duration::from_secs(3600));
    assert_eq!(
        terminal(&owner),
        VerificationTerminalRecord {
            attempt_id: work.key.id,
            generation: work.key.generation,
            outcome: VerificationOutcome::AdoptedSessionOnly
        }
    );
    assert_eq!(owner.snapshot().unwrap().last_authorization, authorization);
    assert!(owner.switch_proposal().unwrap().is_none());
    assert_eq!(
        decision(&owner, &p, SwitchDecision::Decline),
        Ok(SwitchDisposition::AlreadyResolved)
    );
    let final_record = terminal(&owner);
    owner.inner.reaped(work.key);
    assert_eq!(
        decision(&owner, &p, SwitchDecision::Accept),
        Ok(SwitchDisposition::Stale)
    );
    assert_eq!(terminal(&owner), final_record);
    cleanup(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_decline_cancel_markers_and_new_attempt_custody() {
    for cancel in [false, true] {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = initial(&owner).await;
        let (work, p) = proposed(&owner, "one", SyntheticLabel::B, APP).await;
        let authorization = owner.snapshot().unwrap().last_authorization;
        let expected = if cancel {
            assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
            VerificationOutcome::Cancelled
        } else {
            assert_eq!(
                decision(&owner, &p, SwitchDecision::Decline),
                Ok(SwitchDisposition::Declined)
            );
            VerificationOutcome::Declined
        };
        let record = terminal(&owner);
        assert_eq!(record.outcome, expected);
        assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        assert!(owner.switch_proposal().unwrap().is_none());
        assert_eq!(
            decision(&owner, &p, SwitchDecision::Accept),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert_eq!(terminal(&owner), record);
        assert_eq!(owner.snapshot().unwrap().last_authorization, authorization);
        assert_eq!(
            owner
                .inner
                .reserve_configured(select("one"), Instant::now())
                .err(),
            Some(StartError::Busy)
        );
        owner.inner.reaped(work.key);
        let fresh = reserved(&owner, "one").await;
        let newer = sealed(
            &owner,
            &fresh,
            SyntheticLabel::A,
            FILE,
            Instant::now(),
            900,
            true,
        )
        .await;
        let newer_auth = owner.snapshot().unwrap().last_authorization;
        assert_eq!(
            decision(&owner, &p, SwitchDecision::Accept),
            Ok(SwitchDisposition::Stale)
        );
        owner.inner.verification_failed(
            work.key,
            VerificationOutcome::WorkerFailure,
            Instant::now(),
        );
        assert_eq!(terminal(&owner), record);
        assert_eq!(owner.snapshot().unwrap().last_authorization, newer_auth);
        assert!(matches!(
            owner
                .inner
                .verification_completed(fresh.key, newer, Instant::now()),
            VerificationCommit::Finished
        ));
        assert_eq!(terminal(&owner).attempt_id, fresh.key.id);
        assert_eq!(
            terminal(&owner).outcome,
            VerificationOutcome::AdoptedSessionOnly
        );
        assert_eq!(metadata(&owner.inner).unwrap().key, incumbent.key);
        cleanup(&owner, Ok(())).await;
    }
}

#[tokio::test]
async fn independent_original_three_caps_equality_and_backwards_clock() {
    const {
        assert!(SWITCH_CONFIRMATION.as_secs() == 60);
        assert!(ATTEMPT_LIFETIME.as_secs() == 300);
        assert!(CONNECTION_RETENTION.as_secs() == 3600);
    }
    // Direct future clocks, not timers: keep the original 300s admission intact.
    for cap in 0..3 {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = initial(&owner).await;
        let work = reserved(&owner, "one").await;
        let issued = Instant::now();
        let seconds = if cap == 1 { 40 } else { 900 };
        let proof = sealed(
            &owner,
            &work,
            SyntheticLabel::B,
            FILE,
            issued,
            seconds,
            true,
        )
        .await;
        let proposed_at = if cap == 0 {
            work.deadline - Duration::from_secs(10)
        } else {
            Instant::now() + Duration::from_secs(2)
        };
        let expected = match cap {
            0 => work.deadline,
            1 => issued + Duration::from_secs(40),
            _ => proposed_at + Duration::from_secs(60),
        };
        let commit = owner
            .inner
            .verification_completed(work.key, proof, proposed_at);
        assert!(
            Instant::now() < proposed_at,
            "future clock remains deterministic"
        );
        let decision_id = match commit {
            VerificationCommit::AwaitingSwitch {
                decision_id,
                deadline,
            } => {
                assert_eq!(deadline, expected);
                decision_id
            }
            VerificationCommit::Finished => panic!("expected future-clock proposal"),
        };
        {
            let state = raw(&owner.inner);
            let slot = state.slot.as_ref().unwrap();
            let pending = slot.switch.as_ref().unwrap().proposal.as_ref().unwrap();
            assert_eq!(pending.proposed, proposed_at);
            assert_eq!(pending.deadline, expected);
            assert!(
                owner
                    .inner
                    .switch_binding(
                        &state,
                        work.key,
                        decision_id,
                        expected - Duration::from_nanos(1)
                    )
                    .is_ok()
            );
        }
        assert_eq!(
            owner.inner.decide_switch_at(
                work.key,
                decision_id,
                SwitchDecision::Accept,
                Uuid::new_v4(),
                expected
            ),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert_eq!(terminal(&owner).outcome, VerificationOutcome::TimedOut);
        assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        let record = terminal(&owner);
        assert_eq!(
            owner.inner.decide_switch_at(
                work.key,
                decision_id,
                SwitchDecision::Accept,
                Uuid::new_v4(),
                expected + Duration::from_nanos(1)
            ),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert_eq!(terminal(&owner), record);
        cleanup(&owner, Ok(())).await;
    }
    let owner = SessionController::with_catalog(catalog());
    let incumbent = initial(&owner).await;
    let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
    same_proposal(&p, &owner.switch_proposal().unwrap().unwrap());
    {
        let mut state = raw(&owner.inner);
        // Deliberately invalid retained proposal clock; no future proof shortcut.
        state
            .slot
            .as_mut()
            .unwrap()
            .switch
            .as_mut()
            .unwrap()
            .proposal
            .as_mut()
            .unwrap()
            .proposed = Instant::now() + Duration::from_secs(1);
    }
    assert!(owner.switch_proposal().unwrap().is_none());
    assert_eq!(terminal(&owner).outcome, VerificationOutcome::TimedOut);
    assert_eq!(
        decision(&owner, &p, SwitchDecision::Accept),
        Ok(SwitchDisposition::AlreadyResolved)
    );
    assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
    assert_eq!(raw(&owner.inner).slot.as_ref().unwrap().key.id, work.key.id);
    cleanup(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_binding_corruption_never_retargets_consent() {
    for corrupt in 0..8 {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = initial(&owner).await;
        let (_work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
        {
            let mut state = raw(&owner.inner);
            match corrupt {
                0 => state.generation += 1,
                1 => state.connection_epoch += 1,
                2 => state.slot.as_mut().unwrap().registration.project_number = "202".into(),
                3 => {
                    state.slot.as_mut().unwrap().registration.client_id =
                        "c4-other.apps.googleusercontent.com".into()
                }
                4 => state.slot.as_mut().unwrap().registration.registration_id = "two".into(),
                5 => state.slot.as_mut().unwrap().configured_selection = Some(select("two")),
                6 => state.slot.as_mut().unwrap().registration.scopes = FILE.to_vec(),
                _ => state.identity.as_mut().unwrap().key = p.candidate_account.clone(),
            }
        }
        let auth = owner.snapshot().unwrap().last_authorization;
        assert_eq!(
            decision(&owner, &p, SwitchDecision::Accept),
            Ok(SwitchDisposition::Stale)
        );
        assert_eq!(
            terminal(&owner).outcome,
            VerificationOutcome::BindingMismatch
        );
        assert_eq!(
            decision(&owner, &p, SwitchDecision::Accept),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        assert_eq!(owner.snapshot().unwrap().last_authorization, auth);
        cleanup(&owner, Ok(())).await;
    }
}

#[tokio::test]
async fn independent_continuation_immediate_decision_and_actual_pending_timer() {
    for expire in [false, true] {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = initial(&owner).await;
        if expire {
            raw(&owner.inner).watcher_test.confirmation = Some(Duration::from_millis(100));
        }
        let work = reserved(&owner, "one").await;
        let peer = Peer::new(SyntheticLabel::B);
        let value = input(&work, &peer, SyntheticLabel::B, FILE, Instant::now(), 900);
        let inner = Arc::clone(&owner.inner);
        let key = work.key;
        let mut stop = work.stop.clone();
        let task =
            tokio::spawn(async move { continue_configured(&inner, key, value, &mut stop).await });
        observed(&owner.inner, |state| {
            state
                .slot
                .as_ref()
                .is_some_and(|v| v.phase == Phase::AwaitingSwitch)
        })
        .await;
        // Observation above reads RAW State before any expiry-performing getter.
        assert!(!task.is_finished());
        peer.assert_request(SyntheticLabel::B);
        assert_eq!(
            owner
                .inner
                .reserve_configured(select("one"), Instant::now())
                .err(),
            Some(StartError::Busy)
        );
        if expire {
            observed(&owner.inner, |state| {
                state.slot.as_ref().is_some_and(|v| {
                    v.phase == Phase::Draining
                        && v.switch.as_ref().is_some_and(|r| r.proposal.is_none())
                })
            })
            .await;
            assert_eq!(
                raw(&owner.inner).last_verification.unwrap().outcome,
                VerificationOutcome::TimedOut
            );
        } else {
            let p = owner.switch_proposal().unwrap().unwrap();
            // Decide immediately after publication notification, including before
            // the continuation gets its next poll: no lost decision wakeup.
            assert_eq!(
                decision(&owner, &p, SwitchDecision::Decline),
                Ok(SwitchDisposition::Declined)
            );
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap(),
            Ok(())
        );
        assert_eq!(
            terminal(&owner).outcome,
            if expire {
                VerificationOutcome::TimedOut
            } else {
                VerificationOutcome::Declined
            }
        );
        assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        assert_eq!(raw(&owner.inner).watcher_test.starts, 1);
        assert_eq!(
            owner
                .inner
                .reserve_configured(select("one"), Instant::now())
                .err(),
            Some(StartError::Busy)
        );
        // The driven continuation is actually joined, but no full positive OAuth
        // supervisor was created; release only this synthetic reservation.
        owner.inner.reaped(work.key);
        cleanup(&owner, Ok(())).await;
    }
}

#[tokio::test]
async fn independent_accepted_earlier_expiry_reschedules_actual_watcher() {
    let owner = SessionController::with_catalog(catalog());
    let incumbent = initial(&owner).await;
    raw(&owner.inner).watcher_test.retention = Some(Duration::from_millis(100));
    let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
    assert_eq!(
        decision(&owner, &p, SwitchDecision::Accept),
        Ok(SwitchDisposition::AdoptedSessionOnly)
    );
    let replacement = metadata(&owner.inner).unwrap();
    assert!(replacement.deadline < incumbent.deadline);
    assert!(replacement.deadline < replacement.expiry);
    assert_eq!(
        replacement.deadline,
        replacement.adopted + Duration::from_millis(100)
    );
    observed(&owner.inner, |state| state.connected.is_none()).await;
    // Raw expiry precedes any explicit expiry reducer or cleanup notification.
    // The actual watcher runs; this fixture does not instrument prior timer
    // capture or force a particular executor interleaving.
    {
        let state = raw(&owner.inner);
        assert!(Instant::now() >= replacement.deadline);
        assert_eq!(state.identity.as_ref().unwrap().key, replacement.key);
        assert_eq!(state.identity.as_ref().unwrap().id, replacement.id);
        assert_eq!(state.connection_epoch, replacement.epoch);
        assert_eq!(state.watcher_test.starts, 1);
        let slot = state
            .slot
            .as_ref()
            .expect("original reservation still held");
        assert!(slot.key == work.key);
        assert_eq!(slot.phase, Phase::Draining);
        assert!(slot.switch.as_ref().unwrap().proposal.is_none());
    }
    owner.inner.reaped(work.key);
    {
        let state = raw(&owner.inner);
        assert!(state.slot.is_none());
    }
    assert_eq!(
        owner.connection_snapshot().unwrap().status,
        ConnectionStatus::Expired
    );
    cleanup(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_serialized_consent_cancel_disconnect_and_real_barrier_race() {
    for accept_first in [false, true] {
        let owner = SessionController::with_catalog(catalog());
        let incumbent = initial(&owner).await;
        let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
        if accept_first {
            assert_eq!(
                decision(&owner, &p, SwitchDecision::Accept),
                Ok(SwitchDisposition::AdoptedSessionOnly)
            );
            assert_eq!(
                owner.cancel(work.key.id),
                Ok(CancelDisposition::AlreadyTerminal)
            );
            assert_eq!(metadata(&owner.inner).unwrap().key, p.candidate_account);
            assert_eq!(
                terminal(&owner).outcome,
                VerificationOutcome::AdoptedSessionOnly
            );
        } else {
            assert_eq!(owner.cancel(work.key.id), Ok(CancelDisposition::Cancelled));
            assert_eq!(
                decision(&owner, &p, SwitchDecision::Accept),
                Ok(SwitchDisposition::AlreadyResolved)
            );
            assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
            assert_eq!(terminal(&owner).outcome, VerificationOutcome::Cancelled);
        }
        cleanup(&owner, Ok(())).await;
    }
    for accept_first in [false, true] {
        let owner = SessionController::with_catalog(catalog());
        initial(&owner).await;
        let (_work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
        if accept_first {
            assert_eq!(
                decision(&owner, &p, SwitchDecision::Accept),
                Ok(SwitchDisposition::AdoptedSessionOnly)
            );
        }
        owner.invalidate(DisconnectOrSwitch::SwitchAccount).unwrap();
        assert_eq!(
            decision(&owner, &p, SwitchDecision::Accept),
            Ok(SwitchDisposition::AlreadyResolved)
        );
        assert!(metadata(&owner.inner).is_none());
        assert!(raw(&owner.inner).identity.is_none());
        assert_eq!(
            terminal(&owner).outcome,
            if accept_first {
                VerificationOutcome::AdoptedSessionOnly
            } else {
                VerificationOutcome::Cancelled
            }
        );
        cleanup(&owner, Ok(())).await;
    }
    let owner = SessionController::with_catalog(catalog());
    let incumbent = initial(&owner).await;
    let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
    let barrier = Barrier::new(3);
    let (accept, cancel) = thread::scope(|scope| {
        let accept = scope.spawn(|| {
            barrier.wait();
            decision(&owner, &p, SwitchDecision::Accept)
        });
        let cancel = scope.spawn(|| {
            barrier.wait();
            owner.cancel(work.key.id)
        });
        barrier.wait();
        (accept.join().unwrap(), cancel.join().unwrap())
    });
    assert!(matches!(
        (accept, cancel),
        (
            Ok(SwitchDisposition::AdoptedSessionOnly),
            Ok(CancelDisposition::AlreadyTerminal)
        ) | (
            Ok(SwitchDisposition::AlreadyResolved),
            Ok(CancelDisposition::Cancelled)
        )
    ));
    if accept == Ok(SwitchDisposition::AdoptedSessionOnly) {
        assert_eq!(metadata(&owner.inner).unwrap().key, p.candidate_account);
        assert_eq!(
            terminal(&owner).outcome,
            VerificationOutcome::AdoptedSessionOnly
        );
    } else {
        assert!(metadata(&owner.inner).as_ref() == Some(&incumbent));
        assert_eq!(terminal(&owner).outcome, VerificationOutcome::Cancelled);
    }
    cleanup(&owner, Ok(())).await;
}

#[tokio::test]
async fn independent_expired_stamp_and_drop_orders_hold_actual_cleanup_fence() {
    for expired in [false, true] {
        for accept_first in [false, true] {
            let owner = SessionController::with_catalog(catalog());
            let incumbent = initial(&owner).await;
            if expired {
                expire_connection(&mut raw(&owner.inner), incumbent.deadline);
            }
            let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
            assert_eq!(p.incumbent_account, incumbent.key);
            assert_eq!(p.baseline_connection_epoch, incumbent.epoch);
            if expired {
                assert!(metadata(&owner.inner).is_none());
                assert_eq!(
                    owner.connection_snapshot().unwrap().status,
                    ConnectionStatus::Expired
                );
            }
            let (entered, seen) = oneshot::channel();
            let (release, held) = oneshot::channel();
            raw(&owner.inner).watcher_test.cleanup = Some(WatcherTestGate {
                entered,
                release: held,
            });
            if accept_first {
                assert_eq!(
                    decision(&owner, &p, SwitchDecision::Accept),
                    Ok(SwitchDisposition::AdoptedSessionOnly)
                );
            }
            let authorization = owner.snapshot().unwrap().last_authorization;
            let inner = Arc::clone(&owner.inner);
            let weak = Arc::downgrade(&inner);
            drop(owner);
            tokio::time::timeout(Duration::from_secs(1), seen)
                .await
                .unwrap()
                .unwrap();
            {
                let state = raw(&inner);
                assert!(state.closed);
                assert!(state.connected.is_none());
                assert!(state.identity.is_none());
                assert!(state.watcher == WatcherState::Finishing);
                assert!(!watcher_finished(&state));
                let slot = state.slot.as_ref().unwrap();
                assert_eq!(slot.phase, Phase::Draining);
                assert!(slot.pending.is_none());
                assert!(slot.switch.as_ref().unwrap().proposal.is_none());
                assert_eq!(state.last, authorization);
                assert_eq!(
                    state.last_verification.unwrap().outcome,
                    if accept_first {
                        VerificationOutcome::AdoptedSessionOnly
                    } else {
                        VerificationOutcome::Closed
                    }
                );
            }
            assert_eq!(
                inner
                    .reserve_configured(select("one"), Instant::now())
                    .err(),
                Some(StartError::Controller(ControllerError::Closed))
            );
            assert_eq!(
                inner.decide_switch_at(
                    work.key,
                    p.decision_id,
                    SwitchDecision::Accept,
                    Uuid::new_v4(),
                    Instant::now()
                ),
                Err(ControllerError::Closed)
            );
            inner.reaped(work.key); // Only the synthetic slot; watcher still finishing.
            assert!(!watcher_finished(&raw(&inner)));
            release.send(()).unwrap();
            observed(&inner, watcher_finished).await;
            {
                let state = raw(&inner);
                assert!(state.slot.is_none());
                assert!(state.fault.is_none());
            }
            drop(inner);
            tokio::time::timeout(Duration::from_secs(2), async {
                while weak.upgrade().is_some() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("actual finalized watcher releases its last reference");
        }
    }
}

#[tokio::test]
async fn independent_overflow_poison_and_actual_watcher_fault_clear_both_owners() {
    for kind in 0..4 {
        let owner = SessionController::with_catalog(catalog());
        initial(&owner).await;
        let (work, p) = proposed(&owner, "one", SyntheticLabel::B, FILE).await;
        let authorization = owner.snapshot().unwrap().last_authorization;
        let expected = match kind {
            0 => {
                {
                    let mut state = raw(&owner.inner);
                    state.connection_epoch = u64::MAX;
                    let slot = state.slot.as_mut().unwrap();
                    slot.expected_connection_epoch = u64::MAX;
                    slot.switch
                        .as_mut()
                        .unwrap()
                        .proposal
                        .as_mut()
                        .unwrap()
                        .epoch = u64::MAX;
                }
                assert_eq!(
                    decision(&owner, &p, SwitchDecision::Accept),
                    Err(ControllerError::CounterExhausted)
                );
                ControllerError::CounterExhausted
            }
            1 => {
                raw(&owner.inner).generation = u64::MAX;
                assert_eq!(
                    owner.cancel(work.key.id),
                    Err(ControllerError::CounterExhausted)
                );
                ControllerError::CounterExhausted
            }
            2 => {
                assert!(
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        let _guard = owner.inner.state.lock().unwrap();
                        panic!("synthetic C4 mutex poison");
                    }))
                    .is_err()
                );
                assert_eq!(
                    owner.switch_proposal().err(),
                    Some(ControllerError::Poisoned)
                );
                ControllerError::Poisoned
            }
            _ => {
                raw(&owner.inner).watcher_test.panic_next = true;
                owner.inner.changed.send_replace(());
                observed(&owner.inner, |state| {
                    state.fault == Some(ControllerError::WatcherFailure)
                })
                .await;
                assert_eq!(
                    decision(&owner, &p, SwitchDecision::Accept),
                    Err(ControllerError::WatcherFailure)
                );
                ControllerError::WatcherFailure
            }
        };
        {
            let state = raw(&owner.inner);
            assert!(state.closed);
            assert_eq!(state.fault, Some(expected));
            assert!(state.connected.is_none());
            assert!(state.identity.is_none());
            let slot = state.slot.as_ref().unwrap();
            assert!(slot.pending.is_none());
            assert!(slot.switch.as_ref().unwrap().proposal.is_none());
            assert_eq!(state.last, authorization);
            assert_eq!(
                state.last_verification.unwrap().outcome,
                VerificationOutcome::Closed
            );
            if kind == 0 {
                assert_eq!(state.connection_epoch, u64::MAX);
            }
            if kind == 1 {
                assert_eq!(state.generation, u64::MAX);
            }
        }
        cleanup(&owner, Err(expected)).await;
    }
}
