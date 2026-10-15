//! One native desktop authorization-code exchange, not an OAuth/session manager.
//!
//! Run on a native Tokio runtime with timers, never block the UI thread. A code
//! grant is consumed even on failure; remote issuance after dispatch can be
//! uncertain, so never retry it. No browser, storage, account proof or IPC exists
//! here. Native controller adoption must still reject stale attempt/generation
//! results atomically. Dropping Strings/Vecs is not guaranteed zeroization.

use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderName,
    HeaderValue,
};
use reqwest::{Client, Url};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::pkce::AuthorizationCodeGrant;
use super::token_response::{
    MAX_RESPONSE_BYTES, ParsedTokenResponse, TokenResponseError, parse_google_token_response,
};

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const MAX_FORM_BYTES: usize = 8 * 1024;
const MAX_HEADER_FIELDS: usize = 64;
const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Native correlation only, not verified account identity. No serde or Debug.
pub struct ExchangeAttempt {
    pub id: Uuid,
    pub generation: u64,
}

/// Native-only result. Its controller must check active ID/generation/cancel and
/// verified account/access under its own commit lock before adopting tokens.
pub struct ExchangeResult {
    attempt: ExchangeAttempt,
    tokens: ParsedTokenResponse,
}
impl ExchangeResult {
    pub fn attempt(&self) -> &ExchangeAttempt {
        &self.attempt
    }
    pub fn tokens_for_native_controller(&self) -> &ParsedTokenResponse {
        &self.tokens
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeError {
    Cancelled,
    Deadline,
    ClientSetup,
    Transport,
    Redirect,
    ResponseHeaders,
    ContentEncoding,
    BodyLimit,
    TokenResponse(TokenResponseError),
}
impl fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Code exchange error: {self:?}")
    }
}
impl std::error::Error for ExchangeError {}

struct Limits {
    total: Duration,
    connect: Duration,
    read: Duration,
}
impl Limits {
    const PRODUCTION: Self = Self {
        total: Duration::from_secs(30),
        connect: Duration::from_secs(5),
        read: Duration::from_secs(5),
    };
}
enum Target {
    Google,
    // No caller-controlled endpoint or insecure production branch exists.
    #[cfg(test)]
    Loopback(std::net::SocketAddr),
}
impl Target {
    fn url(&self) -> Result<Url, ExchangeError> {
        match self {
            Self::Google => Url::parse(TOKEN_ENDPOINT).map_err(|_| ExchangeError::ClientSetup),
            #[cfg(test)]
            Self::Loopback(address) => {
                if !matches!(address, std::net::SocketAddr::V4(a) if *a.ip() == std::net::Ipv4Addr::LOCALHOST && a.port() != 0)
                {
                    return Err(ExchangeError::ClientSetup);
                }
                Url::parse(&format!("http://{address}/token"))
                    .map_err(|_| ExchangeError::ClientSetup)
            }
        }
    }
    fn https_only(&self) -> bool {
        match self {
            Self::Google => true,
            #[cfg(test)]
            Self::Loopback(_) => false,
        }
    }
}

/// Fixed HTTPS token endpoint, direct-only connectivity, no automatic retry.
/// Sending on OR dropping the cancellation sender cancels this operation. Request
/// construction and platform internals are synchronous/cooperative, not hard-abort
/// guarantees. The absolute deadline starts when this async function is polled.
pub async fn exchange_code(
    grant: AuthorizationCodeGrant,
    attempt: ExchangeAttempt,
    cancel: oneshot::Receiver<()>,
) -> Result<ExchangeResult, ExchangeError> {
    exchange_at(grant, attempt, cancel, Target::Google, Limits::PRODUCTION).await
}

