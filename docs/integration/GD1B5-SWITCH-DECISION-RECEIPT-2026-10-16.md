# GD1b5-S3C4 — exact native switch consent, source-only receipt

2026-10-16; author **gpt-6-astra**, Team slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`; task `01a141be-e98d-7333-b1b8-4d14d2c395ac`. This is author delivery, not independent source acceptance. Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-switch-decision`, detached base `47a1a46b2fec95826ffe5e71e39b4b5889211919`.

**Source candidate frozen for root review. Nine new grouped test definitions; ZERO tests executed.** Standalone pinned formatting and custody checks pass. Compiler, runtime, strict Clippy, independent migration and canonical integration are Not exercised for these bytes. Historical C3 461 passes/14 ignored/fmt/strict are baseline evidence only.

## Exact ownership and authority

Only existing `src-tauri/src/drive_auth/session.rs` and this new receipt changed in the writer checkout. All 674 other existing input files, independent children, original S1 author block, footer registrations, HEAD/index/git pointer and tracked inventory are preserved. No helper, module/export, library, README, Cargo/dependency/lock, application or canonical checkout edits.

Authority: private root SOURCE-GRANT.md SHA `a098bc38c274eca5b2a76101ce316d9bfb2f1b0e952269bbe8866c130e1cfc07`; ROOT-REVIEW.md `24e8bc50122c6236b67c43c361851b2fecf12de75b3f9932f25908bbba3a1a46`; root-pre-source-verification.json `8b34c10e9931a63056f8c91ca68d44c74f7144a06bdb5a3aa4bd3985e2ebed98`. Root input manifest `cafcd8d6dad84d81b9c440c885435a522281d17d5840229f3a0eda549843db8f`, accepted readiness `e22809109a358ca4555676ef908afee2de42eefd1bc36f1ced822dfd754c910f`, preimplementation 12-group known answers `b8a85edb07b4bb5915cfc19631642fd1ee1d3160157d809bb1d98934e33f5f4f` govern this package. Their semantics were not derived from implementation output.

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| Original session.rs | 160247 | `09170305d46b708910b493d9997bca3d7ebf9b7f8d43d3f3d6c42f1cc2008253` |
| Delivered session.rs | 209371 | `525be1112c4273988beee762458a603ef8094d05a38cc0d06e9721b65af041ce` |
| Complete session.patch | 58546 | `6bc96c9880faa389cc75febad7f8d45666498004b47433c961b3b1ff6c6a6dfb` |

Private evidence is `/private/tmp/rapidraw-gd1b5-switch-decision-implementation-20261016/`: original source, full before/after records, exact unified patch/replay checks, format argv/logs, source preservation checks, environment and sealed delivery/manifest. Source-writing scripts and the unformatted test draft are intermediate authoring records, not substitute deliverables; the hash above identifies final source.

## Implemented source behavior

`session.rs:220` adds native `SwitchDecision::{Accept,Decline}`, `SwitchDisposition::{AdoptedSessionOnly,Declined,Stale,AlreadyResolved}` and bounded personal `SwitchProposalSnapshot`. No Debug/Clone/Display/serde for that snapshot or secret owners, and no Debug added to Key. Snapshot fields carry attempt/generation/decision UUID, candidate/incumbent AccountKeys, registration ID, actual grants, baseline epoch and deadline; no token, URL, email or proof export.

At :244 one Slot-owned SwitchRecord retains one decision UUID and optional whole sealed candidate with incumbent stamp, epoch, proposal time and deadline. Consuming it preserves only the marker until Slot reap. No decision history, new owner cycle, mutex, watcher or proposal task. Public `switch_proposal` (:552) and `decide_switch` (:579) use the existing mutex. Random decision/connection UUID creation stays outside it.

Completion (:1056) retains C3 binding checks, and a different identity/project now publishes nonterminal AwaitingSwitch instead of terminal SwitchRequired. Confirmation is exactly `min(original admission, immutable token expiry, checked proposal+60s)`. Existing connected retention remains 3600s. Equality/backwards proposal time fails closed; reads/replays cannot renew. The incumbent remains separately owned, including its identity stamp after expiry. First/same-account adoption and raw pending behavior remain.

Decision (:1249) first observes existing fault/closure and expiry. Wrong tuples are inert Stale; exact consumed tuple is AlreadyResolved while its original Slot remains, including after cancellation changes live generation; after reap it is Stale. An exact unresolved but stale key/epoch/mapping terminates BindingMismatch and returns Stale. No account-key-only approval exists.

Accept requires the same current configured Slot, Running verification/AwaitingSwitch, completed authorization, Running watcher, expected/proposal/live epoch agreement, unchanged incumbent stamp and retained registration mapping, admission/proposal/token time and candidate.matches_native_attempt. `switch_binding` (:1186) refreshes the clock after reconstructing the context under the same lock. Checked epoch increment precedes moving the exact candidate; acceptance assigns a new connection UUID and `min(token expiry, checked acceptance+3600s)`. Actual grants replace, never union. Decline drops only proposal and records Declined. No new credential accessor or account/helper mutation.

`verification_in_flight` (:1605) restricts provider completion/failure to VerifyingAccount: a late duplicate cannot terminalize AwaitingSwitch. `terminate` clears proposal on every exit but keeps its marker. Existing auth and verification channels remain once-only. `expire_verification` (:1639) now covers proposal time/usability; snapshots, decision and owned wait invoke it. Connected expiry remains separate.

`continue_configured` (:2143) subscribes to the existing changed channel before publication. Its returned AwaitingSwitch disposition is awaited by `await_switch` (:2186), inside the original supervisor. That loop checks exact Slot/decision/deadline/binding, observes stop/expiry/change and has no lock across await. The actual supervisor, blocking worker join, browser-ticket acknowledgment and watcher finalization code are unchanged. Only after resolution can the normal continuation return and original cleanup/reaping occur. Global fault/poison/overflow/close clears all retained secrets; ordinary attempt cancellation preserves a valid incumbent. No hard-abort, zeroization or guaranteed runtime-shutdown scheduling claim.

