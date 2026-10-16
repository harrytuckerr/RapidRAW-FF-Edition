# C3 independent migration of the protected C2 tests

Source-only delivery by `gpt-6.1-sol` (live Team roster), task `01a14113-87f1-7aa0-81b9-cdc8a5e6cefe`, 16 October 2026 Melbourne. C3 production author is Astra; this independent migration changes only the protected child in `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-connected-lifecycle-review` at detached `52fe885d8b97fe7e2d0e4f6edad5d8e91befd4f0`.

**Eight existing tests DEFINED; ZERO executed.** Frozen for root migration review. No compiler, Cargo, Clippy, test, socket, runtime, provider, browser, device, dependency, Git mutation or build-cache operation occurred. No cache lease acquired. Static review did not identify a blocking production/API mismatch; this does not establish compilation or C3 acceptance.

Only `src-tauri/src/drive_auth/configured_verification_independent_tests.rs` and this new report are delivered. All other 662 original inputs remain byte-identical, including the entire 160,015-byte production session `64e46d875ec8c29219e92072af6a13be5b9b8105fcd5decd9f0dbdd2db4841f9`, old S1 child `7129423f`, account, catalog, module, README, locks and historical receipts. All 663 original paths/exact inventory matched root manifest `fbd49a08f3e70c924634c3383f37c172b5e4b46aaa7f84671efa18c4b8ff6748` before editing. Afterward only the owned child differs; this report adds the sole new path (664 total). Detached HEAD and index `c622426a7be4db8a648deb94a712be13bf4437687be37faae2bb217cec56f5d5` remain exact. Full before/after inventories and preserved original child/index are private.

## Actual migration and retained ownership

The reducer fixture now awaits the actual production `ensure_watcher` while the synthetic reservation is Starting, before offer/ack/exchange. Its 13 callers await it; the seven-row preflight-negative test becomes async mechanically. No watcher readiness/state is assigned, candidate injected or author fixture reused. Reducers still start no listener/supervisor or Code callback. Their candidates still come only from the unchanged restricted account fixture API and real synthetic HTTP transport when separately executed.

Four intentional positive references change `VerifiedNotAdopted` to `AdoptedSessionOnly`: A/File and B/AppData continuation rows, completion-first cancellation order, and completion-winner race arm. Every failure/cancellation alternative is unchanged. The renamed terminal helper preserves every old assertion and additionally distinguishes owner presence. Fixed successful metadata checks use project987, independent_A/File/missing AppData or independent_B/AppData/missing File; full-scope single File fixtures have no missing scopes. They also check first epoch1/non-nil connection UUID and public ConnectedSessionOnly metadata.

The sealed candidate remains in production State. The test copies only available token-free account/grant/missing-scope/credential-expiry metadata plus adoption/time/UUID/epoch/local deadline. Display name/email/quota stay absent for the fixed permissionId-only responses. Synthetic Slot reaping compares all those metadata fields and identity-stamp consistency before/after, including absence for cancellation/failure rows. It does not expose or clone credentials, reconstruct proof or claim to inspect inaccessible account internals. The existing fixtures begin without an incumbent; new incumbent/reconnect lifecycle probes are outside this migration grant.

Only synthetic Slots with no listener/supervisor are manually reaped after their verifier future ended. A new teardown closes Inner and waits on the actual watcher Stopped notification under a new1s cleanup bound; it never writes a lifecycle state or releases the task manually. It also covers the existing owner-Drop case through retained Inner. The fault test retains its original1s public shutdown and typed results. The entire public admission/ticket test remains byte-identical and retains actual supervisor/worker/ticket cleanup. No old timeout, deadline, race/fence assertion or outcome was weakened.

## Preservation beyond counts

`original-assertions.json` rehashes and pairs **all82 original assertion/panic macro texts**, unchanged byte-for-byte and in order, including binding/scope/cancel/deadline/stale/history/fault/public-resource checks. Their aggregate ledger SHA256 is `258982da070430f274a889f57b36ac6b22c68170ec2b827283184737fc651bf6`. `terminal-expectations.json` separately binds all eight terminal-call argument sets; only the two positive branch/match arms change variant, with every negative argument/control branch preserved. SHA256 `bedf0044b16a183b54fbd2a2a3f4e7f95757eacdaa2d2d43ce2b025bd42aa0c1`. Candidate macro count104 is reported only as inventory, not semantic proof.

