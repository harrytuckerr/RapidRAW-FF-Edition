//! A genuine drive_auth sibling: uses only the restricted bridge/public metadata.
//! Synthetic localhost responses do not prove provider identity or S3 adoption.
use super::account::session_test_support::{
    FixtureError, FixtureSpec, SyntheticLabel, verify_fixture,
};
use super::account::{AccountError, RegistrationField, VerificationContext};
use super::exchange::ExchangeAttempt;
use super::pkce::ApprovedScope;
use super::token_response::TokenResponseError;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use uuid::Uuid;

type Schedule = Vec<(Duration, Vec<u8>)>;

// Own nonblocking fixture, not the author's Server. At most four connections,
//16KiB captured per request,256KiB scheduled output,4s lifetime and joined Drop.
struct PeerHarness {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
    sent: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}
fn live(stop: &AtomicBool, end: Instant) -> bool {
    !stop.load(Ordering::Acquire) && Instant::now() < end
}
fn pause_until(until: Instant, stop: &AtomicBool, end: Instant) -> bool {
    while live(stop, end) && Instant::now() < until {
        thread::sleep(Duration::from_millis(2));
    }
    live(stop, end)
}
fn request_bytes(stream: &mut TcpStream, stop: &AtomicBool, end: Instant) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 1024];
    while live(stop, end) {
        match stream.read(&mut buffer) {
            Ok(0) => return None,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            Err(_) => return None,
        }
        if bytes.len() > 16_384 {
            return None;
        }
        let Some(boundary) = bytes.windows(4).position(|x| x == b"\r\n\r\n") else {
            continue;
        };
        let header = std::str::from_utf8(&bytes[..boundary]).ok()?;
        let lengths: Vec<_> = header
            .split("\r\n")
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .collect();
        if lengths.len() > 1 {
            return None;
        }
        let length = match lengths.first() {
            Some((_, value)) => value.trim().parse::<usize>().ok()?,
            None => 0,
        };
        let total = (boundary + 4).checked_add(length)?;
        if total > 16_384 {
            return None;
        }
        if bytes.len() >= total {
            bytes.truncate(total);
            return Some(bytes);
        }
    }
    None
}
fn send_bytes(stream: &mut TcpStream, bytes: &[u8], stop: &AtomicBool, end: Instant) -> bool {
    let mut offset = 0;
    while offset < bytes.len() && live(stop, end) {
        match stream.write(&bytes[offset..]) {
            Ok(0) => return false,
            Ok(n) => offset += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(_) => return false,
        }
    }
    offset == bytes.len() && live(stop, end)
}
impl PeerHarness {
    fn reply(bytes: Vec<u8>) -> Self {
        Self::schedule(vec![(Duration::ZERO, bytes)])
    }
    fn schedule(plan: Schedule) -> Self {
        assert!(plan.len() <= 256);
        assert!(plan.iter().map(|(_, b)| b.len()).sum::<usize>() <= 256 * 1024);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sent = Arc::new(AtomicUsize::new(0));
        let sent_parts = Arc::clone(&sent);
        let (flag, count, requests) = (stop.clone(), accepted.clone(), captured.clone());
        let worker = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(4);
            while live(&flag, end) && count.load(Ordering::Acquire) < 4 {
                let mut stream = match listener.accept() {
                    Ok((peer, _)) => peer,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                count.fetch_add(1, Ordering::AcqRel);
                if stream.set_nonblocking(true).is_err() {
                    break;
                }
                if let Some(request) = request_bytes(&mut stream, &flag, end) {
                    if let Ok(mut log) = requests.lock() {
                        log.push(request);
                    } else {
                        break;
                    }
                    for (delay, bytes) in &plan {
                        if !pause_until(Instant::now() + *delay, &flag, end)
                            || !send_bytes(&mut stream, bytes, &flag, end)
                        {
                            break;
                        }
                        sent_parts.fetch_add(1, Ordering::AcqRel);
                    }
                }
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
        Self {
            address,
            stop,
            accepted,
            captured,
            sent,
            worker: Some(worker),
        }
    }
    fn count(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }
    fn requests(&self) -> Vec<Vec<u8>> {
        self.captured.lock().unwrap().clone()
    }
    async fn wrote_parts(&self, parts: usize) {
        let end = Instant::now() + Duration::from_secs(1);
        while self.sent.load(Ordering::Acquire) < parts {
            assert!(
                Instant::now() < end,
                "bounded peer did not write response part"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    async fn received(&self) {
        let end = Instant::now() + Duration::from_secs(1);
        while self.requests().is_empty() {
            assert!(
                Instant::now() < end,
                "bounded fixture did not receive request"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}
impl Drop for PeerHarness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn attempt(id: u128) -> ExchangeAttempt {
    ExchangeAttempt {
        id: Uuid::from_u128(id),
        generation: 67,
    }
}
fn context(id: u128, requested: &[ApprovedScope], deadline: Instant) -> VerificationContext {
    VerificationContext::from_native_attempt(
        "independent-lifetime",
        "independent-lifetime.apps.googleusercontent.com",
        "90210",
        requested,
        &attempt(id),
        deadline,
    )
    .expect("native syntax only, not provider mapping proof")
}
fn spec(
    label: SyntheticLabel,
    id: u128,
    scopes: &[ApprovedScope],
    issued: Instant,
    seconds: u64,
) -> FixtureSpec {
    FixtureSpec::new(label, attempt(id), scopes, issued, seconds)
        .expect("restricted input syntax, not proof")
}
fn response(body: &[u8]) -> Vec<u8> {
    assert!(body.len() <= 4096);
    let mut bytes = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn failure<T, E>(result: Result<T, E>) -> E {
    match result {
        Err(error) => error,
        Ok(_) => panic!("fixed negative expectation"),
    }
}

#[tokio::test]
async fn independent_sibling_delayed_fixture_keeps_original_expiry_and_distinct_admission() {
    let peer = PeerHarness::reply(response(br#"{"user":{"permissionId":"sibling_A"}}"#));
    let issued = Instant::now() - Duration::from_secs(6);
    let expected_expiry = issued + Duration::from_secs(30);
    let admission = Instant::now() + Duration::from_secs(3);
    let input = spec(SyntheticLabel::A, 101, &[ApprovedScope::File], issued, 30);
    // Actual dispatch is later than fixture parsing; exact expected expiry is
    // independently fixed from the original issue time, never from dispatch.
    tokio::time::sleep(Duration::from_millis(12)).await;
    let expected = context(
        101,
        &[ApprovedScope::File, ApprovedScope::AppData],
        admission,
    );
    let (keep, cancel) = oneshot::channel();
    let candidate = verify_fixture(
        input,
        context(
            101,
            &[ApprovedScope::AppData, ApprovedScope::File],
            admission,
        ),
        cancel,
        peer.address,
    )
    .await
    .expect("real bounded local response required");
    let now = Instant::now();
    assert!(now < admission);
    assert!(admission < expected_expiry);
    assert_eq!(candidate.credential_expires_at(), expected_expiry);
    assert!(candidate.credentials_usable_at(now));
    assert!(candidate.matches_native_attempt(&expected, now));
    assert!(!candidate.credentials_usable_at(issued - Duration::from_nanos(1)));
    assert!(!candidate.matches_native_attempt(&expected, issued - Duration::from_nanos(1)));
    assert!(candidate.credentials_usable_at(issued));
    assert!(candidate.credentials_usable_at(admission));
    assert!(!candidate.matches_native_attempt(&expected, admission));
    assert!(candidate.credentials_usable_at(admission + Duration::from_nanos(1)));
    assert!(!candidate.matches_native_attempt(&expected, admission + Duration::from_nanos(1)));
    assert!(candidate.credentials_usable_at(expected_expiry - Duration::from_nanos(1)));
    assert!(!candidate.credentials_usable_at(expected_expiry));
    assert!(!candidate.matches_native_attempt(&expected, expected_expiry));
    assert!(!candidate.credentials_usable_at(expected_expiry + Duration::from_nanos(1)));
    assert_eq!(candidate.credential_expires_at(), expected_expiry);
    assert_eq!(
        candidate.account_key().drive_permission_id.as_str(),
        "sibling_A"
    );
    assert_eq!(peer.count(), 1);
    drop(keep);
}

#[tokio::test]
async fn independent_sibling_labels_send_fixed_credentials_and_preserve_actual_partial_grants() {
    for (label, id, scope, missing, access, body, permission) in [
        (
            SyntheticLabel::A,
            201,
            ApprovedScope::File,
            ApprovedScope::AppData,
            "Bearer synthetic-session-A",
            br#"{"user":{"permissionId":"sibling_A"}}"#.as_slice(),
            "sibling_A",
        ),
        (
            SyntheticLabel::B,
            202,
            ApprovedScope::AppData,
            ApprovedScope::File,
            "Bearer synthetic-session-B",
            br#"{"user":{"permissionId":"sibling_B"}}"#.as_slice(),
            "sibling_B",
        ),
    ] {
        let peer = PeerHarness::reply(response(body));
        let issued = Instant::now() - Duration::from_secs(3);
        let admission = Instant::now() + Duration::from_secs(3);
        let input = spec(label, id, &[scope], issued, 20);
        let (keep, cancel) = oneshot::channel();
        let candidate = verify_fixture(
            input,
            context(
                id,
                &[ApprovedScope::AppData, ApprovedScope::File],
                admission,
            ),
            cancel,
            peer.address,
        )
        .await
        .expect("bridge must execute actual transport/parser");
        assert_eq!(
            candidate.credential_expires_at(),
            issued + Duration::from_secs(20)
        );
        assert_eq!(candidate.account_key().project_number.value(), 90210);
        assert_eq!(
            candidate.account_key().drive_permission_id.as_str(),
            permission
        );
        assert_eq!(candidate.granted_scopes(), &[scope]);
        assert_eq!(candidate.missing_requested_scopes(), &[missing]);
        let captures = peer.requests();
        assert_eq!(captures.len(), 1);
        let wire = std::str::from_utf8(&captures[0]).expect("synthetic request capture");
        let (head, body) = wire.split_once("\r\n\r\n").expect("complete GET");
        assert!(body.is_empty());
        let mut lines = head.split("\r\n");
        let first = lines.next().expect("request line");
        let words = first.split(' ').collect::<Vec<_>>();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0], "GET");
        let target = reqwest::Url::parse(&format!("http://127.0.0.1{}", words[1]))
            .expect("relative native target");
        assert_eq!(target.path(), "/drive/v3/about");
        assert_eq!(
            target.query_pairs().into_owned().collect::<Vec<_>>(),
            vec![(
                "fields".to_owned(),
                "user(permissionId,displayName,emailAddress),storageQuota".to_owned()
            )]
        );
        let headers = lines
            .map(|line| line.split_once(':').expect("header pair"))
            .collect::<Vec<_>>();
        let authorization = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| value.trim())
            .collect::<Vec<_>>();
        assert_eq!(authorization, vec![access]);
        assert!(!first.contains("synthetic-session"));
        assert!(!wire.contains("synthetic-session-refresh"));
        for (name, value) in headers {
            if !name.eq_ignore_ascii_case("authorization") {
                assert!(!value.contains("synthetic-session"));
            }
            assert!(!name.eq_ignore_ascii_case("cookie"));
        }
        drop(keep);
    }
    // Fixed label/permission pairs are synthetic custody answers, never an
    // authentication oracle for a dishonest provider or configured project.
}

#[tokio::test]
async fn independent_sibling_expiry_invalid_inputs_targets_and_bodies_cannot_create_proof() {
    let peer = PeerHarness::reply(response(
        br#"{"user":{"permissionId":"must_not_dispatch"}}"#,
    ));
    for mode in 0..5 {
        let issued = if mode == 0 {
            Instant::now() - Duration::from_secs(3)
        } else {
            Instant::now()
        };
        let input = spec(
            SyntheticLabel::A,
            301,
            &[ApprovedScope::File],
            issued,
            if mode == 0 { 1 } else { 30 },
        );
        let id = if mode == 1 { 302 } else { 301 };
        let requested = if mode == 2 {
            vec![ApprovedScope::AppData]
        } else {
            vec![ApprovedScope::File]
        };
        let target = match mode {
            3 => SocketAddr::from(([127, 0, 0, 2], peer.address.port())),
            4 => SocketAddr::from(([127, 0, 0, 1], 0)),
            _ => peer.address,
        };
        let want = match mode {
            0 => AccountError::TokenExpired,
            1 => AccountError::AttemptMismatch,
            2 => AccountError::InvalidGrantedScopes,
            _ => AccountError::ClientSetup,
        };
        let (keep, cancel) = oneshot::channel();
        assert_eq!(
            failure(
                verify_fixture(
                    input,
                    context(id, &requested, Instant::now() + Duration::from_secs(3)),
                    cancel,
                    target
                )
                .await
            ),
            want
        );
        drop(keep);
    }
    assert_eq!(peer.count(), 0);
    let now = Instant::now();
    assert_eq!(
        failure(FixtureSpec::new(
            SyntheticLabel::A,
            ExchangeAttempt {
                id: Uuid::nil(),
                generation: 67
            },
            &[ApprovedScope::File],
            now,
            30
        )),
        FixtureError::Binding(AccountError::InvalidRegistration(
            RegistrationField::AttemptIdentity
        ))
    );
    assert_eq!(
        failure(FixtureSpec::new(
            SyntheticLabel::B,
            attempt(301),
            &[ApprovedScope::File, ApprovedScope::File],
            now,
            30
        )),
        FixtureError::Tokens(TokenResponseError::InvalidRequestedScopes)
    );
    assert_eq!(
        failure(FixtureSpec::new(
            SyntheticLabel::A,
            attempt(301),
            &[ApprovedScope::File],
            now,
            0
        )),
        FixtureError::Tokens(TokenResponseError::InvalidExpiry)
    );
    for (body, want) in [
        (b"{}".as_slice(), AccountError::MissingUser),
        (b"[true]".as_slice(), AccountError::MalformedResponse),
    ] {
        let peer = PeerHarness::reply(response(body));
        let (keep, cancel) = oneshot::channel();
        assert_eq!(
            failure(
                verify_fixture(
                    spec(
                        SyntheticLabel::B,
                        303,
                        &[ApprovedScope::File],
                        Instant::now(),
                        30
                    ),
                    context(
                        303,
                        &[ApprovedScope::File],
                        Instant::now() + Duration::from_secs(3)
                    ),
                    cancel,
                    peer.address
                )
                .await
            ),
            want
        );
        assert_eq!(peer.count(), 1);
        drop(keep);
    }
}

#[tokio::test]
async fn independent_sibling_bridge_forwards_signal_and_drop_cancellation() {
    let peer = PeerHarness::reply(response(
        br#"{"user":{"permissionId":"must_not_dispatch"}}"#,
    ));
    for signal in [false, true] {
        let input = spec(
            SyntheticLabel::A,
            401,
            &[ApprovedScope::File],
            Instant::now(),
            30,
        );
        let context = context(
            401,
            &[ApprovedScope::File],
            Instant::now() + Duration::from_secs(3),
        );
        let (tx, cancel) = oneshot::channel();
        if signal {
            tx.send(()).expect("pre signal");
        } else {
            drop(tx);
        }
        assert_eq!(
            failure(verify_fixture(input, context, cancel, peer.address).await),
            AccountError::Cancelled
        );
    }
    assert_eq!(peer.count(), 0);
    for signal in [false, true] {
        let peer=PeerHarness::schedule(vec![
            (Duration::ZERO,b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n\r\n{".to_vec()),
            (Duration::from_secs(1),b" ".to_vec()),
        ]);
        let (tx, cancel) = oneshot::channel();
        let operation = verify_fixture(
            spec(
                SyntheticLabel::B,
                402,
                &[ApprovedScope::File],
                Instant::now(),
                30,
            ),
            context(
                402,
                &[ApprovedScope::File],
                Instant::now() + Duration::from_secs(3),
            ),
            cancel,
            peer.address,
        );
        tokio::pin!(operation);
        tokio::select! {
            result=&mut operation=>{let _=result;panic!("incomplete response cannot return before cancellation");},
            _=async {peer.received().await;peer.wrote_parts(1).await;}=>{},
        }
        if signal {
            tx.send(()).expect("midstream signal");
        } else {
            drop(tx);
        }
        assert_eq!(failure(operation.await), AccountError::Cancelled);
        assert_eq!(peer.count(), 1);
    }
}
