# Publication docs receipt

**Model:** GPT-6 Luna. **Snapshot:** `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/publication-docs`, detached HEAD `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`, tree `db1268edee6d4291d22ce929b9523b95e6a58982`.

## Files and hashes

- `README.md` — SHA-256 `79e31857d6ed8a08268e580f63729c6d603b8adac99ff2b18bbb5adff3ea0c57`
- `NOTICE.md` — SHA-256 `16f6600700db4cae675b1dbf766d0fbff0de5009389048ec9974f85d6df465a6`
- `docs/integration/PUBLICATION-DOCS-RECEIPT-2026-10-15.md` — its SHA-256 is included in the handoff to the lead.

The README Download paragraph now points to the fork's evergreen releases page for development previews when available, the current status section, and a source-build fallback. It does not claim that a release exists.

The NOTICE update replaces only the dated historical pipeline/status block. It records implemented native Route A, errors for unsupported/captured tables, the production CPU route versus optional GPU parity, incomplete cross-route measurements, the bounded non-HDR Look path, one controlled no-profile byte-identity result, 34 Sharma known answers, 208 library/2 CLI/5 explicit RAF-import checks, unavailable Adobe compare, Android debug build status, physical profile/export limits, and Lightroom 9.6 preflight without a reference export. E1–E5 and Drive remain planned, not implemented. Licensing, trademark, read-only interoperability, embedding and upstream-attribution paragraphs remain unchanged.

**Minimal diff:** 2 existing files changed; `git diff --stat` reports 9 insertions and 26 deletions. One receipt added. No source, binary, license text, protected NOTICE paragraph, commit or release was changed.

## Commands and checks

```text
git worktree add --detach <HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/publication-docs 2a7346229bc0cc398ee7e3fee126eb89428b7dd9
git rev-parse HEAD
git rev-parse HEAD^{tree}
git status --short
git diff --check
git diff --stat
git diff -- README.md NOTICE.md
shasum -a 256 README.md NOTICE.md
python3 -  # read-only Markdown file/anchor checker for README.md and NOTICE.md
```

`git diff --check` passed. The local Markdown link/anchor check passed. No tests, npm commands, builds, device actions or network checks were run.

**Pass:** documentation edits match the supplied publication facts and remain within the allowlist. **Fail:** none observed. **Not exercised:** CI/release availability, builds, application behavior and device/profile/Adobe workflows.

**Next consumer:** lead for review and integration. The working tree is left uncommitted for that review.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `df9ae55e4f2ce7c5a36e71caf3b27392d60bfd7e6eea1808897efebe7343331d` (2642 bytes).
