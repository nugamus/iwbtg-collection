"""Superseded PR work cancels; a main push may not cancel the active cache-publishing run (#1027)."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ConcurrencyTests(unittest.TestCase):
    def setUp(self):
        self.text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

    def test_pr_runs_still_cancel_superseded_work(self):
        self.assertIn(
            "cancel-in-progress: ${{ github.event_name != 'push' }}", self.text
        )

    def test_push_trigger_is_main_only(self):
        on = self.text.split("\non:", 1)[1].split("\nconcurrency:", 1)[0]
        self.assertIn("branches: [main]", on)

    def test_concurrency_is_one_group_per_ref(self):
        self.assertIn("group: ci-${{ github.ref }}", self.text)


if __name__ == "__main__":
    unittest.main()
