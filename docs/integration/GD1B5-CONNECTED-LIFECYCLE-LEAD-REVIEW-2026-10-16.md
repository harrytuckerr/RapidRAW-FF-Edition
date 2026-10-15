# C3 lead production review

16 October 2026; gpt-6.1-sol. No blocking static finding in Astra's source-only
candidate based on accepted C2 `52fe885d8b97fe7e2d0e4f6edad5d8e91befd4f0`.
This permits bounded independent test migration, not compiler/runtime, canonical
or provider acceptance.

The lead read the full receipt and production diff, actual ownership/startup/
completion/expiry/finalization code, the complete owned C2 migration diff and all
eleven final lifecycle definitions. Intermediate authoring scaffolds were not
treated as the final source. All 21 private artifact records and both repository
deliveries were rehashed. All 661 original input paths were verified; only
session.rs changed, the other 660 remain exact, and inventory adds only the
receipt. Applying the unified patch independently to the full baseline reproduces
all 160,015 delivered bytes. The entire 31,205-byte S1 test region compares equal;
both child files and footer registrations remain unchanged.

Candidate source: `64e46d875ec8c29219e92072af6a13be5b9b8105fcd5decd9f0dbdd2db4841f9`.
Receipt: `1786dcf8107b1e64cb3d7b75048eee366cf0ea5f2c86a1156c2947966f0e0d3d`.
Private manifest: `9f432f949eb1797dfcacb667ea7b63c8d338ca02c54686e1578325a0cbd56235`.

## Static conclusions and outstanding proof

One state mutex separates attempt generation from connection epoch. Adoption
checks the current configured attempt, native mapping, original deadline,
candidate binding/usability, expected epoch and watcher Running state. First/same
account moves the whole sealed candidate outside Slot; actual grants replace.
Different identity/project discards the incoming result with SwitchRequired.
Expiry retains the identity stamp, preventing expiry from bypassing account
switch policy. Failed reconnect and Slot reaping preserve a valid incumbent.

The production retention constant is 3600 seconds, capped by token expiry with
checked arithmetic. Post-adoption checks use credential/local/adoption time,
rather than the original authorization deadline. Shared lock entry enforces
expiry. Explicit disconnect clears identity and credentials; poison, overflow or
watcher failure close admission and destroy secret ownership. Public snapshots
have no secret access or personal-data serialization/Debug.

The constructor remains task-free. A configured supervisor starts one watcher
before binding, outside the mutex and inside existing unwind ownership. Ordinary
attempt cancellation is separate from watcher failure. The armed guard exists
before spawn; the loop future ends before Finishing/Stopped publication. Shutdown
also checks watcher finalization and existing Slot/worker/ticket fences. No lock
spans an await and no SessionController owner is captured. These are static
conclusions; scheduled startup, cancellation, unwind, unpolled drop, old-timer
replacement and cleanup-barrier behavior still require independent execution.

The eleven new test definitions distinguish pure clock/reducer checks from
actual scheduled watcher and public-resource cases. Their success candidates
come from the actual restricted account HTTP verifier; synthetic post-exchange
admission is not full OAuth/provider proof. No tests or sockets were run here.

## Explicit independent migration boundary

C3 intentionally changes configured success from VerifiedNotAdopted to retained
AdoptedSessionOnly. The protected C2 independent child still references the old
variant and bypasses watcher startup. Its known incompatibility is a planned
test-integration boundary, not a passing build or permission to weaken the new
watcher predicate.

The next owner may migrate only that child's positive outcome and real-watcher
fixture setup, with a line-by-line reason table and unchanged negative binding,
cancel/deadline/stale/history/resource assertions. Preserve its eight definitions
and historical C2 file/receipt copies. No fake Running, dummy discard mode,
suppressed assertion or source-author edit. The lead reviews the frozen migration
before separately authorizing new independent lifecycle probes and execution.
All existing C3 production bytes and other review inputs remain immutable.

Canonical remains clean at 52fe885d. Compilation, lint, runtime, real account/
provider mapping, C4, configuration activation, browser/IPC/storage/Android/editor/
Drive/sync and product acceptance are Not exercised by this review.
