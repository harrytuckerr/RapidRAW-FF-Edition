use super::*;
use serde_json::{Value, json};
const EMPTY: &str = r#"{"schemaVersion":1}"#;
const BEGIN: &str =
    r#"{"schemaVersion":1,"registrationId":"missing","capabilities":["stateLibrary"]}"#;
const CANCEL: &str = r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000"}"#;
const DECIDE: &str = r#"{"schemaVersion":1,"attemptId":"00000000-0000-0000-0000-000000000000","generation":"0","decisionId":"00000000-0000-0000-0000-000000000000","decision":"accept"}"#;

#[test]
fn independent_lifecycle_wire_limits_and_static_errors_leave_owner_open() {
    const {
        assert!(LIMIT == 4096);
    }
    let owner = DesktopState::empty().unwrap();
    for raw in [
        "",
        "null",
        "[]",
        "{}",
        r#"{"schemaVersion":true}"#,
        r#"{"schemaVersion":1.0}"#,
        r#"{"schemaVersion":1e0}"#,
        r#"{"schemaVersion":2}"#,
        r#"{"schemaVersion":1,"schema\u0056ersion":1}"#,
        r#"{"schemaVersion":1,"extra":null}"#,
        r#"{"schemaVersion":1}[]"#,
    ] {
        assert!(matches!(
            decode::<EmptyRequest>(raw),
            Err(CommandError::InvalidRequest)
        ));
    }
    for code in [
        "-1",
        "2",
        "2147483647",
        "2147483648",
        "0.0",
        "1e0",
        "true",
        "null",
        "\"0\"",
    ] {
        let raw = format!("{{\"schemaVersion\":1,\"code\":{code}}}");
        assert_eq!(exit_intent(&raw), Err(CommandError::InvalidRequest));
    }
    assert_eq!(
        exit_intent(r#"{"schemaVersion":1,"code":0,"c\u006fde":1}"#),
        Err(CommandError::InvalidRequest)
    );
    for code in [0, 1] {
        assert_eq!(
            exit_intent(&format!("{{\"schemaVersion\":1,\"code\":{code}}}")),
            Ok(Intent::Quit { code })
        );
    }
    let exact = format!("{EMPTY}{}", " ".repeat(4096 - EMPTY.len()));
    assert!(decode::<EmptyRequest>(&exact).is_ok());
    assert!(matches!(
        decode::<EmptyRequest>(&(exact + " ")),
        Err(CommandError::InvalidRequest)
    ));
    assert_eq!(
        serde_json::from_str::<Value>(&encode(&owner.snapshot()).unwrap()).unwrap(),
        json!({"schemaVersion":1,"phase":"open","intent":null,"closureFault":null,"cleanupFault":null,"fault":null})
    );
    assert_eq!(
        futures::executor::block_on(begin_forward("main", &owner, BEGIN)),
        Err(CommandError::Facade(FacadeError::NotConfigured))
    );
    for (error, label) in [
        (CommandError::InvalidRequest, "invalidRequest"),
        (CommandError::ForbiddenWindow, "forbiddenWindow"),
        (CommandError::Unavailable, "unavailable"),
        (CommandError::AlreadyFinalizing, "alreadyFinalizing"),
        (
            CommandError::Facade(FacadeError::ControllerFault),
            "controllerFault",
        ),
    ] {
        assert_eq!(serde_json::to_value(error).unwrap(), json!(label));
    }
    #[derive(Serialize)]
    struct Payload {
        text: String,
    }
    assert_eq!(
        encode(&Payload {
            text: "x".repeat(4096)
        }),
        Err(CommandError::Unavailable)
    );
    assert!(owner.application.mark_closing().first); // invalid inputs never marked A.
}

#[test]
fn independent_all_facade_routes_guard_then_forward_without_extra_authority() {
    type Method = fn(&DriveApplication, &str) -> Result<String, FacadeError>;
    struct Case {
        method: Method,
        raw: &'static str,
        field: &'static str,
        value: Value,
    }
    let cases = [
        Case {
            method: DriveApplication::options,
            raw: EMPTY,
            field: "configured",
            value: json!(false),
        },
        Case {
            method: DriveApplication::status,
            raw: EMPTY,
            field: "phase",
            value: json!("idle"),
        },
        Case {
            method: DriveApplication::verification_status,
            raw: EMPTY,
            field: "phase",
            value: json!("idle"),
        },
        Case {
            method: DriveApplication::connection,
            raw: EMPTY,
            field: "status",
            value: json!("disconnected"),
        },
        Case {
            method: DriveApplication::proposal,
            raw: EMPTY,
            field: "proposal",
            value: Value::Null,
        },
        Case {
            method: DriveApplication::cancel,
            raw: CANCEL,
            field: "outcome",
            value: json!("notCurrent"),
        },
        Case {
            method: DriveApplication::decide,
            raw: DECIDE,
            field: "outcome",
            value: json!("stale"),
        },
        Case {
            method: DriveApplication::disconnect,
            raw: EMPTY,
            field: "outcome",
            value: json!("ownershipInvalidated"),
        },
    ];
    for case in cases {
        let owner = DesktopState::empty().unwrap();
        let before = owner.application.status(EMPTY).unwrap();
        for label in ["", "Main", "main ", "other", "main\0"] {
            assert_eq!(
                forward(label, &owner, case.raw, case.method),
                Err(CommandError::ForbiddenWindow)
            );
            assert_eq!(owner.application.status(EMPTY).unwrap(), before);
        }
        assert_eq!(
            forward("main", &owner, "{}", case.method),
            Err(CommandError::Facade(FacadeError::InvalidRequest))
        );
        let raw = forward("main", &owner, case.raw, case.method).unwrap();
        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["schemaVersion"], json!(1));
        assert_eq!(value[case.field], case.value);
        assert!(raw.len() <= 4096);
        for forbidden in [
            "accessToken",
            "refreshToken",
            "clientId",
            "authorizationUrl",
            "https://",
        ] {
            assert!(!raw.contains(forbidden));
        }
    }
    let owner = DesktopState::empty().unwrap();
    assert_eq!(
        futures::executor::block_on(begin_forward("other", &owner, BEGIN)),
        Err(CommandError::ForbiddenWindow)
    );
    assert_eq!(
        futures::executor::block_on(begin_forward("main", &owner, BEGIN)),
        Err(CommandError::Facade(FacadeError::NotConfigured))
    );
}

