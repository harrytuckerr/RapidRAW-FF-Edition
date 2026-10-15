//! Independent account parser/transport/binding tests. Synthetic localhost only.
//! The production Google entry is never called; no parser constructs proof.
use super::super::token_response::parse_google_token_response;
use super::*;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const _: () = {
    assert!(BODY_LIMIT == 65_536);
    assert!(HEADER_LIMIT == 16_384);
    assert!(HEADER_FIELDS == 64);
    assert!(DEPTH_LIMIT == 16);
    assert!(WORK_LIMIT == 4096);
    assert!(MAP_LIMIT == 32);
    assert!(KEY_LIMIT == 128);
};
const CLIENT_ID: &str = "independent-account.apps.googleusercontent.com";
const CANARY_REFRESH: &str = "INDEPENDENT_REFRESH_MUST_NOT_LEAVE";
const ABOUT: &[u8] = br#"{"user":{"permissionId":"independent_permission"}}"#;
type Schedule = Vec<(Duration, Vec<u8>)>;
type Fields = Vec<(String, String)>;

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

fn fixture_input(
    token: &str,
    scope: ApprovedScope,
    issued: Instant,
    seconds: u64,
    id: u128,
) -> OwnedInput {
    let wire = serde_json::to_vec(&serde_json::json!({
        "access_token": token,
        "refresh_token": CANARY_REFRESH,
        "token_type": "Bearer",
        "expires_in": seconds,
        "scope": scope.as_str(),
    }))
    .expect("synthetic token JSON");
    let tokens = parse_google_token_response(
        200,
        &wire,
        &[ApprovedScope::File, ApprovedScope::AppData],
        issued,
        issued,
    )
    .expect("pure token parser, no network or exchange constructor");
    OwnedInput::Fixture {
        attempt: ExchangeAttempt {
            id: uuid::Uuid::from_u128(id),
            generation: 23,
        },
        tokens,
    }
}
fn fresh_input() -> OwnedInput {
    fixture_input(
        "INDEPENDENT_ACCESS_A",
        ApprovedScope::File,
        Instant::now(),
        120,
        41,
    )
}
fn binding(
    input: &OwnedInput,
    requested: &[ApprovedScope],
    deadline: Instant,
) -> VerificationContext {
    VerificationContext::from_native_attempt(
        "independent-native",
        CLIENT_ID,
        "18446744073709551615",
        requested,
        input.attempt(),
        deadline,
    )
    .expect("native configuration syntax, NOT mapping proof")
}
fn limits() -> Limits {
    Limits {
        total: Duration::from_secs(2),
        connect: Duration::from_millis(500),
        read: Duration::from_millis(500),
    }
}
fn failure<T>(result: Result<T, AccountError>) -> AccountError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("fixed expectation requires failure"),
    }
}
fn wire_response(status: u16, fields: &[(String, String)], body: &[u8]) -> Vec<u8> {
    let mut wire = format!("HTTP/1.1 {status} Independent\r\n").into_bytes();
    for (name, value) in fields {
        wire.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    wire.extend_from_slice(b"\r\n");
    wire.extend_from_slice(body);
    wire
}
fn fixed_fields(body: &[u8]) -> Fields {
    vec![
        ("Content-Type".into(), "application/json".into()),
        ("Content-Length".into(), body.len().to_string()),
    ]
}
fn success_wire(body: &[u8]) -> Vec<u8> {
    wire_response(200, &fixed_fields(body), body)
}
#[derive(Clone, Copy)]
enum Framing {
    Fixed,
    Chunked,
    Close,
}
fn framed(body: &[u8], framing: Framing) -> Vec<u8> {
    match framing {
        Framing::Fixed => success_wire(body),
        Framing::Close => wire_response(
            200,
            &[("Content-Type".into(), "application/json".into())],
            body,
        ),
        Framing::Chunked => {
            let mut wire = wire_response(
                200,
                &[
                    ("Content-Type".into(), "application/json".into()),
                    ("Transfer-Encoding".into(), "chunked".into()),
                ],
                &[],
            );
            for part in body.chunks(2048) {
                wire.extend_from_slice(format!("{:X}\r\n", part.len()).as_bytes());
                wire.extend_from_slice(part);
                wire.extend_from_slice(b"\r\n");
            }
            wire.extend_from_slice(b"0\r\n\r\n");
            wire
        }
    }
}
async fn execute(peer: &PeerHarness) -> Result<VerifiedAccountCandidate, AccountError> {
    let input = fresh_input();
    let context = binding(
        &input,
        &[ApprovedScope::AppData, ApprovedScope::File],
        Instant::now() + Duration::from_secs(5),
    );
    let (keep, cancel) = oneshot::channel();
    let result = verify_at(
        input,
        context,
        cancel,
        Target::Loopback(peer.address),
        limits(),
    )
    .await;
    drop(keep);
    result
}
fn padded_about(bytes: usize) -> Vec<u8> {
    assert!(bytes >= ABOUT.len());
    let mut body = ABOUT.to_vec();
    body.resize(bytes, b' ');
    body
}
fn assert_redacted(error: AccountError) {
    let shown = format!("{error:?} {error}");
    for canary in [
        "INDEPENDENT_ACCESS_A",
        CANARY_REFRESH,
        "independent_permission",
        "private@example.invalid",
        "grant=INDEPENDENT",
        "photo-canary",
    ] {
        assert!(
            !shown.contains(canary),
            "typed error must not reflect task canaries"
        );
    }
}

#[tokio::test]
async fn independent_exact_get_sensitive_bearer_partial_scopes_and_no_incidental_fetch() {
    let forbidden_photo = PeerHarness::reply(success_wire(ABOUT));
    let body = serde_json::to_vec(&serde_json::json!({
        "user": {"permissionId":"owned_A", "photoLink":format!("http://{}/photo-canary",forbidden_photo.address)},
        "id_token":"INDEPENDENT_NOT_IDENTITY", "extension":[true,null,{"nested":"ignored"}]
    })).expect("fixed response fixture");
    let peer = PeerHarness::reply(success_wire(&body));
    let candidate = execute(&peer)
        .await
        .expect("actual local verification path");
    assert_eq!(
        candidate.account_key().project_number.value(),
        18_446_744_073_709_551_615
    );
    assert_eq!(
        candidate.account_key().drive_permission_id.as_str(),
        "owned_A"
    );
    assert_eq!(candidate.granted_scopes(), &[ApprovedScope::File]);
    assert_eq!(
        candidate.missing_requested_scopes(),
        &[ApprovedScope::AppData]
    );
    assert!(candidate.display().email_address().is_none());
    assert!(candidate.display().quota().is_none());
    let captures = peer.requests();
    assert_eq!(captures.len(), 1);
    let wire = std::str::from_utf8(&captures[0]).expect("synthetic GET capture");
    let (head, body) = wire.split_once("\r\n\r\n").expect("complete headers");
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
    let url =
        Url::parse(&format!("http://127.0.0.1{}", request[1])).expect("relative request target");
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
    assert_eq!(auth, vec!["Bearer INDEPENDENT_ACCESS_A"]);
    for (name, value) in &headers {
        if !name.eq_ignore_ascii_case("authorization") {
            assert!(!value.contains("INDEPENDENT_ACCESS_A"));
        }
        assert!(!name.eq_ignore_ascii_case("cookie"));
        assert!(!name.eq_ignore_ascii_case("referer"));
        assert!(!name.eq_ignore_ascii_case("proxy-authorization"));
    }
    assert!(!request[1].contains("INDEPENDENT_ACCESS_A"));
    assert!(!wire.contains(CANARY_REFRESH));
    let header = bearer_header("SYNTHETIC-._~+/19==").expect("RFC6750 opaque header");
    assert!(header.is_sensitive());
    assert_eq!(header.as_bytes(), b"Bearer SYNTHETIC-._~+/19==");
    let request = Client::new()
        .get(format!("http://{}/", peer.address))
        .header(AUTHORIZATION, header)
        .build()
        .expect("native request build only; never sent");
    assert!(
        request
            .headers()
            .get(AUTHORIZATION)
            .expect("one auth")
            .is_sensitive()
    );
    assert_eq!(forbidden_photo.count(), 0);
    assert_eq!(peer.count(), 1);
}

#[tokio::test]
async fn independent_candidates_keep_owned_credentials_and_exact_native_binding() {
    for (token, scope, id, permission, missing) in [
        (
            "INDEPENDENT_ACCESS_A",
            ApprovedScope::File,
            41,
            "permission_A",
            ApprovedScope::AppData,
        ),
        (
            "INDEPENDENT_ACCESS_B",
            ApprovedScope::AppData,
            42,
            "permission_B",
            ApprovedScope::File,
        ),
    ] {
        let response = format!(r#"{{"user":{{"permissionId":"{permission}"}}}}"#);
        let peer = PeerHarness::reply(success_wire(response.as_bytes()));
        let input = fixture_input(token, scope, Instant::now(), 120, id);
        let deadline = Instant::now() + Duration::from_secs(5);
        let context = binding(
            &input,
            &[ApprovedScope::AppData, ApprovedScope::File],
            deadline,
        );
        let expected = binding(
            &input,
            &[ApprovedScope::File, ApprovedScope::AppData],
            deadline,
        );
        let (keep, rx) = oneshot::channel();
        let candidate = verify_at(input, context, rx, Target::Loopback(peer.address), limits())
            .await
            .expect("real transport candidate");
        assert_eq!(
            candidate.input.tokens().access_token_for_native_adapter(),
            token
        );
        assert_eq!(
            candidate.account_key().drive_permission_id.as_str(),
            permission
        );
        assert_eq!(candidate.granted_scopes(), &[scope]);
        assert_eq!(candidate.missing_requested_scopes(), &[missing]);
        assert!(candidate.matches_native_attempt(&expected, Instant::now()));
        assert!(!candidate.matches_native_attempt(&expected, deadline));
        for changed in 0..7 {
            let mut other = binding(
                &candidate.input,
                &[ApprovedScope::File, ApprovedScope::AppData],
                deadline,
            );
            match changed {
                0 => other.attempt.id = uuid::Uuid::from_u128(99),
                1 => other.attempt.generation += 1,
                2 => other.project = UnsignedDecimal::new(123),
                3 => other.registration_id = "different-native-registration".into(),
                4 => other.client_id = "other.apps.googleusercontent.com".into(),
                5 => other.admission_deadline += Duration::from_nanos(1),
                6 => other.requested = vec![scope],
                _ => unreachable!(),
            }
            assert!(!candidate.matches_native_attempt(&other, Instant::now()));
        }
        let requests = peer.requests();
        assert_eq!(requests.len(), 1);
        assert!(
            std::str::from_utf8(&requests[0])
                .expect("wire")
                .contains(&format!("Bearer {token}\r\n"))
        );
        drop(keep);
    }
    // The fixture's token/permission mapping is fixed input, not an authenticity
    // oracle for a dishonest provider. Trusted client/project provenance is S3.
    let peer = PeerHarness::reply(success_wire(ABOUT));
    for changed in 0..3 {
        let input = fresh_input();
        let mut context = binding(
            &input,
            &[ApprovedScope::File],
            Instant::now() + Duration::from_secs(5),
        );
        let want = if changed == 2 {
            AccountError::InvalidGrantedScopes
        } else {
            AccountError::AttemptMismatch
        };
        match changed {
            0 => context.attempt.id = uuid::Uuid::from_u128(99),
            1 => context.attempt.generation += 1,
            2 => context.requested = vec![ApprovedScope::AppData],
            _ => unreachable!(),
        }
        let (keep, rx) = oneshot::channel();
        let error =
            failure(verify_at(input, context, rx, Target::Loopback(peer.address), limits()).await);
        assert_eq!(error, want);
        drop(keep);
    }
    assert_eq!(peer.count(), 0);
}

#[test]
fn independent_parser_optional_metadata_integer_and_type_known_answers() {
    for body in [
        r#"{"user":{"permissionId":"id_9-A"}}"#,
        r#"{"user":{"permissionId":"id_9-A","displayName":null,"emailAddress":"   "},"storageQuota":null}"#,
    ] {
        let data = parse_about(body.as_bytes()).expect("known optional metadata");
        assert_eq!(data.permission_id.as_str(), "id_9-A");
        assert!(data.display.name.is_none());
        assert!(data.display.email.is_none());
        assert!(data.display.quota.is_none());
    }
    let data=parse_about(br#"{"user":{"permissionId":"quota_id","displayName":"Example Person","emailAddress":"private@example.invalid"},"storageQuota":{"limit":"9223372036854775807","usage":"0","usageInDrive":"9007199254740993","usageInDriveTrash":"42"}}"#).expect("exact integer answers");
    assert_eq!(data.display.display_name(), Some("Example Person"));
    assert_eq!(
        data.display.email_address(),
        Some("private@example.invalid")
    );
    let quota = data.display.quota().expect("present quota");
    assert_eq!(quota.limit(), Some(9_223_372_036_854_775_807));
    assert_eq!(quota.usage(), Some(0));
    assert_eq!(quota.usage_in_drive(), Some(9_007_199_254_740_993));
    assert_eq!(quota.usage_in_drive_trash(), Some(42));
    let data = parse_about(br#"{"user":{"permissionId":"quota_id"},"storageQuota":{}}"#)
        .expect("present no limit");
    assert_eq!(data.display.quota().expect("known object").limit(), None);
    for (body, want) in [
        (r#"{"user":null}"#, AccountError::MissingUser),
        (r#"{"user":true}"#, AccountError::MalformedResponse),
        (
            r#"{"user":{"permissionId":null}}"#,
            AccountError::MissingPermissionId,
        ),
        (
            r#"{"user":{"permissionId":[]}}"#,
            AccountError::InvalidPermissionId,
        ),
        (
            r#"{"user":{"permissionId":"A.B"}}"#,
            AccountError::InvalidPermissionId,
        ),
        (
            r#"{"user":{"permissionId":"p","displayName":1}}"#,
            AccountError::InvalidDisplayMetadata,
        ),
        (
            r#"{"user":{"permissionId":"p","emailAddress":"\u0000"}}"#,
            AccountError::InvalidDisplayMetadata,
        ),
        (
            r#"{"user":{"permissionId":"p"},"storageQuota":[]}"#,
            AccountError::InvalidQuota,
        ),
        (
            r#"{"user":{"permissionId":"p"},"storageQuota":{"usage":0}}"#,
            AccountError::InvalidQuota,
        ),
        (
            r#"{"user":{"permissionId":"p"},"storageQuota":{"usage":"00"}}"#,
            AccountError::InvalidQuota,
        ),
        (
            r#"{"user":{"permissionId":"p"},"storageQuota":{"limit":"9223372036854775808"}}"#,
            AccountError::InvalidQuota,
        ),
        (
            r#"{"user":{"permissionId":"p"},"error":null}"#,
            AccountError::ProviderErrorPayload,
        ),
    ] {
        assert_eq!(failure(parse_about(body.as_bytes())), want);
    }
    for project in ["18446744073709551616", "0001", "0"] {
        let input = fresh_input();
        assert_eq!(
            failure(VerificationContext::from_native_attempt(
                "r",
                CLIENT_ID,
                project,
                &[ApprovedScope::File],
                input.attempt(),
                Instant::now() + Duration::from_secs(5)
            )),
            AccountError::InvalidRegistration(RegistrationField::ProjectNumber)
        );
    }
}

#[test]
fn independent_parser_decoded_duplicates_utf8_and_positive_metadata_edges() {
    for body in [
        r#"{"user":{"permissionId":"a"},"u\u0073er":{"permissionId":"b"}}"#,
        r#"{"user":{"permissionId":"a","permission\u0049d":"b"}}"#,
        r#"{"user":{"permissionId":"a"},"storageQuota":{"usage":"1","us\u0061ge":"2"}}"#,
        r#"{"user":{"permissionId":"a"},"extra":0,"extr\u0061":1}"#,
    ] {
        assert_eq!(
            failure(parse_about(body.as_bytes())),
            AccountError::MalformedResponse
        );
    }
    assert!(
        parse_about(
            br#"{"user":{"permissionId":"a"},"extra":{"x":1,"x":2},"ignoredNumber":1e999}"#
        )
        .is_ok()
    );
    assert_eq!(
        failure(parse_about(
            b"{\"user\":{\"permissionId\":\"a\"},\"ignored\":\"\xff\"}"
        )),
        AccountError::MalformedResponse
    );
    for invalid in [b"{} {}".as_slice(), b"[]", b"{\"user\":NaN}"] {
        assert_eq!(
            failure(parse_about(invalid)),
            AccountError::MalformedResponse
        );
    }
    let good = serde_json::to_vec(&serde_json::json!({"user":{"permissionId":"p".repeat(256),
        "displayName":"🦀".repeat(256),"emailAddress":"e".repeat(320)}}))
    .expect("positive metadata boundaries");
    assert!(parse_about(&good).is_ok());
    for (field, value) in [
        ("permissionId", "p".repeat(257)),
        ("displayName", "🦀".repeat(257)),
        ("displayName", " ".repeat(257)),
        ("emailAddress", " ".repeat(321)),
    ] {
        let mut user = serde_json::Map::new();
        user.insert("permissionId".into(), serde_json::Value::String("p".into()));
        user.insert(field.into(), serde_json::Value::String(value));
        let body = serde_json::to_vec(&serde_json::json!({"user":user}))
            .expect("negative metadata boundary");
        assert_eq!(
            failure(parse_about(&body)),
            if field == "permissionId" {
                AccountError::InvalidPermissionId
            } else {
                AccountError::InvalidDisplayMetadata
            }
        );
    }
}

#[test]
fn independent_parser_exact_body_depth_work_map_and_decoded_key_limits() {
    assert!(parse_about(&padded_about(65_536)).is_ok());
    assert_eq!(
        failure(parse_about(&padded_about(65_537))),
        AccountError::BodyLimit
    );
    for (arrays, want) in [
        (15, None),
        (16, Some(AccountError::ParseLimit(ParseLimit::Depth))),
    ] {
        let body = format!(
            r#"{{"user":{{"permissionId":"p"}},"extension":{}0{}}}"#,
            "[".repeat(arrays),
            "]".repeat(arrays)
        );
        match want {
            None => assert!(parse_about(body.as_bytes()).is_ok()),
            Some(error) => assert_eq!(failure(parse_about(body.as_bytes())), error),
        }
    }
    // Independently counted work: root/user/array openings=3, their three
    // colons plus root comma=4, and N-1 array commas => N+6 punctuation units.
    for (items, want) in [
        (4090, None),
        (4091, Some(AccountError::ParseLimit(ParseLimit::Work))),
    ] {
        let body = format!(
            r#"{{"user":{{"permissionId":"p"}},"extension":[{}]}}"#,
            vec!["0"; items].join(",")
        );
        match want {
            None => assert!(parse_about(body.as_bytes()).is_ok()),
            Some(error) => assert_eq!(failure(parse_about(body.as_bytes())), error),
        }
    }
    for place in 0..3 {
        for extras in [31, 32] {
            let fields = (0..extras)
                .map(|i| format!("\"k{i}\":0"))
                .collect::<Vec<_>>()
                .join(",");
            let body = match place {
                0 => format!(r#"{{"user":{{"permissionId":"p"}},{fields}}}"#),
                1 => format!(r#"{{"user":{{"permissionId":"p",{fields}}}}}"#),
                2 => format!(
                    r#"{{"user":{{"permissionId":"p"}},"storageQuota":{{"usage":"0",{fields}}}}}"#
                ),
                _ => unreachable!(),
            };
            if extras == 31 {
                assert!(parse_about(body.as_bytes()).is_ok());
            } else {
                assert_eq!(
                    failure(parse_about(body.as_bytes())),
                    AccountError::ParseLimit(ParseLimit::Fields)
                );
            }
        }
    }
    for (key, want) in [
        ("é".repeat(64), None),
        (
            "é".repeat(65),
            Some(AccountError::ParseLimit(ParseLimit::Key)),
        ),
    ] {
        let body = format!(
            r#"{{"user":{{"permissionId":"p"}},{}:0}}"#,
            serde_json::to_string(&key).expect("escaped key")
        )
        .into_bytes();
        match want {
            None => assert!(parse_about(&body).is_ok()),
            Some(error) => assert_eq!(failure(parse_about(&body)), error),
        }
    }
}

#[tokio::test]
async fn independent_actual_header_count_and_aggregate_bytes_accept_exact_edges() {
    for (extra, want) in [(62, None), (63, Some(AccountError::HeaderLimit))] {
        let mut fields = fixed_fields(ABOUT);
        for i in 0..extra {
            fields.push((format!("x-independent-{i}"), "v".into()));
        }
        let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
        let result = execute(&peer).await;
        match want {
            None => assert!(result.is_ok()),
            Some(error) => assert_eq!(failure(result), error),
        }
        assert_eq!(peer.count(), 1);
    }
    for (budget, want) in [(16_384, None), (16_385, Some(AccountError::HeaderLimit))] {
        let mut fields = fixed_fields(ABOUT);
        let base: usize = fields
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum();
        fields.push(("x-pad".into(), "z".repeat(budget - base - 5)));
        assert_eq!(
            fields
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>(),
            budget
        );
        let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
        let result = execute(&peer).await;
        match want {
            None => assert!(result.is_ok()),
            Some(error) => assert_eq!(failure(result), error),
        }
    }
}

#[tokio::test]
async fn independent_all_body_framings_enforce_exact_cap_and_truncation() {
    for framing in [Framing::Fixed, Framing::Chunked, Framing::Close] {
        let peer = PeerHarness::reply(framed(&padded_about(65_536), framing));
        let candidate = execute(&peer)
            .await
            .expect("exact body cap over actual transport");
        assert_eq!(
            candidate.account_key().drive_permission_id.as_str(),
            "independent_permission"
        );
        let oversized = PeerHarness::reply(framed(&padded_about(65_537), framing));
        assert_eq!(failure(execute(&oversized).await), AccountError::BodyLimit);
    }
    for wire in [
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 99\r\n\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n20\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\nZ\r\n{}\r\n0\r\n\r\n".to_vec(),
    ] {
        let peer=PeerHarness::reply(wire);assert_eq!(failure(execute(&peer).await),AccountError::Transport);
    }
    let malformed = PeerHarness::reply(success_wire(
        br#"{"user":{"permissionId":"a","permissionId":"b"}}"#,
    ));
    assert_eq!(
        failure(execute(&malformed).await),
        AccountError::MalformedResponse
    );
}

#[tokio::test]
async fn independent_actual_content_policy_status_redaction_redirect_and_no_replay() {
    for (extra, want) in [
        (
            ("Content-Type", "application/json"),
            AccountError::InvalidHeaders,
        ),
        (("Content-Encoding", "gzip"), AccountError::ContentEncoding),
        (("Content-Encoding", "br"), AccountError::ContentEncoding),
        (
            ("Content-Encoding", "identity,gzip"),
            AccountError::ContentEncoding,
        ),
    ] {
        let mut fields = fixed_fields(ABOUT);
        fields.push((extra.0.into(), extra.1.into()));
        let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
        assert_eq!(failure(execute(&peer).await), want);
    }
    let fields = vec![("Content-Length".into(), ABOUT.len().to_string())];
    let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
    assert_eq!(failure(execute(&peer).await), AccountError::InvalidHeaders);
    let mut fields = fixed_fields(ABOUT);
    fields[0].1 = "Application/JSON; Charset=\"UTF-8\"".into();
    fields.push(("Content-Encoding".into(), "identity".into()));
    let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
    assert!(execute(&peer).await.is_ok());
    fields.push(("Content-Encoding".into(), "identity".into()));
    let peer = PeerHarness::reply(wire_response(200, &fields, ABOUT));
    assert_eq!(failure(execute(&peer).await), AccountError::InvalidHeaders);
    let canaries=b"INDEPENDENT_ACCESS_A INDEPENDENT_REFRESH_MUST_NOT_LEAVE independent_permission private@example.invalid http://example.invalid/?grant=INDEPENDENT photo-canary";
    for (code, want) in [
        (401, AccountError::Unauthorized),
        (403, AccountError::Forbidden),
        (429, AccountError::RateLimited),
        (503, AccountError::Server),
        (418, AccountError::UnexpectedStatus),
    ] {
        let peer = PeerHarness::reply(wire_response(code, &fixed_fields(canaries), canaries));
        let error = failure(execute(&peer).await);
        assert_eq!(error, want);
        assert_redacted(error);
    }
    let peer = PeerHarness::reply(success_wire(canaries));
    let error = failure(execute(&peer).await);
    assert_eq!(error, AccountError::MalformedResponse);
    assert_redacted(error);
    let target = PeerHarness::reply(success_wire(ABOUT));
    for code in [307, 308] {
        let fields = vec![
            (
                "Location".into(),
                format!("http://{}/photo-canary?grant=INDEPENDENT", target.address),
            ),
            ("Content-Length".into(), "0".into()),
        ];
        let peer = PeerHarness::reply(wire_response(code, &fields, &[]));
        let error = failure(execute(&peer).await);
        assert_eq!(error, AccountError::Redirect);
        assert_redacted(error);
        assert_eq!(peer.count(), 1);
    }
    assert_eq!(target.count(), 0);
    let closed = PeerHarness::schedule(vec![]);
    assert_eq!(failure(execute(&closed).await), AccountError::Transport);
    assert_eq!(closed.count(), 1);
    assert_eq!(closed.requests().len(), 1);
}

#[tokio::test]
async fn independent_pre_and_midstream_cancellation_signal_or_drop_never_return_candidate() {
    let peer = PeerHarness::reply(success_wire(ABOUT));
    for signal in [false, true] {
        let input = fresh_input();
        let context = binding(
            &input,
            &[ApprovedScope::File],
            Instant::now() + Duration::from_secs(5),
        );
        let (tx, rx) = oneshot::channel();
        if signal {
            tx.send(()).expect("pre signal");
        } else {
            drop(tx);
        }
        assert_eq!(
            failure(verify_at(input, context, rx, Target::Loopback(peer.address), limits()).await),
            AccountError::Cancelled
        );
    }
    assert_eq!(peer.count(), 0);
    for signal in [false, true] {
        let peer=PeerHarness::schedule(vec![
            (Duration::ZERO,b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n\r\n{".to_vec()),
            (Duration::from_secs(1),b" ".to_vec()),
        ]);
        let input = fresh_input();
        let context = binding(
            &input,
            &[ApprovedScope::File],
            Instant::now() + Duration::from_secs(5),
        );
        let (tx, rx) = oneshot::channel();
        let operation = verify_at(input, context, rx, Target::Loopback(peer.address), limits());
        tokio::pin!(operation);
        tokio::select! {
            result=&mut operation=>{let _=result;panic!("incomplete streamed response cannot finish before cancellation");},
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

#[tokio::test]
async fn independent_admission_and_token_expiry_bound_continuous_body_progress() {
    for token_bound in [false, true] {
        let headers=b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        let mut plan = vec![(Duration::ZERO, headers)];
        for _ in 0..30 {
            plan.push((Duration::from_millis(20), b"1\r\n \r\n".to_vec()));
        }
        plan.push((Duration::ZERO, b"0\r\n\r\n".to_vec()));
        let peer = PeerHarness::schedule(plan);
        let input = if token_bound {
            fixture_input(
                "INDEPENDENT_SHORT",
                ApprovedScope::File,
                Instant::now() - Duration::from_millis(800),
                1,
                43,
            )
        } else {
            fresh_input()
        };
        let context = binding(
            &input,
            &[ApprovedScope::File],
            Instant::now()
                + if token_bound {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(160)
                },
        );
        let (keep, rx) = oneshot::channel();
        let began = Instant::now();
        let error =
            failure(verify_at(input, context, rx, Target::Loopback(peer.address), limits()).await);
        assert_eq!(
            error,
            if token_bound {
                AccountError::TokenExpired
            } else {
                AccountError::Deadline
            }
        );
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "progress cannot renew absolute lifetime"
        );
        assert_eq!(peer.count(), 1);
        assert!(peer.sent.load(Ordering::Acquire) > 1);
        eprintln!(
            "independent account absolute bound observed: token_bound={token_bound}, elapsed_ms={}",
            began.elapsed().as_millis()
        );
        drop(keep);
    }
}

#[test]
fn independent_final_gate_and_header_credential_policy_reject_without_constructing_proof() {
    assert!(bearer_header(&"A".repeat(8192)).is_ok());
    for token in [
        "".to_owned(),
        "=".into(),
        "A=B".into(),
        "bad!token".into(),
        "A".repeat(8193),
    ] {
        assert_eq!(
            failure(bearer_header(&token)),
            AccountError::InvalidBearerCredential
        );
    }
    for mode in 0..4 {
        let input = if mode == 3 {
            fixture_input(
                "INDEPENDENT_EXPIRED",
                ApprovedScope::File,
                Instant::now() - Duration::from_secs(2),
                1,
                44,
            )
        } else {
            fresh_input()
        };
        let context = binding(
            &input,
            &[ApprovedScope::File],
            Instant::now() + Duration::from_secs(5),
        );
        let parsed = parse_about(ABOUT).expect("data only, never proof");
        let (tx, mut rx) = oneshot::channel();
        if mode == 0 {
            drop(tx);
        } else if mode == 1 {
            tx.send(()).expect("final cancellation");
        } else {
            let deadline = if mode == 2 {
                Instant::now()
            } else {
                Instant::now() + Duration::from_secs(1)
            };
            let want = if mode == 2 {
                AccountError::Deadline
            } else {
                AccountError::TokenExpired
            };
            assert_eq!(
                failure(finish(input, context, parsed, &mut rx, deadline)),
                want
            );
            drop(tx);
            continue;
        }
        assert_eq!(
            failure(finish(
                input,
                context,
                parsed,
                &mut rx,
                Instant::now() + Duration::from_secs(1)
            )),
            AccountError::Cancelled
        );
    }
}
