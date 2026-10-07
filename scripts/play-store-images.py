"""Regenerates the Google Play listing graphics from the brand mark.

Play Console asks for a 512 x 512 app icon and a 1024 x 500 feature graphic
(docs/store/android-listing.md). Both are derived from assets/brand rather
than drawn by hand, so a brand change is one command away:

    python scripts/play-store-images.py

Requirements: Pillow, and Microsoft Edge (preinstalled on Windows 10/11). Edge
lays out and rasterises each graphic as an HTML page at its exact size, the
same renderer scripts/windows-store-images.py uses for the MSIX tiles. Pillow
only checks the result and drops the alpha channel Play does not want.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parent.parent
MARK = REPO / "assets" / "brand" / "vitela-app-mark.svg"
# Light-on-dark variant of the mark, for the navy feature graphic.
MARK_DARK = REPO / "assets" / "brand" / "vitela-app-mark-dark.svg"
OUT = REPO / "docs" / "store" / "play"
EDGE_CANDIDATES = [
    Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft/Edge/Application/msedge.exe",
    Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "Microsoft/Edge/Application/msedge.exe",
]
NAVY = "#1B2A57"
WHITE = "#FFFFFF"
MIST = "#C3CEE8"

# The launcher icon (res/drawable/ic_launcher_*.xml): the navy mark on white,
# filling about two thirds of the visible area. Play masks the square itself,
# so the icon is full-bleed with no rounded corners or shadow of its own.
ICON_HTML = f"""
<body style="margin:0;width:512px;height:512px;background:{WHITE};
             display:flex;align-items:center;justify-content:center">
  <img src="{MARK.as_uri()}" style="width:320px;height:320px">
</body>"""

# Play may crop the edges of the feature graphic on some surfaces, so the
# content sits well inside the frame.
FEATURE_HTML = f"""
<body style="margin:0;width:1024px;height:500px;background:{NAVY};
             display:flex;align-items:center;justify-content:center;gap:56px;
             font-family:'Segoe UI',sans-serif">
  <img src="{MARK_DARK.as_uri()}" style="width:220px;height:220px">
  <div>
    <div style="color:{WHITE};font-size:96px;font-weight:600;line-height:1">Vitela</div>
    <div style="color:{MIST};font-size:34px;margin-top:20px">Private, offline PDF editor</div>
  </div>
</body>"""

GRAPHICS = [
    ("icon-512.png", 512, 512, ICON_HTML),
    ("feature-graphic-1024x500.png", 1024, 500, FEATURE_HTML),
]


def render(edge: Path, work: Path, name: str, width: int, height: int, html: str) -> Image.Image:
    page = work / f"{name}.html"
    page.write_text(f"<!doctype html><html>{html}</html>", encoding="utf-8")
    shot = work / name
    subprocess.run(
        [
            str(edge),
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            f"--window-size={width},{height}",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    image = Image.open(shot)
    if image.size != (width, height):
        raise SystemExit(f"Edge rendered {name} at {image.size}, expected {(width, height)}")
    return image.convert("RGB")


def main() -> None:
    edge = next((path for path in EDGE_CANDIDATES if path.exists()), None)
    if edge is None:
        raise SystemExit("Microsoft Edge not found; it rasterises the graphics")
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        for name, width, height, html in GRAPHICS:
            render(edge, Path(tmp), name, width, height, html).save(OUT / name)
    print(f"wrote {len(GRAPHICS)} images to {OUT}")


if __name__ == "__main__":
    main()
