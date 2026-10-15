//! Fixed native Drive about.get verification, not session adoption or sync.
//!
//! The native caller must bind configuration to the original admission. Google
//! authenticates the bearer user's permission identity, NOT our configured
//! client/project mapping. A candidate owns the exact input credentials; only
//! the later controller can atomically reject stale results and adopt it.
//! No credentials/PII are serializable or Debug/Clone here. Ordinary drop is not
//! guaranteed zeroization. No browser, storage, refresh or transport override.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    HeaderMap, HeaderName, HeaderValue,
};
use reqwest::{Client, Url};
use serde::Deserializer;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use tokio::sync::oneshot;

use super::exchange::{ExchangeAttempt, ExchangeResult};
use super::pkce::ApprovedScope;
use super::token_response::{MAX_TOKEN_BYTES, ParsedTokenResponse};
use crate::drive::model::{AccountKey, ProviderId, UnsignedDecimal};

const ENDPOINT: &str = "https://www.googleapis.com/drive/v3/about";
const FIELDS: &str = "user(permissionId,displayName,emailAddress),storageQuota";
const BODY_LIMIT: usize = 64 * 1024;
const HEADER_LIMIT: usize = 16 * 1024;
const HEADER_FIELDS: usize = 64;
const DEPTH_LIMIT: usize = 16;
const WORK_LIMIT: usize = 4096;
const MAP_LIMIT: usize = 32;
const KEY_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationField {
    RegistrationId,
    ClientId,
    ProjectNumber,
    AttemptIdentity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseLimit {
    Depth,
    Work,
    Fields,
    Key,
}
/// No provider/body/header/URL/token strings are retained in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountError {
    InvalidRegistration(RegistrationField),
    InvalidRequestedScopes,
    AttemptMismatch,
    InvalidGrantedScopes,
    InvalidBearerCredential,
    Cancelled,
    TokenExpired,
    Deadline,
    ClockRegression,
    ClientSetup,
    Transport,
    HeaderLimit,
    InvalidHeaders,
    ContentEncoding,
    BodyLimit,
    ParseLimit(ParseLimit),
    Unauthorized,
    Forbidden,
    Redirect,
    RateLimited,
    Server,
    UnexpectedStatus,
    MissingUser,
    MissingPermissionId,
    InvalidPermissionId,
    MalformedResponse,
    InvalidDisplayMetadata,
    InvalidQuota,
    ProviderErrorPayload,
}
impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Account verification error: {self:?}")
    }
}
impl std::error::Error for AccountError {}

/// Native configured metadata only. Construction is NOT authenticated mapping.
pub struct VerificationContext {
    registration_id: String,
    client_id: String,
    project: UnsignedDecimal,
    requested: Vec<ApprovedScope>,
    attempt: ExchangeAttempt,
    admission_deadline: Instant,
}
impl VerificationContext {
    pub fn from_native_attempt(
        registration_id: &str,
        client_id: &str,
        project_number: &str,
        requested: &[ApprovedScope],
        expected_attempt: &ExchangeAttempt,
        admission_deadline: Instant,
    ) -> Result<Self, AccountError> {
        let identifier = |text: &str, max| {
            !text.is_empty()
                && text.len() <= max
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        };
        if !identifier(registration_id, 128) {
            return Err(AccountError::InvalidRegistration(
                RegistrationField::RegistrationId,
            ));
        }
        if !identifier(client_id, 256)
            || client_id.len() <= ".apps.googleusercontent.com".len()
            || !client_id.ends_with(".apps.googleusercontent.com")
        {
            return Err(AccountError::InvalidRegistration(
                RegistrationField::ClientId,
            ));
        }
        let project = decimal(project_number, u64::MAX)
            .filter(|value| *value != 0)
            .ok_or(AccountError::InvalidRegistration(
                RegistrationField::ProjectNumber,
            ))?;
        if !scope_set(requested) {
            return Err(AccountError::InvalidRequestedScopes);
        }
        if expected_attempt.id.is_nil() || expected_attempt.generation == 0 {
            return Err(AccountError::InvalidRegistration(
                RegistrationField::AttemptIdentity,
            ));
        }
        if Instant::now() >= admission_deadline {
            return Err(AccountError::Deadline);
        }
        Ok(Self {
            registration_id: registration_id.to_owned(),
            client_id: client_id.to_owned(),
            project: UnsignedDecimal::new(project),
            requested: requested.to_vec(),
            attempt: ExchangeAttempt {
                id: expected_attempt.id,
                generation: expected_attempt.generation,
            },
            admission_deadline,
        })
    }
    fn same_binding(&self, other: &Self) -> bool {
        self.registration_id == other.registration_id
            && self.client_id == other.client_id
            && self.project == other.project
            && same_attempt(&self.attempt, &other.attempt)
            && self.admission_deadline == other.admission_deadline
            && self.requested.len() == other.requested.len()
            && self
                .requested
                .iter()
                .all(|scope| other.requested.contains(scope))
    }
}

pub struct AccountDisplay {
    name: Option<String>,
    email: Option<String>,
    quota: Option<QuotaSummary>,
}
impl AccountDisplay {
    pub fn display_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn email_address(&self) -> Option<&str> {
        self.email.as_deref()
    }
    /// None means quota unknown, not unlimited.
    pub fn quota(&self) -> Option<&QuotaSummary> {
        self.quota.as_ref()
    }
}
pub struct QuotaSummary {
    limit: Option<u64>,
    usage: Option<u64>,
    drive: Option<u64>,
    trash: Option<u64>,
}
impl QuotaSummary {
    /// None in a PRESENT quota object means no reported limit. Not an entitlement.
    pub fn limit(&self) -> Option<u64> {
        self.limit
    }
    pub fn usage(&self) -> Option<u64> {
        self.usage
    }
    pub fn usage_in_drive(&self) -> Option<u64> {
        self.drive
    }
    pub fn usage_in_drive_trash(&self) -> Option<u64> {
        self.trash
    }
}
/// Sealed request result, not ConnectedSessionOnly. No public construction,
/// extraction/replacement of credentials, serde, Debug, Display or Clone.
pub struct VerifiedAccountCandidate {
    input: OwnedInput,
    context: VerificationContext,
    key: AccountKey,
    display: AccountDisplay,
    missing: Vec<ApprovedScope>,
}
impl VerifiedAccountCandidate {
    /// Immutable credential expiry metadata, not an admission/session deadline
    /// or proof of remote validity. Does not expose or extend the credentials.
    pub fn credential_expires_at(&self) -> Instant {
        self.input.tokens().expires_at()
    }

    /// Local credential validation-time/expiry check only. This does NOT check
    /// admission, current controller state, registration mapping, revocation or
    /// Drive readiness. This is not an account/session proof. At adoption,
    /// matches_native_attempt and the controller's
    /// atomic current-state checks remain mandatory; this is not a substitute.
    pub fn credentials_usable_at(&self, now: Instant) -> bool {
        self.input.tokens().is_usable_at(now)
    }

    /// This borrowed key is syntax/data, not independently transferable proof.
    pub fn account_key(&self) -> &AccountKey {
        &self.key
    }
    pub fn granted_scopes(&self) -> &[ApprovedScope] {
        self.input.tokens().granted_scopes()
    }
    pub fn missing_requested_scopes(&self) -> &[ApprovedScope] {
        &self.missing
    }
    pub fn display(&self) -> &AccountDisplay {
        &self.display
    }
    /// Caller MUST re-read current controller/configuration under its commit
    /// mutex. This method cannot inspect or lock that live state for the caller.
    pub fn matches_native_attempt(&self, expected: &VerificationContext, now: Instant) -> bool {
        self.context.same_binding(expected)
            && same_attempt(self.input.attempt(), &expected.attempt)
            && now < expected.admission_deadline
            && self.input.tokens().is_usable_at(now)
    }
}
enum OwnedInput {
    Exchange(ExchangeResult),
    #[cfg(test)]
    Fixture {
        attempt: ExchangeAttempt,
        tokens: ParsedTokenResponse,
    },
}
impl OwnedInput {
    fn attempt(&self) -> &ExchangeAttempt {
        match self {
            Self::Exchange(value) => value.attempt(),
            #[cfg(test)]
            Self::Fixture { attempt, .. } => attempt,
        }
    }
    fn tokens(&self) -> &ParsedTokenResponse {
        match self {
            Self::Exchange(value) => value.tokens_for_native_controller(),
            #[cfg(test)]
            Self::Fixture { tokens, .. } => tokens,
        }
    }
}
fn same_attempt(a: &ExchangeAttempt, b: &ExchangeAttempt) -> bool {
    a.id == b.id && a.generation == b.generation
}
fn scope_set(scopes: &[ApprovedScope]) -> bool {
    matches!(scopes.len(), 1 | 2) && (scopes.len() == 1 || scopes[0] != scopes[1])
}
fn decimal(text: &str, max: u64) -> Option<u64> {
    if text.is_empty()
        || text.len() > 20
        || !text.bytes().all(|b| b.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return None;
    }
    text.parse::<u64>().ok().filter(|value| *value <= max)
}

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
    #[cfg(test)]
    Loopback(std::net::SocketAddr),
}
impl Target {
    fn url(&self) -> Result<Url, AccountError> {
        let mut url = match self {
            Self::Google => Url::parse(ENDPOINT).map_err(|_| AccountError::ClientSetup)?,
            #[cfg(test)]
            Self::Loopback(address) => {
                if !matches!(address, std::net::SocketAddr::V4(a) if *a.ip() == std::net::Ipv4Addr::LOCALHOST && a.port() != 0)
                {
                    return Err(AccountError::ClientSetup);
                }
                Url::parse(&format!("http://{address}/drive/v3/about"))
                    .map_err(|_| AccountError::ClientSetup)?
            }
        };
        url.query_pairs_mut().append_pair("fields", FIELDS);
        Ok(url)
    }
    fn https_only(&self) -> bool {
        match self {
            Self::Google => true,
            #[cfg(test)]
            Self::Loopback(_) => false,
        }
    }
}

