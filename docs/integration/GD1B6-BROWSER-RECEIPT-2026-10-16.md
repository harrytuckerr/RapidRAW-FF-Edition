# GD1b6 — owned native browser handoff, source-only delivery

2026-10-16; actual author **gpt-6-astra**, Team slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`; task `01a14202-511f-7af2-9f25-5f327bbd6184`. Isolated writer `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-browser`, detached base `6932d49ba6f989baf5b9ca2ae450b4846fec6512`.

**Source candidate only. 14 inline test definitions; ZERO tests, synthetic processes, sockets or OS/browser launches executed.** Standalone pinned rustfmt/check, patch reconstruction and protected-input custody pass. No module registration, compilation, Clippy, runtime or provider acceptance is claimed. Author delivery is not independent review.

## Ownership, dependencies and exact artifacts

Only NEW `src-tauri/src/drive_auth/browser.rs` and this NEW receipt are product writes. All 690 prepared inputs remain exact, including root's Cargo.toml/open dependency and Cargo.lock edge, session/helpers/children/mod/lib/README, source contract and lead review. HEAD/index/git pointer and original 688 tracked paths are unchanged. Root's two new contract/review documents account for 690 baseline product paths; these two author files produce 692.

Authority: full tracked `GD1B6-BROWSER-CONTRACT-2026-10-16.md` and `GD1B6-BROWSER-READINESS-LEAD-REVIEW-2026-10-16.md`, with `/private/tmp/rapidraw-gd1b6-browser-root-20261016/BROWSER-AUTHOR-INPUTS.json` 149518 bytes, SHA `c1039509e35699b7f180d9197641102e269c7888f3ee3b266bfda9683cf6ff97`. Before/after evidence hashes every baseline input and directly parses/compares the Git v2 index; no Git process or mutation.

Root prepared desktop `open = { version = "=5.4.1", default-features = false }` and the existing root lock edge. Root's locked offline metadata/feature-tree Pass establishes graph preparation only. The inherited default/shellexecute-on-windows union is unchanged, with no insecure feature. The author neither ran Cargo nor changed dependencies. Previous C4 479Pass/14ignored/fmt/strict evidence does not validate this unregistered module.

| Deliverable | Bytes | SHA-256 |
|---|---:|---|
| browser.rs | 40752 | `7d93caff619473a8cd694fbf1782739cdecf60ebd7b3d64322e78edf51d8cb32` |
| Complete new-source browser.patch | 41885 | `15e84657d747567d1304d152f24c397e13df709d27ab2d011339c85263c028f1` |

Private evidence: `/private/tmp/rapidraw-gd1b6-browser-implementation-20261016/`. It contains complete pre/post input records, product inventory, exact source patch/reconstruction checks, format argv/logs, environment, delivery and artifact manifest. Final receipt/delivery hashes are external to avoid self-hash cycles.

## Native API and selected command

At `browser.rs:53`, `pub async fn hand_off(BrowserLaunchTicket) -> Result<LaunchOutcome, LaunchError>` is the only public function. LaunchOutcome has only HandedOff, meaning selected OS launcher exit/reap success, not browser load, consent, account identity or Google login. Errors are static InvalidDestination/LaunchFailed/Cancelled/WorkerFailure or existing token-free TicketError. No public URL, command/program, arguments, browser choice, AppHandle, test callback, credential/proof or transport override exists. Owners have no Debug/Clone/serde; static errors/outcomes may derive Debug/Copy/Eq.

`first_default_command` (:99) calls only pinned `open::commands(original).into_iter().next()`. Remaining recipes are dropped without execution. There is no retry, fallback, `with_command`, deprecated shell open or detached/background opener. Selected command receives the original validated bytes; stdin/stdout/stderr become null before spawn. Production code does not format/log the URL, Command, native error or process output. OS argv/environment necessarily receives the URL; no OS-diagnostic invisibility promise is made.

Pinned recipe behavior is inherited: macOS /usr/bin/open, Linux default xdg-open or WSL fixed PowerShell/env recipe, Windows fixed PowerShell or Wine winebrowser. Missing selected launcher fails rather than falling back. Default associations, PATH and OS environment are platform assumptions, not validated browser identity. Android/iOS, application setup, IPC/capabilities and browser invocation are outside this module grant.

## Strict destination validation

`validate_destination` (:287) bounds input to 8192 ASCII bytes, rejects controls and requires the exact raw Google HTTPS endpoint prefix, matching parsed origin/path, no userinfo/password/fragment/explicit-port alias and unchanged parsed serialization. Exactly seven keys in the existing generator's order are required, with exact unescaped key names. Extra, missing, duplicate or percent-escaped key aliases are rejected before acceptance.

`strict_form_decode` (:345) validates each percent triplet and UTF-8 with `String::from_utf8`, never lossy query decoding. Fields enforce existing installed-client grammar/256-byte bound, exact canonical nonzero-u16 127.0.0.1 callback URI, response_type code, unique nonempty approved appdata/file subset preserving order, canonical unpadded 32-byte base64url state/challenge and S256. A fresh serializer reconstructs the generated request shape and must exactly equal the original URL. Lowercase/needless percent escapes, raw-space/%20 aliases and malformed UTF-8 cannot be rewritten into validity. The original bytes, not the reconstructed URL, go to the selected command. Requested scopes remain requests, never grants or provider proof.

The seven-field parser is bounded by overall size and exact key count before additional field allocation. Base64 decoding is limited to 43-character inputs. Temporary Strings are not claimed to be zeroized.

## Ownership, Drop and acknowledgment ordering

`hand_off_with` (:65) creates Pending/Cancelled/Completing atomic state and CancelOnDrop, then moves the ticket into one Tauri blocking closure before awaiting its JoinHandle. An unpolled public future owns/drops its ticket without executing anything. Once the worker is started, caller-future Drop only requests cancellation. It does not abort the worker or destroy its ticket.

`LaunchGuard` (:108) is installed before command construction/spawn. The returned Child is immediately stored in it without an intervening callback/panic hook. Command construction resources are dropped before polling. `reap` polls try_wait every25ms; a successful exit observation reaps and destroys the child handle before any acknowledgment. A wait error is sticky failure, **not cleanup proof**: retain ownership and retry observation, even indefinitely. No child/browser kill or forced detach occurs.

On unwind, LaunchGuard::drop (:185) performs that same cleanup before dropping the ticket. Its ticket and child are owned together on the blocking worker, never by the caller future alone. If cleanup cannot be proven, the one controller Slot remains fenced/Draining and shutdown may remain pending. A child fixture's finite iteration limit is not a guarantee about OS scheduling or a stuck native syscall.

`launch_and_reap` (:228) checks ticket currency before validation, again immediately before dispatch and again before positive acknowledgment. It checks caller cancellation before dispatch and after reaping. Success needs child successful exit, no prior wait error, winning the Pending→Completing CAS (:280), then existing consuming ticket.finish(HandedOff), whose controller mutex checks currency/deadline again. Caller Drop winning first suppresses success; completion winning first is final against caller Drop. Explicit controller cancellation remains authoritative at its own commit lock. Spawn/nonzero/wait failures call finish(Failed) only after cleanup; the static primary error is preserved if that failure acknowledgment is already stale. Unwind uses the ticket's existing failure-on-Drop semantics.

Existing 5s startup/10s handoff/300s admission constants are untouched; the adapter creates no new transaction deadline. A cancellation can race OS launch after the final predispatch check; a late external window cannot be undone. Late acknowledgment cannot resurrect the old transaction. The session's existing worker/ticket/watcher cleanup code is unchanged. No lock crosses an await in this new module.

## Test definitions, not results

Fourteen definitions are present: six platform-independent validation/command-selection/atomic tests, one public unpolled-ticket test, and seven Unix process/socket definitions. None ran. Production process selection has no injection; only private cfg(test) LaunchPlan::Synthetic supplies a boxed fixed test Command and controlled signals/errors. Those definitions still use the same handoff/worker/guard and actual public session/ticket APIs. No session private fields, fabricated ticket/candidate, fake successful output or successful synthetic Code callback exists.

| Definitions | Intended evidence layer |
|---|---|
| generated_subsets_scope_order_and_exact_encoding_are_accepted | Fixed appdata/file/both orders, canonical redirect/client/base64; published RFC7636 challenge fixture |
| destination_aliases_duplicates_unknowns_and_lossy_encoding_fail | Origin/path/port/userinfo aliases; extra/decoded duplicate keys; malformed percent/UTF-8/control/oversize and canonical-encoding failures |
| field_bounds_and_canonical_base64_do_not_admit_variants | Exact client/port/scope/base64 boundaries and noncanonical alternatives |
| selected_recipe_is_exactly_first_and_never_a_program_override | Command-builder shape only, no OS launch; explicit macOS default recipe checks |
| private_synthetic_plan_carries_original_bytes_without_dispatch | Private fixture input preserves original bytes; no dispatch |
| caller_drop_and_completion_have_one_atomic_winner | Actual production CAS versus actual CancelOnDrop with thread barriers, no parallel toy reducer |
| dropping_unpolled_public_future_does_not_dispatch | Public API future Drop; real session listener cleanup when later run, no browser command |
| real_selected_child_success_nonzero_and_spawn_error_are_distinct | Real fixed Unix child exit/nonzero/missing executable, null streams, exact native URL capture without printing it |
| stale_and_real_handoff_expiry_prevent_any_dispatch | Public cancellation and actual existing10s handoff timer reject before spawn |
| caller_drop_before_dispatch_releases_failure_without_starting_child | Held dispatch gate, cancelled caller, no child |
| dropped_caller_and_controller_cancel_keep_actual_child_shutdown_fence | Started held child, caller Drop/control cancellation, Busy and actual shutdown acknowledgment fence |
| panic_and_unknown_wait_keep_ticket_until_real_child_reap | Started-worker unwind and injected unknown wait retain fence; actual child reap precedes release |
| actual_handoff_deadline_does_not_release_a_started_child | Real10s timeout with held child remains Draining/Busy, late completion stays inert |
| earlier_denial_callback_waits_for_real_launcher_acknowledgment | Real localhost denial can arrive first, but no authorization terminal before actual child acknowledgment; no Code exchange/provider endpoint |

Unix fixtures use a fixed /bin/sh child with release file and bounded240/600 iterations of25ms sleep; teardown releases control gates/errors. They are reaped by the actual worker guard, including panic paths. The two real handoff-expiry definitions intentionally use the existing10s budget rather than a protected clock accessor; later scoped runtime therefore includes roughly20s of timer waiting. Synthetic launcher results do not prove default-browser launch. Windows/Wine/WSL process/runtime behavior and actual desktop browser invocation remain separate gates; the Unix process cases do not establish those platforms. Token/provider callbacks are not exercised beyond a synthetic access_denied response.

## Executed checks and stop

Pinned standalone executable `/Users/harrisontucker/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt` reported `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Format argv: `--edition 2024 --config skip_children=true <absolute-new-browser.rs>`; check adds `--check`. Recorded format/check exits are0. No rustc/Cargo/Clippy invocation or build/cache lease occurred. Python patch reconstruction exactly reproduces source; added-line whitespace and sole-public-function checks pass. Full 690 input hashes, tracked inventory, HEAD/index/pointer and root-prepared dependencies match before/after.

**Pass:** these standalone format/source/custody checks only. **Fail:** none observed in the executed checks. **Not exercised:** compilation, 14 test definitions, runtime/process/socket/browser/provider/full-chain/platform/security/product acceptance, independent review and canonical import. No dependency resolution/download/Git/device/OS browser action occurred. New module is deliberately unregistered.

Next: root static review/freeze, then separate isolated module registration and independent probes/runtime grant, with explicit synthetic child/socket authority and exclusive build resources. Future validator may run focused `cargo test --locked --offline --lib drive_auth::browser -- --test-threads=1`, format and strict all-target Clippy, followed by root's combined canonical gate. Stop at any concrete failure; repairs need a bounded grant. No app/IPC/UI/mobile/config/credential-store/refresh/lease/transfers/sync/release activity or credentials request is implied. Source stays frozen for that review.
