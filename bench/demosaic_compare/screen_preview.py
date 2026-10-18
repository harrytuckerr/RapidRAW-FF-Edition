"""At editor-preview size (1920 px), compare:
  - RapidRaw current demosaic, downscaled
  - Markesteijn 1-pass (LibRaw), downscaled
  - 3x3 binning straight from the X-Trans mosaic (every aligned 3x3 block holds
    2 R, 5 G, 2 B), downscaled from 2000 px: no interpolation at all

All three get identical colour processing; resizing happens in linear light.

Usage: python screen_preview.py <input.RAF> <rawler_prefix> <out_png> [preview_px]
"""

import sys
import time
from pathlib import Path

import numpy as np
import rawpy
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare as c  # noqa: E402


def bin3x3(raw: rawpy.RawPy) -> tuple[np.ndarray, float]:
    t = time.perf_counter()
    cfa = raw.raw_image_visible.astype(np.float32)
    col = raw.raw_colors_visible.copy()
    col[col == 3] = 1  # second green index, if any
    black = float(np.mean(raw.black_level_per_channel[:3]))
    cfa = (cfa - black) / (raw.white_level - black)
    h, w = (cfa.shape[0] // 3) * 3, (cfa.shape[1] // 3) * 3
    cfa, col = cfa[:h, :w], col[:h, :w]
    out = np.empty((h // 3, w // 3, 3), np.float32)
    for ch in range(3):
        m = (col == ch).astype(np.float32)
        s = (cfa * m).reshape(h // 3, 3, w // 3, 3).sum(axis=(1, 3))
        n = m.reshape(h // 3, 3, w // 3, 3).sum(axis=(1, 3))
        out[..., ch] = s / np.maximum(n, 1)
    return out, time.perf_counter() - t


def resize_linear(img: np.ndarray, width: int) -> np.ndarray:
    h = round(img.shape[0] * width / img.shape[1])
    chans = [np.asarray(Image.fromarray(img[..., k]).resize((width, h), Image.LANCZOS)) for k in range(3)]
    return np.stack(chans, axis=2)


def main() -> None:
    raf, prefix, out_png = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    px = int(sys.argv[4]) if len(sys.argv) > 4 else 1920

    cur, meta = c.load_current(prefix)
    wb = np.array(meta["wb_coeffs"][:3], dtype=np.float32)
    wbn = wb / wb[1]
    cur = np.clip(cur * wbn, 0, 1)

    with rawpy.imread(raf) as raw:
        binned, t_bin = bin3x3(raw)
        cam_xyz = np.array(raw.rgb_xyz_matrix[:3, :3], dtype=np.float64)
    binned = np.clip(binned * wbn, 0, 1)
    with rawpy.imread(raf) as raw:
        t = time.perf_counter()
        mk = raw.postprocess(demosaic_algorithm=rawpy.DemosaicAlgorithm.PPG, use_camera_wb=True,
                             output_color=rawpy.ColorSpace.raw, gamma=(1, 1), no_auto_bright=True,
                             output_bps=16, user_flip=0, highlight_mode=rawpy.HighlightMode.Clip)
        t_mk = time.perf_counter() - t
    mk = mk.astype(np.float32) / 65535.0

    # Same framing for all three: crop LibRaw and binned to rawler's 16 px default crop.
    mk = mk[16 : 16 + cur.shape[0], 16 : 16 + cur.shape[1]]
    binned = binned[16 // 3 : 16 // 3 + cur.shape[0] // 3, 16 // 3 : 16 // 3 + cur.shape[1] // 3]

    small = [resize_linear(x, px) for x in (cur, mk, binned)]
    m = c.cam_to_srgb(cam_xyz)
    gain = 0.9 / max(np.percentile((small[1] @ m.T)[..., 1], 99.5), 1e-6)
    s8 = [c.to_srgb8(x, m, gain) for x in small]

    def mad(a, b):
        return float(np.abs(a.astype(np.float32) - b.astype(np.float32)).mean())

    # Crops at the same scene positions as the full-res sheets, at 300% of preview size.
    f = px / cur.shape[1]
    spots = [(3180, 2088), (3660, 1615), (3518, 393), (2200, 2400)]
    size, zoom = 110, 3
    tile = size * zoom
    gap, pad, head = 10, 14, 56
    sheet = Image.new("RGB", (pad * 2 + 3 * tile + 2 * gap, head + len(spots) * (tile + gap) + pad), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((pad, 8), f"{Path(raf).name} at editor preview size ({px} px wide), shown at {zoom * 100}%",
           fill=(235, 235, 235), font=c.font(20))
    labels = ["RapidRaw current, downscaled", "Markesteijn 1-pass, downscaled", "3x3 binning (no demosaic), downscaled"]
    for i, lab in enumerate(labels):
        d.text((pad + i * (tile + gap), 34), lab, fill=(200, 200, 200), font=c.font(15))
    y = head
    for sx, sy in spots:
        cx = max(0, int((sx + 70) * f) - size // 2)
        cy = max(0, int((sy + 70) * f) - size // 2)
        for i, img in enumerate(s8):
            crop = Image.fromarray(img[cy : cy + size, cx : cx + size]).resize((tile, tile), Image.NEAREST)
            sheet.paste(crop, (pad + i * (tile + gap), y))
        y += tile + gap
    sheet.save(out_png, optimize=True)

    print(f"preview {px}px: mean abs diff vs Markesteijn — current {mad(s8[0], s8[1]):.2f}, binned {mad(s8[2], s8[1]):.2f}")
    print(f"timings (single-core, Python/LibRaw): 3x3 bin {t_bin:.2f} s (numpy), Markesteijn 1-pass {t_mk:.2f} s")


if __name__ == "__main__":
    main()
