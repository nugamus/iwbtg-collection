"""Bounded CI timeouts, with the native cap covering the measured full-coverage run (#1041)."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class TimeoutTests(unittest.TestCase):
    def setUp(self):
        self.text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

    def test_native_cap_covers_the_measured_full_coverage_run(self):
        native = self.text.split("\n  native:", 1)[1].split("\n  # The two wasm targets", 1)[0]
        cap = int(re.search(r"timeout-minutes:\s*(\d+)", native).group(1))
        # 28m36s measured with a cold-ish cache (run 44531896479); leave room for the main cache save.
        self.assertGreaterEqual(cap, 40)
        self.assertLessEqual(cap, 60, "keep a wedged job bounded")

    def test_wasm_caps_are_unchanged(self):
        for job in ("wasm32-wasip1:", "wasm32-unknown-unknown:"):
            block = self.text.split("\n  " + job, 1)[1]
            self.assertIn("timeout-minutes: 25", block[:600], job)


if __name__ == "__main__":
    unittest.main()
