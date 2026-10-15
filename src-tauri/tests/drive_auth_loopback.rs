//! Independent public-API localhost probes. Synthetic inputs, no provider/browser.
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rapidraw_lib::drive_auth::loopback::{
    Cancellation, LoopbackError, LoopbackSession, MAX_ACTIVE_CONNECTIONS, MAX_HEADER_BYTES,
    MAX_HEADER_FIELDS, MAX_TRANSACTION_CONNECTIONS,
};
use rapidraw_lib::drive_auth::pkce::{ApprovedScope, CallbackOutcome};
use reqwest::Url;
use sha2::{Digest, Sha256};

const CLIENT: &str = "independent-loopback.apps.googleusercontent.com";
const PATH: &str = "/oauth2/callback/google";
const APPDATA: &str = "https://www.googleapis.com/auth/drive.appdata";
const FILE: &str = "https://www.googleapis.com/auth/drive.file";

type NativeResult = Result<CallbackOutcome, LoopbackError>;
struct Endpoint {
    address: SocketAddr,
    redirect: String,
    state: String,
    challenge: String,
}
fn endpoint(session: &mut LoopbackSession) -> Endpoint {
    let url = Url::parse(session.authorization_url_for_browser().unwrap()).unwrap();
    assert!(url.origin().ascii_serialization() == "https://accounts.google.com");
    let fields: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert!(fields["scope"] == format!("{FILE} {APPDATA}"));
    assert!(fields["code_challenge_method"] == "S256");
    let redirect = Url::parse(&fields["redirect_uri"]).unwrap();
    assert!(redirect.scheme() == "http");
    assert!(redirect.host_str() == Some("127.0.0.1"));
    assert!(redirect.path() == PATH);
    assert!(redirect.query().is_none() && redirect.fragment().is_none());
    let port = redirect.port().unwrap();
    assert!(port != 0);
    Endpoint {
        address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        redirect: fields["redirect_uri"].clone(),
        state: fields["state"].clone(),
        challenge: fields["code_challenge"].clone(),
    }
}
fn bind(lifetime: Duration) -> LoopbackSession {
    LoopbackSession::bind(CLIENT, &[FILE, APPDATA], lifetime).unwrap()
}
struct Running {
    endpoint: Endpoint,
    cancel: Cancellation,
    worker: Option<JoinHandle<NativeResult>>,
}
impl Running {
    fn start(lifetime: Duration) -> Self {
        let mut session = bind(lifetime);
        let endpoint = endpoint(&mut session);
        let cancel = session.cancellation();
        Self {
            endpoint,
            cancel,
            worker: Some(thread::spawn(move || session.wait_on_worker())),
        }
    }
    fn target(&self, tail: &str) -> String {
        format!("{PATH}?state={}&{tail}", self.endpoint.state)
    }
    fn request(&self, tail: &str) -> String {
        self.request_target(
            &self.target(tail),
            &format!("Host: {}\r\n", self.endpoint.address),
        )
    }
    fn request_target(&self, target: &str, headers: &str) -> String {
        format!("GET {target} HTTP/1.1\r\n{headers}\r\n")
    }
    fn connect(&self) -> TcpStream {
        let stream =
            TcpStream::connect_timeout(&self.endpoint.address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
    }
    fn exchange(&self, request: &[u8]) -> Vec<u8> {
        let mut stream = self.connect();
        stream.write_all(request).unwrap();
        read_closed(&mut stream)
    }
    fn finish(mut self) -> NativeResult {
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn read_closed(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    match stream.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
            ) => {}
        Err(error) => panic!("unexpected bounded localhost read: {:?}", error.kind()),
    }
    bytes
}
fn closed(address: SocketAddr) {
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
}
fn rejected(bytes: &[u8], case: &str) {
    // Output is explicitly best effort; closure plus a later exact native result
    // proves rejection even when unread excess bytes produce an empty/RST reply.
    assert!(
        bytes.is_empty() || bytes.starts_with(b"HTTP/1.1 400"),
        "bad response category: {case}"
    );
    assert!(!String::from_utf8_lossy(bytes).contains("SYNTHETIC_PRIVATE"));
}
fn privacy(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    for header in [
        "Connection: close\r\n",
        "Cache-Control: no-store\r\n",
        "Pragma: no-cache\r\n",
        "Referrer-Policy: no-referrer\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Content-Security-Policy: default-src 'none'",
    ] {
        assert!(text.contains(header), "missing static privacy header");
    }
    for forbidden in [
        "SYNTHETIC_PRIVATE",
        "<script",
        "http://",
        "https://",
        "Location:",
    ] {
        assert!(!text.contains(forbidden));
    }
}
fn exact_code(running: Running, code: &str) {
    let address = running.endpoint.address;
    let redirect = running.endpoint.redirect.clone();
    let challenge = running.endpoint.challenge.clone();
    let CallbackOutcome::Code(grant) = running.finish().unwrap() else {
        panic!("expected native code")
    };
    assert!(grant.code_for_exchange() == code);
    assert!(grant.client_id() == CLIENT);
    assert!(grant.redirect_uri() == redirect);
    assert!(grant.requested_scopes() == [ApprovedScope::File, ApprovedScope::AppData]);
    // Independent public verifier -> standard SHA256/base64url formula.
    let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(grant.verifier_for_exchange().as_bytes()));
    assert!(challenge == expected);
    closed(address);
}

