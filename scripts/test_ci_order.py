"""The Linux formatting gate must stay ahead of the expensive native build (#1031)."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class FormatOrderTests(unittest.TestCase):
    def test_formatting_runs_before_clippy_and_tests(self):
        text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        fmt = text.index("cargo fmt --all -- --check")
        clippy = text.index("cargo clippy $PACKAGES")
        tests = text.index("cargo test $PACKAGES")
        self.assertLess(fmt, clippy)
        self.assertLess(fmt, tests)

    def test_formatting_stays_in_the_existing_native_job(self):
        text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertNotIn("\n  fmt:\n", text, "a separate fmt job adds another runner and toolchain")
        before = text[:text.index("cargo fmt --all -- --check")]
        self.assertIn("run: rustup show", before)


if __name__ == "__main__":
    unittest.main()
