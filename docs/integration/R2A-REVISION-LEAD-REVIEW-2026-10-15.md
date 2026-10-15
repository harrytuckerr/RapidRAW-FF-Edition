# R2a revision: independent lead source review — 15 October 2026

**Accept the bounded source correction at the pinned hash, conditional on fresh runtime validation.** No blocking defect found in the incremental correction. This is not canonical integration or Android acceptance.

Reviewer: lead Codex CLI, actual model gpt-6.1-sol. Reviewed isolated `uri-revision`, HEAD8a4f02bcec8a3edb567979462cbf022adde6f8b9, with source SHA256 `e66dceb65c983721abbd8f806eb402e6eb5a5fcced0421cd7b58c75ca92979f7`. Implementer: Claude Code - Opus. Original URI design was independently reviewed by Astra; review SHA e9eb2d6b71c81ec9a9fee292da092a17734d9a5b3ff0f17a5e49ce0eebc1eb96 is preserved. The code-review skill was read and applied. Lead did not edit production source.

The full original helper and its four new regression definitions were read. The incremental diff versus rejected dff9be3 is three hunks: a shared cleanup result, the post-publication cleanup call, and four tests. The source-only receipt35c81f2a and frozen original remain historical inputs. Current source hash was recomputed before this report. A text comparison against HEAD confirms the Android local-path/desktop/event body, from its local-path parse through the end of import_files, is unchanged.

## Correctness and boundaries

- `remove_temp` attempts the recorded owned path directly and accepts only success or NotFound as clean. Every other removal error reaches the diagnostic. There is no stat predicate that can turn PermissionDenied or I/O error into absence.
- After successful publication, this produces `Ok(PublishedImport)` with a cleanup warning. It preserves complete final bytes and does not turn cleanup uncertainty into a failed publication. This avoids introducing a cleanup-driven retry; it does not prove that every external retry or interruption is duplicate-free.
- The same rule preserves primary-error-first failure diagnostics while avoiding a false cleanup error for an already absent temporary.
- Validation order, same immutable buffer, write/flush/sync, no-clobber publication, original preservation and collision behavior are unchanged from the reviewed initial URI helper. No deletion of a final/collision target, overwrite fallback or additional source import route is added.
- No new production seam or allocation loop is introduced. The revision does not change file size/memory policy or promise crash/directory durability.

## Regression definitions reviewed

The NotFound test invokes the real default persist/remove functions and checks exact final bytes and absence of a warning. Observed NotFound establishes an absent temporary; without syscall instrumentation it cannot distinguish no-replace rename from a successful hard-link-and-unlink fallback.

The Unix permission regression uses a real hard link, then denies search on a private library directory. The actual helper must return success with warning; after permission restoration it checks complete final bytes and both names. Normal restoration is checked with unwrap, and a Drop guard attempts restoration during panic. It requires a non-root host. The target is task-private, not a user directory.

The missing-temp-after-write-failure test checks retained primary error without a false cleanup message. The unsupported-persist injection checks no final/temp output. These are meaningful helper regressions, with OS permission behavior and injected failure behavior distinguished.

**Reviewer runtime execution of revised source: none yet.** New compilation/tests are the separately assigned R2a revision-validation task. The lead previously reran nine tests of the older verified worker artifact1f736a06; those results do not validate e66dceb6. A changed source hash or test failure reopens this disposition.

## Remaining gates

- Fresh revised-source helper suite, two opt-in controls, default library suite, format and strict native lint.
- Canonical combined-source build/tests before commit.
- Pinned Android build of the actual cfg call site, then provider/picker/Fold filesystem/UI evidence.
- Android local paths/sidecars/deletion (R2b), terminal false Success (R3), actual X-T5 RAW acquisition/decode and original profile/Adobe acceptance remain open.

Commands: source/receipt/diff reads, hashlib verification, git-show/text comparison; no build/cache mutation or device operation by this source review. Pass: static correction, regression design and scope. Not exercised: revised runtime and Android. No merge/release decision follows.