#[test]
fn production_ephemeral_binding_code_extension_and_native_s256_grant() {
    let running = Running::start(Duration::from_secs(8));
    let response = running.exchange(
        running
            .request("code=4%2FSYNTHETIC_PRIVATE%2Bcode&provider_extension=synthetic")
            .as_bytes(),
    );
    privacy(&response);
    assert!(!String::from_utf8_lossy(&response).contains(&running.endpoint.state));
    exact_code(running, "4/SYNTHETIC_PRIVATE+code");
}

#[test]
fn denial_and_provider_error_extensions_are_terminal_with_identical_static_reply() {
    let mut prior = None;
    for (error, denied) in [("access_denied", true), ("server_error", false)] {
        let running = Running::start(Duration::from_secs(8));
        let address = running.endpoint.address;
        let response = running.exchange(running.request(&format!("error={error}&error_description=SYNTHETIC_PRIVATE_DETAIL&provider_extension=synthetic")).as_bytes());
        privacy(&response);
        assert!(!String::from_utf8_lossy(&response).contains(&running.endpoint.state));
        if let Some(previous) = prior {
            assert!(response == previous);
        }
        prior = Some(response);
        assert!(matches!(
            (running.finish().unwrap(), denied),
            (CallbackOutcome::Denied, true) | (CallbackOutcome::ProviderRejected, false)
        ));
        closed(address);
    }
}

#[test]
fn malformed_http_state_duplicates_and_body_cannot_authorize_or_poison_success() {
    let running = Running::start(Duration::from_secs(8));
    let target = running.target("code=SYNTHETIC_PRIVATE_ATTACK");
    let host = format!("Host: {}\r\n", running.endpoint.address);
    let good = running.request_target(&target, &host);
    let mut bad = vec![
        (
            "wrong Host",
            running.request_target(&target, "Host: foreign.invalid\r\n"),
        ),
        (
            "forwarded Host cannot repair",
            running.request_target(
                &target,
                &format!(
                    "Host: foreign.invalid\r\nForwarded: host={}\r\nX-Forwarded-Host: {}\r\n",
                    running.endpoint.address, running.endpoint.address
                ),
            ),
        ),
        (
            "duplicate Host",
            running.request_target(
                &target,
                &format!("{host}hOsT: {}\r\n", running.endpoint.address),
            ),
        ),
        ("missing Host", running.request_target(&target, "")),
        (
            "absolute target",
            running.request_target(
                &format!("http://{}{target}", running.endpoint.address),
                &host,
            ),
        ),
        (
            "encoded path",
            good.replace(PATH, "/oauth2/callback/%67oogle"),
        ),
        (
            "encoded slash",
            good.replace(PATH, "/oauth2%2Fcallback/google"),
        ),
        (
            "wrong state",
            good.replace(&running.endpoint.state, "definitely-wrong"),
        ),
        ("duplicate code", running.request("code=a&%63ode=b")),
        ("duplicate state", running.request("code=a&%73tate=other")),
        ("buffered body", format!("{good}SYNTHETIC_PRIVATE_BODY")),
        ("pipelining", format!("{good}{good}")),
    ];
    for (name, header) in [
        ("length1", "Content-Length: 1"),
        ("length00", "Content-Length: 00"),
        ("duplicate length", "Content-Length: 0\r\nContent-Length: 0"),
        ("transfer encoding", "Transfer-Encoding: chunked"),
        ("expect", "Expect: 100-continue"),
        ("upgrade", "Upgrade: websocket"),
    ] {
        bad.push((
            name,
            running.request_target(&target, &format!("{host}{header}\r\n")),
        ));
    }
    assert!(bad.len() < MAX_TRANSACTION_CONNECTIONS - 1);
    for (name, request) in bad {
        rejected(&running.exchange(request.as_bytes()), name);
    }
    let response = running.exchange(running.request("code=final-correct").as_bytes());
    privacy(&response);
    exact_code(running, "final-correct");
}

