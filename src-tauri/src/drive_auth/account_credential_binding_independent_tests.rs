//! Independent borrowed-binding checks. Candidates come only from the real
//! parser plus restricted localhost verifier; no production provider entry.
use super::super::token_response::parse_google_token_response;
use super::{
    AccountError, ApprovedScope, ExchangeAttempt, Limits, OwnedInput, Target, VerificationContext,
    VerifiedAccountCandidate, verify_at,
};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use uuid::Uuid;

// Own single-request fixture: ephemeral IPv4 localhost, nonblocking I/O,
// 8KiB capture, 4KiB reply and a stop-aware3s worker ceiling. Drop joins even
// during assertion unwinding. Native syscall scheduling is not a hard bound.
struct BindingPeer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicBool>,
    request: Arc<Mutex<Option<Vec<u8>>>>,
    worker: Option<JoinHandle<()>>,
}
impl BindingPeer {
    fn new(permission: &'static str) -> Self {
        let body = serde_json::to_vec(&serde_json::json!({
            "user": {"permissionId": permission}
        }))
        .expect("fixed account body");
        let mut reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        reply.extend_from_slice(&body);
        assert!(reply.len() <= 4096);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("owned local peer");
        listener.set_nonblocking(true).expect("nonblocking accept");
        let address = listener.local_addr().expect("ephemeral address");
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicBool::new(false));
        let request = Arc::new(Mutex::new(None));
        let (halt, count, captured) = (stop.clone(), accepted.clone(), request.clone());
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let live = || !halt.load(Ordering::Acquire) && Instant::now() < deadline;
            while live() {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => return,
                };
                count.store(true, Ordering::Release);
                if stream.set_nonblocking(true).is_err() {
                    return;
                }
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 1024];
                while live() && !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) => return,
                        Ok(size) => {
                            if bytes.len() + size > 8192 {
                                return;
                            }
                            bytes.extend_from_slice(&buffer[..size]);
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) =>
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => return,
                    }
                }
                if !live() {
                    return;
                }
                match captured.lock() {
                    Ok(mut slot) => *slot = Some(bytes),
                    Err(_) => return,
                }
                let mut offset = 0;
                while live() && offset < reply.len() {
                    match stream.write(&reply[offset..]) {
                        Ok(0) => return,
                        Ok(size) => offset += size,
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) =>
                        {
                            thread::sleep(Duration::from_millis(2));
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
            accepted,
            request,
            worker: Some(worker),
        }
    }
    fn wire(&self) -> String {
        let bytes = self
            .request
            .lock()
            .expect("bounded capture mutex")
            .as_ref()
            .expect("actual request captured")
            .clone();
        String::from_utf8(bytes).expect("fixed ASCII request")
    }
}
impl Drop for BindingPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// Only unverified input/configuration is constructed. No private candidate or
// VerificationContext fields, author's new helpers or proof constructor used.
struct BindingCase {
    id: u128,
    generation: u64,
    registration: &'static str,
    client: &'static str,
    project: &'static str,
    access: &'static str,
    refresh: Option<&'static str>,
    requested: Vec<ApprovedScope>,
    actual: Vec<ApprovedScope>,
    started: Instant,
    validated: Instant,
    admission: Instant,
    seconds: u64,
}
impl BindingCase {
    fn a(base: Instant) -> Self {
        Self {
            id: 0x8101,
            generation: 41,
            registration: "validator.desktop_A",
            client: "binding-a.apps.googleusercontent.com",
            project: "9007199254740993",
            access: "BINDING_ACCESS_A-._~+/==",
            refresh: Some("REFRESH_ONLY::CANARY/+/=opaque"),
            requested: vec![ApprovedScope::File, ApprovedScope::AppData],
            actual: vec![ApprovedScope::AppData, ApprovedScope::File],
            started: base - Duration::from_secs(9),
            validated: base - Duration::from_secs(4),
            admission: base + Duration::from_secs(25),
            seconds: 180,
        }
    }
    fn attempt(&self) -> ExchangeAttempt {
        ExchangeAttempt {
            id: Uuid::from_u128(self.id),
            generation: self.generation,
        }
    }
    fn context_for(&self, attempt: &ExchangeAttempt) -> VerificationContext {
        VerificationContext::from_native_attempt(
            self.registration,
            self.client,
            self.project,
            &self.requested,
            attempt,
            self.admission,
        )
        .expect("fixed native syntax, not provider-attested mapping")
    }
    fn context(&self) -> VerificationContext {
        self.context_for(&self.attempt())
    }
    fn input(&self) -> OwnedInput {
        let mut body = serde_json::json!({
            "access_token": self.access, "token_type": "Bearer", "expires_in": self.seconds,
            "scope": self.actual.iter().map(|scope| scope.as_str()).collect::<Vec<_>>().join(" ")
        });
        if let Some(refresh) = self.refresh {
            body["refresh_token"] = serde_json::json!(refresh);
        }
        let body = serde_json::to_vec(&body).expect("fixed synthetic token bytes");
        let tokens =
            parse_google_token_response(200, &body, &self.requested, self.started, self.validated)
                .expect("real parser creates unverified input only");
        OwnedInput::Fixture {
            attempt: self.attempt(),
            tokens,
        }
    }
}
fn local_limits() -> Limits {
    Limits {
        total: Duration::from_secs(2),
        connect: Duration::from_millis(500),
        read: Duration::from_millis(500),
    }
}
async fn verified(case: &BindingCase, peer: &BindingPeer) -> VerifiedAccountCandidate {
    let (keep, cancel) = oneshot::channel();
    let result = verify_at(
        case.input(),
        case.context(),
        cancel,
        Target::Loopback(peer.address),
        local_limits(),
    )
    .await;
    drop(keep);
    let candidate = result.expect("actual restricted HTTP verifier must succeed");
    assert!(peer.accepted.load(Ordering::Acquire));
    candidate
}
fn assert_account_request(wire: &str, access: &str, refresh: Option<&str>) {
    let (head, body) = wire.split_once("\r\n\r\n").expect("complete account GET");
    assert!(body.is_empty());
    let mut lines = head.split("\r\n");
    let request = lines
        .next()
        .expect("request line")
        .split(' ')
        .collect::<Vec<_>>();
    assert_eq!(request.len(), 3);
    assert_eq!(request[0], "GET");
    assert_eq!(request[2], "HTTP/1.1");
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", request[1]))
        .expect("captured relative path");
    assert_eq!(url.path(), "/drive/v3/about");
    assert_eq!(
        url.query_pairs().into_owned().collect::<Vec<_>>(),
        vec![(
            "fields".to_owned(),
            "user(permissionId,displayName,emailAddress),storageQuota".to_owned()
        )]
    );
    let headers = lines
        .map(|line| line.split_once(':').expect("header pair"))
        .collect::<Vec<_>>();
    let auth = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    assert_eq!(auth, vec![format!("Bearer {access}")]);
    assert_eq!(wire.matches(access).count(), 1);
    assert!(!request[1].contains(access));
    assert!(
        headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .all(|(_, value)| value.trim() == "0")
    );
    if let Some(refresh) = refresh {
        assert!(!wire.contains(refresh));
    }
}

