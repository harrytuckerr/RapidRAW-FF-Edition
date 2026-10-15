# GD1b2/3 canonical integration acceptance

Lead/reviewer: actual gpt-6.1-sol. **Accept for local integration on the isolated Drive branch.** Base `cc397890144615938f6036bea2e2e7b507206050`; combined input manifest `774ace57188b6c2335c2567167fcedc241dce1ca165f01047572e602e2e1470a`.

The batch adds the reviewed loopback callback listener and corrected native token-response parser, exports them without registering application commands, and retains the independently written public-API probes as permanent tests. Existing PKCE, editor/profile/renderer/Android code and dependency manifests/locks remain unchanged. README describes the tested components and explicitly keeps Google connection/transfers/sync unavailable. The bounded next exchange design is included as documentation, not implementation.

| Combined gate | Result |
|---|---|
| Default library | Pass288;14 ignored |
| Permanent independent PKCE | Pass7 |
| Permanent independent loopback | Pass10 |
| Permanent independent token-response | Pass10 |
| validate_dcp CLI | Pass2 |
| Format and strict all-target Clippy | Pass |

These are317 executed passing tests across five disjoint targets in one Cargo invocation. Author/focused subsets are not added again. Root read the entire canonical machine receipt `a5425c06c428f84f866b4da4e2e8a81a07883073d959460bec797bbe3e05d620`, actual results/logs, and independently rehashed all36 records in artifact manifest `70b8c601a1626734a2fc91ca90fc4dc23e243a682fd69dc3d8df6369acd43b45` plus all602 frozen product inputs. Permanent test contents are the previously reviewed/executed bytes, with no assertion changes. The validator's source inventory changed only by its authorized new report.

Exact accepted listener `ee3b4ccbd31ff154a2aed9b51723d533c7ee944460a165b14375b42ed7678c41`, corrected parser `7543548a508ae0e1355f3358d6f182543b623d49544533f26be4fe01fee25886`, combined export `7e048aafd468efef5339d2ab0068da479bce456db96d038cd73f51848f1e85a0`. All commands used pinned Rust1.98.1, locked/offline dependencies, no debug/incremental output, known hash-verified ONNX Runtime without downloading, and one explicitly leased native cache. Cache release at2026-10-15T14:39:29Z and subsequent idle were observed.

The original listener-probe lint failure and original parser status-overlap lint failure remain Fail on their original bytes, with exact receipts and corrections retained. Corrected/combined success does not rewrite those results. See the [listener review](GD1B2-INDEPENDENT-LEAD-REVIEW-2026-10-16.md), [original parser failure](GD1B3-RUNTIME-VALIDATION-2026-10-16.md), [parser revision review](GD1B3-INDEPENDENT-LEAD-REVIEW-2026-10-16.md) and [canonical receipt](GD1B23-CANONICAL-VALIDATION-2026-10-16.json).

Actual localhost sockets, callback binding, cancellation/deadlines/resource limits and native synthetic token parsing were exercised. Google/provider authorization, actual token-endpoint HTTP/TLS, account/grants, credential storage/refresh, browser/Tauri/UI/Android/device, sync and ignored/GPU/vendor/platform-matrix gates remain Not exercised. A native code or parsed response is not authenticated account/grant proof. The listener's owned-worker/polling, best-effort response and future session-generation adoption contracts remain explicit.

This acceptance is a local source commit, not a main promotion, release or product connection activation. This review document was added after validation; executable inputs remain exactly the tested frozen bytes. Root alone stages and commits the reviewed paths.
