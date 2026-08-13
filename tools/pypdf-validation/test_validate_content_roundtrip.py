import tempfile
import unittest
import importlib.util
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "validate_content_roundtrip", Path(__file__).with_name("validate_content_roundtrip.py")
)
assert _SPEC and _SPEC.loader
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)
main = _MODULE.main


def _minimal_pdf(pages_text: list[str]) -> bytes:
    """Builds a minimal, hand-assembled multi-page PDF whose pages paint
    `pages_text` with the standard Helvetica font — just enough structure
    for `pypdf.PdfReader.extract_text` to recover the literal strings,
    without pulling in a PDF-authoring dependency the validator itself
    doesn't need."""
    catalog_num = 1
    pages_num = 2
    font_num = 3
    page_nums = [font_num + 1 + 2 * index for index in range(len(pages_text))]
    content_nums = [number + 1 for number in page_nums]

    parts: dict[int, bytes] = {
        catalog_num: f"<< /Type /Catalog /Pages {pages_num} 0 R >>".encode(),
        pages_num: (
            f"<< /Type /Pages /Kids [{' '.join(f'{n} 0 R' for n in page_nums)}] "
            f"/Count {len(pages_text)} >>"
        ).encode(),
        font_num: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    }
    for text, page_num, content_num in zip(pages_text, page_nums, content_nums):
        parts[page_num] = (
            f"<< /Type /Page /Parent {pages_num} 0 R /MediaBox [0 0 200 200] "
            f"/Resources << /Font << /F1 {font_num} 0 R >> >> "
            f"/Contents {content_num} 0 R >>"
        ).encode()
        stream_body = f"BT /F1 12 Tf 20 100 Td ({text}) Tj ET".encode()
        parts[content_num] = (
            f"<< /Length {len(stream_body)} >>\nstream\n".encode() + stream_body + b"\nendstream"
        )

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
        f"trailer\n<< /Size {total_objects + 1} /Root {catalog_num} 0 R >>\n"
        f"startxref\n{xref_offset}\n%%EOF"
    ).encode()
    return bytes(buf)


class ValidatorCliTests(unittest.TestCase):
    def test_rejects_missing_argument_with_usage_exit_code(self) -> None:
        self.assertEqual(main(["validate_content_roundtrip.py"]), 2)

    def test_rejects_missing_file_without_interpreting_shell_metacharacters(self) -> None:
        self.assertEqual(main(["validate_content_roundtrip.py", "missing;not-executed.pdf"]), 2)

    def test_rejects_a_non_pdf_existing_file_as_a_parse_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "not-a-pdf.pdf"
            path.write_text("not a PDF", encoding="utf-8")

            self.assertEqual(main(["validate_content_roundtrip.py", str(path)]), 1)

    def test_accepts_a_valid_roundtrip_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "valid-output.pdf"
            path.write_bytes(_minimal_pdf(["edited page 0", "roundtrip page 1"]))

            self.assertEqual(main(["validate_content_roundtrip.py", str(path)]), 0)

    def test_rejects_the_original_unedited_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "original-fixture.pdf"
            path.write_bytes(_minimal_pdf(["roundtrip page 0", "roundtrip page 1"]))

            self.assertEqual(main(["validate_content_roundtrip.py", str(path)]), 1)

    def test_rejects_output_missing_the_control_page_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "missing-control.pdf"
            path.write_bytes(_minimal_pdf(["edited page 0", "some other page"]))

            self.assertEqual(main(["validate_content_roundtrip.py", str(path)]), 1)


if __name__ == "__main__":
    unittest.main()
