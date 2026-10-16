//! One owned desktop system-browser handoff, not browser load or authorization.
//!
//! Only a generated native ticket enters. The selected launcher child and ticket
//! stay together on one blocking worker until actual reaping, including unwind.
//! Caller Drop requests cancellation; it cannot abort a started OS operation.
//! Unknown cleanup keeps the bounded controller fence even indefinitely. No URL,
//! command, native error or process output is logged/returned. OS argv/environment
//! necessarily receives the URL. Ordinary allocation drop is not zeroization.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;

use super::pkce::{ApprovedScope, CALLBACK_PATH, GOOGLE_AUTHORIZATION_ENDPOINT};
use super::session::{BrowserHandoff, BrowserLaunchTicket, TicketError};

const MAX_URL_BYTES: usize = 8192;
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const PENDING: u8 = 0;
const CANCELLED: u8 = 1;
const COMPLETING: u8 = 2;
const KEYS: [&str; 7] = [
    "client_id",
    "redirect_uri",
    "response_type",
    "scope",
    "state",
    "code_challenge",
    "code_challenge_method",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// The selected OS launcher exited successfully; not browser/login success.
    HandedOff,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchError {
    InvalidDestination,
    LaunchFailed,
    Cancelled,
    WorkerFailure,
    Ticket(TicketError),
}

/// Native async boundary. An unpolled future owns its ticket and opens nothing.
/// Dropping a polled future cannot release a started worker's ticket/child fence.
pub async fn hand_off(ticket: BrowserLaunchTicket) -> Result<LaunchOutcome, LaunchError> {
    hand_off_with(ticket, LaunchPlan::Default).await
}

struct CancelOnDrop(Arc<AtomicU8>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let _ = self
            .0
            .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }
}
async fn hand_off_with(
    ticket: BrowserLaunchTicket,
    plan: LaunchPlan,
) -> Result<LaunchOutcome, LaunchError> {
    let decision = Arc::new(AtomicU8::new(PENDING));
    let _cancel_on_drop = CancelOnDrop(Arc::clone(&decision));
    // The closure already owns the ticket if spawn panics or never starts it.
    // Dropping the async JoinHandle does not destroy a started closure's guard.
    let worker =
        tauri::async_runtime::spawn_blocking(move || launch_worker(ticket, decision, plan));
    worker.await.map_err(|_| LaunchError::WorkerFailure)?
}

enum LaunchPlan {
    Default,
    #[cfg(test)]
    Synthetic {
        command: Box<Command>,
        control: Arc<TestControl>,
    },
}
impl LaunchPlan {
    fn command(self, original: &str) -> Result<Command, LaunchError> {
        match self {
            Self::Default => first_default_command(original),
            #[cfg(test)]
            Self::Synthetic { command, control } => {
                *control.original.lock().expect("synthetic URL capture") =
                    Some(original.to_owned());
                Ok(*command)
            }
        }
    }
}
fn first_default_command(original: &str) -> Result<Command, LaunchError> {
    // Builder only; no that/with/detached/background opening API. Dropping the
    // remaining recipes cannot execute them. No retry/fallback after any error.
    open::commands(original)
        .into_iter()
        .next()
        .ok_or(LaunchError::LaunchFailed)
}

struct LaunchGuard {
    // Drop explicitly reaps this before destroying/releasing the ticket.
    child: Option<Child>,
    ticket: Option<BrowserLaunchTicket>,
    #[cfg(test)]
    control: Option<Arc<TestControl>>,
}
struct Reaped {
    success: bool,
    wait_failed: bool,
}
impl LaunchGuard {
    fn current_url(&self) -> Result<&str, LaunchError> {
        self.ticket
            .as_ref()
            .ok_or(LaunchError::WorkerFailure)?
            .url_for_native_launcher()
            .map_err(LaunchError::Ticket)
    }