#[test]
fn valid_ows_host_and_inert_forwarding_headers_keep_actual_binding() {
    let running = Running::start(Duration::from_secs(8));
    let request = running.request_target(&running.target("code=correct-authority"), &format!("hOsT:\t{} \t\r\nForwarded: host=foreign.invalid;proto=https\r\nX-Forwarded-Host: foreign.invalid\r\nContent-Length: 0\r\n", running.endpoint.address));
    privacy(&running.exchange(request.as_bytes()));
    exact_code(running, "correct-authority");
}

#[test]
fn oversize_and_field_caps_reject_then_exact_byte_boundary_succeeds() {
    let running = Running::start(Duration::from_secs(8));
    let target = running.target("code=at-byte-boundary");
    let prefix = format!(
        "GET {target} HTTP/1.1\r\nHost: {}\r\nX-Pad: ",
        running.endpoint.address
    );
    let suffix = "\r\n\r\n";
    let boundary = format!(
        "{prefix}{}{suffix}",
        "x".repeat(MAX_HEADER_BYTES - prefix.len() - suffix.len())
    );
    assert!(boundary.len() == MAX_HEADER_BYTES);
    let excess = boundary.replacen("X-Pad: ", "X-Pad: x", 1);
    rejected(
        &running.exchange(excess.as_bytes()),
        "one byte over header cap",
    );
    let fields = (0..MAX_HEADER_FIELDS)
        .map(|i| format!("X-{i}: x\r\n"))
        .collect::<String>();
    let crowded = running.request_target(
        &target,
        &format!("Host: {}\r\n{fields}", running.endpoint.address),
    );
    rejected(&running.exchange(crowded.as_bytes()), "33 total fields");
    privacy(&running.exchange(boundary.as_bytes()));
    exact_code(running, "at-byte-boundary");
    let running = Running::start(Duration::from_secs(8));
    let fields = (0..MAX_HEADER_FIELDS - 1)
        .map(|i| format!("X-{i}: x\r\n"))
        .collect::<String>();
    let request = running.request_target(
        &running.target("code=at-field-boundary"),
        &format!("Host: {}\r\n{fields}", running.endpoint.address),
    );
    privacy(&running.exchange(request.as_bytes()));
    exact_code(running, "at-field-boundary");
}

#[test]
fn four_stalled_owned_connections_queue_fifth_until_one_slot_closes() {
    const {
        assert!(MAX_ACTIVE_CONNECTIONS == 4);
    };
    let running = Running::start(Duration::from_secs(8));
    let mut held = Vec::new();
    for _ in 0..4 {
        let mut peer = running.connect();
        peer.write_all(b"GET /").unwrap();
        held.push(peer);
    }
    thread::sleep(Duration::from_millis(120));
    let mut fifth = running.connect();
    fifth
        .write_all(running.request("code=queued-fifth").as_bytes())
        .unwrap();
    fifth
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    let mut byte = [0];
    match fifth.read(&mut byte) {
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ) => {}
        _ => panic!("fifth request was processed while all four slots remained occupied"),
    }
    held.remove(0).shutdown(Shutdown::Both).unwrap();
    fifth
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    privacy(&read_closed(&mut fifth));
    exact_code(running, "queued-fifth");
    for mut peer in held {
        assert!(read_closed(&mut peer).is_empty());
    }
}

