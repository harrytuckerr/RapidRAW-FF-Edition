//! Independent fixed-outcome session tests. Only synthetic localhost traffic.
//! A successful code callback is NEVER positively acknowledged. Marker cases
//! call the real commit reducer without constructing tokens or account proof.

use super::*;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::Barrier;

fn native_request() -> NativeStart {
    NativeStart::new(
        "independent-session.apps.googleusercontent.com".to_owned(),
        "independent-native-registration".to_owned(),
        "784123".to_owned(),
        vec![ApprovedScope::File],
    )
    .expect("fixed native configuration")
}

async fn launch_ticket(owner: &SessionController) -> BrowserLaunchTicket {
    tokio::time::timeout(Duration::from_secs(3), owner.begin(native_request()))
        .await
        .expect("bounded native startup")
        .expect("native ticket")
}

async fn wait_vacant(inner: &Arc<Inner>) {
    let mut changed = inner.changed.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let empty = inner
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .slot
                .is_none();
            if empty {
                break;
            }
            changed.changed().await.expect("owned notification sender");
        }
    })
    .await
    .expect("bounded cooperative reaping");
}

#[derive(Clone)]
struct LocalCallback {
    address: SocketAddr,
    path: String,
    state: String,
}
impl LocalCallback {
    fn from_ticket(ticket: &BrowserLaunchTicket) -> Self {
        let url = reqwest::Url::parse(ticket.url_for_native_launcher().expect("live ticket"))
            .expect("native URL");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        let redirect = reqwest::Url::parse(pairs.get("redirect_uri").expect("native redirect"))
            .expect("redirect URL");
        assert_eq!(redirect.scheme(), "http");
        assert_eq!(redirect.host_str(), Some("127.0.0.1"));
        Self {
            address: ([127, 0, 0, 1], redirect.port().expect("ephemeral port")).into(),
            path: redirect.path().to_owned(),
            state: pairs.get("state").expect("native state").clone(),
        }
    }

    // Each actual blocking client has connect/read/write deadlines and an 8KiB
    // read bound; its handle is joined. No author fixture/server is reused.
    async fn send(self, state: String, response_parameter: &'static str) -> bool {
        let client = tokio::task::spawn_blocking(move || {
            let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(1))
                .expect("local callback connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .expect("read deadline");
            stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .expect("write deadline");
            let request = format!(
                "GET {}?state={}&{}&provider_extension=independent HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
                self.path,
                state,
                response_parameter,
                self.address.port(),
            );
            stream
                .write_all(request.as_bytes())
                .expect("callback write");
            let mut response = Vec::new();
            (&mut stream)
                .take(8192)
                .read_to_end(&mut response)
                .expect("bounded callback response");
            let _ = stream.shutdown(Shutdown::Both);
            response.starts_with(b"HTTP/1.1 200")
        });
        tokio::time::timeout(Duration::from_secs(3), client)
            .await
            .expect("bounded client join")
            .expect("client worker")
    }
}

fn reducer_fixture() -> (SessionController, Work) {
    let owner = SessionController::new();
    let work = owner
        .inner
        .reserve(native_request(), Instant::now())
        .expect("actual reservation reducer");
    (owner, work)
}
fn enter_exchange(owner: &SessionController, key: Key, now: Instant) {
    owner.inner.offer(key, now).expect("offer reducer");
    owner
        .inner
        .acknowledge(key, BrowserHandoff::HandedOff, now)
        .expect("ack reducer only: no worker or callback exists");
    assert!(owner.inner.exchanging(key, now));
}
fn pending_marker(key: Key, expires: Instant, scopes: Vec<ApprovedScope>) -> Pending {
    Pending::Marker {
        key,
        expires,
        scopes,
    }
}
fn invalidate_owner(owner: &SessionController, key: Key, choice: usize) {
    match choice {
        0 => assert_eq!(owner.cancel(key.id), Ok(CancelDisposition::Cancelled)),
        1 => owner
            .invalidate(DisconnectOrSwitch::Disconnect)
            .expect("disconnect"),
        2 => owner
            .invalidate(DisconnectOrSwitch::SwitchAccount)
            .expect("switch"),
        _ => panic!("fixed invalidation case"),
    }
}

