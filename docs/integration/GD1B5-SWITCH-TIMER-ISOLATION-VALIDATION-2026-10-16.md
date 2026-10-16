# GD1b5-S3C4 targeted timer validation

**Pass:** the corrected exact timer test, format check and strict all-target Clippy. Exactly **one** existing test executed, 0 failed, 0 ignored; no86/full/CLI/ignored rerun. This is amended-case validation, **not an additional distinct pass to add to the original86**.

Actual validator `gpt-6.1-sol`, Team slot `01a13e03-d91b-7810-a557-ca8e5fd5cd16`. Review snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-switch-timer-proof-review`, detached `47a1a46b2fec95826ffe5e71e39b4b5889211919`. Corrected child42971B SHA `a35cbb7342db2d4d4b238dd99e7b49febdfebe2702bc76072e3a1a50be200446`; every existing source/test/oracle/doc stayed read-only.

The exact selected test was `drive_auth::session::switch_decision_independent_tests::independent_accepted_earlier_expiry_reschedules_actual_watcher`. Its current body was checked against the new684-input freeze before execution: raw connected-none/deadline/stamp/key/ID/epoch/single-watcher observations and exact Draining/consumed Slot checks precede synthetic reap; the original slot-none assertion follows reap. No explicit expiry getter/reducer/cleanup notification intervenes after public acceptance and before raw expiry observation. The unchanged100ms retention,900s credential and2s observation timeout passed through the actual watcher and restricted localhost verifier.

The amended result closes the documented **intervening cleanup/expiry path** gap for this observed run. It does not instrument prior old-timer capture, guarantee a particular executor interleaving or prove every notification race. No production hook/change, forged watcher/proof, provider identity or full positive OAuth/supervisor claim. Original86/fmt/strictPass and original qualification remain literal immutable evidence for their earlier bytes; this result does not relabel them. Root canonical integration/combined validation remains separate.

| Exact command arguments, cwd src-tauri | Exit | Seconds |
| --- | ---: | ---: |
| cargo test --locked --offline --lib drive_auth::session::switch_decision_independent_tests::independent_accepted_earlier_expiry_reschedules_actual_watcher -- --exact --test-threads=1 | 0 | 80.938575 |
| cargo fmt --all --check | 0 | 0.750111 |
| cargo clippy --locked --offline --all-targets -- -D warnings | 0 | 18.982519 |

Actual summary: `ok. 1 passed; 0 failed; 0 ignored; 0 measured; 463 filtered out; finished in 0.11s`. Exact absolute binary argv, UTC, elapsed time, raw stdout/stderr and preflights are in private `commands.json`; `test-results.json` contains the one actual name/result. All three gates ran once and sequentially. Existing Rust/Cargo1.98.1 version outputs are retained; toolchain host `aarch64-apple-darwin`.

Overrides: `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`, `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`, DEV/TEST profile `DEBUG=0` and `INCREMENTAL=false` under full `CARGO_PROFILE_*` names, `CARGO_INCREMENTAL=0`, `CARGO_NET_OFFLINE=true`, `GIT_OPTIONAL_LOCKS=0`, `PYTHONDONTWRITEBYTECODE=1`, private `TMPDIR=/private/tmp/rapidraw-gd1b5-switch-timer-isolation-validation-20261016/tmp/`, `TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin`, `ORT_SKIP_DOWNLOAD=1`. Each gate freshly verified no competing cargo/rustc/clippy and ≥6GiB free. Only overrides were logged, not unrelated inherited environment.

Known ignored ORT33481272B SHA `2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` was hash-read-copied from canonical; both endpoints verified. Build output confirms existing runtime/download skipped. No dependency resolve/install/clean or download.

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| Current unchanged child | 42971 | `a35cbb7342db2d4d4b238dd99e7b49febdfebe2702bc76072e3a1a50be200446` |
| Actual preserved executed test binary | 41175328 | `4fb099fadf68994d061c0d10083037353150b773a8c42c98d503a4f609b13f19` |
| Immutable wired session | 209473 | `ef4726e1b53c8cf59d94ad4d0e9d86e58597bf827c8bd1bc08e26076545773fa` |
| Entire immutable author prefix | 209371 | `525be1112c4273988beee762458a603ef8094d05a38cc0d06e9721b65af041ce` |

Before/after hashes verify **all684 original files** and exact HEAD/index/pointer/inventory unchanged against targeted-runtime-inputs.json113316B SHA `c5e825629bfa071cd44ca54b737794045a1d511ac9ca53a8f92bbd8d941ec169`. This report is the sole new product path: **685**; ignored ORT/four generated schemas separately inventoried with schema copies. Original29 source-amendment and57 runtime sealed records were read/hash-verified unchanged, including the original qualification/procedural-deviation disclosure. No source/oracle formatting write, Git or app mutation.

One private preparation diagnostic is retained: the runner adapter initially expected a different indentation in the old private orchestration script, asserted before writing a runner, and the attempted launch reported a missing private runner. Only private generation was corrected **before any Cargo invocation**; no gate/source/oracle execution or rerun occurred at that point. This is not a product/test failure or changed expectation.

Cache explicitly **released 2026-10-15T23:34:09.393706+00:00**: no cargo/rustc/clippy processes, 16734408704 bytes free; root-owned lease file unchanged. No further native commands. Private evidence `/private/tmp/rapidraw-gd1b5-switch-timer-isolation-validation-20261016` includes full/sanitized logs, actual binary, complete before/after684 inventory, current frozen child, provenance/original-seal custody, runner preparation diagnostic, env/argv/toolchain, ignored outputs, release, machine receipt/delivery/sealed manifest. Final report/seal hashes are sent through Team after sealing.

**Pass:** one targeted runtime case, fmt, strict all-target Clippy, input custody. **Fail:** no gate failure. **Not exercised:** other85 focused cases in this task, full/default/canonical/ignored suites, forced old-timer interleaving, Google/provider/full OAuth chain, browser/IPC/Android/device/storage/refresh/lease/transfer/sync/product acceptance. Frozen for root review/canonical decision; no downstream authority.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `caf105ce75b4a394fda7bd36741ff2318ede5add53d4da44bc031174af4a672c` (6179 bytes).
