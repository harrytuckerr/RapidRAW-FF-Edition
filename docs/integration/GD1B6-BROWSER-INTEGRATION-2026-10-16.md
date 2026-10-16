# GD1b6 native browser adapter integration — 2026-10-16

The root accepts the reviewed adapter, permanent browser regressions and exact desktop dependency/module wiring for the local Drive foundation branch, based on `6932d49ba6f989baf5b9ca2ae450b4846fec6512`.

The adapter consumes the native launch ticket, validates the generated seven-field authorization URL, starts only the selected default-handler recipe and owns its launcher child through known reaping. Caller-future cancellation cannot acknowledge cleanup prematurely. A successful handoff describes launcher exit and cleanup; it does not establish browser loading or authentication. The already-locked `open` 5.4.1 package is promoted to a direct desktop dependency, with no other package/version/checksum or feature-union change. This commit exports the desktop adapter module without activating application commands or UI.

## Actual canonical validation

One combined command selected the library, `validate_dcp`, loopback, PKCE and token-response targets. Actual disjoint outcomes:

| Target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Library | 473 | 0 | 14 |
| CLI | 2 | 0 | 0 |
| Loopback | 10 | 0 | 0 |
| PKCE | 7 | 0 | 0 |
| Token response | 10 | 0 | 0 |
| Total | 502 | 0 | 14 |

All 516 named outcomes were parsed. The prior 493 target/name/outcomes and exactly 23 browser definitions were preserved, without adding historical focused counts. The combined command exited zero in 239.764969 seconds; `cargo fmt --all --check` exited zero in 0.879403 seconds and strict all-target Clippy in 16.237670 seconds. Each command ran once, with no source/oracle change, repair or rerun. The 14 ignored tests remain **Not exercised**.

The self-contained [canonical validation JSON](GD1B6-BROWSER-CANONICAL-VALIDATION-2026-10-16.json) has SHA256 `05d63dce2e6c289e2df198b64db0d4a25ba5232d33477992f8f1928b44182648`. It embeds all 700 pre/post input hashes, inventories, metadata, commands/logs/environment/toolchain, all outcomes and five actual preserved binary records. Root rehashed its 93 sealed delivery/evidence records, compared all 700 current bytes and Git metadata, verified the exact 701-path final inventory and raw outcomes, and independently rehashed all five executables before acceptance. This final note is the only additional repository path after validation; tested bytes remain unchanged.

The exclusive native cache was released at `2026-10-16T01:20:18.685983Z`, with robust full-width process/PID-parent and fresh fixture/Cargo/executable-path checks finding no survivors and 14,991,114,240 bytes free. No additional native command was required for root acceptance.

## Authorship, history and limits

Astra authored production source; Sol independently reviewed it and authored the child tests. Root reviewed the source and the observer-only correction, imported exact files, and accepted the combined regression. The original interrupted browser run remains historical: 21 complete observations, one incomplete, one unstarted, no summary and Cargo return -2. Incorrect earlier pre-runtime wording was withdrawn. The successful revised focused run is separate, not a replacement or added count. Native observer acquisition/cleanup has no hard OS scheduling guarantee; unknown cleanup cannot become success.

**Pass:** native compilation/regression, exact URL validation, atomic decisions, controlled synthetic processes and bounded localhost denial routes. **Not exercised:** OS default browser invocation or readiness, Google/provider/account-project authentication, successful Code exchange/full positive OAuth chain, application/UI/IPC activation, Android/devices, credential storage/refresh, photo transfers or sync. No main merge, remote push, deployment or release occurs. Google sign-in and sync remain unavailable in the application; the next source boundary is reviewed native application ownership and a token-free command interface.