#[tokio::test]
async fn independent_public_competing_begin_and_replacement_preserve_incumbent() {
    let owner = SessionController::new();
    let (left, right) = tokio::join!(owner.begin(native_request()), owner.begin(native_request()));
    let launch = match (left, right) {
        (Ok(launch), Err(StartError::Busy)) | (Err(StartError::Busy), Ok(launch)) => launch,
        _ => panic!("exactly one public begin must win"),
    };
    let before = owner.snapshot().expect("snapshot");
    assert_eq!(before.phase, Phase::AwaitingBrowser);
    assert!(matches!(
        owner.begin(native_request()).await,
        Err(StartError::Busy)
    ));
    assert_eq!(owner.cancel(Uuid::nil()), Ok(CancelDisposition::NotCurrent));
    assert_eq!(owner.snapshot().expect("snapshot"), before);
    let old_key = launch.key;
    drop(launch);
    wait_vacant(&owner.inner).await;
    assert_eq!(
        owner
            .snapshot()
            .expect("snapshot")
            .last_authorization
            .expect("terminal")
            .outcome,
        AuthorizationOutcome::BrowserUnavailable
    );
    let replacement = launch_ticket(&owner).await;
    let replacement_snapshot = owner.snapshot().expect("snapshot");
    owner.inner.finish(old_key, AuthorizationOutcome::Denied);
    owner.inner.cancel_key(old_key);
    assert_eq!(owner.cancel(old_key.id), Ok(CancelDisposition::NotCurrent));
    assert_eq!(owner.snapshot().expect("snapshot"), replacement_snapshot);
    drop(replacement);
    wait_vacant(&owner.inner).await;
    owner.shutdown().await.expect("closed empty owner");
}

#[tokio::test]
async fn independent_dropped_public_begin_fences_real_startup_and_owner_drop() {
    let owner = SessionController::new();
    let (entered, seen) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let mut request = native_request();
    request.startup_gate = Some(TestStartupGate {
        entered,
        release: blocked,
    });
    let mut begin = Box::pin(owner.begin(request));
    tokio::select! {
        biased;
        _ = &mut begin => panic!("no ticket while real worker is gated"),
        result = seen => result.expect("actual blocking worker entered"),
        _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("bounded gate entry"),
    }
    let admitted = owner.snapshot().expect("starting snapshot");
    drop(begin);
    let cancelled = owner.snapshot().expect("cancelled reservation");
    assert_eq!(cancelled.phase, Phase::Draining);
    assert!(cancelled.generation > admitted.generation);
    assert_eq!(
        cancelled.last_authorization.expect("terminal").outcome,
        AuthorizationOutcome::Cancelled
    );
    assert!(matches!(
        owner.begin(native_request()).await,
        Err(StartError::Busy)
    ));
    release.send(()).expect("release owned worker");
    wait_vacant(&owner.inner).await;
    assert_eq!(
        owner.snapshot().expect("after reaping").last_authorization,
        cancelled.last_authorization
    );
    let launch = launch_ticket(&owner).await;
    let inner = Arc::clone(&owner.inner);
    drop(owner);
    assert!(inner.state.lock().expect("state").closed);
    assert_eq!(launch.url_for_native_launcher(), Err(TicketError::Stale));
    drop(launch);
    wait_vacant(&inner).await;
    assert!(matches!(
        inner.reserve(native_request(), Instant::now()),
        Err(StartError::Controller(ControllerError::Closed))
    ));
}

#[tokio::test]
async fn independent_callback_before_failed_ack_and_acknowledged_errors_are_terminal_once() {
    let owner = SessionController::new();
    let code_ticket = launch_ticket(&owner).await;
    let old_key = code_ticket.key;
    let endpoint = LocalCallback::from_ticket(&code_ticket);
    assert!(
        endpoint
            .clone()
            .send(endpoint.state.clone(), "code=independent-synthetic-code")
            .await
    );
    let held = owner.snapshot().expect("callback held before handoff");
    assert_eq!(held.phase, Phase::AwaitingBrowser);
    assert!(held.last_authorization.is_none());
    assert!(held.granted_subset_pending_verification.is_empty());
    // SECURITY BOUNDARY: this Code outcome can only be failed/dropped, never
    // HandedOff. The production endpoint is not selected or invoked by tests.
    code_ticket
        .finish(BrowserHandoff::Failed)
        .expect("reject browser handoff");
    wait_vacant(&owner.inner).await;
    assert_eq!(
        owner
            .snapshot()
            .expect("snapshot")
            .last_authorization
            .expect("terminal")
            .outcome,
        AuthorizationOutcome::BrowserUnavailable
    );

    for (parameter, expected) in [
        ("error=access_denied", AuthorizationOutcome::Denied),
        (
            "error=temporarily_unavailable",
            AuthorizationOutcome::ProviderRejected,
        ),
    ] {
        let ticket = launch_ticket(&owner).await;
        let endpoint = LocalCallback::from_ticket(&ticket);
        let before = owner.snapshot().expect("before invalid callback");
        assert!(
            !endpoint
                .clone()
                .send("wrong-independent-state".to_owned(), parameter)
                .await
        );
        assert_eq!(owner.snapshot().expect("invalid callback is inert"), before);
        ticket
            .finish(BrowserHandoff::HandedOff)
            .expect("only an error callback is acknowledged");
        assert!(
            endpoint
                .clone()
                .send(endpoint.state.clone(), parameter)
                .await
        );
        wait_vacant(&owner.inner).await;
        let after = owner.snapshot().expect("after denial");
        assert_eq!(
            after.last_authorization.expect("terminal").outcome,
            expected
        );
        assert!(after.granted_subset_pending_verification.is_empty());
        owner
            .inner
            .finish(old_key, AuthorizationOutcome::WorkerFailure);
        assert_eq!(owner.snapshot().expect("stale result is inert"), after);
    }
    owner.shutdown().await.expect("clean shutdown");
}

