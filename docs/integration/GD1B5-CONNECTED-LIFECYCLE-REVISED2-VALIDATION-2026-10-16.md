# C3 corrected-probe validation — runtime Pass, strict lint Fail

Actual model `gpt-6.1-sol`; task `01a1412e-1f68-7322-b9b9-4c61b0682bb7`. Read-only worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-connected-lifecycle-review`, detached `52fe885d8b97fe7e2d0e4f6edad5d8e91befd4f0`. Astra authored C3 production; Sol authored the independent probes; root owns the two subsequent test-only corrections. This grant only executed validation and wrote this report/private evidence.

| Gate (exactly once each) | Result | Wall time |
| --- | --- | --- |
| `cargo test --locked --offline --lib drive_auth::session -- --nocapture --test-threads=1` | Pass, exit0;68 passed/0 failed/0 ignored/378 filtered;4.61s test phase | 58.477923125s |
| `cargo fmt --all --check` | Pass, exit0 | 0.672238459s |
| `cargo clippy --locked --offline --all-targets -- -D warnings` | **Fail, exit101;three await_holding_lock errors** | 20.575493708s |

No repair, suppression, expectation change or rerun after failure.68 selected functions are60 existing+8 new; no historical subsets/parameter rows are added. The corrected startup group now reached and passed exact stored WatcherFailure/Closed/no candidate/no identity, actual Stopped, matching Draining Slot/no pending, single-start and exact shutdown failure assertions. Its previously unreached120ms startup-timeout tail and actual cleanup-gate fence also completed. The passing helper predicate admits only WorkerFailure or Cancelled; which variant occurred is not logged. Original67Pass/1Fail remains Fail; prior static Debug-bound failure remains static Fail/runtime Not exercised. Neither old evidence set was modified or relabelled.

## First failed gate and bounded correction boundary

Observed strict Clippy diagnostics:

- session.rs3904: guard in author pure-clock test, await3930.
- session.rs4175: guard in author counter/poison test, awaits4181/4183.
- configured_verification_independent_tests.rs1110: guard in migrated stalled-continuation test, new watcher cleanup await1140.

Each source span explicitly calls `drop(state)` before the await. The observed failure is the lint's guard lifetime/scope analysis, **not demonstrated runtime lock retention**. Smallest follow-on is an explicit lexical block around each existing test guard/assertion span, ending before await, with operations/outcomes unchanged. No production mutex change or lint allowance is indicated. Root separately acknowledged these bounded cfg(test) scope corrections; they are outside this grant. Full diagnostics and exact snippets are preserved.

## Inputs, environment and artifact

All669 product-input hashes/exact inventory matched freeze `3bd8ba447d0b11c8556451716027260168d8c64f55cd0f96eb22b1318c8883ac` before and after. Equality patch independently verified: only `assert_eq!(slot.key, work.key)` becomes `assert!(slot.key == work.key)`, with exact equality preserved. Current child40,455B `b671ea103c7756967f6e9be1b3fa67fb015b24574c60a01922b0a729065d1af0`; production session f80b8242/exact160015B64e46d87 prefix+110Bfooter and migrated child db61 remain unchanged. HEAD/index and historical reports are exact. This new report alone adds the670th product path; five ignored outputs are separately hashed.

Pinned Rust/Cargo1.98.1 verified. Before every gate: no competing compiler,>=6GiB; recorded free bytes begin 18397597696. Exact argv/cwd/UTC/durations/exits/logs/environment are private. Required overrides: RUSTUP_TOOLCHAIN1.98/AUTO_INSTALL0; DEV+TEST DEBUG0 and INCREMENTALfalse; CARGO_INCREMENTAL0; CARGO_NET_OFFLINEtrue; GIT_OPTIONAL_LOCKS0; TAURI_ENV_TARGET_TRIPLE aarch64-apple-darwin; ORT_SKIP_DOWNLOAD1; PYTHONDONTWRITEBYTECODE1; fresh private TMPDIR; exclusive `/private/tmp/rapidraw-integration-20261015/target`. No secret environment dump/download/dependency/lock changes. Existing ignored ORT33,481,272B SHA2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a was hash-reused; valid runtime/download skipped. Four generated ignored schemas are preserved/hashed.

Actual selected binary `/private/tmp/rapidraw-gd1b5-connected-lifecycle-revised2-validation-20261016/binaries/rapidraw_lib-b8c68747df43d7c8`: 40834016B SHA256 `fd9d60434d96715890926f6bbefc4dee4210a94be185a776038139c0e733fb24`. Current child and binary were preserved before lint; no historical binary substituted. Raw and ANSI-normalized logs, all68 function records, root patch/verification, full before/after669 custody/inventories, ignored outputs and source snippets are private under `/private/tmp/rapidraw-gd1b5-connected-lifecycle-revised2-validation-20261016`.

Actual scheduled raw-State expiry/replacement checks remain separate from direct future-clock/reducer evidence. The60ms delay does not prove the watcher captured an old timer or force a live stale-timer wake; the existing author direct guard assertion is pure reducer coverage. Synthetic HTTP labels prove fixture custody, not provider/client-project/account authentication. No positive Code handoff/Google entry or external provider network occurred.

**Pass:** focused runtime68/format/custody. **Fail:** strict all-target Clippy; overall validation gate remains incomplete. **Not exercised:** full/canonical suite/integration, Google/provider/full OAuth chain, C4/configuration/browser/IPC/store/refresh/lease/Android/device/UI/Drive/sync/product acceptance. No extra tests/commands/source/Git/device actions.

Cache **RELEASED** `2026-10-15T20:11:45.942693+00:00`; no cargo/rustc/clippy-driver, 18352717824B free. Sealed machine receipt/artifact manifest accompany this report; final hashes are in Team handoff. Stop frozen; root alone owns subsequent correction/integration.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `6c91abe7d8dd85139c44b9b46a0767c69f4d7eed657cd655da4c535ca018e04d` (5702 bytes).
