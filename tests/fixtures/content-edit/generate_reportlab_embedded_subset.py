#!/usr/bin/env python3
"""One-off generator for reportlab_embedded_subset.pdf (Batch 21, T-159).

Not run by CI or by any test — this script documents how the committed
fixture was produced and lets it be regenerated if it's ever lost, per the
same "external tool, generated once, versioned" criterion T-144 (Batch 20)
uses for its own AcroForm fixture (see docs/batch-content-edit.md T-159).

Why an external tool at all: every other fixture in this repo is built by
`gen-fixtures` (lopdf), i.e. the same library `pdf-edit`'s writer half also
uses. That proves nothing about whether `pdf-edit`'s *parser* — the
`/Encoding`/`/FontDescriptor` resolution in particular — agrees with how a
genuinely different PDF library serializes an embedded, subsetted font.
reportlab is that different library.

Why reportlab + Bitstream Vera specifically: reportlab bundles Vera.ttf under
its own `reportlab/fonts/` directory, licensed under the Bitstream Vera Fonts
license (see reportlab/fonts/bitstream-vera-license.txt in any reportlab
install) — which explicitly permits embedding the font in a document and
redistributing that document, which is exactly what committing this PDF does.

What this produces, and why it's useful for T-160:
    reportlab embeds Vera.ttf as a `/Subtype /TrueType` simple font (not
    Type0/CID) with a subset tag in `/BaseFont` (e.g. `AAAAAA+...`) and,
    for the 7-bit ASCII text drawn below, no `/Encoding` entry at all —
    `pdf-edit` falls back to its default (StandardEncoding-shaped) table in
    that case (see core/pdf-edit/src/encoding/mod.rs::base_table). That
    table covers ASCII 0x20-0x7E but nothing above it, so:
      - `run.font_kind == FontKind::EmbeddedSimple` (subset tag in
        `/BaseFont`, `/Subtype /TrueType`, not `/Type0`).
      - Replacing the run's text with plain ASCII succeeds.
      - Replacing it with anything containing a non-ASCII character (e.g.
        'é' or a CJK character) fails with `EditError::EncodingGap` —
        Batch 21 decision 3's own example ("un mismo run puede aceptar
        'café' y rechazar '日本語'"), reproduced here against a real
        embedded font instead of a synthetic lopdf dictionary.

Regenerate with:
    pip install reportlab
    python tests/fixtures/content-edit/generate_reportlab_embedded_subset.py
"""

import os

from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

OUTPUT = os.path.join(os.path.dirname(__file__), "reportlab_embedded_subset.pdf")
FIXTURE_TEXT = "Fixture Text"


def main() -> None:
    import reportlab

    vera_path = os.path.join(os.path.dirname(reportlab.__file__), "fonts", "Vera.ttf")
    pdfmetrics.registerFont(TTFont("Vera", vera_path))

    # pageCompression=0: reportlab's default page-content encoding chains
    # ASCII85Decode + FlateDecode, which pdf-edit's content-stream codec
    # deliberately refuses to touch (a filter *chain* is unprovable to
    # round-trip — see core/pdf-edit/src/parse/filter.rs module docs). That
    # refusal is correct and stays covered by pdf-edit's own unit tests;
    # it just isn't what this fixture is for, so turn the chain off and
    # keep the content stream as plain, uncompressed bytes.
    doc = canvas.Canvas(OUTPUT, pagesize=(612, 792), pageCompression=0)
    doc.setFont("Vera", 24)
    doc.drawString(100, 700, FIXTURE_TEXT)
    doc.save()

    print(f"wrote {OUTPUT}")


if __name__ == "__main__":
    main()
