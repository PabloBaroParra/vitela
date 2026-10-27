"""Exercise the actual workflow commands with a sudo that drops the environment.

These mocks check command wiring, not Linux network isolation; the real CI
namespace and ignored egress proof remain responsible for that assertion.
"""

from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/core.yml"
STEPS = {
    "Run the full test suite inside a network namespace": [
        "test", "--workspace", "--locked", "--", "--skip", "gtk_ui_",
    ],
    "Run zero-network-guard's CI-only egress proof inside the namespace": [
        "test", "-p", "zero-network-guard", "--locked", "--", "--ignored",
    ],
}


class ZeroNetworkWorkflowTests(unittest.TestCase):
    def test_commands_survive_sudo_environment_reset(self):
        workflow = WORKFLOW.read_text()
        for step, arguments in STEPS.items():
            for custom_homes in (False, True):
                with self.subTest(step=step, custom_homes=custom_homes):
                    self.run_step(workflow, step, arguments, custom_homes)

    def run_step(self, workflow, step, arguments, custom_homes):
        block = workflow.split(f"      - name: {step}\n        run: |\n", 1)[1]
        command = textwrap.dedent(block.split("\n\n", 1)[0].split("\n      - name:", 1)[0])
        with tempfile.TemporaryDirectory(prefix="zero network;") as directory:
            root = Path(directory)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            expected = {
                "HOME": str(root / "runner home"),
                "RUSTFLAGS": "-D warnings",
                "CARGO_INCREMENTAL": "0",
                "PDFIUM_DYNAMIC_LIB_PATH": str(root / "pdfium lib/libpdfium.so"),
            }
            environment = dict(expected, PATH=f"{bin_dir}:/usr/bin:/bin")
            for name, default in (("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")):
                expected[name] = str(root / f"custom {name}" if custom_homes else Path(expected["HOME"]) / default)
                if custom_homes:
                    environment[name] = expected[name]
            scripts = {
                "sudo": '#!/bin/sh\n[ "$1" != -E ] || shift\nexec /usr/bin/env -i HOME=/root PATH=/usr/bin:/bin "$@"\n',
                "unshare": '#!/bin/sh\n[ "$1" = --net ] || exit 91\nshift\nexec "$@"\n',
                "ip": '#!/bin/sh\n[ "$*" = "link set lo up" ]\n',
                "cargo": (
                    f"#!{sys.executable}\nimport os, sys\n"
                    f"expected = {expected!r}\n"
                    "assert {key: os.environ.get(key) for key in expected} == expected\n"
                    f"assert sys.argv[1:] == {arguments!r}\n"
                    "print('cargo reached with runner environment')\n"
                ),
            }
            for name, contents in scripts.items():
                path = bin_dir / name
                path.write_text(contents)
                path.chmod(0o755)
            result = subprocess.run(
                ["/bin/bash", "-eu", "-c", command], env=environment,
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("cargo reached with runner environment", result.stdout)


if __name__ == "__main__":
    unittest.main()