`migration-operations.json` records all23 exact replacement/addition operations. Reversing them reconstructs all35,930 original bytes exactly. The formatted candidate is token-identical to that preformat source apart from whitespace and rustfmt's single trailing parameter comma. Thus non-macro case tables/control flow remain bound to the exact authorized edit record too. `preservation.json` SHA256 `70ba4ce79c2951707e5257c47e9b138b4404cdf1fc9cca1fcfedfa076e05d7a0` records this and nine entire unchanged function bodies, including public admission/ticket cleanup and the fixture/request producers. `changed-lines.json` contains every removed/added line and its reason; the table below maps every edit block.

## Line-by-line reason map

Line numbers refer to the original child and final migrated child. Insertions name their original insertion site. The large metadata/cleanup insertion is split further below.

| Edit | Original line(s) | Migrated line(s) | Exact reason |
| --- | --- | --- | --- |
| 1 | 1 | 1–2 | Document the deliberate C3 success-semantic migration and retain the synthetic-only boundary. |
| 2 | 51 | 52 | Make reducer setup async so it can await the real watcher. |
| 3 | 53 | 54 | Work must be mutable because production ensure_watcher observes its stop receiver. |
| 4 | insert before 57 | 58–60 | Await actual production watcher startup in Starting phase before offer/exchange; no readiness assignment. |
| 5 | 253 | 257–261 | Rename misleading nonretaining helper; rustfmt expands parameters and adds one trailing comma. |
| 6 | insert before 280 | 288–392 | Add success-versus-failure owner presence, exact token-free candidate/identity/timer metadata, reaping preservation and bounded real watcher finalization. Detailed helper ranges follow. |
| 7 | 496 | 609 | Frozen A/File successful row now expects intentional AdoptedSessionOnly. |
| 8 | 504 | 617 | Frozen B/AppData successful row now expects intentional AdoptedSessionOnly. |
| 9 | 525 | 638 | Mechanical await of real watcher setup. |
| 10 | 535 | 648–658 | Rename terminal helper and add fixed A/B project/identity/grant/missing-scope checks; typed failure rows retain absence. |
| 11 | 547 | 670 | Reap only synthetic Slot; compare all retained metadata before/after. |
| 12 | insert before 556 | 679 | Close and await actual watcher only after old history checks and synthetic Slot reaping. |
| 13 | 559–560 | 683–684 | Mechanical tokio async attribute for unchanged seven preflight-negative rows. |
| 14 | 563 | 687 | Mechanical await of real watcher setup. |
| 15 | insert before 622 | 746 | After unchanged preflight failures/reaping, close and await actual watcher. |
| 16 | 629 | 754 | Mechanical await of real watcher setup. |
| 17 | 655 | 780 | Helper rename only; BindingMismatch unchanged. |
| 18 | insert before 657 | 782 | After unchanged binding failure/reaping, close and await actual watcher. |
| 19 | 660 | 786 | Mechanical await of real watcher setup. |
| 20 | 686 | 812 | Helper rename only; Cancelled unchanged. |
| 21 | insert before 688 | 814 | After unchanged cancellation/reaping, close and await actual watcher. |
| 22 | 690 | 817 | Mechanical await of left real watcher setup. |
| 23 | 699 | 826 | Mechanical await of right real watcher setup. |
| 24 | 715 | 842 | Helper rename only; foreign candidate BindingMismatch unchanged. |
| 25 | insert before 719 | 846–847 | After both synthetic Slots reaped, independently await both actual watchers. |
| 26 | 725 | 854 | Mechanical await of real watcher setup. |
| 27 | 747 | 876 | Helper rename only; cancel-first condition/control flow unchanged. |
| 28 | 753 | 882 | Only completion-first success changes to AdoptedSessionOnly; Cancelled arm unchanged. |
| 29 | 757 | 886–890 | Completion-first A metadata remains exact across Slot reaping; cancel-first stays absent; await watcher afterward. |
| 30 | 760 | 893 | Mechanical await of real watcher setup. |
| 31 | 785 | 918 | Helper rename only; fixed race dispositions unchanged. |
| 32 | 790 | 923 | Only AlreadyTerminal completion-winner success changes to AdoptedSessionOnly; Cancelled/NotCurrent arms unchanged. |
| 33 | 794 | 927–931 | Completion-winner B metadata survives Slot reaping; cancellation winner stays absent; await watcher afterward. |
| 34 | 796 | 933 | Mechanical await of real watcher setup. |
| 35 | insert before 829 | 966 | Await real watcher only after unchanged stale-completion/replacement-history checks and reaping. |
| 36 | 835 | 973 | Mechanical await of real watcher setup. |
| 37 | 850 | 988 | Helper rename only; exact original/token deadline outcomes unchanged. |
| 38 | insert before 872 | 1010 | After unchanged deadline checks/reaping, await watcher. |
| 39 | 874 | 1013 | Mechanical await of real watcher setup. |
| 40 | 928 | 1067 | Helper rename only; snapshot TimedOut unchanged. |
| 41 | insert before 931 | 1070 | After unchanged no-request/snapshot-expiry/reaping checks, await watcher. |
| 42 | 937 | 1077 | Mechanical await of real watcher setup. |
| 43 | insert before 1000 | 1140 | After unchanged verifier cancel/close/Drop/deadline/fence assertions and synthetic Slot reaping, wait for actual watcher even after owner Drop. |
| 44 | 1007 | 1148 | Mechanical await only in fault test; its existing 1s public shutdown bound and typed results stay byte-exact. |

