#!/usr/bin/env python3
"""Keep the README status tables from silently drifting out of reality.

Two things rot on their own if nobody watches them:

  1. The "Tools & features" table names a *crate* per tool. When a crate is
     renamed or removed, that cell becomes a lie. This script fails if any
     crate named there (and not marked "(planned)") is missing from `core/`.

  2. Both tables use a fixed legend of status symbols. A typo'd or invented
     symbol means the table no longer parses the way readers (and this script)
     expect. This script fails on any cell outside the allowed set.

What it deliberately does NOT check: that a cell marked done actually has
passing tests. No generic script can map a table cell to a test without
inventing the mapping — that stays a human/AI convention (see the README's
"Keeping this table honest" note). This guard covers the parts a machine can
verify without guessing.

Exit code 0 = tables are consistent with the repo. Non-zero = drift found,
with one line per problem.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
README = REPO_ROOT / "README.md"
CORE_DIR = REPO_ROOT / "core"

# Allowed status markers. Tools table uses the three-tier set; the platform
# table uses done / in-progress / not-yet.
TOOLS_STATUS_MARKERS = {"✅", "\U0001f6a7", "\U0001f52e"}  # ✅ 🚧 🔮
PLATFORM_CELL_MARKERS = {"✅", "\U0001f6a7", "—"}  # ✅ 🚧 —

CRATE_TOKEN = re.compile(r"`([a-z][a-z0-9-]*)`")


def split_row(line: str) -> list[str]:
    """Split a markdown table row into trimmed cells."""
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def section_rows(text: str, heading: str) -> list[list[str]]:
    """Return the data rows of the first markdown table under `heading`.

    Stops at the next heading. Skips the header row and the `---` separator.
    """
    lines = text.splitlines()
    start = next(
        (i for i, ln in enumerate(lines) if ln.strip() == heading),
        None,
    )
    if start is None:
        raise LookupError(f"heading not found: {heading!r}")

    rows: list[list[str]] = []
    header_seen = False
    for ln in lines[start + 1 :]:
        if ln.startswith("## "):
            break
        if not ln.lstrip().startswith("|"):
            continue
        cells = split_row(ln)
        # Separator row: every cell is dashes/colons.
        if all(set(c) <= {"-", ":"} and c for c in cells):
            continue
        if not header_seen:
            header_seen = True  # first pipe row is the column header
            rows.append(cells)  # keep it so callers can find columns by name
            continue
        rows.append(cells)
    return rows


def check_tools_table(text: str, problems: list[str]) -> None:
    rows = section_rows(text, "## Tools & features")
    header, data = rows[0], rows[1:]
    try:
        crate_col = header.index("Crate")
        status_col = header.index("Status")
    except ValueError:
        problems.append(
            "Tools & features: expected 'Crate' and 'Status' columns "
            f"(got {header})"
        )
        return

    for row in data:
        tool = row[0] if row else "<empty row>"
        crate_cell = row[crate_col] if len(row) > crate_col else ""
        status_cell = row[status_col] if len(row) > status_col else ""

        if not any(m in status_cell for m in TOOLS_STATUS_MARKERS):
            problems.append(
                f"Tools & features: '{tool}' has an unknown status "
                f"marker: {status_cell!r} (expected one of ✅ 🚧 🔮)"
            )

        # A crate assignment marked "(planned)" points at a crate that does
        # not exist yet — that's expected, so skip the existence check.
        if "planned" in crate_cell.lower():
            continue
        for crate in CRATE_TOKEN.findall(crate_cell):
            if not (CORE_DIR / crate).is_dir():
                problems.append(
                    f"Tools & features: '{tool}' names crate `{crate}`, "
                    f"but core/{crate}/ does not exist"
                )


def check_platform_table(text: str, problems: list[str]) -> None:
    rows = section_rows(text, "## Platform status")
    header, data = rows[0], rows[1:]
    for row in data:
        capability = row[0] if row else "<empty row>"
        for cell in row[1:]:
            if cell not in PLATFORM_CELL_MARKERS:
                problems.append(
                    f"Platform status: '{capability}' has an invalid cell "
                    f"{cell!r} (expected one of ✅ 🚧 —)"
                )


def main() -> int:
    # The messages below contain the status emoji. On a Windows console
    # (cp1252) a bare print() of those would raise UnicodeEncodeError, hiding
    # the real problem behind a traceback — force UTF-8 so the output is
    # readable everywhere CI or a developer might run this.
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(encoding="utf-8", errors="replace")

    text = README.read_text(encoding="utf-8")
    problems: list[str] = []

    for check in (check_tools_table, check_platform_table):
        try:
            check(text, problems)
        except LookupError as exc:
            problems.append(str(exc))

    if problems:
        print("README status tables are out of date:\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\nFix the table (or the code) so they agree, then re-run "
            "scripts/check_readme_tables.py.",
            file=sys.stderr,
        )
        return 1

    print("README status tables OK.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
