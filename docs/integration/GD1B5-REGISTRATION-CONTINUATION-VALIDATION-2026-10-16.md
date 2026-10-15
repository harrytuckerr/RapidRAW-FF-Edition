# GD1b5-S3B/C1 — catalog review and focused regression validation

2026-10-16 Melbourne. Actual model `gpt-6-astra`; task `01a140af-ec71-7f10-beda-00bdcdec426c`. Review worktree `drive-registration-continuation-review`, detached `1dbeadf0ba2d0dee4ac4bd0467f6c5cabc3ae7cd`.

**Accept for the assigned B source/public-API review and focused B/C1 runtime gate.** No blocking finding. Root retains canonical integration and acceptance. This does not install a catalog or establish Google/account/product readiness.

Attribution: Sol authored B. Astra independently reviewed all B production and 11 author tests and added the public-API probes below. Astra authored C1, so this report does **not** claim independent C1 source review. Root gpt-6.1-sol supplied independent C1 review4f0b9d93 and whole-baseline reconstruction; Astra executed the unchanged existing session regressions. No new C1 tests or proof stubs were added.

## Reviewed source and contract findings

- registration.rs:62 validates bounded exact ASCII ID/client syntax, positive canonical u64 project and unique approved scopes before copying. `.` ID and `_` client prefix are valid opaque syntax, not filesystem paths or evidence of registered Google clients. The project maximum is literal18446744073709551615; overflow/normalization aliases fail.
- registration.rs:124 rejects more than8 records, duplicate exact IDs and duplicate clients, including conflicting projects. Same project with distinct clients is permitted. Private owned records cannot be mutated through borrowed getters.
- registration.rs:146 is_empty reports only absence of native records. At150 resolve distinguishes NotConfigured, UnknownRegistration and DisallowedScopes, with exact selection and subset semantics; there is no default/fuzzy fallback, grant union or mutation. At180 selectors retain their own bounded copies. Payload-free errors cannot reflect supplied input.
- No loader, real registration, provider-mapping attestation, account identity or application activation was introduced. The presence amendment does not change these boundaries.

Read full original source receipt71856197, amendmentf9640e41, root B reviewcd8c99eb, C1 receipt1373705f/root review4f0b9d93, parent adoption contracte1ea97d2 and availability contractb1d830a7. Exact dependencies are recorded privately. Full user assignment supplied the authority where the Team mailbox preview was truncated; no truncated preview was treated as the full brief.

## Independent tests

NEW permanent `src-tauri/src/drive_auth/registration_independent_tests.rs`:10,573 bytes, SHA256 `a5ef523974b1cb85e7ccdd16852840f549ea81217fcca11317516db44de81826`. Seven grouped functions, no private candidate fields/constants or author test helpers. Imports use only public registration APIs and existing ApprovedScope. No absolute include_str, provider calls or extra dependency.

Expected boundaries came from root's pre-inspection known-answers.json SHA256 `2f2a87afcfd67504dfd15a7d24f6f460f24e48683df779487e8ac9f1a260751c` and presence supplement `fb39e7e2bd79d2c1cb0d037dc6a453305f09ff0bb150e265f59b4147986058aa`. Literal input/expected rows are embedded in Rust; semantic error labels map to the frozen enum, not candidate-derived predicates. Cases cover ID/client/project bounds and opaque positives, both scope orders/invalid sets,0/1/8/9 catalog capacity, duplicates in both orders, same-project distinct clients, exact case/prefix selection, no fallback/union, borrowed-record identity/presence after failed resolutions, and independent owned inputs after caller buffers change. Function count is7; parameter rows are not counted as additional test functions.

The probe and all649 existing inputs were frozen **before first execution**. No expected value, source, test or helper was changed after any execution.

## Executed commands and results

Exactly these four commands ran once, serially, from review/src-tauri:

| Command | Exit | Actual result |
| --- | --- | --- |
| `cargo test --locked --offline --lib drive_auth::registration -- --nocapture --test-threads=1` |0|18 passed,0 failed/ignored,389 filtered;11 author +7 independent |
| `cargo test --locked --offline --lib drive_auth::session -- --nocapture --test-threads=1` |0|29 passed,0 failed/ignored,378 filtered;21 existing author +8 existing independent child |
| `cargo fmt --all -- --check` |0|Pass |
| `cargo clippy --locked --offline --all-targets -- -D warnings` |0|Pass |

