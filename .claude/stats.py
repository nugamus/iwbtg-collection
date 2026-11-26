"""Where agent time goes: one row per session and teammate, read from Claude Code transcripts.

Run from the workspace root: python -I client/.claude/stats.py [--days N]

Columns:
  wall     elapsed first-to-last in-window message, hours (a span, never a sum)
  calls    logical model calls: one per message id, or one per usage line when the id is absent
  ctx      average context per call, thousands of tokens (the largest usage seen for that call)
  build    hours inside cargo commands, union of their intervals (overlaps counted once)
  sum      the same commands added up; may exceed wall when calls overlap, so it has no share
  %        build / wall, using the union
  idle     in-window uncovered gaps over 5 minutes: no tool running and no model call
  slow     cargo commands that hit the 10-minute tool limit or were moved to the background
  open     tool calls with no result at the end of the transcript (partial or interrupted)
  merges   merges of main into a branch (`git merge origin/main`, `gh pr update-branch`)
  confl    tool results that showed a merge conflict
  PRs      `gh pr create` calls
  unk      events with no usable timestamp; they are reported, never guessed

Fragments of one message (the same message id on several transcript lines) are one call; token
usage is taken incrementally so the totals stay exact. A line without an id cannot be matched and
is counted on its own. `--days N` applies to events and clipped tool intervals, not whole sessions:
work before the window is excluded, and a call crossing the boundary counts only its in-window part.

--summary prints tokens and hours per merged PR, per lane (see summary()).

--issues lists the issues that took longest instead. Time is attributed to the issue a session last
named in a shell command (an `issue-<n>-...` branch, `gh issue edit <n> --add-label in-progress`), so
it is an estimate: work before the first such command counts for no issue.
"""

import argparse
from collections import defaultdict
import json
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path


def main_checkout(root):
    """The checkout a lane worktree came from: transcripts are keyed to it, not to the worktree."""
    try:
        out = subprocess.check_output(["git", "-C", str(root), "rev-parse", "--git-common-dir"],
                                      text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    common = Path(out)
    if not common.is_absolute():
        common = (root / common).resolve()
    return common.parent


def holds_client(parent):
    """True for the repo root, a workspace root with `client/`, or one holding the client checkout."""
    if (parent / "crates" / "client").is_dir() or (parent / "client").is_dir():
        return True
    try:
        return any((child / "crates" / "client").is_dir() for child in parent.iterdir() if child.is_dir())
    except OSError:
        return False


def project_dir(repo_root=None, home=None):
    """Transcript folder for the client repo this file belongs to (or its workspace root)."""
    root = Path(repo_root) if repo_root is not None else Path(__file__).resolve().parents[1]
    home = Path(home) if home is not None else Path.home()
    main = main_checkout(root)
    parents = [root, *([main] if main else []), *root.parents, *([main] if main else [])]
    seen = set()
    found = None
    for parent in parents:
        if parent in seen:
            continue
        seen.add(parent)
        d = home / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(parent))
        found = found or d
        if d.is_dir() and holds_client(parent):
            return d
    return found


PROJECTS = project_dir()


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def union_seconds(intervals):
    """Seconds covered by [start, end) intervals; overlapping or touching calls count once."""
    total = 0.0
    start = end = None
    for a, b in sorted(intervals):
        if b <= a:
            continue
        if start is None:
            start, end = a, b
        elif a <= end:
            end = max(end, b)
        else:
            total += end - start
            start, end = a, b
    if start is not None:
        total += end - start
    return total


def uncovered_seconds(a, b, intervals):
    """Part of [a, b) not covered by `intervals` (time a tool was running)."""
    pieces = [(max(a, x), min(b, y)) for x, y in intervals if min(b, y) > max(a, x)]
    return max(0.0, (b - a) - union_seconds(pieces))


