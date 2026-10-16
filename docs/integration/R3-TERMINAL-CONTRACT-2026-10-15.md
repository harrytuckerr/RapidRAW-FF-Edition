# R3 terminal import contract — 15 October 2026

**Ready for separately assigned implementation; no implementation performed.** Replace false all-success feedback with a validated terminal result and protect it with per-run identity plus one active native batch. This amends only R3 and its ownership in the accepted `ANDROID-RAF-REPAIR-REVIEW-2026-10-15.md` (§§63–81; SHA256 `70c16a9879cdeb169160f738988dda527ece968bae1ca7879eb32436fa5d60b1`). R2a staging remains frozen. R2b/local-sidecar/deletion and URI logging changes remain separate.

Author: Codex CLI - Astra, actual `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Source authority: `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/lead` at **`05dd25e74256bf0318cf43e6462cfd778bb4de53`**. Actual report checkout `raf-review` is **`4c1e965a53de9c835df920b2e3d69c2c8a864708`**, not the brief's `8a4f02bc`; no checkout change made. All source references below are to the lead base. Report is the only writable artifact.

## Confirmed defects and boundaries

| Evidence | Reproducer / consequence | Minimal correction |
|---|---|---|
| P1 `src-tauri/src/file_management.rs:4085–4096`; `src/hooks/useTauriListeners.ts:195–212` | Any failed item emits `import-error`, then unconditional unit `import-complete`; consumer overwrites Error with Success, even when every item failed. Zero native inputs also complete successfully. | Count actual item results; require positive, consistent terminal counts; failed batches finish Error. |
| P2 `src/hooks/useFileOperations.ts:302–313` | A second overlapping invocation rejects after A has started; its catch writes global Error over A. A delayed reject from a retired invocation can overwrite a newer run. | Reserve a run synchronously before invoke; gate catch by the exact still-active token. Reject overlapping local requests without mutating the incumbent. |
| P2 `useTauriListeners.ts:179–212,395–405` | Untagged late start/error/complete from A can alter B; `isEffectActive` only fences an effect generation, not a batch. `listen` registration is asynchronous. A fast import can emit before its completion listener is installed. | UUID on every import event; shared active-run coordinator; readiness only after all four listener promises succeed; stale generation/run events inert. |
| `useProcessStore.ts:108–127` | Error/Success installs a five-second reset. Treating each item error as terminal can reset a still-running batch to Idle. | Item errors keep Importing; only terminal outcomes set Error/Success. Active identity lives separately from transient display status. |

`import_files:3940–4100` spawns blocking work and returns `Ok(())` immediately: invoke resolution means **queued**, never imported. Existing local copy/sidecar/delete result semantics (`3981–4082`) are retained: an Err may follow a copied image, and some optional sidecar errors are ignored today. R3 counts these existing `Result` outcomes; it does not certify transaction completeness or original preservation. R2a's successful publication with cleanup warning remains an imported item, not a fabricated failure.

## Decision: one native batch, client-created identity, small frontend coordinator

Use a process-wide native single-active guard **and** fresh client UUIDs. Serialization protects native resource/work overlap; UUID matching protects delayed, duplicate and foreign-window events/catches. Neither alone does both. No event sequence counter, durable journal, cancellation API, store schema change or general task framework is needed.

Add `src/utils/imageImport.ts` for import DTO validation and a module-scoped coordinator shared by the two hooks. Keep existing `ImportState`, `Status`, process-store setters and MainLibrary rendering. Use installed `uuid` for fresh IDs; never reuse a token. Coordinator state is bounded: listener generation/readiness, active `{runId,total,listenerGeneration,maxCurrent,maxFailed}` or null. Retire by clearing active; a new UUID makes a historical-ID collection unnecessary. No per-item error collection in the frontend.

### Wire DTOs (camelCase, all fields required)

```ts
// runId: canonical UUID string generated once by startImportFiles.
// Count: JS Number.isSafeInteger(x), 0 <= x <= Number.MAX_SAFE_INTEGER.
type ImportStart = { runId: string; total: number };
type ImportProgress = { runId: string; total: number; current: number; path: string };
type ImportItemError = { runId: string; total: number; failed: number; errorMessage: string };
type ImportComplete = {
  runId: string;
  total: number;
  imported: number;
  failed: number;
  errorMessage: string;
};
```

Event names stay `import-start`, `import-progress`, `import-error`, `import-complete`. `import_files` gains required `run_id: String` (frontend invoke argument `runId`); existing sourcePaths/destinationFolder/settings arguments and `Result<(),String>` acknowledgement stay. Typed Rust Serialize payloads use `#[serde(rename_all = "camelCase")]`. No legacy unit/string completion or untagged event may be interpreted as success.

