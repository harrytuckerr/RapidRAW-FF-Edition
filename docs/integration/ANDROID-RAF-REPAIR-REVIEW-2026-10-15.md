# H1R independent repair review — 15 October 2026

**Verdict: implementation can begin within the limited scope below after lead assigns ownership.** Reject empty image imports and the demonstrated top-level JPEG signature under a `.raf` name; guard existing opens too. Do not require every `.raf` to start with `FUJIFILM`, and do not reject JPEG-headed content for every RAW extension. Include honest terminal import status and transactional staging; a guard alone does not deliver a recoverable user-visible repair.

Model: `gpt-6-astra`, Codex CLI - Astra, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Snapshot `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/raf-review`, frozen HEAD `4c1e965a53de9c835df920b2e3d69c2c8a864708`. Only this report is writable. No build, device command, source/lock change, staging, commit or push. Review used the engineering code-review skill; implementation remains lead-owned.

## Accepted evidence and limits

The lead's `docs/integration/ANDROID-RAF-DEVICE-2026-10-15.md` was rehashed: `51f10e64afd1277ed2874753a392610a10d5b6385318125214ecf7cd3ca10713`. It establishes a stable staged `DSCF5389.RAF`, 2,940,309 bytes, SHA-256 `82bd1e04633c491824b90cc31486b5c3ff0770a6dc712edf95d74c691bc958c6`, whose captured prefix is JPEG SOI/APP0/JFIF followed by Exif. Complete JPEG validity was not exercised. Repeated decoder/frontend errors are observed in the accepted logs. X-T5 source and import/picker sequence are user-reported.

Neither the original selected bytes nor their URI/provider relation is established. Do not say the provider converted the image, RapidRAW corrupted it, or genuine X-T5 decoding is unsupported. The initial Opus diagnosis was read only as a source map/historical reproducer; its provider-cause, only-import-route and stale empty-file hypotheses are not adopted. No private image or additional source was opened by this review.

## Findings, severity and smallest correction

All repository line numbers refer to the frozen base. `RAWLER` below means `/Users/harrisontucker/.cargo/git/checkouts/rapidraw-dnglab-d9adc5b1c1279a46/a32bc1f/rawler`, locked revision `a32bc1ff1f4f3c56d4d3226082f6eb17414f9159`.

| Priority | Source / reproducer | Consequence / minimum correction |
|---|---|---|
| P1, observed failure plus static gap | [file_management.rs](../../src-tauri/src/file_management.rs):3730–3737,3760–3783. Content URI display name and returned bytes are independent; empty/JPEG bytes named `.RAF` reach `fs::write`. | Invalid item enters the library. Check the successful byte-read result before EXIF extraction, destination creation or write. Empty and RAF/JPEG contradiction fail with a precise message. Preserve originals. |
| P1, static failure-report defect | [file_management.rs](../../src-tauri/src/file_management.rs):3889–3903 emits `import-error`, continues, then unconditionally `import-complete`. [useTauriListeners.ts](../../src/hooks/useTauriListeners.ts):195–209 turns completion into `Status.Success`. [MainLibrary.tsx](../../src/components/panel/MainLibrary.tsx):574–578 displays only a generic failure label. | A rejected import can immediately appear successful, and the useful error is not displayed there. Add authoritative terminal counts/result to completion and show precise error in a toast; preserve folder refresh. UI invoke resolution only means work was queued. |
| P1, proposed-regression risk | `RAWLER/src/decoders/mod.rs`:874–884 deliberately accepts Konica Minolta JFIF/EXIF through `MrwDecoder::new_jfif`; `mrw.rs`:108–144 derives camera/raw data from that container. | Blanket `is_raw_file(name) && JPEG-prefix => reject` can reject a decoder-supported RAW container. Scope the contradiction to `.raf` in this repair. Other formats need separate format-specific evidence; a JFIF prefix alone is not a universal “rendered JPEG” test. |
| P2, existing-open gap | [image_loader.rs](../../src-tauri/src/image_loader.rs):136–174,193–215,982–1020; [raw_processing.rs](../../src-tauri/src/raw_processing.rs):184–190. Extension chooses RAW; mmap failure falls back to real read; empty or JPEG data reaches rawler and its generic error. | Place the shared guard before EXIF persistence/Apple RAW/RAW development in `load_base_image_from_bytes`, not a rewrite keyed to the English “No decoder found” string. Real read errors remain read errors; successful zero-byte reads get the empty-content error. |
| P2, static staging risk | [file_management.rs](../../src-tauri/src/file_management.rs):3767–3774 checks existence then `fs::write`; local path branch copies at3828 and can delete source at3870–3882 on Android. | A failed write can leave a partial destination; existence check alone is not no-clobber publication. Stage privately, publish a completed item without replacement, clean only files owned by that attempt. Do not execute source deletion on any failed/invalid item. |
| P2, profile-path consistency | [image_loader.rs](../../src-tauri/src/image_loader.rs):87–114 selected-profile branch bypasses `load_base_image_from_bytes`. | Also guard bytes at `load_and_composite_with_profiles` before this branch. Keep ordinary `.jpg` profile rejection at97–99 and existing pipeline errors. Do not display the misnamed JPEG as RAW or fall back to upstream rendering after a selected-profile failure. |

