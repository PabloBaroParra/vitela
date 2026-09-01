#!/usr/bin/env python3
"""Validate the caller-owned PDFs emitted by the T-174 ignored Rust tests
(core/pdf-save/tests/metadata_roundtrip.rs) against an independent PDF
library — proving pypdf's own `/Info` decoder agrees with what pdf-save
wrote, not just that pdf-save's reader agrees with its own writer."""

import sys
from pathlib import Path

from pypdf import PdfReader

# "populated": `write_pypdf_validation_output_for_populated_info` starts from
# a fixture with a fully populated `/Info` (all seven text keys + a valid
# `/CreationDate`) and edits only `/Title`, to a non-Latin1 string — pinning
# batch decision 7's UTF-16BE+BOM path — so every other field here is the
# fixture's original, untouched value.
#
# "created": `write_pypdf_validation_output_for_created_info` starts from a
# fixture with no `/Info` at all and writes a brand-new one with all seven
# fields plus a valid `/CreationDate`.
EXPECTED = {
    "populated": {
        "/Title": "Título — 日本語 café",
        "/Author": "Ada Lovelace",
        "/Subject": "Reporte trimestral",
        "/Keywords": "finanzas, Q3",
        "/Creator": "pdf-editor-mvp",
        "/Producer": "pdf-editor-mvp",
        "/CreationDate": "D:20250115093000Z",
    },
    "created": {
        "/Title": "Contrato Digital",
        "/Author": "Equipo Legal",
        "/Subject": "Términos y condiciones",
        "/Keywords": "contrato, legal, digital",
        "/Creator": "pdf-editor-mvp",
        "/Producer": "pdf-editor-mvp",
        "/CreationDate": "D:20260115093000Z",
    },
}


def fail(message: str, code: int) -> int:
    print(message, file=sys.stderr)
    return code


def main(argv: list[str]) -> int:
    if len(argv) != 3 or argv[2] not in EXPECTED:
        return fail(
            f"usage: validate_metadata_roundtrip.py OUTPUT.pdf {{{'|'.join(EXPECTED)}}}", 2
        )
    path = Path(argv[1])
    if not path.is_file():
        return fail(f"input PDF does not exist: {path}", 2)

    try:
        info = PdfReader(path).metadata
    except Exception as error:
        return fail(f"cannot parse or read metadata from {path}: {error}", 1)
    if info is None:
        return fail(f"{path}: document has no /Info dictionary at all", 1)

    for key, expected in EXPECTED[argv[2]].items():
        actual = info.get(key)
        if actual != expected:
            return fail(f"{path}: {key} was {actual!r}, expected {expected!r}", 1)

    # decision 9: nothing in this batch auto-stamps `/Producer`, and neither
    # of these two saves goes through the full-rewrite writer (a
    # metadata-only edit stays incremental per decision 8) — so `/ModDate`,
    # which only the full-rewrite path's `set_mod_date` auto-stamps, must
    # stay exactly as absent as the untouched fixture started.
    if info.get("/ModDate") is not None:
        return fail(f"{path}: /ModDate was written on its own: {info.get('/ModDate')!r}", 1)

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
