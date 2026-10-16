# GD1b2 — native desktop loopback listener, source-only receipt

Frozen 2026-10-16 Melbourne / 2026-10-15 UTC by actual `gpt-6-astra`, Team slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`, task `01a13fba-1ea2-7823-bb2d-02da80d02f05`. **Implementation and test definitions ready for independent review. No Rust compilation or socket test was executed. This does not enable Connect Google or Drive sync.**

Worktree `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-desktop-loopback`; detached base `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`. Own changes are the new listener, one module-export line, and this receipt. The supplied untracked PKCE helper remains SHA-256 `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`; its independent compilation is a separate gate, not presumed by this receipt. No API change was needed.

## Frozen artifact custody

| File | Bytes | SHA-256 |
|---|---:|---|
| `src-tauri/src/drive_auth/loopback.rs` | 31837 | `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41` |
| `src-tauri/src/drive_auth/mod.rs` | 115 | `47ffc8546999cca0a1e5fafc77eb117625821803a501bdf84cdcf4ba6c22c0ca` |
| `/private/tmp/rapidraw-gd1b2-loopback-20261016/source.patch` | 33226 | `98988f83b0fc63c2b421304353499f4202a684fc9185f234df025bbd233b5b5e` |
| `/private/tmp/rapidraw-gd1b2-loopback-20261016/manifest.json` | 4242 | `0a4476e63972ef9942d07480754fd9ed873578768ecb8618ed1ef426cb1b61b0` |
| `/private/tmp/rapidraw-gd1b2-loopback-20261016/commands.json` | 71886 | `b6c0f7b85547d04b091699d3c4b8d6b96d3dd084d425e00cceffb941155f347e` |
| `/private/tmp/rapidraw-gd1b2-loopback-20261016/inputs.json` | 1503 | `fb80d53a2fe8b7b9f7c102be8d0a22a9bebdcc647a7db4daf7318be7909e202e` |

The patch adds `loopback.rs` and changes the supplied module only by `pub mod loopback;`. Original module bytes are preserved privately as `original-mod.rs` (original SHA-256 `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8`). The manifest records before/after equality for PKCE, `lib.rs`, Cargo manifest/lock, package manifest/lock and Drive model/graph. Existing tracked files remain identical to HEAD. No Git mutation, dependency resolution, shared cache, Cargo/npm build, UI registration, browser, device, account or provider operation occurred.

Inputs read: original GD0 §6 at `../drive-contract/docs/drive/contract.md`, SHA-256 `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`; approved desktop clarification at `../ui1-publication-receipts/docs/drive/NATIVE-OAUTH-SCOPE-ADDENDUM-2026-10-16.md`, SHA-256 `6c34f7bafacab48573b2d70f6dfed7d74b868d2ec24f4920d7885356494c646c`. Exact selected appdata/file scopes remain delegated to the helper. Requested scopes are not usable grants or verified account identity; no broader scope fallback or assumption of accumulated desktop grants.

## Native API and ownership

`LoopbackSession::bind(client_id, scopes, lifetime)` (line 80) binds only `127.0.0.1:0`, reads/validates the actual nonzero local port, sets nonblocking mode, then creates the PKCE transaction with that port and `Instant::now()`. No browser URL exists before this binding. Setup errors drop the local listener. No DNS, wildcard bind, IPv6 or reuse option is configured. IPv4 availability and platform-specific socket behavior remain runtime gates; this is not a claim of process ownership against hostile local software.

`authorization_url_for_browser(&mut self)` (line 118) checks lifetime/cancellation, then borrows the native helper URL. It is not IPC-safe and must not enter React/logs. `cancellation()` returns a cloneable atomic signal without secrets. `wait_on_worker(self)` (line 135) consumes the owner and synchronously polls nonblocking sockets; its name/docs require an explicitly owned worker, never the UI thread or an async runtime executor thread. There is no hidden spawned production thread or browser launch.

Return type is `Result<CallbackOutcome, LoopbackError>`: the existing native code+verifier grant, matching denial or provider error, versus bounded local cancel/timeout/resource/I/O/configuration failure. These secret-bearing owners/results have no Debug/Display/Clone/Serialize implementation. Error variants contain only static categories or the helper's bounded typed error, never OS strings or callback text. No logging is present. Clearing/dropping buffers is not guaranteed memory zeroization.

Once running, cancellation is observed before each connection is processed and again before callback forwarding. Already-expired deadline wins over cancellation; an accepted terminal helper result wins over later cancellation. Before starting the worker, dropping the owner releases the socket immediately; merely setting the cancellation signal does not create a background watcher. The next URL access/worker call observes that signal and closes. Later integration must promptly start its worker or drop an abandoned session; this component cannot enforce a deadline while its owner is retained but never polled.

`close`/Drop (lines 248–260) shuts down and drops owned streams, drops the listener and transaction. All public worker return paths—including native code, denial, provider error, cancel, timeout, resource exhaustion or fatal I/O—close before returning. Unwinding also drops the owner. Peer EOF/reset is a rejected connection; unexpected accept/setup/read or local-binding failures terminate the session. No socket handle is exposed for cloning or retargeting.

## Actual HTTP boundary and resource policy

At accept and again immediately before forwarding, the listener verifies the stream's actual local socket equals its bound IPv4 endpoint and its peer is IPv4 loopback. Host/forwarding values never select an authority. `parse_request` (line 334) accepts only:

- ASCII HTTP/1.1 `GET`, exactly space-delimited request line, CRLF headers, exact origin-form `/oauth2/callback/google?<query>`. Absolute/authority/asterisk forms, encoded or normalized path aliases, literal fragments, raw whitespace/control/backslash in the target and other methods/versions fail.
- Exactly one case-insensitive Host field with exact `127.0.0.1:<actual port>` after normal optional field whitespace. Duplicate/missing/wrong Host, leading folded lines, whitespace before colon, invalid token names or control/non-ASCII values fail. Ordinary unknown headers are ignored; `Forwarded` and `X-Forwarded-*` are inert and cannot repair Host.
- No body framing, except a single exact `Content-Length: 0`. Transfer-Encoding, Expect, Upgrade, duplicate/nonzero/noncanonical length, buffered trailing body or pipelining fail. Future unframed bytes cannot be predicted; they are never interpreted and the one-request connection closes.

Only then is `http://127.0.0.1:<actual port>` joined to the untouched target and forwarded to `PkceTransaction::accept_callback`. No duplicate query parser, state decoder or scope logic was introduced. The frozen helper still governs percent/UTF-8 decoding, extension handling, duplicate keys, state, code/error ambiguity, issuer, reported scope, replay and total lifetime. Invalid requests do not extend any deadline or produce a grant.

