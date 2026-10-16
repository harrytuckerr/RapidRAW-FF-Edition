# GD1a1 — strict revision-envelope source receipt

**Ready for independent source review; compilation and tests Not exercised.** Task `01a13f45-885c-72b1-95b2-4ed1aef7a3e2`, actual model `gpt-6-astra`. Snapshot `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-model`, detached base `5ca276ac1dab210f11ee4cecbf904038516164f8`.

Only two new module files, this new receipt and private `/private/tmp/rapidraw-gd1a1-model-20261015/` evidence were written. Existing production files, `lib.rs`, Cargo/package/locks and caches remain unchanged. No Cargo/rustc/test binary, dependency resolution, runtime filesystem/store, auth, network, UI, renderer, device or provider work was performed. Native cache belongs to PS1; the module deliberately remains unregistered.

## Frozen files

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `src-tauri/src/drive/mod.rs` | 101 | `f689981fc6c7f414dfb47e13e827b00234de1e17b72f6d08fc9b816430851481` |
| `src-tauri/src/drive/model.rs` | 35,650 | `f5bcf2019a34b410056e0cb2e514dcc6d471b1357f6aefe55fd1f1cf60cdd1d0` |
| Private `source.patch` | 37,154 | `a4be6e0b35993a7d95da14d7b927eaf5a47cabbff3f205bbd94f3fa38678a90c` |
| Private `manifest.json` | 3,353 | `3599ca74fa88b808117d7c4e438c33d9e65e5e8646ecfa0b07b509e94ece0d3d` |

Receipt hash is supplied separately. Inputs: contract `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`; independent lead review `e7075dbdb4c5a1f63402c4ade6e69d6d0e99874241e26eae1bad24c437af46a5`. Manifest records exact paths/hashes, commands/exits and test count.

## Implemented boundary

`decode_revision(raw, StorageSpace)` is the raw entry point. It checks the 1 MiB input limit before parsing/copying; a serde visitor enforces at most32 object/array containers including the root, rejects duplicate decoded keys at every nesting level (including escaped-equivalent names), and rejects invalid JSON/nonfinite numbers/trailing input. Typed private wire decoding rejects missing/unknown envelope fields, invalid enums, noncanonical UUID/hash/unsigned-decimal strings and exceeded set/text bounds. No handwritten JSON tokenizer or new serde feature is used.

The small model includes account syntax, canonical UUID/SHA-256/u64 decimal/display-text/provider-ID/object-ID primitives, storage/blob/revision enums, BlobRef, RevisionRef and immutable received RevisionV1. Object IDs have fixed or canonical UUID namespaces; kind mismatch fails. Parent and required-reference sets each permit32 entries, dependencies256. Same revision ID + same hash is DuplicateRevisionRef; different hash is ConflictingRevisionRef. Blob same hash + inconsistent length is ConflictingBlobLength; same hash/role repeats are DuplicateBlobRef. Equal content may serve different roles. Self-references fail; graph ancestry/ownership checks are deferred.

OriginalPhoto/RenderedExport blobs require Drive; internal types require appDataFolder. Revision envelopes and their direct dependency set are internal. Original-media references inside a future payload require that later codec; no new locator/URL/path/token fields were introduced here.

Exact received bytes and SHA-256 are retained separately from typed serialization. Public RevisionV1 is not directly deserializable or constructible; callers inspect shared borrows through ReceivedRevision. Payload has only `PayloadApplicability::Unvalidated`, including malformed-but-syntactically-valid content. Serialization/JSON inspection does **not** approve upload/application. ReceivedRevision debug output omits payload and raw bytes; errors do not echo received input.

## Defined regressions and actual checks

**14 inline test functions, zero executed.** Definitions cover camelCase round-trip and decimal strings beyond JS-safe integer range; SHA-256 `abc` known answer and whitespace-distinct received identity; version/unknown fields/enums; hash/UUID/decimal syntax and overflow; namespace/kind/tombstone target; root/nested/escaped duplicate keys; nonfinite/UTF-8/trailing/empty JSON; exact size/depth limits; Unicode text limits; 32+32 reference boundaries/duplicates/conflicting hashes/self-reference; 256 dependency boundaries/roles/length conflicts; all storage-kind combinations; malformed opaque payload remaining Unvalidated; account syntax only.

| Check actually performed | Status |
|---|---|
| `git rev-parse HEAD`; `git diff --exit-code` | Pass — expected base and empty tracked diff |
| `rustfmt --edition 2024 <two owned source paths>` then `--check` | Pass — exit0; formatting/parser check only |
| `git diff --no-index --check /dev/null <each new file>` | Pass — no diagnostics; exit1 denotes new-file difference |
| Compile/typecheck, clippy, 14 tests | **Not exercised** — source-only task and module unregistered |
| Sync, ownership authentication, payload portability, security/provider/device acceptance | **Not exercised** |

Environment observed: Darwin25.5.0 arm64; `rustfmt 1.9.0-stable (8bab26f4f6 2026-07-14)`. Source patch is the concatenation of two read-only `git diff --no-index /dev/null` outputs, not a staged/committed Git change.

## Explicit limits and next consumer

Envelope validation does not authenticate AccountKey, verify fetched dependency contents, validate same-object ancestry/cycles beyond immediate self-reference, infer profile identity/pairing, or validate opaque payload effects/paths/secrets. Provider IDs use conservative bounded ASCII syntax; real Google identity remains a later provider check. CreatedAt and engineContract are bounded nonblank display text, not a time parser or compatibility guarantee. DCP BLAKE3/Look semantics belong to the later portable codec; SHA-256 here never replaces them. The parser's allocation is bounded by input size/depth, but this slice does not promise recoverable allocator exhaustion.

After independent review, root alone registers `pub mod drive` in `lib.rs` and assigns the native cache. Future focused command, from `src-tauri`: `cargo test --locked --offline --lib drive::model`. Run type/lint checks then report actual outcomes; rustfmt is not compilation evidence. Do not dispatch graph/conflict/ack/store/auth/UI work as incidental fixes to this slice. No source change or provider action is requested beyond that separately assigned review/runtime step.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `a1862c812e66a434b394eb3d74b55424f02d7da81dec9b2e0243d9a350de4509` (6299 bytes).
