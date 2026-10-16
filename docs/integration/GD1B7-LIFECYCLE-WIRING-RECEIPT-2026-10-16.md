# GD1b7-B lifecycle wiring — source-only receipt

Actual model/author: **gpt-6-astra**, Team Astra, 2026-10-16. Task `01a14263-fb3a-7761-8cb9-8a8492cae202`; isolated `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-lifecycle-wiring`, detached **abf1ed796246cb952691d287451b843c61985c43**. Binding source grant SHA `c3b28d53b8b4ed0ff45a2bf6b5c8ade62ad5d15b6fdf3ffb23d47f686d0d923a`. This author wrote earlier A/session/browser components; no independent review or runtime acceptance is claimed here. Historical A canonical527Pass/14ignored is not attributed to this package.

## Frozen source and exact scope

| Owned source/test/config path | Bytes | SHA256 |
|---|---:|---|
| `src-tauri/src/drive_auth/lifecycle.rs` | 24357 | `095754c8856165f6159bbd608c01de8d470dcbdd8d61d503c423b4ba99c59131` |
| `src-tauri/src/drive_auth/commands.rs` | 15659 | `0290b6dc29d8cadfac0acfc547684bd2892158848c95be5dfcfe715111659b22` |
| `src-tauri/src/drive_auth/mod.rs` | 1177 | `9c0e1ae19f5f12d384f07361b262a12d2aff6707442c005df3afd6f002a707ff` |
| `src-tauri/src/lib.rs` | 97609 | `7388e4c49a0bf023329954d4e000cc81c43a436383d7a8b690ab0b2503cf4855` |
| `src-tauri/capabilities/default.json` | 674 | `b48bcb5de0d668347ae417d6a3b3ea2cf1257914ad11b992838cfd81887f22dd` |
| `src-tauri/capabilities/mobile-process.json` | 257 | `6e668e7cabeabca20577d56dca8f40f15fc10bb3da13012626043433f1ba6ba7` |
| `src-tauri/tauri.conf.json` | 6235 | `7521aa460abce6f983ad9b3e9fa8350645c5c30524ad9509af6427ce304419c4` |
| `src/components/panel/SettingsPanel.tsx` | 121501 | `28870295b1cefbebe9edb9cfee73cc3753aeea3e53111bdd5cf392f151202c20` |
| `src/hooks/useExternalEditSession.ts` | 4048 | `c4b05f71eb175e24d869feabc46fd127e7dd3e489182cb39a539a300701efd3f` |
| `src/__tests__/applicationLifecycle.test.mjs` | 4723 | `b9822921b8ac1e59e07586e369016199a7b0380790844e1e1f9748915cb7d3fc` |

This new receipt is the eleventh owned output. `source.patch` covers the ten source/test/config paths, **54752B SHA `40a17026ffb8c7627c4bda474f41b3a799c51dd42b36e34a9208dd5aa2c6239b`**. The final `owned-files.patch` additionally contains this receipt. `delivery.json`/`manifest.json` seal all exact hashes without embedding a receipt self-hash.

Full 716-input baseline was verified before edits against `/private/tmp/rapidraw-gd1b7-application-root-20261016/APPLICATION-B-SOURCE-INPUTS.json` (155979B, SHA `d4ff9c09091f78951ed076dcc083fe7e17ecef9ab65d253847c68b7955232615`). Final custody retains all716 before/after hash records: **710 other inputs unchanged**, only six authorised existing paths modified; five authorised new paths bring exact inventory716→721. Detached HEAD/index/pointer and713 indexed paths remain unchanged. No ignored outputs existed in this snapshot. Original helper/test/independent-child/Cargo/package/lock bytes are preserved.

`source-checks.json` and `patch-operations.json` prove forward and reverse replay for all ten code/config/test paths. `region-replacements.json` plus six private baseline copies reconstruct each ENTIRE original existing file. lib.rs changes only desktop state management,12 handler entries and cfg-separated exit hooks; original mobile arm bodies/pre-owner exit2/headless0/1 remain exact. Frontend edits only the granted routing/import/hook/dependency/error regions. Common capability loses only process:default; tauri.conf changes only the selected capability identifier list. Pinned rustfmt sorted the newly inserted lifecycle module export after exchange; removing the two new exports and restoring the description reconstructs exact original mod.rs. No unrelated formatting change was retained.

## Actual implementation

One task-free `DesktopState::empty` owns Arc<DriveApplication> and a bounded coordinator; lib manages it before build/setup only on windows/macos/linux. Native records remain EMPTY. There is no config loader, fallback, client/project/endpoint input, new provider scope or account activation. Existing A methods are unchanged.

`commands.rs` registers nine exact drive_auth forwards plus app_exit/app_restart/app_lifecycle_status. Every wrapper checks the injected WebviewWindow label before work; no caller label is accepted. A responses/errors are forwarded without rewriting. Lifecycle requests are strict object JSON strings <=4096 bytes, schemaVersion1, unknown/decoded-duplicate/trailing/type failures rejected; app_exit accepts integral0/1 only. Sentinel i32::MAX and all other values fail before closure/action. Static errors serialize as bounded codes, including the unchanged A code. Status/ack JSON has a4096-byte output bound. No URL/token/event/native-error logging or private controller escape was added. Framework request-origin enforcement remains Tauri2.12.1 webview.rs:2007/2080 with no remote capability or partial AppManifest; direct guard tests are NOT actual IPC evidence.

