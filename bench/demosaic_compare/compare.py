"""Compare RapidRaw's current X-Trans demosaic against Markesteijn 3-pass.

Both images go through identical colour processing (camera white balance,
the camera's Adobe D65 matrix to sRGB, sRGB gamma), so the only difference
is the demosaic.

  current     : linear camera RGB dumped by the Rust harness (rawler, as RapidRaw calls it)
  markesteijn : LibRaw xtrans_interpolate(3), via rawpy (demosaic quality 3)

Usage: python compare.py <input.RAF> <rawler_prefix> <out_dir> [x,y ...]
Extra x,y pairs add hand-picked crops (top-left corner, in the compared image).
"""

import json
import sys
import time
from pathlib import Path

import numpy as np
import rawpy
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

# XYZ (D65) -> linear sRGB, and its inverse direction for building cam->sRGB.
XYZ_TO_SRGB = np.array(
    [[3.2404542, -1.5371385, -0.4985314], [-0.9692660, 1.8760108, 0.0415560], [0.0556434, -0.2040259, 1.0572252]]
)
SRGB_TO_XYZ = np.linalg.inv(XYZ_TO_SRGB)

CROP = 140  # source pixels per crop
ZOOM = 4  # nearest-neighbour zoom, so individual pixels are visible
N_CROPS = 5


def load_current(prefix: str):
    meta = json.loads(Path(prefix + ".json").read_text())
    data = np.fromfile(prefix + ".f32", dtype="<f4").reshape(meta["height"], meta["width"], 3)
    return data, meta


def render_markesteijn(path: str):
    t = time.perf_counter()
    with rawpy.imread(path) as raw:
        rgb = raw.postprocess(
            demosaic_algorithm=rawpy.DemosaicAlgorithm.AHD,  # quality 3 -> xtrans_interpolate(3) for X-Trans
            use_camera_wb=True,
            output_color=rawpy.ColorSpace.raw,
            gamma=(1, 1),
            no_auto_bright=True,
            output_bps=16,
            user_flip=0,
            highlight_mode=rawpy.HighlightMode.Clip,
        )
        info = {
            "cam_xyz": np.array(raw.rgb_xyz_matrix[:3, :3], dtype=np.float64),
            "camera_wb": list(raw.camera_whitebalance),
            "black": list(raw.black_level_per_channel),
            "white": int(raw.white_level),
        }
    info["ms"] = int((time.perf_counter() - t) * 1000)
    return rgb.astype(np.float32) / 65535.0, info


def cam_to_srgb(cam_xyz: np.ndarray) -> np.ndarray:
    # Same construction as dcraw/LibRaw/rawler: rows of cam<-sRGB normalised to 1 so white stays white.
    cam_rgb = cam_xyz @ SRGB_TO_XYZ
    cam_rgb /= cam_rgb.sum(axis=1, keepdims=True)
    return np.linalg.inv(cam_rgb)


def find_shift(a: np.ndarray, b: np.ndarray, size: int = 2048):
    """Integer (dy, dx) such that a[y, x] ~ b[y - dy, x - dx], by phase correlation."""
    h, w = min(a.shape[0], b.shape[0]), min(a.shape[1], b.shape[1])
    y0, x0 = (h - size) // 2, (w - size) // 2
    pa = np.sqrt(np.clip(a[y0 : y0 + size, x0 : x0 + size], 0, None))
    pb = np.sqrt(np.clip(b[y0 : y0 + size, x0 : x0 + size], 0, None))
    win = np.outer(np.hanning(size), np.hanning(size))
    fa, fb = np.fft.fft2((pa - pa.mean()) * win), np.fft.fft2((pb - pb.mean()) * win)
    r = fa * np.conj(fb)
    corr = np.fft.ifft2(r / (np.abs(r) + 1e-12)).real
    dy, dx = np.unravel_index(np.argmax(corr), corr.shape)
    dy = dy - size if dy > size // 2 else dy
    dx = dx - size if dx > size // 2 else dx
    return int(dy), int(dx)


def overlap(a: np.ndarray, b: np.ndarray, dy: int, dx: int):
    """Crop a and b to their common area given a[y, x] ~ b[y - dy, x - dx]."""
    ay0, by0 = max(dy, 0), max(-dy, 0)
    ax0, bx0 = max(dx, 0), max(-dx, 0)
    h = min(a.shape[0] - ay0, b.shape[0] - by0)
    w = min(a.shape[1] - ax0, b.shape[1] - bx0)
    return a[ay0 : ay0 + h, ax0 : ax0 + w], b[by0 : by0 + h, bx0 : bx0 + w]


def to_srgb8(lin: np.ndarray, m: np.ndarray, gain: float) -> np.ndarray:
    rgb = np.clip((lin @ m.T) * gain, 0, 1)
    srgb = np.where(rgb <= 0.0031308, rgb * 12.92, 1.055 * np.power(rgb, 1 / 2.4) - 0.055)
    return (srgb * 255 + 0.5).astype(np.uint8)


