"""Regenerates the MSIX visual assets of the Windows shell from the brand mark.

The Store package needs a fixed set of PNG tiles (Package.appxmanifest names
them). They are derived from assets/brand/vitela-app-mark.svg rather than drawn
by hand, so a brand change is one command away:

    python scripts/windows-store-images.py

Requirements: Pillow, and Microsoft Edge (preinstalled on Windows 10/11). Edge
rasterises the SVG because it is the one SVG renderer every Windows machine
already has; Pillow only scales and pads the result.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parent.parent
MARK = REPO / "assets" / "brand" / "vitela-app-mark.svg"
# Light-on-dark variant, for the taskbar icon under the dark theme.
MARK_DARK = REPO / "assets" / "brand" / "vitela-app-mark-dark.svg"
OUT = REPO / "apps" / "windows" / "Pdf.Windows" / "Package" / "Images"
EDGE_CANDIDATES = [
    Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft/Edge/Application/msedge.exe",
    Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "Microsoft/Edge/Application/msedge.exe",
]
MASTER = 1024

# (base name, width, height, share of the shorter side the mark occupies)
TILES = [
    ("Square44x44Logo", 44, 44, 0.84),
    ("Square150x150Logo", 150, 150, 0.60),
    ("Wide310x150Logo", 310, 150, 0.60),
    ("StoreLogo", 50, 50, 0.84),
    ("SplashScreen", 620, 300, 0.60),
]
SCALES = [100, 200]
# Taskbar / Start list icons. Windows draws the unplated forms there, picking
# altform-unplated under the dark theme and altform-lightunplated under the
# light one; the navy mark vanishes on a dark taskbar, so the two differ.
TARGET_SIZES = [16, 24, 32, 48, 256]


def render_master(edge: Path, work: Path, svg: Path) -> Image.Image:
    page = work / f"{svg.stem}.html"
    page.write_text(
        "<!doctype html><html><body style='margin:0;background:transparent'>"
        f"<img src='{svg.as_uri()}' style='width:{MASTER}px;height:{MASTER}px;display:block'>"
        "</body></html>",
        encoding="utf-8",
    )
    shot = work / f"{svg.stem}.png"
    subprocess.run(
        [
            str(edge),
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--default-background-color=00000000",
            f"--window-size={MASTER},{MASTER}",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    image = Image.open(shot).convert("RGBA")
    bbox = image.getbbox()
    if bbox is None:
        raise SystemExit("Edge produced an empty render of the brand mark")
    return image.crop(bbox)


def compose(mark: Image.Image, width: int, height: int, share: float) -> Image.Image:
    side = round(min(width, height) * share)
    scale = side / max(mark.size)
    fitted = mark.resize((max(1, round(mark.width * scale)), max(1, round(mark.height * scale))), Image.LANCZOS)
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    canvas.paste(fitted, ((width - fitted.width) // 2, (height - fitted.height) // 2), fitted)
    return canvas


def main() -> None:
    edge = next((path for path in EDGE_CANDIDATES if path.exists()), None)
    if edge is None:
        raise SystemExit("Microsoft Edge not found; it rasterises the SVG")
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        mark = render_master(edge, Path(tmp), MARK)
        mark_dark = render_master(edge, Path(tmp), MARK_DARK)

    for name, width, height, share in TILES:
        for scale in SCALES:
            image = compose(mark, width * scale // 100, height * scale // 100, share)
            image.save(OUT / f"{name}.scale-{scale}.png")
    for size in TARGET_SIZES:
        image = compose(mark, size, size, 1.0)
        image.save(OUT / f"Square44x44Logo.targetsize-{size}.png")
        image.save(OUT / f"Square44x44Logo.targetsize-{size}_altform-lightunplated.png")
        compose(mark_dark, size, size, 1.0).save(OUT / f"Square44x44Logo.targetsize-{size}_altform-unplated.png")
    print(f"wrote {len(list(OUT.glob('*.png')))} images to {OUT}")


if __name__ == "__main__":
    main()
