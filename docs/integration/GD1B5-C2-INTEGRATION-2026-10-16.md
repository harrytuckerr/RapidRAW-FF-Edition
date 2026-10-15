# C2 configured verification integration

Accepted locally on 16 October 2026 by the lead, gpt-6.1-sol, on
`codex/drive-sync-foundation-2026-10-15`, based on
`0131d650706ee1a227f168015c07778d08623cb8`.

The native controller can resolve a configured registration, verify its account
result in the existing supervisor, and immediately discard the entire sealed
candidate before publishing `VerifiedNotAdopted`. Authorization and verification
have separate once-only terminal records. Raw starts retain pending-only
behavior, and the existing worker/ticket cleanup fence remains intact. No
connected credentials, watcher, application connection or configuration loader
is introduced.

Astra authored C2. Sol independently reviewed the production flow and added eight
permanent grouped probes with its own bounded localhost fixture. The lead read
the full author and independent reports, production changes and all final new
tests. The author delivery's twenty artifacts and 654 original inputs were
verified; only session.rs changed. The unified patch independently reconstructed
the entire delivered source. The independent delivery's fifty artifacts, two
repository deliveries, 656 protected inputs, exact inventory and actual binary
were then verified. Its focused 49-test, format and strict-lint gates passed
without repair, suppression, rerun or expectation change.

## Canonical gates

The exact source, test-only footer, permanent probe and receipts were imported;
the README describes verification followed by discard. Sol ran once:

```text
cargo test --locked --offline --lib --bin validate_dcp --test drive_auth_pkce --test drive_auth_loopback --test drive_auth_token_response -- --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
```

Pass: 413 library, 2 CLI, 10 loopback, 7 PKCE and 10 token-response tests:
**442 passed, zero failed, 14 ignored**. Formatting and strict all-target lint
passed. These are five disjoint targets from one invocation; earlier 422/49
results and overlapping subsets are not added. The ignored tests were not run.

The lead read the full [canonical receipt](GD1B5-C2-CANONICAL-VALIDATION-2026-10-16.json)
and rehashed all 37 sealed private artifacts, the external report, all 659 frozen
inputs, all five preserved binaries and the exact 660-path inventory. The full
112,397-byte author prefix and the 118-byte test-only footer remain intact. This
final note is the only subsequent documentation addition; tested executable
bytes are unchanged.

Canonical report SHA-256:
`292fc2898950c7fde59538f8b7de73ef2315c99bb5af179cfa1703134c003f60`.
Private manifest SHA-256:
`630a754535cecfbd88191175a894089ab14c2eae3970b31a0e428e3fbc7a2e5b`.
Evidence:
`/private/tmp/rapidraw-gd1b5-configured-verification-canonical-validation-20261016`.
The cache was explicitly released at `2026-10-15T19:01:11.530384Z`.

## Boundary and next package

Synthetic post-exchange headers, HTTP labels, manual reducer reaping and future
clock comparisons have bounded meanings. Separate public listener/ticket tests
exercise resources they actually start. Neither establishes the full
browser/code/exchange/account chain, provider mapping, actual Google grants,
zeroization, hard abort or product acceptance.

Connected adoption/retention, real catalog activation, browser/IPC, storage,
Android, editor integration, transfers and sync remain Not exercised. The
separate Fold profile/export and Adobe fidelity gates remain open. This local
integration does not merge main, publish or deploy an application.

C3 must keep connected retention inseparable from expiry, cancellation, faults,
watcher startup/finalization and shutdown. It must preserve task-free construction
and independently migrate C2's intentional discard-success expectations to the
new ownership semantics while retaining all negative checks. No retention code
is accepted by this note.
