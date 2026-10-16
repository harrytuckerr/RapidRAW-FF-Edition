# GD1b7-B lifecycle fault/admission repair — source only

Author/model **gpt-6-astra**, 2026-10-16; task `01a142bc-f60b-77b1-8a14-84e931f92685`. Isolated `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-lifecycle-poison-repair`, detached `abf1ed796246cb952691d287451b843c61985c43`. Original F1 remains **Fail: static fault-injection invariant**, not a runtime/compiler failure, ordinary production panic or provider exploit. This source repair has no independent or runtime acceptance yet.

## Exact correction

Only `src-tauri/src/drive_auth/lifecycle.rs` changed. Revised **31712B SHA `50ebdaa19b7c817e15c2b8f077dea3b24478eab269d8137f8c3ada3278523a14`**; original24447B SHA `6f361a47acfe4fbf1d31af13547b3b0e302dca581cee4902df7396f30fbbbcab`. `lifecycle.patch`: **9243B SHA `1e1fd07b9a534ab1383edf80467c39675dc23950de7f9b51f3e33a51bf2d55ab`**. Exact patch forward/reverse and outside-region reconstruction restore every original byte, including the original author test module and root90-byte footer.

`DesktopState::snapshot` now copies coordinator state, releases its mutex, and reconciles an observed Blocked/fault state via existing synchronous idempotent A.mark_closing before publishing it. Static closure-fault provenance is retained. It does not recursively call itself, poll for future poison, spawn work, manufacture a cleanup fence or change A's admission/commit implementation.

New PRIVATE production-used `prepare_request` performs actual coordinator admission and then the same owner observation. A fatal observation returns Unavailable only after closing A, before public request can construct a runner. Reserved native Quit(i32::MAX) remains validation-before-observation: its generic Unavailable does not imply a coordinator fault or close a clean Open owner. Protected command JSON/window/sentinel validation remains unchanged.

New PRIVATE production-used `prepare_exit_response` handles the real native hook's observation, closing A on fatal state before returning Prevent for None/ordinary codes. Foreign reserved code retains its unpreventable-runtime distinction. Public request/native hook call these preparation methods. Final Exit observation also uses the owner snapshot; existing abnormal close paths now retain their static closure fault. All reconciliation calls A without a coordinator lock held. This is observation-time closure only; faults not yet observed and existing already-admitted A races are not given new atomicity guarantees.

Pure Coordinator reducer/cleanup/seal/runner/guard bodies are untouched. No new async work, task collection, retry, action, IPC/configuration seam or helper edit. Existing quit precedence, once-only action, real cleanup fence and closure semantics remain the prior contract. No protected source/assertion was weakened.

## Regression definitions — not executions

Three NEW grouped definitions use actual owner preparation/status methods:

1. Fresh controlled poison while Open; request preparation returns Unavailable/Blocked, real A.begin and A.decide return Closing, repeated mark_closing.first is false, and no intent/start/fence/queue/action appears. A clean reserved-code control remains Open/NotConfigured.
2. Fresh poison observed FIRST by owner snapshot/status: A is closed before publication, repeated observations are stable/idempotent, static poison and closure-fault provenance are retained. The clean snapshot control stays Open.
3. Fresh poison observed FIRST by native exit preparation for None/Some0/Some1: Prevent plus closed A, no new drain or action. A controlled mid-admission Draining fixture then observes a later RunnerFailed fault and cannot acquire a cleanup fence or seal even if a completion value arrives.

These use real task-free DesktopState/A and valid JSON rejected before native runtime work; no AppHandle substitute, public override, protected A mutex access, fake proof or duplicated reducer. The later-fault fixture is explicitly a controlled pre-runner preparation boundary, not a complete app/event-loop invocation. Controlled panic is only mutex-poison injection, not evidence of an ordinary production panic.

Original14 Rust/four frontend definition bodies/names/assertions/helpers are text-exact; three additions make **17 Rust +4 frontend definitions, ZERO executed**. Both independent children remain absent. Lifecycle footer90B and commands footer88B stay exact; commands whole15747B SHA `b103ae5914cafe0aec42dacffafa2c39c5537e336e75d8446fad6dc83e68e4d0` is unchanged.

## Custody and diagnostics

Before edits, the initial checker correctly stopped on an alias representation mismatch: the original manifest's target was resolved destination, while os.readlink returned the first hop. Root correction `/private/tmp/rapidraw-gd1b7-application-root-20261016/APPLICATION-B-POISON-REPAIR-ALIAS-CORRECTION.json` SHA `d039d0b6bbfb214467ac84c5dcf26e1d9175b74b35d27b3d398bcb664a95c565` authorizes V2 ONLY. Original manifest/diagnostic/source grant remain immutable; no link or dependency-tree change occurred.

Authoritative V2 manifest162276B SHA `7569a744a3fe6d51ec7679dad3cc76628afdae84094ca15efea7d044016134e0` preserves all730 inputs/metadata. Literal alias is `/private/tmp/rapidraw-frontend-baseline/node_modules`; resolved destination is `/private/tmp/rapidraw-integration-20261015/frontend/node_modules`. Both are checked explicitly without traversing that tree. The original failed custody.py and pre-edit-alias-block.json are preserved; corrected custody-v2.py reverified730 bytes before editing.

Final before/after records bind all730 supplied hashes:729 unchanged, one authorised lifecycle diff; one new receipt yields exact inventory730→731. All713 index paths and detached HEAD/index/pointer are unchanged. Original42 author +16 name-amendment +19 stopped-review manifest/source records are rehashed unchanged before/after. No original worktree/evidence was edited.

**Pass:** pinned standalone rustfmt edition2024/skip_children write once and check twice, all exit0; new-line whitespace, full patch forward/reverse, full original test/helper/footer and outside-region reconstruction. The initial private reversal checker failed because it retained the separator newline before the new test module; checks-v2.py removes exactly that inserted separator and still demands full original byte equality. No source/test/oracle edit followed that checker failure. Both checker versions and diagnostic are retained. Formatting/checker results are not compilation or tests.

**Not exercised:** Cargo/rustc/Clippy/build/tests/Node, process query/fixture/socket/browser/provider/account/device/runtime/mobile/IPC/event-loop operations, dependency installation or shared cache. No native cache lease. No Git command/metadata write. Source edits, private scripts/hash/index/alias reads and the new receipt are the entire activity.

Evidence: `/private/tmp/rapidraw-gd1b7-lifecycle-poison-repair-20261016` includes exact original copies, production replacement map, unformatted new-test draft, patch/replay/check records, complete custody vectors, corrected authority hashes and final delivery/manifest. Root static review and separately assigned independent runtime remain next; no source author acceptance of the fault repair, app activation, credentials, sync or downstream authority. Stop frozen.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `eb76b4ad906895a11c3d6ca7b5cb1e28b4e6906a101c2bcbef8eeef07084f0f3` (7346 bytes).
