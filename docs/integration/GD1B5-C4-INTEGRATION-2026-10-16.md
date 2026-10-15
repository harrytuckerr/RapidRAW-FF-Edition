# C4 integration acceptance — native account-switch consent

2026-10-16. Root Codex CLI, actual model gpt-6.1-sol. Accepted on the local `codex/drive-sync-foundation-2026-10-15` branch from parent `47a1a46b2fec95826ffe5e71e39b4b5889211919`. This final note closes the Pending canonical gate in the frozen import review; that earlier review remains unchanged.

A different verified account now produces an expiring proposal while the incumbent remains retained. Exact native Accept or Decline identifies the attempt, generation and decision. Acceptance rechecks the binding under the commit mutex and moves the whole candidate with its actual granted scope subset. Decline, cancellation and proposal timeout preserve the incumbent under its own expiry. Confirmation is capped by the original admission deadline, original token expiry and 60 seconds after publication; retained credentials remain capped by token expiry and 3600 seconds. The original supervisor and owned watcher enforce cleanup. This is native foundation behavior, without application activation.

## Canonical validation: Pass

Sol/gpt-6.1-sol executed each authorized command once using Rust/Cargo 1.98.1, offline with the verified existing ONNX runtime and no download:

| Target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Library | 450 | 0 | 14 |
| validate_dcp CLI | 2 | 0 | 0 |
| Loopback integration | 10 | 0 | 0 |
| PKCE integration | 7 | 0 | 0 |
| Token-response integration | 10 | 0 | 0 |
| Disjoint total | 479 | 0 | 14 |

The single five-target Cargo test command exited 0 in 183.187787 seconds. `cargo fmt --all --check` exited 0 in 0.830702 seconds; `cargo clippy --locked --offline --all-targets -- -D warnings` exited 0 in 17.419453 seconds. The 14 ignored cases were not executed. Historical review runs and the amended targeted case are not added to these counts.

Root independently rehashed all 52 sealed records, including all five actual executed binaries and the standalone report, and compared all 493 raw test names/outcomes with the report. All 475 prior cases and their outcomes remain, with 18 new passing C4 cases. All 686 frozen input bytes, exact pre/post inventory, HEAD, index, pointer and branch ref matched before root staging. The canonical report alone added the 687th product path; this final note adds the 688th. The intended commit consists of three modified paths and thirteen new paths, with generated schemas and the ignored runtime excluded.

The self-contained evidence is [GD1B5-C4-CANONICAL-VALIDATION-2026-10-16.json](GD1B5-C4-CANONICAL-VALIDATION-2026-10-16.json), 613530 bytes, SHA256 `ac70c9485efaa2f9eb347f4f28340598e810e2c70302a37181468bdea363c349`. It embeds every actual outcome, complete input custody and all five preserved binary records. Private evidence is `/private/tmp/rapidraw-gd1b5-switch-canonical-validation-20261016`; artifact manifest SHA256 `8658dbb3c4dc3a26f2339cacab86de4968cdeea77dea76acadd0d751f4c8d653`. Root verification is `/private/tmp/rapidraw-gd1b5-switch-canonical-root-20261016/FINAL-ROOT-VERIFICATION.json`.

The native cache lease was explicitly released at `2026-10-15T23:43:55.255197Z`, with no compiler processes and 16670236672 bytes free. No additional build or test command was needed for this root acceptance.

## Source and evidence limits

Accepted session source is 209473 bytes, SHA256 `ef4726e1b53c8cf59d94ad4d0e9d86e58597bf827c8bd1bc08e26076545773fa`; it preserves the complete Astra author prefix plus the exact test-only child registration. The migrated C3 child is `04bf5d97950e7cc33716ea297ad169eac06780da4b97b07e23b8a42e4a790444`; the final C4 child is `a35cbb7342db2d4d4b238dd99e7b49febdfebe2702bc76072e3a1a50be200446`. README changes only the native consent status sentence. Other accepted source, dependencies and protected test inputs remain unchanged.

The original 86-case review Pass retains its timer-isolation qualification. The separately reviewed test correction moved synthetic reap after raw expiry observation; the single amended existing case then passed, closing that intervening cleanup/notification gap in the observed local run. It is not an additional unique test. Forced old-timer capture or a particular executor interleaving was not demonstrated. Earlier source/runtime/lint failures and disclosed private orchestration/path deviations remain historical evidence; they are not relabelled.

Not exercised: Google/provider authentication or project attestation, a complete positive OAuth supervisor chain, browser/IPC/UI activation, Android/device workflows, secure storage, refresh/revocation, credential leases, photo transfers or cross-device sync. Google sign-in and sync remain unavailable in the application. This acceptance performs no main merge, push, deployment or release.
