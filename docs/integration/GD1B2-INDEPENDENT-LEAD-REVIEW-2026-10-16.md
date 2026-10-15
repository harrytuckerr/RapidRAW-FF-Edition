# GD1b2 independent listener source review

Reviewer: lead, actual `gpt-6.1-sol`, 2026-10-16 Australia/Melbourne. **No blocking source finding identified; runtime and integration acceptance pending.** This is not a working Google connection or a complete security audit.

Frozen base `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`; listener SHA-256 `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41`; export module `47ffc8546999cca0a1e5fafc77eb117625821803a501bdf84cdcf4ba6c22c0ca`; protected PKCE helper `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`; author receipt `7f20c8b183d08963f0ba14f23684ce76ab067472403e4f4f8de98ad581a3bd34`. Root read the entire production source, all 16 test definitions, complete receipt and manifest. Manifest `0a4476e63972ef9942d07480754fd9ed873578768ecb8618ed1ef426cb1b61b0` and all 15 referenced file hashes/sizes verified, including protected files, patch, command evidence and contracts. An isolated review snapshot contains these exact inputs plus only root's `pub mod drive_auth;` registration for later compilation.

## Source conclusions

- Binding precedes URL availability and uses only actual `127.0.0.1:0` socket/port information. Accepted streams are checked against the bound local endpoint and an IPv4 loopback peer before forwarding. The Host field never selects the callback authority; forwarded headers are inert.
- The narrow HTTP subset checks exact origin-form callback path/query, GET/HTTP 1.1 framing, one exact Host, header names/values and body framing. It rejects aliases, duplicate Host, buffered trailing data and protocol upgrades. It deliberately does not claim general HTTP-server support. Unknown callback query fields remain governed by the corrected helper.
- Parsing, active streams, accepted requests and per-connection time are bounded. Nonblocking reads and polling permit cancellation/total expiry checks while a peer stalls. Invalid requests do not renew the original PKCE lifetime.
- The consuming worker closes owned streams/listener on all returned outcomes. Owner Drop also closes them. Temporary setup failures drop their local socket. Unexpected I/O remains a typed local failure, while peer reset/EOF rejects only that connection. The source's closure structure is clear; fatal-I/O fault execution remains untested.
- Code/verifier results remain native-only. Static browser responses reflect no request or provider text and contain no external resources. Error types contain only bounded categories. No credentials, callback data or URLs are logged or serialized.

Primary checks: [RFC 8252 §§7.3 and 8.3](https://www.rfc-editor.org/rfc/rfc8252#section-7.3) informs actual loopback port and temporary listener lifetime; [RFC 9112 §§3 and 5–6](https://www.rfc-editor.org/rfc/rfc9112#section-3) informs origin-form, Host cardinality, header syntax and framing. IPv4-only and the stricter callback-only HTTP subset are explicit scope choices, not universal availability/conformance claims.

## Contracts the next integration must preserve

1. Run `wait_on_worker` on an owned worker. Promptly start that worker before browser interaction, or drop abandoned sessions. A retained owner that is never polled cannot enforce expiry by itself. Before a worker starts, merely setting the cancellation flag does not synchronously close the socket; Drop or the next checked operation does. User-facing cancellation completion must follow actual worker/owner cleanup.
2. The four-stream and 32-accept budgets exclude the OS-managed backlog. A local attacker can force a bounded retry failure; these limits do not make loopback a private authenticated channel. PKCE/state and the later official token endpoint remain essential.
3. Static responses use one best-effort nonblocking write. A short write, disconnect or backpressure may prevent complete browser display. It does not undo the accepted native code result or establish browser receipt. Keep those outcomes separate.
4. Accepted native code is not identity, usable grants, stored credentials or account ownership. Future account-generation/cancellation checks must prevent a stale worker result from changing a newer account/session. No Tauri command or browser/token/storage adapter is provided here.

## Verification boundary

Pass: complete independent static review and exact input/custody checks. Sixteen author tests are defined, including 14 real-socket/owner definitions and two pure parser/response definitions; none have been executed for this listener yet. Not exercised: compilation, socket bind/read/write/close, timing/cancellation under load, response delivery/short writes, injected fatal I/O, Windows/Linux behavior, browser, Google consent/token exchange/grants, secure storage, Android or Drive sync.

Next: after the corrected helper's separate validation and explicit cache release, grant one validator the isolated listener snapshot, actual localhost tests and independent requests. Preserve source and expectations on failures, obtain a bounded correction if needed, then review the combined source. Do not import or enable it from this static review alone.

## Runtime acceptance

Accept exact listener `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41` for later canonical integration. Root read the final runtime report `b3065da395b63879e0c88c23525f65477fd98b281840c0aceb85a849b484c8af`, machine receipt `c3a3c8b6d9e21a45a2672e09cab4ce01145baa5fb5e796be29f4d618eb210081`, actual execution logs/results and correction diff. Root rehashed all 64 evidence records under manifest `7d5d2c219f95d789ea008267a8a87c9b05f9bc508b1a6824e3197f90d3079436` and all 565 baseline inputs. Entire ten-probe source was independently read; it uses public production APIs and actual localhost TCP.

The full default suite passed 274 tests with 14 ignored. The 16 supplied listener functions and 30-auth subset overlap this count. Ten independent probes passed before and after the exact two-assertion lint correction; these are ten unique tests, not twenty. The first strict lint attempt failed on two constant assertions in the temporary probe. Moving only those unchanged assertions into const blocks fixed lint; final probe/fmt/strict all-target Clippy passed. Original failure/source/binary remain preserved. Product code and runtime expectations never changed. Corrected probe `33fce63c46907357993c07cacbe8cd56c3b8be593d366fc23f9bb13c77056b72` will remain an independent regression input; canonical combined checks are still required.

Observed macOS cancellation, exact header limits, four active slots, 32-accept budget, absolute connection deadline and retained-owner polling behavior support the stated contracts. Local timing and complete small static replies do not establish universal scheduling, browser receipt, forced short-write or fatal-I/O coverage. Google, external TLS, account/grant, browser, storage, Android and sync remain Not exercised. Cache release at 2026-10-15T14:11:01Z is recorded with no active compiler. Root has not integrated or activated a product connection from this review.
