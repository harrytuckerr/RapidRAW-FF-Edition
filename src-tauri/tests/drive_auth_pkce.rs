//! Independent public-API PKCE regressions; no provider/network I/O.
//! Ports below are synthetic caller assertions, not bound/listening sockets.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rapidraw_lib::drive_auth::pkce::{
    ApprovedScope, AuthorizationCodeGrant, CallbackOutcome, PkceError, PkceTransaction,
    TransactionStatus,
};
use reqwest::Url;
use sha2::{Digest, Sha256};

const CLIENT: &str = "independent-synthetic.apps.googleusercontent.com";
const APPDATA: &str = "https://www.googleapis.com/auth/drive.appdata";
const FILE: &str = "https://www.googleapis.com/auth/drive.file";
const PORT: u16 = 49311;

fn begin(now: Instant, scopes: &[&str]) -> PkceTransaction {
    PkceTransaction::begin(CLIENT, PORT, scopes, now, Duration::from_secs(60))
        .unwrap_or_else(|error| panic!("production SysRng begin failed: {error:?}"))
}

fn wire(tx: &mut PkceTransaction, now: Instant) -> (Url, BTreeMap<String, String>) {
    let url = Url::parse(tx.authorization_url_for_browser(now).unwrap()).unwrap();
    let pairs: Vec<_> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert!(
        pairs.len() == 7,
        "authorization query must have seven fields"
    );
    let query: BTreeMap<_, _> = pairs.into_iter().collect();
    assert!(query.len() == 7, "authorization query has duplicate keys");
    (url, query)
}

fn callback(tx: &PkceTransaction, state: &str, tail: &str) -> String {
    format!("{}?state={state}&{tail}", tx.redirect_uri())
}

fn grant(result: Result<CallbackOutcome, PkceError>) -> AuthorizationCodeGrant {
    match result {
        Ok(CallbackOutcome::Code(value)) => value,
        Err(error) => panic!("expected matching code, got typed error {error:?}"),
        _ => panic!("expected matching code, got non-code outcome"),
    }
}

#[test]
fn production_sysrng_two_fresh_pairs_exact_wire_and_s256_grant_binding() {
    let now = Instant::now();
    let mut first = begin(now, &[APPDATA, FILE]);
    let mut second = begin(now, &[APPDATA, FILE]);
    let (url, first_wire) = wire(&mut first, now);
    let (_, second_wire) = wire(&mut second, now);
    assert!(url.origin().ascii_serialization() == "https://accounts.google.com");
    assert!(url.path() == "/o/oauth2/v2/auth");
    let expected_keys = [
        "client_id",
        "code_challenge",
        "code_challenge_method",
        "redirect_uri",
        "response_type",
        "scope",
        "state",
    ];
    assert!(first_wire.keys().map(String::as_str).eq(expected_keys));
    assert!(first_wire["client_id"] == CLIENT);
    assert!(first_wire["redirect_uri"] == "http://127.0.0.1:49311/oauth2/callback/google");
    assert!(first_wire["response_type"] == "code");
    assert!(first_wire["scope"] == format!("{APPDATA} {FILE}"));
    assert!(first_wire["code_challenge_method"] == "S256");
    assert!(!first_wire.contains_key("include_granted_scopes"));
    assert!(
        first_wire["state"] != second_wire["state"],
        "fresh states repeated"
    );
    assert!(
        first_wire["code_challenge"] != second_wire["code_challenge"],
        "fresh challenges repeated"
    );
    for query in [&first_wire, &second_wire] {
        for key in ["state", "code_challenge"] {
            assert!(query[key].len() == 43);
            assert!(
                query[key]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            );
            assert!(URL_SAFE_NO_PAD.decode(&query[key]).unwrap().len() == 32);
        }
    }
    let response = callback(&first, &first_wire["state"], "code=synthetic-first");
    let received = grant(first.accept_callback(&response, now));
    // Independent reference formula applied to the public returned verifier;
    // candidate challenge generation is not called to compute the expectation.
    let expected =
        URL_SAFE_NO_PAD.encode(Sha256::digest(received.verifier_for_exchange().as_bytes()));
    assert!(
        first_wire["code_challenge"] == expected,
        "S256 grant binding mismatch"
    );
    assert!(received.verifier_for_exchange().len() == 43);
    assert!(
        URL_SAFE_NO_PAD
            .decode(received.verifier_for_exchange())
            .unwrap()
            .len()
            == 32
    );
    assert!(received.code_for_exchange() == "synthetic-first");
    assert!(received.client_id() == CLIENT);
    assert!(received.requested_scopes() == [ApprovedScope::AppData, ApprovedScope::File]);
    let response = callback(&second, &second_wire["state"], "code=synthetic-second");
    let other = grant(second.accept_callback(&response, now));
    assert!(received.verifier_for_exchange() != other.verifier_for_exchange());
}