#[test]
fn independent_capability_cfg_and_caller_matrix_has_no_desktop_plugin_fallback() {
    let common: Value =
        serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
    let desktop: Value =
        serde_json::from_str(include_str!("../../capabilities/desktop.json")).unwrap();
    let mobile: Value =
        serde_json::from_str(include_str!("../../capabilities/mobile-process.json")).unwrap();
    let conf: Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    assert_eq!(mobile["windows"], json!(["main"]));
    assert_eq!(mobile["platforms"], json!(["android", "iOS"]));
    assert_eq!(mobile["permissions"], json!(["process:default"]));
    assert_eq!(
        conf["app"]["security"]["capabilities"],
        json!(["default", "desktop-cloud", "mobile-process"])
    );
    for c in [&common, &desktop, &mobile] {
        assert!(c.get("remote").is_none());
    }
    for c in [&common, &desktop] {
        for p in c["permissions"].as_array().unwrap() {
            assert!(!p.as_str().is_some_and(|s| s.starts_with("process:")));
        }
    }
    let lib = include_str!("../lib.rs");
    let cfg = "#[cfg(any(target_os = \"windows\", target_os = \"macos\", target_os = \"linux\"))]";
    for name in [
        "drive_auth_options",
        "drive_auth_begin",
        "drive_auth_status",
        "drive_auth_verification_status",
        "drive_auth_connection",
        "drive_auth_proposal",
        "drive_auth_cancel",
        "drive_auth_decide",
        "drive_auth_disconnect",
        "app_exit",
        "app_restart",
        "app_lifecycle_status",
    ] {
        assert!(lib.contains(&format!("{cfg}\n            drive_auth::commands::{name},")));
    }
    let modules = include_str!("mod.rs");
    for name in ["commands", "lifecycle"] {
        assert!(modules.contains(&format!("{cfg}\npub(crate) mod {name};")));
    }
    let settings = include_str!("../../../src/components/panel/SettingsPanel.tsx");
    let body = settings
        .split("const handleSaveAndRelaunch = async () => {")
        .nth(1)
        .unwrap()
        .split("const handleProviderChange")
        .next()
        .unwrap();
    assert!(body.contains("['macos', 'windows', 'linux'].includes(osPlatform)"));
    assert!(
        body.contains("invoke('app_restart', { request: JSON.stringify({ schemaVersion: 1 }) })")
    );
    assert!(body.contains("osPlatform === 'android' || osPlatform === 'ios'"));
    let external = include_str!("../../../src/hooks/useExternalEditSession.ts");
    assert!(external.contains(
        "invoke('app_exit', { request: JSON.stringify({ schemaVersion: 1, code: 0 }) })"
    ));
    assert!(external.contains("void requestExit();"));
    // Actual callback execution is the separately authorized four Node cases.
    // Source matrix cannot prove Tauri request origin or effective IPC denial.
}