/// One fixed native request. Cancellation by signal OR sender drop, no retries.
/// Consumes this new input on every outcome; it never mutates a prior session.
/// Only the future controller may commit the returned candidate under its lock.
pub async fn verify_account(
    exchange: ExchangeResult,
    context: VerificationContext,
    cancel: oneshot::Receiver<()>,
) -> Result<VerifiedAccountCandidate, AccountError> {
    verify_at(
        OwnedInput::Exchange(exchange),
        context,
        cancel,
        Target::Google,
        Limits::PRODUCTION,
    )
    .await
}
async fn verify_at(
    input: OwnedInput,
    context: VerificationContext,
    mut cancel: oneshot::Receiver<()>,
    target: Target,
    limits: Limits,
) -> Result<VerifiedAccountCandidate, AccountError> {
    let entry = Instant::now();
    let deadline = entry
        .checked_add(limits.total)
        .ok_or(AccountError::Deadline)?
        .min(context.admission_deadline)
        .min(input.tokens().expires_at());
    gate(&input, &context, &mut cancel, deadline)?;
    let authorization = bearer_header(input.tokens().access_token_for_native_adapter())?;
    let url = target.url()?;
    let client = Client::builder()
        .https_only(target.https_only())
        .tls_backend_rustls()
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .http1_only()
        .pool_max_idle_per_host(0)
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connection_verbose(false)
        .connect_timeout(limits.connect)
        .read_timeout(limits.read)
        .timeout(limits.total)
        .build()
        .map_err(|_| AccountError::ClientSetup)?;
    gate(&input, &context, &mut cancel, deadline)?;
    let operation = async {
        let mut response = client
            .get(url)
            .header(AUTHORIZATION, authorization)
            .header(ACCEPT, "application/json")
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(transport_error)?;
        tokio::task::yield_now().await;
        time_gate(input.tokens(), deadline, Instant::now())?;
        header_budget(response.headers())?;
        status(response.status().as_u16())?;
        let length = success_headers(response.headers())?;
        let mut bytes = Vec::with_capacity(BODY_LIMIT);
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            tokio::task::yield_now().await;
            time_gate(input.tokens(), deadline, Instant::now())?;
            append_body(&mut bytes, &chunk)?;
        }
        if length.is_some_and(|length| length != bytes.len()) {
            return Err(AccountError::Transport);
        }
        tokio::task::yield_now().await;
        time_gate(input.tokens(), deadline, Instant::now())?;
        parse_about(&bytes)
    };
    let parsed = tokio::select! {
        biased;
        _ = &mut cancel => return Err(AccountError::Cancelled),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
            return Err(time_gate(input.tokens(), deadline, Instant::now()).err().unwrap_or(AccountError::Deadline));
        }
        result = operation => result,
    };
    // Recheck cancellation/expiry even when an error completed concurrently.
    // No stale response category can bypass the final ownership/lifetime gate.
    gate(&input, &context, &mut cancel, deadline)?;
    finish(input, context, parsed?, &mut cancel, deadline)
}
fn finish(
    input: OwnedInput,
    context: VerificationContext,
    parsed: AboutFields,
    cancel: &mut oneshot::Receiver<()>,
    deadline: Instant,
) -> Result<VerifiedAccountCandidate, AccountError> {
    let missing = context
        .requested
        .iter()
        .copied()
        .filter(|scope| !input.tokens().granted_scopes().contains(scope))
        .collect();
    let key = AccountKey {
        project_number: context.project,
        drive_permission_id: parsed.permission_id,
    };
    gate(&input, &context, cancel, deadline)?;
    Ok(VerifiedAccountCandidate {
        input,
        context,
        key,
        display: parsed.display,
        missing,
    })
}
fn gate(
    input: &OwnedInput,
    context: &VerificationContext,
    cancel: &mut oneshot::Receiver<()>,
    deadline: Instant,
) -> Result<(), AccountError> {
    match cancel.try_recv() {
        Ok(()) | Err(oneshot::error::TryRecvError::Closed) => return Err(AccountError::Cancelled),
        Err(oneshot::error::TryRecvError::Empty) => {}
    }
    if !same_attempt(input.attempt(), &context.attempt) {
        return Err(AccountError::AttemptMismatch);
    }
    let granted = input.tokens().granted_scopes();
    if !scope_set(granted)
        || granted
            .iter()
            .any(|scope| !context.requested.contains(scope))
    {
        return Err(AccountError::InvalidGrantedScopes);
    }
    time_gate(
        input.tokens(),
        deadline.min(context.admission_deadline),
        Instant::now(),
    )
}
fn time_gate(
    tokens: &ParsedTokenResponse,
    deadline: Instant,
    now: Instant,
) -> Result<(), AccountError> {
    if !tokens.is_usable_at(now) {
        return Err(if now >= tokens.expires_at() {
            AccountError::TokenExpired
        } else {
            AccountError::ClockRegression
        });
    }
    if now >= deadline {
        return Err(AccountError::Deadline);
    }
    Ok(())
}
fn bearer_header(token: &str) -> Result<HeaderValue, AccountError> {
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return Err(AccountError::InvalidBearerCredential);
    }
    let mut padding = false;
    let mut data = 0;
    for b in token.bytes() {
        if b == b'=' {
            padding = true;
        } else if !padding && (b.is_ascii_alphanumeric() || b"-._~+/".contains(&b)) {
            data += 1;
        } else {
            return Err(AccountError::InvalidBearerCredential);
        }
    }
    if data == 0 {
        return Err(AccountError::InvalidBearerCredential);
    }
    let mut bytes = Vec::with_capacity(7 + token.len());
    bytes.extend_from_slice(b"Bearer ");
    bytes.extend_from_slice(token.as_bytes());
    let mut value =
        HeaderValue::from_bytes(&bytes).map_err(|_| AccountError::InvalidBearerCredential)?;
    value.set_sensitive(true);
    Ok(value)
}
fn transport_error(error: reqwest::Error) -> AccountError {
    if error.is_timeout() {
        AccountError::Deadline
    } else {
        AccountError::Transport
    }
}
fn status(code: u16) -> Result<(), AccountError> {
    match code {
        200 => Ok(()),
        300..=399 => Err(AccountError::Redirect),
        401 => Err(AccountError::Unauthorized),
        403 => Err(AccountError::Forbidden),
        429 => Err(AccountError::RateLimited),
        500..=599 => Err(AccountError::Server),
        _ => Err(AccountError::UnexpectedStatus),
    }
}
fn header_budget(headers: &HeaderMap) -> Result<(), AccountError> {
    if headers.len() > HEADER_FIELDS {
        return Err(AccountError::HeaderLimit);
    }
    let mut bytes = 0usize;
    for (key, value) in headers {
        bytes = bytes
            .checked_add(key.as_str().len())
            .and_then(|n| n.checked_add(value.as_bytes().len()))
            .ok_or(AccountError::HeaderLimit)?;
        if bytes > HEADER_LIMIT {
            return Err(AccountError::HeaderLimit);
        }
    }
    Ok(())
}
fn single(headers: &HeaderMap, name: HeaderName) -> Result<Option<&HeaderValue>, AccountError> {
    let all = headers.get_all(name);
    let mut values = all.iter();
    let value = values.next();
    if values.next().is_some() {
        return Err(AccountError::InvalidHeaders);
    }
    Ok(value)
}
fn success_headers(headers: &HeaderMap) -> Result<Option<usize>, AccountError> {
    let content_type = single(headers, CONTENT_TYPE)?.ok_or(AccountError::InvalidHeaders)?;
    if !json_type(
        content_type
            .to_str()
            .map_err(|_| AccountError::InvalidHeaders)?,
    ) {
        return Err(AccountError::InvalidHeaders);
    }
    if let Some(encoding) = single(headers, CONTENT_ENCODING)?
        && !encoding
            .to_str()
            .map_err(|_| AccountError::ContentEncoding)?
            .trim_matches([' ', '\t'])
            .eq_ignore_ascii_case("identity")
    {
        return Err(AccountError::ContentEncoding);
    }
    let Some(value) = single(headers, CONTENT_LENGTH)? else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| AccountError::InvalidHeaders)?
        .trim_matches([' ', '\t']);
    if value.is_empty() {
        return Err(AccountError::InvalidHeaders);
    }
    let mut length = 0usize;
    for b in value.bytes() {
        if !b.is_ascii_digit() {
            return Err(AccountError::InvalidHeaders);
        }
        length = length
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as usize))
            .ok_or(AccountError::BodyLimit)?;
        if length > BODY_LIMIT {
            return Err(AccountError::BodyLimit);
        }
    }
    Ok(Some(length))
}
fn json_type(value: &str) -> bool {
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
    value.eq_ignore_ascii_case("utf-8") || value.eq_ignore_ascii_case("\"utf-8\"")
}
fn append_body(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), AccountError> {
    if body
        .len()
        .checked_add(chunk.len())
        .is_none_or(|n| n > BODY_LIMIT)
    {
        return Err(AccountError::BodyLimit);
    }
    body.extend_from_slice(chunk);
    Ok(())
}