#[test]
fn production_begin_preserves_each_exact_selected_scope_set_without_union() {
    let now = Instant::now();
    for selected in [vec![APPDATA], vec![FILE], vec![FILE, APPDATA]] {
        let mut tx = begin(now, &selected);
        let (_, query) = wire(&mut tx, now);
        assert!(query["scope"] == selected.join(" "));
        assert!(!query.contains_key("include_granted_scopes"));
        assert!(!query.contains_key("openid"));
    }
    assert!(matches!(
        PkceTransaction::begin(
            CLIENT,
            PORT,
            &["https://www.googleapis.com/auth/drive"],
            now,
            Duration::from_secs(60)
        ),
        Err(PkceError::InvalidScopes)
    ));
}

#[test]
fn false_state_or_binding_stays_pending_then_matching_code_and_replay_are_exact() {
    let now = Instant::now();
    let mut tx = begin(now, &[FILE]);
    let (_, query) = wire(&mut tx, now);
    let state = &query["state"];
    let wrong_state = format!(
        "{}{}",
        if state.starts_with('A') { "B" } else { "A" },
        &state[1..]
    );
    let wrong = callback(&tx, &wrong_state, "code=synthetic");
    assert!(matches!(
        tx.accept_callback(&wrong, now),
        Err(PkceError::StateMismatch)
    ));
    assert!(tx.status() == TransactionStatus::Pending);
    for binding in [
        "http://127.0.0.1:49312/oauth2/callback/google",
        "http://localhost:49311/oauth2/callback/google",
        "http://127.0.0.1:49311/other",
    ] {
        let wrong = format!("{binding}?state={state}&code=synthetic");
        assert!(matches!(
            tx.accept_callback(&wrong, now),
            Err(PkceError::CallbackBindingMismatch)
        ));
        assert!(tx.status() == TransactionStatus::Pending);
    }
    let valid = callback(&tx, state, "code=4%2Fsynthetic%2Bcode");
    let received = grant(tx.accept_callback(&valid, now));
    assert!(received.code_for_exchange() == "4/synthetic+code");
    assert!(received.redirect_uri() == "http://127.0.0.1:49311/oauth2/callback/google");
    assert!(received.requested_scopes() == [ApprovedScope::File]);
    assert!(tx.status() == TransactionStatus::CodeReceived);
    assert!(matches!(
        tx.accept_callback(&valid, now),
        Err(PkceError::Finished)
    ));
    assert!(matches!(
        tx.authorization_url_for_browser(now),
        Err(PkceError::Finished)
    ));
}

#[test]
fn invalid_callbacks_do_not_extend_exact_deadline_and_timeout_is_terminal() {
    let now = Instant::now();
    let mut tx = begin(now, &[APPDATA]);
    let (_, query) = wire(&mut tx, now);
    let invalid = callback(&tx, "definitely-not-the-state", "code=synthetic");
    for seconds in [1, 59] {
        assert!(matches!(
            tx.accept_callback(&invalid, now + Duration::from_secs(seconds)),
            Err(PkceError::StateMismatch)
        ));
    }
    let valid = callback(&tx, &query["state"], "code=synthetic");
    assert!(matches!(
        tx.accept_callback(&valid, now + Duration::from_secs(60)),
        Err(PkceError::Expired)
    ));
    assert!(tx.status() == TransactionStatus::Expired);
    assert!(matches!(
        tx.accept_callback(&valid, now + Duration::from_secs(61)),
        Err(PkceError::Finished)
    ));
}

