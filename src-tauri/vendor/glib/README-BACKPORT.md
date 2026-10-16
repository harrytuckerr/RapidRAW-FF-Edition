# glib 0.18.5 security backport

This directory preserves the complete published Rust `glib` 0.18.5 crate, with
one upstream soundness fix and separately added regression tests. The version
remains **0.18.5**; this is a local backport, not an upstream 0.20 upgrade.

- Published archive: <https://static.crates.io/crates/glib/glib-0.18.5.crate>
- Archive SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`
- Official fix: <https://github.com/gtk-rs/gtk-rs-core/pull/1343>
- Exact author commit: `b5a4071e439bef2b5eea76c3aa25e5ae84839e34`
- Advisories: [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g),
  [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html)

The only modification to a published file is in `src/variant_iter.rs`:
`VariantStrIter::impl_get` declares its pointer variable mutable and passes
`&mut p` to the C output argument. Every other published file, including the
normalized manifest, `Cargo.toml.orig`, `COPYRIGHT`, and `LICENSE`, is unchanged.
This notice and the separate `security-tests/glib-variant-iter` regression
workspace are RapidRAW additions.
The archive's `.cargo_vcs_info.json` describes the original upstream source,
not this locally modified copy. No registry checksum metadata is fabricated.

GTK3 0.18.2 and WebKit2GTK 2.0.2 require the 0.18 glib line. The application
manifest applies `[patch.crates-io]` to this directory; both application and
Android lockfiles retain glib 0.18.5 and the same dependency edges, using the
local source. The Android preparation helper copies this relative vendor path
with the other source files before applying its frozen lock bundle.

The Rust crate is MIT licensed; its original `LICENSE` and contributor
`COPYRIGHT` are retained verbatim. This does not change the separate licenses
of linked C GLib/GObject/GIO libraries or other dependencies.

## Validation and advisory tracking

Source review is distinct from compilation and runtime validation. The four
new tests exercise the actual public borrowed-string iterator. They must be
compiled and run **optimized**, for example from an isolated copy:

```sh
cargo test --locked --release --manifest-path security-tests/glib-variant-iter/Cargo.toml --test variant_str_iter_backport
```

The standalone regression workspace freezes only its direct dependency on this
exact vendored crate and its transitive dependencies in its own Cargo.lock;
it does not select the vendored crate's unrelated upstream dev dependencies.
Root validation ran all four public-FFI tests optimized on an ARM64 macOS host
with Rust/Cargo 1.98.1 and system GLib 2.88.1: **4 Pass, 0 Fail**. Standalone
strict Clippy and formatting passed. The real Android preparation helper copied
the vendored files byte-for-byte; desktop and prepared Android application
`cargo metadata --locked --offline --no-deps` checks passed. These metadata
checks do not compile the application. The separate Ubuntu CI job also ran all four optimized tests successfully
([GitHub run 38881722250](https://github.com/harrytuckerr/RapidRAW-HT-Edition/actions/runs/38881722250)). Full Linux GTK/Tauri
builds and rebuilt APK/DMG acceptance remain **Not exercised**.

Advisory tools may continue to flag version 0.18.5, or may stop matching when
the source becomes a local path. Either result must be recorded literally.
Do not rename the crate to 0.20, suppress the advisory globally, or claim an
upstream release upgrade. Keep tracking the backport until a compatible
maintained dependency route replaces it and passes platform validation.
