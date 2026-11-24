"""Focused checks for the lane-instruction contradictions fixed in #1034.

These assert the specific handoff and path statements that misled agents, not a general
documentation style. A valid domain review or a lead integrating after a merge wave is fine.
"""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANES = ROOT / "docs" / "lanes"


class LaneInstructionTests(unittest.TestCase):
    def read(self, name):
        return (LANES / name).read_text(encoding="utf-8")

    def test_ui_merge_authority_is_the_pr_owner(self):
        text = self.read("ui.md")
        self.assertNotIn("The lead merges", text)
        self.assertIn("PR owner", text)
        self.assertIn("--squash --auto", text)

    def test_ui_names_the_current_hud_paths(self):
        text = self.read("ui.md")
        self.assertIn("crates/client/src/hud/", text)
        self.assertIn("docs/architecture/client/hud.md", text)

    def test_sim_does_not_claim_infra_ownership(self):
        text = self.read("sim.md")
        owns = text.split("**Owns:**", 1)[1].split("**Never edits:**", 1)[0]
        listed, _, delegated = owns.partition("The infra lane owns")
        self.assertNotIn(".github/", listed)
        self.assertIn("`.github/`", delegated)

    def test_every_lane_page_defers_to_one_contract(self):
        text = self.read("README.md")
        self.assertIn("authoritative", text.lower())
        self.assertIn("agent-workflow.md", text)
        for name in ("README.md", "data.md", "sim.md", "server.md", "research.md"):
            self.assertNotIn("lead merges", self.read(name).lower(), name)

    def test_the_external_prompt_does_not_decide_routine_merges(self):
        setup = (ROOT / "docs" / "dev-setup.md").read_text(encoding="utf-8")
        self.assertIn("TEAM_LEAD_PROMPT.md", setup)
        self.assertIn("#1025", setup)


if __name__ == "__main__":
    unittest.main()
