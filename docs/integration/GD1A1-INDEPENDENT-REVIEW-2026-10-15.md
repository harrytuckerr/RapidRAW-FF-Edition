# GD1a1 independent review: strict revision-envelope model

**Verdict: ACCEPT for the bounded pure-model scope.** There are no P1 or P2 findings. Three P3 recommendations and four informational notes follow; none blocks freezing this API. This is **not** sync, ownership-authentication, payload-portability, security, provider or device acceptance.

- Reviewer: Claude Code - Opus (slot 01a13e02-3bea-7851-9534-71c7e85ef6f8), model claude-opus-5-5. Author of the source under review: Codex CLI - Astra (gpt-6-astra).
- Worktree: `rapidraw-fuji-team-20261015/drive-model-review`, detached HEAD `5ca276ac1dab210f11ee4cecbf904038516164f8`.
- All source was **read-only**. Only this report and private `/private/tmp/rapidraw-gd1a1-review-20261015/` were written. No source, lib, lock, dependency, Git, device, provider or auth change.

## Inputs verified (sha256)

| Input | SHA-256 | Match |
|---|---|---|
| `src-tauri/src/drive/mod.rs` | `f689981fc6c7f414dfb47e13e827b00234de1e17b72f6d08fc9b816430851481` | yes |
| `src-tauri/src/drive/model.rs` (1,035 lines, read in full) | `f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0` | yes |
| `src-tauri/src/lib.rs` (root-only `pub mod drive;`) | `33ed33787818079b1722f0891408fd9418afaec73af15eb217521294e6f85592` | yes, left unchanged |
| `docs/integration/GD1A1-MODEL-RECEIPT-2026-10-15.md` | `a1862c812e66a434b394eb3d74b55424f02d7da81dec9b2e0243d9a350de4509` | yes |
| GD0 contract `profile-preset-integration/docs/drive/contract.md` | `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19` | yes (§3 DTOs, §5, §7 read) |
| GD0 lead review | `e7075dbdb4c5a1f63402c4ade6e69d6d0e99874241e26eae1bad24c437af46a5` | yes |
| `src-tauri/Cargo.lock` / pinned ONNX runtime | `7ddd491a…765def` / `2b885992…19406a` | unchanged / present |

## Actual compile and test observations (first-ever compilation)

Run inside `src-tauri`; rustc/cargo 1.98.1, uid 501; exclusive `CARGO_TARGET_DIR=/private/tmp/rapidraw-integration-20261015/target`; `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true` (`logs/env.txt` `54d497ec…`). Free disk 26 GiB.

| Command | Exit | Observed | Log sha256 |
|---|---|---|---|
| `cargo test --locked --offline --lib drive::model -- --nocapture` | 0 | **14 passed**, 0 failed, 0 ignored (222 filtered out). No compiler warnings. | `drive-model-tests.log` `78ad1b94cde96bb18c71c651fc969408bf241f2765ae1b80980600276a04beb9` |
| `cargo fmt --all -- --check` | 0 | clean | `fmt.log` `e38302ae…48bf` |
| `cargo clippy --locked --offline --all-targets -- -D warnings` | 0 | no diagnostics; fresh 22:32 lib/test check metadata | `clippy.log` `e82028d51c92d993cf12138f36c26face26d7ec1693b1e9f8bbef68134413f45` |

The test binary is preserved as `drive-model-test-binary`, sha256 `29013c0151d64cf80bdb2f4882075426e4e586e547abd523dfe0c76eaf503bc2`. I read all 14 test bodies (`model.rs:657-1034`); they exercise what the receipt claims.

## Independent probe harness (private; frozen source unchanged)

`probe/probe.rs` (sha256 `8eeb472ac618cd9032ddac866b93c140f7f5960e6167f90b6e5f6504919e59f8`) includes the frozen `model.rs` read-only via `#[path]`. It is compiled with `rustc +1.98 --edition 2024 --test`, linking the already-built `serde`, `serde_json`, `sha2` and `uuid` rlibs from the cache. That needs no Cargo resolution and creates no new target. Binary `probe-bin` `c7cabe9e…b0b0`.

Final run: **31 passed, 0 failed**. That is the 14 frozen tests recompiled plus 17 probes (`logs/probe-run-final.log` `0297cea8…e3e9`).

The first probe run had **1 failure in my harness, not the source**. Case 6 of `p06` used a replacement pattern absent from the sorted-key fixture, so the input was unchanged and decoded successfully. I added an `assert_ne!` that each case changes the input and fixed the pattern. That log is retained as `probe-run-attempt1.log` `03e93fed…6a6c`.

