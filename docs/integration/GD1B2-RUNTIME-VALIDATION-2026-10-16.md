# GD1b2 independent runtime validation — 2026-10-16

**Pass** for the final bounded native/runtime gates. The first strict lint attempt **Fail** was confined to two assertions in the independent temporary probe; its exact source, binary and failure log remain preserved. Lead retains source acceptance/integration. This does not connect Google or implement Drive sync.

Model `gpt-6.1-sol` (live roster), macOS-26.5.2-arm64-arm-64bit; detached `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4` at `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-loopback-review`. Exact listener `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41`, module `47ffc8546999cca0a1e5fafc77eb117625821803a501bdf84cdcf4ba6c22c0ca`, PKCE `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`. Both author and lead receipts read in full before execution.

| Gate | Result | Exit | Elapsed |
|---|---|---:|---:|
| `author16` | 16 passed | 0 | 50.516s |
| `combined30` | 30 passed (includes author16) | 0 | 3.237s |
| `independent10` | 10 passed | 0 | 84.147s |
| `default-lib` | 274 passed, 14 ignored | 0 | 3.204s |
| `fmt` | Pass | 0 | 0.553s |
| `clippy-all-targets` | Fail: temporary-probe constant assertions only | 101 | 9.871s |
| `independent10-const` | same ten passed after authorized lint correction | 0 | 7.610s |
| `fmt-const` | Pass | 0 | 0.548s |
| `clippy-all-targets-const` | Pass, including corrected probe | 0 | 1.236s |

Author16/combined30 are overlapping subsets of default274. The repeated independent ten are the same tests, not twenty unique passes. No old CLI, separate PKCE-only or ignored tests were run.

Commands (from `src-tauri`; the changed independent target/fmt/Clippy repeated only under the explicit lint grant):

```text
author16: cargo test --locked --offline --lib drive_auth::loopback -- --nocapture
combined30: cargo test --locked --offline --lib drive_auth -- --nocapture
independent10: cargo test --locked --offline --test drive_auth_loopback_independent -- --nocapture --test-threads=1
default-lib: cargo test --locked --offline --lib -- --nocapture
fmt: cargo fmt --all -- --check
clippy-all-targets: cargo clippy --locked --offline --all-targets -- -D warnings
independent10-const: cargo test --locked --offline --test drive_auth_loopback_independent -- --nocapture --test-threads=1
fmt-const: cargo fmt --all -- --check
clippy-all-targets-const: cargo clippy --locked --offline --all-targets -- -D warnings
```

The independent target used only public production APIs and actual `127.0.0.1` TCP streams; it did not import private parser/poll helpers or generate expected results from production functions. It verified actual ephemeral redirect binding, production entropy/S256 against the public verifier with a separate SHA256/base64url formula, exact selected scopes, code/denial/provider-error outcomes with unknown extensions, and listener/peer closure. Malformed/wrong/duplicate Host, inert forwarding, absolute/encoded targets, false state, encoded duplicate known keys, body framing, buffered body and pipelining could not authorize or poison a later exact valid code.

Resource probes rejected an extra byte above the 16,384-byte request cap and 33 fields, accepted the exact byte/32-field boundaries, observed a fifth valid request queue while four stalled slots remained occupied, then succeed after one slot closed, and returned ConnectionLimit/closure after 32 wrong requests. Four/32 numeric assertions are compile-time checked in the corrected probe. Kernel-managed backlog is separate and was not measured.

Cancellation/expiry interrupted stalled peers. Total expiry was observed at 185ms initially /184ms after lint correction; trickled-header closure at 2014ms/2021ms despite continued progress. These are current macOS observations with test bounds, not universal scheduling guarantees. Author tests additionally execute the private exact deadline/cap boundary helpers. The public retained-owner probe confirms cancellation alone or expiry without polling does not autonomously close a retained listener: checked URL access, worker entry or Drop performs cleanup. Future integration must promptly start an owned worker or drop an abandoned session.