**Completion validity:** plain non-null object (not array); canonical matching runId; safe integer counts; `total > 0`; `total == active.total`; `0 <= imported,failed <= total`; `imported + failed == total`; `failed >= active.maxFailed`; string errorMessage of at most 1,024 Unicode scalar values. `failed==0` requires `errorMessage==''`; `failed>0` requires a nonblank errorMessage. No numeric coercion/defaulting, NaN/Infinity/fractions/negative values, count repair or success inferred from progress. Extra fields may be ignored. Only this validated terminal result with failed==0 produces Success.

**Other events:** start total must equal the reserved total and be positive; start never resets an already observed run. Progress counts must be safe and `0 <= current <= total == active.total`; path is a string of at most 256 Unicode scalars. Apply only nondecreasing current. Item error total must match; failed must be a positive safe count <= total; message nonblank and <=1,024 scalars. Retain `maxFailed = max(maxFailed,failed)` so duplicate/out-of-order errors cannot inflate counts; show its message in store while keeping Importing. Progress/start never clear an observed error or regress current. Intermediate errors cause no toast flood; completion supplies the single authoritative bounded summary.

**Malformed messages:** callbacks first require current listener generation and an active run. A well-formed foreign/nonmatching ID is ignored. Missing/unusable ID cannot be attributed: ignore, never change status to Success. A matching run with malformed event fields fails closed once: retire it, set Error with “Import result could not be verified; some files may have been imported. Refresh the folder before retrying.”, toast once and refresh once. Do not invent result counts or stop the native worker. The native gate prevents a retry from overlapping surviving work. Losing/unattributable messages may leave Importing; this contract promises no false success, not reliable transport recovery.

### Native producer steps

1. Validate nonempty source list, nonblank destination, canonical runId, and count representable as a safe JS integer before any event/work. Reject invalid request with Err and no batch events. Attempt a static `AtomicBool` compare_exchange gate; busy => Err, no events or writes. A small owning RAII guard releases on every return/unwind; move it into the blocking worker and keep it through the terminal emission attempt. Use standard Acquire/Release (or SeqCst) correctly; do not rely on frontend gating or a mutex held across await. A test-local atomic may exercise the same acquisition helper without cross-test global interference.
2. In the accepted worker, emit typed start once. Run the **existing** item operation once for each source. Record one imported increment for Ok, one failed increment for Err; count by outcomes, not filenames/destination existence/progress. Never early-return the batch on an ordinary item error. Retain the R2a read/validate/stage/publish implementation byte-for-byte; do not revisit optional sidecar, delete or desktop behavior.
3. Progress.current means completed attempts (0 initially, i+1 after each outcome), including failures. Emit a typed item error after a failed outcome with cumulative failed count. A skipped intermediate event does not change totals. Names in new progress/error display fields are bounded basenames (or `Item N` when unavailable), not raw content URIs.
4. Keep only the first three bounded useful failure details, each <=256 Unicode scalars; keep exact failed count independently. Reuse the URI display name already resolved by the item route, without a second provider read; if resolution failed, use `Item N`. For new user-visible detail, collapse control characters, substitute the exact source string with its safe label, redact any remaining `content://` URI token, and truncate at character boundaries. Never attach grants or raw payload objects. Retain actionable reasons (mismatch/empty/read/publish/collision). Existing diagnostic logging is out of scope. Summary <=1,024 scalars: `1 of 2 imported; 1 failed: DSCF5389.RAF: <reason>`; if >3 failures, add `+N further failures`. All-success message is exactly empty. Bound each detail before concatenation, not after storing an unbounded list.
5. Emit complete once after all normal item outcomes, with total/imported/failed/errorMessage. This terminal object is self-contained; frontend must not need every progress/error callback to reconstruct it. Attempt emission even if an intermediate emit failed; report emit failures locally, never reclassify file outcomes based on event transport. Drop guard after terminal attempt. No guarantee of callback delivery/order, crash/panic completion, persistence across reload or native process durability is added. A process abort/lost terminal is not success.

