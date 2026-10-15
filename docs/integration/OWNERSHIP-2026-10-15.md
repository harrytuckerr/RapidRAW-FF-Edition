# Upstream integration ownership and gates
Inputs: fork 6f171d9bc0724361efc385cc43f3a6d1c36f5054; upstream 3385c9f0cf03325f59ce03bb04db486fe907bab1.
Target branch: codex/upstream-sync-2026-10-15. cobalt/main stays unchanged until runtime acceptance.
All workers edit isolated snapshots. Lead alone imports reviewed named files and commits.
- Lead working/: raw_processing.rs, image_loader.rs, app_state.rs, lib.rs, cache_utils.rs, Cargo manifest/lock; build/integration.
- Confucius gpu/: gpu_processing.rs, image_processing.rs, shaders/*.wgsl, dcp/validation.rs.
- Hegel frontend/: src/**, package manifests if needed.
- Peirce android/: workflows build/cobalt-ci, gen/android/**, android_integration.rs, scripts/android-*, Android integration doc.
- Hilbert profiles/: dcp/pipeline.rs, dcp/mod.rs, dcp/commands.rs, export_processing.rs, bin/validate_dcp.rs.
No shared checkout writes. No worker commits, pushes, device mutations. Lead test/build stage uses reviewed imports.
Acceptance: baseline upstream check; integrated check/tests/fmt; production shader validation; CPU/GPU parity; real desktop rendering/export; Android build/start/editor/import evidence (emulator distinguished from physical). Missing matched Cobalt Modular/Adobe references remain explicit fidelity limits. Do not interpret skipped tests as passes. No merge to cobalt/main without requested desktop and Android checks passing.
