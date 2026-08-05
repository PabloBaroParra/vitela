#!/usr/bin/env python3
"""Report maintainability review prompts without blocking a change.

This intentionally lightweight check uses only the Python standard library. It
scans supported Rust, Kotlin, Swift, and C# files, excluding the root-relative
patterns in .maintainabilityignore. Findings are GitHub Actions warnings and do
not change the process exit code; an unreadable configuration is an error.
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = REPO_ROOT / ".maintainabilityignore"
SUPPORTED_SUFFIXES = {".cs", ".kt", ".rs", ".swift"}
SIZE_LIMIT = 350
DECISION_LIMIT = 40
DUPLICATE_BLOCK_LINES = 8
DECISION_TOKENS = re.compile(
    r"\b(?:if|else\s+if|for|while|case|catch|when)\b|&&|\|\|"
)
STRING_OR_NUMBER = re.compile(r'"(?:\\.|[^"\\])*"|\b\d+(?:\.\d+)?\b')


def load_patterns(config: Path) -> list[str]:
    """Read root-relative glob patterns, rejecting malformed configuration."""
    if not config.is_file():
        raise ValueError(f"exclusion configuration not found: {config}")

    patterns: list[str] = []
    for number, raw_line in enumerate(config.read_text(encoding="utf-8").splitlines(), 1):
        pattern = raw_line.strip()
        if not pattern or pattern.startswith("#"):
            continue
        if pattern.startswith("/") or "\\" in pattern:
            raise ValueError(
                f"{config}:{number}: use a relative POSIX glob, not {pattern!r}"
            )
        patterns.append(pattern)
    return patterns


def is_excluded(relative_path: str, patterns: list[str]) -> bool:
    """Match paths against the documented root-relative glob convention."""
    return any(fnmatch.fnmatchcase(relative_path, pattern) for pattern in patterns)


def source_files(patterns: list[str]) -> list[Path]:
    """Return tracked or non-ignored source files in a stable order."""
    try:
        output = subprocess.check_output(
            [
                "git",
                "-C",
                str(REPO_ROOT),
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise ValueError(f"could not list repository files with git: {error}") from error

    files = []
    for relative_path in output.decode("utf-8").split("\0"):
        if not relative_path:
            continue
        path = REPO_ROOT / relative_path
        if not path.is_file() or path.suffix not in SUPPORTED_SUFFIXES:
            continue
        if not is_excluded(relative_path, patterns):
            files.append(path)
    return sorted(files)


def code_line(line: str) -> str:
    """Remove common line comments for intentionally approximate heuristics."""
    return line.split("//", 1)[0].strip()


def warn(path: Path, line: int, message: str) -> None:
    """Emit a GitHub annotation while remaining readable outside Actions."""
    relative_path = path.relative_to(REPO_ROOT).as_posix()
    print(f"::warning file={relative_path},line={line}::{message}")


def check_size_and_decisions(files: list[Path]) -> int:
    """Report large files and decision-dense files as review prompts."""
    findings = 0
    for path in files:
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) > SIZE_LIMIT:
            warn(
                path,
                1,
                f"{len(lines)} lines exceeds the {SIZE_LIMIT}-line review signal; "
                "split by responsibility or justify cohesion in the PR.",
            )
            findings += 1

        decisions = sum(
            len(DECISION_TOKENS.findall(code_line(line))) for line in lines
        )
        if decisions > DECISION_LIMIT:
            warn(
                path,
                1,
                f"{decisions} decision points exceeds the {DECISION_LIMIT}-point "
                "review signal; simplify or document why the control flow is cohesive.",
            )
            findings += 1
    return findings


def normalized_line(line: str) -> str:
    """Normalize safe syntactic noise without claiming semantic equivalence."""
    return re.sub(r"\s+", "", STRING_OR_NUMBER.sub("<literal>", code_line(line))).lower()


def duplicate_blocks(files: list[Path]) -> dict[tuple[str, ...], list[tuple[Path, int]]]:
    """Find exact normalized blocks across the supported languages."""
    occurrences: dict[tuple[str, ...], list[tuple[Path, int]]] = defaultdict(list)
    for path in files:
        normalized = [
            (line_number, normalized_line(line))
            for line_number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1
            )
        ]
        normalized = [(number, line) for number, line in normalized if line]
        for start in range(0, len(normalized) - DUPLICATE_BLOCK_LINES + 1, DUPLICATE_BLOCK_LINES):
            window = normalized[start : start + DUPLICATE_BLOCK_LINES]
            block = tuple(line for _, line in window)
            occurrences[block].append((path, window[0][0]))
    return {block: locations for block, locations in occurrences.items() if len(locations) > 1}


def check_duplication(files: list[Path]) -> int:
    """Report one actionable warning per repeated block, capped for readability."""
    findings = 0
    for locations in list(duplicate_blocks(files).values())[:20]:
        path, line = locations[0]
        references = ", ".join(
            f"{other.relative_to(REPO_ROOT).as_posix()}:{other_line}"
            for other, other_line in locations[1:4]
        )
        warn(
            path,
            line,
            "repeated normalized 8-line block also appears at "
            f"{references}; extract shared behaviour or justify a platform adapter.",
        )
        findings += 1
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--skip-duplication", action="store_true")
    args = parser.parse_args()

    try:
        patterns = load_patterns(args.config)
        files = source_files(patterns)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"maintainability configuration error: {error}", file=sys.stderr)
        return 2

    findings = check_size_and_decisions(files)
    if not args.skip_duplication:
        findings += check_duplication(files)
    print(
        f"Maintainability advisory complete: {findings} warning(s) across "
        f"{len(files)} maintained source file(s)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
