#!/usr/bin/env python3
"""One-off generator for reportlab_acroform.pdf (Batch 20, T-144).

Not run by CI or by any test — this script documents how the committed
fixture was produced and lets it be regenerated if it is ever lost. Same
"external tool, generated once, versioned" criterion as
tests/fixtures/content-edit/generate_reportlab_embedded_subset.py (T-159),
which already names this task as its precedent.

Why an external tool at all: every other AcroForm this repo tests against is
built with lopdf, which is also the library `pdf-form`'s writer half uses.
That proves `pdf-form`'s reader agrees with `pdf-form`'s writer and nothing
more. It says nothing about whether the reader copes with how a genuinely
different PDF producer lays out a field tree — and producers differ a lot
here, because ISO 32000-1 leaves most of the layout free.

What reportlab does differently, and why each difference is worth having:

  - **`/Ff` flags are set, not omitted.** reportlab writes an explicit `/Ff`
    on every field. `pdf-form::read` classifies a button as radio vs
    checkbox vs pushbutton purely from bits 16/17 of that integer, and a
    choice field as dropdown vs listbox from bit 18 — so this fixture is the
    only place those bits arrive from someone else's encoder.
  - **A radio group is a parent with `/Kids`**, each kid a separate widget
    with its own `/Rect` and its own `/AP` `/N` state names. That is the
    shape `read::build_radio_group` walks, and the one this crate's own
    writer produces from the other side.
  - **Widgets are merged into their field dictionaries** for the three
    non-radio fields: one object that is both the field and its `/Widget`
    annotation, referenced from `/Annots` and `/Fields` at once. `pdf-form`
    supports that (it resolves a widget's page through `widget_pages`) but
    every lopdf-built fixture in this repo writes it that way too, so the
    cross-check is in the details: reportlab's `/DA`, its `/MK` appearance
    dictionaries and its `/AP` streams are its own.
  - **`/DA` strings come from reportlab's own serializer.** `parse_da` has
    to recover font family, size and colour out of them. A default it
    silently falls back to would hide a parser gap; the values below are
    deliberately non-default (12pt is, but Courier and the colours are not).
  - **The dropdown carries `/Opt` and a preselected `/V`**, so the reader is
    exercised on an existing value, not only on an empty field.

What it produces (page 1, US Letter, 612x792):

  | `/T`        | type            | notes                                    |
  |-------------|-----------------|------------------------------------------|
  | `full_name` | text            | value "Ada Lovelace", Helvetica 12pt     |
  | `notes`     | text, multiline | `/Ff` bit 13 set, empty                  |
  | `subscribe` | checkbox        | checked, `/AS /Yes`                      |
  | `plan`      | radio group     | 2 kids (`basic`, `pro`), `pro` selected  |
  | `country`   | dropdown        | `/Opt` of 3, value "Uruguay", Courier    |

A `listbox` is deliberately included too, and it is *not* a field this crate
models (only combo choice fields are — `/Ff` bit 18). T-137's read resilience
says an unmodeled field is left intact in the file and simply absent from the
editable set; a fixture containing only readable fields could never catch a
regression that quietly started modelling one.

Regenerate with:
    pip install reportlab
    python tests/fixtures/forms/generate_reportlab_acroform.py
"""

import os

from reportlab.lib.colors import Color
from reportlab.pdfgen import canvas

OUTPUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "reportlab_acroform.pdf")

BLACK = Color(0, 0, 0)
WHITE = Color(1, 1, 1)
BORDER = Color(0.2, 0.2, 0.2)


def main() -> None:
    pdf = canvas.Canvas(OUTPUT, pagesize=(612, 792))
    pdf.setTitle("Vitela AcroForm interop fixture")
    pdf.setFont("Helvetica", 14)
    pdf.drawString(72, 740, "Vitela AcroForm interop fixture")

    form = pdf.acroForm

    form.textfield(
        name="full_name",
        value="Ada Lovelace",
        x=72,
        y=680,
        width=240,
        height=22,
        fontName="Helvetica",
        fontSize=12,
        textColor=BLACK,
        fillColor=WHITE,
        borderColor=BORDER,
    )

    form.textfield(
        name="notes",
        value="",
        x=72,
        y=580,
        width=240,
        height=72,
        fontName="Helvetica",
        fontSize=12,
        textColor=BLACK,
        fillColor=WHITE,
        borderColor=BORDER,
        fieldFlags="multiline",
    )

    form.checkbox(
        name="subscribe",
        checked=True,
        x=72,
        y=540,
        size=18,
        buttonStyle="check",
        fillColor=WHITE,
        borderColor=BORDER,
    )

    form.radio(
        name="plan",
        value="basic",
        selected=False,
        x=72,
        y=500,
        size=18,
        buttonStyle="circle",
        fillColor=WHITE,
        borderColor=BORDER,
    )
    form.radio(
        name="plan",
        value="pro",
        selected=True,
        x=132,
        y=500,
        size=18,
        buttonStyle="circle",
        fillColor=WHITE,
        borderColor=BORDER,
    )

    form.choice(
        name="country",
        value="Uruguay",
        options=["Argentina", "Uruguay", "Chile"],
        x=72,
        y=450,
        width=240,
        height=22,
        fontName="Courier",
        fontSize=12,
        textColor=BLACK,
        fillColor=WHITE,
        borderColor=BORDER,
    )

    # Neither of these is a field `pdf-form` models. They are here so the
    # read path is exercised on a document that contains fields it must
    # skip without disturbing the ones around them.
    form.listbox(
        name="languages",
        value=["es"],
        options=["es", "en", "pt"],
        x=360,
        y=450,
        width=160,
        height=60,
        fontName="Helvetica",
        fontSize=12,
        textColor=BLACK,
        fillColor=WHITE,
        borderColor=BORDER,
    )

    pdf.save()
    print(f"wrote {OUTPUT}")


if __name__ == "__main__":
    main()