Lifecycle uses its production coordinator, runner and finalizer, not a parallel test reducer. First request stores intent and marks A closing synchronously; one runner is attempted. First normal quit replaces Restart while Draining, including after cleanup while queued; an accepted Quit code survives further quit/restart requests. Native None becomes0, ordinary native i32 codes remain intact, reserved sentinel stays separate. A RunnerGuard is created BEFORE constructing/spawning its future: failed/unpolled startup, polled Drop, cleanup panic or scheduling failure becomes blocked, without fallback action. The runtime owns this one runner; no handle collection/abort interface or owner↔task cycle exists.

Only authoritative CleanupComplete queues the finalizer; closure and cleanup faults are retained separately from failure cause. CleanupUnproven blocks. Finalizer seals latest intent under the mutex, releases it, then calls exit(q) or request_restart once. Neither locks nor callbacks span shutdown await, OS action or scheduling. A never-run queued closure remains Draining/closed. A started OS worker has no hard abort/deadline promise. Recursive ExitRequested permits only the exact sealed Some(code); other normal requests are prevented without restarting cleanup. Foreign native sentinel is recorded as an exceptional boundary because Tauri cannot prevent it. Desktop Exit returns to Tauri; no desktop hard exit/restart() call remains in the hooks. Mobile arms, abnormal SIGABRT and pre-owner malformed-headless exit2 remain unchanged.

Both desktop process-plugin grants are removed: keeping allow-exit would permit exit(i32::MAX) past prevent_exit. New mobile-process grants the original process:default only to main on android/iOS and is selected in config. SettingsPanel routes macos/windows/linux to app_restart, android/ios to relaunch, unknown nowhere. External-edit success similarly routes desktop to app_exit code0, mobile to exit(0), unknown/rejected routes reset isFinishing without raw logging/unhandled rejection or plugin fallback. Existing save/export/local-edit operations outside those call regions remain byte-identical.

## Defined tests and evidence layers

**14 Rust groups (12 synchronous,2 tokio) +4 functional frontend groups; ZERO executed.** Exact names are in source-checks.json. Rust definitions cover intent precedence/native codes, queued-versus-sealed action order/replay, Complete fault provenance versus Unproven, preconstructed guard failed/unpolled/polled Drop, panic/scheduling/poison failure, foreign sentinel/recursive codes, real empty A cleanup/no owner cycle, public-session held-ticket cleanup fencing, strict lifecycle parsing/no owner mutation, all nine A forwarding routes/main-window guards/static errors/status and capability policy JSON.

The held-ticket definition uses the existing PUBLIC SessionController/NativeStart boundary and the real shared drain runner. A deliberately exposes no held-ticket accessor; it is NOT labelled a held-ticket DriveApplication integration test, and no helper/sealed proof was fabricated. Empty A shutdown is exercised by its own definition. Synthetic action/queue closures never call AppHandle exit/restart; queue-order/counter fixtures are not OS/event-loop proof. Real local listener definitions use only held/failed tickets and no Code-success/provider flow. Guards may continue native cooperative cleanup after outer future Drop; no abort/hard-release claim.

Frontend definitions extract and transpile the actual named production callback bodies with existing esbuild, then call them with mocked native/settings functions. Their19 platform/failure rows cover five supported platforms, unknown platform, rejected save/invoke/plugin, exact raw payloads and no fallback/unhandled rejection. They are **functional callback routing**, not a full React component/hook render, mounted UI, actual Tauri IPC or physical device test. No node/esbuild/test process ran during authoring.

## Checks and next gate

- **Pass:** pinned standalone rustfmt write/check for the four owned Rust files, with edition2024/skip_children=true; version `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Exact argv/exit0/stdout/stderr retained. Syntax formatting is not compilation.
- **Pass:** full input/metadata/index/inventory custody, exact outside-region reconstruction, forward/reverse source patch replay and changed-line whitespace checks. No Git diff command or metadata write.
- **Not exercised:** Cargo/rustc/Clippy/build, all Rust/frontend tests, app IPC/window/event-loop quit/restart, default browser/Google/provider, config/account/device/mobile/Android build, storage/refresh/revocation/leases/transfers/sync/release. No process query, runtime/socket/browser execution, dependency install/download or shared cache access.

Pinned source anchors: Tauri app:90 sentinel prevention;581 exit fallback;633 request_restart flag/fallback;1449 Exit callback before cleanup/restart. Runtime-wry2.12.1:2597 RequestExit queue,2626 main-thread closure,4300 code decision. Tao0.37.1 desktop run_return then process exit. Macro2.7.1/Android2.6.3 handler:177 retains cfg; utils2.10.1/2.9.3 platform capability filters; Android build pins2.11.5. These are source evidence only; no alternate dependency fetched. Actual app/event-loop/mobile validation remains necessary to claim those layers.

Next consumer is root static review, then an explicitly assigned isolated compile/runtime owner. Suggested focused tests after that grant: cargo test --locked --offline --lib drive_auth::lifecycle; cargo test --locked --offline --lib drive_auth::commands; node --test src/__tests__/applicationLifecycle.test.mjs; required full-format/strict/regression/platform gates as independently assigned. Do not execute from this receipt. Artifacts are frozen in `/private/tmp/rapidraw-gd1b7-lifecycle-wiring-implementation-20261016`. Stop; no downstream authority.
