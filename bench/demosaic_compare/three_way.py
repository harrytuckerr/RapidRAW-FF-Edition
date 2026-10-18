"""Three-column sheet: RapidRaw current | Markesteijn 1-pass | Markesteijn 3-pass,
same colour processing as compare.py, at fixed crop positions.

Usage: python three_way.py <input.RAF> <rawler_prefix> <out_png> x,y [x,y ...]
"""

import sys
import time
from pathlib import Path

import numpy as np
import rawpy
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare as c  # noqa: E402


def libraw(path: str, alg) -> tuple[np.ndarray, float, np.ndarray]:
    t = time.perf_counter()
    with rawpy.imread(path) as raw:
        rgb = raw.postprocess(
            demosaic_algorithm=alg,
            use_camera_wb=True,
            output_color=rawpy.ColorSpace.raw,
            gamma=(1, 1),
            no_auto_bright=True,
            output_bps=16,
            user_flip=0,
            highlight_mode=rawpy.HighlightMode.Clip,
        )
        cam_xyz = np.array(raw.rgb_xyz_matrix[:3, :3], dtype=np.float64)
    return rgb.astype(np.float32) / 65535.0, time.perf_counter() - t, cam_xyz


def main() -> None:
    raf, prefix, out_png = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    crops = [tuple(int(v) for v in p.split(",")) for p in sys.argv[4:]]

    cur, meta = c.load_current(prefix)
    wb = np.array(meta["wb_coeffs"][:3], dtype=np.float32)
    cur = np.clip(cur * (wb / wb[1]), 0, 1)
    one, t1, cam_xyz = libraw(raf, rawpy.DemosaicAlgorithm.PPG)  # quality 2 -> xtrans_interpolate(1)
    three, t3, _ = libraw(raf, rawpy.DemosaicAlgorithm.AHD)  # quality 3 -> xtrans_interpolate(3)

    dy, dx = c.find_shift(cur[..., 1], three[..., 1])
    cur_a, one_a = c.overlap(cur, one, dy, dx)
    _, three_a = c.overlap(cur, three, dy, dx)

    m = c.cam_to_srgb(cam_xyz)
    gain = 0.9 / max(np.percentile((three_a @ m.T)[..., 1], 99.5), 1e-6)
    imgs = [c.to_srgb8(x, m, gain) for x in (cur_a, one_a, three_a)]
    labels = [
        f"RapidRaw current ({meta['develop_ms'] / 1000:.1f} s)",
        f"Markesteijn 1-pass (LibRaw, {t1:.1f} s, 1 core)",
        f"Markesteijn 3-pass (LibRaw, {t3:.1f} s, 1 core)",
    ]

    size, zoom, gap, pad, head = c.CROP, 3, 10, 14, 56
    tile = size * zoom
    w = pad * 2 + 3 * tile + 2 * gap
    h = head + len(crops) * (tile + gap) + pad
    sheet = Image.new("RGB", (w, h), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((pad, 8), f"{Path(raf).name} — demosaic only, identical colour processing ({zoom * 100}%)",
           fill=(235, 235, 235), font=c.font(20))
    for i, lab in enumerate(labels):
        d.text((pad + i * (tile + gap), 34), lab, fill=(200, 200, 200), font=c.font(15))
    y = head
    for x0, y0 in crops:
        for i, img in enumerate(imgs):
            crop = Image.fromarray(img[y0 : y0 + size, x0 : x0 + size]).resize((tile, tile), Image.NEAREST)
            sheet.paste(crop, (pad + i * (tile + gap), y))
        y += tile + gap
    sheet.save(out_png, optimize=True)

    # How far 1-pass is from 3-pass, relative to how far the current demosaic is from 3-pass.
    def mad(a, b):
        return float(np.abs(a.astype(np.float32) - b.astype(np.float32)).mean())

    print(f"1-pass {t1:.2f} s, 3-pass {t3:.2f} s")
    print(f"mean abs diff vs 3-pass (8-bit sRGB): current {mad(imgs[0], imgs[2]):.2f}, 1-pass {mad(imgs[1], imgs[2]):.2f}")


if __name__ == "__main__":
    main()