The few outcome/summary/guard helpers must be used by the real command's worker, not a parallel test-only algorithm. A small batch-driver extraction for host tests is allowed; preserve the existing item operation and staging semantics. Native acceptance tests must include actual temporary local-file imports, not only injected successful results.

### Frontend admission, listener lifecycle and effects

1. Move only the four import listeners out of the existing undifferentiated listener list into an import registration group in `useTauriListeners`. Start a fresh coordinator listener generation, initially not ready. Each callback checks its generation and `isEffectActive`. Mark ready only after all four `listen` promises resolve successfully for this same generation. Registration failure leaves not ready; dispose successful registrations, including registrations that resolve after failure/unmount. Handle promise rejections without unhandled rejections. Do not alter the other event families.
2. **Choose immediate readiness rejection, not an implicit queue:** before calling native, `startImportFiles` synchronously checks the shared coordinator. If a run is active, toast “An import is already running” and return without changing its store state, clearing its timer, allocating a token or invoking native. If no listeners are ready, surface bounded retryable Error/toast and do not invoke. This avoids a hidden promise queue or arbitrary readiness timeout. Empty paths/blank destination are similarly explicit local errors when idle, never Success; they also must not overwrite an incumbent run.
3. On valid idle admission, generate UUID and reserve `{runId,total,generation}` **before any await/native invocation**. Immediately set Importing, empty error/path and progress0/total using the real store setter (clears an old terminal timer). Invoke existing command with runId. Its resolution does nothing to status; events may already have completed. Its catch may retire/write Error/toast **only if the exact token is still active under that generation**. Stale catch after terminal, rejection of an old run after B began, or duplicate callback does nothing. A native busy error for this newly reserved token retires only that attempt; foreign-run events never acquire local ownership.
4. Event admission never creates a run: only a local start request does. Therefore A's late start cannot resurrect A, duplicate completion cannot refresh/toast twice, and events from another window cannot hijack this window. A valid completion retires the token synchronously before store writes/refresh callbacks; set status Error if failed>0 else Success, progress total/total, path empty, errorMessage exactly the validated bounded summary. Error toast uses a run-specific toastId and is shown once. No async callback later writes import status without token validation.
5. Refresh folder trees and reselect the current folder once for **every accepted terminal**, including mixed/all-failed results; this preserves existing refresh behavior, successful mixed items, and visibility of possible existing local-route writes before an Err. Malformed matching terminal/event also refreshes conservatively once. Invoke refresh effects after committing status; catch sync/async refresh errors separately so they cannot undo terminal accounting or re-enter the invocation catch. Do not skip the tree refresh because status is Error. No original/good-item rollback.
6. Cleanup invalidates the listener generation immediately, marks not ready and disposes late-resolving handles. If that generation still owns an active request, retire it as an **unknown result** Error with the verification message above, never success/cancelled; backend may still be running. New effect generation cannot revive it. An old cleanup must not invalidate a newer generation. Existing isEffectActive guards remain. React StrictMode setup/cleanup/setup is explicitly tested. A full webview reload loses this ephemeral state; no cross-reload reconciliation is promised.

Expose a small coordinator API for admission, event consumption, token-checked invocation failure and listener-generation lifecycle; put runtime validation there, not in duplicated hook branches. It can return effects to the hook adapters, but there must be one active identity shared between them. Do not use per-hook useRef identity or process-store status as the lock. Existing five-second status reset remains presentation only; a new Importing setter cancels it and the coordinator never admits a run based solely on Idle.

## Exact proposed ownership and implementation order

**Not authorized to edit by this design task. Lead grants next owner these five paths together; one writer, coordinated native cache window.** Backend/frontend wire changes must be integrated atomically (no old/new protocol mixture).

