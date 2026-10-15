//! Independent localhost transport probes. Synthetic inputs, no provider/TLS I/O.
use super::super::pkce::{ApprovedScope, CallbackOutcome, PkceTransaction};
use super::super::token_response::{HttpFailure, ProviderFailure};
use super::*;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const _: () = {
    assert!(MAX_HEADER_FIELDS == 64);
    assert!(MAX_HEADER_BYTES == 16_384);
    assert!(MAX_RESPONSE_BYTES == 65_536);
    assert!(MAX_FORM_BYTES == 8192);
};
const CLIENT: &str = "independent-exchange.apps.googleusercontent.com";
const CODE: &str = "REVIEW+/%&=?~code";
const APPDATA: &str = "https://www.googleapis.com/auth/drive.appdata";
const FILE: &str = "https://www.googleapis.com/auth/drive.file";
const ACCESS: &str = "SYNTHETIC_INDEPENDENT_ACCESS";
const REFRESH: &str = "SYNTHETIC_INDEPENDENT_REFRESH";
type Schedule = Vec<(Duration, Vec<u8>)>;
type Fields = Vec<(String, String)>;

// Own nonblocking fixture, not the author's Server. At most four connections,
//16KiB captured per request,256KiB scheduled output,4s lifetime and joined Drop.
struct PeerHarness {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
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
        if lengths.len() != 1 {
            return None;
        }
        let length = lengths[0].1.trim().parse::<usize>().ok()?;
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
        assert!(plan.iter().map(|(_, b)| b.len()).sum::<usize>() <= 256 * 1024);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let captured = Arc::new(Mutex::new(Vec::new()));
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
            worker: Some(worker),
        }
    }
    fn count(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }
    fn requests(&self) -> Vec<Vec<u8>> {
        self.captured.lock().unwrap().clone()
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

fn code_grant(scopes: &[&str]) -> AuthorizationCodeGrant {
    let now = Instant::now();
    let mut transaction =
        PkceTransaction::begin(CLIENT, 54123, scopes, now, Duration::from_secs(60)).unwrap();
    let authorization =
        Url::parse(transaction.authorization_url_for_browser(now).unwrap()).unwrap();
    let state = authorization
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let mut callback = Url::parse(transaction.redirect_uri()).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("state", &state)
        .append_pair("code", CODE);
    let CallbackOutcome::Code(grant) = transaction.accept_callback(callback.as_str(), now).unwrap()
    else {
        panic!("synthetic grant expected")
    };
    grant
}
fn attempt_metadata() -> ExchangeAttempt {
    ExchangeAttempt {
        id: Uuid::from_u128(0x19abc),
        generation: 29,
    }
}
fn test_limits() -> Limits {
    Limits {
        total: Duration::from_secs(2),
        connect: Duration::from_millis(500),
        read: Duration::from_millis(500),
    }
}
fn token_body(scope: &str) -> Vec<u8> {
    format!(r#"{{"access_token":"{ACCESS}","token_type":"Bearer","expires_in":120,"scope":"{scope}","refresh_token":"{REFRESH}"}}"#).into_bytes()
}
fn raw_response(status: &str, fields: &[(String, String)], body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 {status}\r\n").into_bytes();
    for (name, value) in fields {
        bytes.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    bytes.extend_from_slice(b"\r\n");
    bytes.extend_from_slice(body);
    bytes
}
fn fixed_fields(body: &[u8]) -> Fields {
    vec![
        ("content-type".into(), "application/json".into()),
        ("content-length".into(), body.len().to_string()),
    ]
}
fn fixed_response(body: &[u8]) -> Vec<u8> {
    raw_response("200 OK", &fixed_fields(body), body)
}
#[derive(Clone, Copy)]
enum Framing {
    Fixed,
    Chunked,
    Close,
}
fn framed_response(body: &[u8], framing: Framing) -> Vec<u8> {
    match framing {
        Framing::Fixed => fixed_response(body),
        Framing::Close => raw_response(
            "200 OK",
            &[("content-type".into(), "application/json".into())],
            body,
        ),
        Framing::Chunked => {
            let mut encoded = Vec::new();
            for part in body.chunks(997) {
                encoded.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
                encoded.extend_from_slice(part);
                encoded.extend_from_slice(b"\r\n");
            }
            encoded.extend_from_slice(b"0\r\n\r\n");
            raw_response(
                "200 OK",
                &[
                    ("content-type".into(), "application/json".into()),
                    ("transfer-encoding".into(), "chunked".into()),
                ],
                &encoded,
            )
        }
    }
}
async fn execute(server: &PeerHarness) -> Result<ExchangeResult, ExchangeError> {
    let (hold, receiver) = oneshot::channel();
    let result = exchange_at(
        code_grant(&[APPDATA, FILE]),
        attempt_metadata(),
        receiver,
        Target::Loopback(server.address),
        test_limits(),
    )
    .await;
    drop(hold);
    result
}
fn exact_tokens(result: &ExchangeResult) {
    assert!(result.attempt().id == Uuid::from_u128(0x19abc));
    assert!(result.attempt().generation == 29);
    let tokens = result.tokens_for_native_controller();
    assert!(tokens.granted_scopes() == [ApprovedScope::File]);
    assert!(tokens.access_token_for_native_adapter() == ACCESS);
    assert!(tokens.refresh_token_for_native_adapter() == Some(REFRESH));
}

#[tokio::test]
async fn independent_post_has_exact_native_values_five_fields_and_partial_capability() {
    let server = PeerHarness::reply(fixed_response(&token_body(FILE)));
    let grant = code_grant(&[APPDATA, FILE]);
    let verifier = grant.verifier_for_exchange().to_owned();
    let (hold, receiver) = oneshot::channel();
    let result = exchange_at(
        grant,
        attempt_metadata(),
        receiver,
        Target::Loopback(server.address),
        test_limits(),
    )
    .await
    .unwrap();
    drop(hold);
    exact_tokens(&result);
    let requests = server.requests();
    assert!(requests.len() == 1 && server.count() == 1);
    let raw = std::str::from_utf8(&requests[0]).unwrap();
    let (headers, form) = raw.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /token HTTP/1.1\r\n"));
    let lower = headers.to_ascii_lowercase();
    assert!(!headers.contains(&verifier) && !headers.contains(CODE));
    for required in [
        "content-type: application/x-www-form-urlencoded",
        "accept: application/json",
        "accept-encoding: identity",
    ] {
        assert!(lower.lines().any(|line| line == required));
    }
    for forbidden in [
        "authorization:",
        "cookie:",
        "referer:",
        "dpop:",
        "review",
        CLIENT,
        "code_verifier",
        verifier.as_str(),
    ] {
        assert!(!lower.contains(forbidden));
    }
    let expected = format!(
        "grant_type=authorization_code&code=REVIEW%2B%2F%25%26%3D%3F%7Ecode&code_verifier={verifier}&client_id=independent-exchange.apps.googleusercontent.com&redirect_uri=http%3A%2F%2F127.0.0.1%3A54123%2Foauth2%2Fcallback%2Fgoogle"
    );
    assert!(form == expected);
    assert!(form.split('&').count() == 5);
    assert!(!form.contains("client_secret") && !form.contains("scope="));
}

#[tokio::test]
async fn independent_positive_header_count_and_byte_boundaries_cross_actual_transport() {
    let body = token_body(FILE);
    for (count, accept) in [(64, true), (65, false)] {
        let mut fields = fixed_fields(&body);
        fields.extend((0..count - 2).map(|i| (format!("x-review-{i}"), "v".to_owned())));
        assert!(fields.len() == count);
        let server = PeerHarness::reply(raw_response("200 OK", &fields, &body));
        let result = execute(&server).await;
        if accept {
            exact_tokens(&result.unwrap());
        } else {
            assert!(matches!(result, Err(ExchangeError::ResponseHeaders)));
        }
        assert!(server.count() == 1);
    }
    for (total, accept) in [(16_384usize, true), (16_385, false)] {
        let mut fields = fixed_fields(&body);
        let base: usize = fields.iter().map(|(n, v)| n.len() + v.len()).sum();
        fields.push(("x-pad".into(), "x".repeat(total - base - 5)));
        assert!(fields.iter().map(|(n, v)| n.len() + v.len()).sum::<usize>() == total);
        let server = PeerHarness::reply(raw_response("200 OK", &fields, &body));
        let result = execute(&server).await;
        if accept {
            exact_tokens(&result.unwrap());
        } else {
            assert!(matches!(result, Err(ExchangeError::ResponseHeaders)));
        }
        assert!(server.count() == 1);
    }
}

#[tokio::test]
async fn independent_all_body_framings_have_exact_cap_success_and_overflow_failure() {
    let mut exact = token_body(FILE);
    exact.resize(65_536, b' ');
    let mut excess = exact.clone();
    excess.push(b' ');
    for framing in [Framing::Fixed, Framing::Chunked, Framing::Close] {
        let server = PeerHarness::reply(framed_response(&exact, framing));
        exact_tokens(&execute(&server).await.unwrap());
        assert!(server.count() == 1);
        let server = PeerHarness::reply(framed_response(&excess, framing));
        assert!(matches!(
            execute(&server).await,
            Err(ExchangeError::BodyLimit)
        ));
        assert!(server.count() == 1);
    }
}

#[tokio::test]
async fn independent_truncated_framing_and_duplicate_json_never_accept_valid_prefix() {
    let body = token_body(FILE);
    let mut length = fixed_fields(&body);
    length[1].1 = (body.len() + 11).to_string();
    let short_fixed = raw_response("200 OK", &length, &body);
    let mut missing_terminal = format!("{:x}\r\n", body.len()).into_bytes();
    missing_terminal.extend_from_slice(&body);
    missing_terminal.extend_from_slice(b"\r\n");
    let chunk_headers = vec![
        ("content-type".into(), "application/json".into()),
        ("transfer-encoding".into(), "chunked".into()),
    ];
    for response in [
        short_fixed,
        raw_response("200 OK", &chunk_headers, &missing_terminal),
        raw_response("200 OK", &chunk_headers, b"ZZ\r\n{}\r\n0\r\n\r\n"),
    ] {
        let server = PeerHarness::reply(response);
        assert!(matches!(
            execute(&server).await,
            Err(ExchangeError::Transport)
        ));
        assert!(server.count() == 1 && server.requests().len() == 1);
    }
    let duplicate = String::from_utf8(body).unwrap().replace(
        "\"token_type\":",
        "\"access_token\":\"SYNTHETIC_SHADOW\",\"token_type\":",
    );
    let server = PeerHarness::reply(fixed_response(duplicate.as_bytes()));
    assert!(matches!(
        execute(&server).await,
        Err(ExchangeError::TokenResponse(
            TokenResponseError::MalformedResponse
        ))
    ));
}

#[tokio::test]
async fn independent_duplicate_metadata_and_content_encoding_are_not_normalized_away() {
    let body = token_body(FILE);
    let mut cases = Vec::new();
    for additions in [
        vec![("content-type", "application/json")],
        vec![
            ("content-encoding", "identity"),
            ("content-encoding", "identity"),
        ],
    ] {
        let mut fields = fixed_fields(&body);
        fields.extend(additions.into_iter().map(|(n, v)| (n.into(), v.into())));
        cases.push((fields, ExchangeError::ResponseHeaders));
    }
    let mut duplicate = fixed_fields(&body);
    duplicate.push(("content-length".into(), body.len().to_string()));
    cases.push((duplicate, ExchangeError::ResponseHeaders));
    for encoding in ["gzip", "br", "deflate", "zstd", "identity, identity"] {
        let mut fields = fixed_fields(&body);
        fields.push(("content-encoding".into(), encoding.into()));
        cases.push((fields, ExchangeError::ContentEncoding));
    }
    for kind in [
        "text/html",
        "application/json; charset=latin1",
        "application/json; charset=utf-8; charset=utf-8",
    ] {
        let mut fields = fixed_fields(&body);
        fields[0].1 = kind.into();
        cases.push((fields, ExchangeError::ResponseHeaders));
    }
    for (fields, expected) in cases {
        let server = PeerHarness::reply(raw_response("200 OK", &fields, &body));
        assert!(execute(&server).await.err() == Some(expected));
        assert!(server.count() == 1);
    }
    let mut positive = fixed_fields(&body);
    positive[0].1 = "Application/JSON; Charset = \"UTF-8\"".into();
    positive.push(("content-encoding".into(), "IdEnTiTy".into()));
    let server = PeerHarness::reply(raw_response("200 OK", &positive, &body));
    exact_tokens(&execute(&server).await.unwrap());
}

#[tokio::test]
async fn independent_redirect_capture_and_post_receipt_disconnect_cannot_replay() {
    let capture = PeerHarness::reply(fixed_response(&token_body(FILE)));
    let fields = vec![
        (
            "location".into(),
            format!(
                "http://{}/capture?SYNTHETIC_PRIVATE_REDIRECT",
                capture.address
            ),
        ),
        ("content-length".into(), "0".into()),
    ];
    let redirect = PeerHarness::reply(raw_response("307 Temporary Redirect", &fields, b""));
    let error = execute(&redirect).await.err().unwrap();
    assert_eq!(error, ExchangeError::Redirect);
    for text in [error.to_string(), format!("{error:?}")] {
        assert!(!text.contains("SYNTHETIC") && !text.contains("127.0.0.1"));
    }
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(redirect.count() == 1 && redirect.requests().len() == 1 && capture.count() == 0);
    let disconnect = PeerHarness::schedule(Vec::new());
    assert!(matches!(
        execute(&disconnect).await,
        Err(ExchangeError::Transport)
    ));
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(disconnect.count() == 1 && disconnect.requests().len() == 1);
}

#[tokio::test]
async fn independent_preclosed_and_midstream_cancellation_do_not_return_tokens() {
    for signal in [true, false] {
        let server = PeerHarness::reply(fixed_response(&token_body(FILE)));
        let (sender, receiver) = oneshot::channel();
        if signal {
            sender.send(()).unwrap();
        } else {
            drop(sender);
        }
        let result = exchange_at(
            code_grant(&[APPDATA, FILE]),
            attempt_metadata(),
            receiver,
            Target::Loopback(server.address),
            test_limits(),
        )
        .await;
        assert!(matches!(result, Err(ExchangeError::Cancelled)));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(server.count() == 0 && server.requests().is_empty());
    }
    for signal in [true, false] {
        let headers =
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 999\r\n\r\n{"
                .to_vec();
        let server = PeerHarness::schedule(vec![
            (Duration::ZERO, headers),
            (Duration::from_secs(2), b"unused".to_vec()),
        ]);
        let (sender, receiver) = oneshot::channel();
        let operation = exchange_at(
            code_grant(&[APPDATA, FILE]),
            attempt_metadata(),
            receiver,
            Target::Loopback(server.address),
            test_limits(),
        );
        let cancellation = async {
            server.received().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            if signal {
                let _ = sender.send(());
            } else {
                drop(sender);
            }
        };
        let start = Instant::now();
        let (result, ()) = tokio::join!(operation, cancellation);
        assert!(matches!(result, Err(ExchangeError::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(server.count() == 1 && server.requests().len() == 1);
    }
}

#[tokio::test]
async fn independent_continuous_body_progress_cannot_renew_absolute_deadline() {
    let headers =
        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 4096\r\n\r\n"
            .to_vec();
    let mut plan = vec![(Duration::ZERO, headers)];
    plan.extend((0..50).map(|_| (Duration::from_millis(25), b" ".to_vec())));
    let server = PeerHarness::schedule(plan);
    let grant = code_grant(&[APPDATA, FILE]);
    let (hold, receiver) = oneshot::channel();
    let constrained = Limits {
        total: Duration::from_millis(300),
        connect: Duration::from_millis(150),
        read: Duration::from_millis(150),
    };
    let start = Instant::now();
    let result = exchange_at(
        grant,
        attempt_metadata(),
        receiver,
        Target::Loopback(server.address),
        constrained,
    )
    .await;
    drop(hold);
    assert!(matches!(result, Err(ExchangeError::Deadline)));
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(240));
    assert!(elapsed < Duration::from_secs(1));
    assert!(server.count() == 1 && server.requests().len() == 1);
    eprintln!(
        "independent exchange absolute deadline observed in {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn independent_status_scope_and_redaction_use_actual_response_parser() {
    let error_body=br#"{"error":"invalid_grant","error_description":"SYNTHETIC_PRIVATE_DETAIL","error_uri":"https://synthetic.invalid/SYNTHETIC_URI","id_token":"SYNTHETIC_ID","account":"SYNTHETIC_ACCOUNT"}"#;
    let provider = PeerHarness::reply(raw_response(
        "400 Bad Request",
        &fixed_fields(error_body),
        error_body,
    ));
    let provider_error = execute(&provider).await.err().unwrap();
    assert_eq!(
        provider_error,
        ExchangeError::TokenResponse(TokenResponseError::Provider {
            http: HttpFailure::BadRequest,
            error: ProviderFailure::InvalidGrant
        })
    );
    let forbidden_body = token_body(FILE);
    let forbidden = PeerHarness::reply(raw_response(
        "403 Forbidden",
        &fixed_fields(&forbidden_body),
        &forbidden_body,
    ));
    let forbidden_error = execute(&forbidden).await.err().unwrap();
    assert_eq!(
        forbidden_error,
        ExchangeError::TokenResponse(TokenResponseError::Http(HttpFailure::Forbidden))
    );
    let unrequested = PeerHarness::reply(fixed_response(&token_body(APPDATA)));
    let (hold, receiver) = oneshot::channel();
    let scope_error = exchange_at(
        code_grant(&[FILE]),
        attempt_metadata(),
        receiver,
        Target::Loopback(unrequested.address),
        test_limits(),
    )
    .await
    .err()
    .unwrap();
    drop(hold);
    assert_eq!(
        scope_error,
        ExchangeError::TokenResponse(TokenResponseError::UnrequestedScope)
    );
    let broad = PeerHarness::reply(fixed_response(&token_body(
        "https://www.googleapis.com/auth/drive",
    )));
    let broad_error = execute(&broad).await.err().unwrap();
    assert_eq!(
        broad_error,
        ExchangeError::TokenResponse(TokenResponseError::InvalidScopeClaim)
    );
    for error in [provider_error, forbidden_error, scope_error, broad_error] {
        for text in [error.to_string(), format!("{error:?}")] {
            assert!(text.len() < 220);
            for marker in [
                "SYNTHETIC_",
                "synthetic.invalid",
                "http://",
                "https://",
                ACCESS,
                REFRESH,
                CODE,
                CLIENT,
            ] {
                assert!(!text.contains(marker));
            }
        }
    }
}
