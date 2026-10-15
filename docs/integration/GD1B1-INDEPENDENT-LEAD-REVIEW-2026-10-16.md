# GD1b1 independent source review

Reviewer: lead, actual `gpt-6.1-sol`. Reviewed 2026-10-16 Australia/Melbourne. Verdict: **Revise before integration**. Static review only; the separate runtime validation task has not yet returned.

Reviewed base `e8113f9c49d064ea0ef8c6c8e0effe3c67c6bed4`, original `drive_auth/pkce.rs` SHA-256 `50d623cfe07972c843351339ba1d5e3f6810e40dee0b5e7788514f8c2f00f9c4`, module export `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8`, author receipt `8d26fa9ea8bbfefc427f2c8a48c314e9b94187060f07647f7fa3bd07fa3e8488`. Read the complete helper, all 12 test definitions, receipt and manifest. After the author began the explicitly requested correction, the original was reconstructed in isolated review snapshots from the hash-verified private source patch `f56bc755db4fad44c6613191a666f4c027715e37da90a5cea2bf86b461dfc81c`, with its original source hashes verified. The changing author path is not treated as the frozen review input.

## F1 — P2: reject-all callback field allowlist prevents extension compatibility

Location: `PkceTransaction::callback_parameters`, the known-key `matches!` branch; the original malformed-input test also expects `unknown=x` to fail.

Reproducer: construct a transaction and its otherwise valid, exact-bound callback with the matching state and `code=synthetic&provider_extension=synthetic`. The parser returns `InvalidCallback` solely because of the extension key; the transaction remains Pending. This is directly established by source inspection, not yet an executed Rust reproducer. It is not evidence that Google currently emits this particular synthetic field.

