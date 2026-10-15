# GD1a2 independent lead review — 2026-10-15

**Accept for the bounded structural graph only.** Reviewer: root Codex CLI / gpt-6.1-sol, independent of Astra’s source authorship and validation execution. This is not Google/provider, account-ownership, payload-applicability, store/durability or product acceptance.

Reviewed the complete production implementation, strict model boundary, frozen GD0 contract, author receipt and actual validation logs. Opus executed the first 17 graph tests but could not finish delivery; Astra’s takeover is labelled author-run validation, not independent review. The original strict-Clippy failure (ten cloned singleton slices in tests) is retained. Luna changed exactly those ten test-call expressions; the 14,072-byte production prefix remains byte-identical (`9f28c812057992f1d21ae01e46f08aa7a0eae80fc83061a8bfdd62f799a2b8b3`). No assertion, algorithm, model, identity, dependency or lock changes were accepted with that repair.

Final source: `state.rs` SHA-256 `52db27914e6209611d902473fa91e16dd2383fa34efcaf5777f4bc9e27517902`; export-only `mod.rs` `f46c6145370f504089e16a6db9d55435e7bd465c4a2036685015abb285b85c14`. Canonical parent `3b1d89e1eb977db29059b8faedac708a969e1bfa` already contains the accepted strict model and PS1.

## Independent behavior checks

Root generated 64 deterministic synthetic DAGs with two object kinds, missing headers/required revisions/blobs and tombstone markers. An independent Python oracle computes readiness by recursive transitive prerequisites and heads from full ancestor-set closure; production uses iterative indegrees and direct-parent supersession. Exact raw JSON hashes bind references before subsets and arrival orders are chosen. All 64 cases agree in six arrival orders each: 384 graph analyses. Pending objects refuse a resolution plan. Private Rust probe imports the exact final model/state source, pins the same four direct dependency versions as production, and runs 32 tests: the 31 retained model/graph tests plus this independent parameterized check. Both original and final-source probe logs are retained; final result 32Pass/0Fail.

Read-only reasoning also checked scope isolation; latched collision/capacity failures; present reference hash/object checks; blocking through required edges without inventing ancestry; deterministic conflict ordering; all-observed-head resolution; pending descendants retaining known heads; and the explicit distinction between a declared tombstone head and authorized deletion. No blocking runtime defect was found within this scope. A raw decoded header is not authenticated, and a structurally single head is never Ready.

## Combined imported source

Actual canonical checks: 244 library tests Pass, 14 ignored (not passes), formatting Pass and strict all-target Clippy Pass. Source/model/lib/manifests/locks hashes stayed unchanged during the run. Exact commands, durations, exit codes and log hashes are in the adjacent canonical JSON receipt. No application, device, provider or renderer was exercised. Shared desktop cache is released after these commands.

## Limits and next consumer

The blob callback is a caller-verified inventory assertion, not content hashing. Opaque payloads remain Unvalidated. Cross-object required revisions are allowed within the partition; parent ancestry stays same-object. Scope errors must be handled by callers; neither UUIDs nor typed account keys establish ownership. Classification assumes supplied headers, not complete Drive listing. The 10,000-header cap and worst-case reference memory need device measurement before production enablement. No concurrent-state application, stale-ack handling, journal, auth, sync, schema codec or deletion behavior is implemented here.

Root may commit this reviewed foundation. It remains separate from the UI1 desktop/Android artifacts and does not create a working Google connection or change profile rendering. Future adapters require their own bounded implementation and review.