#[tokio::test]
async fn independent_live_partial_client_and_held_ticket_fence_shutdown_until_drop() {
    let owner = SessionController::new();
    let ticket = launch_ticket(&owner).await;
    let endpoint = LocalCallback::from_ticket(&ticket);
    let mut client = TcpStream::connect_timeout(&endpoint.address, Duration::from_secs(1))
        .expect("real partial client");
    client
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("bounded write");
    client.write_all(b"GET /").expect("partial request");
    let key = ticket.key;
    owner
        .invalidate(DisconnectOrSwitch::SwitchAccount)
        .expect("invalidate real worker");
    let terminal = owner.snapshot().expect("draining").last_authorization;
    assert_eq!(owner.snapshot().expect("draining").phase, Phase::Draining);
    assert!(matches!(
        owner.begin(native_request()).await,
        Err(StartError::Busy)
    ));
    let mut shutdown = Box::pin(owner.shutdown());
    tokio::select! {
        biased;
        _ = &mut shutdown => panic!("held ticket must retain cleanup slot"),
        _ = tokio::time::sleep(Duration::from_millis(30)) => {},
    }
    assert_eq!(
        owner.snapshot().expect("closed draining").phase,
        Phase::Draining
    );
    assert_eq!(
        ticket.finish(BrowserHandoff::Failed),
        Err(TicketError::Stale)
    );
    let _ = client.shutdown(Shutdown::Both);
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), &mut shutdown)
        .await
        .expect("bounded cooperative shutdown")
        .expect("shutdown");
    assert_eq!(
        owner.snapshot().expect("closed snapshot").phase,
        Phase::Closed
    );
    assert_eq!(
        owner.snapshot().expect("once terminal").last_authorization,
        terminal
    );
    owner.inner.finish(key, AuthorizationOutcome::WorkerFailure);
    assert_eq!(
        owner.snapshot().expect("late worker").last_authorization,
        terminal
    );
    assert!(TcpStream::connect_timeout(&endpoint.address, Duration::from_millis(100)).is_err());
}