| Owned path | Bounded edits |
|---|---|
| `src-tauri/src/file_management.rs` | `import_files` admission/guard/worker accounting/event payloads; small real-driver/summary helpers and `r3_terminal_tests`. Do not change R2a helpers or unrelated local operations. |
| New `src/utils/imageImport.ts` | DTOs, runtime validation, bounded singleton coordinator; no React/component/store schema or dependency additions. |
| `src/hooks/useFileOperations.ts` | Only `startImportFiles` admission/runId/catch, using existing toast/store facilities. Both modal `handleStartImport:315–321` and Android direct picker call `399` already delegate here; preserve those entry points. This explicitly supersedes the earlier read-only ownership for this file. |
| `src/hooks/useTauriListeners.ts` | Only four import registrations, readiness/cleanup and guarded terminal effects; preserve other listener families and thumbnail batching. |
| New `src/__tests__/imageImport.test.mjs` | Installed node/esbuild/react-test-renderer harness, real hooks/coordinator/process store; mock only native event/invoke and unrelated UI/platform boundaries. |

Read-only: process/UI stores, ImportState/Status/MainLibrary, App.tsx, AppProperties Invokes enum, lib.rs command registration, existing profile tests, i18n, packages/locks, staging helpers, Android/provider, RAW/DCP/render code. No `test:imports` script is needed: run the new file directly. Future worker receipt may be granted separately by lead. If implementation exposes a need outside these files, return exact evidence for reassignment.

Suggested serial work: (A) DTO/driver/guard host regressions and coordinator transitions; (B) wire both actual hooks plus transport-mock regressions; (C) independent source review, then lead's combined checks. Do not claim A alone fixes the UI or B's mocked transport proves Android. Stop at frozen reviewed patch/receipt for integration.

## Required acceptance tests and commands (future implementation)

| Test | Required observations |
|---|---|
| Native single failure, mixed batch, all good | Real batch driver emits exactly one start/complete and counts each outcome once. Use actual missing source plus real temporary image copy, delete=false, not only outcome mocks. Mixed valid file remains byte-identical, no Success on failure. Assert event order/payload shape and bounded named reasons. |
| Native empty/busy/malformed ID | No item calls, no start/terminal events, Err; active guard survives until held worker finishes and releases on normal/error/unwind paths. Two concurrent acquisitions admit exactly one. R2a focused regression remains unchanged. |
| Actual hooks: failure/mixed/all good | Feed native-shaped payloads through registered callbacks, not direct store assignments. Assert final store status, total/current, actionable toast, refresh calls once; mixed preserves Error; positive all-good alone yields Success. Invoke resolution alone stays Importing. |
| Empty/malformed terminals | 0/0/0, null, unit/string, missing ID, wrong ID, counts mismatch, total different from reservation, NaN/Infinity/fraction/negative, nonstring/oversize/blank error, failed0 with message, failed below prior cumulative error. No Success; matching malformed retires Error, unidentifiable/foreign ignored. |
| Overlap and stale rejection | A pending; B requested => no second invoke and A unchanged. Native busy rejection tested separately. A completes; B admitted; deferred rejection of A cannot change B. A terminal before its invoke resolves/rejects stays terminal. Exercise hooks from two mounted callers sharing coordinator. |
| Repeated/stale events | A fails then B succeeds; duplicate terminal refresh/toast once; late A start/progress/error/terminal and decreasing progress cannot alter B. Late progress after B terminal inert. Item Error during a long run remains Importing beyond five seconds; final Error timer does not reset the next active run. |
| Registration and cleanup | Four deferred listen promises; pre-ready request never invokes. Reject one registration, resolve others late: all created handles disposed, no unhandled promises, no admission. Unmount before/after readiness and StrictMode remount: old callbacks/late registration/cleanup inert; current generation preserved; active lost result never Success. |
| Terminal effects | Refresh callback throws/rejects without changing counts/status or producing duplicate terminal toast; latest callback refs used. All-failed terminal still refreshes possible local outputs. Error message bounded with >3 failures, Unicode, long names/control chars/content-URI text; no grant URI in new display payload. |

