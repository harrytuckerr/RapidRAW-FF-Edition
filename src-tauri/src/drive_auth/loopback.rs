//! Bounded native desktop OAuth callback listener. No browser launch or token I/O.
//!
//! Bind and URL access are native-only. `wait_on_worker` is synchronous and MUST
//! run on an explicitly owned worker, never a UI/runtime executor thread. This
//! narrow HTTP/1.1 GET subset is not a general HTTP server. Loopback access is not
//! identity: the frozen PKCE transaction validates state and response semantics.
//! Secret-bearing owners/results have no Debug, Display, Clone or serde impl.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::pkce::{CALLBACK_PATH, CallbackOutcome, PkceError, PkceTransaction};

pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HEADER_FIELDS: usize = 32;
pub const MAX_ACTIVE_CONNECTIONS: usize = 4;
pub const MAX_TRANSACTION_CONNECTIONS: usize = 32;
pub const CONNECTION_LIFETIME: Duration = Duration::from_secs(2);
pub const POLL_INTERVAL: Duration = Duration::from_millis(10);

// No response content comes from a request or provider. Connection-close framing
// is intentional. The body makes no claim that authorization succeeded.
const RESPONSE_OK: &[u8] = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nPragma: no-cache\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nX-Content-Type-Options: nosniff\r\n\r\n<!doctype html><title>RapidRAW</title><p>Request received. Return to RapidRAW.</p>";
const RESPONSE_BAD: &[u8] = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nPragma: no-cache\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nX-Content-Type-Options: nosniff\r\n\r\n<!doctype html><title>RapidRAW</title><p>Request not accepted. Return to RapidRAW.</p>";

/// Typed, bounded diagnostics only. No OS messages or request bytes are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackError {
    Bind,
    SocketSetup,
    SocketBinding,
    Accept,
    Read,
    Cancelled,
    TimedOut,
    ConnectionLimit,
    Closed,
    Pkce(PkceError),
}

/// Native cancellation signal only; carries no transaction state or credentials.
/// The worker observes it at its next poll. Before starting the worker, drop the
/// session (or call URL access, which checks cancellation) to release the socket.
#[derive(Clone)]
pub struct Cancellation {
    requested: Arc<AtomicBool>,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.requested.store(true, Ordering::Release);
    }
}