def pick_crops(a8: np.ndarray, b8: np.ndarray, n: int):
    """Windows where the two demosaics differ most (non-overlapping, away from borders)."""
    diff = np.abs(a8.astype(np.float32) - b8.astype(np.float32)).mean(axis=2)
    score = ndimage.uniform_filter(diff, size=CROP)
    luma = a8.mean(axis=2)
    score[(ndimage.uniform_filter(luma, size=CROP) > 235)] = 0  # skip blown areas
    m = CROP
    score[:m], score[-m:], score[:, :m], score[:, -m:] = 0, 0, 0, 0
    picks = []
    for _ in range(n):
        y, x = np.unravel_index(np.argmax(score), score.shape)
        picks.append((int(y - CROP // 2), int(x - CROP // 2)))
        score[max(0, y - 2 * CROP) : y + 2 * CROP, max(0, x - 2 * CROP) : x + 2 * CROP] = 0
    return picks


def font(size: int):
    for p in ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/SFNS.ttf"):
        try:
            return ImageFont.truetype(p, size)
        except OSError:
            continue
    return ImageFont.load_default()


def build_sheet(a8, b8, picks, labels, title) -> Image.Image:
    tile = CROP * ZOOM
    gap, head, pad = 12, 70, 16
    w = pad * 2 + tile * 2 + gap
    h = head + len(picks) * (tile + gap + 26) + pad
    sheet = Image.new("RGB", (w, h), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((pad, 10), title, fill=(235, 235, 235), font=font(22))
    for i, lab in enumerate(labels):
        d.text((pad + i * (tile + gap), 42), lab, fill=(200, 200, 200), font=font(18))
    y = head
    for n, (cy, cx, how) in enumerate(picks, 1):
        for i, img in enumerate((a8, b8)):
            crop = Image.fromarray(img[cy : cy + CROP, cx : cx + CROP]).resize((tile, tile), Image.NEAREST)
            sheet.paste(crop, (pad + i * (tile + gap), y))
        note = f"Crop {n}  {how}  (x={cx}, y={cy}, {CROP}px at {ZOOM * 100}%)"
        d.text((pad, y + tile + 4), note, fill=(170, 170, 170), font=font(16))
        y += tile + gap + 26
    return sheet


def build_overview(img8, picks) -> Image.Image:
    im = Image.fromarray(img8)
    scale = 1600 / im.width
    im = im.resize((1600, int(im.height * scale)), Image.LANCZOS)
    d = ImageDraw.Draw(im)
    for n, (cy, cx, _how) in enumerate(picks, 1):
        box = [cx * scale, cy * scale, (cx + CROP) * scale, (cy + CROP) * scale]
        d.rectangle(box, outline=(255, 210, 0), width=3)
        d.text((box[2] + 4, box[1]), str(n), fill=(255, 210, 0), font=font(26))
    return im


def main() -> None:
    raf, prefix, out_dir = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    out_dir.mkdir(parents=True, exist_ok=True)

    cur, meta = load_current(prefix)
    mk, info = render_markesteijn(raf)

    # Camera white balance on the rawler output (LibRaw applies it before demosaicing).
    wb = np.array(meta["wb_coeffs"][:3], dtype=np.float32)
    cur = np.clip(cur * (wb / wb[1]), 0, 1)

    dy, dx = find_shift(cur[..., 1], mk[..., 1])
    cur, mk = overlap(cur, mk, dy, dx)

    # Residual per-channel gain between the two decoders (black/white normalisation differs slightly).
    valid = np.all((cur > 0.03) & (cur < 0.8) & (mk > 0.03) & (mk < 0.8), axis=2)
    gains = np.array([np.median(mk[..., c][valid] / cur[..., c][valid]) for c in range(3)], dtype=np.float32)
    cur = cur * gains

    m = cam_to_srgb(info["cam_xyz"])
    lum = (mk @ m.T)[..., 1]
    view_gain = 0.9 / max(np.percentile(lum, 99.5), 1e-6)
    cur8, mk8 = to_srgb8(cur, m, view_gain), to_srgb8(mk, m, view_gain)

    picks = [(y, x, "— biggest difference") for y, x in pick_crops(cur8, mk8, N_CROPS)]
    for pair in sys.argv[4:]:
        x, y = (int(v) for v in pair.split(","))
        picks.append((y, x, "— hand-picked"))
    labels = ["RapidRaw current (rawler XTransDemosaic)", "Markesteijn 3-pass (LibRaw)"]
    title = f"{Path(raf).name} — {meta['make']} {meta['model']} — demosaic only, identical colour processing"
    build_sheet(cur8, mk8, picks, labels, title).save(out_dir / "side_by_side.png", optimize=True)
    build_overview(mk8, picks).save(out_dir / "crop_locations.jpg", quality=90)
    Image.fromarray(cur8).save(out_dir / "full_current.jpg", quality=92)
    Image.fromarray(mk8).save(out_dir / "full_markesteijn.jpg", quality=92)

    # Neighbouring shifts should all be worse if the alignment is right.
    def mad(sy, sx):
        a, b = overlap(cur[..., 1], mk[..., 1], sy, sx)
        return float(np.abs(a[1000:2000, 1000:2000] - b[1000:2000, 1000:2000]).mean())

    report = {
        "file": raf,
        "shift_dy_dx": [dy, dx],
        "alignment_mad": {"best": mad(0, 0), "y+1": mad(1, 0), "x+1": mad(0, 1)},
        "channel_gains_applied_to_current": gains.round(4).tolist(),
        "rawler_wb": meta["wb_coeffs"],
        "libraw_camera_wb": info["camera_wb"],
        "rawler_black_white": [meta["black"], meta["white"]],
        "libraw_black_white": [info["black"], info["white"]],
        "rawler_develop_ms": meta["develop_ms"],
        "libraw_total_ms": info["ms"],
        "crops_xy": [[x, y] for y, x, _how in picks],
        "size_compared": list(cur8.shape[:2]),
    }
    (out_dir / "report.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