| Limit | Concrete policy |
|---|---|
| Accepted concurrent connections | At most 4 owned streams; no extra accept while full |
| Lifetime connection budget | At most 32 accepts, including wrong requests, EOF and stalled peers; when exhausted and all accepted streams finish, return `ConnectionLimit` |
| Kernel queue | OS-managed pending backlog is not counted as an owned stream or claimed to be configured by std; listener close discards it |
| Header/request bytes | 16,384 per accepted stream; 16,385-byte reserved buffer detects overflow, with at most 1,024 new bytes read per stream per poll |
| Header count | At most 32 fields; PKCE independently retains its 8,192-byte callback and query limits |
| Connection time | Absolute 2 seconds from accept; partial/trickled bytes never reset it |
| Total time | Original helper monotonic lifetime, at most 600 seconds; never renewed by local requests |
| Poll work | At most 4 nonblocking reads and one accept per poll, then 10ms sleep; real scheduler delays are not a hard real-time guarantee |
| Response work | One nonblocking write of a fixed sub-kilobyte response, no retry, then shutdown; peer backpressure/short write/disconnect cannot stall or undo a native result |

Payload buffering is at most four reserved 16,385-byte connection buffers, plus bounded temporary parsing/helper allocations. The limits are conservative policy, not measured throughput or provider compatibility evidence. A local attacker can exhaust the 32-connection budget and require the user to retry; exhaustion is explicit failure, never authorization.

