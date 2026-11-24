"""Tracked workspace paths and runnable commands name the actual checkouts (#1042, #1346)."""
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class WorkspacePathTests(unittest.TestCase):
    def read(self, name):
        return (ROOT / name).read_text(encoding="utf-8")

    def test_layout_names_all_three_checkouts(self):
        text = self.read("docs/dev-setup.md")
        for folder in ("iwbtg-collection/", "iwbtg-collection-research/", "iwbtg-collection-games/"):
            self.assertIn(folder, text)

    def test_runnable_commands_use_the_client_folder(self):
        setup = self.read("docs/dev-setup.md")
        self.assertIn("python -I iwbtg-collection/.claude/stats.py", setup)
        self.assertIn("cd iwbtg-collection", setup)
        self.assertNotIn("python -I client/.claude/stats.py", setup)
        self.assertNotIn("cd client\n", setup)

    def test_fidelity_uses_the_research_checkout(self):
        setup = self.read("docs/dev-setup.md")
        self.assertIn("IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask fidelity", setup)
        self.assertNotIn("IWBTG_RESEARCH=../research", setup)

    def test_metrics_and_workflow_commands_agree(self):
        for name in ("docs/velocity.md", "docs/agent-workflow.md"):
            self.assertIn("python -I iwbtg-collection/.claude/stats.py", self.read(name), name)

    def test_clean_room_contract_points_at_the_research_checkout(self):
        contract = self.read("CLAUDE.md")
        self.assertIn("../iwbtg-collection-research/specs/", contract)
        self.assertIn("../iwbtg-collection-games", contract)
        self.assertIn("IWBTG_RESEARCH=../iwbtg-collection-research", contract)

    def test_issue_hook_resolves_the_tracked_checkouts(self):
        hook = self.read(".claude/hooks/issues.py")
        # Paths come from the REPOS map, not literal `client`/`research`/`games` folder names.
        self.assertIn('REPOS = {"c": "iwbtg-collection"', hook)
        self.assertNotIn('WORKSPACE / "client"', hook)
        self.assertNotIn('for repo in ("client", "research", "games")', hook)
        self.assertNotIn('root / "client"', hook)
        self.assertNotIn("client/workspace/settings.json", hook)
        self.assertIn("IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask fidelity", hook)
        self.assertNotIn("IWBTG_RESEARCH=../research", hook)

    def test_xtask_fidelity_error_names_the_research_checkout(self):
        xtask = self.read("crates/xtask/src/main.rs")
        self.assertIn("for example ../iwbtg-collection-research", xtask)
        self.assertNotIn("for example ../research", xtask)

    def test_workspace_settings_load_hooks_from_the_tracked_checkouts(self):
        settings = json.loads(self.read("workspace/settings.json"))
        self.assertIsInstance(settings["hooks"], dict)  # the file stays valid JSON
        text = self.read("workspace/settings.json")
        self.assertNotIn("${CLAUDE_PROJECT_DIR}/client/", text)
        self.assertNotIn("${CLAUDE_PROJECT_DIR}/research/", text)
        for folder in ("iwbtg-collection/", "iwbtg-collection-research/"):
            self.assertIn(f"${{CLAUDE_PROJECT_DIR}}/{folder}", text)


if __name__ == "__main__":
    unittest.main()