async fn exchange_at(
    grant: AuthorizationCodeGrant,
    attempt: ExchangeAttempt,
    mut cancel: oneshot::Receiver<()>,
    target: Target,
    limits: Limits,
) -> Result<ExchangeResult, ExchangeError> {
    let started = Instant::now();
    let deadline = started
        .checked_add(limits.total)
        .ok_or(ExchangeError::Deadline)?;
    check_gate(&mut cancel, deadline)?;
    let endpoint = target.url()?;
    let client = build_client(&target, &limits)?;
    let form = encode_grant(&grant)?;
    check_gate(&mut cancel, deadline)?;

    // The complete owned send/read/parse future is dropped when either other
    // select branch wins. No detached operation or background retry is spawned.
    let operation = async {
        let request_started = Instant::now();
        let mut response = client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .header(ACCEPT_ENCODING, "identity")
            .body(form)
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(ExchangeError::Redirect);
        }
        validate_headers(response.headers())?;
        let mut bytes = Vec::with_capacity(MAX_RESPONSE_BYTES);
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            append_response(&mut bytes, &chunk)?;
            // Ensure an already-buffered/empty-frame stream cannot monopolize
            // this poll and starve the select cancellation/deadline branches.
            tokio::task::yield_now().await;
        }
        // A transport error above never permits parsing a truncated prefix.
        parse_google_token_response(
            status.as_u16(),
            &bytes,
            grant.requested_scopes(),
            request_started,
            Instant::now(),
        )
        .map_err(ExchangeError::TokenResponse)
    };
    let tokens = tokio::select! {
        biased;
        _ = &mut cancel => return Err(ExchangeError::Cancelled),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => return Err(ExchangeError::Deadline),
        result = operation => result?,
    };
    finish_result(tokens, attempt, &mut cancel, deadline)
}

fn finish_result(
    tokens: ParsedTokenResponse,
    attempt: ExchangeAttempt,
    cancel: &mut oneshot::Receiver<()>,
    deadline: Instant,
) -> Result<ExchangeResult, ExchangeError> {
    check_gate(cancel, deadline)?;
    if !tokens.is_usable_at(Instant::now()) {
        return Err(ExchangeError::TokenResponse(
            TokenResponseError::ExpiredAtReceipt,
        ));
    }
    // A later cancellation/account switch can still race this return: the native
    // controller's atomic generation check is mandatory, not implemented here.
    Ok(ExchangeResult { attempt, tokens })
}
fn check_gate(cancel: &mut oneshot::Receiver<()>, deadline: Instant) -> Result<(), ExchangeError> {
    match cancel.try_recv() {
        Ok(()) | Err(oneshot::error::TryRecvError::Closed) => return Err(ExchangeError::Cancelled),
        Err(oneshot::error::TryRecvError::Empty) => {}
    }
    if Instant::now() >= deadline {
        return Err(ExchangeError::Deadline);
    }
    Ok(())
}
fn build_client(target: &Target, limits: &Limits) -> Result<Client, ExchangeError> {
    Client::builder()
        .https_only(target.https_only())
        .tls_backend_rustls()
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .retry(reqwest::retry::never())
        .http1_only()
        .pool_max_idle_per_host(0)
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connect_timeout(limits.connect)
        .read_timeout(limits.read)
        .timeout(limits.total)
        .connection_verbose(false)
        // Fresh builder: no cookie store/provider or ambient sensitive headers.
        // No certificate/hostname verification bypass or extra roots.
        .build()
        .map_err(|_| ExchangeError::ClientSetup)
}
fn transport_error(error: reqwest::Error) -> ExchangeError {
    // Never retain, display, debug-format or log a reqwest error/request/response.
    if error.is_timeout() {
        ExchangeError::Deadline
    } else {
        ExchangeError::Transport
    }
}

