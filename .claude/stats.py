"""Where agent time goes: one row per session and teammate, read from Claude Code transcripts.

Run from the workspace root: python -I client/.claude/stats.py [--days N]

Columns:
  wall     first to last message, hours
  turns    model calls
  ctx      average context per call, thousands of tokens (cost grows with it)
  build    hours spent inside cargo commands, and that share of the wall time
  idle     hours with no tool running and no model call for over 5 minutes in a row: waiting for a
           message (teammates: usually the lead; a lead session: usually the user)
  10min    cargo commands that hit the 10-minute tool limit or were moved to the background
  merges   merges of main into a branch (`git merge origin/main`, `gh pr update-branch`)
  confl    tool results that showed a merge conflict
  PRs      `gh pr create` calls

--issues lists the issues that took longest instead. Time is attributed to the issue a session last
named in a shell command (an `issue-<n>-...` branch, `gh issue edit <n> --add-label in-progress`), so
it is an estimate: work before the first such command counts for no issue.
"""

import argparse
from collections import defaultdict
import json
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

def projects_dir():
    """Transcripts of the workspace root this file sits under (also from a worktree in worktrees/)."""
    for parent in Path(__file__).resolve().parents:
        d = Path.home() / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(parent))
        if (parent / "client").is_dir() and d.is_dir():
            return d
    return d


PROJECTS = projects_dir()


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def scan(path):
    r = dict(first=None, last=None, turns=0, ctx=0, cargo=0.0, idle=0.0, slow=0, merges=0, confl=0, prs=0,
             issues=defaultdict(lambda: dict(wall=0.0, cargo=0.0, idle=0.0, turns=0)))
    pending = {}
    current = None
    for line in path.open(encoding="utf-8", errors="replace"):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        t = e.get("timestamp")
        if not t:
            continue
        t = ts(t)
        gap = (t - r["last"]).total_seconds() if r["last"] else 0
        if gap > 300 and not pending:
            r["idle"] += gap
            if current:
                r["issues"][current]["idle"] += gap
        if current and gap > 0:
            r["issues"][current]["wall"] += gap
        r["first"] = r["first"] or t
        r["last"] = max(r["last"] or t, t)  # queued notices carry their older send time
        m = e.get("message") or {}
        if e.get("type") == "assistant" and m.get("usage"):
            u = m["usage"]
            r["turns"] += 1
            if current:
                r["issues"][current]["turns"] += 1
            r["ctx"] += sum(u.get(k, 0) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"))
        content = m.get("content")
        if not isinstance(content, list):
            continue
        for b in content:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "tool_use" and b.get("name") in ("Bash", "PowerShell"):
                cmd = (b.get("input") or {}).get("command", "")
                if re.search(r"merge (origin/)?main|update-branch", cmd):
                    r["merges"] += 1
                if "gh pr create" in cmd:
                    r["prs"] += 1
                m2 = re.search(r"issue-(\d+)-", cmd) or re.search(r"gh issue edit (\d+)[^\n]*--add-label in-progress", cmd)
                if m2:
                    current = int(m2.group(1))
                pending[b["id"]] = (cmd, t)
            elif b.get("type") == "tool_result" and b.get("tool_use_id") in pending:
                cmd, t0 = pending.pop(b["tool_use_id"])
                text = json.dumps(b.get("content"))[:4000]
                if "CONFLICT" in text:
                    r["confl"] += 1
                if re.search(r"\bcargo (build|check|clippy|test|run|nextest)", cmd):
                    d = (t - t0).total_seconds()
                    r["cargo"] += d
                    if current:
                        r["issues"][current]["cargo"] += d
                    if d >= 590 or "moved to the background" in text:
                        r["slow"] += 1
    return r


def row(name, r):
    wall = (r["last"] - r["first"]).total_seconds() / 3600
    share = r["cargo"] / 3600 / wall * 100 if wall else 0
    ctx = r["ctx"] / max(1, r["turns"]) / 1000
    return (f"{name:28s} {r['first']:%m-%d %H:%M} {wall:5.1f}h {r['turns']:5d} {ctx:5.0f}k "
            f"{r['cargo'] / 3600:5.1f}h {share:3.0f}% {r['idle'] / 3600:5.1f}h {r['slow']:5d} {r['merges']:6d} {r['confl']:5d} {r['prs']:3d}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=float, default=3, help="sessions started in the last N days (default 3)")
    ap.add_argument("--issues", action="store_true", help="list the issues that took longest instead")
    a = ap.parse_args()
    if not PROJECTS.is_dir():
        sys.exit(f"no transcripts at {PROJECTS}")
    since = datetime.now(timezone.utc) - timedelta(days=a.days)
    if a.issues:
        return by_issue(since)
    print(f"{'session / teammate':28s} {'start':11s} {'wall':>6s} {'turns':>5s} {'ctx':>6s} {'build':>6s} {'%':>4s} "
          f"{'idle':>6s} {'10min':>5s} {'merges':>6s} {'confl':>5s} {'PRs':>3s}")
    total = dict(wall=0.0, cargo=0.0, idle=0.0)
    for lead in sorted(PROJECTS.glob("*.jsonl"), key=lambda p: p.stat().st_mtime):
        r = scan(lead)
        if not r["first"] or r["last"] < since or r["turns"] < 5:
            continue
        print(row(lead.stem[:8] + " (lead)", r))
        for sub in sorted((PROJECTS / lead.stem / "subagents").glob("**/*.jsonl")):
            s = scan(sub)
            if not s["first"] or s["turns"] < 5:
                continue
            name = re.sub(r"^agent-a?|-[0-9a-f]{6,}$", "", sub.stem)
            print(row("  " + name[:26], s))
            total["wall"] += (s["last"] - s["first"]).total_seconds() / 3600
            total["cargo"] += s["cargo"] / 3600
            total["idle"] += s["idle"] / 3600
    if total["wall"]:
        print(f"\nTeammates: {total['cargo']:.1f}h of {total['wall']:.1f}h inside cargo "
              f"({total['cargo'] / total['wall'] * 100:.0f}%), {total['idle']:.1f}h idle "
              f"({total['idle'] / total['wall'] * 100:.0f}%).")


def by_issue(since, top=15):
    total = defaultdict(lambda: dict(wall=0.0, cargo=0.0, idle=0.0, turns=0, agents=0))
    for f in PROJECTS.glob("**/*.jsonl"):
        r = scan(f)
        if not r["first"] or r["last"] < since:
            continue
        for n, v in r["issues"].items():
            for k in ("wall", "cargo", "idle", "turns"):
                total[n][k] += v[k]
            total[n]["agents"] += 1
    print(f"{'issue':>6s} {'agent h':>8s} {'build':>6s} {'idle':>6s} {'turns':>5s} {'sessions':>8s}")
    for n, v in sorted(total.items(), key=lambda kv: -kv[1]["wall"])[:top]:
        print(f"#{n:<5d} {v['wall'] / 3600:7.1f}h {v['cargo'] / 3600:5.1f}h {v['idle'] / 3600:5.1f}h {v['turns']:5d} {v['agents']:8d}")


if __name__ == "__main__":
    main()
