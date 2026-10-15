//! Independent public parser probes with fixed synthetic claims; no I/O/provider.
use rapidraw_lib::drive_auth::pkce::ApprovedScope;
use rapidraw_lib::drive_auth::token_response::{
    HttpFailure, MAX_JSON_DEPTH, MAX_RESPONSE_BYTES, MAX_STRUCTURAL_TOKENS, MAX_TOKEN_BYTES,
    ParsedTokenResponse, ProviderFailure, TokenField, TokenResponseError,
    parse_google_token_response,
};
use std::time::{Duration, Instant};

const _: () = {
    assert!(MAX_RESPONSE_BYTES == 65_536);
    assert!(MAX_JSON_DEPTH == 16);
    assert!(MAX_STRUCTURAL_TOKENS == 4096);
    assert!(MAX_TOKEN_BYTES == 8192);
};
const APPDATA: &str = "https://www.googleapis.com/auth/drive.appdata";
const FILE: &str = "https://www.googleapis.com/auth/drive.file";
const ACCESS: &str = "SYNTHETIC_ACCESS.not-a-jwt+/=~!@:$%&*()[]{}'\\\"";
const REFRESH: &str = "SYNTHETIC_REFRESH.rotated+/=_-";
const BOTH: [ApprovedScope; 2] = [ApprovedScope::AppData, ApprovedScope::File];
fn q(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}
fn body(scope: &str) -> String {
    format!(
        "{{\"access_token\":{},\"token_type\":\"Bearer\",\"expires_in\":60,\"scope\":{}}}",
        q(ACCESS),
        q(scope)
    )
}
fn extra(base: &str, fields: &str) -> String {
    assert!(base.ends_with('}'));
    format!("{},{fields}}}", &base[..base.len() - 1])
}
fn parse(
    text: &str,
    requested: &[ApprovedScope],
) -> Result<ParsedTokenResponse, TokenResponseError> {
    let start = Instant::now();
    parse_google_token_response(200, text.as_bytes(), requested, start, start)
}
fn err(text: &str) -> TokenResponseError {
    parse(text, &BOTH)
        .err()
        .expect("synthetic claim must reject")
}
fn parsed(text: &str) -> ParsedTokenResponse {
    parse(text, &BOTH).unwrap()
}

#[test]
fn partial_explicit_capabilities_are_exact_and_extensions_cannot_union_or_identify() {
    for (scope, expected) in [
        (FILE, ApprovedScope::File),
        (APPDATA, ApprovedScope::AppData),
    ] {
        let text = extra(
            &body(scope),
            r#""id_token":{"sub":"SYNTHETIC_ACCOUNT"},"account":"SYNTHETIC_ACCOUNT","granted_scopes":["https://www.googleapis.com/auth/drive"],"extension":{"scope":"broad","access_token":"SYNTHETIC_FOREIGN"},"refresh_token_expires_in":1e999"#,
        );
        let result = parsed(&text);
        assert!(result.granted_scopes() == [expected]);
        assert!(result.access_token_for_native_adapter() == ACCESS);
        assert!(result.refresh_token_for_native_adapter().is_none());
    }
    let result = parsed(&body(&format!("{FILE} {APPDATA}")));
    assert!(result.granted_scopes() == [ApprovedScope::File, ApprovedScope::AppData]);
    let result = parsed(&extra(
        &body(FILE),
        r#""extension":{"x":1,"x":2},"token_metadata":[false,null,1e999]"#,
    ));
    assert!(result.granted_scopes() == [ApprovedScope::File]);
}

#[test]
fn scope_missing_unrequested_broad_duplicate_and_invalid_requests_fail_closed() {
    let missing = format!(
        "{{\"access_token\":{},\"token_type\":\"Bearer\",\"expires_in\":60}}",
        q(ACCESS)
    );
    assert_eq!(
        err(&missing),
        TokenResponseError::MissingField(TokenField::Scope)
    );
    assert_eq!(
        parse(&body(FILE), &[ApprovedScope::AppData]).err(),
        Some(TokenResponseError::UnrequestedScope)
    );
    for scope in [
        "",
        "https://www.googleapis.com/auth/drive",
        "openid",
        "https://www.googleapis.com/auth/drive.readonly",
        &format!("{FILE} {FILE}"),
        &format!(" {FILE}"),
        &format!("{FILE} "),
        &format!("{FILE}\t{APPDATA}"),
        &format!("{FILE}  {APPDATA}"),
    ] {
        assert_eq!(err(&body(scope)), TokenResponseError::InvalidScopeClaim);
    }
    for request in [
        vec![],
        vec![ApprovedScope::File, ApprovedScope::File],
        vec![
            ApprovedScope::File,
            ApprovedScope::AppData,
            ApprovedScope::File,
        ],
    ] {
        assert_eq!(
            parse(&body(FILE), &request).err(),
            Some(TokenResponseError::InvalidRequestedScopes)
        );
    }
}

