# C3 independent runtime validation — first failed gate preserved

**Fail: one focused run selected68 definitions;67 passed,1 failed,0 ignored,378 filtered out.** No rerun, repair, expectation change, cargo fmt-all or Clippy followed. Canonical integration is not accepted by this receipt.

Actual model `gpt-6.1-sol`; C3 author `gpt-6-astra`. Task `01a14113-e59e-7fc0-b40a-97599fe724fd`, 16 October 2026 Melbourne. Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-connected-lifecycle-review`, detached `52fe885d8b97fe7e2d0e4f6edad5d8e91befd4f0`. Owned only the new connected lifecycle child, this new report and private evidence. Runtime ORT read-copy and generated ignored schemas were the explicit build-artifact exceptions.

## Exact failure and attribution

`src-tauri/src/drive_auth/connected_lifecycle_independent_tests.rs:786`, in `independent_actual_lazy_startup_cancellation_failure_and_unpolled_guard`: a synthetic configured reservation sets existing `watcher_test.fail_start=true`, then awaits actual `ensure_watcher`.

- Frozen expected: `Err(AuthorizationOutcome::WorkerFailure)`.
- Observed: `Err(AuthorizationOutcome::Cancelled)`.
- Source/oracle remains exactly as frozen; no broadened predicate or repaired result is represented as passing.

Production `ensure_watcher` at session.rs1408–1445 maps lock faults to WorkerFailure but can return Cancelled for !live or its biased stop arm at1436. Actual watcher-start failure faults/closes State and signals Slot stop, so stop versus changed scheduling can expose Cancelled. This is an internal-helper outcome mismatch, not by itself a demonstrated product-safety defect. Root independently adjudicated that the private exact WorkerFailure result is not promised and likely overconstrains this probe; stored WatcherFailure/Closed state and actual cleanup are authoritative. Root indicated no production correction from this failure, but granted **no repair/rerun**. That classification does not relabel the failed67/1 gate.

The failed group completed its earlier real raw/startup-cancel/reuse and exact never-polled guard rows before the assertion. Its failed-start row's subsequent reaping/final shutdown assertions and all later120ms startup-timeout rows were **Not exercised** after786. The separate existing author startup-failure and startup-timeout definitions passed within this same run; they are not substitute execution of my unreached rows. `first-failure.txt`, immutable full log/binary/source, `failure.json` and separately attributed `root-adjudication.json` preserve this distinction. A separate bounded probe-correction grant is the next decision; no production change is inferred here.

## Defined versus executed coverage

Eight new grouped definitions were frozen before execution; all eight selected:7Pass/1Fail. All60 existing selected definitions passed (21S1author+8oldchild+12C2author+8migratedC2child+11C3author). Total is67Pass/1Fail, not previous historical subsets added again. Parameter rows are not counted as separate passes. Original migration child db61, old S1 child7129 and every inline expectation remain unchanged.

| New group | Observed function result | Evidence layer / limit |
| --- | --- | --- |
| Retained ownership/reconnect | Pass | actual HTTP, real mutex reducers and watcher; synthetic post-exchange admission |
| Identity/disconnect | Pass | actual HTTP + explicit future-clock reducer, not provider attestation |
| Exact clocks | Pass | direct clock/reducer comparisons, not one-hour scheduled test |
| Scheduled/raw separation | Pass | actual timers/raw observation; prior-timer capture not forced/observed; existing direct old-timer author assertion separate |
| Startup | Fail at internal startup-result assertion; later timeout rows Not exercised | actual spawned watcher/public raw listener + direct guarded-future Drop; synthetic Slot reaping only without worker |
| Watcher panic/Drop | Pass | actual task/cleanup gate; no hard-abort or zeroization claim |
| AND resource fences | Pass | actual public listener/supervisor/ticket/task; no manual reaping |
| Current/faults/orders | Pass | actual HTTP candidates with real same mutex reducers and watcher; direct synthetic fault/counter setup |

Candidates came from the actual restricted account HTTP verifier using fixed synthetic A/B input and the new child's owned stop-aware joined ephemeral localhost peer. Capture is capped16KiB, response body at1024B, worker budget4s and joined Drop; account calls retain their existing bounded transport. No author fixture/helper, proof constructor, output stub, fake Running or successful Code callback was used. Synthetic native binding/permission labels prove fixture custody only.

Scheduled replacement/expiry uses raw State before `connection_snapshot` or `Inner.lock` could itself expire credentials. The actual replacement remained after the previous deadline; the actual later watcher expiry removed its credentials while raw pending remained separate, with identity stamp and one idle/reused watcher. **The60ms sleep only offers scheduling opportunity; it does not prove the watcher captured the previous timer or force a live stale-timer wake.** No frozen source changed after root raised that precision note. Existing author session.rs3995 separately calls `expire_observed_connection` with the old timer at old.deadline+100s, asserting replacement UUID/status remain; that is direct reducer coverage, not scheduled stale-timer proof. Exact snippet and layer limits are private.

Other passing groups distinguish explicit future-clock boundary checks, real thread-barrier reducer races, actual watcher panic/unpolled/startup hooks, owner Drop/weak-reference cleanup and public listener/ticket resources. The watcher AND actual held ticket each fenced shutdown in both release orders, including after dropping the first shutdown future. Only synthetic Slots with no listener/supervisor were manually reaped; public resources used actual cleanup. No total-RAM, zeroization, hard-abort, full OAuth or provider-authentication claim follows.

## One command, pinned environment and artifact

Exactly once from `src-tauri`:

```text
cargo test --locked --offline --lib drive_auth::session -- --nocapture --test-threads=1
```

Exit101; 112.557947041s wall time;4.46s test phase. Started `2026-10-15T19:58:38.570217+00:00`, completed `2026-10-15T20:00:31.128360+00:00`. Preflight no competing compiler and 18503368704B free (>=6GiB). Rust1.98.1 `48a229cea`, Cargo1.98.1 `797e8a9bc`; no toolchain install. Only existing bounded synthetic localhost tests ran. No full/default/CLI/account/ignored extra invocation.

Exact environment: `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`, `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, both `CARGO_PROFILE_*_INCREMENTAL=false`, `CARGO_INCREMENTAL=0`, `CARGO_NET_OFFLINE=true`, `GIT_OPTIONAL_LOCKS=0`, `TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin`, `ORT_SKIP_DOWNLOAD=1`, `PYTHONDONTWRITEBYTECODE=1`, private `/private/tmp/rapidraw-gd1b5-connected-lifecycle-validation-20261016/tmp/`. Exact full names/argv/UTC/exits/durations, any cleared inherited variable names, raw and ANSI-normalized logs are in invocation/result JSON. No secret environment dump.

