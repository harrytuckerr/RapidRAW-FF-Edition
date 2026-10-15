# Build and validate the upstream-integrated fork

Integration baseline (15 October 2026): upstream `3385c9f0` plus Cobalt `6f171d9b`.
Use `docs/integration/STATUS-2026-10-15.md` for actual acceptance evidence. Instructions are not claims that every platform has passed.

## Desktop

Requirements: Rust 1.98 (selected by `src-tauri/rust-toolchain.toml`), Node 22, npm, and the normal Tauri system dependencies. The locally provisioned Rust patch version is 1.98.1. Run Rust commands **inside src-tauri**, or explicitly use `cargo +1.98`: passing only `--manifest-path` from the repository root does not select the nested toolchain.

```sh
npm ci
npm run build
npm run test:profiles
cd src-tauri
cargo check --locked --lib
cargo fmt --all -- --check
cargo test --locked --lib
cargo test --locked --lib gpu_vs_cpu_parity -- --ignored --nocapture
cargo clippy --locked --all-targets -- -D warnings
```

The default suite validates the production WGSL variants without a GPU. GPU parity is deliberately separate and requires a working graphics adapter. An ignored test is not a pass. Optional `--all-features` also requires the tethering system libraries on supported desktop platforms.

For development, run `npm run start` at the root. For a packaged desktop release, run `npm run tauri build`. Large debug builds can consume several GB; isolated runs may set `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_DEV_INCREMENTAL=false` to bound storage.

Frontend lint/typecheck/i18n have inherited errors. Compare new diagnostics against unmodified upstream rather than claiming those gates pass because Vite builds.

## Android

Use the isolated preparation/build script. Do not downgrade the desktop lockfile with ad hoc `cargo update` calls or assume a plain local Tauri build matches upstream's Android CI pins.

Required components: JDK17, SDK platform36/build-tools36.0.0, NDK r26d (`26.3.11579264`), and Rust1.98 target `aarch64-linux-android`.

```sh
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/26.3.11579264"
rustup target add --toolchain 1.98 aarch64-linux-android
python3 scripts/android-build.py resolve --workspace /private/tmp/rapidraw-android-resolved
# Review and preserve the emitted Android locks and source receipt.
python3 scripts/android-build.py build --workspace /private/tmp/rapidraw-android-resolved
```

For repeat runs, use `prepare --locks PATH_TO_REVIEWED_ANDROID_LOCKS --workspace NEW_PATH` rather than resolving again. The source must have no unresolved Git merge entries. See `docs/android-integration-2026-10-15.md` for pins, signing, CI and current limits.

Builds never install automatically. Runtime acceptance must record device ID, API/WebView/GPU, APK hash, startup/editor rendering, profile picker success/cancel, persistence and export. Emulator acceptance does not establish physical GPU or device continuity acceptance. Never clear app data to conceal migration failures.

## Reproducible profile fixtures

```sh
python3 scripts/integration-fixtures.py --output-dir /private/tmp/rapidraw-fixtures
cd src-tauri
cargo run --locked --bin validate_dcp -- render \
  --dcp /private/tmp/rapidraw-fixtures/synthetic-base.dcp \
  --raw /private/tmp/rapidraw-fixtures/synthetic-camera.dng \
  --output /private/tmp/rapidraw-fixtures/profile-render.tif
```

These generated files are synthetic, not commercial assets. The validation renderer writes linear working-space TIFF; it is not a display-encoded Adobe reference or proof of Adobe parity. Commercial DCP/XMP and photographer RAW/reference files remain outside version control.
