# GD1b7-B — coherent desktop lifecycle amendment

Design only; gpt-6-astra, 2026-10-16, canonical `abf1ed796246cb952691d287451b843c61985c43`. A’s 527Pass/14ignored/fmt/strict are accepted prior evidence, not app acceptance. I authored A: this is lifecycle design, not independent review. Frozen A APIs remain unchanged.

## One implementation grant

NEW `src-tauri/src/drive_auth/commands.rs` (real wrappers) and `lifecycle.rs` (production-used coordinator/drain runner); existing `drive_auth/mod.rs`, `lib.rs` manage/invoke/run regions, `capabilities/default.json`, `tauri.conf.json`; NEW `capabilities/mobile-process.json`; narrow caller migrations in `src/components/panel/SettingsPanel.tsx` and `src/hooks/useExternalEditSession.ts`. Inline lifecycle/wrapper tests and NEW `src/__tests__/applicationLifecycle.test.mjs` routing tests belong to this same package. No Cargo/helper changes or preparatory stub.

Manage one desktop state containing `Arc<DriveApplication>` and a coordinator before `.build`/setup (lib:1890–1962,2333,2377,2508). Construct with EMPTY native records, task-free; NotConfigured remains authoritative. No registration loader, fallback or client/project input. Coordinator stores only bounded intent/fault/state, no AppHandle/task handle; one draining task may temporarily own handles/Arcs without a reverse ownership edge. No lock spans await, shutdown, entropy or OS calls.

## Fixed intent/action contract

Native intents: `Quit(i32)` or `Restart`. Normalize normal `ExitRequested(None)` to Quit(0); preserve first accepted Some(code), including headless0/1. Reserve `RESTART_EXIT_CODE=i32::MAX` exclusively for the final native restart. New renderer exit accepts only0/1. States: Open, Draining(intent,fence,queued), Final(action,fault), Blocked(staticFault).

| Current/input | Required transition/action |
|---|---|
| Open + Quit/Restart | Store intent, synchronously mark closing, start exactly one drain |
| Draining(Restart) + first normal quit | Replace with Quit(code); no second task |
| Draining(Quit(q)) + any request | Keep q; restart cannot supersede it |
| Draining + repeated Restart | Keep intent/task |
| CleanupComplete(optional fault) | Record fence/fault, queue exactly one main-thread finalizer |
| CleanupUnproven, runner panic/start failure or scheduling failure | Blocked; retain closing, no exit/restart/retry loop |
| Finalizer + complete Draining | Under mutex seal Final(current intent); release lock, perform action once |
| Final/Blocked + later request | AlreadyFinalizing/Unavailable; no new drain/action |

**Linearization is the finalizer’s mutex transition**, not cleanup completion or scheduling. Normal quit may override restart until that transition, including while the closure is queued. After it, requests cannot change action. The accepted first normal code wins: Restart→Quit(1)→Quit(0) yields1; Quit(None)→Restart yields0. No promise that an OS event not yet delivered counts as an accepted request.

Call `run_on_main_thread` only after the fence; its Result failure becomes Blocked. A queued closure that never executes keeps admission closed without claiming completion. Spawn/runner failure must record a static failure, never issue fallback exit; at most one runner is attempted. Do not use nonexistent try_spawn or abort a started native worker. Native status retains `open/draining/finalizing/blocked`, intent and static fault; no credentials or raw runtime errors. Faults are closed codes: cleanupUnproven, cleanupFault(A-code), spawnFailed, runnerFailed, scheduleFailed, poisoned. Never return native error strings.

## Actual Tauri ordering and final exit

Tauri2.12.1 app:633 sets restart_on_exit before requesting the sentinel; app:1449 invokes our Exit callback **before** cleanup/restart. Therefore remove desktop hard exits from BOTH lib:2545–2560 arms. FinalRestart calls only `request_restart()`, never `restart()`. FinalQuit calls `exit(q)`. In recursive ExitRequested, allow only the sealed action’s expected code (sentinel or Some(q)); prevent other normal requests, including None, without restarting draining. Exit callback returns to Tauri; it must not hard-exit or re-drain.