Responses are static minimal HTML: `200` for a processed code/denial/error and `400` for rejected requests, both telling the user to return to RapidRAW without claiming successful sign-in. Both include connection-close framing, `Cache-Control: no-store`, `Pragma: no-cache`, `Referrer-Policy: no-referrer`, restrictive CSP (`default-src 'none'`, no framing/base/forms), and nosniff. No reflected path/query/provider text, state, verifier, code, redirects, scripts or external resources. Delivery is best effort; terminal cancellation/expiry/fatal paths may simply close sockets. Browser rendering is Not exercised.

## Definitions and checks

**Pass:** standalone `rustfmt --edition 2024 --check --config skip_children=true src-tauri/src/drive_auth/mod.rs src-tauri/src/drive_auth/loopback.rs`; `git diff --check`; `git diff HEAD --exit-code`; protected hashes versus captured inputs. Formatter is installed rustfmt 1.9.0/rustc 1.97.1, macOS 26.5.2 arm64. Project review compilation must use pinned 1.98. `git diff --no-index` exit 1 is the expected added/changed-file result; commands and exits are recorded. Commands evidence includes intermediate and final source diff outputs, not a transcript of every exploratory read.

**Defined, not run:** 16 test functions: 14 use real localhost TCP streams/production owner or its actual private poll/read helpers; two check pure parser/static response behavior. Cases cover actual code+native verifier/closure; denial and provider-error closure; wrong/multiple Host, absolute target, encoded/dot path, wrong method/version/state, encoded known-key duplicate and fragment; inert forwarding headers; body/TE/Expect/Upgrade/control injection; body/pipelining/LF/missing Host parsing; stalled versus concurrent valid client; slow header expiry with later success; oversize/field-count rejection; cancel and total timeout; 32 wrong requests; owner drop/preworker cancellation; real socket binding; active-connection cap and deterministic exact per-connection deadline; privacy headers. Unknown callback extensions are present in successful code and denial cases. Table entries are not separate test functions. All requests/client IDs are synthetic; no Google account or provider is involved in these definitions.

**Not exercised:** any compile/lint/test binary, actual bind/accept/read/write/close, OS entropy, timing/cancellation/short-write/backlog behavior, injected fatal I/O, browser interaction, native Tauri transport, live OAuth, granted-access/account verification, token exchange/refresh, secure storage, Android, Drive/photo/library transfer or sync. Source-only means neither compiler correctness nor runtime acceptance is established. No claim is made that the listener implements a full HTTP server or that its static tests validate Google interoperability.

Next reviewer: root registers this module only in an isolated review snapshot, owns the native cache and runs from `src-tauri` with project Rust 1.98 and established offline/no-debug/no-incremental/private-TMPDIR environment: `cargo test --locked --offline --lib drive_auth::loopback` (16 defined tests), then `cargo test --locked --offline --lib drive_auth` (includes the frozen 14 PKCE definitions), appropriate formatter/strict lint and independent socket/malformed-request probes. No Tauri command/browser launch is needed for these tests. Reviewer may find compile/runtime issues; implementation acceptance remains theirs. Token exchange, secure storage and user-visible Connect Google are separately owned later slices.

## Primary sources

Read 2026-10-16 Melbourne / 2026-10-15 UTC:

- [RFC8252 §§7.3, 8.3–8.4, 8.9–8.10](https://www.rfc-editor.org/rfc/rfc8252.txt): native loopback literal/actual ephemeral port, temporary listener, state and redirect binding. IPv4-only is this assignment's constrained subset; RFC guidance recommends trying both IP versions, so broader platform availability is not claimed.
- [RFC9112 §§2–3, 5–6](https://www.rfc-editor.org/rfc/rfc9112.txt): request-line/CRLF syntax, Host cardinality, origin form, token fields, obsolete folding and message framing inform the parser. General HTTP servers also support forms/features intentionally rejected here, including absolute-form; this callback-only subset is explicit and not advertised as general HTTP conformance.
- [RFC6749 §4.1.2](https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2): unrecognized response parameters remain ignored by the frozen helper, while known fields retain exact validation. No broad scope or account inference is introduced by the listener.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `7f20c8b183d08963f0ba14f23684ce76ab067472403e4f4f8de98ad581a3bd34` (13360 bytes).
