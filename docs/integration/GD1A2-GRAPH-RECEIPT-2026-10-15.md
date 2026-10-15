# GD1a2 — pure ancestry graph source receipt

**Source ready for independent review; compilation/tests Not exercised.** Task `01a13f76-d275-7683-8811-727214da5558`; actual model `gpt-6-astra`. Owned worktree `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/drive-graph`, detached base `4710485193650fe881509c239f2cfe770bf9cead`. The assigned directory was initially absent; lead created it before any write. Worker performed no Git mutation.

Only new `src-tauri/src/drive/state.rs`, one `pub mod state;` export in `drive/mod.rs`, this receipt and private `/private/tmp/rapidraw-gd1a2-graph-20261015/` evidence were written. Model/lib/Cargo/lock bytes match HEAD exactly. No Cargo, rustc, test binary, native/build cache, dependency, npm, provider, account, filesystem-runtime/store, UI, renderer or device action occurred.

## Frozen custody

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `src-tauri/src/drive/state.rs` | 29,229 | `4aa951702419bd31cf50c5581071167657852a20ac754941f39d7d49c146acbb` |
| `src-tauri/src/drive/mod.rs` | 116 | `f46c6145370f504089e16a6db9d55435e7bd465c4a2036685015abb285b85c14` |
| Private `source.patch` | 30,464 | `a5482828a08115780cf7b7e135440b05bb9df50398512cf969f46d8913b1b907` |
| Private `manifest.json` | 3,884 | `3b0c10c21c1cb095ba42321286a4d373dbc545dffeead9ec0c08c552a557bcd5` |

Receipt hash is supplied separately. GD0 contract `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19` and accepted GD1a1 independent review were read; exact input/protected-file hashes and verification argv/exits are in manifest.json. Frozen model remains `f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0`.

## Implemented API and semantics

`RevisionHeader::from_received(account, &ReceivedRevision)` extracts only account/workspace/object identities, exact received revision ID/SHA-256, declared kind, parents, required-revision refs and blob refs. It retains **no raw JSON, Value tree, display text, clock or device-sequence data**. The caller can drop the decoded payload immediately; sequential decode/store integration is future work.

`RevisionGraph` is one explicit account/workspace partition with at most **10,000 headers**. `ingest` returns Inserted or exact-ID/hash Duplicate; another hash for the same ID is RevisionCollision. ScopeMismatch rejects foreign input without retargeting the index. Collision/capacity failures latch: subsequent analysis fails rather than treating the accepted prefix as complete. No winner-selection/reset method exists.

`analyze(known_blob_length)` checks present reference hashes and same-object parent edges, then uses iterative Kahn traversal over parents **and** required dependencies. Missing parents/required refs/blobs generate explicit counts and propagate blocking to descendants. Known blob hash with mismatched length is corruption. A cycle or cross-object parent/hash mismatch fails analysis. Required-revision dependencies may cross objects inside the partition; they do not create ancestry or remove another object's heads.

Only fully structurally resolved descendants supersede their parents. A pending descendant leaves known resolved ancestor heads visible. Results retain per-object pending revisions separately from None/Single/Conflict structural heads; therefore even a Single head accompanied by pending descendants is not a resolution candidate. Object/revision output order is deterministic and independent of insertion order. No timestamp or lexicographic ID chooses a winning payload; sorting is presentation only.

`GraphAnalysis::resolution_parents` returns **all** observed resolved heads for that object, or PendingInputs/TooManyResolutionHeads/UnknownObject. It refuses to truncate more than32 heads. The plan is for the analyzed snapshot only; a newly observed concurrent branch remains a conflict after ingesting a resolution based on an older snapshot. It neither selects a payload nor performs a save.

**Tombstone boundary:** declared `kind=tombstone` uses the same object/parent ancestry, so only causal ancestors are superseded and a concurrent edit remains a conflict. The frozen envelope's opaque payload is still Unvalidated. This graph cannot certify payload `targetObjectId`/`observedHeads`, perform deletion, decide whether removing settings/workspaceBinding is meaningful, or authorize a restore. It reports a **structural head with declared kind**, not “deleted” or Ready. No model change or guessed winner was introduced. Restored payload/value semantics remain a later codec/user-resolution boundary.

The blob callback is a stable pure inventory lookup supplied by a later caller; this code does not fetch/hash/authenticate content. Classification is relative to supplied headers, not proof of a complete provider listing. Account association is recorded, not authenticated. Clean/dirty local state, payload portability and renderer compatibility remain outside this slice. No Ready or apply/upload method exists.

## Defined tests and actual verification

**17 inline test functions defined; zero executed.** Coverage definitions include all six permutations of a fork with duplicate ingest; pending descendants retaining known heads; missing ancestor without false tip; actual differing raw bytes and whitespace collisions; account/workspace rejection; cross-object-parent rejection versus legal required dependency; direct/transitive missing refs/blobs and wrong length; reference hash mismatch; cycles; all-head resolution and a later unseen branch; causal tombstone/concurrent edit/restore shape; 33-head refusal; 10,000 accepted headers, duplicate at cap and explicit overflow; a 10,000-deep iterative chain; and payload-free header extraction.

Cycle tests deliberately inject corrupt *private test headers* after decoding because mutually self-consistent raw-hash commitments are not practical fixture JSON. This exercises the defensive graph check without pretending to supply a live-provider cycle or bypassing the production header constructor. Other ordinary ancestry/identity tests derive headers through the real frozen decoder.

| Check executed | Result |
|---|---|
| `git rev-parse HEAD` | Pass, expected47104851 base |
| Protected files compared byte-for-byte to `git show HEAD:<path>` | Pass: model.rs/lib.rs/Cargo.toml/Cargo.lock unchanged |
| Tracked diff inventory | Pass: mod.rs only, one insertion; state.rs new |
| `rustfmt --edition 2024 --check --config skip_children=true <mod.rs> <state.rs>` | Pass, exit0; syntax/format only |
| `git diff --check`; no-index whitespace check for new state.rs | Pass, no diagnostics (no-index exit1 denotes added file) |
| Compile, tests, clippy, performance/peak-memory measurement | **Not exercised** |
| Store/auth/provider/Android/security/render acceptance | **Not exercised** |

Environment: Darwin25.5.0 arm64; rustfmt1.9.0-stable (`8bab26f4f6`,2026-07-14). Header limits imply at most640,000 parent/required edges and2,560,000 blob-reference checks. Analysis uses sorted borrowed headers, reverse adjacency and counters rather than recursive traversal or an ancestor-closure matrix. These are source bounds, not measured phone memory/performance guarantees; maximum-sized headers still consume memory and allocator exhaustion is not promised recoverable.

## Next consumer and stop

Root alone integrates this frozen diff after independent review and assigns the native cache. Future command from `src-tauri`: `cargo test --locked --offline --lib drive::state` (17 new definitions), followed by the already accepted model suite and appropriate fmt/strict lint checks. The parent base already registers `pub mod drive`; this task does not alter lib.rs. Compilation may expose type/lint issues because source-only formatting is not compilation evidence.

Stop at this artifact. No acknowledgement generations, graph persistence, remote transport, portable payload codec, profile math, auth or UI work is bundled into it.