No replacement post-fence hard exit is needed: runtime-wry:4300 selects ExitWithCode; pinned Tao0.37.1 desktop run methods call process::exit(returned_code). Tauri then gets its cleanup/restart opportunity. `exit`/`request_restart` have internal fallback termination on runtime-send failure (:581/:633); they are invoked only after the proven fence. Their OS success cannot be acknowledged reliably. An unexpected Exit/foreign native sentinel is outside preventable cleanup guarantees; no false safety claim. Preserve malformed-headless exit2 before ownership (lib:1895), SIGABRT handler and forced/runtime-failure exclusions. Keep mobile exit arms unchanged.

## Close the permission bypass; preserve mobile

**Additional concrete blocker:** process plugin exit accepts arbitrary i32; `exit(2147483647)` bypasses prevent_exit even without restart_on_exit (plugin-process commands:8–10; Tauri app:90). Removing only restart permission is insufficient.

Remove process:default from common capability; grant it only in NEW `mobile-process` capability, windows main, platforms `["android","iOS"]`, explicitly selected in tauri.conf’s capability list. Desktop receives neither plugin exit nor restart. No changes to desktop-cloud or unrelated permissions. Add guarded `app_exit`/`app_restart` commands; migrate external-edit exit0 and SettingsPanel relaunch together. Existing useOsPlatform supplies routing: macos/windows/linux use native commands; android/ios keep existing plugin calls; unknown fails closed, never plugin fallback.

Guard modules, managed state, every generate_handler entry and desktop run arms with identical windows/macos/linux cfg. Installed macros2.7.1 and Android-pinned2.6.3 preserve entry attributes (handler:177); utils2.10.1/2.9.3 filter capabilities by platform. Android2.11.5 sources are present; its relevant parser/restart behavior was inspected. This supports the design, not Android compilation. CI uses scripts/android-build.py’s frozen2.11.5 stack; no lock change/download now.

## Wrappers and gates

Every new wrapper requires injected WebviewWindow label main, never a renderer window identifier; Tauri’s request-origin ACL gate rejects remote content before invocation (webview:2007,2080). Keep remote capabilities absent; no partial AppManifest. Forward A’s exact raw-string requests/static responses and explicit cancel. `app_restart` takes {schemaVersion:1}; `app_exit` adds code:0|1. Reject sentinel, negative/>1, nonintegral/string/bool code, duplicate/unknown fields and trailing JSON before closing or scheduling. Both return version1 outcome accepted/alreadyPending/quitPending; after commitment return AlreadyFinalizing, on blocked state Unavailable. Wrapper errors are closed InvalidRequest/ForbiddenWindow/Unavailable/AlreadyFinalizing or an A static code. Lifecycle requests are strict4KiB strings, response≤4KiB. No URL/token/event/native-error logging. JS promise abandonment does not cancel native work (ipc:329); lifecycle acknowledgment means accepted intent, not completed termination.

Twelve fixed acceptance groups: (1) intent/code precedence; (2) queued-versus-sealed races; (3) Complete/fault versus Unproven; (4) spawn/scheduling/poison failure and once-only resources; (5) actual facade held-ticket shutdown fence; (6) empty owner/strict serialization; (7) main/non-main/remote IPC; (8) sentinel rejection and desktop plugin denial; (9) desktop cfg plus pinned Android capability/compile checks; (10) desktop/mobile/unknown caller routing; (11) headless0/1/None, pre-owner2 and recursive events; (12) actual Tauri quit/restart ordering and codes. Pure reducers/mocks do not prove event-loop or OS behavior. Runtime/browser/provider/device/full OAuth remain Not exercised; no configuration, storage, refresh, leases, transfer, sync or release. Freeze for root decision, then separately granted source and independent validation.
