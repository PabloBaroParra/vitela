#!/usr/bin/env python3
"""Convert the shared shell icons in assets/icons into Android vector drawables.

Every icon is one stroked <g> holding <path>, <rect> and <circle> children
(see assets/README.md). Android's VectorDrawable only draws paths, so rects
and circles become path data here. The stroke is written black and the app
tints it at the call site, exactly as the Windows and GTK shells do.

Run it after adding or changing an icon; the output is committed so a build
never needs Python:

    python scripts/android-icons.py
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE = REPO_ROOT / "assets" / "icons"
TARGET = REPO_ROOT / "apps" / "android" / "app" / "src" / "main" / "res" / "drawable"
SVG = "{http://www.w3.org/2000/svg}"
PREFIX = "ic_shell_"


def number(value: float) -> str:
    return f"{value:g}"


def rect_path(node: ET.Element) -> str:
    x, y = float(node.get("x", 0)), float(node.get("y", 0))
    w, h = float(node.get("width")), float(node.get("height"))
    r = min(float(node.get("rx", 0)), w / 2, h / 2)
    if r == 0:
        return f"M{number(x)},{number(y)}h{number(w)}v{number(h)}h{number(-w)}Z"
    arc = f"a{number(r)},{number(r)} 0 0 1"
    return (
        f"M{number(x + r)},{number(y)}h{number(w - 2 * r)}{arc} {number(r)},{number(r)}"
        f"v{number(h - 2 * r)}{arc} {number(-r)},{number(r)}"
        f"h{number(-(w - 2 * r))}{arc} {number(-r)},{number(-r)}"
        f"v{number(-(h - 2 * r))}{arc} {number(r)},{number(-r)}Z"
    )


def circle_path(node: ET.Element) -> str:
    cx, cy, r = float(node.get("cx")), float(node.get("cy")), float(node.get("r"))
    return (
        f"M{number(cx - r)},{number(cy)}a{number(r)},{number(r)} 0 1 0 {number(2 * r)},0"
        f"a{number(r)},{number(r)} 0 1 0 {number(-2 * r)},0Z"
    )


def path_data(node: ET.Element) -> str:
    tag = node.tag.removeprefix(SVG)
    if tag == "path":
        return node.get("d")
    if tag == "rect":
        return rect_path(node)
    if tag == "circle":
        return circle_path(node)
    raise ValueError(f"unsupported element <{tag}>")


def convert(svg: Path) -> str:
    root = ET.parse(svg).getroot()
    group = root.find(f"{SVG}g")
    if group is None:
        raise ValueError(f"{svg.name}: expected one stroked <g>")
    width = group.get("stroke-width", "2")
    cap = group.get("stroke-linecap", "round")
    join = group.get("stroke-linejoin", "round")
    paths = []
    for child in group:
        paths.append(
            "    <path\n"
            f'        android:pathData="{path_data(child)}"\n'
            '        android:strokeColor="#FF000000"\n'
            f'        android:strokeWidth="{width}"\n'
            f'        android:strokeLineCap="{cap}"\n'
            f'        android:strokeLineJoin="{join}" />'
        )
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f"<!-- Generated from assets/icons/{svg.name} by scripts/android-icons.py. Do not edit. -->\n"
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
        '    android:width="24dp"\n'
        '    android:height="24dp"\n'
        '    android:viewportWidth="24"\n'
        '    android:viewportHeight="24">\n'
        + "\n".join(paths)
        + "\n</vector>\n"
    )


def main() -> int:
    for stale in TARGET.glob(f"{PREFIX}*.xml"):
        stale.unlink()
    icons = sorted(SOURCE.glob("*.svg"))
    for svg in icons:
        name = PREFIX + svg.stem.replace("-", "_")
        (TARGET / f"{name}.xml").write_text(convert(svg), encoding="utf-8", newline="\n")
    print(f"wrote {len(icons)} drawables to {TARGET.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
