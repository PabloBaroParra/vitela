import importlib.util
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "validate_forms_roundtrip", Path(__file__).with_name("validate_forms_roundtrip.py")
)
assert _SPEC and _SPEC.loader
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)
main = _MODULE.main
COMBO = _MODULE.COMBO
MULTILINE = _MODULE.MULTILINE
RADIO = _MODULE.RADIO


def _field_object(name: str, spec: dict) -> bytes:
    """One merged field+widget dictionary, the shape both emitted files use."""
    entries = [
        b"/Type /Annot",
        b"/Subtype /Widget",
        f"/FT {spec['ft']}".encode(),
        f"/T ({name})".encode(),
        b"/Rect [0 0 100 20]",
    ]
    value = spec["v"]
    entries.append(
        f"/V {value}".encode() if value.startswith("/") else f"/V ({value})".encode()
    )
    if spec.get("ff") is not None:
        entries.append(f"/Ff {spec['ff']}".encode())
    if spec.get("opt") is not None:
        rendered = " ".join(f"({option})" for option in spec["opt"])
        entries.append(f"/Opt [{rendered}]".encode())
    return b"<< " + b" ".join(entries) + b" >>"


def _acroform_pdf(fields: dict[str, dict]) -> bytes:
    """Builds a minimal, hand-assembled one-page PDF with an `/AcroForm`,
    just enough structure for `pypdf.PdfReader.get_fields` to walk it —
    without pulling in a PDF-authoring dependency the validator itself does
    not need. Mirrors `test_validate_content_roundtrip.py`'s `_minimal_pdf`."""
    catalog_num = 1
    pages_num = 2
    page_num = 3
    acroform_num = 4
    field_nums = {name: acroform_num + 1 + index for index, name in enumerate(fields)}
    refs = " ".join(f"{number} 0 R" for number in field_nums.values())

    parts: dict[int, bytes] = {
        catalog_num: (
            f"<< /Type /Catalog /Pages {pages_num} 0 R "
            f"/AcroForm {acroform_num} 0 R >>"
        ).encode(),
        pages_num: f"<< /Type /Pages /Kids [{page_num} 0 R] /Count 1 >>".encode(),
        page_num: (
            f"<< /Type /Page /Parent {pages_num} 0 R /MediaBox [0 0 200 200] "
            f"/Annots [{refs}] >>"
        ).encode(),
        acroform_num: f"<< /Fields [{refs}] /DA (/Helv 12 Tf 0 g) >>".encode(),
    }
    for name, spec in fields.items():
        parts[field_nums[name]] = _field_object(name, spec)

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


def _authored_fields() -> dict[str, dict]:
    return {
        "applicant": {"ft": "/Tx", "v": "Ada Lovelace", "ff": MULTILINE},
        "agrees": {"ft": "/Btn", "v": "/Yes"},
        "plan": {"ft": "/Btn", "v": "/pro", "ff": RADIO},
        "country": {
            "ft": "/Ch",
            "v": "Uruguay",
            "ff": COMBO,
            "opt": ["Argentina", "Uruguay", "Chile"],
        },
    }


class ValidatorCliTests(unittest.TestCase):
    def _run(self, fields: dict[str, dict], mode: str = "authored") -> int:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "output.pdf"
            path.write_bytes(_acroform_pdf(fields))
            return main(["validate_forms_roundtrip.py", str(path), mode])

    def test_rejects_missing_arguments_with_usage_exit_code(self) -> None:
        self.assertEqual(main(["validate_forms_roundtrip.py"]), 2)

    def test_rejects_an_unknown_mode_with_usage_exit_code(self) -> None:
        self.assertEqual(main(["validate_forms_roundtrip.py", "any.pdf", "nope"]), 2)

    def test_rejects_missing_file_without_interpreting_shell_metacharacters(self) -> None:
        self.assertEqual(
            main(["validate_forms_roundtrip.py", "missing;not-executed.pdf", "authored"]), 2
        )

    def test_rejects_a_non_pdf_existing_file_as_a_parse_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "not-a-pdf.pdf"
            path.write_text("not a PDF", encoding="utf-8")

            self.assertEqual(main(["validate_forms_roundtrip.py", str(path), "authored"]), 1)

    def test_accepts_a_valid_authored_form(self) -> None:
        self.assertEqual(self._run(_authored_fields()), 0)

    def test_rejects_a_field_whose_value_was_not_written(self) -> None:
        fields = _authored_fields()
        fields["applicant"]["v"] = ""

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_button_left_in_its_off_state(self) -> None:
        fields = _authored_fields()
        fields["agrees"]["v"] = "/Off"

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_dropdown_that_lost_its_options(self) -> None:
        fields = _authored_fields()
        fields["country"]["opt"] = ["Argentina"]

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_field_that_lost_its_kind_flag(self) -> None:
        fields = _authored_fields()
        fields["plan"]["ff"] = None

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_field_written_with_the_wrong_type(self) -> None:
        fields = _authored_fields()
        fields["country"]["ft"] = "/Tx"

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_dropped_field(self) -> None:
        fields = _authored_fields()
        del fields["plan"]

        self.assertEqual(self._run(fields), 1)

    def test_rejects_an_unexpected_extra_field(self) -> None:
        fields = _authored_fields()
        fields["stowaway"] = {"ft": "/Tx", "v": "surprise"}

        self.assertEqual(self._run(fields), 1)

    def test_rejects_a_document_with_no_acroform_at_all(self) -> None:
        self.assertEqual(self._run({}), 1)

    def test_authored_expectations_are_not_accidentally_the_filled_ones(self) -> None:
        self.assertEqual(self._run(_authored_fields(), mode="filled"), 1)


if __name__ == "__main__":
    unittest.main()
