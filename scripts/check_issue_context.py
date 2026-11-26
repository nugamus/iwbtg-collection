"""Audit open `agent-ready` issues for the `## Context` block that label promises (#1610).

Every `agent-ready` issue carries a non-empty `## Context` block — code paths, the doc page
or claim ids, the narrow test command and what must not change — so the next agent can
start without rediscovering the codebase (CLAUDE.md, Issues; docs/agent-workflow.md,
Issues and triage). Nothing gated the label, so #1610 found 84 of 118 open issues without
one. This audit flags them and never edits an issue.

  python -I scripts/check_issue_context.py          full report; exit 1 when any are missing
  python -I scripts/check_issue_context.py --list   only the missing issue numbers
  python -I scripts/check_issue_context.py --hook   one line for the SessionStart hook; exit 0

The GitHub read is one read-only `gh issue list`. Without `gh`, a token or the network it
prints a clear `skipped` line and exits 0: a transient failure must never break a session.
The SessionStart hook (.claude/hooks/issues.py, `start`) runs `--hook` when this file
exists, so an issue that loses its block shows up on the next session start.
"""
import argparse
import json
import re
import subprocess
import sys

REPO = "nugamus/iwbtg-collection"
# Same section rule as .claude/hooks/issues.py's context_block, which prints the block;
# scripts/test_issue_context.py asserts the two agree on every sample.
CONTEXT_SECTION = re.compile(r"^## Context[ \t]*\r?\n(.*?)(?=^## |\Z)", re.M | re.S)
# All open agent-ready issues fit well under this; `gh issue list` pages for us.
FETCH_LIMIT = 500
# The one-line report names this many numbers, then says how many more.
SHOWN = 10
TIMEOUT = 8  # a session start must not wait on a stale token


def context_block(body):
    """The `## Context` section of an issue body, trimmed; `""` when the issue has none.

    Keep in step with `.claude/hooks/issues.py`'s `context_block`; the self-test compares them.
    """
    if not body:
        return ""
    match = CONTEXT_SECTION.search(body)
    return match.group(1).strip() if match else ""


def missing_context(issues):
    """The `(number, title)` pairs of issues whose body has no non-empty `## Context` block."""
    return [
        (issue.get("number"), issue.get("title") or "")
        for issue in issues
        if not context_block(issue.get("body"))
    ]


def fetch_issues(run=subprocess.run):
    """Open `agent-ready` issues through one read-only `gh issue list`.

    Raises `RuntimeError` with a readable reason when the call cannot run or its answer is
    unusable; callers turn that into a `skipped` line, never a failed session.
    """
    command = [
        "gh", "issue", "list", "--repo", REPO, "--state", "open", "--label", "agent-ready",
        "--limit", str(FETCH_LIMIT), "--json", "number,title,body",
    ]
    try:
        result = run(command, capture_output=True, text=True, encoding="utf-8", timeout=TIMEOUT)
    except (OSError, subprocess.SubprocessError) as error:
        raise RuntimeError(f"gh is not available ({type(error).__name__})") from error
    if result.returncode != 0:
        detail = (result.stderr or "").strip().splitlines()
        raise RuntimeError(f"gh failed: {detail[-1]}" if detail else f"gh exited {result.returncode}")
    try:
        issues = json.loads(result.stdout)
    except ValueError as error:
        raise RuntimeError("gh returned no JSON") from error
    if not isinstance(issues, list):
        raise RuntimeError("gh returned an unexpected shape")
    return issues


def report(issues, as_list=False):
    """The audit text: one line naming what is missing, or the bare numbers in `--list` mode."""
    missing = missing_context(issues)
    if as_list:
        return "\n".join(str(number) for number, _ in missing)
    if not missing:
        return f"Agent-ready Context (#1610): all {len(issues)} open agent-ready issue(s) have a ## Context block."
    shown = ", ".join(f"#{number}" for number, _ in missing[:SHOWN])
    more = f", +{len(missing) - SHOWN} more" if len(missing) > SHOWN else ""
    first_number, first_title = missing[0]
    return (
        f"Agent-ready Context (#1610): {len(missing)} of {len(issues)} open agent-ready issue(s) have no "
        f"## Context block: {shown}{more}. Each needs code paths, the doc page or claim ids, the narrow test "
        f"command and what must not change (docs/agent-workflow.md, Issues and triage). Start with "
        f"#{first_number} {first_title}; run `python -I scripts/check_issue_context.py --list` for every number."
    )


def main(argv=None, run=subprocess.run):
    """Run the audit and return its exit code: 0 clean or skipped, 1 when issues are missing."""
    parser = argparse.ArgumentParser(description="Audit open agent-ready issues for ## Context (#1610).")
    parser.add_argument("--list", action="store_true", dest="as_list",
                        help="print only the missing issue numbers")
    parser.add_argument("--hook", action="store_true",
                        help="one line for the SessionStart hook; always exits 0")
    args = parser.parse_args(argv)
    try:
        issues = fetch_issues(run=run)
    except RuntimeError as error:
        print(f"Agent-ready Context (#1610): skipped ({error}).")
        return 0
    text = report(issues, args.as_list)
    if text:
        print(text)
    if args.hook or not missing_context(issues):
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