#[test]
fn typed_errors_never_echo_inputs_and_provider_denial_or_error_is_single_use() {
    let now = Instant::now();
    let mut tx = begin(now, &[FILE]);
    let (_, query) = wire(&mut tx, now);
    let bad = callback(
        &tx,
        &query["state"],
        "code=synthetic-secret-marker&code=shadow",
    );
    let error = match tx.accept_callback(&bad, now) {
        Err(error) => error,
        _ => panic!("duplicate response unexpectedly accepted"),
    };
    assert!(error == PkceError::DuplicateParameter);
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("synthetic-secret-marker"));
    assert!(!rendered.contains(&query["state"]));
    assert!(!rendered.contains("http"));
    assert!(rendered.len() < 200);
    let good = callback(&tx, &query["state"], "code=synthetic-redaction-check");
    let received = grant(tx.accept_callback(&good, now));
    assert!(!rendered.contains(received.verifier_for_exchange()));
    for (provider_error, expected_status) in [
        ("access_denied", TransactionStatus::Denied),
        ("server_error", TransactionStatus::ProviderRejected),
    ] {
        let mut tx = begin(now, &[APPDATA]);
        let (_, query) = wire(&mut tx, now);
        let tail = format!(
            "error={provider_error}&error_description=SYNTHETIC_PRIVATE_MESSAGE&error_uri=https%3A%2F%2Fsynthetic.invalid%2Ffake-grant"
        );
        let response = callback(&tx, &query["state"], &tail);
        let outcome = tx.accept_callback(&response, now).unwrap();
        assert!(matches!(
            (&outcome, expected_status),
            (CallbackOutcome::Denied, TransactionStatus::Denied)
                | (
                    CallbackOutcome::ProviderRejected,
                    TransactionStatus::ProviderRejected
                )
        ));
        assert!(tx.status() == expected_status);
        assert!(matches!(
            tx.accept_callback(&response, now),
            Err(PkceError::Finished)
        ));
    }
}

#[test]
fn production_cancel_and_regressing_clock_are_terminal() {
    let now = Instant::now();
    let mut cancelled = begin(now, &[APPDATA]);
    cancelled.cancel(now).unwrap();
    assert!(cancelled.status() == TransactionStatus::Cancelled);
    assert!(matches!(cancelled.cancel(now), Err(PkceError::Finished)));
    let mut clock = begin(now, &[FILE]);
    clock.check_active(now + Duration::from_secs(2)).unwrap();
    assert!(matches!(
        clock.check_active(now + Duration::from_secs(1)),
        Err(PkceError::ClockRegression)
    ));
    assert!(clock.status() == TransactionStatus::ClockFailed);
    assert!(matches!(
        clock.check_active(now + Duration::from_secs(3)),
        Err(PkceError::Finished)
    ));
}

#[test]
fn rfc6749_valid_matching_code_must_ignore_unknown_provider_extension() {
    let now = Instant::now();
    let mut tx = begin(now, &[APPDATA]);
    let (_, query) = wire(&mut tx, now);
    let response = callback(
        &tx,
        &query["state"],
        "code=synthetic-extension-code&provider_extension=synthetic",
    );
    match tx.accept_callback(&response, now) {
        Ok(CallbackOutcome::Code(received)) => {
            assert!(received.code_for_exchange() == "synthetic-extension-code");
            assert!(tx.status() == TransactionStatus::CodeReceived);
        }
        Err(error) => panic!(
            "RFC6749 section4.1.2 requires ignoring an unrecognized response parameter: expected Code, observed {error:?}, status {:?}",
            tx.status()
        ),
        _ => panic!("expected matching Code outcome, observed provider error outcome"),
    }
}
