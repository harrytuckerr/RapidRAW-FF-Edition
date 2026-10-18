# X-Trans demosaic comparison tools

This is the existing exploratory comparison harness moved into a standalone crate. It is not yet the future X1 acceptance harness. The scripts retain their original calculations; this preparation only changes the standalone `rawler` git revision to the revision locked by the app. Source copying and a Python AST parse-only check were completed; no accepted benchmark or comparison was performed for this migration. A separate accidental metadata-probe render, cleanup of its output paths, and uncertainty about pre-existing same-named outputs are recorded in the private preparation receipt. Matching source-RAF hashes before and after do not establish preservation of any pre-existing output.

## Keep inputs and outputs private

Use only RAF files and Capture One exports you are authorized to compare. Pass their existing local paths to the tools; do not copy RAWs, TIFFs, crops, rendered frames, vendor profiles, or report images into this repository. Put the Rust prefix and every script output under a private directory outside the checkout, for example `/private/tmp/rapidraw-xtrans-run/`. The local `.gitignore` also excludes this crate's `target/`, generated `Cargo.lock`, conventional `out/` and `results/` folders, Python caches/environments, and common root-level output files. Check `git status` before sharing results; ignore rules are a backstop, not permission to store private media in the crate.

Example variables below are placeholders. Set `RAF`, `NEUTRAL_TIF`, and `DEFAULT_TIF` to the exact private input files for a run.

```sh
CRATE="$(pwd)/bench/demosaic_compare"
RUN="/private/tmp/rapidraw-xtrans-run"
RAF="/path/to/private/frame.RAF"
NEUTRAL_TIF="/path/to/private/capture-one-neutral.tif"
DEFAULT_TIF="/path/to/private/capture-one-default.tif"
mkdir -p "$RUN"
```

## Existing commands

The Rust program mirrors the app's current rawler `Quality` path and writes `<prefix>.f32` and `<prefix>.json`. Run it first if the comparison scripts need a current-path dump:

```sh
cargo run --manifest-path "$CRATE/Cargo.toml" --release -- "$RAF" "$RUN/current"
```

Python scripts use `rawpy`, `numpy`, `Pillow`, and `scipy`; `c1_compare.py` also imports `tifffile`. The current inspected Python installs do not include that comparison stack. If a later runtime grant permits setup, keep the environment outside the checkout:

```sh
/opt/homebrew/bin/python3.12 -m venv "$RUN/venv"
"$RUN/venv/bin/python" -m pip install rawpy numpy pillow scipy tifffile
PY="$RUN/venv/bin/python"
```

Run the scripts with isolated mode (`-I`) and keep their output paths in `RUN`:

```sh
"$PY" -I "$CRATE/compare.py" "$RAF" "$RUN/current" "$RUN/current-vs-3pass" "3180,2088" "3660,1615"
"$PY" -I "$CRATE/three_way.py" "$RAF" "$RUN/current" "$RUN/three-way.png" "3180,2088" "3660,1615"
"$PY" -I "$CRATE/screen_preview.py" "$RAF" "$RUN/current" "$RUN/screen-preview.png" 1920
"$PY" -I "$CRATE/c1_compare.py" --raf "$RAF" --rawler-prefix "$RUN/current" --out "$RUN/capture-one" --c1 "Capture One neutral=$NEUTRAL_TIF" --c1 "Capture One default=$DEFAULT_TIF" --passes 1
```

`compare.py` writes a comparison sheet, crop-location image, full renders, and a JSON report below its output directory. `three_way.py`, `screen_preview.py`, and `c1_compare.py` write their image/report outputs to the explicit output path. `pick_frames.py` samples RAF filenames from each supplied folder and reads embedded JPEG thumbnails; only give it an explicitly selected private folder, never a broad photo-library path:

```sh
python -I "$CRATE/pick_frames.py" "$RUN/frame-rank" "/path/to/one/designated/private-RAF-folder"
```

This standalone crate intentionally has no committed `Cargo.lock`, matching the requested migration. Its dependency graph has not been resolved or verified here; a later harness owner should freeze a lock strategy before treating runs as reproducible. These commands document current interfaces; they are not validated acceptance commands, and no accepted benchmark or comparison was produced for this migration.

## What the harness currently compares

- `src/main.rs` dumps the current rawler demosaic path after the app-like `u32::MAX` white-level handling and rescale. It does not implement a Markesteijn CPU algorithm or call an app API.
- `compare.py` compares current rawler output with LibRaw's X-Trans 3-pass path. `three_way.py` adds LibRaw 1-pass. `screen_preview.py` compares a resized preview and the experimental 3×3 mosaic binning reference; the latter is not a valid demosaic substitute. `c1_compare.py` compares those renders with supplied Capture One TIFF exports and reports crop-level high-frequency chroma and luma-gradient proxies. `compare.py` fits per-channel residual gains, and `c1_compare.py` independently fits per-channel histograms to the first Capture One export. These historical adjustments make the outputs exploratory, not final CPU/LibRaw, false-colour, colour-fidelity, or acceptance oracles. Freeze one shared geometry/tone protocol before using a future acceptance harness. The TIFF reader requires an embedded sRGB profile.
- `pick_frames.py` ranks supplied RAFs using embedded JPEG previews. Its sampling score is only a frame-selection heuristic.
- There is no `--algo` option, GPU implementation, pluggable app demosaic API, preprocessing toggle, repeated-run timing summary, or accepted CPU/GPU tolerance here. The Rust command reports one develop duration; the Python scripts report their own single-run timings.

## Later harness work, not implemented here

A future harness change needs a deliberate CPU/API design before adding options. The requested interface is `--algo current|markesteijn1|markesteijn3|gpu1`, plus at least three timed runs with min/median, thread count and machine load recorded, and an explicit option to include RapidRAW raw preprocessing. GPU output, app-route parity and the acceptance thresholds must remain unclaimed until their implementation, source ownership, oracle, and independent review are settled. Do not infer a passing threshold from the historical comparison files.

The existing Capture One results are black-box exports and the present metric values are proxies. This crate does not inspect Capture One software, establish Adobe colour fidelity, or prove that the editor and export use a future demosaic consistently.