The added helper block is fully accounted for: new288–291 owner presence;294–306 token-free metadata record;308–336 original identity/metadata/display-absence capture;338–360 fixed A/B identity/grants/public metadata;362–371 exact preservation across synthetic Slot reaping;373–393 real close/Stopped wait. These are source definitions, not observations of execution.

## Checks, hashes and custody

Original child35,930B SHA256 `2fe0df0c8c0191581ff5632295e67a4b79705c548bbdf921883af4a1e32dee8c`.
Migrated child41495B SHA256 `db61ed54bbdde133b820a7f0d8ed671d593b6ed32aacc95dd1d19743b710c429`.
Unified patch16853B SHA256 `aec3a15947669f00754f5af4779dfaa1f4df2c8816d79b1500f5fdf8d53d4f13`.
The exact unified patch was independently replayed in memory against the entire original and reproduced the entire formatted candidate. Whitespace/final-newline checks Pass.

Pinned standalone formatter only: `<HOME>/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt --edition 2024 --config skip_children=true [owned child]`, then the same with `--check`; both exits0. Formatter version `rustfmt 1.9.0-stable (48a229ceae 2026-09-01)`. Environment `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`. Exact argv/UTC/durations/output are in `format-results.json`; no Rust compiler/toolchain installation was invoked. Python source/custody scripts and execution metadata are private. Two private authoring-check preconditions initially failed (helper occurrence count and formatter-added trailing comma); `authoring-notes.json` retains them. They were corrected in private authoring/custody code, without changing expectations or production bytes; they are not Rust test results.

Read the full root review `2c045545`, author receipt `1786dcf8`, brief `979af4ef`, root fixed cases `14036f43`, actual production completion/watcher/cleanup paths, protected child and relevant account/catalog APIs. Their exact dependency hashes/copies are private. The planning contracts do not override actual frozen APIs. Source inspection found the real watcher and retained metadata APIs sufficient for this bounded migration; compiler/runtime behavior is Not exercised.

Private evidence: `/private/tmp/rapidraw-gd1b5-connected-lifecycle-migration-20261016`. `receipt.json` records exact source/check/custody status; `artifact-hashes.json` seals all private files and both delivered paths. Before/after663-input inventories, exact Git metadata bytes, baseline/final child, patch, original negative expectations, operation/diff/reason records, formatter logs and source-only checks are retained. The final manifest/report hashes are supplied in the Team handoff rather than introducing a self-referential seal.

**Pass:** owned-file formatting/check, whitespace, exact patch replay, full original assertion/negative-terminal preservation, reversed-baseline proof, protected custody/inventory/HEAD/index.
**Fail:** no production failure established by this source-only task; private authoring-check errors are preserved as described.
**Not exercised:** Rust compilation/type checking, Cargo/tests/Clippy, actual watcher scheduling/finalization, socket/candidate/adoption behavior, new independent C3 lifecycle probes, canonical integration, Google/provider/account mapping/full OAuth chain, C4/configuration/IPC/browser/storage/Android/UI/Drive/sync/security completion. Existing historical C2 passes are preserved and do not validate changed C3 bytes. No cache lease was acquired; none is held or released by this task. Stop frozen for root migration review; later probes/runtime need a separate grant.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `89d16e7397f205f1397e4af205766dd12c90e0cf9fe85d5bc716cb0302a61ed0` (13717 bytes).