## What rawler actually proves

`RAWLER/src/decoders/raf.rs:61–65` probes the first eight bytes for `FUJIFILM`; `decoders/mod.rs:854–856` immediately constructs that decoder when it matches. Construction can still fail in RAF parsing/camera/makernotes (`raf.rs:211–219`); a short matching header is not a valid RAW test. Other probes include TIFF/DNG (`mod.rs:909–920`), TIFF with Fuji Make (`958–960`), JFIF-based Minolta (`874–884`) and size-based legacy RAW (`991–994`). The final unsupported error is at997–1001. Actual X-T5 camera metadata exists at `data/cameras/fuji/x-t5.toml:1–4`; that is source support evidence, not an X-T5 decode pass.

Thus a positive RAF magic allowlist adds rejection semantics beyond this defect. A legitimate DNG fixture or TIFF containing an embedded preview can be routed by content even with a `.raf` filename today. Preserve those routes, including existing fallback; leave malformed/unknown nonempty data to the current decoder. Do not scan inside a valid RAW for JPEG markers—embedded JPEGs are normal inputs to the existing fallback. The guard is an early contradiction detector, not a RAW validator or camera-support table.

`android_integration.rs:336–395` reads `openInputStream` into a vector, propagates JNI/read errors and closes the stream after the loop. A clean immediate EOF returns `Ok(empty)`. No transcoding call appears in that helper. No JNI change is needed to reject empty image content at the image-import consumer; changing this general reader would also affect its other consumers. No storage permission or manifest observation establishes the actual source used in the reported event. `import_files` also has a non-content-URI path (`3786` onward), which must not be described as unreachable on Android.

## Frozen repair contract and slices

### R1 — helper and existing-open path

In existing `formats.rs`, add a small typed `ImageContentIssue` (`Empty`, `RafHasJpegSignature`) and a pure `validate_image_content(name: &Path, bytes: &[u8]) -> Result<(), ImageContentIssue>` (names are proposed). Rules: empty => Empty for image-import/open consumers; case-insensitive extension `raf` plus bytes beginning `FF D8 FF` => RafHasJpegSignature; otherwise continue normal handling. Treat the three-byte match as a signature, not proof of complete JPEG decoding. Unknown/truncated nonempty data is not relabelled; a partial `FUJI` prefix is not declared valid. `.jpg` with genuine JPEG bytes continues through the normal JPEG decoder. `.mrw`/other RAW are not subjected to this RAF-specific contradiction rule.