## Defined regressions and migration

Original inline definitions: 44 = S1 21 + C2 12 + C3 11. New definitions: **9**, total inline **53**, executed **0**. No count is a runtime result.

| New definition/group | Fixed behavior challenged |
|---|---|
| exact_consent_is_nonterminal_until_accept_and_replaces_actual_subset | Publication, bounded metadata/Busy, unchanged incumbent/auth/previous verification, wrong tuple components, actual partial replacement and acceptance lifetime, one-shot/reaped replay |
| decline_cancel_and_expiry_preserve_incumbent_and_consumed_marker | Decline/cancel/expiry outcomes, incumbent/authorization preservation, marker after generation change and reap, once-terminal |
| exact_deadline_minimum_no_observation_renewal_and_backwards_clock | Admission/token/60s minimum, one-nanosecond-before/equality, repeated observations, backward proposal clock |
| expired_stamp_still_requires_exact_consent_and_late_completion_is_inert | Expired incumbent identity/epoch, real second sealed completion/error cannot destroy proposal, exact acceptance |
| exact_binding_corruption_cannot_retarget_a_proposal | Epoch, generation, project, selection and requested-set mismatch; inert first result and consumed replay |
| accept_cancel_disconnect_and_close_have_serialized_winners | Both deterministic orders plus actual thread-barrier accept/cancel race; explicit disconnect/close clears regardless of ordering |
| faults_erase_both_owners_without_wrapping_or_reopening | Both counters, mutex poison, actual scheduled watcher unwind; raw owner/proposal/fault inspection |
| original_continuation_waits_for_decision_or_scheduled_expiry | Actual shared verifier/continuation, Busy until decision, short real expiry, raw-State observation before lazy getters, single watcher |
| drop_and_unwind_discard_proposal_before_actual_watcher_finalization | Drop before/after accept, supervisor error-handler reducer, actual watcher cleanup gate and finalization/reference-cycle boundary |

These definitions use the existing restricted account HTTP bridge and bounded joined synthetic Peer. No proof factory, production endpoint override or forged Running state. Pure future-clock/reducer tests are explicitly distinct from real timers/watcher/continuation definitions. Reducer reservations manually reap only fixtures that started no listener/supervisor. **They do not establish a full positive browser/code-exchange/account/supervisor chain.** Existing real worker/ticket cleanup definitions are unchanged; the new unwind assertion invokes the real terminal handler but does not claim to panic the full OAuth chain. Physical/provider/runtime proof remains separate.

Only original C3 inline `different_identity_and_project_require_switch_even_after_expiry` (:4176) migrates: its two temporary SwitchRequired expectations become AwaitingSwitch, incumbent/stamp checks while pending, exact Decline, then cleanup. All original identity/project/expiry/disconnect assertions remain; project stamp/epoch checks are strengthened. The S1 block SHA remains `439fb48f47cdd16d990d653b2242c0ea3cccefd9103ee47d6fee600cf6786012`; all C2 bytes and all other original C3 inline test/helper bytes are exact after reversing this one migration and removing the new nested test module.

**Expected temporary test-source incompatibility:** protected `connected_lifecycle_independent_tests.rs:538` still names removed SwitchRequired. It must be separately migrated after root source review. No dummy variant is retained to hide this; no compile attempt occurred. Protected C3 child remains `b671ea103c7756967f6e9be1b3fa67fb015b24574c60a01922b0a729065d1af0`, C2 child `3175092139d499ad7a2dd01a9f8430502e84cfc4ffd3a520f0cfa3a330825ea4`, S1 child `7129423f99b35f724aa82d4ad301bb5708660983a7ba49cabcf0b1084ad2f82b`. Independent acceptance is not authored here.

## Checks actually performed and next gate

Pinned binary `/Users/harrisontucker/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt`, reported `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Exact format argv: binary `--edition 2024 --config skip_children=true <absolute-owned-session.rs>`; check adds `--check`. Format/check exits 0, logs and timestamps preserved. No rustc/Cargo/Clippy/test or dependency action. Python exact patch replay reproduces final source, added-line whitespace check is clean, and preserved test/child/footer blocks are compared byte-for-byte.

Before/after source custody rehashed all 675 baseline records: exactly session.rs differs, other 674 identical. Direct Git-index parsing confirms the original 675 tracked paths; HEAD/index/pointer bytes remain exact, with this one new untracked receipt making 676 product paths. No Git process/mutation, cache acquisition, build, socket/network/browser/provider/account/device/Android activity occurred. A read-only relative-path rg lookup initially used the wrong cwd and was repeated with the explicit writer cwd; it changed nothing and is not a validation gate.

**Pass:** standalone formatting, exact patch replay, owned scope/protected bytes/index/HEAD/inventory checks. **Fail:** none observed in those executed checks. **Not exercised:** typechecking/compilation, any tests, Clippy, independent migration/security review, runtime/provider/full-chain/product acceptance and canonical import.

Next consumer: root reviews frozen production and definitions; a separate owner migrates the protected C3 child and adds independent probes. Only a separate runtime/cache grant may run `cargo test --locked --offline --lib drive_auth::session -- --test-threads=1`, fmt/strict all-target Clippy and root's combined canonical gate. Stop at the first concrete failure; no source-author repair or downstream work is implied. Real registration/config, browser/IPC/UI, store/refresh/revoke, credential lease, Android, photos/queues/transfers/sync, main/push/deploy remain excluded. No credentials requested. Source is frozen for that review.
