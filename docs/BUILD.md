# Build the RapidRAW Fujifilm edition

This edition combines the fork's Cobalt and Fuji work with upstream RapidRAW through `6e2631ad0885fcc7622c95d1d18a6f9146b96b91`. The RAW dependency is pinned to the public [Fuji RAW source](https://github.com/harrytuckerr/RapidRAW-DngLab/tree/43457211d28f11362932ec3e5602d7142a174722), revision `43457211d28f11362932ec3e5602d7142a174722`; no private checkout override is required.

Use the source archive and checksums attached to an APK release for that build's exact inputs and validation scope. Internal operational receipts are not part of the public source bundle. Release notes distinguish fresh validation, retained checks and known failures.

All shell blocks below start from the repository root. Run Rust commands from `src-tauri`, or prefix them with `cargo +1.98`; `--manifest-path` alone does not select the nested toolchain in `src-tauri/rust-toolchain.toml`. Each Rust block changes into `src-tauri` once, so start the next block again from the repository root.

## Desktop

Use Node.js 22 and the Rust toolchain selected in `src-tauri/rust-toolchain.toml` (Rust 1.98). Install the normal Tauri system dependencies for your host.

```sh
# From the repository root.
npm ci
npm run build
npm run test:profiles

cd src-tauri
cargo check --locked --lib
cargo fmt --all -- --check
cargo test --locked --lib
cargo clippy --locked --all-targets -- -D warnings
```

The desktop DCP profile route uses the shared CPU pipeline. The ignored `gpu_vs_cpu_parity` test below exercises optional shader parity; it does not exercise production GPU profile processing or establish Adobe, Android or physical-device acceptance. Run it only where a working graphics adapter is available:

```sh
# From the repository root.
cd src-tauri
cargo test --locked --lib gpu_vs_cpu_parity -- --ignored --nocapture
```

Optional `--all-features` builds also require the tethering system libraries for the host. Frontend lint, typecheck and i18n checks have inherited upstream errors; compare new diagnostics with the unmodified upstream baseline. A Vite build alone does not clear those gates.

For development, run `npm run start` from the repository root. For a packaged desktop build, run `npm run tauri build`.

## Android

Build from an isolated workspace using the checked-in Android dependency-lock bundle. The merged stack uses Tauri/CLI/API 2.12.1; the helper verifies the Rust and npm pins before building. Requirements are JDK 17, Android SDK platform 36 and build-tools 36.0.0, NDK r26d (`26.3.11579264`), and Rust 1.98 target `aarch64-linux-android`.

```sh
# From the repository root.
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/26.3.11579264"
rustup target add --toolchain 1.98 aarch64-linux-android

# Use a new workspace outside the checkout.
python3 scripts/android-build.py prepare \
  --workspace /private/tmp/rapidraw-next-android \
  --locks src-tauri/gen/android/dependency-locks
python3 scripts/android-build.py build \
  --workspace /private/tmp/rapidraw-next-android
```

`prepare` copies the source and the reviewed lock bundle into the isolated workspace. Review its source and lock receipts before building. Do not use `resolve` for routine builds: use it only for a deliberate dependency-lock update that will be reviewed and frozen. Do not run Cargo dependency updates against the desktop lockfile to make Android resolve.

The build script does not install the APK. Runtime evidence must record the device ID, Android/API and WebView versions, renderer/backend where available, APK hash, and the actions actually exercised. Record only actions actually exercised for the build being distributed. A successful package build or install does not establish profile import, persistence, export, speed or crash acceptance. A historical API 33 SwiftShader preview hang is an emulator result, not a physical-device result. See the current [checkpoint notes](releases/1.6.27.md) for the Fold-scoped validation and unresolved limitations. Never clear app data to hide a migration failure.

## Synthetic profile fixture

Generate redistributable synthetic inputs, then render through the shared selected-profile pipeline:

```sh
# From the repository root.
python3 scripts/integration-fixtures.py --output-dir /private/tmp/rapidraw-fixtures
cd src-tauri
cargo run --locked --bin validate_dcp -- render \
  --dcp /private/tmp/rapidraw-fixtures/synthetic-base.dcp \
  --raw /private/tmp/rapidraw-fixtures/synthetic-camera.dng \
  --output /private/tmp/rapidraw-fixtures/profile-render.tif
```

The renderer writes a linear sRGB working-space TIFF. It is not a display-encoded Adobe reference or proof of Adobe parity. The `validate_dcp` CLI has no implemented Adobe reference-comparison command. Keep commercial DCP/XMP files and private RAW/reference images outside version control.

Follow the [AGPL-3.0 license](../LICENSE) and retain the project's [upstream notices](../NOTICE.md) when distributing builds.
