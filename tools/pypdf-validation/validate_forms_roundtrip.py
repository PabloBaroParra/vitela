#!/usr/bin/env python3
"""Validate the caller-owned PDFs emitted by the T-145 ignored Rust tests
(core/pdf-save/tests/forms_roundtrip.rs) against an independent PDF library
— proving pypdf's own AcroForm decoder agrees with what pdf-save wrote, not
just that pdf-form's reader agrees with pdf-form's writer.

Two modes, one per emitted file:

  authored — a form this workspace created from nothing, one field of each
      of the four supported kinds, every value set, saved through the
      full-rewrite writer.

  filled — tests/fixtures/forms/reportlab_acroform.pdf (T-144's committed
      foreign fixture) with two of its fields filled in and saved
      incrementally. The point is not only that the two changed, but that
      the four that did not are still there and still say what reportlab
      said — including `languages`, a listbox `pdf-form` deliberately does
      not model. "Lo no modelado se preserva intacto" is a contract about
      the file, so only a reader that is not us can confirm it.

`/Ff` is checked as well as `/V`, because the flags are what carry the
*kind* across: multiline is bit 13 (4096), radio is bit 16 (32768) and combo
is bit 18 (131072). A writer that dropped them would still round-trip
through our own reader, which has the model in hand either way.
"""

import sys
from pathlib import Path

from pypdf import PdfReader

MULTILINE = 1 << 12
RADIO = 1 << 15
COMBO = 1 << 17

# name -> (/FT, /V, required /Ff bits or None, /Opt or None)
EXPECTED = {
    "authored": {
        "applicant": ("/Tx", "Ada Lovelace", MULTILINE, None),
        "agrees": ("/Btn", "/Yes", None, None),
        "plan": ("/Btn", "/pro", RADIO, None),
        "country": ("/Ch", "Uruguay", COMBO, ["Argentina", "Uruguay", "Chile"]),
    },
    "filled": {
        # Changed by the save under test.
        "notes": ("/Tx", "Filled by Vitela", MULTILINE, None),
        "country": ("/Ch", "Chile", COMBO, ["Argentina", "Uruguay", "Chile"]),
        # Untouched, and reportlab's own values — an incremental save that
        # rewrote more of the AcroForm than it had to would show up here.
        "full_name": ("/Tx", "Ada Lovelace", None, None),
        "subscribe": ("/Btn", "/Yes", None, None),
        "plan": ("/Btn", "/pro", RADIO, None),
        # Never modeled by pdf-form, and still intact.
        "languages": ("/Ch", "es", None, ["es", "en", "pt"]),
    },
}


def fail(message: str, code: int) -> int:
    print(message, file=sys.stderr)
    return code


def main(argv: list[str]) -> int:
    if len(argv) != 3 or argv[2] not in EXPECTED:
        return fail(
            f"usage: validate_forms_roundtrip.py OUTPUT.pdf {{{'|'.join(EXPECTED)}}}", 2
        )
    path = Path(argv[1])
    mode = argv[2]
    if not path.is_file():
        return fail(f"input PDF does not exist: {path}", 2)

    try:
        fields = PdfReader(path).get_fields()
    except Exception as error:
        return fail(f"cannot parse or read form fields from {path}: {error}", 1)
    if not fields:
        return fail(f"{path}: document exposes no AcroForm fields at all", 1)

    expected = EXPECTED[mode]
    missing = sorted(set(expected) - set(fields))
    if missing:
        return fail(f"{path}: missing form fields {missing}", 1)
    unexpected = sorted(set(fields) - set(expected))
    if unexpected:
        return fail(f"{path}: unexpected extra form fields {unexpected}", 1)

    for name, (field_type, value, flag, options) in expected.items():
        field = fields[name]
        actual_type = field.get("/FT")
        if actual_type != field_type:
            return fail(f"{path}: {name} is {actual_type!r}, expected {field_type!r}", 1)

        actual_value = field.get("/V")
        # pypdf hands back a NameObject for button values and a TextString
        # for the rest; str() flattens both to what the file says.
        if actual_value is None or str(actual_value) != value:
            return fail(f"{path}: {name} = {actual_value!r}, expected {value!r}", 1)

        if flag is not None:
            actual_flags = int(field.get("/Ff") or 0)
            if actual_flags & flag != flag:
                return fail(
                    f"{path}: {name} has /Ff {actual_flags}, missing bit mask {flag}", 1
                )

        if options is not None:
            actual_options = [str(option) for option in field.get("/Opt") or []]
            if actual_options != options:
                return fail(
                    f"{path}: {name} /Opt was {actual_options!r}, expected {options!r}", 1
                )

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