def scan(path, since=None, until=None):
    """One transcript. `since`/`until` clip events and intervals to the reporting window."""
    r = dict(first=None, last=None, win_first=None, win_last=None, wall=0.0, turns=0, ctx=0,
             cargo=0.0, cargo_union=0.0, idle=0.0, slow=0, open=0, merges=0, confl=0, prs=0,
             fresh=0, cached=0, out=0, unknown=0, intervals=[],
             issues=defaultdict(lambda: dict(wall=0.0, cargo=0.0, cargo_union=0.0, idle=0.0,
                                             turns=0, tokens=0, intervals=[])))
    pending = {}       # tool_use id -> (command, start, issue, is_window_start)
    seen_usage = {}    # message id -> last (fresh, cached, out) usage
    call_ctx = {}      # message id -> largest context total seen
    current = None
    marks = []         # (time, issue active at that time) for in-window events
    tools = []         # (start, end) of every completed tool call, for idle subtraction

    def in_window(t):
        return (since is None or t >= since) and (until is None or t <= until)

    def clipped(a, b):
        lo = a if since is None else max(a, since)
        hi = b if until is None else min(b, until)
        return lo, hi

    for line in path.open(encoding="utf-8", errors="replace"):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        raw = e.get("timestamp")
        if not raw:
            if e.get("type") or e.get("message"):
                r["unknown"] += 1
            continue
        try:
            t = ts(raw)
        except ValueError:
            if e.get("type") or e.get("message"):
                r["unknown"] += 1
            continue
        r["first"] = t if r["first"] is None else min(r["first"], t)
        r["last"] = t if r["last"] is None else max(r["last"], t)
        inside = in_window(t)
        if inside:
            r["win_first"] = t if r["win_first"] is None else min(r["win_first"], t)
            r["win_last"] = t if r["win_last"] is None else max(r["win_last"], t)
        m = e.get("message") or {}
        if e.get("type") == "assistant" and m.get("usage") and inside:
            u = m["usage"]
            mine = ((u.get("input_tokens", 0) or 0) + (u.get("cache_creation_input_tokens", 0) or 0),
                    u.get("cache_read_input_tokens", 0) or 0,
                    u.get("output_tokens", 0) or 0)
            mid = m.get("id")
            if mid is None:
                # No identity to deduplicate against: count the line once, as honestly as possible.
                old = (0, 0, 0)
                r["turns"] += 1
                if current:
                    r["issues"][current]["turns"] += 1
                total_ctx = mine[0] + mine[1]  # input, cache writes and cache reads; not output
                r["ctx"] += total_ctx
            else:
                old = seen_usage.get(mid)
                if old is None:
                    old = (0, 0, 0)
                    r["turns"] += 1
                    if current:
                        r["issues"][current]["turns"] += 1
                total_ctx = mine[0] + mine[1]
                before = call_ctx.get(mid, 0)
                if total_ctx > before:
                    r["ctx"] += total_ctx - before
                    call_ctx[mid] = total_ctx
            d = [max(0, a - b) for a, b in zip(mine, old)]
            seen_usage[mid] = mine
            r["fresh"] += d[0]
            r["cached"] += d[1]
            r["out"] += d[2]
            if current:
                r["issues"][current]["tokens"] += d[0] + d[2]
        content = m.get("content")
        for b in content if isinstance(content, list) else ():
            if not isinstance(b, dict):
                continue
            if b.get("type") == "tool_use" and b.get("name") in ("Bash", "PowerShell"):
                cmd = (b.get("input") or {}).get("command", "")
                if inside:
                    if re.search(r"merge (origin/)?main|update-branch", cmd):
                        r["merges"] += 1
                    if "gh pr create" in cmd:
                        r["prs"] += 1
                    m2 = re.search(r"issue-(\d+)-", cmd) or re.search(r"gh issue edit (\d+)[^\n]*--add-label in-progress", cmd)
                    if m2:
                        current = int(m2.group(1))
                if b.get("id"):
                    pending[b["id"]] = (cmd, t, current)
            elif b.get("type") == "tool_result" and b.get("tool_use_id") in pending:
                cmd, t0, issue = pending.pop(b["tool_use_id"])
                text = json.dumps(b.get("content"))[:4000]
                if "CONFLICT" in text and inside:
                    r["confl"] += 1
                if re.search(r"\bcargo (build|check|clippy|test|run|nextest)", cmd):
                    a, z = clipped(t0, t)
                    raw = (t - t0).total_seconds()
                    if z > a:
                        # A call crossing the window boundary still makes the window busy from its edge.
                        r["win_first"] = a if r["win_first"] is None else min(r["win_first"], a)
                        r["win_last"] = z if r["win_last"] is None else max(r["win_last"], z)
                        d = (z - a).total_seconds()
                        r["cargo"] += d
                        r["intervals"].append((a.timestamp(), z.timestamp()))
                        tools.append((a.timestamp(), z.timestamp()))
                        if issue is not None:
                            r["issues"][issue]["cargo"] += d
                            r["issues"][issue]["intervals"].append((a.timestamp(), z.timestamp()))
                        if raw >= 590 or "moved to the background" in text:
                            r["slow"] += 1
        if inside:
            # The issue active after this line's commands owns the span to the next event.
            marks.append((t, current))

    r["open"] = len(pending)
    r["cargo_union"] = union_seconds(r["intervals"])
    for v in r["issues"].values():
        v["cargo_union"] = union_seconds(v.pop("intervals"))
    if r["win_first"] is not None and r["win_last"] is not None:
        r["wall"] = (r["win_last"] - r["win_first"]).total_seconds()

    # Gaps between in-window events are elapsed time; the part with a tool running is not idle.
    marks.sort(key=lambda p: p[0])
    for (t0, issue0), (t1, _) in zip(marks, marks[1:]):
        gap = (t1 - t0).total_seconds()
        idle = uncovered_seconds(t0.timestamp(), t1.timestamp(), tools)
        if idle > 300:
            r["idle"] += idle
            if issue0 is not None:
                r["issues"][issue0]["idle"] += idle
        if issue0 is not None:
            r["issues"][issue0]["wall"] += max(0.0, gap)
    return r


