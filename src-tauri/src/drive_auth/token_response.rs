//! Bounded native parsing of a Google OAuth token endpoint response.
//!
//! This is not an HTTP client, authenticated provenance/account proof, credential
//! store or refresh policy. The caller must bind these bytes to its verified TLS
//! token request and exact requested scopes. Never put body bytes or parsed tokens
//! in UI IPC/logs. Strings are dropped normally, not guaranteed zeroized.

use std::collections::BTreeSet;
use std::fmt;
use std::time::{Duration, Instant};

use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::pkce::ApprovedScope;

pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_JSON_DEPTH: usize = 16;
pub const MAX_STRUCTURAL_TOKENS: usize = 4096;
pub const MAX_TOKEN_BYTES: usize = 8192;
const MAX_ROOT_FIELDS: usize = 32;
const MAX_KEY_BYTES: usize = 128;
const MAX_SCOPE_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenField {
    AccessToken,
    TokenType,
    ExpiresIn,
    Scope,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpFailure {
    UnexpectedStatus,
    Redirect,
    BadRequest,
    Unauthorized,
    Forbidden,
    RateLimited,
    Client,
    Server,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    InvalidRequest,
    InvalidClient,
    InvalidGrant,
    UnauthorizedClient,
    UnsupportedGrantType,
    InvalidScope,
    Other,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenResponseError {
    InvalidStatus,
    InvalidRequestedScopes,
    BodyLimit,
    DepthLimit,
    WorkLimit,
    MalformedResponse,
    MissingField(TokenField),
    InvalidAccessToken,
    InvalidRefreshToken,
    UnsupportedTokenType,
    InvalidExpiry,
    ExpiryOverflow,
    ExpiredAtReceipt,
    ClockRegression,
    InvalidScopeClaim,
    UnrequestedScope,
    AmbiguousResponse,
    UnexpectedErrorOnSuccess,
    Http(HttpFailure),
    Provider {
        http: HttpFailure,
        error: ProviderFailure,
    },
}
impl fmt::Display for TokenResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token response error: {self:?}")
    }
}
impl std::error::Error for TokenResponseError {}

/// Syntactically validated provider claims only. No Debug/Display/Clone/serde.
/// The tokens are opaque; neither token contents nor reported scopes prove
/// provenance/account ownership. No mutation of an existing credential occurs.
pub struct ParsedTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    granted_scopes: Vec<ApprovedScope>,
    expires_at: Instant,
    validated_at: Instant,
}
impl ParsedTokenResponse {
    pub fn access_token_for_native_adapter(&self) -> &str {
        &self.access_token
    }
    /// None means NOT SUPPLIED. It never means delete/revoke an existing token.
    pub fn refresh_token_for_native_adapter(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }
    /// Exactly the explicit reported subset, never requested/previous-grant union.
    pub fn granted_scopes(&self) -> &[ApprovedScope] {
        &self.granted_scopes
    }
    pub fn expires_at(&self) -> Instant {
        self.expires_at
    }
    /// Call before use; parsing cannot keep a retained token perpetually usable.
    /// Reject a caller clock earlier than the validation point as well as expiry.
    pub fn is_usable_at(&self, now: Instant) -> bool {
        now >= self.validated_at && now < self.expires_at
    }
}