Use at the start of `image_loader::load_base_image_from_bytes`, ahead of `persist_exif_if_missing`, and at the profile-aware `load_and_composite_with_profiles` boundary. Preserve existing cancellation precedence when a request is already cancelled. Errors should identify only a safe display basename and actual observed condition, for example:

- `“DSCF5389.RAF” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.`
- Import empty: `No image data was read for “name”. Select the original file and try again.`
- Existing open empty: `“name” is empty and cannot be opened.`
- Actual source read failure: `Could not read “name”: <underlying error>`; never turn a permission/I/O exception into empty content.

No cause attribution (“the source app supplied a JPEG”), automatic rename, JPEG substitution, selected-profile fallback or silent repair. Existing rejected files stay untouched; the user can select a genuine original later. The present `useImageLoader.ts:105–109` catch already displays backend text; no edit needed there. Direct CLI/late `lib.rs::profile_base_image` calls still retain existing pipeline rejection and may keep generic errors; this slice does not claim every no-path decoder call gains a filename-aware message or solve cached-file replacement races.

### R2 — Android import transaction

Own the Android image-import consumer in `file_management.rs`, not `android_integration.rs` or camera math. Both `content://` and Android local-file inputs reaching `import_files` must obtain actual bytes/read status, run R1, and stage only validated nonempty content. Validate the resolved display name **before** filename-template renaming so changing output names cannot bypass the check. A copy with `.RAF`, `.raf` or `.RaF` gets the same result. Do not use reported MIME/length as proof; they may be useful future provenance, but are unnecessary for this guard.

For URI reads, `Err` propagates and no write occurs; `Ok(empty)` is the distinct empty case. Guard before EXIF calls/directory creation where possible. For Android local files, put the same guard before copying/deleting; do not leave that branch as an unmentioned bypass. Read/stage from a single opened source or immutable byte buffer so the validated payload is the payload published. Preserve existing desktop behavior outside the explicitly assigned branch.

Use a narrow non-Android-gated staging helper in this file so actual transaction code is host-testable. A hidden temporary in the destination filesystem receives the validated bytes with `write_all`, flush/sync as supported by the agreed write contract, then no-clobber publication. Existing `tempfile=3.27.0` is available; `NamedTempFile::persist_noclobber` is a candidate, not an assumed universally atomic Android primitive. Installed docs at `tempfile/src/file/mod.rs:781–794` promise no overwrite but warn that a temporary hard link may remain. Confirm target-filesystem behavior on Fold before acceptance; fail clearly if no-clobber publication is unsupported, never fall back to overwriting `rename`/`fs::write`.

Failed validation/read/write/publish: no final image, no library success for that item, original and pre-existing same-name destination unchanged. Clean only the owned temporary/new paths; never delete an existing collision target or the currently failed staged photo. Report cleanup failure separately and retain the primary error; hidden leftovers must not appear as imported images. A process crash may leave a hidden temporary; no claim of general crash recovery is made.

For Android local-path imports with sidecars, track new files created by this attempt and roll them back on a required sidecar failure before reporting item success. Do not delete the source until all required copy/publish operations have succeeded; URI `delete_after_import` remains skipped as at3776–3780. Do not broaden source deletion or alter unrelated desktop trash logic. If this local-path transaction is split into a second patch, label first patch explicitly **URI-import-only**, and do not claim all Android imports protected until both land.

Mixed batches retain independently completed valid items. “No partial success” means no partially written failed item and no false all-success result; it does not mean deleting good imports or originals to roll back an entire batch.

### R3 — terminal feedback (required for useful delivery)

Backend tracks imported/failed counts and per-item useful errors. Emit `import-complete` with `{total, imported, failed, errorMessage}` once after the batch; counts must sum correctly. Existing `import-error` can remain for compatibility, but final result is authoritative. `failed>0` => terminal Error and precise bounded summary (`1 of 2 imported; 1 failed: …`); `failed==0` => Success. Include item basename, not content-URI/grant text, in new diagnostics. `startImportFiles` invoke resolution is not completion.

