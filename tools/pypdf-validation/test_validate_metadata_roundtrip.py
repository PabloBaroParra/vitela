import tempfile
import unittest
import importlib.util
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "validate_metadata_roundtrip", Path(__file__).with_name("validate_metadata_roundtrip.py")
)
assert _SPEC and _SPEC.loader
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)
main = _MODULE.main
EXPECTED = _MODULE.EXPECTED


def _minimal_pdf_with_info(info_entries: dict[str, bytes]) -> bytes:
    """A minimal, hand-assembled one-page PDF whose `/Info` dict carries
    `info_entries` verbatim (each value already a PDF literal-string
    payload, e.g. `b"(Ada Lovelace)"` or a `<FEFF...>` hex string) — just
    enough structure for `pypdf.PdfReader.metadata` to read it back, without
    pulling in a PDF-authoring dependency the validator itself doesn't need.
    """
    catalog_num, pages_num, page_num, info_num = 1, 2, 3, 4
    parts: dict[int, bytes] = {
        catalog_num: f"<< /Type /Catalog /Pages {pages_num} 0 R >>".encode(),
        pages_num: f"<< /Type /Pages /Kids [{page_num} 0 R] /Count 1 >>".encode(),
        page_num: (
            f"<< /Type /Page /Parent {pages_num} 0 R /MediaBox [0 0 200 200] >>"
        ).encode(),
    }
    body = b" ".join(f"/{key}".encode() + b" " + value for key, value in info_entries.items())
    parts[info_num] = b"<< " + body + b" >>"

    total_objects = max(parts)
    buf = bytearray(b"%PDF-1.4\n")
    offsets = [0] * (total_objects + 1)
    for number in range(1, total_objects + 1):
        offsets[number] = len(buf)
        buf += f"{number} 0 obj\n".encode() + parts[number] + b"\nendobj\n"
    xref_offset = len(buf)
    buf += f"xref\n0 {total_objects + 1}\n".encode()
    buf += b"0000000000 65535 f \n"
    for number in range(1, total_objects + 1):
        buf += f"{offsets[number]:010d} 00000 n \n".encode()
    buf += (
        f"trailer\n<< /Size {total_objects + 1} /Root {catalog_num} 0 R "
        f"/Info {info_num} 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    ).encode()
    return bytes(buf)


def _pdf_string_literal(value: str) -> bytes:
    """Encodes `value` the way a real writer would choose between a plain
    literal string and a UTF-16BE+BOM hex string (PDF 32000-2 §7.9.2.2) —
    the same choice `pdf-save`'s own `encode_pdf_text_string` makes (batch
    decision 7), needed here because `Título — 日本語 café` written as a
    literal `(...)` would just be raw UTF-8 bytes, which no PDF reader
    interprets as UTF-8."""
    if all(0x20 <= ord(char) <= 0x7E for char in value):
        escaped = value.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
        return f"({escaped})".encode("ascii")
    utf16be_with_bom = b"\xfe\xff" + value.encode("utf-16-be")
    return f"<{utf16be_with_bom.hex()}>".encode("ascii")


def _pdf_from_expected(mode: str, extra: dict[str, bytes] | None = None) -> bytes:
    entries = {
        key.lstrip("/"): _pdf_string_literal(value) for key, value in EXPECTED[mode].items()
    }
    if extra:
        entries.update(extra)
    return _minimal_pdf_with_info(entries)


class ValidatorCliTests(unittest.TestCase):
    def test_rejects_missing_arguments_with_usage_exit_code(self) -> None:
        self.assertEqual(main(["validate_metadata_roundtrip.py"]), 2)

    def test_rejects_an_unknown_mode_with_usage_exit_code(self) -> None:
        self.assertEqual(
            main(["validate_metadata_roundtrip.py", "whatever.pdf", "bogus-mode"]), 2
        )

    def test_rejects_missing_file_without_interpreting_shell_metacharacters(self) -> None:
        self.assertEqual(
            main(["validate_metadata_roundtrip.py", "missing;not-executed.pdf", "populated"]), 2
        )

    def test_rejects_a_non_pdf_existing_file_as_a_parse_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "not-a-pdf.pdf"
            path.write_text("not a PDF", encoding="utf-8")

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "populated"]), 1)

    def test_accepts_the_populated_scenario_s_own_expected_values(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "populated.pdf"
            path.write_bytes(_pdf_from_expected("populated"))

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "populated"]), 0)

    def test_accepts_the_created_scenario_s_own_expected_values(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "created.pdf"
            path.write_bytes(_pdf_from_expected("created"))

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "created"]), 0)

    def test_rejects_a_mismatched_field(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "wrong-title.pdf"
            entries = {
                key.lstrip("/"): f"({value})".encode()
                for key, value in EXPECTED["created"].items()
            }
            entries["Title"] = b"(Not The Right Title)"
            path.write_bytes(_minimal_pdf_with_info(entries))

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "created"]), 1)

    def test_rejects_a_document_with_no_info_dictionary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "no-info.pdf"
            buf = (
                b"%PDF-1.4\n"
                b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
                b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n"
                b"trailer\n<< /Size 3 /Root 1 0 R >>\n%%EOF"
            )
            path.write_bytes(buf)

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "populated"]), 1)

    def test_rejects_an_unexpected_mod_date_auto_stamp(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "stamped.pdf"
            path.write_bytes(
                _pdf_from_expected("created", extra={"ModDate": b"(D:20260101000000Z)"})
            )

            self.assertEqual(main(["validate_metadata_roundtrip.py", str(path), "created"]), 1)


if __name__ == "__main__":
    unittest.main()
