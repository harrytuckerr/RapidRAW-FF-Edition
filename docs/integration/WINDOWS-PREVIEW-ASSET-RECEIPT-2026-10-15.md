# Windows preview assets — bounded verification, 2026-10-15

**Pass:** both assigned artifact archives match API/source/digest/size, pass ZIP CRC/safe extraction, and their Windows jobs completed successfully. Windows execution and internal NSIS payload inspection are **Not exercised**.

Actual model `gpt-6.1-sol`. Repository `harrytuckerr/RapidRAW-Fujifilm-edition`; frozen head `2a7346229bc0cc398ee7e3fee126eb89428b7dd9`; [run38708083220](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/actions/runs/38708083220). Owned output only: `/private/tmp/rapidraw-main-publication-20261015/windows-batch1`.

| CI application target | Installer | Bytes | SHA256 |
|---|---|---:|---|
| x64 | `RapidRAW_1.6.5_x64-setup.exe` | 24806952 | `0b660bcd179220a44cabd19207eb92d00c854b8025f69c3866697e9e46519bfc` |
| ARM64/aarch64-pc-windows-msvc | `RapidRAW_1.6.5_arm64-setup.exe` | 22187897 | `b55889d9b52d1d9e79b0c181f4f2a5da87ff33a3112f51b38e16b95149c70385` |

Each extracted installer remains at its exact absolute `path` in receipt.json; nested extraction paths preserve the archive member name.

| Artifact | ZIP bytes | ZIP SHA256 |
|---|---:|---|
| 11961087576 | 24797284 | `16527bf1eefa3e380d4a2cbb7c46abf048f20575bf519f4a411dbcb0a423f508` |
| 11961675654 | 22177447 | `6b4377a6d283e5c810f4f606fe3810ed6ccf686f1ce8bacb2fcaef2caaab415a` |

API artifact metadata matched repository/run/head, expected name/size/digest and non-expired state; downloaded bytes matched again. Each ZIP had one regular EXE member. CRC, entry count/total size, relative paths, duplicate/case collisions, encryption/type/symlink checks passed before extraction under the owned directory.

Both actual installer stubs are PE32/I386 (`Machine=0x014c`, optional header `0x010b`). Python bounded PE parsing, file and objdump agree. Neither has a PE certificate table (`offset=0,size=0`). These header meanings follow [Microsoft PE/COFF documentation](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format). This does not establish the compressed application payload architecture or Windows trust/catalog signing.

| Job | Final observation |
|---|---|
| [116332374430](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/actions/runs/38708083220/job/116332374430) | completed/success, 2026-10-15T09:24:59Z |
| [116332374920](https://github.com/harrytuckerr/RapidRAW-Fujifilm-edition/actions/runs/38708083220/job/116332374920) | completed/success, 2026-10-15T09:23:24Z |

The initial x64 job snapshot was in progress despite its uploaded artifact; one final snapshot after local inspection showed success. No CI wait loop was used. API snapshots and exact commands/exits/times are retained.

`bsdtar -tf` returned exit1 / unrecognized archive format for each NSIS EXE. No 7z/NSIS package extraction or Windows Authenticode verifier was available, and no dependencies were installed. Internal application/resource/private-asset audit, payload architecture from extracted application PE, Windows install/launch/runtime, and Authenticode trust/catalog/revocation are **Not exercised**.

Private-asset exclusion remains limited to frozen source/CI provenance: repository/head/job/API identities, CI checkout/build/target-specific EXE upload definitions, and no additional bundle resources/externalBin declared in frozen tauri.conf.json. No deep installer content exclusion claim is made.

No EXE executed, installed, or opened with Wine; no build/Cargo/npm/phone/UI/Git/source/cache/upload/release mutation. Only read-only gh API calls and the two downloads were used. Root alone publishes. Scope completed and stopped after these two artifacts.

Full metadata in receipt.json; original archives, extracted installers, raw API observations, format-tool logs and parser scripts retained. artifact-hashes.json freezes each file, including the two installers.