struct AboutFields {
    permission_id: ProviderId,
    display: AccountDisplay,
}
#[derive(Default)]
struct RawUser {
    permission: Option<String>,
    name: Option<String>,
    email: Option<String>,
}
#[derive(Default)]
struct RawQuota {
    limit: Option<String>,
    usage: Option<String>,
    drive: Option<String>,
    trash: Option<String>,
}
struct RawAbout {
    user: Option<RawUser>,
    quota: Option<RawQuota>,
    error: bool,
}

fn parse_about(body: &[u8]) -> Result<AboutFields, AccountError> {
    json_budget(body)?;
    // Validate ignored extension strings as UTF-8 too; IgnoredAny need not materialize them.
    std::str::from_utf8(body).map_err(|_| AccountError::MalformedResponse)?;
    let failure = Cell::new(None);
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let raw = RootSeed(&failure)
        .deserialize(&mut decoder)
        .and_then(|value| decoder.end().map(|()| value))
        .map_err(|_| failure.get().unwrap_or(AccountError::MalformedResponse))?;
    if raw.error {
        return Err(AccountError::ProviderErrorPayload);
    }
    let user = raw.user.ok_or(AccountError::MissingUser)?;
    let permission = user.permission.ok_or(AccountError::MissingPermissionId)?;
    let permission_id =
        ProviderId::try_from(permission).map_err(|_| AccountError::InvalidPermissionId)?;
    let name = display_text(user.name, 1024, Some(256))?;
    let email = display_text(user.email, 320, None)?;
    let quota = raw
        .quota
        .map(|raw| -> Result<QuotaSummary, AccountError> {
            let number = |value: Option<String>| {
                value
                    .map(|s| decimal(&s, i64::MAX as u64).ok_or(AccountError::InvalidQuota))
                    .transpose()
            };
            Ok(QuotaSummary {
                limit: number(raw.limit)?,
                usage: number(raw.usage)?,
                drive: number(raw.drive)?,
                trash: number(raw.trash)?,
            })
        })
        .transpose()?;
    Ok(AboutFields {
        permission_id,
        display: AccountDisplay { name, email, quota },
    })
}
fn display_text(
    value: Option<String>,
    bytes: usize,
    scalars: Option<usize>,
) -> Result<Option<String>, AccountError> {
    let Some(text) = value else { return Ok(None) };
    if text.len() > bytes
        || scalars.is_some_and(|max| text.chars().count() > max)
        || text.chars().any(char::is_control)
    {
        return Err(AccountError::InvalidDisplayMetadata);
    }
    if text.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}