#[test]
fn trickled_header_absolute_deadline_does_not_renew_and_next_client_succeeds() {
    let running = Running::start(Duration::from_secs(8));
    let mut slow = running.connect();
    let start = Instant::now();
    slow.write_all(b"GET /").unwrap();
    for _ in 0..5 {
        thread::sleep(Duration::from_millis(300));
        slow.write_all(b"x").unwrap();
    }
    let response = read_closed(&mut slow);
    rejected(&response, "trickled incomplete header");
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(1900));
    assert!(
        elapsed < Duration::from_secs(3),
        "progress renewed the absolute per-connection deadline"
    );
    eprintln!(
        "independent trickled-header close observed in {}ms",
        elapsed.as_millis()
    );
    privacy(&running.exchange(running.request("code=after-trickle").as_bytes()));
    exact_code(running, "after-trickle");
}

#[test]
fn thirty_two_wrong_requests_end_with_connection_limit_and_closed_listener() {
    const {
        assert!(MAX_TRANSACTION_CONNECTIONS == 32);
    };
    let running = Running::start(Duration::from_secs(8));
    let address = running.endpoint.address;
    let request = running
        .request("code=never-authorized")
        .replace(&running.endpoint.state, "wrong-state");
    for _ in 0..32 {
        rejected(&running.exchange(request.as_bytes()), "budget wrong state");
    }
    assert!(matches!(
        running.finish(),
        Err(LoopbackError::ConnectionLimit)
    ));
    closed(address);
}

#[test]
fn cancel_and_total_expiry_interrupt_stalls_and_close_peers() {
    let running = Running::start(Duration::from_secs(8));
    let address = running.endpoint.address;
    let mut stalled = running.connect();
    stalled.write_all(b"GET /").unwrap();
    let start = Instant::now();
    running.cancel.cancel();
    assert!(matches!(running.finish(), Err(LoopbackError::Cancelled)));
    assert!(start.elapsed() < Duration::from_secs(1));
    closed(address);
    assert!(read_closed(&mut stalled).is_empty());
    let start = Instant::now();
    let running = Running::start(Duration::from_millis(180));
    let address = running.endpoint.address;
    let mut stalled = running.connect();
    assert!(matches!(running.finish(), Err(LoopbackError::TimedOut)));
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(170) && elapsed < Duration::from_secs(2));
    eprintln!(
        "independent total expiry observed in {}ms",
        elapsed.as_millis()
    );
    closed(address);
    assert!(read_closed(&mut stalled).is_empty());
}

#[test]
fn retained_unpolled_owner_needs_poll_or_drop_for_cancel_expiry_and_closure() {
    let mut session = bind(Duration::from_secs(8));
    let address = endpoint(&mut session).address;
    session.cancellation().cancel();
    // Signal alone has no hidden background watcher or synchronous cleanup.
    drop(TcpStream::connect_timeout(&address, Duration::from_millis(100)).unwrap());
    assert!(matches!(
        session.authorization_url_for_browser(),
        Err(LoopbackError::Cancelled)
    ));
    closed(address);
    let mut session = bind(Duration::from_millis(80));
    let address = endpoint(&mut session).address;
    thread::sleep(Duration::from_millis(120));
    drop(TcpStream::connect_timeout(&address, Duration::from_millis(100)).unwrap());
    assert!(matches!(
        session.wait_on_worker(),
        Err(LoopbackError::TimedOut)
    ));
    closed(address);
    let mut session = bind(Duration::from_secs(8));
    let address = endpoint(&mut session).address;
    drop(session);
    closed(address);
}
