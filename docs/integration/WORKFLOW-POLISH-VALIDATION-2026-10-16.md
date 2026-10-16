# Workflow polish — source validation, 2026-10-16

Camera Profile now uses one Import menu with separate base-DCP and Look-XMP actions. Profile display orders CCD, Fuji/Fujifilm and Cobalt families, with Modular bases first. Display order does not determine compatibility or change an existing selection.

The global White Balance menu provides As Shot, full-frame Auto and nominal Kelvin presets. Auto uses the existing pixel sampler and absolute white-balance conversion. Request, image-source and edit revisions prevent delayed results from overwriting a later image or manual change; unrelated edits are merged. The native loader serializes generation admission and frame publication, and Auto supplies the expected image path. Legacy manual sampling can omit that optional argument. Daylight and Flash share a nominal Kelvin value; persisted numeric values cannot distinguish their preset identity.

Adjustment-preset import resolves Android document names and reads picker URIs through the existing platform helpers. Develop XMP remains separate from profile Looks. Cancellation, empty results, partial success and failure have distinct feedback; details are bounded and redact raw transport errors.

## Validation

The final source cohort was based on `873dee3156aa231e076a0b526ccf1d7d2b916cdf`. Dependency manifests, lockfiles, vendored security patches and published preview assets were unchanged.

| Check                                                                    | Actual result                                                                                                                              |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Five native targets: library, DCP CLI, PKCE, loopback and token response | **Pass: 577 passed, 0 failed, 14 ignored**; all 591 named outcomes checked; five executed binaries retained                                |
| Seven frontend test files                                                | **Pass: 73 passed, 0 failed, 0 skipped**                                                                                                   |
| Production Vite build                                                    | **Pass**, eight output files; chunk-size warning retained                                                                                  |
| Scoped Prettier and `cargo fmt --all --check`                            | **Pass**                                                                                                                                   |
| Locked, offline, all-target Clippy with `-D warnings`                    | **Pass**                                                                                                                                   |
| TypeScript                                                               | **Fail: 70 errors**, source-mapped to the unchanged baseline; 12 earlier diagnostic bodies differ only in proven resource-key union counts |
| Scoped ESLint                                                            | **Fail: 94 errors, 4 warnings**, same source sites and messages as the baseline                                                            |
| Translation validation                                                   | **Fail: 225 existing issues**, baseline/candidate output byte-identical                                                                    |

The final native command was `cargo test --locked --offline --lib --bin validate_dcp --test drive_auth_pkce --test drive_auth_loopback --test drive_auth_token_response -- --nocapture --test-threads=1`. The 14 ignored manual/private-fixture cases stayed ignored. Added checks exercise the actual publication helper under a forced stale-load interleaving, same-path reload generations, the synthetic pixel sampler, source mismatch rejection, URI/parser transport seams, display ordering and real frontend hooks/callbacks. Synthetic inputs do not establish commercial-profile fidelity.

All 913 source inputs remained unchanged during the final gates. Earlier failed checks and intermediate results remain separate; test counts are not added across runs. Root retained full input hashes, exact outcomes, raw logs, tools, commands and binaries in private evidence.

## Acceptance limits

No browser automation provider was available, so actual browser rendering, native keyboard behavior and visual acceptance are **Not exercised**. These checks also do not establish Android compilation, physical document-picker behavior, Mac/Android photo editing, save/reopen, export, Adobe parity, provider authentication or device performance. Automatic Modular selection and profile-switch caching are separate work.

The published `fuji-preview-2026.10.16.1` APK, DMG and source archive predate these changes and the later security fixes. They retain their original bytes and are not rebuilt feature or security releases.
