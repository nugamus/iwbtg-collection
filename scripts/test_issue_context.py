"""Self-test for the agent-ready `## Context` audit (#1610, part of #1610's guard half).

The SessionStart hook runs `scripts/check_issue_context.py --hook`; this file proves its
detection and its read-only GitHub call. CI's changes job discovers `scripts/test_*.py`, so
the guard stays tested without a workflow edit. The live board is not touched: every case
here feeds a fixture.
"""
import importlib.util
import io
import json
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import check_issue_context as audit

ROOT = Path(__file__).resolve().parents[1]

WITH_CONTEXT = (
    "## Goal\n\nDo the thing.\n\n"
    "## Context\n\n- Code: `crates/x/src/lib.rs`\n- Test: `cargo test -p x`\n\n"
    "## Done when\n\n- [ ] done\n"
)
EMPTY_CONTEXT = "## Goal\n\nDo the thing.\n\n## Context\n\n## Done when\n\n- [ ] done\n"
NO_CONTEXT = "## Goal\n\nDo the thing.\n\n## Done when\n\n- [ ] done\n"


def board(*bodies):
    return [
        {"number": index + 1, "title": f"Issue {index + 1}", "body": body}
        for index, body in enumerate(bodies)
    ]


class Result:
    def __init__(self, returncode=0, stdout="[]", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


def runner(issues=None, result=None, calls=None):
    """A fake `subprocess.run` that answers one `gh issue list` call."""
    def run(command, **kwargs):
        if calls is not None:
            calls.append((command, kwargs))
        if result is not None:
            return result
        return Result(stdout=json.dumps(issues or []))
    return run


class DetectionTests(unittest.TestCase):
    def test_a_present_block_is_extracted(self):
        self.assertEqual(
            audit.context_block(WITH_CONTEXT),
            "- Code: `crates/x/src/lib.rs`\n- Test: `cargo test -p x`",
        )
        self.assertEqual(audit.context_block("## Context\n\n- one line\n"), "- one line")

    def test_a_missing_or_empty_block_is_missing(self):
        for body in (NO_CONTEXT, EMPTY_CONTEXT, "", None, "## Contextual\n- not the section\n"):
            self.assertEqual(audit.context_block(body), "", repr(body))

    def test_the_audit_and_the_hook_agree(self):
        # issues.py prints the block under the issue; this audit flags its absence. If either
        # regex drifts, an issue could print "(no ## Context block)" yet pass the guard.
        spec = importlib.util.spec_from_file_location(
            "hook_issues", ROOT / ".claude" / "hooks" / "issues.py"
        )
        hook = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(hook)
        for body in (WITH_CONTEXT, EMPTY_CONTEXT, NO_CONTEXT, "", None, "## Context\ntext\n## Next\n"):
            self.assertEqual(audit.context_block(body), hook.context_block(body), repr(body))


class ReportTests(unittest.TestCase):
    def test_only_issues_without_a_block_are_flagged(self):
        missing = audit.missing_context(board(WITH_CONTEXT, NO_CONTEXT, EMPTY_CONTEXT))
        self.assertEqual(missing, [(2, "Issue 2"), (3, "Issue 3")])

    def test_the_report_names_the_count_and_the_numbers(self):
        text = audit.report(board(WITH_CONTEXT, NO_CONTEXT, EMPTY_CONTEXT))
        self.assertIn("2 of 3", text)
        self.assertIn("#2, #3", text)
        self.assertNotIn("#1,", text)
        self.assertIn("docs/agent-workflow.md", text)

    def test_a_clean_board_says_so(self):
        self.assertIn("all 2 open agent-ready issue(s) have a ## Context block", audit.report(board(WITH_CONTEXT, WITH_CONTEXT)))

    def test_list_mode_prints_only_the_numbers(self):
        self.assertEqual(audit.report(board(WITH_CONTEXT, NO_CONTEXT), as_list=True), "2")
        self.assertIn("Agent-ready Context", audit.report(board(WITH_CONTEXT), as_list=False))


class GithubTests(unittest.TestCase):
    def test_the_call_is_one_read_only_list_with_bodies(self):
        calls = []
        issues = audit.fetch_issues(run=runner([{"number": 7, "title": "x", "body": WITH_CONTEXT}], calls=calls))
        self.assertEqual([issue["number"] for issue in issues], [7])
        self.assertEqual(len(calls), 1)
        command, kwargs = calls[0]
        self.assertEqual(command[:3], ["gh", "issue", "list"])
        self.assertIn("agent-ready", command)
        self.assertIn("number,title,body", command)
        self.assertIn("open", command)
        self.assertEqual(kwargs["timeout"], audit.TIMEOUT)
        for forbidden in ("edit", "comment", "close", "create", "reopen"):
            self.assertNotIn(forbidden, command)

    def test_missing_gh_is_a_readable_error(self):
        def run(command, **kwargs):
            raise FileNotFoundError("gh")
        with self.assertRaises(RuntimeError) as caught:
            audit.fetch_issues(run=run)
        self.assertIn("gh is not available", str(caught.exception))

    def test_a_failed_or_garbled_answer_is_a_readable_error(self):
        for result in (
            Result(returncode=1, stderr="gh: not logged in\n"),
            Result(stdout="not json"),
            Result(stdout='{"message": "nope"}'),
        ):
            with self.assertRaises(RuntimeError, msg=result.stdout):
                audit.fetch_issues(run=runner(result=result))

    def test_a_timeout_is_a_readable_error(self):
        import subprocess
        def run(command, **kwargs):
            raise subprocess.TimeoutExpired(command, kwargs.get("timeout"))
        with self.assertRaises(RuntimeError) as caught:
            audit.fetch_issues(run=run)
        self.assertIn("TimeoutExpired", str(caught.exception))


class ExitTests(unittest.TestCase):
    def output(self, argv, run):
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            code = audit.main(argv, run=run)
        return code, buffer.getvalue()

    def test_findings_exit_one_and_are_printed(self):
        code, out = self.output([], runner(board(WITH_CONTEXT, NO_CONTEXT)))
        self.assertEqual(code, 1)
        self.assertIn("1 of 2", out)

    def test_a_clean_board_exits_zero(self):
        code, out = self.output([], runner(board(WITH_CONTEXT)))
        self.assertEqual(code, 0)
        self.assertIn("all 1", out)

    def test_hook_mode_always_exits_zero(self):
        code, out = self.output(["--hook"], runner(board(NO_CONTEXT)))
        self.assertEqual(code, 0)
        self.assertIn("1 of 1", out)

    def test_a_github_failure_skips_clearly_and_exits_zero(self):
        for argv in ([], ["--hook"], ["--list"]):
            code, out = self.output(argv, runner(result=Result(returncode=1, stderr="no auth\n")))
            self.assertEqual(code, 0, argv)
            self.assertIn("skipped", out)

    def test_list_mode_prints_numbers_and_keeps_the_finding_exit(self):
        code, out = self.output(["--list"], runner(board(WITH_CONTEXT, NO_CONTEXT)))
        self.assertEqual(code, 1)
        self.assertEqual(out.strip(), "2")


class SourceTests(unittest.TestCase):
    def test_the_audit_has_no_way_to_write_an_issue(self):
        # The read-only call is the guard's whole safety story: a `gh issue edit` anywhere in
        # the file would let a session-start hook rewrite the board.
        source = (ROOT / "scripts" / "check_issue_context.py").read_text(encoding="utf-8")
        for forbidden in ("issue edit", "issue comment", "issue close", "issue reopen", "issue create"):
            self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
