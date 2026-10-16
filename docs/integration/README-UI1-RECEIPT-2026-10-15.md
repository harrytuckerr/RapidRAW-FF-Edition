# README UI1 receipt — 2026-10-15

- **Worker/model:** Codex CLI - Luna / GPT-6 Luna
- **Snapshot/base:** `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/readme-ui1`, detached `bb21a75815d5331c65dec2efa23ac712e545c2ed`
- **Exact writable files:** `README.md`; new `docs/integration/README-UI1-RECEIPT-2026-10-15.md`.
- **No other checkout edits:** canonical `profile-preset-integration` had unrelated UI and integration changes and was read-only; all edits were made in this detached snapshot.

## Read-only evidence reviewed

- `../profile-preset-integration/docs/integration/UI1-INDEPENDENT-REVIEW-2026-10-15.md` and `UI1-PROFILE-PRESET-RECEIPT-2026-10-15.md`.
- `../profile-preset-integration/docs/integration/PS1-INDEPENDENT-LEAD-REVIEW-2026-10-15.md`, `PS1-PRESET-PROFILE-ROUNDTRIP-RECEIPT-2026-10-15.md`, and `UI1-PS1-CANONICAL-VALIDATION-2026-10-15.json`.
- Accepted UI source: `src/components/panel/right/ProfilePanel.tsx`, `src/components/panel/right/PresetsPanel.tsx`, `src/components/ui/ProfileBrowser.tsx`, `src/i18n/locales/en.json`, and `src/__tests__/profilePresetUi.test.mjs` (read from the canonical integration snapshot). Native preset metadata sources `src-tauri/src/file_management.rs` and `src-tauri/src/preset_converter.rs` were also read there.
- Frozen Drive contract `../drive-contract/docs/drive/contract.md`, SHA-256 `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`.
- Starting `README.md` in the assigned detached snapshot.

## Change summary

The README now distinguishes Camera Profile base `.dcp` / Look `.xmp` imports from Adjustment presets / **Import presets**, and directs Lightroom develop-adjustment XMP to presets while stating that mapping is incomplete and full Lightroom compatibility is not promised. It documents that ordinary preset apply/intensity/deselect preserves the current profile and that explicit capture metadata plus saved profile fields survive native serializer JSON round-trip/import-export checks. Legacy absent capture intent remains unrecoverable.

Status text separates source/component and host serialization checks from OS chooser/Tauri IPC, physical Android, rendering and Adobe acceptance. The published preview 0.1 is identified as predating the revised UI. The roadmap now marks the approved Drive scope as priority but not implemented, removes the old statement that edit sync is out of scope, retains no automatic photo backup, and lists Point Colour H/S/L as a working assumption plus planned improvement to existing Lensfun Auto correction using camera/lens metadata.

## Validation and evidence boundary

- **Pass:** `git diff --check` on the README; 59 Markdown link targets resolve to existing local files, and all local README heading anchors resolve. Python checks also verified final newlines and no trailing whitespace in both allowed files.
- **Pass in reviewed source evidence, not rerun here:** UI1 component/profile/import tests (46 passed) and frontend bundle; PS1 native serializer tests (5 passed), library tests (213 passed), formatting and strict Clippy. UI1 typecheck (70 inherited diagnostics) and scoped lint (46 inherited diagnostics) remain failures with no new diagnostics recorded in the reviews.
- **Not exercised by this docs-only task:** actual OS file chooser, Tauri IPC round-trip, physical Android revised UI/preset persistence, profile pixels/export, real matched Cobalt pair, Adobe comparison, Drive/provider sync, or a new published release.
- No tests/builds, source/lock changes, staging, commits, pushes, device actions, or releases were performed by this task.

## Frozen artifacts

- `README.md` SHA-256: `5adca1573d12120ac73f3455261f481a86e4bf60bc92f2c53e6eab2473fe83ef`
- This receipt SHA-256: sent to the lead with the reviewable README diff.

**Next consumer:** lead review/import. The README remains a detached, uncommitted patch; no new release is implied.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `97511c5e7af5b9a61612fba69b999a3da3992dc6e4fc1d53f556abec07aa108c` (3912 bytes).