Use the existing `src/__tests__/profileCompatibility.test.mjs` pattern: esbuild `write:false`, actual React hook mount/update/unmount in `act`, actual Zustand store; controlled deferred invoke/listen promises and fake timers; transport dispatch helpers should throw if no listener was registered, rather than silently bypassing readiness. Assert native call arguments include the new UUID. Do not mock the coordinator/store/entire successful import route. Count test functions separately from cases/fixtures.

After lead provisions the implementation checkout with **existing installed dependencies** and assigns the cache window:

```sh
# From assigned frontend checkout; no npm install/ci, package/lock edits or new resolution.
node --test src/__tests__/imageImport.test.mjs
npm run test:profiles
npm run typecheck
node node_modules/eslint/bin/eslint.js src/utils/imageImport.ts src/hooks/useFileOperations.ts src/hooks/useTauriListeners.ts src/__tests__/imageImport.test.mjs
# From its src-tauri, only in the assigned native cache window:
cargo test --offline --locked --lib r3_terminal_tests -- --nocapture
cargo test --offline --locked --lib file_management::tests -- --nocapture
# A filter matching zero tests is not acceptance. Record names/counts, ignored controls
# and baseline failures; private native controls require a separate explicit fixture grant.
git diff --check
```

The existing R2a tests are under `file_management::tests` at4544; fixture-gated ignored tests remain separate. Verify filters against the implementation tree before execution. Host Rust tests prove producer/helper bookkeeping and relevant local filesystem behavior. React tests prove actual hook/store behavior over mocked native transport. A separately rebuilt native app must still verify command argument serialization, real listener readiness/delivery, mixed-batch feedback and physical picker/provider behavior. No Android, original-source preservation, native RAF/fidelity or Adobe claim follows from this contract.

## Frozen source identity and review evidence

SHA-256 recomputed from `git show 05dd25e7:<path>` and compared byte-for-byte to lead working files (all matched):

| File | SHA-256 |
|---|---|
| `src-tauri/src/file_management.rs` | `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7` |
| `src/hooks/useTauriListeners.ts` | `55b05c8f37bf4d9ad7ec6b9db4f212d06dcbd77569dcb5d1cb63666d9c10e536` |
| `src/hooks/useFileOperations.ts` | `083eef881de18373017bca677fd2b04c7fa9a38a03002862bab99bb77cc2a206` |
| `src/store/useProcessStore.ts` | `7a6e0efbc29cfc23fcf6fff7aacbc9f65808d5334456552a9e2408a943ee3976` |
| `src/components/ui/ExportImportProperties.tsx` | `465c54cba802a6201df03aa2d0d316b58f52b9fafc71df3f9403defc41f39c54` |
| `src/__tests__/profileCompatibility.test.mjs` | `569336a4a131a285468e38072efe04b8892ce0385b174562222dd522619e9b5b` |
| `package.json` | `9796b348c9f7767854fa701b8696969e1ab06e2d52e58b4dd9fc3cade7447e08` |

Installed primary implementation read: `<HOME>/Documents/CoWork/RapidRaw/node_modules/@tauri-apps/api/event.js:71–83` registers via asynchronous invoke and resolves an unlisten callback. The lead snapshot has no node_modules; no install or symlink was created. Current package declares existing uuid/esbuild/react-test-renderer and `test:profiles`; no additional dependency needed. Source-only review does not establish native transport ordering, so the contract deliberately tolerates reorder/duplication and gates readiness.

**Pass — read-only source/identity/call-site analysis and completed contract. Fail — current frozen false-success and overlap behavior remain unfixed. Not exercised — new tests (not written), Rust/Node test execution, builds, shared cache, devices or native transport.** Commands actually executed: rg, cat/sed/nl/tail, read-only git rev-parse/status/show, shasum, and stdin Python hashlib/subprocess source identity comparison. No production source, lock, Git, cache or device mutation. Report hash is delivered in Team message. Remaining implementation choices are ordinary function names/test seams; no unresolved choice about protocol, success predicate or overlap policy remains. Lead must explicitly grant proposed new ownership and native test window.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `050005a661b88d7cec03252ae500970452b7cae599b409edc03507991aadab88` (23753 bytes).
