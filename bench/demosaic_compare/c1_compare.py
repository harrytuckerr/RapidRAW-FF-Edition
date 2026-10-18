"""Black-box comparison against Capture One exports.

Columns: RapidRaw current demosaic | Markesteijn (stand-in for X1, via LibRaw) | each
Capture One export. Only exported pixels are compared; nothing of Capture One is
inspected beyond its output TIFFs.

Capture One exports must be 16-bit (or 8-bit) TIFFs encoded in sRGB with the
profile embedded. Our renders are tone-matched to the first Capture One export
(per-channel histogram matching) so the crops compare detail, not tone. Later
exports (e.g. Capture One defaults) are shown as exported.

Usage:
  python c1_compare.py --raf X.RAF --rawler-prefix P --out DIR \
      --c1 "Capture One neutral=/path/neutral.tif" [--c1 "Capture One default=/path/default.tif"] \
      [--passes 1|3] [--crops "3180,2088 3660,1615 3518,393 2200,2400"]
"""

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import rawpy
import tifffile
from PIL import Image, ImageDraw
from scipy import ndimage

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare as c  # noqa: E402

DEFAULT_CROPS = "3180,2088 3660,1615 3518,393 2200,2400"


def srgb_decode(v: np.ndarray) -> np.ndarray:
    return np.where(v <= 0.04045, v / 12.92, np.power((v + 0.055) / 1.055, 2.4))


def srgb_encode(v: np.ndarray) -> np.ndarray:
    v = np.clip(v, 0, 1)
    return np.where(v <= 0.0031308, v * 12.92, 1.055 * np.power(v, 1 / 2.4) - 0.055)


def icc_description(icc: bytes) -> str:
    """Profile description from an ICC blob ('desc' or 'mluc' tag), or ''."""
    try:
        count = int.from_bytes(icc[128:132], "big")
        for i in range(count):
            sig, off, size = (icc[132 + 12 * i : 136 + 12 * i], *(
                int.from_bytes(icc[136 + 12 * i + k : 140 + 12 * i + k], "big") for k in (0, 4)))
            if sig != b"desc":
                continue
            tag = icc[off : off + size]
            if tag[:4] == b"desc":
                n = int.from_bytes(tag[8:12], "big")
                return tag[12 : 12 + n].split(b"\0")[0].decode("latin-1")
            if tag[:4] == b"mluc":
                rec_len, rec_off = int.from_bytes(tag[20:24], "big"), int.from_bytes(tag[24:28], "big")
                return tag[rec_off : rec_off + rec_len].decode("utf-16-be", "replace")
    except Exception:  # noqa: BLE001 - malformed profile, treated as unknown
        pass
    return ""


def load_c1(path: str) -> tuple[np.ndarray, str]:
    """Returns linear sRGB float image and the embedded profile name."""
    with tifffile.TiffFile(path) as tf:
        page = tf.pages[0]
        arr = page.asarray()
        icc_tag = page.tags.get("InterColorProfile")
        icc = bytes(icc_tag.value) if icc_tag is not None else b""
    desc = icc_description(icc) if icc else ""
    if "srgb" not in desc.lower():
        raise SystemExit(f"{path}: embedded profile is '{desc or 'none'}'. Export as sRGB with the profile embedded.")
    if arr.ndim != 3 or arr.shape[2] < 3:
        raise SystemExit(f"{path}: expected an RGB TIFF, got shape {arr.shape}")
    scale = 65535.0 if arr.dtype == np.uint16 else 255.0
    return srgb_decode(arr[..., :3].astype(np.float32) / scale), desc


def match_histograms(src: np.ndarray, ref: np.ndarray, sample: int = 2_000_000) -> list:
    """Per-channel quantile maps from src to ref (both sRGB-encoded floats)."""
    rng = np.random.default_rng(0)
    maps = []
    for ch in range(3):
        s, r = src[..., ch].ravel(), ref[..., ch].ravel()
        s = np.sort(s[rng.integers(0, s.size, min(sample, s.size))])
        r = np.sort(r[rng.integers(0, r.size, min(sample, r.size))])
        q = np.linspace(0, 1, 1025)
        maps.append((np.quantile(s, q), np.quantile(r, q)))
    return maps


