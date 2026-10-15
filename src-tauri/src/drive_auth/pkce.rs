//! A single native desktop Google PKCE transaction, not an OAuth client.
//!
//! The caller must first bind a listener exclusively to 127.0.0.1, then supply
//! its actual port. A later listener must validate GET, local socket/Host and raw
//! request-target before constructing the absolute callback URL; this helper
//! does not authenticate an HTTP request or trust forwarded headers. Never pass
//! callback URLs, authorization URLs, codes or verifiers through JS or logs.
//!
//! No browser, HTTP listener, token exchange, credentials store or account proof
//! exists here. Payload-bearing types intentionally have no Debug/Display/serde
//! or Clone implementation. Explicit native secret access is for a later token
//! adapter only. Dropping/clearing Strings is NOT guaranteed memory zeroization.

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRng, rngs::SysRng};
use reqwest::Url;
use sha2::{Digest, Sha256};

pub const GOOGLE_AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const CALLBACK_PATH: &str = "/oauth2/callback/google";
pub const MAX_LIFETIME: Duration = Duration::from_secs(600);
pub const MAX_CALLBACK_BYTES: usize = 8192;
const MAX_CODE_BYTES: usize = 2048;
const MAX_QUERY_PAIRS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedScope {
    AppData,
    File,
}
impl ApprovedScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppData => "https://www.googleapis.com/auth/drive.appdata",
            Self::File => "https://www.googleapis.com/auth/drive.file",
        }
    }
    fn parse(value: &str) -> Result<Self, PkceError> {
        match value {
            "https://www.googleapis.com/auth/drive.appdata" => Ok(Self::AppData),
            "https://www.googleapis.com/auth/drive.file" => Ok(Self::File),
            _ => Err(PkceError::InvalidScopes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceError {
    InvalidClientId,
    InvalidPort,
    InvalidLifetime,
    InvalidScopes,
    EntropyUnavailable,
    InvalidCallback,
    CallbackLimit,
    CallbackBindingMismatch,
    DuplicateParameter,
    StateMismatch,
    AmbiguousResponse,
    InvalidCode,
    InvalidProviderError,
    IssuerMismatch,
    ScopeMismatch,
    Expired,
    ClockRegression,
    Finished,
}
impl fmt::Display for PkceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No received input or inner URL/RNG error may be embedded here.
        write!(f, "PKCE transaction error: {self:?}")
    }
}
impl std::error::Error for PkceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    CodeReceived,
    Denied,
    ProviderRejected,
    Cancelled,
    Expired,
    ClockFailed,
}

struct Secrets {
    state: String,
    verifier: String,
}

/// Code and matching verifier are native-only. This is NOT authenticated identity
/// or an access token. The eventual exchange must verify granted scopes/account.
pub struct AuthorizationCodeGrant {
    code: String,
    verifier: String,
    client_id: String,
    redirect_uri: String,
    requested_scopes: Vec<ApprovedScope>,
}
impl AuthorizationCodeGrant {
    pub fn code_for_exchange(&self) -> &str {
        &self.code
    }
    pub fn verifier_for_exchange(&self) -> &str {
        &self.verifier
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }
    pub fn requested_scopes(&self) -> &[ApprovedScope] {
        &self.requested_scopes
    }
}
pub enum CallbackOutcome {
    Code(AuthorizationCodeGrant),
    Denied,
    ProviderRejected,
}

pub struct PkceTransaction {
    client_id: String,
    port: u16,
    redirect_uri: String,
    authorization_url: String,
    requested_scopes: Vec<ApprovedScope>,
    secrets: Option<Secrets>,
    last_observed: Instant,
    deadline: Instant,
    status: TransactionStatus,
}
impl PkceTransaction {
    /// The port is a caller assertion about an already-bound IPv4 loopback socket.
    /// Requested scopes are exact, not an incremental/previous-grant union.
    pub fn begin(
        client_id: &str,
        port: u16,
        scopes: &[&str],
        now: Instant,
        lifetime: Duration,
    ) -> Result<Self, PkceError> {
        Self::with_entropy(client_id, port, scopes, now, lifetime, |bytes| {
            SysRng.try_fill_bytes(bytes).map_err(|_| ())
        })
    }