fn encode_grant(grant: &AuthorizationCodeGrant) -> Result<Vec<u8>, ExchangeError> {
    let fields = [
        ("grant_type", "authorization_code"),
        ("code", grant.code_for_exchange()),
        ("code_verifier", grant.verifier_for_exchange()),
        ("client_id", grant.client_id()),
        ("redirect_uri", grant.redirect_uri()),
    ];
    let mut body = Vec::with_capacity(MAX_FORM_BYTES);
    for (index, (name, value)) in fields.iter().enumerate() {
        if index != 0 {
            append_form(&mut body, b"&")?;
        }
        form_component(&mut body, name)?;
        append_form(&mut body, b"=")?;
        form_component(&mut body, value)?;
    }
    Ok(body)
}
fn form_component(body: &mut Vec<u8>, text: &str) -> Result<(), ExchangeError> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"*.-_".contains(&byte) {
            append_form(body, &[byte])?;
        } else if byte == b' ' {
            append_form(body, b"+")?;
        } else {
            append_form(
                body,
                &[b'%', HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]],
            )?;
        }
    }
    Ok(())
}
fn append_form(body: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ExchangeError> {
    if body
        .len()
        .checked_add(bytes.len())
        .is_none_or(|len| len > MAX_FORM_BYTES)
    {
        return Err(ExchangeError::BodyLimit);
    }
    body.extend_from_slice(bytes);
    Ok(())
}
fn append_response(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), ExchangeError> {
    if body
        .len()
        .checked_add(chunk.len())
        .is_none_or(|len| len > MAX_RESPONSE_BYTES)
    {
        return Err(ExchangeError::BodyLimit);
    }
    body.extend_from_slice(chunk);
    Ok(())
}

fn single_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&HeaderValue>, ExchangeError> {
    let all = headers.get_all(name);
    let mut values = all.iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(ExchangeError::ResponseHeaders);
    }
    Ok(first)
}
fn validate_headers(headers: &HeaderMap) -> Result<(), ExchangeError> {
    let mut bytes = 0usize;
    for (index, (name, value)) in headers.iter().enumerate() {
        if index >= MAX_HEADER_FIELDS {
            return Err(ExchangeError::ResponseHeaders);
        }
        bytes = bytes
            .checked_add(name.as_str().len())
            .and_then(|n| n.checked_add(value.as_bytes().len()))
            .filter(|n| *n <= MAX_HEADER_BYTES)
            .ok_or(ExchangeError::ResponseHeaders)?;
    }
    let content_type = single_header(headers, CONTENT_TYPE)?
        .ok_or(ExchangeError::ResponseHeaders)?
        .to_str()
        .map_err(|_| ExchangeError::ResponseHeaders)?;
    if !json_content_type(content_type) {
        return Err(ExchangeError::ResponseHeaders);
    }
    if let Some(encoding) = single_header(headers, CONTENT_ENCODING)? {
        let text = encoding
            .to_str()
            .map_err(|_| ExchangeError::ContentEncoding)?;
        if !text
            .trim_matches([' ', '\t'])
            .eq_ignore_ascii_case("identity")
        {
            return Err(ExchangeError::ContentEncoding);
        }
    }
    if let Some(length) = single_header(headers, CONTENT_LENGTH)? {
        let text = length
            .to_str()
            .map_err(|_| ExchangeError::ResponseHeaders)?;
        let text = text.trim_matches([' ', '\t']);
        if text.is_empty() {
            return Err(ExchangeError::ResponseHeaders);
        }
        let mut count = 0usize;
        for byte in text.bytes() {
            if !byte.is_ascii_digit() {
                return Err(ExchangeError::ResponseHeaders);
            }
            count = count
                .checked_mul(10)
                .and_then(|n| n.checked_add((byte - b'0') as usize))
                .ok_or(ExchangeError::BodyLimit)?;
        }
        if count > MAX_RESPONSE_BYTES {
            return Err(ExchangeError::BodyLimit);
        }
    }
    Ok(())
}
fn json_content_type(value: &str) -> bool {
    let mut parts = value.split(';');
    if !parts
        .next()
        .unwrap_or("")
        .trim_matches([' ', '\t'])
        .eq_ignore_ascii_case("application/json")
    {
        return false;
    }
    let Some(parameter) = parts.next() else {
        return true;
    };
    if parts.next().is_some() {
        return false;
    }
    let Some((name, value)) = parameter.split_once('=') else {
        return false;
    };
    if !name
        .trim_matches([' ', '\t'])
        .eq_ignore_ascii_case("charset")
    {
        return false;
    }
    let value = value.trim_matches([' ', '\t']);
    let value = if let Some(quoted) = value.strip_prefix('"') {
        let Some(inner) = quoted.strip_suffix('"') else {
            return false;
        };
        inner
    } else {
        value
    };
    value.eq_ignore_ascii_case("utf-8")
}

