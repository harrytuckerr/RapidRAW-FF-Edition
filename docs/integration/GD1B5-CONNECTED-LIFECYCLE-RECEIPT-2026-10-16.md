# GD1b5-S3C3 — connected ownership and complete lifecycle

Source-only delivery, 2026-10-16 Melbourne; actual model `gpt-6-astra`. Task `01a140f4-031a-7840-93b6-7ba7ae0a6b13`. Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-connected-lifecycle`, detached `52fe885d8b97fe7e2d0e4f6edad5d8e91befd4f0`.

**Frozen for root production review, separately authorized independent test migration, then runtime validation. ZERO tests executed.** This implements native first/same-account ownership; it does not activate Google sign-in, a real catalog, UI or Drive jobs. The protected historical C2 child still requires its explicitly planned migration before a coherent test build; no compile/runtime/Clippy acceptance is claimed.

## Scope and dependencies

Only existing `src-tauri/src/drive_auth/session.rs` and this new receipt changed. Private evidence: `/private/tmp/rapidraw-gd1b5-connected-lifecycle-implementation-20261016`. Account/catalog/model helpers, mod/lib, both independent children, README, Cargo/locks and every other input remain immutable. No Git mutation, cache/build/compiler/test/socket/runtime/network/provider/browser/device/dependency action occurred.

Read the full root grant, accepted C2 source/runtime/canonical reports, current actual APIs/tests, parent adoption/slicing contracts and reviews, readiness0104ee97, root review91a9d7c7 and frozen future cases14036f43. The canonical442Pass/0Fail/14ignored/fmt/strict evidence is baseline evidence only. Exact read-only dependencies are in dependencies.json. The initial guessed independent-child filename did not exist; the actual source footer located configured_verification_independent_tests.rs, which was read in full and preserved.

## Native ownership and atomic adoption

State now holds one ConnectedOwner containing the WHOLE sealed candidate, connection UUID/checked epoch, adoption time and retention deadline outside Slot. One token-free AccountKey/UUID identity stamp survives expiry. Slot captures expected connection epoch at admission, independent of attempt generation. No token getter, clone/parts API, credential lease, serializer or public proof constructor exists.

The configured completion commit rechecks the existing UUID/generation, once-terminal configured phase, immutable mapping/request, original admission deadline, candidate binding/usability and expected epoch under the same mutex. It requires actual watcher Running. UUID entropy is acquired outside the lock. Checked epoch advancement precedes ownership publication. First/same AccountKey replaces the entire old owner atomically; actual granted scopes replace prior capabilities, never union them. Different identity or project drops the incoming candidate with SwitchRequired, preserving incumbent/expired identity; no C4 candidate or confirmation is stored.

Root-frozen retention is `min(candidate.credential_expires_at(), checked adoption + 3600s)`. It never extends token expiry. Before adoption the original admission deadline remains mandatory; after adoption expiry uses credential usability, adoption-clock and local retention only. ConnectionSnapshot contains bounded native status/UUID/epoch/AccountKey/actual scopes, deliberately no Debug/Clone/Display/serde because it carries identity. It is not permission for remote jobs or proof of current remote validity.

Inner.lock performs the shared expiry reducer before normal observation/mutation; raw recovery/reaping also applies expiry. Expiry removes credentials but keeps the identity stamp and epoch. Timers carry UUID+epoch+deadline and only expire matching ownership. Reaping Slot cannot remove a connection. Failed/cancelled/stale/timed-out reconnects preserve a still-valid incumbent. Cancel-first rejects adoption; adoption-first leaves stale attempt cancellation inert. Explicit Disconnect/SwitchAccount invalidation clears identity/credentials and advances checked correlation counters; it supplies no reusable switch approval. Fault/poison/overflow clear secret ownership and close admission. Lifecycle changes preserve bounded historical authorization/verification terminal records.

Raw new/begin stays pending-only on configured or connected controllers, using unchanged C1 continuation. with_catalog remains task-free and runtime-independent. No application configuration or command registration was added.

## One lazy watcher and actual cleanup fence

The first configured supervisor starts the single watcher before listener binding, within the existing catch/unwind envelope and original5s startup/300s admission budgets. prepare_watcher reserves Starting under the shared mutex and arms a guard constructed before spawn; spawn occurs outside the lock. The actual task subscribes before publishing Running. No try_spawn API, polling success stub, fake Running or per-connection restart exists. Ordinary attempt cancellation ends that attempt's wait without closing a healthy watcher/controller/incumbent. A watcher still starting may complete to idle; its original startup deadline never renews.

The watcher remains one controller-lifetime task while idle. It holds Inner, never SessionController, and no task handle is stored back into Inner. The loop copies timer identity under lock, unlocks to await existing watch.changed/sleep_until, then rechecks under lock. No mutex crosses an await. The armed guard covers spawn failure and never-polled future destruction. Catch/unwind completes and drops the loop future and panic result before finalization begins.

Running readiness is distinct from Finishing/Stopped cleanup. Unexpected loop/startup failure closes admission and clears credentials before Finishing is exposed. The final guard publishes Stopped only after loop receiver/timer resources and any test cleanup gate have ended, with no later await. A normal finalization claim is accepted only on a closed controller; poison recovery is destruction-only. A finalizer with no matching startup ownership leaves a closed Finishing fence rather than advertise release. Dropping a JoinHandle is not used as completion evidence.

Close/owner Drop clears secrets synchronously and signals both work and lifecycle notifications. Shutdown waits outside the mutex for BOTH actual Slot/worker/ticket reaping and watcher finalization. Dropping shutdown does not remove cleanup ownership. Started blocking calls remain cooperative/non-abortable; executor teardown can prevent completion. No hard deadline or zeroization guarantee is asserted.

Production bounds: one attempt supervisor, one blocking listener and launcher ticket, at most one expiry task, one retained incumbent plus one in-flight reconnect result, one identity stamp and bounded terminal records. The existing watch channel is reused through a sender clone in State so even fault paths can wake lifecycle cleanup. No second state mutex or task framework.

## Intentional owned-test migration

The original21 S1 author definitions and old independent child7129423f remain byte-identical. Both independent child files and footer registrations are untouched, including protected C2 child2fe0df0c. Owned C2 changes are isolated in owned-c2-test-migration.diff:

| Before | After / reason |
| --- | --- |
| Synchronous reducer reserved helper, no watcher | Async helper awaits actual ensure_watcher readiness before entering exchange; callers await it. No readiness field is forged. |
| One synchronous extraction-negative test | Same negative assertions, async to use real watcher setup. |
| Positive VerifiedNotAdopted expectations | AdoptedSessionOnly, reflecting intentional C3 retention. The negative expiry exclusion targets the new success variant. |
| Positive test name ending discarded | Renamed retained; no assertion removed. |

All12 owned C2 definitions remain, now async. Existing assertion macro counts remain22 assert,34 assert_eq,1 assert_ne and3 panic; the explicit diff records exact changes rather than treating equal counts as a semantic proof. Negative binding/cancel/expiry/stale/history/resource expectations remain intact. The old VerifiedNotAdopted enum variant is removed; no unused compatibility discard mode was introduced.

**The immutable independent C2 child still references that removed variant and uses manual watcher-bypassing success setup.** Root must assign its independent success/real-watcher fixture migration after reviewing frozen production. This is an intentional incomplete test integration boundary, not green compilation or permission for the source author to edit that child. Historical C2 bytes/reports are preserved. Root's later migration must preserve all negative/fence assertions.

## Eleven new lifecycle definitions — not run

All use owned helpers/A's actual restricted synthetic HTTP verifier for successful candidates. No fake candidate or production endpoint override. Private hooks are cfg(test) only: startup/cleanup gates, internal failure/panic trigger, short retention for scheduled tests and an actual-start counter. Production retention remains the literal3600s policy. Gates have finite2s waits; peers retain bounded stop-aware I/O and joined Drop.

| Group | Intended evidence after separate execution |
| --- | --- |
| First/same/partial/failed reconnect | Whole owner survives reaping, actual subset replaces, raw-on-connected stays legacy, real verifier setup failure keeps incumbent, watcher starts once. |
| Different identity/project/expired stamp | SwitchRequired before/after expiry; project namespace differs; explicit disconnect clears stamp before a new first account. |
| Pure clock policy | Exact min(token,3600s), admission passes after adoption, exact expiry and invalid pre-adoption clock; terminal history unchanged. |
| Scheduled expiry/idle reuse/old timer | Actual short timer clears credentials, watcher remains idle/reusable, old timer cannot erase replacement. |
| Reconnect cancellation/epoch | Both lock orders and actual barrier race with incumbent; timed-out and wrong-epoch candidate cannot replace it. |
| Independent counters/poison | Connection epoch and attempt generation exhaustion plus mutex poison clear credentials/identity and finish cleanup. |
| Real startup cancellation | Gate before Running, no ticket/ownership, ordinary cancellation preserves healthy controller; real readiness and next begin reuse one task. |
| Startup failure/unpolled future | Internal start failure and destruction of the exact never-polled guarded future fail closed without forged readiness. |
| Startup timeout fence | Short original startup budget expires while actual gate is owned; shutdown cannot complete until that resource ends. |
| Shutdown watcher + ticket | Actual watcher cleanup gate and real launcher ticket independently fence shutdown until both end. |
| Watcher unwind/owner Drop | Failure clears retained credentials before cleanup gate releases; owner Drop clears secrets, joins lifecycle via notifications, and has no retained owner cycle. |

Pure future-Instant comparisons/manual reducer reaping are distinguished from actual scheduled watcher/listener cases. Reducer fixtures do not own listeners and are reaped manually only in that setting. No positive Code handoff, live Google, one-hour sleep, provider mapping claim or full OAuth chain is added. These are definitions only: new11 executed0; existing tests rerun0.

## Exact custody and checks

Baseline session.rs112,515B SHA256 `553251e93ec39d735305683b890cb0ee2de45fb6817c4516ecc7188db702731c` includes root's preserved independent footer. Candidate160,015B SHA256 `64e46d875ec8c29219e92072af6a13be5b9b8105fcd5decd9f0dbdd2db4841f9`. Unified patch64,510B SHA256 `7b708f91487bb1b3c051351905501ff8c7951e812f2aab078200869e61888b39`.

All661 root inputs were verified before editing from manifest748285258e9d109534f29a06ca9ced8b4c05e86a6bf00be61f8c4b9a2f19dd16. Only session.rs differs; all other660 files and exact tracked inventory/HEAD/index remain unchanged. S1 module and both child footers compare byte-for-byte. supervise, bind_worker, join_worker, receive_ack, continue_pending, retain_pending, terminate, verification_current/owned and expire_pending retain exact function bytes; their surrounding lifecycle paths intentionally gain the C3 checks above.

The generated patch was replayed independently in memory against the whole baseline and reproduced the complete candidate. Source-only whitespace checks pass. Pinned rustfmt1.9.0-stable (48a229ceae), RUSTUP_TOOLCHAIN=1.98/RUSTUP_AUTO_INSTALL=0, formatted and checked only owned session.rs with `--edition 2024 --config skip_children=true`; exits0. checks.json `78e95751cbddcdf3ae1885169a992f6dbc630518c1345a1de957d5d528b17f49`; preservation.json `d84548f6f5a5e4902737da5809b532c26a878e8ef7a812ac72404d73cf8cd33d`. Full manifests/patches and exact final source are private. Authoring scripts/lifecycle_tests.rs are intermediate scaffolds; authoritative definitions are in frozen-session.rs, including the final connected-raw/race/timeout additions.

Pass: formatting/whitespace, patch replay, protected custody and static preservation checks. Not exercised: compiler/type checking, Cargo/tests/Clippy, watcher/socket/runtime behavior, independent test migration, canonical acceptance, provider/account/browser/UI/IPC/storage/refresh/Android/device/editor/transfer/sync. No cache lease was acquired. Existing442 baseline passes are not changed-source evidence.

Next consumer: root production review, then separately authorized independent child migration and C3 fixed-case validation, then focused/combined canonical checks under a new cache grant. No source-author repairs after freeze without a separate grant. C4 confirmation, real configuration, credential leases, persistence and application activation remain excluded. Stop frozen; no downstream work or credential request.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `1786dcf8107b1e64cb3d7b75048eee366cf0ea5f2c86a1156c2947966f0e0d3d` (13850 bytes).