In `useTauriListeners.ts`, consume the payload, refresh folder trees for successful items even in mixed results, preserve Error at completion, and show a toast containing the actionable summary. Existing MainLibrary generic failure label can remain because the toast surfaces detail; no component/store/schema/i18n rewrite required. Malformed completion payload must not become Success. Guard overlapping runs or explicitly serialize import requests before claiming event-to-run correctness; existing events have no run ID. Do not accidentally clear a newer run's failure in a late completion. Lead may choose a small single-active-import guard in the producer rather than a new event protocol.

## Exact proposed ownership

One implementation owner, serialized because files are shared; lead imports after independent review. R1 first; R2/R3 together form deliverable Android import protection.

| Files | Allowed change |
|---|---|
| `src-tauri/src/formats.rs` | Typed contradiction/empty helper and focused tests; preserve extension lists. |
| `src-tauri/src/image_loader.rs` | Two consumer checks; actual open/profile-route regression tests. No decoder, fallback or colour changes. |
| `src-tauri/src/file_management.rs` | Android import reads/validation/staging cleanup, host-testable staging helper, terminal outcomes/single-run guard if chosen, in-file tests. Preserve unrelated file operations. |
| `src/hooks/useTauriListeners.ts` | Completion/error message/status handling only. |
| New `src/__tests__/imageImport.test.mjs` | Exercise actual listener/hook/store behavior with controlled events using existing node/esbuild/react-test-renderer tools. No package/lock change. |

Read-only: `android_integration.rs`, `raw_processing.rs`, `dcp/*`, `lib.rs`, `useFileOperations.ts`, `MainLibrary.tsx`, Android manifest/Kotlin, existing `scripts/integration-fixtures.py`, rawler checkout and all dependency locks. If a real defect needs another file, report exact evidence for lead reassignment. No vendor fixtures committed. The content helper may be small; avoid tests solely proving that its own prefix comparison agrees with itself.

## Meaningful verification matrix

| Test / expected result | Evidence boundary |
|---|---|
| Generate a complete small JPEG with existing image encoder; stage as `.RAF`/`.raf`/`.RaF`. Actual staging rejects, no final/temp-visible item or EXIF sidecar; source hash unchanged. Same bytes as `.jpg` import/open normally. | Exercises helper **through** staging and open, not only marker literals. Test complete JPEG + accepted 64-byte header separately; no claim full captured photo was transferred. |
| Call actual `load_base_image_from_bytes` on misnamed JPEG and empty input, with Apple-RAW option both off/on; expect precise errors before decoder/fallback. Profile-aware wrapper also rejects it; legitimate `.jpg` + selected profile keeps RAW-required rejection. | Covers preexisting staged files and profile bypass. No display-as-RAW workaround. |
| Existing synthetic CFA DNG from `scripts/integration-fixtures.py` imports byte-identically and decodes through actual rawler; also use a `.RAF` alias in isolated tests to prove absence of a new Fuji-magic allowlist. | Decoder remains authoritative. Synthetic pass is not native X-T5 acceptance. |
| A designated genuine native RAF (existing lawful X-Pro2 fixture, and genuine X-T5 when available) retains full file hash after staging; actual decode dimensions/camera agree before/after guard. | Required native regression; X-Pro2 cannot substitute for X-T5 device acceptance. No made-up `FUJIFILM` header test counts as native decode. |
| Existing TIFF/RAW embedded-preview fallback still executes for an appropriate fixture; embedded JPEG marker at nonzero offset is not rejected. Preserve JFIF/Minolta route eligibility; use real fixture if available, otherwise report native Minolta Not exercised and prove no general RAW-prefix rule in review. | No new RAW format allowlist or removal of supported fallback. |
| Source read returns Err (including an error after some bytes): precise read error, no staging. Clean EOF/zero bytes: empty error. | Host adapter seam with controlled read failure exercises transaction; JNI/provider behavior still needs device test. Do not mock a successful entire app route. |
| Inject short-write/write-error, collision created at publish time, unwritable destination, required sidecar failure, cleanup failure. | No final failed item, no original/pre-existing target mutation. Error surfaced; cleanup failures recorded. Normal output equals input byte-for-byte. |
| Emit/import one failed item, one valid + one failed, all valid, zero/malformed terminal result, repeated run and overlapping-run rejection. Actual listener ends Error when any failure; precise message visible; refresh still runs; clean next run can succeed. | Backend outcome tests plus frontend event-consumer tests; fake events do not prove physical native transport. |
| Physical rebuilt APK: reselect designated misnamed fixture via actual picker and open existing staged mismatch; observe error, no new invalid import. Import genuine RAF and normal JPEG; test duplicate target and empty/read-error provider fixture when available. | Sole device operator, separate assigned test window; recorded source/APK/hash. No resetting/deleting user's failed file. |