fn json_budget(body: &[u8]) -> Result<(), AccountError> {
    if body.len() > BODY_LIMIT {
        return Err(AccountError::BodyLimit);
    }
    let (mut depth, mut work, mut quoted, mut escaped) = (0usize, 0usize, false, false);
    for &b in body {
        if quoted {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            continue;
        }
        match b {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                work += 1;
                if depth > DEPTH_LIMIT {
                    return Err(AccountError::ParseLimit(ParseLimit::Depth));
                }
            }
            b'}' | b']' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(AccountError::MalformedResponse)?
            }
            b',' | b':' => work += 1,
            _ => {}
        }
        if work > WORK_LIMIT {
            return Err(AccountError::ParseLimit(ParseLimit::Work));
        }
    }
    if quoted || depth != 0 {
        return Err(AccountError::MalformedResponse);
    }
    Ok(())
}
// A private typed error cell preserves a field category through serde failures.
// It never contains parser strings or supplied data; serde errors are discarded.
fn tagged<E>(failure: &Cell<Option<AccountError>>, kind: AccountError, error: E) -> E {
    if failure.get().is_none() {
        failure.set(Some(kind));
    }
    error
}
fn key<E: de::Error>(
    seen: &mut BTreeSet<String>,
    name: &str,
    failure: &Cell<Option<AccountError>>,
) -> Result<(), E> {
    let kind = if name.len() > KEY_LIMIT {
        Some(AccountError::ParseLimit(ParseLimit::Key))
    } else if seen.len() >= MAP_LIMIT {
        Some(AccountError::ParseLimit(ParseLimit::Fields))
    } else if seen.contains(name) {
        Some(AccountError::MalformedResponse)
    } else {
        None
    };
    if let Some(kind) = kind {
        return Err(tagged(
            failure,
            kind,
            E::custom("invalid bounded account map"),
        ));
    }
    seen.insert(name.to_owned());
    Ok(())
}
struct RootSeed<'a>(&'a Cell<Option<AccountError>>);
impl<'de> DeserializeSeed<'de> for RootSeed<'_> {
    type Value = RawAbout;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for RootSeed<'_> {
    type Value = RawAbout;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded account object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut value = RawAbout {
            user: None,
            quota: None,
            error: false,
        };
        let mut seen = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            key::<A::Error>(&mut seen, &name, self.0)?;
            match name.as_str() {
                "user" => value.user = map.next_value_seed(UserSeed(self.0))?,
                "storageQuota" => {
                    value.quota = map
                        .next_value_seed(QuotaSeed(self.0))
                        .map_err(|e| tagged(self.0, AccountError::InvalidQuota, e))?
                }
                "error" => {
                    value.error = true;
                    map.next_value::<IgnoredAny>()?;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(value)
    }
}
struct UserSeed<'a>(&'a Cell<Option<AccountError>>);
impl<'de> DeserializeSeed<'de> for UserSeed<'_> {
    type Value = Option<RawUser>;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for UserSeed<'_> {
    type Value = Option<RawUser>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional bounded user object")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut value = RawUser::default();
        let mut seen = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            key::<A::Error>(&mut seen, &name, self.0)?;
            match name.as_str() {
                "permissionId" => {
                    value.permission = map
                        .next_value::<Option<String>>()
                        .map_err(|e| tagged(self.0, AccountError::InvalidPermissionId, e))?
                }
                "displayName" => {
                    value.name = map
                        .next_value::<Option<String>>()
                        .map_err(|e| tagged(self.0, AccountError::InvalidDisplayMetadata, e))?
                }
                "emailAddress" => {
                    value.email = map
                        .next_value::<Option<String>>()
                        .map_err(|e| tagged(self.0, AccountError::InvalidDisplayMetadata, e))?
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Some(value))
    }
}
struct QuotaSeed<'a>(&'a Cell<Option<AccountError>>);
impl<'de> DeserializeSeed<'de> for QuotaSeed<'_> {
    type Value = Option<RawQuota>;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for QuotaSeed<'_> {
    type Value = Option<RawQuota>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional bounded quota object")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut value = RawQuota::default();
        let mut seen = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            key::<A::Error>(&mut seen, &name, self.0)?;
            match name.as_str() {
                "limit" => value.limit = map.next_value::<Option<String>>()?,
                "usage" => value.usage = map.next_value::<Option<String>>()?,
                "usageInDrive" => value.drive = map.next_value::<Option<String>>()?,
                "usageInDriveTrash" => value.trash = map.next_value::<Option<String>>()?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::super::token_response::parse_google_token_response;
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    const CLIENT: &str = "synthetic-desktop.apps.googleusercontent.com";
    const BODY_A: &[u8] = br#"{"user":{"permissionId":"permission_A"}}"#;
    const BODY_B: &[u8] =
        br#"{"user":{"permissionId":"permission_B","displayName":"Synthetic B"}}"#;
    fn input(
        token: &str,
        scopes: &[ApprovedScope],
        issued: Instant,
        expires: u64,
        id: u128,
    ) -> OwnedInput {
        let body = serde_json::to_vec(&serde_json::json!({
            "access_token":token,"refresh_token":"synthetic-refresh-canary",
            "token_type":"Bearer","expires_in":expires,
            "scope": scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")
        }))
        .expect("synthetic token JSON");
        OwnedInput::Fixture {
            attempt: ExchangeAttempt {
                id: uuid::Uuid::from_u128(id),
                generation: 7,
            },
            tokens: parse_google_token_response(200, &body, scopes, issued, issued)
                .expect("pure synthetic tokens"),
        }
    }
    fn ordinary() -> OwnedInput {
        input(
            "synthetic-token-A",
            &[ApprovedScope::File],
            Instant::now(),
            3600,
            1,
        )
    }
    fn context(
        input: &OwnedInput,
        project: &str,
        deadline: Instant,
        scopes: &[ApprovedScope],
    ) -> VerificationContext {
        VerificationContext::from_native_attempt(
            "desktop",
            CLIENT,
            project,
            scopes,
            input.attempt(),
            deadline,
        )
        .expect("synthetic binding")
    }
    fn limits() -> Limits {
        Limits {
            total: Duration::from_secs(2),
            connect: Duration::from_millis(500),
            read: Duration::from_millis(500),
        }
    }
    fn response(status: u16, extra: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes = format!("HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", body.len()).into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
    fn chunked(body: &[u8]) -> Vec<u8> {
        let mut bytes = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        for part in body.chunks(1024) {
            bytes.extend_from_slice(format!("{:X}\r\n", part.len()).as_bytes());
            bytes.extend_from_slice(part);
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"0\r\n\r\n");
        bytes
    }
    fn error<T>(result: Result<T, AccountError>) -> AccountError {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected typed failure"),
        }
    }

    // Actual localhost transport. Synthetic captures are bounded to four16KiB
    // requests; each worker has a3s ceiling and a stop flag, finite socket I/O,
    // and joined Drop. No environment mutation, Google endpoint or detached task.
    struct Server {
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        count: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<Vec<u8>>>>,
        thread: Option<JoinHandle<()>>,
    }
    impl Server {
        fn reply(bytes: Vec<u8>) -> Self {
            Self::script(vec![(Duration::ZERO, bytes)])
        }
        fn script(parts: Vec<(Duration, Vec<u8>)>) -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("local bind");
            listener.set_nonblocking(true).expect("nonblocking");
            let address = listener.local_addr().expect("local address");
            let stop = Arc::new(AtomicBool::new(false));
            let count = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let (halt, accepted, captured) =
                (Arc::clone(&stop), Arc::clone(&count), Arc::clone(&requests));
            let thread = thread::spawn(move || {
                let end = Instant::now() + Duration::from_secs(3);
                while !halt.load(Ordering::Acquire)
                    && Instant::now() < end
                    && accepted.load(Ordering::Acquire) < 4
                {
                    let (mut stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => break,
                    };
                    accepted.fetch_add(1, Ordering::AcqRel);
                    if stream
                        .set_read_timeout(Some(Duration::from_millis(10)))
                        .is_err()
                        || stream
                            .set_write_timeout(Some(Duration::from_millis(20)))
                            .is_err()
                    {
                        continue;
                    }
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while request.len() < HEADER_LIMIT
                        && !request.windows(4).any(|w| w == b"\r\n\r\n")
                        && !halt.load(Ordering::Acquire)
                        && Instant::now() < end
                    {
                        match stream.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                if request.len() + n > HEADER_LIMIT {
                                    break;
                                }
                                request.extend_from_slice(&buffer[..n]);
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                                ) => {}
                            Err(_) => break,
                        }
                    }
                    captured
                        .lock()
                        .expect("synthetic capture mutex")
                        .push(request);
                    for (delay, bytes) in &parts {
                        let ready = Instant::now() + *delay;
                        while Instant::now() < ready
                            && Instant::now() < end
                            && !halt.load(Ordering::Acquire)
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        if halt.load(Ordering::Acquire) || Instant::now() >= end {
                            break;
                        }
                        for chunk in bytes.chunks(4096) {
                            if halt.load(Ordering::Acquire)
                                || Instant::now() >= end
                                || stream.write_all(chunk).is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            });
            Self {
                address,
                stop,
                count,
                requests,
                thread: Some(thread),
            }
        }
        async fn captured(&self) {
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    if !self.requests.lock().expect("capture").is_empty() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .expect("request capture deadline");
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    async fn execute(server: &Server) -> Result<VerifiedAccountCandidate, AccountError> {
        let input = ordinary();
        let context = context(
            &input,
            "123",
            Instant::now() + Duration::from_secs(5),
            &[ApprovedScope::AppData, ApprovedScope::File],
        );
        let (tx, rx) = oneshot::channel();
        let result = verify_at(
            input,
            context,
            rx,
            Target::Loopback(server.address),
            limits(),
        )
        .await;
        drop(tx);
        result
    }

    #[test]
    fn registration_is_syntax_only_and_rejects_noncanonical_or_overflow_projects() {
        let input = ordinary();
        let deadline = Instant::now() + Duration::from_secs(5);
        for project in [
            "",
            "0",
            "01",
            "+1",
            "1.0",
            "18446744073709551616",
            "99999999999999999999999999999999",
        ] {
            assert_eq!(
                error(VerificationContext::from_native_attempt(
                    "desktop",
                    CLIENT,
                    project,
                    &[ApprovedScope::File],
                    input.attempt(),
                    deadline
                )),
                AccountError::InvalidRegistration(RegistrationField::ProjectNumber)
            );
        }
        let context = context(
            &input,
            "18446744073709551615",
            deadline,
            &[ApprovedScope::File],
        );
        assert_eq!(context.project.value(), u64::MAX);
        for client in [
            "",
            "https://example.invalid",
            "a.apps.googleusercontent.com/",
        ] {
            assert_eq!(
                error(VerificationContext::from_native_attempt(
                    "desktop",
                    client,
                    "123",
                    &[ApprovedScope::File],
                    input.attempt(),
                    deadline
                )),
                AccountError::InvalidRegistration(RegistrationField::ClientId)
            );
        }
        for requested in [
            vec![],
            vec![ApprovedScope::File; 2],
            vec![ApprovedScope::File; 3],
        ] {
            assert_eq!(
                error(VerificationContext::from_native_attempt(
                    "desktop",
                    CLIENT,
                    "123",
                    &requested,
                    input.attempt(),
                    deadline
                )),
                AccountError::InvalidRequestedScopes
            );
        }
        let nil = ExchangeAttempt {
            id: uuid::Uuid::nil(),
            generation: 7,
        };
        assert_eq!(
            error(VerificationContext::from_native_attempt(
                "desktop",
                CLIENT,
                "123",
                &[ApprovedScope::File],
                &nil,
                deadline
            )),
            AccountError::InvalidRegistration(RegistrationField::AttemptIdentity)
        );
    }

    #[test]
    fn bearer_is_sensitive_bounded_and_never_reinterpreted() {
        let header = bearer_header("A-._~+/09==").expect("RFC bearer syntax");
        assert!(header.is_sensitive());
        assert_eq!(header.as_bytes(), b"Bearer A-._~+/09==");
        for value in ["", "=", "A=B", "a b", "a!b", "a\r\nb", "é"] {
            assert_eq!(
                error(bearer_header(value)),
                AccountError::InvalidBearerCredential
            );
        }
        assert!(bearer_header(&"a".repeat(MAX_TOKEN_BYTES)).is_ok());
        assert_eq!(
            error(bearer_header(&"a".repeat(MAX_TOKEN_BYTES + 1))),
            AccountError::InvalidBearerCredential
        );
    }

    #[test]
    fn parser_optional_display_quota_and_known_answers() {
        for body in [BODY_A,br#"{"user":{"permissionId":"permission_A","emailAddress":null,"displayName":"   "},"storageQuota":null}"#] {
            let value = parse_about(body).expect("optional metadata");
            assert_eq!(value.permission_id.as_str(),"permission_A");
            assert!(value.display.name.is_none()); assert!(value.display.email.is_none()); assert!(value.display.quota.is_none());
        }
        let value = parse_about(br#"{"user":{"permissionId":"permission_A","displayName":"Synthetic Person","emailAddress":"synthetic@example.invalid"},"storageQuota":{"usage":"42","usageInDrive":"20","usageInDriveTrash":"1"},"extension":{"items":[1,true,null,{"a":"b"}]}}"#).expect("quota and extensions");
        let quota = value.display.quota().expect("present quota");
        assert_eq!(quota.limit(), None);
        assert_eq!(quota.usage(), Some(42));
        assert_eq!(quota.usage_in_drive(), Some(20));
        assert_eq!(quota.usage_in_drive_trash(), Some(1));
        assert_eq!(value.display.display_name(), Some("Synthetic Person"));
        let bound = parse_about(br#"{"user":{"permissionId":"p"},"storageQuota":{"limit":"9223372036854775807","usage":"0"}}"#).expect("int64 bound");
        assert_eq!(
            bound.display.quota().expect("quota").limit(),
            Some(i64::MAX as u64)
        );
    }

    #[test]
    fn parser_missing_null_wrong_types_and_error_payload_are_distinct() {
        for (body, want) in [
            (r#"{}"#, AccountError::MissingUser),
            (r#"{"user":null}"#, AccountError::MissingUser),
            (r#"{"user":[]}"#, AccountError::MalformedResponse),
            (r#"{"user":{}}"#, AccountError::MissingPermissionId),
            (
                r#"{"user":{"permissionId":null}}"#,
                AccountError::MissingPermissionId,
            ),
            (
                r#"{"user":{"permissionId":42}}"#,
                AccountError::InvalidPermissionId,
            ),
            (
                r#"{"user":{"permissionId":" p "}}"#,
                AccountError::InvalidPermissionId,
            ),
            (
                r#"{"user":{"permissionId":""}}"#,
                AccountError::InvalidPermissionId,
            ),
            (
                r#"{"user":{"permissionId":"p","emailAddress":true}}"#,
                AccountError::InvalidDisplayMetadata,
            ),
            (
                r#"{"user":{"permissionId":"p"},"storageQuota":false}"#,
                AccountError::InvalidQuota,
            ),
            (
                r#"{"user":{"permissionId":"p"},"storageQuota":{"usage":2}}"#,
                AccountError::InvalidQuota,
            ),
            (
                r#"{"user":{"permissionId":"p"},"error":{"message":"synthetic-secret-canary"}}"#,
                AccountError::ProviderErrorPayload,
            ),
        ] {
            assert_eq!(error(parse_about(body.as_bytes())), want);
        }
    }

    #[test]
    fn parser_decoded_duplicate_keys_never_overwrite_identity_or_metadata() {
        for body in [
            r#"{"user":{"permissionId":"A"},"u\u0073er":{"permissionId":"B"}}"#,
            r#"{"user":{"permissionId":"A","permission\u0049d":"B"}}"#,
            r#"{"user":{"permissionId":"A","displayName":"A","displayName":"B"}}"#,
            r#"{"user":{"permissionId":"A"},"storageQuota":{"usage":"1","us\u0061ge":"2"}}"#,
            r#"{"user":{"permissionId":"A"},"extension":1,"extension":2}"#,
        ] {
            assert_eq!(
                error(parse_about(body.as_bytes())),
                AccountError::MalformedResponse
            );
        }
        // Entirely ignored extension objects are not a duplicate-key proof boundary.
        assert!(parse_about(br#"{"user":{"permissionId":"A"},"extension":{"x":1,"x":2}}"#).is_ok());
    }

    #[test]
    fn parser_display_and_quota_limits_apply_before_blank_handling() {
        for value in [
            " ".repeat(1025),
            "x".repeat(257),
            "\n".into(),
            "é".repeat(257),
        ] {
            let body = serde_json::to_vec(
                &serde_json::json!({"user":{"permissionId":"p","displayName":value}}),
            )
            .expect("fixture");
            assert_eq!(
                error(parse_about(&body)),
                AccountError::InvalidDisplayMetadata
            );
        }
        let body = serde_json::to_vec(
            &serde_json::json!({"user":{"permissionId":"p","displayName":"🦀".repeat(256)}}),
        )
        .expect("fixture");
        assert!(parse_about(&body).is_ok());
        for value in [
            "-1",
            "01",
            "1.5",
            "9223372036854775808",
            "18446744073709551616",
        ] {
            let body = serde_json::to_vec(
                &serde_json::json!({"user":{"permissionId":"p"},"storageQuota":{"limit":value}}),
            )
            .expect("fixture");
            assert_eq!(error(parse_about(&body)), AccountError::InvalidQuota);
        }
        let body = serde_json::to_vec(
            &serde_json::json!({"user":{"permissionId":"p","emailAddress":" ".repeat(321)}}),
        )
        .expect("fixture");
        assert_eq!(
            error(parse_about(&body)),
            AccountError::InvalidDisplayMetadata
        );
    }

    #[test]
    fn parser_depth_work_size_fields_utf8_and_trailing_limits() {
        let mut exact = BODY_A.to_vec();
        exact.resize(BODY_LIMIT, b' ');
        assert!(parse_about(&exact).is_ok());
        exact.push(b' ');
        assert_eq!(error(parse_about(&exact)), AccountError::BodyLimit);
        let nested = format!(
            r#"{{"user":{{"permissionId":"p"}},"extension":{}0{}}}"#,
            "[".repeat(DEPTH_LIMIT),
            "]".repeat(DEPTH_LIMIT)
        );
        assert_eq!(
            error(parse_about(nested.as_bytes())),
            AccountError::ParseLimit(ParseLimit::Depth)
        );
        let wide = format!(
            r#"{{"user":{{"permissionId":"p"}},"extension":[{}]}}"#,
            vec!["0"; WORK_LIMIT].join(",")
        );
        assert_eq!(
            error(parse_about(wide.as_bytes())),
            AccountError::ParseLimit(ParseLimit::Work)
        );
        let fields = (0..MAP_LIMIT)
            .map(|n| format!("\"x{n}\":0"))
            .collect::<Vec<_>>()
            .join(",");
        let fields = format!(r#"{{"user":{{"permissionId":"p"}},{fields}}}"#);
        assert_eq!(
            error(parse_about(fields.as_bytes())),
            AccountError::ParseLimit(ParseLimit::Fields)
        );
        let key = format!(
            r#"{{"user":{{"permissionId":"p"}},"{}":0}}"#,
            "x".repeat(KEY_LIMIT + 1)
        );
        assert_eq!(
            error(parse_about(key.as_bytes())),
            AccountError::ParseLimit(ParseLimit::Key)
        );
        for bad in [
            b"[]".as_slice(),
            b"null",
            b"{",
            b"{\"x\":NaN}",
            b"{} {}",
            b"{\"x\":\"\xff\"}",
        ] {
            assert_eq!(error(parse_about(bad)), AccountError::MalformedResponse);
        }
    }

    #[tokio::test]
    async fn actual_get_exact_projection_privacy_and_partial_scopes() {
        let thumbnail = Server::reply(response(200, "", BODY_B));
        let body = serde_json::to_vec(&serde_json::json!({
            "user": {"permissionId":"permission_A", "photoLink":format!("http://{}/private",thumbnail.address)},
            "id_token":"synthetic-not-identity"
        })).expect("synthetic extension fixture");
        let server = Server::reply(response(200, "", &body));
        let candidate = execute(&server)
            .await
            .expect("synthetic authenticated response");
        assert_eq!(
            candidate.account_key().drive_permission_id.as_str(),
            "permission_A"
        );
        assert_eq!(candidate.account_key().project_number.value(), 123);
        assert_eq!(candidate.granted_scopes(), &[ApprovedScope::File]);
        assert_eq!(
            candidate.missing_requested_scopes(),
            &[ApprovedScope::AppData]
        );
        let requests = server.requests.lock().expect("capture");
        assert_eq!(requests.len(), 1);
        let wire = std::str::from_utf8(&requests[0]).expect("synthetic request");
        let (headers, body) = wire.split_once("\r\n\r\n").expect("headers");
        assert!(body.is_empty());
        let mut lines = headers.split("\r\n");
        let first = lines.next().expect("request line");
        let mut pieces = first.split(' ');
        assert_eq!(pieces.next(), Some("GET"));
        let target = pieces.next().expect("target");
        assert!(!target.contains("synthetic-token"));
        let url = Url::parse(&format!("http://127.0.0.1{target}")).expect("relative fixed target");
        assert_eq!(url.path(), "/drive/v3/about");
        assert_eq!(
            url.query_pairs().into_owned().collect::<Vec<_>>(),
            vec![("fields".to_owned(), FIELDS.to_owned())]
        );
        let auth: Vec<_> = lines
            .filter(|line| line.to_ascii_lowercase().starts_with("authorization:"))
            .collect();
        assert_eq!(auth.len(), 1);
        assert_eq!(
            auth[0].split_once(':').expect("header").1.trim(),
            "Bearer synthetic-token-A"
        );
        assert!(!wire.contains("synthetic-refresh-canary"));
        assert!(!wire.to_ascii_lowercase().contains("cookie:"));
        assert_eq!(thumbnail.count.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn actual_two_inputs_cannot_exchange_tokens_proof_or_context() {
        for (token, id, body, permission) in [
            ("synthetic-token-A", 1, BODY_A, "permission_A"),
            ("synthetic-token-B", 2, BODY_B, "permission_B"),
        ] {
            let server = Server::reply(response(200, "", body));
            let owned = input(token, &[ApprovedScope::File], Instant::now(), 3600, id);
            let deadline = Instant::now() + Duration::from_secs(5);
            let binding = context(&owned, "123", deadline, &[ApprovedScope::File]);
            let mut expected = context(&owned, "123", deadline, &[ApprovedScope::File]);
            let wrong_project = context(&owned, "456", deadline, &[ApprovedScope::File]);
            let (tx, rx) = oneshot::channel();
            let candidate = verify_at(
                owned,
                binding,
                rx,
                Target::Loopback(server.address),
                limits(),
            )
            .await
            .expect("candidate");
            assert_eq!(
                candidate.account_key().drive_permission_id.as_str(),
                permission
            );
            assert_eq!(
                candidate.input.tokens().access_token_for_native_adapter(),
                token
            );
            assert!(candidate.matches_native_attempt(&expected, Instant::now()));
            assert!(!candidate.matches_native_attempt(&wrong_project, Instant::now()));
            assert!(!candidate.matches_native_attempt(&expected, deadline));
            expected.attempt.generation += 1;
            assert!(!candidate.matches_native_attempt(&expected, Instant::now()));
            expected.attempt.generation -= 1;
            expected.registration_id = "changed-registration".into();
            assert!(!candidate.matches_native_attempt(&expected, Instant::now()));
            expected.registration_id = "desktop".into();
            expected.client_id = "other.apps.googleusercontent.com".into();
            assert!(!candidate.matches_native_attempt(&expected, Instant::now()));
            drop(tx);
        }
        // Context/results mismatch is rejected before an HTTP request, not guessed
        // from permissionId or email. Project authenticity itself remains configured.
        let server = Server::reply(response(200, "", BODY_A));
        for mismatch in 0..3 {
            let owned = ordinary();
            let mut binding = context(
                &owned,
                "123",
                Instant::now() + Duration::from_secs(5),
                &[ApprovedScope::File],
            );
            if mismatch == 0 {
                binding.attempt.generation += 1;
            } else if mismatch == 1 {
                binding.attempt.id = uuid::Uuid::from_u128(99);
            } else {
                binding.requested = vec![ApprovedScope::AppData];
            }
            let (tx, rx) = oneshot::channel();
            assert_eq!(
                error(
                    verify_at(
                        owned,
                        binding,
                        rx,
                        Target::Loopback(server.address),
                        limits()
                    )
                    .await
                ),
                if mismatch == 2 {
                    AccountError::InvalidGrantedScopes
                } else {
                    AccountError::AttemptMismatch
                }
            );
            drop(tx);
        }
        assert_eq!(server.count.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn actual_non_success_and_malformed_bodies_are_redacted() {
        for (code, want) in [
            (401, AccountError::Unauthorized),
            (403, AccountError::Forbidden),
            (429, AccountError::RateLimited),
            (503, AccountError::Server),
            (204, AccountError::UnexpectedStatus),
        ] {
            let server = Server::reply(response(code, "", b"<html>synthetic-secret-canary</html>"));
            let observed = error(execute(&server).await);
            assert_eq!(observed, want);
            assert!(!format!("{observed:?} {observed}").contains("synthetic-secret-canary"));
        }
        let server = Server::reply(response(200, "", b"<html>synthetic-token-A</html>"));
        let observed = error(execute(&server).await);
        assert_eq!(observed, AccountError::MalformedResponse);
        assert!(!format!("{observed:?}").contains("synthetic-token-A"));
    }

    #[tokio::test]
    async fn actual_redirects_never_forward_credentials_and_disconnect_is_not_replayed() {
        let destination = Server::reply(response(200, "", BODY_B));
        for code in [301, 302, 307, 308] {
            let source = Server::reply(response(
                code,
                &format!(
                    "Location: http://{}/drive/v3/about\r\n",
                    destination.address
                ),
                b"",
            ));
            assert_eq!(error(execute(&source).await), AccountError::Redirect);
            assert_eq!(source.count.load(Ordering::Acquire), 1);
        }
        assert_eq!(destination.count.load(Ordering::Acquire), 0);
        let close = Server::script(vec![]);
        assert_eq!(error(execute(&close).await), AccountError::Transport);
        assert_eq!(close.count.load(Ordering::Acquire), 1);
    }

    #[test]
    fn header_and_body_limits_have_exact_bounded_checks() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=\"UTF-8\""),
        );
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("00065536"));
        assert_eq!(success_headers(&headers), Ok(Some(BODY_LIMIT)));
        headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        assert_eq!(success_headers(&headers), Err(AccountError::InvalidHeaders));
        headers.remove(CONTENT_TYPE);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        assert_eq!(
            success_headers(&headers),
            Err(AccountError::ContentEncoding)
        );
        headers.remove(CONTENT_ENCODING);
        headers.insert(
            CONTENT_LENGTH,
            HeaderValue::from_static("184467440737095516160"),
        );
        assert_eq!(success_headers(&headers), Err(AccountError::BodyLimit));
        let mut many = HeaderMap::new();
        for _ in 0..65 {
            many.append("x-count", HeaderValue::from_static("x"));
        }
        assert_eq!(header_budget(&many), Err(AccountError::HeaderLimit));
        let mut large = HeaderMap::new();
        large.insert(
            "x-bytes",
            HeaderValue::from_bytes(&vec![b'a'; HEADER_LIMIT]).expect("synthetic header"),
        );
        assert_eq!(header_budget(&large), Err(AccountError::HeaderLimit));
        let mut body = vec![0; BODY_LIMIT - 1];
        assert_eq!(append_body(&mut body, b"x"), Ok(()));
        assert_eq!(append_body(&mut body, b"x"), Err(AccountError::BodyLimit));
        assert_eq!(body.len(), BODY_LIMIT);
    }

    #[tokio::test]
    async fn actual_content_headers_chunking_overflow_and_truncation() {
        let valid = Server::reply(chunked(BODY_A));
        assert!(execute(&valid).await.is_ok());
        for (bytes,want) in [
            (response(200, &"X-Count: x\r\n".repeat(65), BODY_A), AccountError::HeaderLimit),
            (response(200, &format!("X-Bytes: {}\r\n", "x".repeat(HEADER_LIMIT)), BODY_A), AccountError::HeaderLimit),

            (response(200,"Content-Encoding: gzip\r\n",BODY_A),AccountError::ContentEncoding),
            (response(200,"Content-Type: application/json\r\n",BODY_A),AccountError::InvalidHeaders),
            (b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),AccountError::InvalidHeaders),
            (chunked(&vec![b' ';BODY_LIMIT+1]),AccountError::BodyLimit),
            (b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n".to_vec(),AccountError::BodyLimit),
            (b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 50\r\nConnection: close\r\n\r\n{}".to_vec(),AccountError::Transport),
            (b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n10\r\n{}".to_vec(),AccountError::Transport),
        ] { let server=Server::reply(bytes); assert_eq!(error(execute(&server).await),want); }
    }

    #[tokio::test]
    async fn actual_cancel_signal_or_drop_before_dispatch_and_while_waiting() {
        let server = Server::script(vec![(
            Duration::from_millis(500),
            response(200, "", BODY_A),
        )]);
        for signal in [true, false] {
            let owned = ordinary();
            let binding = context(
                &owned,
                "123",
                Instant::now() + Duration::from_secs(5),
                &[ApprovedScope::File],
            );
            let (tx, rx) = oneshot::channel();
            if signal {
                let _ = tx.send(());
            } else {
                drop(tx);
            }
            assert_eq!(
                error(
                    verify_at(
                        owned,
                        binding,
                        rx,
                        Target::Loopback(server.address),
                        limits()
                    )
                    .await
                ),
                AccountError::Cancelled
            );
        }
        assert_eq!(server.count.load(Ordering::Acquire), 0);
        for signal in [true, false] {
            let delayed = Server::script(vec![(
                Duration::from_millis(500),
                response(200, "", BODY_A),
            )]);
            let owned = ordinary();
            let binding = context(
                &owned,
                "123",
                Instant::now() + Duration::from_secs(5),
                &[ApprovedScope::File],
            );
            let (tx, rx) = oneshot::channel();
            let address = delayed.address;
            let operation = verify_at(owned, binding, rx, Target::Loopback(address), limits());
            tokio::pin!(operation);
            tokio::select! {
                _ = &mut operation => panic!("request completed before controlled cancellation"),
                _ = delayed.captured() => {}
            }
            if signal {
                let _ = tx.send(());
            } else {
                drop(tx);
            }
            assert_eq!(error(operation.await), AccountError::Cancelled);
            assert_eq!(delayed.count.load(Ordering::Acquire), 1);
        }
    }

    #[tokio::test]
    async fn actual_deadlines_cover_stalls_drips_and_expiring_input() {
        for mode in 0..3 {
            let parts = if mode == 0 {
                vec![(Duration::from_millis(250), response(200, "", BODY_A))]
            } else {
                vec![(Duration::ZERO,b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec()),(Duration::from_millis(40),b"1\r\n \r\n".to_vec()),(Duration::from_millis(40),b"1\r\n \r\n".to_vec()),(Duration::from_millis(200),b"0\r\n\r\n".to_vec())]
            };
            let server = Server::script(parts);
            let owned = ordinary();
            let binding = context(
                &owned,
                "123",
                Instant::now() + Duration::from_millis(70),
                &[ApprovedScope::File],
            );
            let (tx, rx) = oneshot::channel();
            let limit = Limits {
                total: Duration::from_secs(2),
                connect: Duration::from_millis(200),
                read: if mode == 2 {
                    Duration::from_millis(15)
                } else {
                    Duration::from_millis(200)
                },
            };
            assert_eq!(
                error(verify_at(owned, binding, rx, Target::Loopback(server.address), limit).await),
                AccountError::Deadline
            );
            drop(tx);
        }
        let server = Server::script(vec![(
            Duration::from_millis(250),
            response(200, "", BODY_A),
        )]);
        let owned = input(
            "synthetic-short",
            &[ApprovedScope::File],
            Instant::now() - Duration::from_millis(900),
            1,
            1,
        );
        let binding = context(
            &owned,
            "123",
            Instant::now() + Duration::from_secs(5),
            &[ApprovedScope::File],
        );
        let (tx, rx) = oneshot::channel();
        assert_eq!(
            error(
                verify_at(
                    owned,
                    binding,
                    rx,
                    Target::Loopback(server.address),
                    limits()
                )
                .await
            ),
            AccountError::TokenExpired
        );
        drop(tx);
    }

    #[tokio::test]
    async fn expired_future_clock_and_bad_header_credentials_fail_without_dispatch() {
        let server = Server::reply(response(200, "", BODY_A));
        for (token, issued, expires, want) in [
            (
                "synthetic-expired",
                Instant::now() - Duration::from_secs(2),
                1,
                AccountError::TokenExpired,
            ),
            (
                "synthetic-future",
                Instant::now() + Duration::from_secs(2),
                3600,
                AccountError::ClockRegression,
            ),
            (
                "synthetic!invalid",
                Instant::now(),
                3600,
                AccountError::InvalidBearerCredential,
            ),
        ] {
            let owned = input(token, &[ApprovedScope::File], issued, expires, 1);
            let binding = context(
                &owned,
                "123",
                Instant::now() + Duration::from_secs(5),
                &[ApprovedScope::File],
            );
            let (tx, rx) = oneshot::channel();
            assert_eq!(
                error(
                    verify_at(
                        owned,
                        binding,
                        rx,
                        Target::Loopback(server.address),
                        limits()
                    )
                    .await
                ),
                want
            );
            drop(tx);
        }
        assert_eq!(server.count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn final_gate_rejects_cancel_or_expiry_without_making_parser_proof() {
        let owned = ordinary();
        let binding = context(
            &owned,
            "123",
            Instant::now() + Duration::from_secs(5),
            &[ApprovedScope::File],
        );
        let parsed = parse_about(BODY_A).expect("data only");
        let (tx, mut rx) = oneshot::channel();
        drop(tx);
        assert_eq!(
            error(finish(
                owned,
                binding,
                parsed,
                &mut rx,
                Instant::now() + Duration::from_secs(1)
            )),
            AccountError::Cancelled
        );
        let owned = ordinary();
        let binding = context(
            &owned,
            "123",
            Instant::now() + Duration::from_secs(5),
            &[ApprovedScope::File],
        );
        let parsed = parse_about(BODY_A).expect("data only");
        let (tx, mut rx) = oneshot::channel();
        assert_eq!(
            error(finish(owned, binding, parsed, &mut rx, Instant::now())),
            AccountError::Deadline
        );
        drop(tx);
    }
}

// Synthetic inputs are available to sibling tests only. No production endpoint,
// credential setter or verified-candidate constructor is exported by this module.
#[cfg(test)]
pub(in crate::drive_auth) mod session_test_support {
    use super::{
        AccountError, ApprovedScope, ExchangeAttempt, Limits, OwnedInput, ParsedTokenResponse,
        RegistrationField, Target, VerificationContext, VerifiedAccountCandidate, verify_at,
    };
    use crate::drive_auth::token_response::{TokenResponseError, parse_google_token_response};
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;

    #[derive(Clone, Copy)]
    pub(in crate::drive_auth) enum SyntheticLabel {
        A,
        B,
    }

    /// Static errors around existing binding/parser errors, confined to tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(in crate::drive_auth) enum FixtureError {
        Binding(AccountError),
        Tokens(TokenResponseError),
    }

    /// Input syntax only, never account proof. No raw token/response argument,
    /// public fields, Debug, Clone, deserializer or credential accessor.
    pub(in crate::drive_auth) struct FixtureSpec {
        attempt: ExchangeAttempt,
        tokens: ParsedTokenResponse,
    }
    impl FixtureSpec {
        pub(in crate::drive_auth) fn new(
            label: SyntheticLabel,
            attempt: ExchangeAttempt,
            actual_scopes: &[ApprovedScope],
            issued_at: Instant,
            expires_in: u64,
        ) -> Result<Self, FixtureError> {
            if attempt.id.is_nil() || attempt.generation == 0 {
                return Err(FixtureError::Binding(AccountError::InvalidRegistration(
                    RegistrationField::AttemptIdentity,
                )));
            }
            // Validate before constructing a scope string; even fixture requests
            // cannot allocate an arbitrarily large scope list. Unsupported
            // scopes/labels are unrepresentable in the closed input enums.
            if actual_scopes.is_empty()
                || actual_scopes.len() > 2
                || (actual_scopes.len() == 2 && actual_scopes[0] == actual_scopes[1])
            {
                return Err(FixtureError::Tokens(
                    TokenResponseError::InvalidRequestedScopes,
                ));
            }
            let (access, refresh) = match label {
                SyntheticLabel::A => ("synthetic-session-A", "synthetic-session-refresh-A"),
                SyntheticLabel::B => ("synthetic-session-B", "synthetic-session-refresh-B"),
            };
            let scope = actual_scopes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            // All text here comes from fixed literals or the closed scope enum.
            let body = format!(
                r#"{{"access_token":"{access}","refresh_token":"{refresh}","token_type":"Bearer","expires_in":{expires_in},"scope":"{scope}"}}"#,
            );
            let tokens = parse_google_token_response(
                200,
                body.as_bytes(),
                actual_scopes,
                issued_at,
                issued_at,
            )
            .map_err(FixtureError::Tokens)?;
            // Parse exactly once at the caller's explicit fixture time. Retaining
            // this result prevents verify_fixture from renewing its expiry.
            Ok(Self { attempt, tokens })
        }
    }

    pub(in crate::drive_auth) async fn verify_fixture(
        spec: FixtureSpec,
        context: VerificationContext,
        cancel: oneshot::Receiver<()>,
        loopback: SocketAddr,
    ) -> Result<VerifiedAccountCandidate, AccountError> {
        verify_at(
            OwnedInput::Fixture {
                attempt: spec.attempt,
                tokens: spec.tokens,
            },
            context,
            cancel,
            Target::Loopback(loopback),
            Limits {
                total: Duration::from_secs(2),
                connect: Duration::from_millis(500),
                read: Duration::from_millis(500),
            },
        )
        .await
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::session_test_support::{FixtureError, FixtureSpec, SyntheticLabel, verify_fixture};
    use super::{
        AccountError, ApprovedScope, ExchangeAttempt, RegistrationField, VerificationContext,
    };
    use crate::drive_auth::token_response::TokenResponseError;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;
    use uuid::Uuid;

    fn attempt(id: u128) -> ExchangeAttempt {
        ExchangeAttempt {
            id: Uuid::from_u128(id),
            generation: 11,
        }
    }
    fn context(id: u128, deadline: Instant) -> VerificationContext {
        VerificationContext::from_native_attempt(
            "desktop-fixture",
            "synthetic-lifetime.apps.googleusercontent.com",
            "123",
            &[ApprovedScope::AppData, ApprovedScope::File],
            &attempt(id),
            deadline,
        )
        .expect("bounded native fixture context")
    }
    fn failure<T, E>(result: Result<T, E>) -> E {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected fixture failure"),
        }
    }

    enum Reply {
        A,
        B,
        MissingUser,
    }
    // One request, at most16KiB captured, fixed response,3s worker lifetime,
    // stop-aware accept/read loop and finite writes. Joined Drop on panic too.
    // This is a new owned peer; existing author/independent fixtures are untouched.
    struct Peer {
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        request: Arc<Mutex<Option<Vec<u8>>>>,
        worker: Option<JoinHandle<()>>,
    }
    impl Peer {
        fn new(reply: Reply) -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("ephemeral peer");
            listener.set_nonblocking(true).expect("nonblocking peer");
            let address = listener.local_addr().expect("local address");
            let stop = Arc::new(AtomicBool::new(false));
            let request = Arc::new(Mutex::new(None));
            let (halt, captured) = (Arc::clone(&stop), Arc::clone(&request));
            let worker = thread::spawn(move || {
                let end = Instant::now() + Duration::from_secs(3);
                while !halt.load(Ordering::Acquire) && Instant::now() < end {
                    let (mut stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => return,
                    };
                    if stream
                        .set_read_timeout(Some(Duration::from_millis(10)))
                        .is_err()
                        || stream
                            .set_write_timeout(Some(Duration::from_millis(20)))
                            .is_err()
                    {
                        return;
                    }
                    let mut bytes = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !bytes.windows(4).any(|part| part == b"\r\n\r\n")
                        && !halt.load(Ordering::Acquire)
                        && Instant::now() < end
                    {
                        match stream.read(&mut buffer) {
                            Ok(0) => return,
                            Ok(n) => {
                                if bytes.len() + n > 16 * 1024 {
                                    return;
                                }
                                bytes.extend_from_slice(&buffer[..n]);
                            }
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                                ) => {}
                            Err(_) => return,
                        }
                    }
                    if halt.load(Ordering::Acquire) || Instant::now() >= end {
                        return;
                    }
                    *captured.lock().expect("fixture capture") = Some(bytes);
                    let body = match reply {
                        Reply::A => r#"{"user":{"permissionId":"session_A"}}"#,
                        Reply::B => r#"{"user":{"permissionId":"session_B"}}"#,
                        Reply::MissingUser => r#"{}"#,
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len(),
                    );
                    let _ = stream.write_all(response.as_bytes());
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
        fn captured(&self) -> Vec<u8> {
            self.request
                .lock()
                .expect("fixture capture")
                .as_ref()
                .expect("captured request")
                .clone()
        }
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    #[tokio::test]
    async fn lifetime_metadata_separates_admission_validation_and_expiry_without_renewal() {
        let peer = Peer::new(Reply::A);
        let issued = Instant::now();
        let admission = issued + Duration::from_secs(5);
        let expected_expiry = issued + Duration::from_secs(60);
        let spec = FixtureSpec::new(
            SyntheticLabel::A,
            attempt(1),
            &[ApprovedScope::File],
            issued,
            60,
        )
        .expect("synthetic input");
        let expected = context(1, admission);
        let (tx, rx) = oneshot::channel();
        let candidate = verify_fixture(spec, context(1, admission), rx, peer.address)
            .await
            .expect("actual synthetic HTTP candidate");
        let now = Instant::now();
        assert!(now < admission);
        assert_eq!(candidate.credential_expires_at(), expected_expiry);
        assert!(candidate.credentials_usable_at(now));
        assert!(candidate.matches_native_attempt(&expected, now));
        assert!(!candidate.credentials_usable_at(issued - Duration::from_millis(1)));
        assert!(candidate.credentials_usable_at(admission));
        assert!(!candidate.matches_native_attempt(&expected, admission));
        assert!(candidate.credentials_usable_at(expected_expiry - Duration::from_nanos(1)));
        assert!(!candidate.credentials_usable_at(expected_expiry));
        assert!(!candidate.credentials_usable_at(expected_expiry + Duration::from_millis(1)));
        assert!(!candidate.matches_native_attempt(&expected, expected_expiry));
        // These are explicit clock comparisons, not sleeps or altered credentials.
        assert_eq!(candidate.credential_expires_at(), expected_expiry);
        drop(tx);
    }

    #[tokio::test]
    async fn sibling_bridge_binds_distinct_fixed_credentials_to_actual_http_candidates() {
        for (label, reply, id, access, permission) in [
            (
                SyntheticLabel::A,
                Reply::A,
                1,
                "Bearer synthetic-session-A",
                "session_A",
            ),
            (
                SyntheticLabel::B,
                Reply::B,
                2,
                "Bearer synthetic-session-B",
                "session_B",
            ),
        ] {
            let peer = Peer::new(reply);
            let issued = Instant::now();
            let spec = FixtureSpec::new(label, attempt(id), &[ApprovedScope::File], issued, 60)
                .expect("synthetic input");
            let (tx, rx) = oneshot::channel();
            let candidate = verify_fixture(
                spec,
                context(id, issued + Duration::from_secs(5)),
                rx,
                peer.address,
            )
            .await
            .expect("actual verifier response");
            assert_eq!(
                candidate.account_key().drive_permission_id.as_str(),
                permission
            );
            assert_eq!(candidate.granted_scopes(), &[ApprovedScope::File]);
            assert_eq!(
                candidate.missing_requested_scopes(),
                &[ApprovedScope::AppData]
            );
            assert_eq!(
                candidate.credential_expires_at(),
                issued + Duration::from_secs(60)
            );
            let captured = peer.captured();
            let wire = std::str::from_utf8(&captured).expect("synthetic ASCII request");
            assert!(wire.starts_with("GET /drive/v3/about?fields="));
            let authorization: Vec<_> = wire
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then_some(value.trim())
                })
                .collect();
            assert_eq!(authorization, vec![access]);
            assert!(!wire.contains("synthetic-session-refresh"));
            assert!(
                !wire
                    .lines()
                    .next()
                    .expect("request line")
                    .contains("synthetic-session")
            );
            drop(tx);
        }
    }

    #[test]
    fn fixture_constructor_rejects_invalid_binding_scopes_and_lifetime() {
        let now = Instant::now();
        for invalid in [
            ExchangeAttempt {
                id: Uuid::nil(),
                generation: 11,
            },
            ExchangeAttempt {
                id: Uuid::from_u128(1),
                generation: 0,
            },
        ] {
            assert_eq!(
                failure(FixtureSpec::new(
                    SyntheticLabel::A,
                    invalid,
                    &[ApprovedScope::File],
                    now,
                    60
                )),
                FixtureError::Binding(AccountError::InvalidRegistration(
                    RegistrationField::AttemptIdentity
                ))
            );
        }
        for invalid in [
            vec![],
            vec![ApprovedScope::File; 2],
            vec![ApprovedScope::File; 3],
        ] {
            assert_eq!(
                failure(FixtureSpec::new(
                    SyntheticLabel::B,
                    attempt(1),
                    &invalid,
                    now,
                    60
                )),
                FixtureError::Tokens(TokenResponseError::InvalidRequestedScopes)
            );
        }
        assert_eq!(
            failure(FixtureSpec::new(
                SyntheticLabel::A,
                attempt(1),
                &[ApprovedScope::File],
                now,
                0
            )),
            FixtureError::Tokens(TokenResponseError::InvalidExpiry)
        );
        assert_eq!(
            failure(FixtureSpec::new(
                SyntheticLabel::A,
                attempt(1),
                &[ApprovedScope::File],
                now,
                u64::MAX
            )),
            FixtureError::Tokens(TokenResponseError::ExpiryOverflow)
        );
        // Closed enums admit neither arbitrary token labels nor broad scopes.
        assert!(
            FixtureSpec::new(
                SyntheticLabel::B,
                attempt(1),
                &[ApprovedScope::AppData, ApprovedScope::File],
                now,
                60
            )
            .is_ok()
        );
    }

    #[tokio::test]
    async fn bridge_requires_a_valid_local_transport_response_and_matching_binding() {
        let peer = Peer::new(Reply::MissingUser);
        let issued = Instant::now();
        let spec = FixtureSpec::new(
            SyntheticLabel::A,
            attempt(1),
            &[ApprovedScope::File],
            issued,
            60,
        )
        .expect("fixture");
        let (tx, rx) = oneshot::channel();
        assert_eq!(
            failure(
                verify_fixture(
                    spec,
                    context(1, issued + Duration::from_secs(5)),
                    rx,
                    peer.address
                )
                .await
            ),
            AccountError::MissingUser
        );
        assert!(!peer.captured().is_empty());
        drop(tx);
        for (address, expected) in [
            (
                SocketAddr::from(([0, 0, 0, 0], 12345)),
                AccountError::ClientSetup,
            ),
            (
                SocketAddr::from(([127, 0, 0, 1], 0)),
                AccountError::ClientSetup,
            ),
        ] {
            let spec = FixtureSpec::new(
                SyntheticLabel::B,
                attempt(1),
                &[ApprovedScope::File],
                issued,
                60,
            )
            .expect("fixture");
            let (tx, rx) = oneshot::channel();
            assert_eq!(
                failure(
                    verify_fixture(
                        spec,
                        context(1, issued + Duration::from_secs(5)),
                        rx,
                        address
                    )
                    .await
                ),
                expected
            );
            drop(tx);
        }
        let spec = FixtureSpec::new(
            SyntheticLabel::A,
            attempt(1),
            &[ApprovedScope::File],
            issued,
            60,
        )
        .expect("fixture");
        let (tx, rx) = oneshot::channel();
        assert_eq!(
            failure(
                verify_fixture(
                    spec,
                    context(2, issued + Duration::from_secs(5)),
                    rx,
                    peer.address
                )
                .await
            ),
            AccountError::AttemptMismatch
        );
        drop(tx);
    }
}

#[cfg(test)]
#[path = "account_independent_tests.rs"]
mod independent_tests;