Observed complete success/denial/error replies had static privacy headers, no state/code/provider-marker reflection, no external resources, and identical denial/error bytes. Delivery is best effort by design; rejection reads permit an empty/reset reply. Forced short writes/backpressure and fatal I/O injection remain Not exercised. Native acceptance is separate from browser receipt.

The first strict Clippy exit101 flagged only `assertions_on_constants` at temporary probe lines332/396. Lead explicitly authorized exactly two same-value const-block moves in message `01a13fe4-1ce7-7700-87f8-61a4381c6e76`. No production/helper/constant/oracle or runtime-expectation edits, suppression, or failure relabeling. Original probe SHA `bd3ad746c0c97aa78df90f72d77af57996e3ddfdcb8b7605ba6d44b7fd3a7565`; corrected `33fce63c46907357993c07cacbe8cd56c3b8be593d366fc23f9bb13c77056b72`. Exact two-hunk diff and both executed binaries preserved. Corrected ten/fmt/strict all-target Clippy Pass; no default-lib repetition.

All 565 pre-existing tracked/untracked inputs matched before and after. Temporary probe bytes were preserved privately then **only that file** removed; no existing test/file/directory was removed. Product source/registration, receipts, README, dependencies, locks and Git stayed untouched. Protected manifest `cdb3323096c81afff095f02e214d5ef56342d4d0318958d397194ace3ed39bcf`. Known ignored ORT33,481,272B /`2b885992d3d6fa4130d39ec84a80d7504ff52750027c547bb22c86165f19406a` hash-read-copied; build.rs confirmed the existing valid runtime and skipped download. Four ignored schema outputs were generated and hashed.

Rust/Cargo1.98.1, rustfmt1.9.0-stable, Clippy0.1.98. Every gate checked no competing compiler and >=6GiB reserve. Exact environment/UTC/argv/exits/free space/durations are in each invocation/result JSON: declared exclusive target, DEV/TEST debug0, DEV/TEST incrementalfalse, CARGO_INCREMENTAL0, RUSTUP_TOOLCHAIN1.98, CARGO_NET_OFFLINEtrue, private TMPDIR, TAURI_ENV_TARGET_TRIPLE aarch64-apple-darwin, ORT_SKIP_DOWNLOAD1 and GIT_OPTIONAL_LOCKS0. All Cargo checks locked/offline; no download/dependency refresh or source repair.

Actual preserved binaries:
- `author16`: 33097984B, SHA256 `c6c76cd22ced9b58fbba78679e81e5d07620aefca809eb50c6e19309a1d9200b`.
- `independent10`: 2451584B, SHA256 `4052fa62c436c7f3176671151dad8a32377517579887e29b3e91e59dacde4575`.
- `independent10-const`: 2451584B, SHA256 `a9001a3f96a75dfebfbdb9754c29651e60e9dcf5b26e5d555ae0e3838f3980ad`.

**Not exercised:** Google/provider/account authorization, external network, browser rendering, Tauri/OS integration, token exchange/storage/refresh, Drive sync, Windows/Linux/IPv6, injected fatal I/O, forced response backpressure/short writes, hard real-time guarantees, memory zeroization, Android/device/frontend, ignored/GPU/external-fixture tests. This is bounded validation, not complete security/product acceptance.

Private evidence `/private/tmp/rapidraw-gd1b2-loopback-validation-20261016`; `receipt.json` SHA256 `c3a3c8b6d9e21a45a2672e09cab4ce01145baa5fb5e796be29f4d618eb210081`. Logs, both probes, correction diff, commands/environment, before/after manifests, binary custody and cleanup are sealed by `artifact-hashes.json` (excludes itself).

Desktop cache **released** at `2026-10-15T14:11:01.111625+00:00`, no active cargo/rustc/Clippy, 17.47GiB free. No further build or downstream work.