[RFC 6749 §4.1.2](https://www.rfc-editor.org/rfc/rfc6749#section-4.1.2) requires clients to ignore unrecognized authorization-response parameters. A standards-conforming provider extension can therefore prevent a successful login in this implementation. Failing closed is safe from accidental authorization here, but it is an interoperability defect.

Smallest correction: ignore well-formed unknown response fields without using them as identity, scope or authorization evidence. Preserve strict binding, percent/UTF-8/control checks, resource bounds, duplicate detection, state validation, code/error exclusivity, issuer/scope policy, expiry and replay protection. Add real-helper regressions for a valid code and matching denial carrying an ignored extension, while encoded duplicate known fields and mismatched state remain rejected. No new dependency, HTTP listener, token exchange or scope expansion is needed.

The lead explicitly requested this correction within the author's existing source-only ownership. The original rejected candidate remains reproducible through its private patch; a new revision hash and receipt are required before acceptance.

## Other source findings

No other blocking source defect identified in this review. Static checks support the following narrow conclusions:

- Production generation calls fallible OS entropy, splits separate 32-byte state/verifier values and uses S256. The RFC 7636 fixture is fixed and independently published; no weak entropy fallback is present.
- Requests use only the exact approved state/photo scope sets and the official Google endpoint. The installed-app incremental-scope clarification is honored. Requested scopes and callback hints are explicitly not granted-access or account proof.
- Raw and parsed redirect checks bind IPv4 loopback, actual asserted port and exact path; normalized path/host aliases, fragments and userinfo are rejected. The future HTTP listener still has to establish its own socket, method, Host and request-target boundary.
- A valid code/error consumes the transaction, malformed/mismatched input does not extend its deadline, and cancellation/expiry/clock regression are terminal. Mutable transaction access supplies local serialization; this does not establish an HTTP concurrency implementation.
- Secret-bearing types have no Debug, Display, serialization or Clone implementation. Error variants contain no received text. Clearing/dropping strings is not a guaranteed zeroization mechanism, as documented.

These are source-level observations, not independently executed entropy, parser or security acceptance. Client-ID syntax checks do not prove registration. Authorization-code receipt is not authenticated identity. The selected helper resource limits require later provider compatibility tests.

## Acceptance boundary and next consumer

Pass: complete static review, primary RFC requirement checked, original source/receipt/patch identities bound. Fail: F1 on the original candidate, statically reproduced. Not exercised: compilation, 12 inline tests, independent runtime probes, listener/browser/token/secure-store integration, Google consent/grants, Android, cross-device sync and device operations.

The separate validator owns only its isolated original-source snapshot and the explicitly leased desktop build cache. The author owns the correction in its own worktree. Root retains acceptance and integration authority. Re-review the revised exact diff, then run focused tests/probes and combined checks before importing; do not label the helper as a working Google connection.

## Revision 1 source re-review

The author froze corrected helper SHA-256 `85bd9b4882a9fb45d54c63231b22bf962c322d884f9aa84ca67f866e551e7dd8`, unchanged export `c744895383a3048117ebbf250ddf4e3cf34441b8ab013c715bf11f2dd45bd7b8`, and refreshed receipt `2610413dce7d9ee805c90888c611a38dd8bdfba664fc19f8746213811643f7c2`. Root verified the revision manifest `a67c3596d1c59a950272f9d7bb0bd65606418745ed0ee7d302a9134fd53e6ef0` and its nine referenced file hashes/sizes, including preserved original source/receipt and exact correction patch `111259a0e7b82b3417840472a24116f5cbc078c1286290bc0c27ac24feeccd46`.

The reviewed production diff replaces only the known-key allowlist with RFC 6749 extension-name validation. Unknown fields stay in the bounded temporary map for duplicate detection; only the pre-existing semantic keys are consulted. The new code and denial tests include extensions; existing encoded-key duplicate, scope, identity, state and resource protections remain explicit. Other production helper bytes are unchanged. Fourteen author test functions are now defined, not yet executed.

F1 is corrected by static inspection. No new blocking source finding identified in this bounded re-review. **Revised-source acceptance remains pending compilation and independent runtime probes**; the original validation task must not be presented as testing the revised bytes. Root imported the corrected frozen source into a separate revision snapshot and added only its module registration for the later compile. Neither snapshot exposes commands or performs Google authorization.

## Original runtime reproduction

The independent validator subsequently compiled the original exact helper in the project. Root read the actual author-test, independent-probe and strict-Clippy transcripts under `/private/tmp/rapidraw-gd1b1-original-validation-20261016/`, as well as the complete seven-probe source. The 12 supplied tests passed, formatting passed and strict library/test Clippy passed. The independent suite exited 101 with six Pass and one Fail: matching state/code plus the synthetic extension returned `InvalidCallback`, status `Pending`. Its expected-success assertion was preserved; no panic expectation or result relabeling hid the failure.

The public-API probes exercise production OS entropy, request wire fields and exact selected scopes, independent S256 recomputation from the returned verifier, state/redirect binding, single-use code/provider errors, cancellation/clock/expiry and secret-free diagnostics. They use synthetic callbacks and no sockets/provider traffic. The root prepared a byte-identical copy of this probe in the corrected review snapshot for the next run. These observations establish an executed original-source reproduction of F1; they establish no revised-source runtime result. The validator's final frozen receipt and source-custody hashes are still required before integration.

## Revised helper acceptance and canonical import

The corrected source subsequently passed all 14 supplied functions, the unchanged seven independent probes, the full default library suite (258 Pass, 14 ignored), formatting and strict all-target Clippy. Root read the complete final report `888b0b2ee4c6284384f4c7839090536b0519c9e0d2a842dfa92d27fa58d0eff0`, verified machine receipt `0ee10bd6c623a1121d28bc6d34a328891741a4f3a22c353e622fa550c4a48285` and all 37 records in artifact manifest `0ca8684fb709d11a17f197725f921f5cb9ecc58d1190eb3f77c2c400e6975e75`. The original validation's 29 artifact records were independently verified as well.

Root directly replayed the preserved original binaries (12 Pass; independent six Pass/one Fail) and corrected binaries (14 Pass; independent seven Pass). Corrected binaries were copied byte-identically into a lead-owned executable path because the preserved validator copies lacked execute permission; their bytes and metadata were left untouched. Corrected hashes: library `93602526a9eab2734da128297a57df667ed22028b8a225c451aba2240aa9d307`, independent `d80e35d65e4b589731fe7f906d1e7e7916a8b50459aecdd39a7aacc5ce619bce`. Lead replay receipts/logs are under `/private/tmp/rapidraw-gd1b1-lead-replay-20261016/`. No build-cache, product-source, socket or provider operation was needed for those replays.

**Accept the corrected native helper for canonical integration. F1 is resolved on the revised bytes.** It does not establish live OAuth, identity, usable provider grants, secure storage, browser/listener behavior or Drive sync. The original rejected candidate and its failure evidence remain preserved.

Root imported the exact corrected source and author/original/revised review receipts into the isolated Drive foundation branch, preserving the merged editor changes. The seven independent probes are retained as permanent `src-tauri/tests/drive_auth_pkce.rs` regressions. Only their first comment and Rust formatting changed; root compared the complete formatted body against the original and proved it byte-identical. No assertion or expectation changed. The import receipt is `/private/tmp/rapidraw-gd1b1-canonical-import-20261016/probe-import.json`. Combined-source checks remain a separate final gate before commit. Listener and token-response work are excluded from this import.

## Canonical gate and local commit acceptance

The combined canonical input at pre-commit HEAD `75be1370087119b773d632488986e16201870b14` passed 258 default library tests (14 ignored), all seven permanent independent tests, both CLI tests, formatting and strict all-target Clippy. The 14 author functions are included in the library count. Root read the complete canonical report `fddaba2af4d9c1641a6ece2bea581e3585748eeb7b56bde77a8964fb7a175f8b`, verified manifest `6eea7b2d2b6ce106dfcb947010d706ce8b6a1ba248f905390035a65b7f9e1210` and all 37 evidence records, and independently rehashed all 587 frozen inputs against `7ce5350bbd0b4f7ec359e1b668c6ee25be972935847e33d22a8fa7ca553427db`. No source changed during validation. Cache release is recorded at 2026-10-15T13:55:03Z.

**Pass for local native-helper integration.** This final review paragraph is documentation added after validation; executable inputs remain the tested bytes. No Google sign-in, listener, token exchange, credentials, account, UI, Android, device or Drive operation was exercised. This batch is committed only on the isolated Drive branch; it is not a new main promotion or release.