| Probe | Result |
|---|---|
| p01 escaped root duplicate `"schemaVersion"` | DuplicateJsonKey ✓ |
| p02 duplicate keys inside a typed parent ref and inside a dependency BlobRef | DuplicateJsonKey (caught by the strict pass, before serde's per-struct duplicate-field check) ✓ |
| p03 duplicate key with an identical value | DuplicateJsonKey ✓ |
| p04 depth bombs within 1 MiB (400k `[`; 100k nested objects) | DepthLimit, no stack overflow ✓ |
| p05 root + 31 nested **objects** OK; 32 → DepthLimit | ✓ (exact boundary also with objects) |
| p06 errors from inputs carrying a marker in unknown-key / UUID / duplicate-key / control-text / truncated / path-like objectId positions | Neither `Display` nor `Debug` echoes input ✓ |
| p07 same hash and role with conflicting lengths | ConflictingBlobLength takes precedence over DuplicateBlobRef ✓ |
| p08 `bytes` `"0"` and `"18446744073709551615"` | accepted ✓ |
| p09 UTF-8 BOM, lone surrogate `\ud800`, invalid escape | rejected ✓ (BOM → InvalidJson) |
| p10 `schemaVersion` as `1.0`, `4294967297`, `"1"`, `1e0`, `true` | rejected ✓ |
| p11 `raw_sha256` equals an independent SHA-256 of the exact received bytes, including whitespace | ✓ |
| p12 flat 500k-element payload near 1 MiB | decodes in ~15–19 ms; stays `Unvalidated` ✓ |
| p13 observations (see I1–I3) | nil UUID, bidi/zero-width text, tombstone→settings all accepted |
| p14 same revision ID in `parents` and `requiredRevisions` | DuplicateRevisionRef ✓ |
| p15 `WrongSpace` checked before parsing; `SizeLimit` before space | ✓ |
| p16/p00 peak-memory probes (see P3-2) | measured |

## Challenge checklist (source evidence)

- **Duplicate decoded keys at root, nested and escaped levels** (`model.rs:612-627`). `next_key::<String>()` compares decoded keys against the map being built, at every nesting level, before `Value` could discard a duplicate. ✓
- **Bounds and limits.**
  - 1 MiB is checked before any parse or copy (`:526`).
  - Depth counts containers including the root (`:558-565`), and the exact boundaries are tested.
  - Refs ≤ 32 each (also across parents + required) and dependencies ≤ 256 (`:444-451`), with no truncation.
  - No preallocation from the input: `Vec::new()` / `Map::new()`, and serde_json gives no length hint.
  - Trailing input is rejected (`deserializer.end()`, `:540`). ✓
- **Canonical forms.**
  - UUID: length 36 plus a lowercase hyphenated round-trip (`:94-106`).
  - SHA-256: lowercase 64-hex (`:55-68`).
  - u64 decimal strings with no sign, no leading zero, length ≤ 20 and a checked parse for overflow (`:134-150`).
  - Zero and max are accepted. ✓
- **Raw bytes and hash vs reserialization.** `raw_bytes` and `raw_sha256` come from the received slice (`:544-548`); typed serialization is separate, and the test plus p11 confirm it. ✓
- **Identical vs conflicting references.** Revision refs: same ID with the same hash → Duplicate, with a different hash → Conflicting; self-reference rejected (`:452-464`). Blobs: same hash with a different length → ConflictingBlobLength; same hash and role → Duplicate; equal content in different roles is allowed (`:465-478`). ✓
- **Storage and kind mapping.** `BlobKind::required_space` sends originals and exports to Drive and all else to appDataFolder (`:238-245`). `decode_revision` accepts only appDataFolder, and every dependency must be internal (`:529`, `:468`). This matches contract §5 (`:102`). ✓
- **Object identity.** Fixed or kind-plus-canonical-UUID namespaces, with `edit:<mediaId>:<variantId>` (`:290-316`) per contract §3 (`:70`). A kind mismatch fails, and tombstone targets an existing object ID (`:439-443`) per contract `:114`. ✓
- **No error or debug leak.** `EnvelopeError` carries no data and serde error text is discarded (`:539-542`). `ReceivedRevision` Debug shows only the length and `Unvalidated` (`:514-521`). `RevisionV1` and `UnvalidatedPayload` implement no Debug. ✓ (p06)
- **No fake Ready.** `PayloadApplicability` has only `Unvalidated` (`:338-341`). The payload field is private, and `RevisionV1` cannot be constructed or deserialized outside `decode_revision` (`RevisionWire` is private). ✓
- **Identity syntax ≠ ownership; opaque JSON ≠ portable.** This is documented in code (`:1-6`, `:180`, `:210`, `:343-344`, `:523-524`) and in the receipt. `AccountKey` is syntax only, and the opaque payload is explicitly not a codec (contract `:92`). ✓
- **Scope.** No graph, store, ack, provider, network, filesystem or UI code is present. `mod.rs` exports only `model`. ✓
- **Dependency features.** The tests prove serde_json is not built with `arbitrary_precision`, which would route numbers through `visit_map`. `preserve_order` does not affect duplicate detection or round-trip `Value` equality.

## Findings

**P3-1: display text accepts invisible and bidirectional format characters.**
- Location: `model.rs:159-162` (`BoundedText`).
- Repro (p13): `createdAt = "2026\u{202e}10\u{200b}"` decodes OK. `char::is_control` rejects only Cc, not Cf (bidi overrides, zero-width).
- Consequence: a received `engineContract` or `createdAt` could visually spoof itself if shown verbatim. There is no integrity impact, since these fields are display-only per contract `:70`.
- Smallest fix, in a later slice or at the display layer: also reject `General_Category=Cf` bidi/format characters (U+200B–U+200F, U+202A–U+202E, U+2066–U+2069, U+FEFF), or sanitize where rendered. This is not required for GD1a1 acceptance.

**P3-2: memory amplification of about 45–95× per accepted 1 MiB manifest.**
- Location: `decode_revision` / `StrictValue` (`:532-548`). The whole `Value` tree is materialized, then moved into the typed wire struct, and a copy of the raw bytes is kept.
- Repro (peak RSS of an isolated test process, `/usr/bin/time -l`):
  - baseline small fixture: 2.18 MB (`rss-p00…` `320131e2…`);
  - 500k-element flat payload near 1 MiB: **42.7 MB** (`rss-p12…` `0bc69f08…`);
  - 120k small objects near 1 MiB: **98.0 MB** (`rss-p16…` `678a38c4…`).
- Within contract §7 limits (1 MiB, nesting 32). However, contract §7 also plans up to 10,000 revision headers per reconciliation batch on Android.
- Smallest fix, in the next slice and not here:
  - decode manifests sequentially (never batch-hold decoded payload trees);
  - optionally add a total-node budget to `StrictValue` (a shared counter, e.g. ≤ 100k values) mapped to a `SizeLimit` / `ResourceLimit` error;
  - measure on the Fold before choosing the number.

  Do not change the 1 MiB / 32 limits silently. The receipt already discloses that allocator exhaustion is not promised to be recoverable.

**P3-3: decode failure does not hand back bytes for quarantine.**
- Contract `:36` requires retaining incompatible downloaded bytes quarantined for recovery. `decode_revision(&[u8], …)` borrows the input and returns only an error, so the caller still owns the bytes.
- No defect in this slice. The future store or ingest slice must persist the raw bytes on every `Err` before discarding them, and its tests should assert that.

**Informational:**
- I1: The nil UUID `00000000-0000-0000-0000-000000000000` is accepted as a workspace/revision/device ID. This is syntax-only by design.
- I2: `kind: tombstone` may target singletons (`settings`, `workspaceBinding`). Whether that is semantically meaningful belongs to the graph slice.
- I3: `engineContract` / `createdAt` may contain path- or URL-like *values*. Contract `:36` forbids path/URL *fields*; these are display text.
- I4: The same revision ID in both `parents` and `requiredRevisions` is rejected as a duplicate (p14). That is consistent with contract `:56` (cross-object refs are separate from ancestry, so they cannot legitimately coincide).

## Gates

| Item | Status |
|---|---|
| Source and inputs hash-verified; full source and tests read | Pass |
| First compile, 14 `drive::model` tests, fmt, strict all-targets clippy | **Pass** (actual runs) |
| Independent probes: duplicates (root/nested/escaped/typed), depth/size, canonical forms, raw hash, ref/blob semantics, space mapping, no leak, Unvalidated | **Pass** (31/31; one harness bug disclosed and corrected) |
| Memory amplification measured | Recorded as P3-2 (not blocking) |
| Graph/ancestry, ownership authentication, dependency content, payload codec/portability, provider/device/security acceptance | **Not exercised**; out of scope |

**Cache `/private/tmp/rapidraw-integration-20261015/target` is RELEASED.** No cargo, rustc or probe process is running.