#[tokio::test]
async fn independent_binding_fixed_tuple_and_scope_order() {
    let case = BindingCase::a(Instant::now());
    let peer = BindingPeer::new("credential_A");
    let candidate = verified(&case, &peer).await;
    let view = candidate.credential_binding();
    assert_eq!(
        view.account_key().project_number.value(),
        9_007_199_254_740_993
    );
    assert_eq!(
        view.account_key().drive_permission_id.as_str(),
        "credential_A"
    );
    assert_eq!(view.registration_id(), "validator.desktop_A");
    assert_eq!(view.client_id(), "binding-a.apps.googleusercontent.com");
    assert_eq!(
        view.requested_scopes(),
        &[ApprovedScope::File, ApprovedScope::AppData]
    );
    assert_eq!(
        view.granted_scopes(),
        &[ApprovedScope::AppData, ApprovedScope::File]
    );
    assert_eq!(view.attempt_id(), Uuid::from_u128(0x8101));
    assert_eq!(view.generation(), 41);
    assert_eq!(view.admission_deadline(), case.admission);
    assert_eq!(
        view.credential_expires_at(),
        case.started + Duration::from_secs(180)
    );
    assert!(view.refresh_token_present());
    assert!(candidate.missing_requested_scopes().is_empty());
    assert!(candidate.matches_native_attempt(&case.context(), Instant::now()));
    assert_account_request(&peer.wire(), case.access, case.refresh);
}

