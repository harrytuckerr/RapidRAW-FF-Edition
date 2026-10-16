//! Main-window desktop IPC. Tauri's request-origin ACL gate precedes these
//! wrappers; no remote capabilities are installed. Label checks are not a claim
//! that a direct function fixture exercises that IPC gate. No token/URL events.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use tauri::{AppHandle, State, WebviewWindow};

use super::application::{DriveApplication, FacadeError};
use super::lifecycle::{self, DesktopState, Intent, RequestError};

const LIMIT: usize = 4096;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandError {
    InvalidRequest,
    ForbiddenWindow,
    Unavailable,
    AlreadyFinalizing,
    Facade(FacadeError),
}
impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::InvalidRequest => serializer.serialize_str("invalidRequest"),
            Self::ForbiddenWindow => serializer.serialize_str("forbiddenWindow"),
            Self::Unavailable => serializer.serialize_str("unavailable"),
            Self::AlreadyFinalizing => serializer.serialize_str("alreadyFinalizing"),
            Self::Facade(error) => error.serialize(serializer),
        }
    }
}
fn guard(label: &str) -> Result<(), CommandError> {
    if label == "main" {
        Ok(())
    } else {
        Err(CommandError::ForbiddenWindow)
    }
}
fn lifecycle_error(error: RequestError) -> CommandError {
    match error {
        RequestError::Unavailable => CommandError::Unavailable,
        RequestError::AlreadyFinalizing => CommandError::AlreadyFinalizing,
    }
}
fn forward(
    label: &str,
    state: &DesktopState,
    request: &str,
    method: fn(&DriveApplication, &str) -> Result<String, FacadeError>,
) -> Result<String, CommandError> {
    guard(label)?;
    method(&state.application, request).map_err(CommandError::Facade)
}
async fn begin_forward(
    label: &str,
    state: &DesktopState,
    request: &str,
) -> Result<String, CommandError> {
    guard(label)?;
    state
        .application
        .begin(request)
        .await
        .map_err(CommandError::Facade)
}

#[tauri::command]
pub(crate) fn drive_auth_options(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(window.label(), &state, &request, DriveApplication::options)
}

#[tauri::command]
pub(crate) fn drive_auth_status(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(window.label(), &state, &request, DriveApplication::status)
}

#[tauri::command]
pub(crate) fn drive_auth_verification_status(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(
        window.label(),
        &state,
        &request,
        DriveApplication::verification_status,
    )
}

#[tauri::command]
pub(crate) fn drive_auth_connection(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(
        window.label(),
        &state,
        &request,
        DriveApplication::connection,
    )
}

#[tauri::command]
pub(crate) fn drive_auth_proposal(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(window.label(), &state, &request, DriveApplication::proposal)
}

#[tauri::command]
pub(crate) fn drive_auth_cancel(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(window.label(), &state, &request, DriveApplication::cancel)
}

#[tauri::command]
pub(crate) fn drive_auth_decide(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(window.label(), &state, &request, DriveApplication::decide)
}

#[tauri::command]
pub(crate) fn drive_auth_disconnect(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    forward(
        window.label(),
        &state,
        &request,
        DriveApplication::disconnect,
    )
}