def apply_maps(img: np.ndarray, maps: list) -> np.ndarray:
    return np.stack([np.interp(img[..., ch], *maps[ch]) for ch in range(3)], axis=2)


def metrics(enc: np.ndarray) -> dict:
    """False-colour proxy (high-frequency chroma RMS) and sharpness proxy (mean luma gradient)."""
    y = 0.2126 * enc[..., 0] + 0.7152 * enc[..., 1] + 0.0722 * enc[..., 2]
    cb, cr = enc[..., 2] - y, enc[..., 0] - y
    hf = [ch - ndimage.uniform_filter(ch, 5) for ch in (cb, cr)]
    grad = np.hypot(ndimage.sobel(y, 0), ndimage.sobel(y, 1))
    return {"hf_chroma_rms": float(np.sqrt(np.mean(hf[0] ** 2 + hf[1] ** 2)) * 255), "luma_gradient": float(grad.mean() * 255)}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--raf", required=True)
    ap.add_argument("--rawler-prefix", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--c1", action="append", required=True, help='"Label=/path/file.tif"')
    ap.add_argument("--passes", type=int, choices=(1, 3), default=1)
    ap.add_argument("--crops", default=DEFAULT_CROPS)
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    crops = [tuple(int(v) for v in p.split(",")) for p in args.crops.split()]

    # Our two renders, aligned to the rawler/RapidRaw frame, in linear sRGB.
    cur, meta = c.load_current(args.rawler_prefix)
    wb = np.array(meta["wb_coeffs"][:3], dtype=np.float32)
    cur = np.clip(cur * (wb / wb[1]), 0, 1)
    alg = rawpy.DemosaicAlgorithm.AHD if args.passes == 3 else rawpy.DemosaicAlgorithm.PPG
    with rawpy.imread(args.raf) as raw:
        mk = raw.postprocess(demosaic_algorithm=alg, use_camera_wb=True, output_color=rawpy.ColorSpace.raw,
                             gamma=(1, 1), no_auto_bright=True, output_bps=16, user_flip=0,
                             highlight_mode=rawpy.HighlightMode.Clip).astype(np.float32) / 65535.0
        cam_xyz = np.array(raw.rgb_xyz_matrix[:3, :3], dtype=np.float64)
    dy, dx = c.find_shift(cur[..., 1], mk[..., 1])
    cur, mk = c.overlap(cur, mk, dy, dx)
    m = c.cam_to_srgb(cam_xyz)
    cur_lin, mk_lin = np.clip(cur @ m.T, 0, None), np.clip(mk @ m.T, 0, None)

    # Capture One exports, aligned to our frame.
    c1 = []
    for spec in args.c1:
        label, path = spec.split("=", 1)
        img, desc = load_c1(path)
        best = None
        for k in (0, 1, 3, 2):  # try orientations in case the export was rotated
            cand = np.rot90(img, k)
            if abs(cand.shape[0] / cand.shape[1] - mk_lin.shape[0] / mk_lin.shape[1]) > 0.05:
                continue
            sy, sx = c.find_shift(mk_lin[..., 1], cand[..., 1])
            a, b = c.overlap(mk_lin[..., 1], cand[..., 1], sy, sx)
            score = float(np.abs(srgb_encode(a[::4, ::4]) - srgb_encode(b[::4, ::4])).mean())
            if best is None or score < best[0]:
                best = (score, k, sy, sx, cand)
        if best is None:
            raise SystemExit(f"{path}: aspect ratio does not match the RAW")
        _, k, sy, sx, cand = best

        def mad(oy, ox, cand=cand, sy=sy, sx=sx):
            a, b = c.overlap(mk_lin[..., 1], cand[..., 1], sy + oy, sx + ox)
            h, w = a.shape
            return float(np.abs(srgb_encode(a[h // 3 : h // 3 + 800, w // 3 : w // 3 + 800])
                                - srgb_encode(b[h // 3 : h // 3 + 800, w // 3 : w // 3 + 800])).mean())

        c1.append({"label": label, "path": path, "profile": desc, "img": cand, "rot90": k, "shift": (sy, sx),
                   "alignment_mad": {"best": mad(0, 0), "y+1": mad(1, 0), "x+1": mad(0, 1)}, "size": cand.shape[:2]})

    # Tone-match our renders to the first Capture One export (encoded domain), for display and metrics.
    ref = c1[0]
    a_cur, ref_img = c.overlap(srgb_encode(cur_lin), srgb_encode(ref["img"]), *ref["shift"])
    a_mk, _ = c.overlap(srgb_encode(mk_lin), srgb_encode(ref["img"]), *ref["shift"])
    cur_enc = apply_maps(srgb_encode(cur_lin), match_histograms(a_cur, ref_img))
    mk_enc = apply_maps(srgb_encode(mk_lin), match_histograms(a_mk, ref_img))

    def local_shift(entry, x, y, s, patch=256):
        """Refine the global shift around one crop (handles lens-corrected exports)."""
        sy, sx = entry["shift"]
        img = entry["img"]
        cy, cx = y + s // 2 - patch // 2, x + s // 2 - patch // 2
        if cy < 0 or cx < 0 or cy + patch > mk_lin.shape[0] or cx + patch > mk_lin.shape[1]:
            return sy, sx
        yy, xx = cy - sy, cx - sx
        if yy < 0 or xx < 0 or yy + patch > img.shape[0] or xx + patch > img.shape[1]:
            return sy, sx
        ref = mk_lin[cy : cy + patch, cx : cx + patch, 1]
        cand = img[yy : yy + patch, xx : xx + patch, 1]
        dy, dx = c.find_shift(ref, cand, size=patch)
        return (sy + dy, sx + dx) if max(abs(dy), abs(dx)) < patch // 4 else (sy, sx)

    def c1_crop(entry, x, y, s):
        sy, sx = local_shift(entry, x, y, s)
        yy, xx = y - sy, x - sx
        img = entry["img"]
        if yy < 0 or xx < 0 or yy + s > img.shape[0] or xx + s > img.shape[1]:
            return None
        return srgb_encode(img[yy : yy + s, xx : xx + s])

    labels = ["RapidRaw current", f"Markesteijn {args.passes}-pass (LibRaw, X1 stand-in)"] + [e["label"] for e in c1]
    size, zoom, gap, pad, head = c.CROP, 3, 10, 14, 56
    tile = size * zoom
    cols = len(labels)
    sheet = Image.new("RGB", (pad * 2 + cols * tile + (cols - 1) * gap, head + len(crops) * (tile + gap + 22) + pad), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((pad, 8), f"{Path(args.raf).name} — 300% crops; our renders tone-matched to '{c1[0]['label']}'",
           fill=(235, 235, 235), font=c.font(20))
    for i, lab in enumerate(labels):
        d.text((pad + i * (tile + gap), 34), lab, fill=(200, 200, 200), font=c.font(15))

    table = []
    y0 = head
    for n, (x, y) in enumerate(crops, 1):
        tiles = [cur_enc[y : y + size, x : x + size], mk_enc[y : y + size, x : x + size]]
        tiles += [c1_crop(e, x, y, size) for e in c1]
        row = {"crop": n, "x": x, "y": y}
        for i, t in enumerate(tiles):
            if t is None:
                continue
            sheet.paste(Image.fromarray((np.clip(t, 0, 1) * 255 + 0.5).astype(np.uint8)).resize((tile, tile), Image.NEAREST),
                        (pad + i * (tile + gap), y0))
            row[labels[i]] = metrics(t)
        d.text((pad, y0 + tile + 3), f"Crop {n} (x={x}, y={y})", fill=(170, 170, 170), font=c.font(14))
        table.append(row)
        y0 += tile + gap + 22
    sheet.save(out / "capture_one_comparison.png", optimize=True)

    report = {
        "raf": args.raf,
        "markesteijn_passes": args.passes,
        "capture_one": [{k: v for k, v in e.items() if k != "img"} for e in c1],
        "crops": table,
        "notes": "hf_chroma_rms: lower = less false colour (also less real colour texture). "
                 "luma_gradient: higher = sharper (also more noise/sharpening). Compare within a crop.",
    }
    (out / "capture_one_report.json").write_text(json.dumps(report, indent=2, default=list))
    print(json.dumps(report, indent=2, default=list))


if __name__ == "__main__":
    main()