#[tokio::test]
async fn independent_binding_refresh_presence_and_wire_privacy() {
    for (refresh, present) in [
        (None, false),
        (Some("REFRESH_ONLY::CANARY/+/=opaque"), true),
    ] {
        let mut case = BindingCase::a(Instant::now());
        case.refresh = refresh;
        case.actual = vec![ApprovedScope::File];
        let peer = BindingPeer::new("privacy_id");
        let candidate = verified(&case, &peer).await;
        let view = candidate.credential_binding();
        assert_eq!(view.refresh_token_present(), present);
        assert_eq!(view.granted_scopes(), &[ApprovedScope::File]);
        assert_eq!(
            view.requested_scopes(),
            &[ApprovedScope::File, ApprovedScope::AppData]
        );
        assert_eq!(
            candidate.missing_requested_scopes(),
            &[ApprovedScope::AppData]
        );
        let metadata = format!(
            "{:?}|{}|{}|{:?}|{:?}|{}|{}|{:?}|{:?}|{}",
            view.account_key(),
            view.registration_id(),
            view.client_id(),
            view.requested_scopes(),
            view.granted_scopes(),
            view.attempt_id(),
            view.generation(),
            view.admission_deadline(),
            view.credential_expires_at(),
            view.refresh_token_present()
        );
        assert!(!metadata.contains("BINDING_ACCESS_A-._~+/=="));
        assert!(!metadata.contains("REFRESH_ONLY::CANARY/+/=opaque"));
        assert_account_request(&peer.wire(), case.access, refresh);
    }
}

#[tokio::test]
async fn independent_binding_candidates_remain_distinct_and_attempt_mismatch_rejects() {
    let a_case = BindingCase::a(Instant::now());
    let a_peer = BindingPeer::new("credential_A");
    let a = verified(&a_case, &a_peer).await;
    let a_view = a.credential_binding();
    let mut b_case = BindingCase::a(Instant::now());
    b_case.id = 0x8202;
    b_case.generation = 52;
    b_case.project = "18446744073709551615";
    b_case.registration = "validator.desktop_B";
    b_case.client = "binding-b.apps.googleusercontent.com";
    b_case.access = "BINDING_ACCESS_B";
    b_case.refresh = None;
    b_case.requested = vec![ApprovedScope::AppData, ApprovedScope::File];
    b_case.actual = vec![ApprovedScope::AppData];
    let b_peer = BindingPeer::new("credential_B");
    let b = verified(&b_case, &b_peer).await;
    let b_view = b.credential_binding();
    assert_eq!(b_view.account_key().project_number.value(), u64::MAX);
    assert_eq!(
        b_view.account_key().drive_permission_id.as_str(),
        "credential_B"
    );
    assert_eq!(b_view.registration_id(), "validator.desktop_B");
    assert_eq!(b_view.client_id(), "binding-b.apps.googleusercontent.com");
    assert_eq!(b_view.attempt_id(), Uuid::from_u128(0x8202));
    assert_eq!(b_view.generation(), 52);
    assert_eq!(
        b_view.requested_scopes(),
        &[ApprovedScope::AppData, ApprovedScope::File]
    );
    assert_eq!(b_view.granted_scopes(), &[ApprovedScope::AppData]);
    assert_eq!(b.missing_requested_scopes(), &[ApprovedScope::File]);
    assert!(!b_view.refresh_token_present());
    assert!(!a.matches_native_attempt(&b_case.context(), Instant::now()));
    let mut c_case = BindingCase::a(Instant::now());
    c_case.id = 0x8303;
    c_case.generation = 63;
    c_case.registration = "validator.changed";
    c_case.client = "binding-changed.apps.googleusercontent.com";
    c_case.access = "BINDING_ACCESS_C";
    c_case.actual = vec![ApprovedScope::File];
    let c_peer = BindingPeer::new("credential_A");
    let c = verified(&c_case, &c_peer).await;
    let c_view = c.credential_binding();
    assert_eq!(c_view.account_key(), a_view.account_key());
    assert_eq!(c_view.registration_id(), "validator.changed");
    assert_eq!(
        c_view.client_id(),
        "binding-changed.apps.googleusercontent.com"
    );
    assert_eq!(c_view.attempt_id(), Uuid::from_u128(0x8303));
    assert_eq!(c_view.generation(), 63);
    assert_eq!(c_view.granted_scopes(), &[ApprovedScope::File]);
    assert!(!a.matches_native_attempt(&c_case.context(), Instant::now()));
    assert!(c.matches_native_attempt(&c_case.context(), Instant::now()));
    assert_eq!(a_view.registration_id(), "validator.desktop_A");
    assert_eq!(a_view.client_id(), "binding-a.apps.googleusercontent.com");
    assert_eq!(a_view.attempt_id(), Uuid::from_u128(0x8101));
    assert_eq!(a_view.generation(), 41);
    assert_eq!(
        a_view.granted_scopes(),
        &[ApprovedScope::AppData, ApprovedScope::File]
    );
    assert_eq!(a_view.admission_deadline(), a_case.admission);
    assert_eq!(
        a_view.credential_expires_at(),
        a_case.started + Duration::from_secs(180)
    );
    assert_account_request(&a_peer.wire(), a_case.access, a_case.refresh);
    assert_account_request(&b_peer.wire(), b_case.access, b_case.refresh);
    assert_account_request(&c_peer.wire(), c_case.access, c_case.refresh);
    for wrong in [
        ExchangeAttempt {
            id: Uuid::from_u128(0xdead),
            generation: a_case.generation,
        },
        ExchangeAttempt {
            id: Uuid::from_u128(a_case.id),
            generation: 99,
        },
    ] {
        let idle_peer = BindingPeer::new("must_not_verify");
        let (keep, cancel) = oneshot::channel();
        let result = verify_at(
            a_case.input(),
            a_case.context_for(&wrong),
            cancel,
            Target::Loopback(idle_peer.address),
            local_limits(),
        )
        .await;
        match result {
            Err(error) => assert_eq!(error, AccountError::AttemptMismatch),
            Ok(_) => panic!("mismatched attempt must never yield a candidate"),
        }
        assert!(!idle_peer.accepted.load(Ordering::Acquire));
        assert!(idle_peer.request.lock().expect("capture").is_none());
        drop(keep);
    }
}