    // A private deterministic/failing entropy seam for tests; no public verifier injection.
    fn with_entropy<F>(
        client_id: &str,
        port: u16,
        scopes: &[&str],
        now: Instant,
        lifetime: Duration,
        fill: F,
    ) -> Result<Self, PkceError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ()>,
    {
        if client_id.len() <= ".apps.googleusercontent.com".len()
            || client_id.len() > 256
            || !client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            || !client_id.ends_with(".apps.googleusercontent.com")
        {
            return Err(PkceError::InvalidClientId);
        }
        if port == 0 {
            return Err(PkceError::InvalidPort);
        }
        if lifetime.is_zero() || lifetime > MAX_LIFETIME {
            return Err(PkceError::InvalidLifetime);
        }
        let deadline = now
            .checked_add(lifetime)
            .ok_or(PkceError::InvalidLifetime)?;
        let requested_scopes = parse_scopes(scopes)?;
        let mut entropy = [0u8; 64];
        fill(&mut entropy).map_err(|_| PkceError::EntropyUnavailable)?;
        // Two disjoint 32-byte draws: each base64url value is 43 unreserved chars.
        let state = URL_SAFE_NO_PAD.encode(&entropy[..32]);
        let verifier = URL_SAFE_NO_PAD.encode(&entropy[32..]);
        entropy.fill(0); // Best effort only; not a zeroization guarantee.
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
        let mut url =
            Url::parse(GOOGLE_AUTHORIZATION_ENDPOINT).map_err(|_| PkceError::InvalidCallback)?;
        let scope = requested_scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &s256_challenge(&verifier))
            .append_pair("code_challenge_method", "S256");
        Ok(Self {
            client_id: client_id.to_owned(),
            port,
            redirect_uri,
            authorization_url: url.to_string(),
            requested_scopes,
            secrets: Some(Secrets { state, verifier }),
            last_observed: now,
            deadline,
            status: TransactionStatus::Pending,
        })
    }

    /// Native browser-launch boundary only; the returned URL contains state.
    pub fn authorization_url_for_browser(&mut self, now: Instant) -> Result<&str, PkceError> {
        self.check_active(now)?;
        Ok(&self.authorization_url)
    }
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }
    pub fn status(&self) -> TransactionStatus {
        self.status
    }
    /// Explicit monotonic time; no wall-clock reads or timeout extension on callbacks.
    pub fn check_active(&mut self, now: Instant) -> Result<(), PkceError> {
        if self.status != TransactionStatus::Pending {
            return Err(PkceError::Finished);
        }
        if now < self.last_observed {
            self.finish(TransactionStatus::ClockFailed);
            return Err(PkceError::ClockRegression);
        }
        if now >= self.deadline {
            self.finish(TransactionStatus::Expired);
            return Err(PkceError::Expired);
        }
        self.last_observed = now;
        Ok(())
    }
    pub fn cancel(&mut self, now: Instant) -> Result<(), PkceError> {
        self.check_active(now)?;
        self.finish(TransactionStatus::Cancelled);
        Ok(())
    }
    fn finish(&mut self, status: TransactionStatus) {
        self.secrets.take();
        self.authorization_url.clear();
        self.status = status;
    }

    /// Invalid/mismatched callbacks do not consume the pending transaction.
    /// A valid matching code or provider error consumes it exactly once. Expiry,
    /// local cancellation and clock regression are also terminal. Replay fails.
    pub fn accept_callback(
        &mut self,
        raw: &str,
        now: Instant,
    ) -> Result<CallbackOutcome, PkceError> {
        self.check_active(now)?;
        let params = self.callback_parameters(raw)?;
        let state = params.get("state").ok_or(PkceError::StateMismatch)?;
        let secrets = self.secrets.as_ref().ok_or(PkceError::Finished)?;
        if !state_matches(state.as_bytes(), secrets.state.as_bytes()) {
            return Err(PkceError::StateMismatch);
        }
        let code = params.get("code");
        let error = params.get("error");
        if code.is_some() == error.is_some() {
            return Err(PkceError::AmbiguousResponse);
        }
        if params
            .get("iss")
            .is_some_and(|v| v != "https://accounts.google.com")
        {
            return Err(PkceError::IssuerMismatch);
        }
        if let Some(scope) = params.get("scope") {
            let fields: Vec<_> = scope.split_ascii_whitespace().collect();
            let reported = parse_scopes(&fields).map_err(|_| PkceError::ScopeMismatch)?;
            if reported.iter().any(|s| !self.requested_scopes.contains(s)) {
                return Err(PkceError::ScopeMismatch);
            }
            // This untrusted response hint never establishes actual granted scopes.
        }
        if let Some(code) = code {
            if code.is_empty()
                || code.len() > MAX_CODE_BYTES
                || !code.bytes().all(|b| (0x21..=0x7e).contains(&b))
            {
                return Err(PkceError::InvalidCode);
            }
            let secrets = self.secrets.take().ok_or(PkceError::Finished)?;
            let grant = AuthorizationCodeGrant {
                code: code.clone(),
                verifier: secrets.verifier,
                client_id: self.client_id.clone(),
                redirect_uri: self.redirect_uri.clone(),
                requested_scopes: self.requested_scopes.clone(),
            };
            self.finish(TransactionStatus::CodeReceived);
            return Ok(CallbackOutcome::Code(grant));
        }
        let error = error.ok_or(PkceError::AmbiguousResponse)?;
        if error.is_empty()
            || error.len() > 128
            || !error
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(PkceError::InvalidProviderError);
        }
        if error == "access_denied" {
            self.finish(TransactionStatus::Denied);
            Ok(CallbackOutcome::Denied)
        } else {
            self.finish(TransactionStatus::ProviderRejected);
            Ok(CallbackOutcome::ProviderRejected)
        }
    }

    fn callback_parameters(&self, raw: &str) -> Result<BTreeMap<String, String>, PkceError> {
        if raw.len() > MAX_CALLBACK_BYTES {
            return Err(PkceError::CallbackLimit);
        }
        // Reject URL parser repairs: controls, backslashes, whitespace and literal
        // non-ASCII must not disappear or normalize before binding checks.
        if !raw.is_ascii()
            || raw
                .bytes()
                .any(|b| b.is_ascii_control() || b.is_ascii_whitespace() || b == b'\\')
        {
            return Err(PkceError::InvalidCallback);
        }
        let parsed = Url::parse(raw).map_err(|_| PkceError::InvalidCallback)?;
        if parsed.fragment().is_some() {
            return Err(PkceError::InvalidCallback);
        }
        let (binding, query) = raw.split_once('?').ok_or(PkceError::InvalidCallback)?;
        // Raw equality rejects 127.1/integer hosts, userinfo, default-port aliases,
        // percent-encoded or dot-normalized paths, and a different transaction port.
        if binding != self.redirect_uri
            || parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.port_or_known_default() != Some(self.port)
            || parsed.path() != CALLBACK_PATH
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(PkceError::CallbackBindingMismatch);
        }
        let mut params = BTreeMap::new();
        for (index, pair) in query.split('&').enumerate() {
            if index >= MAX_QUERY_PAIRS {
                return Err(PkceError::CallbackLimit);
            }
            let (key, value) = pair.split_once('=').ok_or(PkceError::InvalidCallback)?;
            let key = decode_form_component(key)?;
            let value = decode_form_component(value)?;
            if key.len() > 64 || value.len() > 4096 {
                return Err(PkceError::CallbackLimit);
            }
            // RFC6749 section 8.2 defines extension parameter names.
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
            {
                return Err(PkceError::InvalidCallback);
            }
            // RFC6749 section 4.1.2: ignore unrecognized response parameters.
            // Keep them in this bounded temporary map solely to detect duplicates;
            // accept_callback reads only explicitly supported semantic keys.
            if params.insert(key, value).is_some() {
                return Err(PkceError::DuplicateParameter);
            }
        }
        Ok(params)
    }
}