/// Requires status 200, explicit scope and positive integer expires_in. RFC6749
/// permits omitted scope when unchanged, but this Google adapter deliberately
/// rejects that ambiguity; it does not manufacture grants from the request.
///
/// Capture request_started_at immediately before sending the token request, and
/// now after receiving it. Expiry is conservatively anchored at request start so
/// transit/parse time cannot extend lifetime. No wall clock or expiry default.
/// Unknown response fields are ignored under the same JSON resource bounds.
pub fn parse_google_token_response(
    http_status: u16,
    body: &[u8],
    requested: &[ApprovedScope],
    request_started_at: Instant,
    now: Instant,
) -> Result<ParsedTokenResponse, TokenResponseError> {
    if !(100..=599).contains(&http_status) {
        return Err(TokenResponseError::InvalidStatus);
    }
    if requested.is_empty()
        || requested.len() > 2
        || requested
            .iter()
            .enumerate()
            .any(|(i, scope)| requested[..i].contains(scope))
    {
        return Err(TokenResponseError::InvalidRequestedScopes);
    }
    if now < request_started_at {
        return Err(TokenResponseError::ClockRegression);
    }
    check_json_budget(body)?;
    let http_failure = classify_http(http_status);
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let fields = RawFields::deserialize(&mut decoder)
        .and_then(|fields| decoder.end().map(|()| fields))
        .map_err(|_| {
            http_failure.map_or(
                TokenResponseError::MalformedResponse,
                TokenResponseError::Http,
            )
        })?;
    let has_success = fields.access_token.is_some()
        || fields.refresh_token.is_some()
        || fields.token_type.is_some()
        || fields.expires_in.is_some()
        || fields.scope.is_some();
    let has_error = fields.error.is_some() || fields.error_details;
    if has_success && has_error {
        return Err(TokenResponseError::AmbiguousResponse);
    }
    if let Some(http) = http_failure {
        return Err(match fields.error {
            Some(error) => TokenResponseError::Provider {
                http,
                error: classify_provider(&error),
            },
            None => TokenResponseError::Http(http),
        });
    }
    if has_error {
        return Err(TokenResponseError::UnexpectedErrorOnSuccess);
    }
    let access_token = fields
        .access_token
        .ok_or(TokenResponseError::MissingField(TokenField::AccessToken))?;
    if !valid_token(&access_token) {
        return Err(TokenResponseError::InvalidAccessToken);
    }
    if fields
        .refresh_token
        .as_ref()
        .is_some_and(|token| !valid_token(token))
    {
        return Err(TokenResponseError::InvalidRefreshToken);
    }
    let token_type = fields
        .token_type
        .ok_or(TokenResponseError::MissingField(TokenField::TokenType))?;
    if !token_type.eq_ignore_ascii_case("Bearer") {
        return Err(TokenResponseError::UnsupportedTokenType);
    }
    let seconds = fields
        .expires_in
        .ok_or(TokenResponseError::MissingField(TokenField::ExpiresIn))?;
    if seconds == 0 {
        return Err(TokenResponseError::InvalidExpiry);
    }
    let expires_at = request_started_at
        .checked_add(Duration::from_secs(seconds))
        .ok_or(TokenResponseError::ExpiryOverflow)?;
    if now >= expires_at {
        return Err(TokenResponseError::ExpiredAtReceipt);
    }
    let scope = fields
        .scope
        .ok_or(TokenResponseError::MissingField(TokenField::Scope))?;
    let granted_scopes = parse_granted_scopes(&scope, requested)?;
    Ok(ParsedTokenResponse {
        access_token,
        refresh_token: fields.refresh_token,
        granted_scopes,
        expires_at,
        validated_at: now,
    })
}

fn valid_token(token: &str) -> bool {
    // Transport-safe opaque secret: no trim, base64/JWT decode, prefix inference
    // or ASCII punctuation restrictions. Bounds are adapter policy, not RFC maxima.
    !token.is_empty()
        && token.len() <= MAX_TOKEN_BYTES
        && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}
fn parse_granted_scopes(
    raw: &str,
    requested: &[ApprovedScope],
) -> Result<Vec<ApprovedScope>, TokenResponseError> {
    if raw.is_empty() || raw.len() > MAX_SCOPE_BYTES {
        return Err(TokenResponseError::InvalidScopeClaim);
    }
    let mut result = Vec::with_capacity(2);
    for token in raw.split(' ') {
        let scope = match token {
            "https://www.googleapis.com/auth/drive.appdata" => ApprovedScope::AppData,
            "https://www.googleapis.com/auth/drive.file" => ApprovedScope::File,
            _ => return Err(TokenResponseError::InvalidScopeClaim),
        };
        if result.contains(&scope) {
            return Err(TokenResponseError::InvalidScopeClaim);
        }
        if !requested.contains(&scope) {
            return Err(TokenResponseError::UnrequestedScope);
        }
        result.push(scope);
    }
    Ok(result)
}
fn classify_http(status: u16) -> Option<HttpFailure> {
    Some(match status {
        200 => return None,
        300..=399 => HttpFailure::Redirect,
        400 => HttpFailure::BadRequest,
        401 => HttpFailure::Unauthorized,
        403 => HttpFailure::Forbidden,
        429 => HttpFailure::RateLimited,
        402 | 404..=428 | 430..=499 => HttpFailure::Client,
        500..=599 => HttpFailure::Server,
        _ => HttpFailure::UnexpectedStatus,
    })
}
fn classify_provider(error: &str) -> ProviderFailure {
    match error {
        "invalid_request" => ProviderFailure::InvalidRequest,
        "invalid_client" => ProviderFailure::InvalidClient,
        "invalid_grant" => ProviderFailure::InvalidGrant,
        "unauthorized_client" => ProviderFailure::UnauthorizedClient,
        "unsupported_grant_type" => ProviderFailure::UnsupportedGrantType,
        "invalid_scope" => ProviderFailure::InvalidScope,
        _ => ProviderFailure::Other,
    }
}