struct Connection {
    stream: TcpStream,
    accepted_at: Instant,
    bytes: Vec<u8>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

pub struct LoopbackSession {
    listener: Option<TcpListener>,
    transaction: Option<PkceTransaction>,
    address: SocketAddr,
    cancellation: Cancellation,
    connections: Vec<Connection>,
    accepted: usize,
}
impl LoopbackSession {
    /// Binds ONLY 127.0.0.1:0, then creates PKCE against the actual socket port.
    /// No socket reuse option, DNS, IPv6 or non-loopback fallback is configured.
    pub fn bind(
        client_id: &str,
        scopes: &[&str],
        lifetime: Duration,
    ) -> Result<Self, LoopbackError> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| LoopbackError::Bind)?;
        let address = listener
            .local_addr()
            .map_err(|_| LoopbackError::SocketBinding)?;
        if !matches!(address, SocketAddr::V4(a) if *a.ip() == Ipv4Addr::LOCALHOST && a.port() != 0)
        {
            return Err(LoopbackError::SocketBinding);
        }
        listener
            .set_nonblocking(true)
            .map_err(|_| LoopbackError::SocketSetup)?;
        let transaction =
            PkceTransaction::begin(client_id, address.port(), scopes, Instant::now(), lifetime)
                .map_err(LoopbackError::Pkce)?;
        Ok(Self {
            listener: Some(listener),
            transaction: Some(transaction),
            address,
            cancellation: Cancellation {
                requested: Arc::new(AtomicBool::new(false)),
            },
            connections: Vec::with_capacity(MAX_ACTIVE_CONNECTIONS),
            accepted: 0,
        })
    }

    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    /// Native browser-launch boundary only. The borrowed URL contains state and
    /// must never be serialized into UI IPC or logs. No browser is launched here.
    pub fn authorization_url_for_browser(&mut self) -> Result<&str, LoopbackError> {
        let now = Instant::now();
        if let Err(error) = self.check_active(now) {
            self.close();
            return Err(error);
        }
        self.transaction
            .as_mut()
            .ok_or(LoopbackError::Closed)?
            .authorization_url_for_browser(now)
            .map_err(LoopbackError::Pkce)
    }

    /// Consumes the owner and closes all owned sockets before returning, on every
    /// success/error path. Dropping it before starting also closes the listener.
    /// At most one nonblocking read per connection and one accept per poll; sleep
    /// is 10ms. Scheduling delays are OS-dependent, not a real-time guarantee.
    pub fn wait_on_worker(mut self) -> Result<CallbackOutcome, LoopbackError> {
        let result = loop {
            match self.poll_once() {
                Ok(Some(outcome)) => break Ok(outcome),
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(error) => break Err(error),
            }
        };
        self.close();
        result
    }

    fn check_active(&mut self, now: Instant) -> Result<(), LoopbackError> {
        let tx = self.transaction.as_mut().ok_or(LoopbackError::Closed)?;
        // Deadline wins if already expired. Otherwise cancellation observed at
        // this boundary wins over processing a ready socket. Once PKCE accepts a
        // callback, its terminal result wins over later cancellation.
        tx.check_active(now).map_err(map_transaction_error)?;
        if self.cancellation.requested.load(Ordering::Acquire) {
            tx.cancel(now).map_err(map_transaction_error)?;
            return Err(LoopbackError::Cancelled);
        }
        Ok(())
    }

    fn poll_once(&mut self) -> Result<Option<CallbackOutcome>, LoopbackError> {
        self.check_active(Instant::now())?;
        let mut index = 0;
        while index < self.connections.len() {
            self.check_active(Instant::now())?;
            let read = read_request(&mut self.connections[index], Instant::now());
            match read {
                ReadState::Pending => index += 1,
                ReadState::Reject => {
                    let mut conn = self.connections.swap_remove(index);
                    respond(&mut conn.stream, RESPONSE_BAD);
                }
                ReadState::Fatal => return Err(LoopbackError::Read),
                ReadState::Complete => {
                    let mut conn = self.connections.swap_remove(index);
                    // Socket binding is checked at accept AND at the forwarding
                    // boundary. No Host/Forwarded value chooses this authority.
                    if !socket_matches(&conn.stream, self.address) {
                        return Err(LoopbackError::SocketBinding);
                    }
                    let target = match parse_request(&conn.bytes, self.address.port()) {
                        Ok(target) => target,
                        Err(()) => {
                            respond(&mut conn.stream, RESPONSE_BAD);
                            continue;
                        }
                    };
                    self.check_active(Instant::now())?;
                    let callback = format!("http://127.0.0.1:{}{target}", self.address.port());
                    let result = self
                        .transaction
                        .as_mut()
                        .ok_or(LoopbackError::Closed)?
                        .accept_callback(&callback, Instant::now());
                    match result {
                        Ok(outcome) => {
                            respond(&mut conn.stream, RESPONSE_OK);
                            return Ok(Some(outcome));
                        }
                        Err(PkceError::Expired) => return Err(LoopbackError::TimedOut),
                        Err(PkceError::ClockRegression | PkceError::Finished) => {
                            return Err(LoopbackError::Closed);
                        }
                        Err(_) => respond(&mut conn.stream, RESPONSE_BAD),
                    }
                }
            }
        }
        if self.accepted == MAX_TRANSACTION_CONNECTIONS && self.connections.is_empty() {
            return Err(LoopbackError::ConnectionLimit);
        }
        if self.connections.len() < MAX_ACTIVE_CONNECTIONS
            && self.accepted < MAX_TRANSACTION_CONNECTIONS
        {
            let listener = self.listener.as_ref().ok_or(LoopbackError::Closed)?;
            if listener
                .local_addr()
                .map_err(|_| LoopbackError::SocketBinding)?
                != self.address
            {
                return Err(LoopbackError::SocketBinding);
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    self.accepted += 1;
                    if !socket_matches(&stream, self.address) {
                        return Err(LoopbackError::SocketBinding);
                    }
                    stream
                        .set_nonblocking(true)
                        .map_err(|_| LoopbackError::SocketSetup)?;
                    self.connections.push(Connection {
                        stream,
                        accepted_at: Instant::now(),
                        bytes: Vec::with_capacity(MAX_HEADER_BYTES + 1),
                    });
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err(LoopbackError::Accept),
            }
        }
        Ok(None)
    }

    fn close(&mut self) {
        self.connections.clear();
        drop(self.listener.take());
        drop(self.transaction.take());
    }
}
impl Drop for LoopbackSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn map_transaction_error(error: PkceError) -> LoopbackError {
    match error {
        PkceError::Expired => LoopbackError::TimedOut,
        other => LoopbackError::Pkce(other),
    }
}
fn socket_matches(stream: &TcpStream, bound: SocketAddr) -> bool {
    stream.local_addr().is_ok_and(|local| local == bound)
        && stream
            .peer_addr()
            .is_ok_and(|peer| matches!(peer, SocketAddr::V4(a) if a.ip().is_loopback()))
}
fn respond(stream: &mut TcpStream, response: &'static [u8]) {
    // Accepted streams are nonblocking. Best-effort bounded static output: a
    // disconnected/backpressured browser does not undo a valid native result.
    // One write, even on EINTR/short write: no retry/wait, keepalive, reflected
    // bytes, redirect or external resource. Browser delivery is not guaranteed.
    let _write_result = stream.write(response);
    let _ = stream.shutdown(Shutdown::Both);
}

