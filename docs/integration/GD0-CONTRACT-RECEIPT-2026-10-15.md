# GD0 contract receipt — 2026-10-15

**Delivered for independent design review.** Task `01a13f34-3c0e-7a03-b4b3-f1ebcfec38da`; actual model `gpt-6-astra` per Team roster. Worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-contract`, detached base `5ca276ac1dab210f11ee4cecbf904038516164f8`.

Only new `docs/drive/contract.md`, this receipt and private `/private/tmp/rapidraw-gd0-contract-20261015/` evidence were written. No source/lock/Git/account/login/project/resource/build/cache/device/UI mutation. Google primary documentation was browsed; no authenticated provider request or account data was accessed.

## Frozen delivery

- [Contract](../drive/contract.md): SHA-256 **`c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`**, 43,785 bytes.
- Private input/check manifest `/private/tmp/rapidraw-gd0-contract-20261015/manifest.json`: SHA-256 **`a2d22181b4a47423fef38120eef1c09d6aaa2d3ba72bcd6ed94aaa2aee8b68ca`**. Contains 25 source/research/map input hashes, primary URLs and exact final verification commands/exits. Receipt hash supplied separately to avoid self-reference.

The latest user/lead scope takes precedence over older no-sync/no-profile-upload text. Settings, saved edits, named checkpoints and selected legitimately imported preset/profile libraries are included. Exact original profile bytes, SHA-256, DCP BLAKE3 ID, Look UUID, pairing and Amount are preserved. Original photo upload remains explicit; online-only photos download on demand.

Chosen design separates **explicit drive.appdata consent** for hidden internal state/libraries from **drive.file consent** for the visible photo workspace. Native account identity/generation guards, immutable revisions/blobs, record-level conflicts, causal tombstones, local durability/outbox and on-demand media hash binding are specified. No last-writer-wins, silent scope escalation, account retargeting, global profile replacement or remote-original deletion. appDataFolder's suitability for internal dependencies is labelled a design inference; same-project native visibility and policy fit remain gates.

Haiku's map SHA `af74957f1e50b32a186ad986dbc255c50ddfbb74474a05e83a98edc190cfe5b5` was read and verified. Independently confirmed source F1: frontend includes `includeProfile`, Rust Preset lacks it; save/reload loses the flag while selection survives in adjustments. Contract adds a bounded pre-sync round-trip repair and explicit legacy-unknown state. Corrupt settings/sidecar fallback and direct writes are static findings, not newly executed reproductions. Checkpoint persistence foundation is separate from full E4 history/provenance UI.

## Evidence and limits

| Check | Result |
|---|---|
| Frozen source identity / tracked diff | Pass: `git rev-parse HEAD` matches; `git diff --exit-code` exit0/empty |
| Source/persistence/profile seam audit | Pass — static reading only; paths/line anchors in contract §2 |
| Current primary Google docs | Pass — scopes, appdata, native OAuth, Android GIS, Picker, cross-client consent, about/files/changes, upload/download, quotas, policy and token practices checked on 2026-10-15; links near claims |
| New document whitespace | Pass: no diagnostics from `git diff --no-index --check /dev/null docs/drive/contract.md` (exit1 because new file differs); explicit Python trailing-whitespace check passes |
| Initial receipt assembly | Failed assertion expected no-index exit0; corrected expectation after observing exit1 with empty output. No production impact |
| Builds/tests/provider/security/device/Adobe/fidelity | Not exercised; none claimed |

The existing Fold receipt establishes API37/target36 and a prior staged-file mismatch, not free RAM, throughput or sustainable Drive capacity. Cache/buffer/transfer limits are labelled conservative provisional policy requiring measurements. No transient quota/pricing number becomes a product guarantee.

Current Google policy's explicit app-content-backup restriction and cache-retention constraint are recorded without assuming all editing/configuration sync is backup. Confirm the actual workspace/library/offline-copy use case before public enablement; pure/local implementation can proceed independently. Required future inputs are owner-controlled project/client IDs, enabled APIs, consent/branding/privacy details, exact Android package/signing SHA-1 and designated test-account consent through Google UI. No passwords, billing creation or operational approval was requested.

## Next consumer brief

Assign **GD1a only** first: new `src-tauri/src/drive/mod.rs`, `model.rs`, `state.rs` with inline pure tests; lead owns `lib.rs` registration and all shared manifests/locks. Validate bounded v1 envelopes/account/storage space, derive immutable graph heads, detect conflicts/tombstone races and reject stale acknowledgements. Payload applicability remains Unvalidated until the separate codec slice. No filesystem/network/auth/UI renderer work. Exact test cases, later isolated owners, commands and stop points are in contract §9; the preset round-trip prerequisite is separately assignable.

Unresolved acceptance decisions: actual cross-native grant/space visibility; provider policy/retention eligibility; secure-store and Android SDK dependency choices; exact portable adjustment/RenderContext codec after consumer audit; measured phone budgets; full E4 UX and E1 encoded-export integration. Neither completion of GD0 nor later mock success closes those gates.