#[test]
fn independent_same_reducer_rejects_identity_grant_phase_and_exact_expiry() {
    let (owner, work) = reducer_fixture();
    assert!(
        owner
            .inner
            .adopt_pending(
                work.key,
                pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
                Instant::now()
            )
            .is_none()
    );
    enter_exchange(&owner, work.key, Instant::now());
    let untouched = owner.snapshot().expect("exchanging");
    let wrong_id = Key {
        id: Uuid::nil(),
        generation: work.key.generation,
    };
    let wrong_generation = Key {
        id: work.key.id,
        generation: work.key.generation + 1,
    };
    for (identity, expires, scopes) in [
        (wrong_id, work.deadline, vec![ApprovedScope::File]),
        (wrong_generation, work.deadline, vec![ApprovedScope::File]),
        (work.key, Instant::now(), vec![ApprovedScope::File]),
        (work.key, work.deadline, vec![]),
        (
            work.key,
            work.deadline,
            vec![ApprovedScope::File, ApprovedScope::File],
        ),
        (work.key, work.deadline, vec![ApprovedScope::AppData]),
        (
            work.key,
            work.deadline,
            vec![ApprovedScope::File, ApprovedScope::AppData],
        ),
    ] {
        assert!(
            owner
                .inner
                .adopt_pending(
                    work.key,
                    pending_marker(identity, expires, scopes),
                    Instant::now()
                )
                .is_none()
        );
        assert_eq!(
            owner.snapshot().expect("rejection leaves state intact"),
            untouched
        );
    }
    assert!(
        owner
            .inner
            .adopt_pending(
                work.key,
                pending_marker(
                    work.key,
                    work.deadline + Duration::from_secs(1),
                    vec![ApprovedScope::File]
                ),
                work.deadline
            )
            .is_none()
    );
    assert_eq!(owner.snapshot().expect("deadline rejection"), untouched);
    assert_eq!(
        owner.inner.adopt_pending(
            work.key,
            pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
            Instant::now()
        ),
        Some(work.deadline)
    );
    let pending = owner.snapshot().expect("pending");
    assert_eq!(pending.phase, Phase::AccountVerificationPending);
    assert_eq!(
        pending.granted_subset_pending_verification,
        vec![ApprovedScope::File]
    );
    assert!(matches!(
        owner.inner.reserve(native_request(), Instant::now()),
        Err(StartError::Busy)
    ));
    owner.inner.discard_pending(work.key);
    assert_eq!(
        owner
            .snapshot()
            .expect("discard preserves outcome")
            .last_authorization,
        pending.last_authorization
    );
    owner.inner.reaped(work.key); // Reducer-only fixture never started a worker.
}

#[test]
fn independent_completion_and_invalidation_orders_and_barrier_share_one_commit() {
    for choice in 0..3 {
        for completion_first in [false, true] {
            let (owner, work) = reducer_fixture();
            enter_exchange(&owner, work.key, Instant::now());
            if completion_first {
                assert!(
                    owner
                        .inner
                        .adopt_pending(
                            work.key,
                            pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
                            Instant::now()
                        )
                        .is_some()
                );
            }
            invalidate_owner(&owner, work.key, choice);
            assert!(
                owner
                    .inner
                    .adopt_pending(
                        work.key,
                        pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
                        Instant::now()
                    )
                    .is_none()
            );
            let after = owner.snapshot().expect("after invalidation");
            assert_eq!(after.phase, Phase::Draining);
            assert!(after.granted_subset_pending_verification.is_empty());
            assert_eq!(
                after.last_authorization.expect("one result").outcome,
                if completion_first {
                    AuthorizationOutcome::CompletedPendingVerification
                } else {
                    AuthorizationOutcome::Cancelled
                }
            );
            owner.inner.reaped(work.key);
        }
        let (owner, work) = reducer_fixture();
        enter_exchange(&owner, work.key, Instant::now());
        let key = work.key;
        let deadline = work.deadline;
        let barrier = Barrier::new(3);
        let adopted = std::thread::scope(|scope| {
            let complete = scope.spawn(|| {
                barrier.wait();
                owner.inner.adopt_pending(
                    key,
                    pending_marker(key, deadline, vec![ApprovedScope::File]),
                    Instant::now(),
                )
            });
            let invalidate = scope.spawn(|| {
                barrier.wait();
                invalidate_owner(&owner, key, choice);
            });
            barrier.wait();
            let result = complete.join().expect("completion joined");
            invalidate.join().expect("invalidation joined");
            result.is_some()
        });
        let after = owner.snapshot().expect("joined race");
        assert_eq!(after.phase, Phase::Draining);
        assert!(after.granted_subset_pending_verification.is_empty());
        assert_eq!(
            after
                .last_authorization
                .expect("winner exactly determines outcome")
                .outcome,
            if adopted {
                AuthorizationOutcome::CompletedPendingVerification
            } else {
                AuthorizationOutcome::Cancelled
            }
        );
        owner.inner.reaped(key);
        let replacement = owner
            .inner
            .reserve(native_request(), Instant::now())
            .expect("replacement");
        let before_late = owner.snapshot().expect("new reservation");
        assert!(
            owner
                .inner
                .adopt_pending(
                    key,
                    pending_marker(key, deadline, vec![ApprovedScope::File]),
                    Instant::now()
                )
                .is_none()
        );
        owner.inner.finish(key, AuthorizationOutcome::WorkerFailure);
        assert_eq!(
            owner.snapshot().expect("old completion cannot overwrite"),
            before_late
        );
        owner
            .inner
            .finish(replacement.key, AuthorizationOutcome::Cancelled);
        owner.inner.reaped(replacement.key);
    }
}

