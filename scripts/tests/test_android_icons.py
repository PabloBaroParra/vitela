"""Tests for scripts/android-icons.py.

Run from the repository root:

    python3 -m unittest discover -s scripts/tests -p "test_android_icons.py"
"""

from __future__ import annotations

import importlib.util
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "android-icons.py"
_spec = importlib.util.spec_from_file_location("android_icons", SCRIPT)
icons = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(icons)

ANDROID = "{http://schemas.android.com/apk/res/android}"


def element(markup: str) -> ET.Element:
    return ET.fromstring(f'<svg xmlns="http://www.w3.org/2000/svg">{markup}</svg>')[0]


class ShapeToPathTest(unittest.TestCase):
    def test_a_square_rect_is_a_closed_box(self):
        self.assertEqual(icons.path_data(element('<rect x="2" y="3" width="4" height="5"/>')), "M2,3h4v5h-4Z")

    def test_a_rounded_rect_draws_one_arc_per_corner(self):
        data = icons.path_data(element('<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/>'))

        self.assertTrue(data.startswith("M5,3.5h4"))
        self.assertEqual(data.count("a1.5,1.5 0 0 1"), 4)

    def test_a_corner_radius_never_exceeds_half_the_side(self):
        data = icons.path_data(element('<rect x="0" y="0" width="4" height="10" rx="9"/>'))

        self.assertIn("a2,2 0 0 1", data)

    def test_a_circle_is_two_half_arcs(self):
        self.assertEqual(
            icons.path_data(element('<circle cx="10" cy="10" r="6.5"/>')),
            "M3.5,10a6.5,6.5 0 1 0 13,0a6.5,6.5 0 1 0 -13,0Z",
        )

    def test_paths_pass_through_untouched(self):
        self.assertEqual(icons.path_data(element('<path d="m15 15 5.5 5.5"/>')), "m15 15 5.5 5.5")

    def test_an_unsupported_element_is_refused(self):
        with self.assertRaises(ValueError):
            icons.path_data(element('<line x1="0" y1="0" x2="1" y2="1"/>'))


class ConvertTest(unittest.TestCase):
    def test_every_shared_icon_becomes_a_24dp_stroked_vector(self):
        for svg in sorted(icons.SOURCE.glob("*.svg")):
            with self.subTest(icon=svg.name):
                vector = ET.fromstring(icons.convert(svg))
                self.assertEqual(vector.get(f"{ANDROID}viewportWidth"), "24")
                paths = vector.findall("path")
                self.assertTrue(paths)
                for path in paths:
                    self.assertEqual(path.get(f"{ANDROID}strokeColor"), "#FF000000")
                    self.assertIsNone(path.get(f"{ANDROID}fillColor"))

    def test_the_committed_drawables_match_the_shared_icons(self):
        expected = icons.render_all()
        committed = {path.name: path.read_text(encoding="utf-8") for path in icons.TARGET.glob(f"{icons.PREFIX}*.xml")}

        self.assertEqual(sorted(committed), sorted(expected), "run: python3 scripts/android-icons.py")
        for name, text in expected.items():
            self.assertEqual(committed[name], text, f"{name} is stale; run: python3 scripts/android-icons.py")


if __name__ == "__main__":
    unittest.main()