Thus47 disjoint executed tests, not a full-suite result. No full404/auth/CLI/ignored rerun occurred. Expected panic diagnostics from session poison/worker-panic tests were caught by their tests; the29-test summary is0 failures. The only compiler warning text was the existing build-script notice that valid ORT was present and download skipped; strict lint still exited0.

Exact UTC starts/ends, durations, argv, overrides, disk/process preflights, raw and sanitized logs are registration.json/session.json/fmt.json/clippy.json plus corresponding logs. First compilation took53.00s, session invocation1.76s, fmt0.61s, Clippy11.61s. Each preflight had no competing compiler and at least6GiB free.

Both focused runs used actual `/private/tmp/rapidraw-integration-20261015/target/debug/deps/rapidraw_lib-b8c68747df43d7c8`. Each run's binary was preserved as binaries/registration and binaries/session. They are identical40,169,520-byte binaries, SHA256 `c53105881272c60b79463c458fd1ea9678e3c16e33ad02f22dbe7d5045b0a47b`; these are two filtered executions of one binary, not two different build artifacts.

Environment: Rust/Cargo1.98.1 with RUSTUP_TOOLCHAIN=1.98 and RUSTUP_AUTO_INSTALL=0; DEV/TEST DEBUG=0 and INCREMENTAL=false, CARGO_INCREMENTAL=0, CARGO_NET_OFFLINE=true, GIT_OPTIONAL_LOCKS=0, private TMPDIR, target aarch64-apple-darwin, ORT_SKIP_DOWNLOAD=1, exclusive desktop CARGO_TARGET_DIR. All exact values are recorded per command. Only the new probe was formatted before freezing with standalone pinned rustfmt skip_children; whole-crate fmt later was check-only.

## Custody and generated output

Protected registration.rs20,157B SHA `f9d4a82d7eb3673d70ca022cd44e39074fba15257cc542e64667e62c328dde26`; session.rs65,639B `eba4929001cd3c04aab9c6142b6151a70f899debb773e8b5ac0a4305299b87e0`; root mod.rs770B `e2329fbde92001175c21f3e43e583d41023c9a62a0ad01d77ea6c8a6e680efda` remain exact. Root649-input manifest `f4ed00bf993ecf3325c6693c7469717f91d5912b696d497ce0ec9f0087043694` was rehashed and every existing path checked before/after each command. Exact inventory adds only the owned probe, then this report. Existing B11, C1 author21/child8, other tests, docs, README, mod/lib and locks are unchanged; no repair/retry/suppression.

ORT was initially absent in this isolated checkout. The explicitly authorized canonical copy was verified before/after:33,481,272B, SHA256 `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a`. No download. Builds generated four ignored Tauri schema files from an initially empty schema directory; before/after hashes and copies are preserved in private evidence. These generated ignored outputs are not tracked-source edits.

Private evidence: `/private/tmp/rapidraw-gd1b5-registration-continuation-validation-20261016`; sealed delivery/artifact manifests contain report, probe, logs, binary and input hashes. Desktop cache released at2026-10-15T18:07:08.594546Z, no cargo/rustc/clippy processes;18,702,004,224 bytes free. No further build commands after release.

## Boundaries

Pass: independent B static/public-API contract review,18 catalog tests,29 existing session regressions, fmt, strict all-target lint and649-input custody. Fail: none observed in these assigned checks.

Not exercised: full native suite/canonical integration, real configuration provenance/client registration, Google/account/provider/TLS operation, positive Code handoff/exchange, browser/IPC/UI/storage/Android/device, configured verification/adoption/C2–C4 or sync. Existing S1 localhost tests were inspected: code callbacks only receive failed handoff; positively acknowledged callbacks carry denial/provider-error outcomes, never a code exchange. Fixed catalog syntax/presence and fixture identity do not prove actual mapping or grants. Independent source/runtime evidence remains bounded to these frozen bytes and assigned tests.

Stop with permanent probes and frozen report for root review/import; no downstream authority.