Ignored ORT33,481,272B SHA256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` was read/hash-copied from canonical into this review's ignored resources. Build log confirms valid runtime/download skipped; Cargo was offline. Four ignored generated schema files were disclosed, copied and hashed. The permitted shared target recompiled local dependencies; no dependency/lock refresh or new target cache was used.

One actual selected test binary preserved: `/private/tmp/rapidraw-gd1b5-connected-lifecycle-validation-20261016/binaries/rapidraw_lib-b8c68747df43d7c8`, 40772304B SHA256 `6593549248c7e904fdbd9f2ba8e02efb71963253bacbb009c99581157c5661ec`. Child39592B SHA256 `3f14d8bfd841ef24aa8a00b87e3284acb609a4e6b86367fae0adab5f191a79f3`; new-file patch40733B SHA256 `438a3ebdcdd40d0c2710c1d375c34beb5db68e4c4ed0fb623ad206831e36e0f5`. Frozen oracle/source hash predates the first run and remains exact. Historical other failures and baseline receipts are unchanged.

## Custody, status and stop

All665 root inputs and exact inventory matched manifest `d36549e38fb675d2eb724fb07d04caada5f6ff16835803631a93c0069c6258b7` before work and remain byte-identical afterward. Session160,125B `f80b82429cbf3a9b20fd7f88c44c3987fbe51452f966a27933368d17bbb833d9` retains the exact160,015B author prefix64e46d87 plus110B root footer. Both old independent children, catalog/account/module/README/locks and all historical reports remain intact. HEAD/index unchanged. Product inventory adds only the new child and this report (667 files); ORT+four ignored schemas are separately listed. Full pre/post665 manifests/inventories and frozen child are private.

**Pass:** compilation of frozen source/new probe;67 selected functions; pre-run owned rustfmt/check; frozen665-input/child/HEAD/index custody. **Fail:** the single focused runtime gate and one new startup group, unchanged as above. **Not exercised:** cargo fmt-all/strict Clippy after failure, later tail rows of the failed group, combined canonical suite/integration, production Google exchange/provider/account mapping/full-chain authentication, C4, real catalog/configuration, browser/IPC/secure store/refresh/credential lease, Android/device/UI/editor/Drive transfers/sync or product/security acceptance.

Cache **RELEASED** at `2026-10-15T20:01:36.684348+00:00`; no cargo/rustc/clippy-driver process, 18468745216B free. No more build commands. Private evidence `/private/tmp/rapidraw-gd1b5-connected-lifecycle-validation-20261016` includes machine receipt, exact logs/binary/source/patch/cases, fault context, pre/post custody and sealed artifact manifest. Final hashes are supplied in Team handoff to avoid self-referential sealing. Stop frozen for root review; no downstream authority or waiting.