#[tokio::test]
async fn independent_binding_views_preserve_clock_bounds_and_borrows() {
    let case = BindingCase::a(Instant::now());
    let peer = BindingPeer::new("clock_binding");
    let candidate = verified(&case, &peer).await;
    let first = candidate.credential_binding();
    let expiry = case.started + Duration::from_secs(180);
    let expected = case.context();
    for (label, clock, usable, matching) in [
        (
            "before-validation",
            case.validated - Duration::from_nanos(1),
            false,
            false,
        ),
        (
            "at-validation historical comparison",
            case.validated,
            true,
            true,
        ),
        (
            "before-admission",
            case.admission - Duration::from_nanos(1),
            true,
            true,
        ),
        ("exact-admission", case.admission, true, false),
        (
            "before-expiry",
            expiry - Duration::from_nanos(1),
            true,
            false,
        ),
        ("exact-expiry", expiry, false, false),
        (
            "future-expired",
            expiry + Duration::from_secs(10),
            false,
            false,
        ),
    ] {
        assert_eq!(candidate.credentials_usable_at(clock), usable, "{label}");
        assert_eq!(
            candidate.matches_native_attempt(&expected, clock),
            matching,
            "{label}"
        );
        let again = candidate.credential_binding();
        assert!(std::ptr::eq(first.account_key(), candidate.account_key()));
        assert!(std::ptr::eq(first.account_key(), again.account_key()));
        assert!(std::ptr::eq(
            first.requested_scopes(),
            again.requested_scopes()
        ));
        assert!(std::ptr::eq(
            first.granted_scopes(),
            candidate.granted_scopes()
        ));
        assert!(std::ptr::eq(first.granted_scopes(), again.granted_scopes()));
        assert!(std::ptr::eq(
            first.registration_id(),
            again.registration_id()
        ));
        assert!(std::ptr::eq(first.client_id(), again.client_id()));
        assert_eq!(again.attempt_id(), Uuid::from_u128(0x8101));
        assert_eq!(again.generation(), 41);
        assert_eq!(again.admission_deadline(), case.admission);
        assert_eq!(again.credential_expires_at(), expiry);
        assert_eq!(candidate.credential_expires_at(), expiry);
        assert!(again.refresh_token_present());
    }
    assert_account_request(&peer.wire(), case.access, case.refresh);
}