#[tauri::command]
pub(crate) async fn drive_auth_begin(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    begin_forward(window.label(), &state, &request).await
}
struct Version;
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match u8::deserialize(d)? {
            1 => Ok(Self),
            _ => Err(de::Error::custom("invalid version")),
        }
    }
}
struct ExitCode(i32);
impl<'de> Deserialize<'de> for ExitCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match u8::deserialize(d)? {
            code @ (0 | 1) => Ok(Self(i32::from(code))),
            _ => Err(de::Error::custom("invalid exit code")),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExitRequest {
    #[serde(rename = "schemaVersion")]
    _version: Version,
    code: ExitCode,
}
fn decode<T: de::DeserializeOwned>(raw: &str) -> Result<T, CommandError> {
    if raw.len() > LIMIT || !raw.trim_start().starts_with('{') {
        return Err(CommandError::InvalidRequest);
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = T::deserialize(&mut decoder).map_err(|_| CommandError::InvalidRequest)?;
    decoder.end().map_err(|_| CommandError::InvalidRequest)?;
    Ok(value)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a, T> {
    schema_version: u8,
    #[serde(flatten)]
    body: &'a T,
}
fn encode<T: Serialize>(body: &T) -> Result<String, CommandError> {
    let encoded = serde_json::to_string(&Envelope {
        schema_version: 1,
        body,
    })
    .map_err(|_| CommandError::Unavailable)?;
    if encoded.len() > LIMIT {
        Err(CommandError::Unavailable)
    } else {
        Ok(encoded)
    }
}
#[derive(Serialize)]
struct Reply {
    outcome: lifecycle::Acknowledgment,
}
fn exit_intent(raw: &str) -> Result<Intent, CommandError> {
    let request = decode::<ExitRequest>(raw)?;
    Ok(Intent::Quit {
        code: request.code.0,
    })
}
#[tauri::command]
pub(crate) fn app_exit(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    guard(window.label())?;
    let intent = exit_intent(&request)?;
    let outcome = lifecycle::request(&state, &app, intent).map_err(lifecycle_error)?;
    encode(&Reply { outcome })
}
#[tauri::command]
pub(crate) fn app_restart(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    guard(window.label())?;
    decode::<EmptyRequest>(&request)?;
    let outcome = lifecycle::request(&state, &app, Intent::Restart).map_err(lifecycle_error)?;
    encode(&Reply { outcome })
}
#[tauri::command]
pub(crate) fn app_lifecycle_status(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
    request: String,
) -> Result<String, CommandError> {
    guard(window.label())?;
    decode::<EmptyRequest>(&request)?;
    encode(&state.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    const EMPTY: &str = r#"{"schemaVersion":1}"#;
    type Method = fn(&DriveApplication, &str) -> Result<String, FacadeError>;
    #[test]
    fn invalid_lifecycle_requests_leave_the_owner_open_before_any_action() {
        let state = DesktopState::empty().unwrap();
        let before = encode(&state.snapshot()).unwrap();
        for raw in [
            r#"{"schemaVersion":1,"code":2147483647}"#,
            r#"{"schemaVersion":1,"code":-1}"#,
            r#"{"schemaVersion":1,"code":2}"#,
            r#"{"schemaVersion":1,"code":1.0}"#,
            r#"{"schemaVersion":1,"code":"0"}"#,
            r#"{"schemaVersion":1,"code":null}"#,
            r#"{"schemaVersion":1,"code":true}"#,
            r#"{"schemaVersion":1,"code":1e0}"#,
            r#"{"schemaVersion":1,"code":0,"c\u006fde":1}"#,
            r#"{"schemaVersion":1,"code":0,"schema\u0056ersion":1}"#,
            r#"{"schemaVersion":1,"code":0,"extra":1}"#,
            r#"{"schemaVersion":1,"code":0}{}"#,
            r#"{"schemaVersion":0,"code":0}"#,
            r#"{"schemaVersion":1.0,"code":0}"#,
            "[1,0]",
            "null",
            "{}",
        ] {
            assert_eq!(exit_intent(raw), Err(CommandError::InvalidRequest));
            assert_eq!(encode(&state.snapshot()).unwrap(), before);
        }
        for code in [0, 1] {
            assert_eq!(
                exit_intent(&json!({"schemaVersion":1,"code":code}).to_string()).unwrap(),
                Intent::Quit { code }
            );
        }
        let exact = format!("{EMPTY}{}", " ".repeat(LIMIT - EMPTY.len()));
        assert!(decode::<EmptyRequest>(&exact).is_ok());
        assert!(matches!(
            decode::<EmptyRequest>(&(exact + " ")),
            Err(CommandError::InvalidRequest)
        ));
        for raw in [
            "[1]",
            r#"{"schemaVersion":1,"code":0}"#,
            r#"{"schemaVersion":1,"schema\u0056ersion":1}"#,
            r#"{"schemaVersion":1}false"#,
        ] {
            assert!(matches!(
                decode::<EmptyRequest>(raw),
                Err(CommandError::InvalidRequest)
            ));
        }
        assert_eq!(encode(&state.snapshot()).unwrap(), before);
    }
    #[test]
    fn all_sync_forwards_apply_the_window_guard_and_preserve_a_bytes() {
        let methods: [(Method, &str); 8] = [
            (DriveApplication::options, EMPTY),
            (DriveApplication::status, EMPTY),
            (DriveApplication::verification_status, EMPTY),
            (DriveApplication::connection, EMPTY),
            (DriveApplication::proposal, EMPTY),
            (
                DriveApplication::cancel,
                r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000"}"#,
            ),
            (
                DriveApplication::decide,
                r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000","generation":"0","decisionId":"00000000-0000-0000-0000-000000000000","decision":"accept"}"#,
            ),
            (DriveApplication::disconnect, EMPTY),
        ];
        for (method, request) in methods {
            let a = DesktopState::empty().unwrap();
            let b = DesktopState::empty().unwrap();
            let before = a.application.status(EMPTY).unwrap();
            for label in ["", "Main", "other", "main "] {
                assert_eq!(
                    forward(label, &a, request, method),
                    Err(CommandError::ForbiddenWindow)
                );
            }
            assert_eq!(a.application.status(EMPTY).unwrap(), before);
            assert_eq!(
                forward("main", &a, request, method),
                method(&b.application, request).map_err(CommandError::Facade)
            );
            assert_eq!(
                forward("main", &a, "invalid", method),
                Err(CommandError::Facade(FacadeError::InvalidRequest))
            );
        }
        // Direct guard calls are NOT proof of the framework remote-origin gate.
    }
    #[test]
    fn async_forward_empty_config_and_static_error_status_serialization() {
        let state = DesktopState::empty().unwrap();
        let request = r#"{"schemaVersion":1,"registrationId":"not-configured","capabilities":["stateLibrary"]}"#;
        assert_eq!(
            futures::executor::block_on(begin_forward("other", &state, request)),
            Err(CommandError::ForbiddenWindow)
        );
        assert_eq!(
            futures::executor::block_on(begin_forward("main", &state, request)),
            Err(CommandError::Facade(FacadeError::NotConfigured))
        );
        let v: Value = serde_json::from_str(&encode(&state.snapshot()).unwrap()).unwrap();
        assert_eq!(
            v,
            json!({"schemaVersion":1,"phase":"open","intent":null,"closureFault":null,"cleanupFault":null,"fault":null})
        );
        for (e, s) in [
            (CommandError::InvalidRequest, "invalidRequest"),
            (CommandError::ForbiddenWindow, "forbiddenWindow"),
            (CommandError::Unavailable, "unavailable"),
            (CommandError::AlreadyFinalizing, "alreadyFinalizing"),
        ] {
            assert_eq!(serde_json::to_value(e).unwrap(), json!(s));
        }
        for code in [
            FacadeError::InvalidRequest,
            FacadeError::NotConfigured,
            FacadeError::UnknownRegistration,
            FacadeError::DisallowedCapabilities,
            FacadeError::Busy,
            FacadeError::Closing,
            FacadeError::Closed,
            FacadeError::Stale,
            FacadeError::TimedOut,
            FacadeError::Cancelled,
            FacadeError::LaunchFailed,
            FacadeError::WorkerFailure,
            FacadeError::AttemptEnded,
            FacadeError::ControllerFault,
        ] {
            assert_eq!(
                serde_json::to_value(CommandError::Facade(code)).unwrap(),
                serde_json::to_value(code).unwrap()
            );
        }
        for ack in [
            lifecycle::Acknowledgment::Accepted,
            lifecycle::Acknowledgment::AlreadyPending,
            lifecycle::Acknowledgment::QuitPending,
        ] {
            assert!(encode(&Reply { outcome: ack }).unwrap().len() < LIMIT);
        }
        #[derive(Serialize)]
        struct Large {
            value: String,
        }
        assert_eq!(
            encode(&Large {
                value: "x".repeat(LIMIT)
            }),
            Err(CommandError::Unavailable)
        );
    }
    #[test]
    fn static_capability_selection_removes_both_desktop_bypasses_and_retains_mobile() {
        let common: Value =
            serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        let mobile: Value =
            serde_json::from_str(include_str!("../../capabilities/mobile-process.json")).unwrap();
        let desktop: Value =
            serde_json::from_str(include_str!("../../capabilities/desktop.json")).unwrap();
        let config: Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(mobile["platforms"], json!(["android", "iOS"]));
        assert_eq!(mobile["windows"], json!(["main"]));
        assert_eq!(mobile["permissions"], json!(["process:default"]));
        assert!(
            config["app"]["security"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("mobile-process"))
        );
        for cap in [&common, &desktop] {
            assert!(
                cap["permissions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| !p.as_str().is_some_and(|s| s.starts_with("process:")))
            );
        }
        for cap in [&common, &desktop, &mobile] {
            assert!(cap.get("remote").is_none());
        }
        // Manifest policy fixture only: actual desktop IPC denial/mobile compile
        // requires the independent platform/Tauri gates, not JSON inspection.
    }
}

#[cfg(test)]
#[path = "commands_independent_tests.rs"]
mod commands_independent_tests;