fn parse_scopes(scopes: &[&str]) -> Result<Vec<ApprovedScope>, PkceError> {
    if scopes.is_empty() || scopes.len() > 2 {
        return Err(PkceError::InvalidScopes);
    }
    let mut result = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let scope = ApprovedScope::parse(scope)?;
        if result.contains(&scope) {
            return Err(PkceError::InvalidScopes);
        }
        result.push(scope);
    }
    Ok(result)
}
fn s256_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn state_matches(actual: &[u8], expected: &[u8]) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

// Url's form decoder is intentionally tolerant; require valid percent triplets
// and UTF-8 instead of lossy replacement, and decode exactly once. '+' is space.
fn decode_form_component(raw: &str) -> Result<String, PkceError> {
    let mut output = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let high = *bytes.get(i + 1).ok_or(PkceError::InvalidCallback)?;
                let low = *bytes.get(i + 2).ok_or(PkceError::InvalidCallback)?;
                output.push((hex_digit(high)? << 4) | hex_digit(low)?);
                i += 3;
            }
            b'+' => {
                output.push(b' ');
                i += 1;
            }
            byte => {
                output.push(byte);
                i += 1;
            }
        }
    }
    let text = String::from_utf8(output).map_err(|_| PkceError::InvalidCallback)?;
    if text.chars().any(char::is_control) {
        return Err(PkceError::InvalidCallback);
    }
    Ok(text)
}
fn hex_digit(byte: u8) -> Result<u8, PkceError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PkceError::InvalidCallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CLIENT: &str = "synthetic.apps.googleusercontent.com";
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    fn fill_fixture(bytes: &mut [u8]) -> Result<(), ()> {
        // RFC7636 Appendix B's published 32 octets, no expected-value generator.
        let verifier = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212,
            37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        if bytes.len() != 64 {
            return Err(());
        }
        bytes[..32].fill(5);
        bytes[32..].copy_from_slice(&verifier);
        Ok(())
    }
    fn transaction(now: Instant) -> PkceTransaction {
        PkceTransaction::with_entropy(
            CLIENT,
            49152,
            &[ApprovedScope::AppData.as_str()],
            now,
            Duration::from_secs(60),
            fill_fixture,
        )
        .unwrap()
    }
    fn callback(tx: &PkceTransaction, tail: &str) -> String {
        format!(
            "{}?state={}&{tail}",
            tx.redirect_uri(),
            tx.secrets.as_ref().unwrap().state
        )
    }
    fn rejected(tx: &mut PkceTransaction, url: &str, now: Instant) -> PkceError {
        match tx.accept_callback(url, now) {
            Err(error) => error,
            Ok(_) => panic!("unexpected callback acceptance"),
        }
    }

    #[test]
    fn rfc7636_challenge_and_generated_authorization_binding() {
        assert_eq!(s256_challenge(VERIFIER), CHALLENGE);
        let now = Instant::now();
        let mut tx = transaction(now);
        let url = Url::parse(tx.authorization_url_for_browser(now).unwrap()).unwrap();
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://accounts.google.com"
        );
        assert_eq!(url.path(), "/o/oauth2/v2/auth");
        let query: BTreeMap<_, _> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(query.len(), 7);
        assert_eq!(query["client_id"], CLIENT);
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge"], CHALLENGE);
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(
            query["redirect_uri"],
            "http://127.0.0.1:49152/oauth2/callback/google"
        );
        assert_eq!(query["scope"], ApprovedScope::AppData.as_str());
        assert_eq!(query["state"].len(), 43);
        assert!(!url.as_str().contains(VERIFIER));
        assert_ne!(query["state"], VERIFIER);
    }

    #[test]
    fn scopes_are_exact_and_broad_unknown_empty_duplicate_sets_fail() {
        let now = Instant::now();
        for scopes in [
            vec![],
            vec![""],
            vec!["https://www.googleapis.com/auth/drive"],
            vec!["https://www.googleapis.com/auth/drive.readonly"],
            vec!["openid"],
            vec![ApprovedScope::File.as_str(), ApprovedScope::File.as_str()],
        ] {
            let result = PkceTransaction::with_entropy(
                CLIENT,
                49152,
                &scopes,
                now,
                Duration::from_secs(60),
                fill_fixture,
            );
            assert!(matches!(result, Err(PkceError::InvalidScopes)));
        }
        for scopes in [
            vec![ApprovedScope::File.as_str()],
            vec![
                ApprovedScope::AppData.as_str(),
                ApprovedScope::File.as_str(),
            ],
        ] {
            let mut tx = PkceTransaction::with_entropy(
                CLIENT,
                49152,
                &scopes,
                now,
                Duration::from_secs(60),
                fill_fixture,
            )
            .unwrap();
            let url = Url::parse(tx.authorization_url_for_browser(now).unwrap()).unwrap();
            assert_eq!(
                url.query_pairs().find(|(k, _)| k == "scope").unwrap().1,
                scopes.join(" ")
            );
            assert!(
                !url.query_pairs()
                    .any(|(k, _)| k == "include_granted_scopes")
            );
        }
    }

    #[test]
    fn invalid_configuration_and_entropy_failure_are_typed() {
        let now = Instant::now();
        for client in [
            "",
            "client?scope=anything",
            "https://example.invalid",
            "wrong.example",
            ".apps.googleusercontent.com",
        ] {
            assert!(matches!(
                PkceTransaction::with_entropy(
                    client,
                    49152,
                    &[ApprovedScope::File.as_str()],
                    now,
                    Duration::from_secs(60),
                    fill_fixture
                ),
                Err(PkceError::InvalidClientId)
            ));
        }
        assert!(matches!(
            PkceTransaction::with_entropy(
                CLIENT,
                0,
                &[ApprovedScope::File.as_str()],
                now,
                Duration::from_secs(60),
                fill_fixture
            ),
            Err(PkceError::InvalidPort)
        ));
        for duration in [Duration::ZERO, MAX_LIFETIME + Duration::from_nanos(1)] {
            assert!(matches!(
                PkceTransaction::with_entropy(
                    CLIENT,
                    49152,
                    &[ApprovedScope::File.as_str()],
                    now,
                    duration,
                    fill_fixture
                ),
                Err(PkceError::InvalidLifetime)
            ));
        }
        assert!(matches!(
            PkceTransaction::with_entropy(
                CLIENT,
                49152,
                &[ApprovedScope::File.as_str()],
                now,
                Duration::from_secs(60),
                |_| Err(())
            ),
            Err(PkceError::EntropyUnavailable)
        ));
    }

    #[test]
    fn valid_code_moves_matching_verifier_and_cannot_be_replayed() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let url = callback(&tx, "code=4%2Fsynthetic%2Bcode");
        let CallbackOutcome::Code(grant) = tx.accept_callback(&url, now).unwrap() else {
            panic!("expected code");
        };
        assert_eq!(grant.code_for_exchange(), "4/synthetic+code");
        assert_eq!(grant.verifier_for_exchange(), VERIFIER);
        assert_eq!(grant.client_id(), CLIENT);
        assert_eq!(grant.redirect_uri(), tx.redirect_uri());
        assert_eq!(grant.requested_scopes(), &[ApprovedScope::AppData]);
        assert_eq!(tx.status(), TransactionStatus::CodeReceived);
        assert!(tx.secrets.is_none());
        assert!(tx.authorization_url.is_empty());
        assert_eq!(rejected(&mut tx, &url, now), PkceError::Finished);
        assert_eq!(
            tx.authorization_url_for_browser(now).err(),
            Some(PkceError::Finished)
        );
    }

    #[test]
    fn mismatch_does_not_consume_but_matching_provider_errors_do() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let wrong = format!("{}?state=wrong&error=access_denied", tx.redirect_uri());
        assert_eq!(rejected(&mut tx, &wrong, now), PkceError::StateMismatch);
        assert_eq!(tx.status(), TransactionStatus::Pending);
        let denied = callback(
            &tx,
            "error=access_denied&error_description=synthetic%20detail",
        );
        assert!(matches!(
            tx.accept_callback(&denied, now),
            Ok(CallbackOutcome::Denied)
        ));
        assert_eq!(tx.status(), TransactionStatus::Denied);
        assert_eq!(rejected(&mut tx, &denied, now), PkceError::Finished);
        let mut tx = transaction(now);
        let other = callback(
            &tx,
            "error=server_error&error_uri=https%3A%2F%2Fexample.invalid",
        );
        assert!(matches!(
            tx.accept_callback(&other, now),
            Ok(CallbackOutcome::ProviderRejected)
        ));
        assert!(tx.secrets.is_none());
    }

    #[test]
    fn exact_raw_loopback_origin_path_port_and_no_fragment_are_required() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let good = callback(&tx, "code=synthetic");
        for bad in [
            good.replace("127.0.0.1", "localhost"),
            good.replace("127.0.0.1", "127.1"),
            good.replace("127.0.0.1", "2130706433"),
            good.replace("http:", "https:"),
            good.replace(":49152", ":49153"),
            good.replace("127.0.0.1", "user@127.0.0.1"),
            good.replace(CALLBACK_PATH, "/other"),
            good.replace(CALLBACK_PATH, "/x/../oauth2/callback/google"),
            good.replace(CALLBACK_PATH, "/oauth2/callback/%67oogle"),
            format!("{good}#"),
            format!("{good}#code=other"),
            format!(" {good}"),
            good.replace("/oauth2", "\\oauth2"),
        ] {
            assert_ne!(bad, good);
            assert!(tx.accept_callback(&bad, now).is_err());
            assert_eq!(tx.status(), TransactionStatus::Pending);
        }
        assert!(matches!(
            tx.accept_callback(&good, now),
            Ok(CallbackOutcome::Code(_))
        ));
    }

    #[test]
    fn decoded_duplicate_keys_and_ambiguous_responses_fail_without_consumption() {
        let now = Instant::now();
        let mut tx = transaction(now);
        for tail in [
            "code=a&code=b",
            "code=a&c%6fde=b",
            "code=a&%73tate=wrong",
            "error=access_denied&%65rror=other",
        ] {
            let url = callback(&tx, tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::DuplicateParameter);
            assert_eq!(tx.status(), TransactionStatus::Pending);
        }
        for tail in ["code=a&error=access_denied", "code=&error=", "authuser=0"] {
            let url = callback(&tx, tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::AmbiguousResponse);
        }
    }

    #[test]
    fn malformed_percent_utf8_controls_and_code_fields_fail() {
        let now = Instant::now();
        let mut tx = transaction(now);
        for tail in [
            "code=%",
            "code=%2",
            "code=%GG",
            "code=%FF",
            "code=%00",
            "code=%0D%0A",
            "code=abc&",
            "code=abc&bare",
            "code=abc&unknown=%GG",
            "code=abc&unknown=%FF",
            "code=abc&unknown=%00",
            "code=abc&=x",
            "code=abc&unknown%20key=x",
            "code=abc&%E7%95%8C=x",
        ] {
            let url = callback(&tx, tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::InvalidCallback);
        }
        for tail in ["code=", "code=+", "code=%20", "code=%E7%95%8C"] {
            let url = callback(&tx, tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::InvalidCode);
        }
        for tail in ["error=", "error=a%20b"] {
            let url = callback(&tx, tail);
            assert_eq!(
                rejected(&mut tx, &url, now),
                PkceError::InvalidProviderError
            );
        }
        let url = callback(&tx, "code=%252Fsynthetic");
        let CallbackOutcome::Code(grant) = tx.accept_callback(&url, now).unwrap() else {
            panic!("expected code");
        };
        assert_eq!(grant.code_for_exchange(), "%2Fsynthetic");
    }

    #[test]
    fn unknown_extensions_are_ignored_for_code_and_denial() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let url = callback(
            &tx,
            "code=synthetic&provider_extension=synthetic&granted_scope=broad&account_id=foreign",
        );
        let CallbackOutcome::Code(grant) = tx.accept_callback(&url, now).unwrap() else {
            panic!("expected code");
        };
        assert_eq!(grant.code_for_exchange(), "synthetic");
        assert_eq!(grant.verifier_for_exchange(), VERIFIER);
        assert_eq!(grant.client_id(), CLIENT);
        assert_eq!(grant.redirect_uri(), tx.redirect_uri());
        assert_eq!(grant.requested_scopes(), &[ApprovedScope::AppData]);
        assert_eq!(tx.status(), TransactionStatus::CodeReceived);
        assert_eq!(rejected(&mut tx, &url, now), PkceError::Finished);
        let mut tx = transaction(now);
        let url = callback(&tx, "error=access_denied&provider_extension=synthetic");
        assert!(matches!(
            tx.accept_callback(&url, now),
            Ok(CallbackOutcome::Denied)
        ));
        assert_eq!(tx.status(), TransactionStatus::Denied);
        assert_eq!(rejected(&mut tx, &url, now), PkceError::Finished);
    }

    #[test]
    fn ignored_extensions_still_obey_duplicate_and_resource_guards() {
        let now = Instant::now();
        let mut tx = transaction(now);
        for tail in [
            "code=a&provider_extension=a&provider_%65xtension=b",
            "code=a&provider_extension=x&%63ode=b",
        ] {
            let url = callback(&tx, tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::DuplicateParameter);
        }
        for tail in [
            format!("code=a&{}=x", "k".repeat(65)),
            format!("code=a&provider_extension={}", "x".repeat(4097)),
            format!(
                "code=a&{}",
                (0..15)
                    .map(|i| format!("extension{i}=x"))
                    .collect::<Vec<_>>()
                    .join("&")
            ),
        ] {
            let url = callback(&tx, &tail);
            assert_eq!(rejected(&mut tx, &url, now), PkceError::CallbackLimit);
            assert_eq!(tx.status(), TransactionStatus::Pending);
        }
        let url = format!("{}?code=a&provider_extension=state", tx.redirect_uri());
        assert_eq!(rejected(&mut tx, &url, now), PkceError::StateMismatch);
        let url = callback(&tx, "provider_extension=code");
        assert_eq!(rejected(&mut tx, &url, now), PkceError::AmbiguousResponse);
    }

    #[test]
    fn timeout_cancel_and_clock_regression_are_terminal() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let url = callback(&tx, "code=synthetic");
        assert_eq!(tx.check_active(now + Duration::from_secs(59)), Ok(()));
        assert_eq!(
            rejected(&mut tx, &url, now + Duration::from_secs(60)),
            PkceError::Expired
        );
        assert_eq!(tx.status(), TransactionStatus::Expired);
        assert!(tx.secrets.is_none());
        assert_eq!(rejected(&mut tx, &url, now), PkceError::Finished);
        let mut tx = transaction(now);
        tx.cancel(now).unwrap();
        assert_eq!(tx.status(), TransactionStatus::Cancelled);
        assert_eq!(tx.cancel(now), Err(PkceError::Finished));
        assert_eq!(rejected(&mut tx, &url, now), PkceError::Finished);
        let mut tx = transaction(now);
        tx.check_active(now + Duration::from_secs(1)).unwrap();
        assert_eq!(tx.check_active(now), Err(PkceError::ClockRegression));
        assert_eq!(tx.status(), TransactionStatus::ClockFailed);
        assert!(tx.secrets.is_none());
    }

    #[test]
    fn repeated_invalid_callbacks_never_extend_the_deadline() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let wrong = format!("{}?state=wrong&code=a", tx.redirect_uri());
        for second in [1, 20, 59] {
            assert_eq!(
                rejected(&mut tx, &wrong, now + Duration::from_secs(second)),
                PkceError::StateMismatch
            );
        }
        let good = callback(&tx, "code=a");
        assert_eq!(
            rejected(&mut tx, &good, now + Duration::from_secs(60)),
            PkceError::Expired
        );
    }

    #[test]
    fn callback_limits_and_reported_scope_or_issuer_mismatch_are_explicit() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let oversized = "x".repeat(MAX_CALLBACK_BYTES + 1);
        assert_eq!(rejected(&mut tx, &oversized, now), PkceError::CallbackLimit);
        let code = callback(&tx, &format!("code={}", "x".repeat(MAX_CODE_BYTES + 1)));
        assert_eq!(rejected(&mut tx, &code, now), PkceError::InvalidCode);
        for tail in [
            "code=a&iss=https%3A%2F%2Fevil.invalid",
            "code=a&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file",
            "code=a&scope=",
        ] {
            let url = callback(&tx, tail);
            assert!(matches!(
                rejected(&mut tx, &url, now),
                PkceError::IssuerMismatch | PkceError::ScopeMismatch
            ));
            assert_eq!(tx.status(), TransactionStatus::Pending);
        }
        let good = callback(
            &tx,
            "code=a&iss=https%3A%2F%2Faccounts.google.com&authuser=0&prompt=consent",
        );
        assert!(matches!(
            tx.accept_callback(&good, now),
            Ok(CallbackOutcome::Code(_))
        ));
    }

    #[test]
    fn errors_do_not_echo_callback_secrets() {
        let now = Instant::now();
        let mut tx = transaction(now);
        let url = callback(&tx, "code=synthetic-secret&code=other");
        let error = rejected(&mut tx, &url, now);
        assert_eq!(error, PkceError::DuplicateParameter);
        for message in [format!("{error}"), format!("{error:?}")] {
            assert!(!message.contains("synthetic-secret"));
            assert!(!message.contains(VERIFIER));
            assert!(message.len() < 100);
        }
    }
}