def row(name, r):
    wall = r["wall"] / 3600
    union = r["cargo_union"] / 3600
    share = union / wall * 100 if wall else 0
    ctx = r["ctx"] / max(1, r["turns"]) / 1000
    start = r["win_first"] or r["first"]
    return (f"{name:28s} {start:%m-%d %H:%M} {wall:5.1f}h {r['turns']:5d} {ctx:5.0f}k "
            f"{union:5.1f}h {r['cargo'] / 3600:5.1f}h {share:3.0f}% {r['idle'] / 3600:5.1f}h "
            f"{r['slow']:5d} {r['open']:5d} {r['merges']:6d} {r['confl']:5d} {r['prs']:3d} {r['unknown']:4d}")


def lane_of(stem):
    """Teammate transcripts are agent-a<name>-<id>; the name is a lane plus a counter (data2, res191)."""
    m = re.match(r"agent-a([a-z]{2,})", stem)
    return {"res": "research"}.get(m.group(1), m.group(1)) if m else "other"


def merged_prs(since):
    """(number, issue) of PRs merged since `since`, from gh; the issue comes from an issue-<n>- branch."""
    try:
        out = subprocess.run(["gh", "pr", "list", "--repo", "nugamus/iwbtg-collection", "--state", "merged", "--limit", "300",
                              "--search", f"merged:>={since:%Y-%m-%d}", "--json", "number,headRefName"],
                             capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    res = []
    for p in json.loads(out):
        m = re.match(r"issue-(\d+)-", p["headRefName"])
        res.append((p["number"], int(m.group(1)) if m else None))
    return res


def summary(since):
    """Tokens and hours per merged client PR, per lane.

    Tokens: fresh = input plus cache writes, out = output; cache reads are listed apart (cheap, but they scale
    with context). Hours: in-window wall time minus idle (waiting for a message). A merged PR counts for the
    lane of the session that spent most time on its issue; PRs with no issue-<n>- branch count for no lane.
    Spend with no PR (research lane work lands in another repo, lead sessions) is in the lane's tokens but
    gives no PR."""
    lanes = defaultdict(lambda: dict(fresh=0, cached=0, out=0, hours=0.0, sessions=0))
    issue_lane = {}
    best = defaultdict(float)
    for lead in PROJECTS.glob("*.jsonl"):
        for f, lane in [(lead, "lead")] + [(s, lane_of(s.stem))
                                           for s in (PROJECTS / lead.stem / "subagents").glob("**/*.jsonl")]:
            r = scan(f, since)
            if r["win_first"] is None or r["turns"] < 5:
                continue
            L = lanes[lane]
            L["sessions"] += 1
            for k in ("fresh", "cached", "out"):
                L[k] += r[k]
            L["hours"] += (r["wall"] - r["idle"]) / 3600
            for n, v in r["issues"].items():
                if v["wall"] > best[n]:
                    best[n], issue_lane[n] = v["wall"], lane
    prs = merged_prs(since)
    if prs is None:
        print("gh failed: no merged PR counts")
        prs = []
    count = defaultdict(int)
    for _, issue in prs:
        if issue in issue_lane:
            count[issue_lane[issue]] += 1
    print(f"{'lane':10s} {'sessions':>8s} {'fresh M':>8s} {'cache M':>8s} {'out M':>7s} {'hours':>6s} {'PRs':>4s} {'fresh k/PR':>10s} {'out k/PR':>9s} {'h/PR':>6s}")
    tot = dict(fresh=0, cached=0, out=0, hours=0.0, sessions=0)
    for name, L in sorted(lanes.items()):
        n = count[name]
        per = (lambda x: f"{x / n:.0f}") if n else (lambda x: "-")
        print(f"{name:10s} {L['sessions']:8d} {L['fresh'] / 1e6:8.1f} {L['cached'] / 1e6:8.1f} {L['out'] / 1e6:7.2f} {L['hours']:6.1f} {n:4d} "
              f"{per(L['fresh'] / 1e3):>10s} {per(L['out'] / 1e3):>9s} {(L['hours'] / n if n else 0):6.1f}")
        for k in tot:
            tot[k] += L[k]
    n = len(prs)
    print(f"\nAll lanes: {len(prs)} PRs merged in {(datetime.now(timezone.utc) - since).days} days ({sum(count.values())} matched to a lane).")
    if n:
        print(f"Per merged PR: {tot['fresh'] / n / 1e3:.0f}k fresh input, {tot['out'] / n / 1e3:.0f}k output, "
              f"{tot['cached'] / n / 1e6:.1f}M cache reads, {tot['hours'] / n:.2f} active hours.")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=float, default=3, help="events in the last N days (default 3)")
    ap.add_argument("--issues", action="store_true", help="list the issues that took longest instead")
    ap.add_argument("--summary", action="store_true", help="tokens and hours per merged PR, per lane (use with --days 7)")
    ap.add_argument("--test", action="store_true", help="run the synthetic accounting tests")
    a = ap.parse_args()
    if a.test:
        return test()
    if not PROJECTS.is_dir():
        sys.exit(f"no transcripts at {PROJECTS}")
    since = datetime.now(timezone.utc) - timedelta(days=a.days)
    if a.summary:
        return summary(since)
    if a.issues:
        return by_issue(since)
    print(f"{'session / teammate':28s} {'start':11s} {'wall':>6s} {'calls':>5s} {'ctx':>6s} {'build':>6s} {'sum':>6s} {'%':>4s} "
          f"{'idle':>6s} {'slow':>5s} {'open':>5s} {'merges':>6s} {'confl':>5s} {'PRs':>3s} {'unk':>4s}")
    total = dict(wall=0.0, cargo=0.0, cargo_union=0.0, idle=0.0)
    for lead in sorted(PROJECTS.glob("*.jsonl"), key=lambda p: p.stat().st_mtime):
        r = scan(lead, since)
        if r["win_first"] is None or r["turns"] < 5:
            continue
        print(row(lead.stem[:8] + " (lead)", r))
        for sub in sorted((PROJECTS / lead.stem / "subagents").glob("**/*.jsonl")):
            s = scan(sub, since)
            if s["win_first"] is None or s["turns"] < 5:
                continue
            name = re.sub(r"^agent-a?|-[0-9a-f]{6,}$", "", sub.stem)
            print(row("  " + name[:26], s))
            for k in total:
                total[k] += s[k]
    if total["wall"]:
        print(f"\nTeammates: {total['cargo_union'] / 3600:.1f}h build elapsed of {total['wall'] / 3600:.1f}h wall "
              f"({total['cargo_union'] / total['wall'] * 100:.0f}%), {total['cargo'] / 3600:.1f}h summed across commands, "
              f"{total['idle'] / 3600:.1f}h idle ({total['idle'] / total['wall'] * 100:.0f}%).")


