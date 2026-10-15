# R1 independent review — 15 October 2026

**Verdict: Accept the frozen two-source-file R1 change. Revise the receipt prose separately as listed below.** No blocking source defect found. This accepts the bounded existing-open guard, not Android runtime behavior, original RAW acquisition, import staging or overall profile acceptance.

Reviewer: Codex CLI - Astra, actual model `gpt-6-astra`, slot `01a13e02-3bb7-7100-b36a-2e591c9f54e7`. Reviewed snapshot: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/raf-guards`, detached base `8877d7eaa14442814912a199c8de9abdfba5c8b9`. Report-only checkout: `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/raf-review`. Prior design `ANDROID-RAF-REPAIR-REVIEW-2026-10-15.md` governs only its R1 slice here. No source edits, builds, tests, device commands, private-photo rendering or build-cache use by this reviewer.

## Frozen input verification

All hashes below were independently recomputed. Current `git diff -- src-tauri/src/formats.rs src-tauri/src/image_loader.rs` is byte-for-byte equal to the supplied frozen diff. The reviewed base and original `4c1e965a53de9c835df920b2e3d69c2c8a864708` have no differences in these two base source files. Scope is +132/−0 in formats and +275/−1 in image_loader, mostly tests; no other tracked file is changed. The implementer's receipt is untracked.

| Input | SHA-256 |
|---|---|
| `raf-guards/src-tauri/src/formats.rs` | `66134f6ce2183036fb35798cfb7eefc9d7ef512d72cbb2d873b762c9b4f2e5bf` |
| `raf-guards/src-tauri/src/image_loader.rs` | `fcf43150b241ee0cfdf0d2be1983ac91bfc837059b38dfdf91e1c11177f31cf5` |
| `/private/tmp/rapidraw-r1-20261015/r1-source.diff` | `2b35732d010f43d30bd539884be0efb93f0b5bcfe850091c5e080d1d6fd4a8d1` |
| `/private/tmp/rapidraw-r1-20261015/rapidraw-r1-test-binary` | `126499362bfedc7ba1490f0ed6650c84dfa9b5dd11b45360d0a9a1ee4a1a532a` |
| `raf-guards/docs/integration/R1-GUARDS-RECEIPT-2026-10-15.md` | `c284a7b615a6294bac52bec270724f308fd24fd0446004efc7e3469f73cf05f1` |
| `raf-guards/src-tauri/Cargo.lock` | `7ddd491af60c07c0d0a0983ab77c6f81409961831ed679b7b444103a6d765def` (unchanged) |

Hashing the preserved test binary establishes its identity, not a fresh build or independently reconstructed binary→source provenance. Build/test association remains the implementer's receipt plus inspected logs.

## Source assessment

Line numbers here refer to **modified raf-guards files**, not the review checkout's older source.

| Requirement | Assessment and evidence |
|---|---|
| Narrow detector | `formats.rs:131–151` rejects empty data, or exactly a case-insensitive `.raf` extension with leading `FF D8 FF`. Other nonempty inputs continue. No `FUJIFILM` allowlist, scan for interior JPEGs, image conversion or all-RAW JPEG prohibition. Passing does not certify a valid image. |
| Useful, bounded message | `formats.rs:155–167` uses basename and observed signature/empty condition, with a recovery action. It does not blame the provider or infer X-T5 decoder support. No rename/deletion or silent JPEG substitution. |
| Cancellation | `image_loader.rs:73–88` first checks the cheap content predicate; when a new content error would be returned, an already-superseded token returns `Load cancelled` instead. Accepted content returns immediately and keeps the pre-existing cancellation checks/order. This is not a new promise of eliminating all cancellation races; none is introduced by the O(1) check. |
| Early normal-open boundary | `image_loader.rs:146` precedes EXIF persistence at160, Apple RAW at166–180 and rawler later in the function. The rejected bytes cannot enter those branches. No altered decoder math or successful-input conversion. |
| Early selected-profile boundary | `image_loader.rs:116` precedes selection/pairing/development at118–131. A selected profile cannot bypass the guard. Normal JPEG plus selected profile still reaches `Camera profiles require RAW input` at119–120. |
| Read-error distinction | Existing `image_loader.rs:1006–1044` still propagates actual fallback `fs::read` errors at1029–1031 before calling the bytes loader. A successful empty read now receives the empty-content message. No I/O exception is translated into empty content. This is static evidence, not an executed I/O-failure test. |
| Consumer error surfaced | Existing `useImageLoader.ts:105–109` logs/toasts the backend error for the active effect. Actual Android toast/transport was not exercised by this review or the supplied host tests. |
| Existing valid/fallback paths | Nonmatching buffers follow the identical existing body. Complete synthetic DNG and native RAF control tests compare guarded to direct development; TIFF-with-embedded-JPEG test exercises existing fallback. `.MRW` JPEG-container eligibility is retained. Native Minolta decoding remains untested, appropriately labelled. |

No requirement to reject unrecognized nonempty formats is added. Direct CLI/late profile calls, existing cached-image bypasses and direct thumbnail fallback are outside the two guarded entry points, consistent with the bounded design. No R2/R3 change is assessed or proposed here.

## Test evidence and limitations

**Tests executed by this reviewer: none.** Existing logs were read and rehashed; the test binary was hashed but not run. No native/private photograph was read/rendered. The following are **Pass — recorded implementer runs**, not independently re-executed tests:

| Command in implementer receipt | Verified log result |
|---|---|
| `cargo fmt --all -- --check` | exit0 |
| `cargo test --locked --lib formats::tests -- --nocapture` | 5 passed; 0 failed/ignored |
| `cargo test --locked --lib image_loader::tests -- --nocapture` | 6 passed; 3 ignored |
| Same image-loader filter with supplied R1 env inputs and `--ignored --nocapture` | 3 passed; native control4000×6000, CFA DNG64×64, failed payload under a `.jpg` test name7752×5178 |
| `cargo test --locked --lib` | 150 passed; 0 failed; 12 ignored |

The last total is 139 pre-existing default tests plus 11 new default tests, not 150 additional tests. The three explicitly invoked R1 controls are separate; nine pre-existing ignored acceptance tests were not invoked. Missing control env variables panic instead of producing a skip-shaped Pass (`image_loader.rs:1294–1296`).

The six consumer tests use the actual loader/profile wrapper, including a complete encoded JPEG, uppercase/mixed-case RAF names, Apple-RAW setting on/off, empty inputs, cancelled tokens, normal JPEG profile rejection, Minolta route eligibility and TIFF preview fallback. The profile fixture selects a Look with an empty registry: it establishes that the content guard runs before selection errors, not a successful commercial-profile render. The Apple-RAW tests establish early rejection before that branch, not Apple decoder acceptance. Temporary-path tests do not exercise the asynchronous Tauri `load_image` command or Android filesystem reads.

The equality control at `image_loader.rs:1299–1318` invokes real direct `develop_raw_image`, then guarded open with enhancement disabled. This is a meaningful same-build value-equivalence regression; it is not a separate pre-change-binary comparison. It uses numeric f32 vector equality, not serialized-byte equality (see P3 below). Guard-before-EXIF is proven by source ordering; the tests do not separately assert that no EXIF sidecar was created.

Verified log fingerprints (under `/private/tmp/rapidraw-r1-20261015/logs/`):

- `fmt.log`: `e38302aef01b329ec6de7dbdcd03361300700f9cae380d6b90b871d975b748bf`
- `formats-tests.log`: `3d932ebad8243784478e91e69f8bb44066897521b307ce09094bfb6555856bb2`
- `image-loader-tests.log`: `1078ad20a9910b47e53259d837e60b1c56bde02bf511571edfe8779e85ed16e6`
- `image-loader-ignored-tests.log`: `e32855c08fcd81bab7246006e9bdb7c5ffec3c4477b6d7487321d506d1a703e2`
- `lib-suite.log`: `284d14e1e6fcd20b16ec090fac590456a6f2cdf81cca970761c0dd3dd39aba22`

## Receipt corrections — nonblocking for the source diff

All three are **P3 documentation/evidence precision**, not runtime defects. Lead can amend the receipt without changing the accepted source hashes.

1. **Receipt:79 contradicts its own test table and code.** It says no test exercised the pure prefix helper alone. Reproducer: inspect `formats.rs:179–247` and `formats-tests.log:24–31`; five validator/message tests ran. Replace with: “Five helper/message tests supplement six consumer tests and three opt-in controls; acceptance does not rely on helper tests alone.” These helper tests are allowed, so their presence is not a source problem.
2. **Receipt:46 understates compilation activity.** It claims one test-crate build and later reuse. `formats-tests.log:21` reports2m36s, while `image-loader-tests.log:1–3` again says `Compiling RapidRAW` and1m05s. Correct to the observed two compilation invocations followed by reuse; do not infer why compilation repeated. Also account for the build-script ONNX runtime download in `formats-tests.log:16–20` to `src-tauri/resources/libonnxruntime.dylib` when describing build artifacts. This is not a new dependency-lock/source edit claim; the reviewer did not access that build resource/cache.
3. **Receipt:75–76,90 overstate “bit-identical”.** Reproducer: `image_loader.rs:1316` compares f32 vectors with `==`; positive and negative zero compare equal although their bits differ. The measured claim is exact per-element f32 value equality with equal dimensions on the same build. Use that wording. A byte/to_bits assertion would be needed for a bit-identity claim, but no extra source/test change is required for this bounded guard acceptance.

The receipt appropriately leaves genuine X-T5 RAW, native Minolta fixture, Android build/device/UI, profiles/fidelity/performance and import staging/status Not exercised. Its private-payload `.jpg` test is an explicit test-only name passed to an in-memory loader, not a production rename/fallback and not a recommendation to substitute JPEG for RAW.

## Disposition and review receipt

**Accept — source**, frozen hashes above. **Revise — receipt prose only**, P3 items1–3. **Pass — independent static checks:** scope/hashes, exact frozen diff equality, guard placement, no widened detector, cancellation/error propagation, existing-path preservation, `git diff --check`. **Pass — inspected worker logs:** stated host test counts with the limits above. **Not exercised by reviewer:** all test execution, builds, devices, photo decoding, private payload validity, source/provider equality. **User-reported:** no new user/device acceptance adopted.

Read-only commands: `git rev-parse HEAD`, `git status --short`, `git diff --numstat`, frozen-diff comparison via Python/subprocess/hashlib, `git diff --check`, `nl`/`rg`/`cat`/`tail`, `shasum -a 256` over assigned source/receipt/log/binary. One base-comparison command initially had a mistyped SHA and exited128; corrected exact-SHA command returned0 and no source differences. No test binary execution or build-cache access.

Only new artifact is this report; final SHA-256 is delivered through Team messaging. Next consumer: lead imports the accepted two-source-file diff with receipt corrections and performs combined validation/device evidence under its own assignment. This review neither changes source nor expands scope.
