#!/usr/bin/env python3
"""Validate the caller-owned PDF emitted by the T-160 ignored Rust test."""

import sys
from pathlib import Path

from pypdf import PdfReader


def fail(message: str, code: int) -> int:
    print(message, file=sys.stderr)
    return code


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        return fail("usage: validate_content_roundtrip.py OUTPUT.pdf", 2)
    path = Path(argv[1])
    if not path.is_file():
        return fail(f"input PDF does not exist: {path}", 2)
    try:
        text = "\n".join(page.extract_text() or "" for page in PdfReader(path).pages)
    except Exception as error:
        return fail(f"cannot parse or extract {path}: {error}", 1)
    for expected in ("edited page 0", "roundtrip page 1"):
        if expected not in text:
            return fail(f"{path}: missing expected text {expected!r}", 1)
    if "roundtrip page 0" in text:
        return fail(f"{path}: found unexpected original text 'roundtrip page 0'", 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