    fn reap(&mut self) -> Reaped {
        let mut wait_failed = false;
        loop {
            let Some(child) = self.child.as_mut() else {
                return Reaped {
                    success: false,
                    wait_failed,
                };
            };
            #[cfg(test)]
            let inject_error = self
                .control
                .as_ref()
                .is_some_and(|c| c.wait_error.load(Ordering::Acquire));
            #[cfg(not(test))]
            let inject_error = false;
            let result = if inject_error {
                Err(std::io::Error::from(std::io::ErrorKind::Other))
            } else {
                child.try_wait()
            };
            match result {
                Ok(Some(status)) => {
                    let success = status.success();
                    // Child handle/resources are destroyed before any ticket ack.
                    drop(self.child.take());
                    return Reaped {
                        success,
                        wait_failed,
                    };
                }
                Ok(None) => {}
                Err(_) => {
                    // A wait failure is not proof of exit/reaping. Keep ownership;
                    // only an eventual successful observation can release it.
                    wait_failed = true;
                    #[cfg(test)]
                    if let Some(control) = &self.control {
                        control.wait_error_seen.store(true, Ordering::Release);
                    }
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn finish(&mut self, outcome: BrowserHandoff) -> Result<(), LaunchError> {
        if self.child.is_some() {
            let _ = self.reap();
        }
        self.ticket
            .take()
            .ok_or(LaunchError::WorkerFailure)?
            .finish(outcome)
            .map_err(LaunchError::Ticket)
    }
}
impl Drop for LaunchGuard {
    fn drop(&mut self) {
        // Runs on the owning blocking worker, including during unwind. If OS
        // cleanup remains unknown, stay here with the ticket rather than detach.
        if self.child.is_some() {
            let _ = self.reap();
        }
        drop(self.ticket.take());
    }
}

fn launch_worker(
    ticket: BrowserLaunchTicket,
    decision: Arc<AtomicU8>,
    plan: LaunchPlan,
) -> Result<LaunchOutcome, LaunchError> {
    #[cfg(test)]
    let control = match &plan {
        LaunchPlan::Default => None,
        LaunchPlan::Synthetic { control, .. } => Some(Arc::clone(control)),
    };
    // Install before constructing/spawning anything. After spawn returns, place
    // Child in this guard immediately, with no user callback/panic seam between.
    let mut guard = LaunchGuard {
        child: None,
        ticket: Some(ticket),
        #[cfg(test)]
        control,
    };
    let result = launch_and_reap(&mut guard, &decision, plan);
    match result {
        Ok(()) => guard
            .finish(BrowserHandoff::HandedOff)
            .map(|()| LaunchOutcome::HandedOff),
        Err(error) => {
            // Preserve the static primary error; stale failure acknowledgment
            // cannot overwrite a newer controller terminal or release early.
            let _ = guard.finish(BrowserHandoff::Failed);
            Err(error)
        }
    }
}

fn launch_and_reap(
    guard: &mut LaunchGuard,
    decision: &AtomicU8,
    plan: LaunchPlan,
) -> Result<(), LaunchError> {
    #[cfg(test)]
    if let Some(control) = &guard.control {
        control.entered.store(true, Ordering::Release);
        while !control.dispatch.load(Ordering::Acquire) {
            if decision.load(Ordering::Acquire) == CANCELLED {
                return Err(LaunchError::Cancelled);
            }
            guard.current_url()?;
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    if decision.load(Ordering::Acquire) == CANCELLED {
        return Err(LaunchError::Cancelled);
    }
    let original = guard.current_url()?;
    validate_destination(original)?;
    let mut command = plan.command(original)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Building platform recipes may take time. Never dispatch on an old check.
    guard.current_url()?;
    if decision.load(Ordering::Acquire) == CANCELLED {
        return Err(LaunchError::Cancelled);
    }
    guard.child = Some(command.spawn().map_err(|_| LaunchError::LaunchFailed)?);
    drop(command);
    #[cfg(test)]
    if let Some(control) = &guard.control {
        control.started.store(true, Ordering::Release);
        if control.panic_after_spawn.load(Ordering::Acquire) {
            panic!("synthetic browser worker unwind");
        }
    }
    let reaped = guard.reap();
    if decision.load(Ordering::Acquire) == CANCELLED {
        return Err(LaunchError::Cancelled);
    }
    if !reaped.success || reaped.wait_failed {
        return Err(LaunchError::LaunchFailed);
    }
    guard.current_url()?;
    // Final caller-drop winner is independent of the controller mutex; finish
    // rechecks controller currency and its original deadline atomically there.
    claim_completion(decision)
}
fn claim_completion(decision: &AtomicU8) -> Result<(), LaunchError> {
    decision
        .compare_exchange(PENDING, COMPLETING, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| LaunchError::Cancelled)
}

fn validate_destination(original: &str) -> Result<(), LaunchError> {
    let invalid = LaunchError::InvalidDestination;
    if original.len() > MAX_URL_BYTES
        || !original.is_ascii()
        || original.bytes().any(|b| b.is_ascii_control())
        || !original.starts_with("https://accounts.google.com/o/oauth2/v2/auth?")
    {
        return Err(invalid);
    }
    let parsed = Url::parse(original).map_err(|_| invalid)?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("accounts.google.com")
        || parsed.port().is_some()
        || parsed.path() != "/o/oauth2/v2/auth"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || parsed.as_str() != original
    {
        return Err(invalid);
    }
    let query = parsed.query().ok_or(invalid)?;
    let mut fields = Vec::with_capacity(KEYS.len());
    for (index, pair) in query.split('&').enumerate() {
        let (key, value) = pair.split_once('=').ok_or(invalid)?;
        if KEYS.get(index).copied() != Some(key) {
            return Err(invalid);
        }
        fields.push(strict_form_decode(value)?);
    }
    if fields.len() != KEYS.len() {
        return Err(invalid);
    }
    if !installed_client(&fields[0])
        || !canonical_redirect(&fields[1])
        || fields[2] != "code"
        || !approved_scopes(&fields[3])
        || !canonical_secret(&fields[4])
        || !canonical_secret(&fields[5])
        || fields[6] != "S256"
    {
        return Err(invalid);
    }
    // Require precisely the generator's encoding, not a lossy parse/rewrite.
    // This also rejects lowercase/needless percent escapes, raw spaces and '%20'
    // aliases for the form serializer's '+', while preserving scope order.
    let mut canonical = Url::parse(GOOGLE_AUTHORIZATION_ENDPOINT).map_err(|_| invalid)?;
    {
        let mut pairs = canonical.query_pairs_mut();
        for (key, value) in KEYS.iter().zip(&fields) {
            pairs.append_pair(key, value);
        }
    }
    if canonical.as_str() != original {
        return Err(invalid);
    }
    Ok(())
}
fn strict_form_decode(value: &str) -> Result<String, LaunchError> {
    fn nibble(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => decoded.push(b' '),
            b'%' => {
                let high = bytes
                    .get(i + 1)
                    .copied()
                    .and_then(nibble)
                    .ok_or(LaunchError::InvalidDestination)?;
                let low = bytes
                    .get(i + 2)
                    .copied()
                    .and_then(nibble)
                    .ok_or(LaunchError::InvalidDestination)?;
                decoded.push(high * 16 + low);
                i += 2;
            }
            b => decoded.push(b),
        }
        i += 1;
    }
    String::from_utf8(decoded).map_err(|_| LaunchError::InvalidDestination)
}
fn installed_client(value: &str) -> bool {
    const SUFFIX: &str = ".apps.googleusercontent.com";
    value.len() <= 256
        && value.len() > SUFFIX.len()
        && value.ends_with(SUFFIX)
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
fn canonical_redirect(value: &str) -> bool {
    let Some(port) = value
        .strip_prefix("http://127.0.0.1:")
        .and_then(|v| v.strip_suffix(CALLBACK_PATH))
    else {
        return false;
    };
    let Ok(number) = port.parse::<u16>() else {
        return false;
    };
    number != 0 && number.to_string() == port
}
fn approved_scopes(value: &str) -> bool {
    let mut scopes = value.split(' ');
    let Some(first) = scopes.next() else {
        return false;
    };
    let approved =
        |s: &str| s == ApprovedScope::AppData.as_str() || s == ApprovedScope::File.as_str();
    if !approved(first) {
        return false;
    }
    match scopes.next() {
        None => true,
        Some(second) => approved(second) && first != second && scopes.next().is_none(),
    }
}
fn canonical_secret(value: &str) -> bool {
    if value.len() != 43 {
        return false;
    }
    match URL_SAFE_NO_PAD.decode(value) {
        Ok(bytes) => bytes.len() == 32 && URL_SAFE_NO_PAD.encode(bytes) == value,
        Err(_) => false,
    }
}

#[cfg(test)]
struct TestControl {
    entered: std::sync::atomic::AtomicBool,
    dispatch: std::sync::atomic::AtomicBool,
    started: std::sync::atomic::AtomicBool,
    wait_error: std::sync::atomic::AtomicBool,
    wait_error_seen: std::sync::atomic::AtomicBool,
    panic_after_spawn: std::sync::atomic::AtomicBool,
    original: std::sync::Mutex<Option<String>>,
}
#[cfg(test)]
impl TestControl {
    fn new() -> Arc<Self> {
        use std::sync::atomic::AtomicBool;
        Arc::new(Self {
            entered: AtomicBool::new(false),
            dispatch: AtomicBool::new(true),
            started: AtomicBool::new(false),
            wait_error: AtomicBool::new(false),
            wait_error_seen: AtomicBool::new(false),
            panic_after_spawn: AtomicBool::new(false),
            original: std::sync::Mutex::new(None),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::{
        AuthorizationOutcome, NativeStart, Phase, SessionController, StartError,
    };
    use super::*;
    #[cfg(unix)]
    use std::sync::atomic::AtomicBool;

    const CLIENT: &str = "browser-fixture.apps.googleusercontent.com";
    const STATE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn url(scope: &str) -> String {
        let mut value = Url::parse(GOOGLE_AUTHORIZATION_ENDPOINT).unwrap();
        value
            .query_pairs_mut()
            .append_pair("client_id", CLIENT)
            .append_pair(
                "redirect_uri",
                "http://127.0.0.1:43123/oauth2/callback/google",
            )
            .append_pair("response_type", "code")
            .append_pair("scope", scope)
            .append_pair("state", STATE)
            .append_pair("code_challenge", CHALLENGE)
            .append_pair("code_challenge_method", "S256");
        value.to_string()
    }
    fn request() -> NativeStart {
        NativeStart::new(
            CLIENT.into(),
            "browser-fixture".into(),
            "7".into(),
            vec![ApprovedScope::AppData, ApprovedScope::File],
        )
        .unwrap()
    }
    async fn ticket() -> (SessionController, BrowserLaunchTicket) {
        let owner = SessionController::new();
        let ticket = tokio::time::timeout(Duration::from_secs(3), owner.begin(request()))
            .await
            .unwrap()
            .unwrap();
        (owner, ticket)
    }
    async fn shutdown(owner: &SessionController) {
        tokio::time::timeout(Duration::from_secs(3), owner.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
    async fn terminal(owner: &SessionController) -> AuthorizationOutcome {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                if let Some(last) = owner.snapshot().unwrap().last_authorization {
                    return last.outcome;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("bounded controller terminal")
    }
    #[cfg(unix)]
    async fn flag(value: &AtomicBool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !value.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("bounded worker signal")
    }

    #[test]
    fn generated_subsets_scope_order_and_exact_encoding_are_accepted() {
        let a = ApprovedScope::AppData.as_str();
        let f = ApprovedScope::File.as_str();
        for scope in [
            a.to_owned(),
            f.to_owned(),
            format!("{a} {f}"),
            format!("{f} {a}"),
        ] {
            let value = url(&scope);
            assert_eq!(validate_destination(&value), Ok(()));
            assert!(value.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id="));
        }
        assert!(canonical_secret(STATE));
        assert!(canonical_secret(CHALLENGE)); // Published RFC7636 challenge, fixed expectation.
        assert!(installed_client("_.apps.googleusercontent.com"));
        assert!(canonical_redirect(
            "http://127.0.0.1:65535/oauth2/callback/google"
        ));
    }

    #[test]
    fn destination_aliases_duplicates_unknowns_and_lossy_encoding_fail() {
        let valid = url(ApprovedScope::File.as_str());
        let cases = [
            valid.replacen("https://", "http://", 1),
            valid.replacen("accounts.google.com", "ACCOUNTS.GOOGLE.COM", 1),
            valid.replacen("accounts.google.com", "accounts.google.com:443", 1),
            valid.replacen("accounts.google.com", "accounts.google.com.evil.invalid", 1),
            valid.replacen("accounts.google.com", "user@accounts.google.com", 1),
            valid.replacen("/o/oauth2/v2/auth", "/x/../o/oauth2/v2/auth", 1),
            valid.replacen("/auth?", "/%61uth?", 1),
            format!("{valid}#fragment"),
            format!("{valid}\n"),
            format!("{valid}&state={STATE}"),
            format!("{valid}&st%61te={STATE}"),
            format!("{valid}&extension=x"),
            valid.replacen("client_id=", "client%5Fid=", 1),
            valid.replacen("client_id=", "state=", 1),
            valid.replace(STATE, "%FF"),
            valid.replace(STATE, "%C0%AF"),
            valid.replace(STATE, "%"),
            valid.replace(STATE, "%0"),
            valid.replace(STATE, "%Q0"),
            valid.replace(STATE, "%00"),
            valid.replace(STATE, "é"),
            valid.replacen("%3A", "%3a", 1),
            valid.replacen("client_id=b", "client_id=%62", 1),
            valid.replace("response_type=code", "response_type=token"),
            valid.replace("code_challenge_method=S256", "code_challenge_method=plain"),
            "x".repeat(MAX_URL_BYTES + 1),
        ];
        for value in cases {
            assert_eq!(
                validate_destination(&value),
                Err(LaunchError::InvalidDestination)
            );
        }
        let both = url(&format!(
            "{} {}",
            ApprovedScope::AppData.as_str(),
            ApprovedScope::File.as_str()
        ));
        assert_eq!(
            validate_destination(&both.replace('+', "%20")),
            Err(LaunchError::InvalidDestination)
        );
        assert_eq!(strict_form_decode("%E2%82%AC").as_deref(), Ok("€"));
        assert_eq!(
            strict_form_decode("%E2%28%A1"),
            Err(LaunchError::InvalidDestination)
        );
    }

    #[test]
    fn field_bounds_and_canonical_base64_do_not_admit_variants() {
        for value in [
            ".apps.googleusercontent.com",
            " x.apps.googleusercontent.com",
            "x.apps.googleusercontent.com/",
            "x.apps.googleusercontent.com@evil",
        ] {
            assert!(!installed_client(value));
        }
        let suffix = ".apps.googleusercontent.com";
        assert!(installed_client(&format!(
            "{}{suffix}",
            "x".repeat(256 - suffix.len())
        )));
        assert!(!installed_client(&format!(
            "{}{suffix}",
            "x".repeat(257 - suffix.len())
        )));
        for value in [
            "http://localhost:1/oauth2/callback/google",
            "http://127.0.0.1:0/oauth2/callback/google",
            "http://127.0.0.1:01/oauth2/callback/google",
            "http://127.0.0.1:+1/oauth2/callback/google",
            "http://127.0.0.1:65536/oauth2/callback/google",
            "http://127.0.0.1:1/oauth2/callback/%67oogle",
            "http://127.0.0.1:1/oauth2/callback/google?x=1",
        ] {
            assert!(!canonical_redirect(value));
        }
        for value in [
            "".to_owned(),
            format!(
                "{} {}",
                ApprovedScope::File.as_str(),
                ApprovedScope::File.as_str()
            ),
            format!("{} ", ApprovedScope::File.as_str()),
            "https://www.googleapis.com/auth/drive".into(),
        ] {
            assert!(!approved_scopes(&value));
            assert_eq!(
                validate_destination(&url(&value)),
                Err(LaunchError::InvalidDestination)
            );
        }
        for value in [
            format!("{STATE}="),
            STATE[..42].to_owned(),
            "A".repeat(44),
            format!("{}B", &STATE[..42]),
        ] {
            assert!(!canonical_secret(&value));
        }
    }

    #[test]
    fn selected_recipe_is_exactly_first_and_never_a_program_override() {
        let value = url(ApprovedScope::File.as_str());
        let expected = open::commands(&value).into_iter().next().unwrap();
        let chosen = first_default_command(&value).unwrap();
        assert!(chosen.get_program() == expected.get_program());
        assert!(chosen.get_args().eq(expected.get_args()));
        assert!(chosen.get_envs().eq(expected.get_envs()));
        #[cfg(target_os = "macos")]
        {
            assert!(chosen.get_program() == "/usr/bin/open");
            assert!(
                chosen
                    .get_args()
                    .eq([std::ffi::OsStr::new("--"), std::ffi::OsStr::new(&value)])
            );
        }
    }

    #[test]
    fn private_synthetic_plan_carries_original_bytes_without_dispatch() {
        let control = TestControl::new();
        let value = url(ApprovedScope::File.as_str());
        let plan = LaunchPlan::Synthetic {
            command: Box::new(Command::new("nonexecuted-synthetic-command")),
            control: Arc::clone(&control),
        };
        let selected = plan.command(&value).unwrap();
        assert!(selected.get_program() == "nonexecuted-synthetic-command");
        assert!(control.original.lock().unwrap().as_deref() == Some(value.as_str()));
        assert!(!control.started.load(Ordering::Acquire));
    }

    #[test]
    fn caller_drop_and_completion_have_one_atomic_winner() {
        use std::sync::Barrier;
        for _ in 0..16 {
            let state = Arc::new(AtomicU8::new(PENDING));
            let barrier = Barrier::new(3);
            let result = std::thread::scope(|threads| {
                let c = Arc::clone(&state);
                let a = threads.spawn(|| {
                    barrier.wait();
                    claim_completion(&state)
                });
                let d = threads.spawn(|| {
                    barrier.wait();
                    drop(CancelOnDrop(c));
                });
                barrier.wait();
                d.join().unwrap();
                a.join().unwrap()
            });
            match result {
                Ok(()) => assert_eq!(state.load(Ordering::Acquire), COMPLETING),
                Err(LaunchError::Cancelled) => assert_eq!(state.load(Ordering::Acquire), CANCELLED),
                _ => panic!("unexpected static completion disposition"),
            }
            assert_eq!(claim_completion(&state), Err(LaunchError::Cancelled));
        }
    }

    #[tokio::test]
    async fn dropping_unpolled_public_future_does_not_dispatch() {
        let (owner, ticket) = ticket().await;
        drop(hand_off(ticket)); // Public API; no polling, so no OS command construction/launch.
        assert_eq!(
            terminal(&owner).await,
            AuthorizationOutcome::BrowserUnavailable
        );
        shutdown(&owner).await;
    }

    #[cfg(unix)]
    mod processes {
        use super::*;
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::path::PathBuf;

        struct Fixture {
            _dir: tempfile::TempDir,
            release: PathBuf,
            control: Arc<TestControl>,
        }
        impl Fixture {
            fn held() -> (Self, LaunchPlan) {
                Self::bounded(240)
            }
            fn bounded(ticks: u16) -> (Self, LaunchPlan) {
                let dir = tempfile::tempdir().unwrap();
                let release = dir.path().join("release");
                let control = TestControl::new();
                let mut command = Command::new("/bin/sh");
                command.args(["-c", "i=0; while [ ! -e \"$RAPIDRAW_BROWSER_RELEASE\" ]; do i=$((i+1)); if [ \"$i\" -gt \"$RAPIDRAW_BROWSER_TICKS\" ]; then exit 9; fi; sleep 0.025; done; exit 0"])
                    .env("RAPIDRAW_BROWSER_RELEASE", &release)
                    .env("RAPIDRAW_BROWSER_TICKS", ticks.to_string());
                let plan = LaunchPlan::Synthetic {
                    command: Box::new(command),
                    control: Arc::clone(&control),
                };
                (
                    Self {
                        _dir: dir,
                        release,
                        control,
                    },
                    plan,
                )
            }
            fn release(&self) {
                std::fs::write(&self.release, b"release").unwrap();
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                self.control.dispatch.store(true, Ordering::Release);
                self.control.wait_error.store(false, Ordering::Release);
                let _ = std::fs::write(&self.release, b"release");
                // Child fixture also has its own finite bound if a panic deletes
                // the directory before its next poll. The worker still reaps it.
            }
        }
        fn command(script: &str) -> (Arc<TestControl>, LaunchPlan) {
            let control = TestControl::new();
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            (
                Arc::clone(&control),
                LaunchPlan::Synthetic {
                    command: Box::new(command),
                    control,
                },
            )
        }

        #[tokio::test]
        async fn real_selected_child_success_nonzero_and_spawn_error_are_distinct() {
            for mode in 0..3 {
                let (owner, ticket) = ticket().await;
                let original = ticket.url_for_native_launcher().unwrap().to_owned();
                let (control, plan) = if mode == 2 {
                    let control = TestControl::new();
                    let directory = tempfile::tempdir().unwrap();
                    let command = Command::new(directory.path().join("absent-launcher"));
                    (
                        Arc::clone(&control),
                        LaunchPlan::Synthetic {
                            command: Box::new(command),
                            control,
                        },
                    )
                } else {
                    command(if mode == 0 {
                        // Null standard streams are character devices, and stdin
                        // is EOF. No captured output or inherited terminal/pipe.
                        "test -c /dev/fd/0 && test -c /dev/fd/1 && test -c /dev/fd/2 || exit 8; if read x; then exit 8; fi; exit 0"
                    } else {
                        "exit 7"
                    })
                };
                let outcome =
                    tokio::time::timeout(Duration::from_secs(3), hand_off_with(ticket, plan))
                        .await
                        .unwrap();
                assert!(
                    control.original.lock().unwrap().as_deref() == Some(original.as_str()),
                    "original generated bytes only"
                );
                if mode == 0 {
                    assert_eq!(outcome, Ok(LaunchOutcome::HandedOff));
                    assert!(owner.snapshot().unwrap().last_authorization.is_none());
                    assert_eq!(owner.snapshot().unwrap().phase, Phase::AwaitingCallback);
                } else {
                    assert_eq!(outcome, Err(LaunchError::LaunchFailed));
                    assert_eq!(
                        terminal(&owner).await,
                        AuthorizationOutcome::BrowserUnavailable
                    );
                }
                assert_eq!(control.started.load(Ordering::Acquire), mode != 2);
                shutdown(&owner).await;
            }
        }

        #[tokio::test]
        async fn stale_and_real_handoff_expiry_prevent_any_dispatch() {
            for expired in [false, true] {
                let (owner, ticket) = ticket().await;
                if expired {
                    // Actual existing 10s handoff timer: no private session clock
                    // mutation or new test accessor. This definition is not run here.
                    assert_eq!(terminal(&owner).await, AuthorizationOutcome::TimedOut);
                } else {
                    owner
                        .cancel(owner.snapshot().unwrap().attempt_id.unwrap())
                        .unwrap();
                }
                let (control, plan) = command("exit 0");
                let result = hand_off_with(ticket, plan).await;
                assert!(matches!(
                    result,
                    Err(LaunchError::Ticket(
                        TicketError::Stale | TicketError::Expired
                    ))
                ));
                assert!(!control.started.load(Ordering::Acquire));
                assert!(control.original.lock().unwrap().is_none());
                shutdown(&owner).await;
            }
        }

        #[tokio::test]
        async fn caller_drop_before_dispatch_releases_failure_without_starting_child() {
            let (owner, ticket) = ticket().await;
            let (control, plan) = command("exit 0");
            control.dispatch.store(false, Ordering::Release);
            let mut operation = Box::pin(hand_off_with(ticket, plan));
            tokio::select! { biased; _ = &mut operation => panic!("held dispatch"), _ = flag(&control.entered) => {} }
            drop(operation);
            assert_eq!(
                terminal(&owner).await,
                AuthorizationOutcome::BrowserUnavailable
            );
            assert!(!control.started.load(Ordering::Acquire));
            shutdown(&owner).await;
        }

        #[tokio::test]
        async fn dropped_caller_and_controller_cancel_keep_actual_child_shutdown_fence() {
            for drop_caller in [false, true] {
                let (owner, ticket) = ticket().await;
                let (fixture, plan) = Fixture::held();
                let mut operation = Box::pin(hand_off_with(ticket, plan));
                tokio::select! { biased; _ = &mut operation => panic!("child is held"), _ = flag(&fixture.control.started) => {} }
                let mut operation = if drop_caller {
                    drop(operation);
                    None
                } else {
                    Some(operation)
                };
                let id = owner.snapshot().unwrap().attempt_id.unwrap();
                owner.cancel(id).unwrap();
                assert_eq!(owner.begin(request()).await.err(), Some(StartError::Busy));
                let mut closing = Box::pin(owner.shutdown());
                tokio::select! { biased; _ = &mut closing => panic!("child acknowledgment still owned"), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
                fixture.release();
                if let Some(op) = operation.take() {
                    assert!(matches!(
                        tokio::time::timeout(Duration::from_secs(3), op)
                            .await
                            .unwrap(),
                        Err(LaunchError::Ticket(_))
                    ));
                }
                tokio::time::timeout(Duration::from_secs(3), closing)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    owner
                        .snapshot()
                        .unwrap()
                        .last_authorization
                        .unwrap()
                        .outcome,
                    AuthorizationOutcome::Cancelled
                );
            }
        }

        #[tokio::test]
        async fn panic_and_unknown_wait_keep_ticket_until_real_child_reap() {
            for panic_worker in [false, true] {
                let (owner, ticket) = ticket().await;
                let (fixture, plan) = Fixture::held();
                fixture
                    .control
                    .panic_after_spawn
                    .store(panic_worker, Ordering::Release);
                fixture
                    .control
                    .wait_error
                    .store(!panic_worker, Ordering::Release);
                let mut operation = Box::pin(hand_off_with(ticket, plan));
                tokio::select! { biased; _ = &mut operation => panic!("child cleanup must be held"), _ = flag(&fixture.control.started) => {} }
                if !panic_worker {
                    tokio::select! { biased; _ = &mut operation => panic!("unknown wait cannot release"), _ = flag(&fixture.control.wait_error_seen) => {} }
                }
                let mut closing = Box::pin(owner.shutdown());
                tokio::select! { biased; _ = &mut closing => panic!("guard owns child and ticket"), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
                fixture.release();
                if !panic_worker {
                    tokio::select! { biased; _ = &mut operation => panic!("wait remains unproven"), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
                    fixture.control.wait_error.store(false, Ordering::Release);
                }
                let result = tokio::time::timeout(Duration::from_secs(3), operation)
                    .await
                    .unwrap();
                assert_eq!(
                    result,
                    Err(if panic_worker {
                        LaunchError::WorkerFailure
                    } else {
                        LaunchError::LaunchFailed
                    })
                );
                tokio::time::timeout(Duration::from_secs(3), closing)
                    .await
                    .unwrap()
                    .unwrap();
            }
        }

        #[tokio::test]
        async fn actual_handoff_deadline_does_not_release_a_started_child() {
            let (owner, ticket) = ticket().await;
            let (fixture, plan) = Fixture::bounded(600); // 600 iterations/25ms sleeps; no OS wall-clock guarantee.
            let mut operation = Box::pin(hand_off_with(ticket, plan));
            tokio::select! { biased; _ = &mut operation => panic!("held child"), _ = flag(&fixture.control.started) => {} }
            assert_eq!(terminal(&owner).await, AuthorizationOutcome::TimedOut);
            assert_eq!(owner.snapshot().unwrap().phase, Phase::Draining);
            assert_eq!(owner.begin(request()).await.err(), Some(StartError::Busy));
            let mut closing = Box::pin(owner.shutdown());
            tokio::select! { biased; _ = &mut closing => panic!("timed out is not reaped"), _ = tokio::time::sleep(Duration::from_millis(50)) => {} }
            fixture.release();
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(3), operation)
                    .await
                    .unwrap(),
                Err(LaunchError::Ticket(_))
            ));
            tokio::time::timeout(Duration::from_secs(3), closing)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                owner
                    .snapshot()
                    .unwrap()
                    .last_authorization
                    .unwrap()
                    .outcome,
                AuthorizationOutcome::TimedOut
            );
        }

        #[tokio::test]
        async fn earlier_denial_callback_waits_for_real_launcher_acknowledgment() {
            let (owner, ticket) = ticket().await;
            let parsed = Url::parse(ticket.url_for_native_launcher().unwrap()).unwrap();
            let redirect = parsed
                .query_pairs()
                .find(|(k, _)| k == "redirect_uri")
                .unwrap()
                .1
                .into_owned();
            let state = parsed
                .query_pairs()
                .find(|(k, _)| k == "state")
                .unwrap()
                .1
                .into_owned();
            let address = Url::parse(&redirect).unwrap();
            let port = address.port().unwrap();
            let target = format!("{}?state={state}&error=access_denied", address.path());
            let callback = tokio::task::spawn_blocking(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                stream.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                let mut reply = [0; 512];
                let _ = stream.read(&mut reply);
            });
            tokio::time::timeout(Duration::from_secs(2), callback)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(owner.snapshot().unwrap().phase, Phase::AwaitingBrowser);
            assert!(owner.snapshot().unwrap().last_authorization.is_none());
            let (fixture, plan) = Fixture::held();
            let mut operation = Box::pin(hand_off_with(ticket, plan));
            tokio::select! { biased; _ = &mut operation => panic!("held launcher"), _ = flag(&fixture.control.started) => {} }
            assert!(owner.snapshot().unwrap().last_authorization.is_none());
            fixture.release();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), operation)
                    .await
                    .unwrap(),
                Ok(LaunchOutcome::HandedOff)
            );
            assert_eq!(terminal(&owner).await, AuthorizationOutcome::Denied);
            shutdown(&owner).await;
            // Never send synthetic Code: no production token/account endpoint is reached.
        }
    }
}

#[cfg(test)]
#[path = "browser_independent_tests.rs"]
mod browser_independent_tests;