Proposed commands after implementation, not run here (lead supplies isolated path and bounded build target):

```sh
cd /Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/<assigned-repair-snapshot>/src-tauri
cargo fmt --all -- --check
cargo test --locked --lib formats::tests -- --nocapture
cargo test --locked --lib image_loader::tests -- --nocapture
cargo test --locked --lib file_management::tests -- --nocapture
cd ..
node --test src/__tests__/imageImport.test.mjs
npm run test:profiles
```

New test modules/names must match these filters and receipts must record executed counts (zero tests is not Pass). The implementation receipt additionally names private native/synthetic fixture hashes and before/after decode outputs. Android cfg/build verification and real JNI/event/filesystem/UI tests require a fresh source-matched APK through existing pinned build wrapper, assigned by lead; no dependency resolution or stale APK acceptance. Do not write ad hoc full builds in this review.

## Receipt and remaining gates

**Pass — static review:** frozen HEAD and clean initial snapshot; accepted device report hash checked; source import/open/event/decoder routes and installed tempfile behavior inspected. **Fail — accepted runtime evidence:** staged `.RAF` open reported `No decoder found`; this reviewer did not rerun it. **Fail — static contract gaps:** unchecked import, generic open error, terminal Success overwrite, partial-write exposure. **Not exercised:** implementation/tests/build, genuine X-T5 decode, native Minolta regression, physical repaired import/open, source/provider identity or original-byte preservation. **User-reported:** X-T5 import/picker sequence, earlier launch/UI/basic edits only.

Commands were read-only `git rev-parse HEAD`, `git status --short`, `nl`/`sed`/`rg`/`cat`, `shasum -a 256`; no devices or builds. Initial memory quick pass in this thread had no RapidRAW hit; no memory-derived claims. Source fingerprints at frozen base:

| File | SHA-256 |
|---|---|
| `formats.rs` | `8f30478cf51e1f800b97ab926c36ad42124a93b0501fb78a2556775a793ffb17` |
| `file_management.rs` | `227a82b96e02a4304c909214660791b4ab5fea9de496c58f3780a25a0f7c886f` |
| `image_loader.rs` | `9eb42a235162d4816bf7b7145a42cd2f7a6b1fc9e63dc12b717aa28ab262f156` |
| `useTauriListeners.ts` | `55b05c8f37bf4d9ad7ec6b9db4f212d06dcbd77569dcb5d1cb63666d9c10e536` |
| `src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` |

Diff: one new review document; its final SHA-256 is in delivery message. Next consumer: lead assigns the five-file repair to one owner and freezes it for independent review, then sole device operator verifies a rebuilt APK. The scope is ready without resolving provider provenance, provided the claim remains **reject/report known bad content safely**, not **original RAF acquisition fixed**. Original selected source→provider bytes→staged bytes equality is a separate live gate and remains open.
