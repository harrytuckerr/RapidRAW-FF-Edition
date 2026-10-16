# GD1b5-S3B native registration — source-only receipt

2026-10-16 Melbourne. Task `01a14094-1888-75a3-8573-66cc91b1c98b`; Codex CLI - Sol, Team roster model `gpt-6.1-sol`, macOS arm64. Isolated `drive-desktop-registration`, detached `1dbeadf0ba2d0dee4ac4bd0467f6c5cabc3ae7cd`. Root retains module registration, independent review/runtime and integration authority.

Added only `src-tauri/src/drive_auth/registration.rs` plus this receipt. The pure native API implements `NativeRegistration::from_native_configuration`, `NativeRegistrationCatalog::from_native_records`, `RegistrationSelection::new` and immutable `catalog.resolve`. Fields are private; record/selector getters borrow existing strings/slices. Errors are payload-free typed variants with fixed Display text.

IDs accept 1–128 ASCII alphanumeric/`._-` bytes. Client IDs accept at most 256 such bytes, a nonempty prefix and exact `.apps.googleusercontent.com` suffix. Projects are canonical positive decimal u64, retained byte-for-byte. Every input bound precedes copying. Allowed/requested sets contain 1–2 unique existing AppData/File values; order is retained but resolution uses subset semantics. Catalogs accept 0–8 records, reject duplicate exact IDs or clients even across different projects, and resolve only the exact selector. Empty, unknown and disallowed selections fail separately without fallback or mutation. No real registrations, loader, normalization, inferred project, scope union or activation exists.

Read the full adoption contract `e1ea97d2cc024d82f6b046657ece99965ee1a83026865bd5fe48522e047c15fc`, lead review `ec61fe37642c3c039f3372dd0bdc7413dd2835ec845f423db4b8de7a08887782` and dispatch preparation `038709cf11f458a4ff080153ad5fe05cf821bf3525b7d320757def2dbcaa1b2d`. Planning's old 5a source is rebound to actual 1d: account 6b0ab79e, session 17ed9fec and PKCE 85bd9b48. B intentionally matches S2's positive-u64 project constraint; legacy NativeStart's 32-digit syntax is unchanged. Syntax validation and native caller provenance are not provider-attested mapping, identity or actual grants.

| Check | Result |
| --- | --- |
| Defined inline tests | 11 grouped definitions; **zero executed** |
| Standalone pinned rustfmt write/check | Pass, both exit 0; only the new file |
| New-file patch/whitespace check | Pass; exact roundtrip, no trailing whitespace/CR, final newline |
| Existing input custody | Pass; all 643 tracked files/index/HEAD unchanged; exact inventory adds only the new source and this receipt |
| Rust compilation, runtime tests, Clippy, canonical acceptance | Not exercised |
| Provider/account/browser/IPC/storage/device/UI/sync and C adoption | Not exercised |

The 11 definitions cover owned borrowed getters, ID/client lengths and syntax, canonical u64 maximum/overflow, invalid/ordered scope sets, empty/single/eight/nine catalogs, exact/conflicting duplicates in both orders, exact selection/no fallback, capability subsets, unchanged record after failed resolution and static nonreflecting errors. Synthetic values exist only in cfg(test) fixtures. They have not established runtime behavior.

Only these formatting invocations ran, using `<HOME>/.rustup/toolchains/1.98-aarch64-apple-darwin/bin/rustfmt` (`rustfmt 1.9.0-stable`, 48a229ceae): `--edition 2024 --config skip_children=true <new file>` and the same invocation with `--check`. Both exit 0; durations 0.045917458s and 0.021723250s. `RUSTUP_TOOLCHAIN=1.98`, `RUSTUP_AUTO_INSTALL=0`, `GIT_OPTIONAL_LOCKS=0`. No Cargo/rustc/tests/Clippy, dependency resolution/download, Git execution, shared-cache access or socket/provider/device activity occurred. No cache lease was acquired or used.

Frozen source: 19,864 B, SHA-256 `64291650a04a3a45b6d13b918f8ff664350187850589a03ed6ca378005919fb4`. Minimal new-file patch: 20,495 B, SHA-256 `876e3fa95f98eaef183715cc2d831f7ceb666aafdd618d869d2cd2b528fa4820`. Private evidence `/private/tmp/rapidraw-gd1b5-registration-implementation-20261016` contains exact source/patch, before/after 643-input manifests and inventories, pinned formatter command/result logs, environment, self-review notes, machine receipt and sealed artifact manifest. HEAD/index were read as data; no Git command ran. No Rust build/test binary was produced. Stop frozen for independent source review and separately authorized runtime validation; worker completion is not acceptance.

---

> Public redacted copy (2026-10-16). Personal home paths and physical device serials are display placeholders; embedded Git index payloads, where present, are summarized by hash/size/equality. Full original bytes are retained privately. Existing test outcomes, IDs and hashes describe the original evidence/source/artifacts, not this redacted document or placeholder paths. This forward edit does not purge history or release copies. Original document SHA-256: `71856197be68516aad60c9510579a2f36aece5206fcf7f1b98c4e06edb2e6e0a` (4392 bytes).