#[tokio::test]
async fn independent_original_deadline_and_retention_keep_one_authorization_result() {
    const {
        assert!(ATTEMPT_LIFETIME.as_secs() == 300);
        assert!(STARTUP_LIFETIME.as_secs() == 5);
        assert!(HANDOFF_LIFETIME.as_secs() == 10);
    }
    let (owner, work) = reducer_fixture();
    let original = work.deadline;
    let near_end = original - Duration::from_millis(100);
    enter_exchange(&owner, work.key, near_end);
    let expires = owner
        .inner
        .adopt_pending(
            work.key,
            pending_marker(
                work.key,
                original + Duration::from_secs(60),
                vec![ApprovedScope::File],
            ),
            near_end,
        )
        .expect("pending at original boundary");
    assert_eq!(expires, original);
    let once = owner.snapshot().expect("pending").last_authorization;
    {
        let mut state = owner.inner.state.lock().expect("state");
        assert_eq!(state.slot.as_ref().expect("slot").deadline, original);
        expire_pending(&mut state, original);
    }
    let after = owner.snapshot().expect("absolute expiry");
    assert_eq!(after.phase, Phase::Draining);
    assert_eq!(after.last_authorization, once);
    assert!(after.granted_subset_pending_verification.is_empty());
    owner.inner.reaped(work.key);

    let (owner, mut work) = reducer_fixture();
    enter_exchange(&owner, work.key, Instant::now());
    let expiry = Instant::now() + Duration::from_millis(30);
    let retained_until = owner
        .inner
        .adopt_pending(
            work.key,
            pending_marker(work.key, expiry, vec![ApprovedScope::File]),
            Instant::now(),
        )
        .expect("short marker");
    let once = owner.snapshot().expect("pending").last_authorization;
    tokio::time::timeout(
        Duration::from_secs(1),
        retain_pending(&owner.inner, work.key, retained_until, &mut work.stop),
    )
    .await
    .expect("real retention helper deadline");
    let after = owner.snapshot().expect("retention expired");
    assert_eq!(after.phase, Phase::Draining);
    assert_eq!(after.last_authorization, once);
    assert!(after.granted_subset_pending_verification.is_empty());
    owner.inner.reaped(work.key);
}

#[tokio::test]
async fn independent_counter_fault_with_real_worker_and_poisoned_pending_fail_closed() {
    let owner = SessionController::new();
    owner.inner.state.lock().expect("state").generation = u64::MAX - 1;
    let ticket = launch_ticket(&owner).await;
    let key = ticket.key;
    assert_eq!(key.generation, u64::MAX);
    assert_eq!(owner.cancel(key.id), Err(ControllerError::CounterExhausted));
    assert_eq!(owner.snapshot(), Err(ControllerError::CounterExhausted));
    assert_eq!(
        ticket.finish(BrowserHandoff::Failed),
        Err(TicketError::Controller(ControllerError::CounterExhausted))
    );
    tokio::time::timeout(Duration::from_secs(3), owner.shutdown())
        .await
        .expect("bounded fault cleanup")
        .expect_err("counter fault remains latched");
    assert!(matches!(
        owner.begin(native_request()).await,
        Err(StartError::Controller(ControllerError::CounterExhausted))
    ));
    assert!(owner.inner.state.lock().expect("state").slot.is_none());

    let (owner, work) = reducer_fixture();
    enter_exchange(&owner, work.key, Instant::now());
    assert!(
        owner
            .inner
            .adopt_pending(
                work.key,
                pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
                Instant::now()
            )
            .is_some()
    );
    let before = owner.snapshot().expect("pending").last_authorization;
    let inner = Arc::clone(&owner.inner);
    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _held = inner.state.lock().expect("state");
            panic!("independent synthetic mutex poison");
        }))
        .is_err()
    );
    assert_eq!(owner.snapshot(), Err(ControllerError::Poisoned));
    assert!(
        owner
            .inner
            .adopt_pending(
                work.key,
                pending_marker(work.key, work.deadline, vec![ApprovedScope::File]),
                Instant::now()
            )
            .is_none()
    );
    {
        let state = inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert!(state.closed);
        assert_eq!(state.fault, Some(ControllerError::Poisoned));
        assert!(
            state
                .slot
                .as_ref()
                .expect("cleanup fence")
                .pending
                .is_none()
        );
        assert_eq!(state.last, before);
        assert!(*work.stop.borrow());
    }
    inner.reaped(work.key); // No worker exists for this reducer-only marker.
    assert_eq!(owner.shutdown().await, Err(ControllerError::Poisoned));
}