#[test]
fn escaped_keys_decode_once_and_root_duplicates_never_override() {
    let base = extra(&body(FILE), &format!("\"refresh_token\":{}", q(REFRESH)));
    for field in [
        r#""access_\u0074oken":"SYNTHETIC_SHADOW""#,
        r#""refresh_\u0074oken":"SYNTHETIC_SHADOW""#,
        r#""token_\u0074ype":"Bearer""#,
        r#""expires_\u0069n":120"#,
        r#""scop\u0065":"https://www.googleapis.com/auth/drive.appdata""#,
    ] {
        assert_eq!(
            err(&extra(&base, field)),
            TokenResponseError::MalformedResponse
        );
    }
    assert_eq!(
        err(&extra(&body(FILE), r#""unknown":1,"unkn\u006fwn":2"#)),
        TokenResponseError::MalformedResponse
    );
    let escaped = base.replace("access_token", r"access_\u0074oken");
    let result = parsed(&escaped);
    assert!(result.access_token_for_native_adapter() == ACCESS);
    assert!(result.refresh_token_for_native_adapter() == Some(REFRESH));
    assert!(result.granted_scopes() == [ApprovedScope::File]);
}

#[test]
fn tokens_stay_opaque_and_refresh_absence_rotation_null_and_controls_differ() {
    let base = body(FILE);
    let absent = parsed(&base);
    assert!(absent.access_token_for_native_adapter() == ACCESS);
    assert!(absent.refresh_token_for_native_adapter().is_none());
    let rotated = parsed(&extra(&base, &format!("\"refresh_token\":{}", q(REFRESH))));
    assert!(rotated.refresh_token_for_native_adapter() == Some(REFRESH));
    assert_eq!(
        err(&extra(&base, r#""refresh_token":null"#)),
        TokenResponseError::MalformedResponse
    );
    for invalid in [
        "",
        "space value",
        "tab\tvalue",
        "nul\0value",
        "newline\nvalue",
        "DEL\u{7f}",
        "nonascii界",
    ] {
        let access = base.replace(&q(ACCESS), &q(invalid));
        assert_eq!(err(&access), TokenResponseError::InvalidAccessToken);
        assert_eq!(
            err(&extra(&base, &format!("\"refresh_token\":{}", q(invalid)))),
            TokenResponseError::InvalidRefreshToken
        );
    }
    let limit = "x".repeat(8192);
    let boundary = extra(
        &base.replace(&q(ACCESS), &q(&limit)),
        &format!("\"refresh_token\":{}", q(&limit)),
    );
    let result = parsed(&boundary);
    assert!(result.access_token_for_native_adapter() == limit);
    assert!(result.refresh_token_for_native_adapter() == Some(limit.as_str()));
    let over = format!("{limit}x");
    assert_eq!(
        err(&base.replace(&q(ACCESS), &q(&over))),
        TokenResponseError::InvalidAccessToken
    );
    assert_eq!(
        err(&extra(&base, &format!("\"refresh_token\":{}", q(&over)))),
        TokenResponseError::InvalidRefreshToken
    );
    for supported in ["Bearer", "bEaReR"] {
        assert!(parse(&base.replace("Bearer", supported), &BOTH).is_ok());
    }
    for unsupported in ["DPoP", "MAC", " Bearer", "Bearer "] {
        assert_eq!(
            err(&base.replace("Bearer", unsupported)),
            TokenResponseError::UnsupportedTokenType
        );
    }
}

#[test]
fn integer_seconds_anchor_to_request_start_with_exact_latency_and_use_boundaries() {
    let start = Instant::now();
    let now = start + Duration::from_secs(7);
    let text = body(FILE);
    let result = parse_google_token_response(200, text.as_bytes(), &BOTH, start, now).unwrap();
    let expiry = start + Duration::from_secs(60);
    assert!(result.expires_at() == expiry);
    assert!(result.is_usable_at(now));
    assert!(!result.is_usable_at(now - Duration::from_nanos(1)));
    assert!(result.is_usable_at(expiry - Duration::from_nanos(1)));
    assert!(!result.is_usable_at(expiry));
    assert_eq!(
        parse_google_token_response(200, text.as_bytes(), &BOTH, start, expiry).err(),
        Some(TokenResponseError::ExpiredAtReceipt)
    );
    assert_eq!(
        parse_google_token_response(
            200,
            text.as_bytes(),
            &BOTH,
            start,
            expiry + Duration::from_secs(1)
        )
        .err(),
        Some(TokenResponseError::ExpiredAtReceipt)
    );
    assert_eq!(
        parse_google_token_response(
            200,
            text.as_bytes(),
            &BOTH,
            start + Duration::from_nanos(1),
            start
        )
        .err(),
        Some(TokenResponseError::ClockRegression)
    );
    for invalid in [
        "0",
        "-0",
        "-1",
        "1.0",
        "1e0",
        "1.5",
        r#""60""#,
        "null",
        "true",
        "NaN",
        "Infinity",
        "18446744073709551616",
    ] {
        assert!(
            parse(
                &text.replace("\"expires_in\":60", &format!("\"expires_in\":{invalid}")),
                &BOTH
            )
            .is_err()
        );
    }
    // Exact integer arithmetic from std, not the candidate parser, determines
    // platform Instant representability. No arbitrary lifetime clamp is assumed.
    for seconds in [9_007_199_254_740_993u64, u64::MAX] {
        let text = text.replace("\"expires_in\":60", &format!("\"expires_in\":{seconds}"));
        let observed = parse_google_token_response(200, text.as_bytes(), &BOTH, start, start);
        match start.checked_add(Duration::from_secs(seconds)) {
            Some(expected) => assert!(observed.unwrap().expires_at() == expected),
            None => assert_eq!(observed.err(), Some(TokenResponseError::ExpiryOverflow)),
        }
    }
}

#[test]
fn whole_body_depth_and_structural_work_have_exact_success_and_failure_edges() {
    let base = body(FILE);
    let prefix = extra(&base, r#""padding":"""#);
    let padding = 65_536 - prefix.len();
    let exact = extra(&base, &format!("\"padding\":{}", q(&"x".repeat(padding))));
    assert!(exact.len() == 65_536);
    assert!(parse(&exact, &BOTH).is_ok());
    assert_eq!(err(&(exact + " ")), TokenResponseError::BodyLimit);
    for (arrays, expected) in [(15, None), (16, Some(TokenResponseError::DepthLimit))] {
        let text = extra(
            &base,
            &format!(
                "\"extension\":{}0{}",
                "[".repeat(arrays),
                "]".repeat(arrays)
            ),
        );
        assert_eq!(parse(&text, &BOTH).err(), expected);
    }
    // Four-field root contributes8 structural tokens. Adding extension:[...]
    // adds N+2: root comma/colon, array open and N-1 element commas => N+10.
    let wide = |n: usize| {
        extra(
            &base,
            &format!("\"extension\":[{}]", vec!["0"; n].join(",")),
        )
    };
    assert!(parse(&wide(4086), &BOTH).is_ok());
    assert_eq!(err(&wide(4087)), TokenResponseError::WorkLimit);
    let quoted = extra(
        &base,
        &format!("\"extension\":{}", q(&"{[,:]\\\"}".repeat(1000))),
    );
    assert!(parse(&quoted, &BOTH).is_ok());
}

#[test]
fn root_key_and_field_limits_are_decoded_and_nested_values_never_promote() {
    let base = body(FILE);
    let fields = |n: usize| {
        (0..n)
            .map(|i| format!("\"extension{i}\":0"))
            .collect::<Vec<_>>()
            .join(",")
    };
    assert!(parse(&extra(&base, &fields(28)), &BOTH).is_ok()); //4+28=32 root fields
    assert_eq!(
        err(&extra(&base, &fields(29))),
        TokenResponseError::MalformedResponse
    );
    assert!(parse(&extra(&base, &format!("{}:0", q(&"k".repeat(128)))), &BOTH).is_ok());
    assert_eq!(
        err(&extra(&base, &format!("{}:0", q(&"k".repeat(129))))),
        TokenResponseError::MalformedResponse
    );
    //128 decoded bytes remain valid even when JSON escapes use768 wire bytes.
    let escaped = format!("\"{}\":0", r"\u006b".repeat(128));
    assert!(parse(&extra(&base, &escaped), &BOTH).is_ok());
    let only_nested = format!(
        "{{\"extension\":{base},\"id_token\":\"SYNTHETIC_ID\",\"account\":\"SYNTHETIC_ACCOUNT\"}}"
    );
    assert_eq!(
        err(&only_nested),
        TokenResponseError::MissingField(TokenField::AccessToken)
    );
}

#[test]
fn missing_wrong_types_invalid_utf8_and_trailing_junk_never_form_tokens() {
    let base = body(FILE);
    for (key, value, field) in [
        ("access_token", q(ACCESS), TokenField::AccessToken),
        ("token_type", q("Bearer"), TokenField::TokenType),
        ("expires_in", "60".to_owned(), TokenField::ExpiresIn),
        ("scope", q(FILE), TokenField::Scope),
    ] {
        let pair = format!("\"{key}\":{value}");
        let missing = base
            .replace(&format!("{pair},"), "")
            .replace(&format!(",{pair}"), "");
        assert_eq!(err(&missing), TokenResponseError::MissingField(field));
        let wrong = base.replace(&pair, &format!("\"{key}\":null"));
        assert_eq!(err(&wrong), TokenResponseError::MalformedResponse);
    }
    for invalid in [
        "".to_owned(),
        "[]".to_owned(),
        "null".to_owned(),
        format!("{base}{{}}"),
        format!("{base} trailing"),
        extra(&base, r#""unknown":NaN"#),
        extra(&base, r#""unknown":{]"#),
        extra(&base, r#""unknown":"unterminated"#),
    ] {
        assert!(parse(&invalid, &BOTH).is_err());
    }
    let start = Instant::now();
    assert_eq!(
        parse_google_token_response(200, &[0xff, 0xfe], &BOTH, start, start).err(),
        Some(TokenResponseError::MalformedResponse)
    );
    assert!(parse(&(base + "\r\n\t "), &BOTH).is_ok());
}

#[test]
fn non200_categories_and_success_error_ambiguity_do_not_yield_tokens() {
    let start = Instant::now();
    let text = body(FILE);
    for (status, expected) in [
        (100, HttpFailure::UnexpectedStatus),
        (201, HttpFailure::UnexpectedStatus),
        (204, HttpFailure::UnexpectedStatus),
        (302, HttpFailure::Redirect),
        (400, HttpFailure::BadRequest),
        (401, HttpFailure::Unauthorized),
        (403, HttpFailure::Forbidden),
        (418, HttpFailure::Client),
        (429, HttpFailure::RateLimited),
        (503, HttpFailure::Server),
    ] {
        assert_eq!(
            parse_google_token_response(status, text.as_bytes(), &BOTH, start, start).err(),
            Some(TokenResponseError::Http(expected))
        );
    }
    for status in [0, 99, 600, u16::MAX] {
        assert_eq!(
            parse_google_token_response(status, text.as_bytes(), &BOTH, start, start).err(),
            Some(TokenResponseError::InvalidStatus)
        );
    }
    for field in [
        r#""error":"invalid_grant""#,
        r#""error_description":"SYNTHETIC_PRIVATE""#,
        r#""error_uri":"https://synthetic.invalid/SYNTHETIC_PRIVATE""#,
    ] {
        let ambiguous = extra(&text, field);
        for status in [200, 400] {
            assert_eq!(
                parse_google_token_response(status, ambiguous.as_bytes(), &BOTH, start, start)
                    .err(),
                Some(TokenResponseError::AmbiguousResponse)
            );
        }
    }
    assert_eq!(
        err(r#"{"error":"invalid_grant"}"#),
        TokenResponseError::UnexpectedErrorOnSuccess
    );
    assert_eq!(
        parse_google_token_response(503, b"<html>SYNTHETIC_PRIVATE</html>", &BOTH, start, start)
            .err(),
        Some(TokenResponseError::Http(HttpFailure::Server))
    );
    //A preflight resource failure may precede the non200 category.
    assert_eq!(
        parse_google_token_response(503, &vec![b'x'; 65_537], &BOTH, start, start).err(),
        Some(TokenResponseError::BodyLimit)
    );
}

#[test]
fn typed_diagnostics_never_reflect_tokens_bodies_keys_id_account_or_uri() {
    let start = Instant::now();
    let provider=br#"{"error":"invalid_grant","error_description":"SYNTHETIC_PRIVATE_DETAIL","error_uri":"https://synthetic.invalid/SYNTHETIC_URI","id_token":"SYNTHETIC_ID","account":"SYNTHETIC_ACCOUNT","extension":{"access_token":"SYNTHETIC_ACCESS","refresh_token":"SYNTHETIC_REFRESH"}}"#;
    let known = parse_google_token_response(400, provider, &BOTH, start, start)
        .err()
        .unwrap();
    assert_eq!(
        known,
        TokenResponseError::Provider {
            http: HttpFailure::BadRequest,
            error: ProviderFailure::InvalidGrant
        }
    );
    let unknown = br#"{"error":"SYNTHETIC_UNKNOWN_ERROR"}"#;
    let other = parse_google_token_response(401, unknown, &BOTH, start, start)
        .err()
        .unwrap();
    assert_eq!(
        other,
        TokenResponseError::Provider {
            http: HttpFailure::Unauthorized,
            error: ProviderFailure::Other
        }
    );
    let duplicate = err(&extra(&body(FILE), r#""access_token":"SYNTHETIC_SHADOW""#));
    let bad_access = err(&body(FILE).replace(&q(ACCESS), &q("SYNTHETIC_ACCESS bad")));
    let malformed = err(r#"{"SYNTHETIC_PRIVATE_KEY":[}"#);
    for error in [known, other, duplicate, bad_access, malformed] {
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(rendered.len() < 200);
            for marker in [
                "SYNTHETIC_",
                "synthetic.invalid",
                "http://",
                "https://",
                ACCESS,
                REFRESH,
            ] {
                assert!(!rendered.contains(marker));
            }
        }
    }
}
