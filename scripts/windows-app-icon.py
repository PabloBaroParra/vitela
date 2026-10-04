"""Regenerates the Windows shell's multi-size .ico from the brand mark.

    python scripts/windows-app-icon.py

Reuses the rasteriser of scripts/windows-store-images.py (Microsoft Edge renders
assets/brand/vitela-app-mark.svg; Pillow scales), so the exe icon, the title bar
icon and the MSIX tiles all come from one source. Output:

    apps/windows/Pdf.Windows/vitela.ico   16, 20, 24, 32, 40, 48, 64, 256 px

The navy mark disappears on a dark taskbar and an .ico carries no theme
variant, so the mark sits on a white rounded plate that reads on both.
"""

from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
OUT = HERE.parent / "apps" / "windows" / "Pdf.Windows" / "vitela.ico"
PLATE = (255, 255, 255, 255)
PLATE_RADIUS = 0.22
MARK_SHARE = 0.70
SUPERSAMPLE = 4


def load_store_images():
    spec = importlib.util.spec_from_file_location("windows_store_images", HERE / "windows-store-images.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def icon_frame(mark: Image.Image, compose, size: int) -> Image.Image:
    big = size * SUPERSAMPLE
    plate = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(plate).rounded_rectangle((0, 0, big - 1, big - 1), radius=round(big * PLATE_RADIUS), fill=PLATE)
    plate.alpha_composite(compose(mark, big, big, MARK_SHARE))
    return plate.resize((size, size), Image.LANCZOS)


def main() -> None:
    store = load_store_images()
    edge = next((path for path in store.EDGE_CANDIDATES if path.exists()), None)
    if edge is None:
        raise SystemExit("Microsoft Edge not found; it rasterises the SVG")
    with tempfile.TemporaryDirectory() as tmp:
        mark = store.render_master(edge, Path(tmp), store.MARK)
    frames = [icon_frame(mark, store.compose, size) for size in SIZES]
    frames[-1].save(OUT, format="ICO", sizes=[(size, size) for size in SIZES], append_images=frames[:-1])
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
