# GD0 acceptance matrix v2 receipt — 2026-10-15

- **Worker/model:** Codex CLI - Luna / GPT-6 Luna
- **Snapshot/base:** `<HOME>/.codex/worktrees/rapidraw-fuji-team-20261015/drive-acceptance-v2` at `5ca276ac1dab210f11ee4cecbf904038516164f8`
- **Frozen contract:** `../drive-contract/docs/drive/contract.md`; SHA-256 `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`
- **Other requirement source:** `<HOME>/Documents/CoWork/RapidRaw/ORCHESTRATOR_HANDOVER_2026-10-15.md` §16.5
- **Seed:** original v1 matrix hash `dc525bdf0afdbefcc8fba1dcfe17493ce1b8dcde697109487fa893b3c0920b1e`; original snapshot was left untouched.
- **Writable files in this snapshot:** existing `docs/drive/ACCEPTANCE-MATRIX.md`; new `docs/integration/GD0-ACCEPTANCE-MATRIX-V2-RECEIPT-2026-10-15.md`.

## Changes and validation

The matrix retains original cases GD0-01 through GD0-13, all marked **Not exercised**, and adds 15 separately tracked cases GD0-14 through GD0-28, also **Not exercised**. Added coverage makes contract branches explicit for appDataFolder versus visible photo workspace consent, same/separate OAuth projects, authorization failures, workspace and app-data loss, complete pagination, strict schema/dependency checks, stale acknowledgements/account generations, profile collisions, secret/redirect handling, resumable transfer failures, portable render dependencies, cache expiry and durable local recovery. Subcases require independent evidence; no branch inherits another branch's outcome. The matrix keeps the existing Adobe, colour, profile and export gates unchanged.

The contract is treated as an implementable test model, not proof of cross-client grants, profile-library policy eligibility, or operational behavior. App-data is described separately from the visible photo workspace. This work makes no new auth, policy, conflict, retention, cache, or colour decision.

Read-only inspection and checks performed from the snapshot root:

```sh
git rev-parse HEAD
git status --short
shasum -a 256 docs/drive/ACCEPTANCE-MATRIX.md ../drive-contract/docs/drive/contract.md
sed -n '74,151p' ../drive-contract/docs/drive/contract.md
sed -n '525,555p' <HOME>/Documents/CoWork/RapidRaw/ORCHESTRATOR_HANDOVER_2026-10-15.md
```

The base matched. Initial status showed only the seeded untracked `docs/drive/` directory; its matrix file matched the supplied seed hash. A Python structural check verified all 28 case IDs are unique, GD0-01–GD0-13 were preserved, every case has **Not exercised** status, the 15 new IDs are present, and both allowed documents end in newline with no trailing whitespace. No tests/builds were run; no provider, account, network, device, source implementation, or Git mutation occurred. The matrix contains no Markdown links requiring target checks.

- **Final matrix SHA-256:** `d6523f4961d5b6d1dff3074db54c3c973a2a26caf08c793dff7c6041e620b44e`
- **Final receipt SHA-256:** sent in the team report.

## Acceptance boundary and remaining inputs

All 28 acceptance cases remain **Not exercised**. This was documentation-only preparation, not operational acceptance. Required external inputs include owner-controlled Google Cloud project ID/number, enabled Drive and Picker APIs, consent-screen URLs/domains/audience, desktop OAuth client and loopback registration, Android client registration and actual signer SHA-1 for each tested build, designated test-account authorization, fresh scope grants, Workspace restrictions, exact application artifacts and physical device/OS, synthetic assets, and controlled network/storage conditions. These are prerequisites for the corresponding live cases, not evidence of a pass.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `def2538ccef8c5f919a29055f9b5bc7c7efea095a2524e1f8e398f53e0cb1f11` (3703 bytes).