enum ReadState {
    Pending,
    Complete,
    Reject,
    Fatal,
}
fn read_request(conn: &mut Connection, now: Instant) -> ReadState {
    if now.duration_since(conn.accepted_at) >= CONNECTION_LIFETIME {
        return ReadState::Reject;
    }
    let mut chunk = [0u8; 1024];
    let remaining = (MAX_HEADER_BYTES + 1 - conn.bytes.len()).min(chunk.len());
    match conn.stream.read(&mut chunk[..remaining]) {
        Ok(0) => ReadState::Reject,
        Ok(count) => {
            conn.bytes.extend_from_slice(&chunk[..count]);
            if conn.bytes.len() > MAX_HEADER_BYTES {
                return ReadState::Reject;
            }
            if conn.bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                ReadState::Complete
            } else {
                ReadState::Pending
            }
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            ReadState::Pending
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
            ) =>
        {
            ReadState::Reject
        }
        Err(_) => ReadState::Fatal,
    }
}

/// Strict HTTP/1.1 origin-form GET subset: CRLF, exactly one matching Host,
/// ASCII headers, no obs-fold, and no body except a single Content-Length: 0.
/// Reject TE/Expect/Upgrade and already-buffered trailing bytes/pipelining.
/// Later unframed bytes cannot be predicted; they are never interpreted and the
/// connection is closed after this one request. Forwarding headers are inert.
fn parse_request(bytes: &[u8], port: u16) -> Result<&str, ()> {
    if bytes.len() > MAX_HEADER_BYTES || !bytes.is_ascii() || !bytes.ends_with(b"\r\n\r\n") {
        return Err(());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let mut request = lines.next().ok_or(())?.split(' ');
    if request.next() != Some("GET") {
        return Err(());
    }
    let target = request.next().ok_or(())?;
    if request.next() != Some("HTTP/1.1") || request.next().is_some() {
        return Err(());
    }
    let (path, _) = target.split_once('?').ok_or(())?;
    if path != CALLBACK_PATH
        || target
            .bytes()
            .any(|b| b <= 0x20 || b >= 0x7f || matches!(b, b'#' | b'\\'))
    {
        return Err(());
    }
    let expected_host = format!("127.0.0.1:{port}");
    let mut host_seen = false;
    let mut length_seen = false;
    for (index, line) in lines.enumerate() {
        if index >= MAX_HEADER_FIELDS {
            return Err(());
        }
        let (name, value) = line.split_once(':').ok_or(())?;
        if name.is_empty()
            || !name.bytes().all(header_token)
            || value.bytes().any(|b| (b < 0x20 && b != b'\t') || b >= 0x7f)
        {
            return Err(());
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("host") {
            if host_seen || value != expected_host {
                return Err(());
            }
            host_seen = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            if length_seen || value != "0" {
                return Err(());
            }
            length_seen = true;
        } else if ["transfer-encoding", "expect", "upgrade"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n))
        {
            return Err(());
        }
    }
    if !host_seen {
        return Err(());
    }
    Ok(target)
}
fn header_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::super::pkce::ApprovedScope;
    use super::*;
    use reqwest::Url;
    use std::thread::JoinHandle;

    const CLIENT: &str = "synthetic.apps.googleusercontent.com";
    struct Running {
        address: SocketAddr,
        state: String,
        cancel: Cancellation,
        worker: Option<JoinHandle<Result<CallbackOutcome, LoopbackError>>>,
    }
    impl Running {
        fn start(lifetime: Duration) -> Self {
            let mut session =
                LoopbackSession::bind(CLIENT, &[ApprovedScope::AppData.as_str()], lifetime)
                    .unwrap();
            let address = session.address;
            let url = Url::parse(session.authorization_url_for_browser().unwrap()).unwrap();
            let state = url
                .query_pairs()
                .find(|(key, _)| key == "state")
                .unwrap()
                .1
                .into_owned();
            assert_eq!(
                url.query_pairs()
                    .find(|(key, _)| key == "redirect_uri")
                    .unwrap()
                    .1,
                format!("http://127.0.0.1:{}{CALLBACK_PATH}", address.port())
            );
            let cancel = session.cancellation();
            let worker = Some(thread::spawn(move || session.wait_on_worker()));
            Self {
                address,
                state,
                cancel,
                worker,
            }
        }
        fn request(&self, tail: &str) -> String {
            format!(
                "GET {CALLBACK_PATH}?state={}&{tail} HTTP/1.1\r\nHost: {}\r\n\r\n",
                self.state, self.address
            )
        }
        fn send(&self, request: &[u8]) -> Vec<u8> {
            let mut stream =
                TcpStream::connect_timeout(&self.address, Duration::from_secs(1)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            stream.write_all(request).unwrap();
            let mut response = Vec::new();
            // RST after rejecting unread excess bytes is a permitted close.
            match stream.read_to_end(&mut response) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
                Err(_) => panic!("unexpected response read error"),
            }
            response
        }
        fn finish(mut self) -> Result<CallbackOutcome, LoopbackError> {
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
    fn assert_closed(address: SocketAddr) {
        assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    }
    fn assert_bad(response: &[u8]) {
        assert!(response.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
    }

    #[test]
    fn real_code_request_returns_native_grant_and_closes_listener() {
        let running = Running::start(Duration::from_secs(10));
        let address = running.address;
        let request = running.request("code=4%2Fsynthetic&provider_extension=ignored");
        let response = running.send(request.as_bytes());
        assert_eq!(response, RESPONSE_OK);
        assert!(!String::from_utf8_lossy(&response).contains(&running.state));
        assert!(!String::from_utf8_lossy(&response).contains("synthetic"));
        let CallbackOutcome::Code(grant) = running.finish().unwrap() else {
            panic!("expected code")
        };
        assert_eq!(grant.code_for_exchange(), "4/synthetic");
        assert_eq!(grant.verifier_for_exchange().len(), 43);
        assert_eq!(grant.client_id(), CLIENT);
        assert_eq!(grant.requested_scopes(), &[ApprovedScope::AppData]);
        assert_closed(address);
    }

    #[test]
    fn real_denial_and_provider_error_are_terminal_without_reflection() {
        for (error, denied) in [("access_denied", true), ("server_error", false)] {
            let running = Running::start(Duration::from_secs(10));
            let address = running.address;
            let response = running.send(
                running
                    .request(&format!(
                        "error={error}&error_description=private&extension=x"
                    ))
                    .as_bytes(),
            );
            assert_eq!(response, RESPONSE_OK);
            assert!(!String::from_utf8_lossy(&response).contains("private"));
            let outcome = running.finish().unwrap();
            assert!(matches!(
                (outcome, denied),
                (CallbackOutcome::Denied, true) | (CallbackOutcome::ProviderRejected, false)
            ));
            assert_closed(address);
        }
    }

    #[test]
    fn real_invalid_requests_cannot_complete_or_poison_later_valid_callback() {
        let running = Running::start(Duration::from_secs(10));
        let good = running.request("code=synthetic");
        let target = format!("{CALLBACK_PATH}?state={}&code=synthetic", running.state);
        let bad = [
            good.replace("Host: ", "Host: foreign.invalid:"),
            good.replace(
                "\r\n\r\n",
                &format!("\r\nhOsT: {}\r\n\r\n", running.address),
            ),
            good.replace(&target, &format!("http://{}{target}", running.address)),
            good.replace(CALLBACK_PATH, "/oauth2/callback/%67oogle"),
            good.replace(CALLBACK_PATH, "/a/../oauth2/callback/google"),
            good.replace("GET ", "POST "),
            good.replace("HTTP/1.1", "HTTP/1.0"),
            good.replace(&running.state, "wrong-state"),
            good.replace("code=synthetic", "code=a&%63ode=b"),
            good.replace("code=synthetic", "code=a#fragment"),
            good.replace("Host:", "Host :"),
            good.replace("\r\nHost", "\r\n Host"),
        ];
        for request in bad {
            assert_bad(&running.send(request.as_bytes()));
        }
        assert_eq!(running.send(good.as_bytes()), RESPONSE_OK);
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
    }

    #[test]
    fn forwarding_headers_never_repair_wrong_host_or_retarget_authority() {
        let running = Running::start(Duration::from_secs(10));
        let extra = format!(
            "Forwarded: host={}\r\nX-Forwarded-Host: {}\r\nX-Forwarded-Proto: https\r\n",
            running.address, running.address
        );
        let good = running
            .request("code=synthetic")
            .replace("\r\n\r\n", &format!("\r\n{extra}\r\n"));
        let wrong = good.replacen(
            &format!("Host: {}", running.address),
            "Host: evil.invalid",
            1,
        );
        assert_bad(&running.send(wrong.as_bytes()));
        assert_eq!(running.send(good.as_bytes()), RESPONSE_OK);
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
    }

    #[test]
    fn body_framing_and_header_injection_are_rejected() {
        let running = Running::start(Duration::from_secs(10));
        let good = running.request("code=synthetic");
        for header in [
            "Content-Length: 1",
            "Content-Length: 0\r\nContent-Length: 0",
            "Transfer-Encoding: chunked",
            "Transfer-Encoding: identity",
            "Expect: 100-continue",
            "Upgrade: websocket",
            "X-Test: a\nb",
            "X-Test: a\rb",
            "X-Test: a\0b",
        ] {
            let request = good.replace("\r\n\r\n", &format!("\r\n{header}\r\n\r\n"));
            assert_bad(&running.send(request.as_bytes()));
        }
        let allowed = good.replace("\r\n\r\n", "\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(running.send(allowed.as_bytes()), RESPONSE_OK);
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
    }

    #[test]
    fn parser_rejects_buffered_body_pipelining_lf_only_and_missing_host() {
        let request =
            format!("GET {CALLBACK_PATH}?state=a&code=b HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n\r\n");
        for invalid in [
            format!("{request}x"),
            format!("{request}{request}"),
            request.replace("\r\n", "\n"),
            request.replace("Host: 127.0.0.1:1234\r\n", ""),
            request.replace("127.0.0.1:1234", "127.1:1234"),
            request.replace("127.0.0.1:1234", "localhost:1234"),
            request.replace("127.0.0.1:1234", "127.0.0.1:01234"),
            request.replace("GET ", "GET  "),
        ] {
            assert!(parse_request(invalid.as_bytes(), 1234).is_err());
        }
        assert!(parse_request(request.as_bytes(), 1234).is_ok());
    }

    #[test]
    fn stalled_client_does_not_block_concurrent_valid_request() {
        let running = Running::start(Duration::from_secs(10));
        let mut stalled = TcpStream::connect(running.address).unwrap();
        stalled.write_all(b"GET /oauth2/").unwrap();
        assert_eq!(
            running.send(running.request("code=synthetic").as_bytes()),
            RESPONSE_OK
        );
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
        stalled
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0];
        match stalled.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            _ => panic!("peer was not closed"),
        }
    }

    #[test]
    fn slow_header_deadline_is_absolute_and_session_remains_usable() {
        let running = Running::start(Duration::from_secs(10));
        let mut slow = TcpStream::connect(running.address).unwrap();
        slow.write_all(b"GET ").unwrap();
        slow.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        let start = Instant::now();
        let mut response = Vec::new();
        slow.read_to_end(&mut response).unwrap();
        assert_bad(&response);
        assert!(start.elapsed() < Duration::from_secs(4));
        assert_eq!(
            running.send(running.request("code=synthetic").as_bytes()),
            RESPONSE_OK
        );
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
    }

    #[test]
    fn oversized_headers_and_too_many_fields_are_rejected() {
        let running = Running::start(Duration::from_secs(10));
        let good = running.request("code=synthetic");
        let oversized = good.replace(
            "\r\n\r\n",
            &format!("\r\nX-Pad: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES)),
        );
        // Small local TCP write fits typical buffers; response may be followed by
        // reset because the rejected remainder is intentionally never consumed.
        let response = running.send(oversized.as_bytes());
        assert!(response.is_empty() || response == RESPONSE_BAD);
        let fields = (0..MAX_HEADER_FIELDS)
            .map(|n| format!("X-{n}: a\r\n"))
            .collect::<String>();
        let crowded = good.replace("\r\n\r\n", &format!("\r\n{fields}\r\n"));
        assert_bad(&running.send(crowded.as_bytes()));
        assert_eq!(running.send(good.as_bytes()), RESPONSE_OK);
        assert!(matches!(running.finish(), Ok(CallbackOutcome::Code(_))));
    }

    #[test]
    fn cancellation_interrupts_stalled_connection_and_closes_listener() {
        let running = Running::start(Duration::from_secs(10));
        let address = running.address;
        let _stalled = TcpStream::connect(address).unwrap();
        let start = Instant::now();
        running.cancel.cancel();
        assert!(matches!(running.finish(), Err(LoopbackError::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_closed(address);
    }

    #[test]
    fn total_timeout_closes_idle_listener_despite_stalled_peer() {
        let running = Running::start(Duration::from_millis(150));
        let address = running.address;
        let _stalled = TcpStream::connect(address).unwrap();
        assert!(matches!(running.finish(), Err(LoopbackError::TimedOut)));
        assert_closed(address);
    }

    #[test]
    fn wrong_requests_exhaust_bounded_budget_without_success() {
        let running = Running::start(Duration::from_secs(10));
        let address = running.address;
        for _ in 0..MAX_TRANSACTION_CONNECTIONS {
            let request = running.request("code=a").replace(&running.state, "wrong");
            assert_bad(&running.send(request.as_bytes()));
        }
        assert!(matches!(
            running.finish(),
            Err(LoopbackError::ConnectionLimit)
        ));
        assert_closed(address);
    }

    #[test]
    fn drop_and_cancel_before_worker_close_bound_listener() {
        let session = LoopbackSession::bind(
            CLIENT,
            &[ApprovedScope::File.as_str()],
            Duration::from_secs(10),
        )
        .unwrap();
        let address = session.address;
        drop(session);
        assert_closed(address);
        let mut session = LoopbackSession::bind(
            CLIENT,
            &[ApprovedScope::File.as_str()],
            Duration::from_secs(10),
        )
        .unwrap();
        let address = session.address;
        session.cancellation().cancel();
        assert_eq!(
            session.authorization_url_for_browser().err(),
            Some(LoopbackError::Cancelled)
        );
        assert_closed(address);
    }

    #[test]
    fn local_binding_check_rejects_the_wrong_bound_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();
        assert!(socket_matches(&server, address));
        assert!(!socket_matches(
            &server,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
        ));
        drop(client);
    }

    #[test]
    fn active_connection_cap_and_absolute_connection_deadline_use_real_streams() {
        let mut session = LoopbackSession::bind(
            CLIENT,
            &[ApprovedScope::File.as_str()],
            Duration::from_secs(10),
        )
        .unwrap();
        let peers: Vec<_> = (0..MAX_ACTIVE_CONNECTIONS + 1)
            .map(|_| TcpStream::connect(session.address).unwrap())
            .collect();
        for _ in 0..MAX_ACTIVE_CONNECTIONS + 1 {
            assert!(session.poll_once().unwrap().is_none());
        }
        assert_eq!(session.connections.len(), MAX_ACTIVE_CONNECTIONS);
        assert_eq!(session.accepted, MAX_ACTIVE_CONNECTIONS);
        let connection = &mut session.connections[0];
        connection.bytes.extend_from_slice(b"GET /");
        let boundary = connection.accepted_at + CONNECTION_LIFETIME;
        assert!(matches!(
            read_request(connection, boundary),
            ReadState::Reject
        ));
        let address = session.address;
        drop(session);
        assert_closed(address);
        drop(peers);
    }

    #[test]
    fn static_responses_have_privacy_headers_and_no_external_resources() {
        for response in [RESPONSE_OK, RESPONSE_BAD] {
            let text = std::str::from_utf8(response).unwrap();
            assert!(text.contains("Cache-Control: no-store\r\n"));
            assert!(text.contains("Referrer-Policy: no-referrer\r\n"));
            assert!(text.contains("Content-Security-Policy: default-src 'none'"));
            assert!(text.contains("Connection: close\r\n"));
            assert!(!text.contains("<script"));
            assert!(!text.contains("http://"));
            assert!(!text.contains("https://"));
        }
    }
}