def by_issue(since, top=15):
    total = defaultdict(lambda: dict(wall=0.0, cargo=0.0, cargo_union=0.0, idle=0.0, turns=0, agents=0))
    for f in PROJECTS.glob("**/*.jsonl"):
        r = scan(f, since)
        if r["win_first"] is None:
            continue
        for n, v in r["issues"].items():
            for k in ("wall", "cargo", "cargo_union", "idle", "turns"):
                total[n][k] += v[k]
            total[n]["agents"] += 1
    print(f"{'issue':>6s} {'agent h':>8s} {'build':>6s} {'sum':>6s} {'idle':>6s} {'calls':>5s} {'sessions':>8s}")
    for n, v in sorted(total.items(), key=lambda kv: -kv[1]["wall"])[:top]:
        print(f"#{n:<5d} {v['wall'] / 3600:7.1f}h {v['cargo_union'] / 3600:5.1f}h {v['cargo'] / 3600:5.1f}h "
              f"{v['idle'] / 3600:5.1f}h {v['turns']:5d} {v['agents']:8d}")


def test():
    """Synthetic transcripts only: no real session text is read."""
    def usage(t, mid=None, inp=10, cached=100, out=5):
        msg = {"usage": {"input_tokens": inp, "cache_creation_input_tokens": 0,
                         "cache_read_input_tokens": cached, "output_tokens": out}, "content": []}
        if mid is not None:
            msg["id"] = mid
        return {"type": "assistant", "timestamp": t, "message": msg}

    def tool_use(t, tid, cmd):
        return {"type": "assistant", "timestamp": t,
                "message": {"content": [{"type": "tool_use", "id": tid, "name": "Bash",
                                          "input": {"command": cmd}}]}}

    def tool_result(t, tid, text="ok"):
        return {"type": "user", "timestamp": t,
                "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": text}]}}

    def scanned(events, since=None):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "session.jsonl"
            path.write_text("".join(json.dumps(e) + "\n" for e in events), encoding="utf-8")
            return scan(path, since)

    # One logical call on two lines: one turn, one context, incremental tokens.
    r = scanned([usage("2026-01-01T00:00:00Z", "m1"),
                 usage("2026-01-01T00:00:00Z", "m1")])
    assert (r["turns"], r["ctx"], r["fresh"], r["cached"], r["out"]) == (1, 110, 10, 100, 5), r
    r = scanned([usage("2026-01-01T00:00:00Z", "m1", out=5),
                 usage("2026-01-01T00:00:01Z", "m1", out=12)])
    assert (r["turns"], r["ctx"], r["out"]) == (1, 110, 12), r

    # A line with no id cannot be matched and is its own call, not silently merged into another.
    r = scanned([usage("2026-01-01T00:00:00Z"), usage("2026-01-01T00:00:01Z")])
    assert (r["turns"], r["ctx"], r["out"]) == (2, 220, 10), r

    # Overlapping 20 s cargo calls in a 30 s span: sum 40 s, elapsed union 30 s, share 100%.
    r = scanned([tool_use("2026-01-01T00:00:00Z", "t1", "cargo test"),
                 tool_use("2026-01-01T00:00:10Z", "t2", "cargo check"),
                 tool_result("2026-01-01T00:00:20Z", "t1"),
                 tool_result("2026-01-01T00:00:30Z", "t2")])
    assert (r["wall"], r["cargo"], r["cargo_union"]) == (30, 40, 30), r
    assert "100%" in row("x", r), row("x", r)

    # A partial call (no result) contributes no duration and is reported as open.
    r = scanned([usage("2026-01-01T00:00:00Z", "m1"),
                 tool_use("2026-01-01T00:00:00Z", "t1", "cargo build")])
    assert (r["open"], r["cargo"], r["cargo_union"]) == (1, 0, 0), r

    # The window excludes older work and clips a call crossing its start.
    events = [usage("2026-01-01T23:00:00Z", "old", inp=999, cached=0, out=99),
              tool_use("2026-01-01T23:59:50Z", "t1", "cargo test"),
              usage("2026-01-02T00:00:05Z", "new"),
              tool_result("2026-01-02T00:00:10Z", "t1")]
    since = ts("2026-01-02T00:00:00Z")
    r = scanned(events, since)
    assert r["turns"] == 1 and r["fresh"] == 10, r
    assert (r["wall"], r["cargo"], r["cargo_union"]) == (10, 10, 10), r
    assert r["first"] < since and r["win_first"] >= since, r

    # Queued/out-of-order notices: wall spans the earliest and latest timestamp, idle stays sane.
    r = scanned([usage("2026-01-01T00:00:30Z", "m1"),
                 usage("2026-01-01T00:00:10Z", "m2"),
                 usage("2026-01-01T00:00:40Z", "m3")])
    assert r["wall"] == 30 and r["turns"] == 3 and r["idle"] == 0, r

    # A long gap is idle, except for the time a tool was running.
    r = scanned([usage("2026-01-01T00:00:00Z", "m1"),
                 usage("2026-01-01T00:20:00Z", "m2")])
    assert r["idle"] == 1200, r
    r = scanned([usage("2026-01-01T00:00:00Z", "m1"),
                 tool_use("2026-01-01T00:00:00Z", "t1", "cargo test"),
                 tool_result("2026-01-01T00:20:00Z", "t1"),
                 usage("2026-01-01T00:20:00Z", "m2")])
    assert r["idle"] == 0 and r["cargo_union"] == 1200, r

    # Union maths: touching and nested intervals count once.
    assert union_seconds([(0, 10), (10, 20), (5, 15)]) == 20
    assert union_seconds([(30, 40), (0, 10)]) == 20
    assert union_seconds([]) == 0

    # The transcript folder is found for a client checkout and for a workspace root holding it.
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder) / "repo"
        (root / "crates" / "client").mkdir(parents=True)
        home = Path(folder) / "home"
        d = home / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(root))
        d.mkdir(parents=True)
        assert project_dir(root, home) == d, project_dir(root, home)
        workspace = root.parent
        (workspace / "other").mkdir()
        wd = home / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(workspace))
        wd.mkdir(parents=True)
        assert project_dir(workspace / "other", home) == wd, project_dir(workspace / "other", home)

    print("stats self-test passed")


if __name__ == "__main__":
    main()