#[cfg(test)]
mod tests {
    use super::super::pkce::{ApprovedScope, CallbackOutcome, PkceTransaction};
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    const CLIENT_ID: &str = "synthetic.apps.googleusercontent.com";
    const CODE: &str = "synthetic+/=%&?~code";
    fn attempt() -> ExchangeAttempt {
        ExchangeAttempt {
            id: Uuid::from_u128(42),
            generation: 7,
        }
    }
    fn grant() -> AuthorizationCodeGrant {
        let now = Instant::now();
        let mut tx = PkceTransaction::begin(
            CLIENT_ID,
            49152,
            &[
                ApprovedScope::AppData.as_str(),
                ApprovedScope::File.as_str(),
            ],
            now,
            Duration::from_secs(60),
        )
        .unwrap();
        let auth = Url::parse(tx.authorization_url_for_browser(now).unwrap()).unwrap();
        let state = auth
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let mut callback = Url::parse(tx.redirect_uri()).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("state", &state)
            .append_pair("code", CODE);
        let CallbackOutcome::Code(grant) = tx.accept_callback(callback.as_str(), now).unwrap()
        else {
            panic!("fixture code expected")
        };
        grant
    }
    fn limits() -> Limits {
        Limits {
            total: Duration::from_secs(3),
            connect: Duration::from_millis(500),
            read: Duration::from_millis(500),
        }
    }
    fn body() -> Vec<u8> {
        format!(r#"{{"access_token":"synthetic-access","token_type":"Bearer","expires_in":3600,"scope":"{}","refresh_token":"synthetic-refresh"}}"#, ApprovedScope::File.as_str()).into_bytes()
    }
    fn fixed(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n", body.len()).into_bytes();
        response.extend_from_slice(body);
        response
    }
    fn chunked(body: &[u8]) -> Vec<u8> {
        let mut response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        for chunk in body.chunks(1024) {
            response.extend_from_slice(format!("{:X}\r\n", chunk.len()).as_bytes());
            response.extend_from_slice(chunk);
            response.extend_from_slice(b"\r\n");
        }
        response.extend_from_slice(b"0\r\n\r\n");
        response
    }

    // Real loopback transport fixture. Every server has a stop flag, hard3s
    // thread lifetime, short socket timeouts and joined Drop, even on test panic.
    // At most4 captured synthetic requests, each at most16KiB; no real credentials.
    struct Server {
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        count: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<Vec<u8>>>>,
        worker: Option<JoinHandle<()>>,
    }
    impl Server {
        fn response(response: Vec<u8>) -> Self {
            Self::script(vec![(Duration::ZERO, response)])
        }
        fn script(pieces: Vec<(Duration, Vec<u8>)>) -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let count = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let (thread_stop, thread_count, thread_requests) =
                (stop.clone(), count.clone(), requests.clone());
            let worker = thread::spawn(move || {
                let end = Instant::now() + Duration::from_secs(3);
                while !thread_stop.load(Ordering::Acquire) && Instant::now() < end {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => break,
                    };
                    if thread_count.fetch_add(1, Ordering::AcqRel) >= 4 {
                        break;
                    }
                    if stream
                        .set_read_timeout(Some(Duration::from_millis(10)))
                        .is_err()
                        || stream
                            .set_write_timeout(Some(Duration::from_millis(10)))
                            .is_err()
                    {
                        break;
                    }
                    if let Some(request) = read_test_request(&mut stream, &thread_stop, end) {
                        if let Ok(mut requests) = thread_requests.lock() {
                            requests.push(request);
                        }
                        for (delay, bytes) in &pieces {
                            let until = Instant::now() + *delay;
                            while Instant::now() < until
                                && !thread_stop.load(Ordering::Acquire)
                                && Instant::now() < end
                            {
                                thread::sleep(Duration::from_millis(2));
                            }
                            if !write_test_bytes(&mut stream, bytes, &thread_stop, end) {
                                break;
                            }
                        }
                    }
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
            });
            Self {
                address,
                stop,
                count,
                requests,
                worker: Some(worker),
            }
        }
        fn count(&self) -> usize {
            self.count.load(Ordering::Acquire)
        }
        async fn wait_for_request(&self) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.requests.lock().unwrap().is_empty() {
                assert!(Instant::now() < deadline, "fixture request did not arrive");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    fn read_test_request(
        stream: &mut TcpStream,
        stop: &AtomicBool,
        end: Instant,
    ) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 1024];
        while !stop.load(Ordering::Acquire) && Instant::now() < end {
            match stream.read(&mut chunk) {
                Ok(0) => return None,
                Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(_) => return None,
            }
            if bytes.len() > 16 * 1024 {
                return None;
            }
            if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&bytes[..pos]).ok()?;
                let length = headers
                    .split("\r\n")
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                let required = (pos + 4).checked_add(length)?;
                if required > 16 * 1024 {
                    return None;
                }
                if bytes.len() >= required {
                    bytes.truncate(required);
                    return Some(bytes);
                }
            }
        }
        None
    }
    fn write_test_bytes(
        stream: &mut TcpStream,
        bytes: &[u8],
        stop: &AtomicBool,
        end: Instant,
    ) -> bool {
        let mut sent = 0;
        while sent < bytes.len() {
            if stop.load(Ordering::Acquire) || Instant::now() >= end {
                return false;
            }
            match stream.write(&bytes[sent..]) {
                Ok(0) => return false,
                Ok(n) => sent += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return false,
            }
        }
        !stop.load(Ordering::Acquire) && Instant::now() < end
    }
    async fn run(server: &Server) -> Result<ExchangeResult, ExchangeError> {
        let (_keep_sender, cancel) = oneshot::channel();
        exchange_at(
            grant(),
            attempt(),
            cancel,
            Target::Loopback(server.address),
            limits(),
        )
        .await
    }

    #[test]
    fn form_encoding_has_fixed_utf8_answers_and_hard_bound() {
        let mut encoded = Vec::new();
        form_component(&mut encoded, "A z+/%&=?~*.-_界").unwrap();
        assert_eq!(encoded, b"A+z%2B%2F%25%26%3D%3F%7E*.-_%E7%95%8C");
        let mut exact = vec![b'x'; MAX_FORM_BYTES - 1];
        form_component(&mut exact, "A").unwrap();
        assert_eq!(exact.len(), MAX_FORM_BYTES);
        assert_eq!(
            form_component(&mut exact, "B"),
            Err(ExchangeError::BodyLimit)
        );
        assert_eq!(exact.len(), MAX_FORM_BYTES);
    }

    #[tokio::test]
    async fn actual_post_has_exact_fields_privacy_and_partial_scope() {
        let server = Server::response(fixed("200 OK", "", &body()));
        let owned_grant = grant();
        let verifier = owned_grant.verifier_for_exchange().to_owned();
        let (_sender, receiver) = oneshot::channel();
        let result = exchange_at(
            owned_grant,
            attempt(),
            receiver,
            Target::Loopback(server.address),
            limits(),
        )
        .await
        .unwrap();
        assert_eq!(result.attempt().id, Uuid::from_u128(42));
        assert_eq!(result.attempt().generation, 7);
        assert_eq!(
            result.tokens_for_native_controller().granted_scopes(),
            &[ApprovedScope::File]
        );
        assert_eq!(
            result
                .tokens_for_native_controller()
                .access_token_for_native_adapter(),
            "synthetic-access"
        );
        assert_eq!(
            result
                .tokens_for_native_controller()
                .refresh_token_for_native_adapter(),
            Some("synthetic-refresh")
        );
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let request = std::str::from_utf8(&requests[0]).unwrap();
        let (headers, encoded) = request.split_once("\r\n\r\n").unwrap();
        assert!(headers.starts_with("POST /token HTTP/1.1\r\n"));
        let lower = headers.to_ascii_lowercase();
        assert!(
            lower
                .lines()
                .any(|line| line == "content-type: application/x-www-form-urlencoded")
        );
        assert!(
            lower
                .lines()
                .any(|line| line == "accept-encoding: identity")
        );
        for forbidden in [
            "authorization:",
            "cookie:",
            "referer:",
            "dpop:",
            "synthetic",
            "code_verifier",
        ] {
            assert!(!lower.contains(forbidden));
        }
        let expected = format!(
            "grant_type=authorization_code&code=synthetic%2B%2F%3D%25%26%3F%7Ecode&code_verifier={verifier}&client_id=synthetic.apps.googleusercontent.com&redirect_uri=http%3A%2F%2F127.0.0.1%3A49152%2Foauth2%2Fcallback%2Fgoogle"
        );
        assert_eq!(encoded, expected);
        assert_eq!(encoded.split('&').count(), 5);
        assert!(!encoded.contains("client_secret"));
        assert!(!encoded.contains("scope="));
    }

    #[tokio::test]
    async fn fixed_chunked_and_close_delimited_bodies_use_actual_parser() {
        let raw = body();
        let mut close_delimited = b"HTTP/1.1 200 OK\r\nContent-Type: APPLICATION/JSON; Charset=\"UTF-8\"\r\nConnection: close\r\n\r\n".to_vec();
        close_delimited.extend_from_slice(&raw);
        for response in [fixed("200 OK", "", &raw), chunked(&raw), close_delimited] {
            let server = Server::response(response);
            assert!(run(&server).await.is_ok());
        }
        for raw in [
            b"{}".to_vec(),
            b"".to_vec(),
            body()
                .into_iter()
                .chain(b" trailing".iter().copied())
                .collect(),
        ] {
            let server = Server::response(fixed("200 OK", "", &raw));
            assert!(matches!(
                run(&server).await,
                Err(ExchangeError::TokenResponse(_))
            ));
        }
        let duplicate = String::from_utf8(body()).unwrap().replace(
            "\"token_type\":",
            "\"access_token\":\"other\",\"token_type\":",
        );
        let server = Server::response(fixed("200 OK", "", duplicate.as_bytes()));
        assert!(matches!(
            run(&server).await,
            Err(ExchangeError::TokenResponse(_))
        ));
    }

    #[tokio::test]
    async fn exact_response_cap_and_overflow_with_or_without_length() {
        let mut exact = body();
        exact.resize(MAX_RESPONSE_BYTES, b' ');
        let server = Server::response(chunked(&exact));
        assert!(run(&server).await.is_ok());
        let mut too_large = exact.clone();
        too_large.push(b' ');
        for response in [chunked(&too_large), fixed("200 OK", "", &too_large)] {
            let server = Server::response(response);
            assert!(matches!(run(&server).await, Err(ExchangeError::BodyLimit)));
        }
        let mut accumulator = vec![0; MAX_RESPONSE_BYTES];
        assert_eq!(
            append_response(&mut accumulator, b"x"),
            Err(ExchangeError::BodyLimit)
        );
        assert_eq!(accumulator.len(), MAX_RESPONSE_BYTES);
    }

    #[tokio::test]
    async fn truncated_and_invalid_chunk_framing_never_parse_prefix() {
        let raw = body();
        let mut truncated = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", raw.len()+10).into_bytes();
        truncated.extend_from_slice(&raw);
        let invalid = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\nZZ\r\n{}\r\n0\r\n\r\n".to_vec();
        for response in [truncated, invalid] {
            let server = Server::response(response);
            assert!(matches!(run(&server).await, Err(ExchangeError::Transport)));
        }
    }

    #[tokio::test]
    async fn response_header_and_encoding_policy_is_enforced_over_transport() {
        for extra in [
            "Content-Type: application/json\r\n",
            "Content-Encoding: gzip\r\n",
            "Content-Encoding: br\r\n",
            "Content-Encoding: identity, identity\r\n",
            "Content-Encoding: identity\r\nContent-Encoding: identity\r\n",
        ] {
            let server = Server::response(fixed("200 OK", extra, &body()));
            assert!(matches!(
                run(&server).await,
                Err(ExchangeError::ResponseHeaders | ExchangeError::ContentEncoding)
            ));
        }
        for kind in [
            "text/html",
            "application/json; charset=latin1",
            "application/json; charset=utf-8; charset=utf-8",
        ] {
            let response = String::from_utf8(fixed("200 OK", "", &body()))
                .unwrap()
                .replace(
                    "Content-Type: application/json",
                    &format!("Content-Type: {kind}"),
                );
            let server = Server::response(response.into_bytes());
            assert!(matches!(
                run(&server).await,
                Err(ExchangeError::ResponseHeaders)
            ));
        }
        for extra in [
            format!("Content-Length: {}\r\n", body().len()),
            format!("X-Pad: {}\r\n", "x".repeat(MAX_HEADER_BYTES)),
            "X-Many: x\r\n".repeat(MAX_HEADER_FIELDS),
        ] {
            let server = Server::response(fixed("200 OK", &extra, &body()));
            assert!(matches!(
                run(&server).await,
                Err(ExchangeError::ResponseHeaders)
            ));
        }
        let server = Server::response(fixed("200 OK", "Content-Encoding: identity\r\n", &body()));
        assert!(run(&server).await.is_ok());
    }

    #[test]
    fn header_count_bytes_length_and_duplicates_have_explicit_bounds() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        for length in ["65537", "99999999999999999999999999999999999"] {
            headers.insert(CONTENT_LENGTH, HeaderValue::from_str(length).unwrap());
            assert_eq!(validate_headers(&headers), Err(ExchangeError::BodyLimit));
        }
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("2, 2"));
        assert_eq!(
            validate_headers(&headers),
            Err(ExchangeError::ResponseHeaders)
        );
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("2"));
        headers.append(CONTENT_LENGTH, HeaderValue::from_static("2"));
        assert_eq!(
            validate_headers(&headers),
            Err(ExchangeError::ResponseHeaders)
        );
        headers.remove(CONTENT_LENGTH);
        headers.insert(
            "x-pad",
            HeaderValue::from_str(&"x".repeat(MAX_HEADER_BYTES)).unwrap(),
        );
        assert_eq!(
            validate_headers(&headers),
            Err(ExchangeError::ResponseHeaders)
        );
        headers.remove("x-pad");
        for _ in 0..MAX_HEADER_FIELDS {
            headers.append("x-many", HeaderValue::from_static("x"));
        }
        assert_eq!(
            validate_headers(&headers),
            Err(ExchangeError::ResponseHeaders)
        );
        for content_type in [
            "application/json",
            " Application/JSON ; charset = \"utf-8\" ",
        ] {
            assert!(json_content_type(content_type));
        }
        for content_type in [
            "application/json;",
            "application/json; charset=\"utf-8",
            "application/json; other=utf-8",
        ] {
            assert!(!json_content_type(content_type));
        }
    }

    #[tokio::test]
    async fn redirect_does_not_forward_and_disconnect_does_not_replay() {
        let capture = Server::response(fixed("200 OK", "", &body()));
        let redirect = Server::response(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/capture?synthetic-private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", capture.address).into_bytes());
        assert!(matches!(run(&redirect).await, Err(ExchangeError::Redirect)));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(redirect.count(), 1);
        assert_eq!(capture.count(), 0);
        let disconnect = Server::script(Vec::new());
        assert!(matches!(
            run(&disconnect).await,
            Err(ExchangeError::Transport)
        ));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(disconnect.count(), 1);
    }

    #[tokio::test]
    async fn pre_signaled_or_closed_cancellation_prevents_dispatch() {
        for signal in [true, false] {
            let server = Server::response(fixed("200 OK", "", &body()));
            let (sender, cancel) = oneshot::channel();
            if signal {
                sender.send(()).unwrap();
            } else {
                drop(sender);
            }
            assert!(matches!(
                exchange_at(
                    grant(),
                    attempt(),
                    cancel,
                    Target::Loopback(server.address),
                    limits()
                )
                .await,
                Err(ExchangeError::Cancelled)
            ));
            assert_eq!(server.count(), 0);
        }
    }

    #[tokio::test]
    async fn mid_body_signal_and_sender_drop_cancel_owned_future() {
        for signal in [true, false] {
            let server = Server::script(vec![
                (Duration::ZERO, b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999\r\n\r\n".to_vec()),
                (Duration::from_secs(2), b"never-used".to_vec()),
            ]);
            let (sender, cancel) = oneshot::channel();
            let exchange = exchange_at(
                grant(),
                attempt(),
                cancel,
                Target::Loopback(server.address),
                limits(),
            );
            let cancellation = async {
                server.wait_for_request().await;
                tokio::time::sleep(Duration::from_millis(20)).await;
                if signal {
                    let _ = sender.send(());
                } else {
                    drop(sender);
                }
            };
            let (result, ()) = tokio::join!(exchange, cancellation);
            assert!(matches!(result, Err(ExchangeError::Cancelled)));
            assert_eq!(server.count(), 1);
        }
    }

    #[tokio::test]
    async fn header_and_body_stalls_and_drips_obey_deadlines() {
        let header =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999\r\n\r\n"
                .to_vec();
        for pieces in [
            vec![(Duration::from_secs(2), header.clone())],
            vec![
                (Duration::ZERO, header.clone()),
                (Duration::from_secs(2), b"x".to_vec()),
            ],
        ] {
            let server = Server::script(pieces);
            let (_sender, cancel) = oneshot::channel();
            let constrained = Limits {
                total: Duration::from_secs(2),
                connect: Duration::from_millis(500),
                read: Duration::from_millis(100),
            };
            assert!(matches!(
                exchange_at(
                    grant(),
                    attempt(),
                    cancel,
                    Target::Loopback(server.address),
                    constrained
                )
                .await,
                Err(ExchangeError::Deadline)
            ));
            assert_eq!(server.count(), 1);
        }
        let mut drip = vec![(Duration::ZERO, header)];
        drip.extend((0..30).map(|_| (Duration::from_millis(40), b" ".to_vec())));
        let server = Server::script(drip);
        let (_sender, cancel) = oneshot::channel();
        let constrained = Limits {
            total: Duration::from_millis(500),
            connect: Duration::from_millis(300),
            read: Duration::from_millis(150),
        };
        assert!(matches!(
            exchange_at(
                grant(),
                attempt(),
                cancel,
                Target::Loopback(server.address),
                constrained
            )
            .await,
            Err(ExchangeError::Deadline)
        ));
        assert_eq!(server.count(), 1);
    }

    fn parsed_fixture() -> ParsedTokenResponse {
        let now = Instant::now();
        parse_google_token_response(200, &body(), &[ApprovedScope::File], now, now).unwrap()
    }
    #[test]
    fn final_gate_checks_cancel_deadline_expiry_and_metadata_handoff() {
        let (sender, mut cancel) = oneshot::channel();
        sender.send(()).unwrap();
        assert!(matches!(
            finish_result(
                parsed_fixture(),
                attempt(),
                &mut cancel,
                Instant::now() + Duration::from_secs(1)
            ),
            Err(ExchangeError::Cancelled)
        ));
        let (_sender, mut cancel) = oneshot::channel();
        assert!(matches!(
            finish_result(parsed_fixture(), attempt(), &mut cancel, Instant::now()),
            Err(ExchangeError::Deadline)
        ));
        let now = Instant::now();
        let one_second = String::from_utf8(body()).unwrap().replace("3600", "1");
        let already_expired = parse_google_token_response(
            200,
            one_second.as_bytes(),
            &[ApprovedScope::File],
            now - Duration::from_secs(2),
            now - Duration::from_secs(2),
        )
        .unwrap();
        assert!(matches!(
            finish_result(
                already_expired,
                attempt(),
                &mut cancel,
                now + Duration::from_secs(1)
            ),
            Err(ExchangeError::TokenResponse(
                TokenResponseError::ExpiredAtReceipt
            ))
        ));
        let result = finish_result(
            parsed_fixture(),
            attempt(),
            &mut cancel,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        // Caller contract fixture only: the real controller/commit lock is later.
        let active_id = Uuid::from_u128(43);
        let active_generation = 8;
        assert!(
            !(result.attempt().id == active_id && result.attempt().generation == active_generation)
        );
        assert_eq!(result.attempt().id, Uuid::from_u128(42));
    }

    #[tokio::test]
    async fn typed_provider_errors_and_transport_policy_do_not_echo_secrets() {
        let error_body = br#"{"error":"invalid_grant","error_description":"synthetic-private","error_uri":"https://example.invalid/private"}"#;
        let server = Server::response(fixed("400 Bad Request", "", error_body));
        let error = run(&server).await.err().unwrap();
        assert!(matches!(
            error,
            ExchangeError::TokenResponse(TokenResponseError::Provider { .. })
        ));
        for text in [format!("{error}"), format!("{error:?}")] {
            for secret in [
                CODE,
                "synthetic-private",
                "example.invalid",
                "synthetic-access",
            ] {
                assert!(!text.contains(secret));
            }
            assert!(text.len() < 200);
        }
        let target = Target::Google;
        let url = target.url().unwrap();
        assert_eq!(url.as_str(), TOKEN_ENDPOINT);
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
        assert!(url.username().is_empty());
        assert!(target.https_only());
        assert_eq!(Limits::PRODUCTION.total, Duration::from_secs(30));
        assert_eq!(Limits::PRODUCTION.connect, Duration::from_secs(5));
        assert_eq!(Limits::PRODUCTION.read, Duration::from_secs(5));
    }
}

#[cfg(test)]
#[path = "exchange_independent_tests.rs"]
mod independent_tests;