// Linear resource preflight ONLY, not a JSON parser. Counts nesting and structural
// punctuation outside strings before serde can recurse/skip unknown values. Serde
// remains responsible for full syntax, UTF-8, number types and trailing-content
// validation. Escaped delimiters/quotes inside strings do not consume this budget.
fn check_json_budget(body: &[u8]) -> Result<(), TokenResponseError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(TokenResponseError::BodyLimit);
    }
    let (mut depth, mut work, mut quoted, mut escaped) = (0usize, 0usize, false, false);
    for &byte in body {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                work += 1;
                if depth > MAX_JSON_DEPTH {
                    return Err(TokenResponseError::DepthLimit);
                }
            }
            b'}' | b']' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(TokenResponseError::MalformedResponse)?;
            }
            b',' | b':' => work += 1,
            _ => {}
        }
        if work > MAX_STRUCTURAL_TOKENS {
            return Err(TokenResponseError::WorkLimit);
        }
    }
    if quoted || depth != 0 {
        return Err(TokenResponseError::MalformedResponse);
    }
    Ok(())
}

// Private wire staging only: no Value tree, logging, Debug or public Deserialize.
#[derive(Default)]
struct RawFields {
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    error: Option<String>,
    error_details: bool,
}
impl<'de> Deserialize<'de> for RawFields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldsVisitor;
        impl<'de> Visitor<'de> for FieldsVisitor {
            type Value = RawFields;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("token response object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = RawFields::default();
                let mut seen = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key.len() > MAX_KEY_BYTES || seen.len() >= MAX_ROOT_FIELDS {
                        return Err(de::Error::custom("response field limit"));
                    }
                    // Decoded keys detect escaped aliases, including unknown
                    // root duplicates. Nested extension objects have no semantics.
                    if !seen.insert(key.clone()) {
                        return Err(de::Error::custom("duplicate response field"));
                    }
                    match key.as_str() {
                        "access_token" => fields.access_token = Some(map.next_value::<String>()?),
                        "refresh_token" => fields.refresh_token = Some(map.next_value::<String>()?),
                        "token_type" => fields.token_type = Some(map.next_value::<String>()?),
                        // serde's u64 visitor rejects floats/exponents, strings,
                        // negative values and overflow; absent is distinct from null.
                        "expires_in" => fields.expires_in = Some(map.next_value::<u64>()?),
                        "scope" => fields.scope = Some(map.next_value::<String>()?),
                        "error" => {
                            let error = map.next_value::<String>()?;
                            if error.is_empty() || error.len() > 128 || !error.bytes().all(nqschar)
                            {
                                return Err(de::Error::custom("invalid provider error"));
                            }
                            fields.error = Some(error);
                        }
                        "error_description" | "error_uri" => {
                            let discarded = map.next_value::<String>()?;
                            if discarded.len() > 2048 || !discarded.bytes().all(nqschar) {
                                return Err(de::Error::custom("invalid error detail"));
                            }
                            fields.error_details = true;
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(fields)
            }
        }
        deserializer.deserialize_map(FieldsVisitor)
    }
}
fn nqschar(byte: u8) -> bool {
    matches!(byte, 0x20..=0x21 | 0x23..=0x5b | 0x5d..=0x7e)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACCESS: &str = "synthetic-access+/=_-.~";
    const REFRESH: &str = "synthetic-refresh+/=_-.~";
    fn quoted(text: &str) -> String {
        serde_json::to_string(text).unwrap()
    }
    fn fixture(scope: &str, refresh: bool) -> String {
        let refresh = if refresh {
            format!(",\"refresh_token\":{}", quoted(REFRESH))
        } else {
            String::new()
        };
        format!(
            "{{\"access_token\":{},\"token_type\":\"Bearer\",\"expires_in\":3600,\"scope\":{}{refresh}}}",
            quoted(ACCESS),
            quoted(scope)
        )
    }
    fn parse(
        body: &str,
        requested: &[ApprovedScope],
    ) -> Result<ParsedTokenResponse, TokenResponseError> {
        let now = Instant::now();
        parse_google_token_response(200, body.as_bytes(), requested, now, now)
    }
    fn rejected(body: &str) -> TokenResponseError {
        parse(body, &[ApprovedScope::AppData, ApprovedScope::File])
            .err()
            .expect("must reject")
    }
    fn with_extra(body: &str, extra: &str) -> String {
        format!("{},{} }}", &body[..body.len() - 1], extra)
    }

    #[test]
    fn valid_code_response_preserves_native_secrets_and_explicit_scope() {
        let start = Instant::now();
        let now = start + Duration::from_secs(2);
        let body = fixture(ApprovedScope::AppData.as_str(), true);
        let parsed = parse_google_token_response(
            200,
            body.as_bytes(),
            &[ApprovedScope::AppData],
            start,
            now,
        )
        .unwrap();
        assert_eq!(parsed.access_token_for_native_adapter(), ACCESS);
        assert_eq!(parsed.refresh_token_for_native_adapter(), Some(REFRESH));
        assert_eq!(parsed.granted_scopes(), &[ApprovedScope::AppData]);
        assert_eq!(parsed.expires_at(), start + Duration::from_secs(3600));
        assert!(parsed.is_usable_at(now));
        assert!(!parsed.is_usable_at(start));
        assert!(!parsed.is_usable_at(parsed.expires_at()));
    }

    #[test]
    fn renewal_absence_is_not_revocation_and_rotation_is_preserved() {
        let requested = [ApprovedScope::File];
        let body = fixture(ApprovedScope::File.as_str(), false);
        let parsed = parse(&body, &requested).unwrap();
        assert_eq!(parsed.refresh_token_for_native_adapter(), None);
        let rotated = fixture(ApprovedScope::File.as_str(), true)
            .replace(REFRESH, "synthetic-rotated-refresh");
        let parsed = parse(&rotated, &requested).unwrap();
        assert_eq!(
            parsed.refresh_token_for_native_adapter(),
            Some("synthetic-rotated-refresh")
        );
        // A parser result contains no operation on any preexisting credential.
        let null = with_extra(&body, "\"refresh_token\":null");
        assert!(parse(&null, &requested).is_err());
    }

    #[test]
    fn partial_approval_never_expands_to_requested_union() {
        let both = [ApprovedScope::AppData, ApprovedScope::File];
        for scope in both {
            let parsed = parse(&fixture(scope.as_str(), false), &both).unwrap();
            assert_eq!(parsed.granted_scopes(), &[scope]);
        }
        let scope = format!(
            "{} {}",
            ApprovedScope::File.as_str(),
            ApprovedScope::AppData.as_str()
        );
        let parsed = parse(&fixture(&scope, false), &both).unwrap();
        assert_eq!(
            parsed.granted_scopes(),
            &[ApprovedScope::File, ApprovedScope::AppData]
        );
        assert_eq!(
            parse(
                &fixture(ApprovedScope::File.as_str(), false),
                &[ApprovedScope::AppData]
            )
            .err(),
            Some(TokenResponseError::UnrequestedScope)
        );
    }

    #[test]
    fn missing_unknown_broad_empty_duplicate_and_malformed_scopes_fail() {
        let body = fixture(ApprovedScope::AppData.as_str(), false);
        let missing = body.replace(
            &format!(",\"scope\":{}", quoted(ApprovedScope::AppData.as_str())),
            "",
        );
        assert_eq!(
            rejected(&missing),
            TokenResponseError::MissingField(TokenField::Scope)
        );
        for scope in [
            "",
            "https://www.googleapis.com/auth/drive",
            "https://www.googleapis.com/auth/drive.readonly",
            "openid",
            "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/drive.file",
            " https://www.googleapis.com/auth/drive.file",
            "https://www.googleapis.com/auth/drive.file ",
            "https://www.googleapis.com/auth/drive.file\thttps://www.googleapis.com/auth/drive.appdata",
            "https://www.googleapis.com/auth/drive.file  https://www.googleapis.com/auth/drive.appdata",
        ] {
            assert_eq!(
                rejected(&fixture(scope, false)),
                TokenResponseError::InvalidScopeClaim
            );
        }
        assert_eq!(
            parse(&body, &[]).err(),
            Some(TokenResponseError::InvalidRequestedScopes)
        );
        assert_eq!(
            parse(&body, &[ApprovedScope::AppData, ApprovedScope::AppData]).err(),
            Some(TokenResponseError::InvalidRequestedScopes)
        );
    }

    #[test]
    fn bearer_is_case_insensitive_but_other_types_and_whitespace_fail() {
        let body = fixture(ApprovedScope::File.as_str(), false);
        for supported in ["Bearer", "bearer", "bEaReR"] {
            assert!(parse(&body.replace("Bearer", supported), &[ApprovedScope::File]).is_ok());
        }
        for unsupported in ["", " Bearer", "Bearer ", "DPoP", "MAC"] {
            assert_eq!(
                rejected(&body.replace("Bearer", unsupported)),
                TokenResponseError::UnsupportedTokenType
            );
        }
    }

    #[test]
    fn missing_critical_fields_and_wrong_types_never_form_credentials() {
        let base = fixture(ApprovedScope::AppData.as_str(), false);
        for (name, value, field) in [
            ("access_token", quoted(ACCESS), TokenField::AccessToken),
            ("token_type", quoted("Bearer"), TokenField::TokenType),
            ("expires_in", "3600".to_owned(), TokenField::ExpiresIn),
            (
                "scope",
                quoted(ApprovedScope::AppData.as_str()),
                TokenField::Scope,
            ),
        ] {
            let pair = format!("\"{name}\":{value}");
            let missing = base
                .replace(&format!("{pair},"), "")
                .replace(&format!(",{pair}"), "");
            assert_eq!(rejected(&missing), TokenResponseError::MissingField(field));
            for wrong in ["null", "false", "[]", "{}"] {
                assert_eq!(
                    rejected(&base.replace(&pair, &format!("\"{name}\":{wrong}"))),
                    TokenResponseError::MalformedResponse
                );
            }
        }
    }

    #[test]
    fn decoded_critical_key_duplicates_and_escaped_aliases_are_rejected() {
        let base = fixture(ApprovedScope::AppData.as_str(), true);
        for extra in [
            "\"access_token\":\"other\"",
            "\"access_\\u0074oken\":\"other\"",
            "\"refresh_\\u0074oken\":\"other\"",
            "\"token_\\u0074ype\":\"Bearer\"",
            "\"expires_\\u0069n\":3600",
            "\"scop\\u0065\":\"other\"",
        ] {
            assert_eq!(
                rejected(&with_extra(&base, extra)),
                TokenResponseError::MalformedResponse
            );
        }
        // An escaped key by itself is decoded normally, not considered unknown.
        assert!(
            parse(
                &base.replace("access_token", "access_\\u0074oken"),
                &[ApprovedScope::AppData]
            )
            .is_ok()
        );
        let duplicated_error = br#"{"error":"invalid_grant","err\u006fr":"invalid_client"}"#;
        let now = Instant::now();
        assert!(
            parse_google_token_response(400, duplicated_error, &[ApprovedScope::File], now, now)
                .is_err()
        );
    }

    #[test]
    fn expiry_requires_positive_integer_and_checked_monotonic_boundary() {
        let base = fixture(ApprovedScope::File.as_str(), false);
        for bad in [
            "0",
            "-0",
            "-1",
            "1.0",
            "1e0",
            "1.5",
            "1e999",
            "18446744073709551616",
            "\"3600\"",
            "NaN",
            "Infinity",
        ] {
            assert!(parse(&base.replace("3600", bad), &[ApprovedScope::File]).is_err());
        }
        let start = Instant::now();
        let one = base.replace("3600", "1");
        assert!(
            parse_google_token_response(
                200,
                one.as_bytes(),
                &[ApprovedScope::File],
                start,
                start + Duration::from_millis(999)
            )
            .is_ok()
        );
        assert_eq!(
            parse_google_token_response(
                200,
                one.as_bytes(),
                &[ApprovedScope::File],
                start,
                start + Duration::from_secs(1)
            )
            .err(),
            Some(TokenResponseError::ExpiredAtReceipt)
        );
        assert_eq!(
            parse_google_token_response(
                200,
                one.as_bytes(),
                &[ApprovedScope::File],
                start + Duration::from_secs(1),
                start
            )
            .err(),
            Some(TokenResponseError::ClockRegression)
        );
        let huge = base.replace("3600", &u64::MAX.to_string());
        let result =
            parse_google_token_response(200, huge.as_bytes(), &[ApprovedScope::File], start, start);
        match start.checked_add(Duration::from_secs(u64::MAX)) {
            None => assert_eq!(result.err(), Some(TokenResponseError::ExpiryOverflow)),
            Some(expected) => assert_eq!(result.unwrap().expires_at(), expected),
        }
    }

    #[test]
    fn token_bytes_are_opaque_bounded_and_control_free() {
        let base = fixture(ApprovedScope::AppData.as_str(), true);
        for token in [
            "",
            "has space",
            "has\tcontrol",
            "has\ncontrol",
            "nul\0byte",
            "nonascii界",
        ] {
            assert_eq!(
                rejected(&base.replace(&quoted(ACCESS), &quoted(token))),
                TokenResponseError::InvalidAccessToken
            );
            assert_eq!(
                rejected(&base.replace(&quoted(REFRESH), &quoted(token))),
                TokenResponseError::InvalidRefreshToken
            );
        }
        let limit = "x".repeat(MAX_TOKEN_BYTES);
        let valid = base
            .replace(&quoted(ACCESS), &quoted(&limit))
            .replace(&quoted(REFRESH), &quoted(&limit));
        assert_eq!(
            parse(&valid, &[ApprovedScope::AppData])
                .unwrap()
                .access_token_for_native_adapter()
                .len(),
            MAX_TOKEN_BYTES
        );
        assert_eq!(
            rejected(&base.replace(&quoted(ACCESS), &quoted(&(limit.clone() + "x")))),
            TokenResponseError::InvalidAccessToken
        );
        assert_eq!(
            rejected(&base.replace(&quoted(REFRESH), &quoted(&(limit + "x")))),
            TokenResponseError::InvalidRefreshToken
        );
        let opaque = "not.a.jwt+/=~!@:$%&*()[]{}'\\\"";
        let parsed = parse(
            &base.replace(&quoted(ACCESS), &quoted(opaque)),
            &[ApprovedScope::AppData],
        )
        .unwrap();
        assert_eq!(parsed.access_token_for_native_adapter(), opaque);
    }

    #[test]
    fn ignored_extensions_cannot_override_tokens_scopes_or_identity() {
        let base = fixture(ApprovedScope::File.as_str(), false);
        let extended = with_extra(
            &base,
            r#""id_token":"synthetic-unverified", "extension":{"scope":"broad","access_token":"foreign","nested":[null,true,2.5]}, "account":"foreign", "granted_scopes":["broad"]"#,
        );
        let parsed = parse(&extended, &[ApprovedScope::AppData, ApprovedScope::File]).unwrap();
        assert_eq!(parsed.access_token_for_native_adapter(), ACCESS);
        assert_eq!(parsed.granted_scopes(), &[ApprovedScope::File]);
        assert_eq!(parsed.refresh_token_for_native_adapter(), None);
        let only_nested = r#"{"extension":{"access_token":"synthetic","scope":"broad","expires_in":3600,"token_type":"Bearer"}}"#;
        assert_eq!(
            rejected(only_nested),
            TokenResponseError::MissingField(TokenField::AccessToken)
        );
    }

    #[test]
    fn json_depth_body_work_and_root_field_limits_are_enforced() {
        let base = fixture(ApprovedScope::File.as_str(), false);
        let nested = |levels: usize| {
            with_extra(
                &base,
                &format!(
                    "\"extension\":{}0{}",
                    "[".repeat(levels),
                    "]".repeat(levels)
                ),
            )
        };
        assert!(parse(&nested(MAX_JSON_DEPTH - 1), &[ApprovedScope::File]).is_ok());
        assert_eq!(
            rejected(&nested(MAX_JSON_DEPTH)),
            TokenResponseError::DepthLimit
        );
        let large = with_extra(
            &base,
            &format!("\"extension\":{}", quoted(&"x".repeat(MAX_RESPONSE_BYTES))),
        );
        assert_eq!(rejected(&large), TokenResponseError::BodyLimit);
        let wide = with_extra(
            &base,
            &format!(
                "\"extension\":[{}]",
                vec!["0"; MAX_STRUCTURAL_TOKENS].join(",")
            ),
        );
        assert_eq!(rejected(&wide), TokenResponseError::WorkLimit);
        let fields = (0..MAX_ROOT_FIELDS)
            .map(|i| format!("\"extension{i}\":0"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            rejected(&with_extra(&base, &fields)),
            TokenResponseError::MalformedResponse
        );
        assert_eq!(
            rejected(&with_extra(
                &base,
                &format!("{}:0", quoted(&"x".repeat(MAX_KEY_BYTES + 1)))
            )),
            TokenResponseError::MalformedResponse
        );
        // String contents are not nesting/work: escaped quotes and punctuation.
        assert!(
            parse(
                &with_extra(
                    &base,
                    &format!("\"extension\":{}", quoted(&"{[,:]\\\"}".repeat(100)))
                ),
                &[ApprovedScope::File]
            )
            .is_ok()
        );
    }

    #[test]
    fn malformed_json_trailing_content_and_non_object_fail_without_dumps() {
        let base = fixture(ApprovedScope::File.as_str(), false);
        for body in [
            "".to_owned(),
            "[]".to_owned(),
            "null".to_owned(),
            "<html>private</html>".to_owned(),
            format!("{base}{{}}"),
            format!("{base} trailing"),
            with_extra(&base, "\"extension\":NaN"),
            with_extra(&base, "\"extension\":{]"),
            base.replace("3600", "+3600"),
        ] {
            assert!(parse(&body, &[ApprovedScope::File]).is_err());
        }
        let now = Instant::now();
        assert!(
            parse_google_token_response(200, &[0xff, 0], &[ApprovedScope::File], now, now).is_err()
        );
        assert!(parse(&(base + "\r\n \t"), &[ApprovedScope::File]).is_ok());
    }

    #[test]
    fn non_200_and_error_success_ambiguity_never_return_credentials() {
        let base = fixture(ApprovedScope::File.as_str(), false);
        let now = Instant::now();
        for status in [0, 99, 100, 201, 204, 302, 400, 401, 403, 429, 500, 599, 600] {
            assert!(
                parse_google_token_response(
                    status,
                    base.as_bytes(),
                    &[ApprovedScope::File],
                    now,
                    now
                )
                .is_err()
            );
        }
        for extra in [
            "\"error\":\"invalid_grant\"",
            "\"error_description\":\"private\"",
            "\"error_uri\":\"https://example.invalid/private\"",
        ] {
            let body = with_extra(&base, extra);
            for status in [200, 400] {
                assert_eq!(
                    parse_google_token_response(
                        status,
                        body.as_bytes(),
                        &[ApprovedScope::File],
                        now,
                        now
                    )
                    .err(),
                    Some(TokenResponseError::AmbiguousResponse)
                );
            }
        }
        assert_eq!(
            rejected(r#"{"error":"invalid_grant"}"#),
            TokenResponseError::UnexpectedErrorOnSuccess
        );
    }

    #[test]
    fn provider_http_categories_and_error_redaction_are_bounded() {
        let now = Instant::now();
        let body = br#"{"error":"invalid_grant","error_description":"synthetic-private-secret","error_uri":"https://example.invalid/secret","extension":{"private":"synthetic-token"}}"#;
        let error = parse_google_token_response(400, body, &[ApprovedScope::File], now, now)
            .err()
            .unwrap();
        assert_eq!(
            error,
            TokenResponseError::Provider {
                http: HttpFailure::BadRequest,
                error: ProviderFailure::InvalidGrant
            }
        );
        for text in [format!("{error}"), format!("{error:?}")] {
            for secret in [
                "synthetic-private-secret",
                "example.invalid",
                "synthetic-token",
            ] {
                assert!(!text.contains(secret));
            }
            assert!(text.len() < 160);
        }
        let unknown = br#"{"error":"synthetic-unknown-provider-code"}"#;
        assert_eq!(
            parse_google_token_response(401, unknown, &[ApprovedScope::File], now, now).err(),
            Some(TokenResponseError::Provider {
                http: HttpFailure::Unauthorized,
                error: ProviderFailure::Other
            })
        );
        assert_eq!(
            parse_google_token_response(
                503,
                b"<html>private</html>",
                &[ApprovedScope::File],
                now,
                now
            )
            .err(),
            Some(TokenResponseError::Http(HttpFailure::Server))
        );
        for malformed in [
            r#"{"error":null}"#,
            r#"{"error":""}"#,
            r#"{"error":42}"#,
            r#"{"error":"bad\ncode"}"#,
            r#"{"error":"invalid_grant","error_uri":{}}"#,
        ] {
            assert!(
                parse_google_token_response(
                    400,
                    malformed.as_bytes(),
                    &[ApprovedScope::File],
                    now,
                    now
                )
                .is_err()
            );
        }
    }
}
