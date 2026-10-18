"""Rank raw files by fine detail (using their embedded JPEG previews) and save a
contact sheet of the top candidates.

Usage: python pick_frames.py <out_dir> <folder> [<folder> ...]
"""

import io
import json
import sys
from pathlib import Path

import numpy as np
import rawpy
from PIL import Image, ImageDraw
from scipy import ndimage

STEP = 2  # sample every Nth file per folder to keep this quick
TOP = 16


def detail_score(img: Image.Image) -> float:
    g = np.asarray(img.convert("L").resize((960, 640)), dtype=np.float32)
    # High-frequency energy: variance of the Laplacian, ignoring blown or crushed areas.
    lap = ndimage.laplace(g)
    mask = (g > 8) & (g < 247)
    return float(lap[mask].var()) if mask.any() else 0.0


def main() -> None:
    out_dir = Path(sys.argv[1])
    out_dir.mkdir(parents=True, exist_ok=True)
    files = []
    for folder in sys.argv[2:]:
        files += sorted(Path(folder).glob("*.RAF"))[::STEP]

    scored = []
    for f in files:
        try:
            with rawpy.imread(str(f)) as raw:
                thumb = raw.extract_thumb()
            if thumb.format != rawpy.ThumbFormat.JPEG:
                continue
            img = Image.open(io.BytesIO(thumb.data))
            scored.append((detail_score(img), str(f), img))
        except Exception as e:  # noqa: BLE001 - skip unreadable files
            print(f"skip {f.name}: {e}", file=sys.stderr)

    scored.sort(key=lambda t: t[0], reverse=True)
    top = scored[:TOP]

    tw, th, cols = 360, 240, 4
    sheet = Image.new("RGB", (cols * tw, ((len(top) + cols - 1) // cols) * (th + 18)), "white")
    draw = ImageDraw.Draw(sheet)
    for i, (score, path, img) in enumerate(top):
        x, y = (i % cols) * tw, (i // cols) * (th + 18)
        t = img.copy()
        t.thumbnail((tw - 4, th - 4))
        sheet.paste(t, (x + 2, y + 2))
        draw.text((x + 4, y + th), f"{i}: {Path(path).name}  {score:.0f}", fill="black")
    sheet.save(out_dir / "candidates.jpg", quality=85)
    (out_dir / "candidates.json").write_text(
        json.dumps([{"i": i, "file": p, "score": s} for i, (s, p, _) in enumerate(top)], indent=2)
    )
    print(f"scored {len(scored)} files; top {len(top)} saved")


if __name__ == "__main__":
    main()
