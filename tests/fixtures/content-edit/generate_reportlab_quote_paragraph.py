#!/usr/bin/env python3
"""One-off generator for reportlab_quote_paragraph.pdf (paragraph text blocks).

Not run by CI or by any test — this script documents how the committed
fixture was produced and lets it be regenerated, following the same
"external tool, generated once, versioned" criterion as
generate_reportlab_embedded_subset.py (read that script's docstring for why
reportlab and why Bitstream Vera).

What this produces: one page shaped like a joinery quote.

- A heading in Vera 14 ("PRESUPUESTO 3.725/1"), far enough above the body
  that it must stay a block of its own.
- A four-line specification paragraph in Vera 9 with an 11 pt leading, laid
  out by one text object with `T*` line breaks, painted in a non-black fill
  colour with a non-zero character spacing (`Tc`) — the look an inserted
  line has to keep when a retype grows the paragraph.
- A small table in the same font and size: a header row and two item rows,
  three left-aligned columns, rows 14 pt apart. Its cells stack exactly like
  short paragraph lines would, which is what the detector must not join.

The text is ASCII only on purpose. reportlab subsets the TrueType font and
maps non-ASCII characters to codes only its /ToUnicode CMap explains, which
pdf-edit does not read yet, so an accented "COMPOSICIÓN" would decode as
U+FFFD and could not be retyped — a separate gap, not what this fixture is
for.

Regenerate with:
    pip install reportlab
    python tests/fixtures/content-edit/generate_reportlab_quote_paragraph.py
"""

import os

from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

OUTPUT = os.path.join(os.path.dirname(__file__), "reportlab_quote_paragraph.pdf")

HEADING = "PRESUPUESTO 3.725/1"

SPEC_LINES = [
    "COMPOSICION: Aluminio lacado con rotura de puente termico",
    "COLOR: Blanco RAL 9016 mate",
    "APERTURA: Corredera de dos hojas",
    "VIDRIO: Doble 4/16/4 bajo emisivo",
]

TABLE_COLUMNS = [72, 160, 400]
TABLE_ROWS = [
    ["Cantidad", "Descripcion", "Precio"],
    ["1", "Ventana corredera", "450,00"],
    ["2", "Puerta abatible", "890,00"],
]


def main() -> None:
    import reportlab

    vera_path = os.path.join(os.path.dirname(reportlab.__file__), "fonts", "Vera.ttf")
    pdfmetrics.registerFont(TTFont("Vera", vera_path))

    # pageCompression=0 for the same reason as the embedded-subset fixture:
    # reportlab's default ASCII85 + Flate chain is a filter chain pdf-edit
    # refuses to rewrite.
    doc = canvas.Canvas(OUTPUT, pagesize=(612, 792), pageCompression=0)
    # A fixed creation date and no random id keep regeneration stable.
    doc.setCreator("generate_reportlab_quote_paragraph.py")

    doc.setFont("Vera", 14)
    doc.drawString(72, 720, HEADING)

    spec = doc.beginText(72, 660)
    spec.setFont("Vera", 9, leading=11)
    spec.setFillColorRGB(0.1, 0.2, 0.55)
    spec.setCharSpace(0.3)
    for line in SPEC_LINES:
        spec.textLine(line)
    doc.drawText(spec)

    doc.setFillColorRGB(0, 0, 0)
    doc.setFont("Vera", 9)
    baseline = 580
    for row in TABLE_ROWS:
        for x, cell in zip(TABLE_COLUMNS, row):
            doc.drawString(x, baseline, cell)
        baseline -= 14

    doc.save()
    print(f"wrote {OUTPUT}")


if __name__ == "__main__":
    main()
