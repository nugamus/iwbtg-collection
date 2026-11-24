"""Issue-tracking harness for agents, shared by the client, research and games repos.

  python -I .claude/hooks/issues.py start   SessionStart: inject milestone progress, the next
                                            agent-ready issues with their `## Context` block,
                                            answered needs-user questions and the triage queue.
  python -I .claude/hooks/issues.py stop    Stop: a turn that changed files must end with a
                                            "Findings:" line (issues filed, or "none").
  python -I .claude/hooks/issues.py --test  Self-test of the stop check, settings drift and fidelity checks.

start also warns when the workspace-root .claude/settings.json differs from iwbtg-collection/workspace/settings.json,
and reports the client repo's CI on main, with a loud first line when main is red (#595), and flags open
PRs whose newest Actions run has sat queued over an hour (#666).

The same file lives in all three repos; edit it in the client repo and copy it over.
"""
import json, os, re, subprocess, sys
import hashlib
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

OWNER = "nugamus"
REPOS = {"c": "iwbtg-collection", "r": "iwbtg-collection-research", "g": "iwbtg-collection-games"}


def workspace_of(repo):
    """The folder holding the three checkouts: the parent of a main checkout, or of `worktrees/` for
    a lane worktree (`<workspace>/worktrees/<repo>-<lane>/.claude/hooks/issues.py`)."""
    return repo.parent.parent if repo.parent.name == "worktrees" else repo.parent


REPO = Path(__file__).resolve().parents[2]  # <workspace>/<repo>/.claude/hooks/issues.py
WORKSPACE = workspace_of(REPO)
AGENT_MARK = "<!-- agent -->"  # agents start every issue comment with this, so user replies stand out
EDIT_TOOLS = {"Edit", "Write", "NotebookEdit"}
FINDINGS = re.compile(r"^\W*findings\W*:", re.I | re.M)
# The `## Context` section an agent-ready issue carries (#788), and how much of it `start` prints.
CONTEXT_SECTION = re.compile(r"^## Context[ \t]*\r?\n(.*?)(?=^## |\Z)", re.M | re.S)
CONTEXT_LINES = 6


def context_block(body):
    """The `## Context` section of an issue body, trimmed; `""` when the issue has none."""
    if not body:
        return ""
    match = CONTEXT_SECTION.search(body)
    return match.group(1).strip() if match else ""


def context_lines(block):
    """The compact, indented lines `start` prints under an issue."""
    if not block:
        return ["  (no ## Context block)"]
    lines = [line.strip() for line in block.splitlines() if line.strip()]
    out = [f"  {line}" for line in lines[:CONTEXT_LINES]]
    if len(lines) > CONTEXT_LINES:
        out.append("  … (rest in the issue)")
    return out

FRAGMENT = """
fragment R on Repository {
  milestones(first: 30, states: OPEN) { nodes { title
    open: issues(states: OPEN) { totalCount } closed: issues(states: CLOSED) { totalCount } } }
  ready: issues(first: 100, states: OPEN, labels: ["agent-ready"]) { nodes { number title body
    milestone { title } blockedBy(first: 20) { nodes { state } } subIssuesSummary { total completed }
    labels(first: 20) { nodes { name } } } }
  claimed: issues(first: 50, states: OPEN, labels: ["in-progress"]) { nodes { number title } }
  user: issues(first: 50, states: OPEN, labels: ["needs-user"]) { nodes { number title
    comments(last: 1) { nodes { body } } } }
  triage: issues(first: 20, states: OPEN, labels: ["triage"]) { totalCount nodes { number title } }
  prs: pullRequests(first: 30, states: OPEN) { nodes { number title headRefName isDraft mergeable
    autoMergeRequest { enabledAt } commits(last: 1) { nodes { commit { checkSuites(last: 20) { nodes {
    status createdAt workflowRun { workflow { name } } } } } } } } }
}"""

# CI on the client repo's main (the other repos have no CI): the newest commits and their check runs.
MAIN_CI = """main: repository(owner: "%s", name: "%s") { ref(qualifiedName: "refs/heads/main") { target {
  ... on Commit { history(first: 10) { nodes { oid messageHeadline statusCheckRollup { contexts(first: 50) { nodes {
    __typename ... on CheckRun { name conclusion detailsUrl } ... on StatusContext { context state targetUrl } } } } } } } } } }"""
RED = {"FAILURE", "ERROR", "TIMED_OUT", "STARTUP_FAILURE", "ACTION_REQUIRED"}
GREEN = {"SUCCESS", "SKIPPED", "NEUTRAL"}


def main_ci(main):
    """(top warning, status line) for CI on main. Walks main's newest commits to the first with a verdict:
    a failed check makes it red; all checks passed makes it green; a commit whose run was cancelled (a newer
    push supersedes it: `cancel-in-progress`) or is still running gives none. Bad data: ("", note), never raises."""
    try:
        running = None
        for c in main["ref"]["target"]["history"]["nodes"]:
            checks = ((c.get("statusCheckRollup") or {}).get("contexts") or {}).get("nodes") or []
            if not checks:
                continue
            what = f"{c['oid'][:7]} \"{c['messageHeadline']}\""
            states = [k.get("conclusion") or k.get("state") for k in checks]
            failed = [k for k, st in zip(checks, states) if st in RED]
            # A superseded run: only the aggregate ci-ok reports FAILURE (its needs were cancelled), so no verdict (#741).
            # A real failure next to a cancelled job (fail-fast) stays red.
            if failed and "CANCELLED" in states and all(k.get("name") == "ci-ok" for k in failed):
                continue
            if failed:
                url = re.sub(r"/job/\d+$", "", failed[0].get("detailsUrl") or failed[0].get("targetUrl") or "")
                names = ", ".join(k.get("name") or k.get("context") or "?" for k in failed)
                newer = f" A newer commit is still running: {running}." if running else ""
                return (f"MAIN IS RED: CI failed on {what} (failed: {names}): {url}\n"
                        "Fix main first: find or file the fix issue before merging more PRs; every branch that "
                        f"merges main inherits the break.{newer} Check: `gh run list --branch main --limit 3`.\n\n",
                        f"Main CI: RED on {what} (failed: {names}) {url}" + (f"; {running} still running" if running else ""))
            if all(st in GREEN for st in states):
                return "", f"Main CI: green on {what}" + (f"; {running} still running" if running else "")
            if running is None and any(st in (None, "PENDING", "EXPECTED") for st in states):
                running = what
        return "", "Main CI: no finished run in main's last 10 commits" + (f"; {running} running" if running else "")
    except Exception as e:  # never break the hook over CI data
        return "", f"Main CI: unknown ({type(e).__name__})"


STUCK_AFTER = 3600  # seconds a run may sit queued before the hook calls it stuck


def stuck_ci(pr, now):
    """Whole hours the newest run of some workflow on the PR's head commit has sat QUEUED, if over STUCK_AFTER, else 0.
    Runs started 2026-11-11 stayed queued with no jobs for 16 hours and auto-merge waited (#666). Only Actions
    suites count (other apps' suites stay queued forever); a reopened PR's new run supersedes the stuck one.
    Bad data: 0, never raises."""
    try:
        newest = {}
        for s in pr["commits"]["nodes"][0]["commit"]["checkSuites"]["nodes"]:
            if s.get("workflowRun"):
                name = s["workflowRun"]["workflow"]["name"]
                if name not in newest or s["createdAt"] > newest[name]["createdAt"]:
                    newest[name] = s
        ages = [(now - datetime.fromisoformat(s["createdAt"].replace("Z", "+00:00"))).total_seconds()
                for s in newest.values() if s["status"] == "QUEUED"]
        return max(1, int(max(ages) // 3600)) if ages and max(ages) > STUCK_AFTER else 0
    except Exception:
        return 0


def ms_key(title):
    m = re.match(r"M(\d+)", title or "")
    return int(m.group(1)) if m else 99


def settings_drift(root=WORKSPACE):
    """A warning if the workspace-root settings differ from the tracked copy in the client checkout, else ''."""
    live = root / ".claude" / "settings.json"
    tracked = root / REPOS["c"] / "workspace" / "settings.json"
    if not tracked.is_file():
        return ""
    try:
        same = live.is_file() and json.loads(live.read_text(encoding="utf-8")) == json.loads(tracked.read_text(encoding="utf-8"))
    except ValueError:
        same = False
    if same:
        return ""
    return ("WARNING: the workspace-root .claude/settings.json is missing or differs from the tracked copy "
            f"{REPOS['c']}/workspace/settings.json. Copy the tracked one over (or land your change in the tracked copy "
            f"through a PR), then restart Claude Code. See {REPOS['c']}/docs/dev-setup.md.\n\n")


BASELINE = "crates/tools/fidelity-baseline.txt"
FIDELITY_STAMP = Path("target") / "xtask" / "last-fidelity.txt"  # `cargo xtask fidelity` writes "<origin/main sha> <unix secs>"
GAME_PROGRESS_STAMP = Path("target") / "xtask" / "game-progress.txt"  # `cargo xtask fidelity` writes the per-game `pkg check` counts (#1354)
PORT_PROGRESS_WIDTH = 24


def fidelity_stale(repo=WORKSPACE / REPOS["c"], ref="origin/main"):
    """One line when commits touching crates/sim or crates/format landed on `ref` after the last fidelity sweep
    (#935), else ''. The sweep is the commit in target/xtask/last-fidelity.txt; without it (or when that commit
    is unknown here) the commit that last changed the baseline file stands in. Local git only (no fetch)."""
    def git(*args):
        r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, encoding="utf-8", timeout=10)
        return r.stdout.strip() if r.returncode == 0 else ""
    try:
        since = ""
        try:
            since = (repo / FIDELITY_STAMP).read_text(encoding="utf-8").split()[0]
        except (OSError, IndexError):
            pass
        if not since or not git("rev-parse", "--verify", "-q", since + "^{commit}"):
            since = git("log", "-1", "--format=%H", ref, "--", BASELINE)
        if not since:
            return ""  # no sweep recorded and no baseline on main yet, or no such ref
        n = int(git("rev-list", "--count", f"{since}..{ref}", "--", "crates/sim", "crates/format") or 0)
    except Exception:
        return ""
    return (f"FIDELITY STALE: {n} sim/format commit(s) on main since the last fidelity sweep. "
            "Run `IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask fidelity` (lead, after a merge wave).\n\n") if n else ""


def fidelity_games(repo=REPO):
    """{game: {"OK", "OPEN", "FAIL"} counts} from the public fidelity baseline, or {} without one."""
    counts = {}
    try:
        text = (repo / BASELINE).read_text(encoding="utf-8")
    except OSError:
        return counts
    for line in text.splitlines():
        words = line.split("#", 1)[0].split()
        if len(words) != 2 or words[1] not in ("OK", "OPEN", "FAIL"):
            continue
        game = words[0].split("/", 1)[0]
        counts.setdefault(game, {"OK": 0, "OPEN": 0, "FAIL": 0})[words[1]] += 1
    return counts


def game_progress_stamp(repo=REPO, fallback=None):
    """(missing scripts by game, packages path, packages present, stamp found) from the freshest
    `game-progress.txt`; ({}, None, False, False) when there is none."""
    for root in [repo] + ([fallback] if fallback else []):
        try:
            text = (root / GAME_PROGRESS_STAMP).read_text(encoding="utf-8")
        except OSError:
            continue
        missing, packages, present = {}, None, False
        for line in text.splitlines():
            words = line.split()
            if not words or words[0].startswith("#"):
                continue
            if words[0] == "missing-scripts" and len(words) == 3:
                try:
                    missing[words[1]] = int(words[2])
                except ValueError:
                    pass
            elif words[0] == "missing-scripts-input" and len(words) >= 3:
                present, packages = words[1] == "present", " ".join(words[2:])
        return missing, packages, present, True
    return {}, None, False, False


def game_progress_lines(workspace=WORKSPACE, repo=REPO):
    """Per-game port progress: fidelity from the baseline, missing scripts from the last sweep's
    `pkg check` stamp, and port yes/no from a `<game>/` folder in the games checkout. A value that
    was not measured prints `not measured` and names the missing input; it never prints a zero
    (#1354). Empty when the repo has no baseline."""
    games = fidelity_games(repo)
    if not games:
        return []
    missing, packages, present, found = game_progress_stamp(repo, workspace / REPOS["c"])
    games_root = workspace / REPOS["g"]
    games_found = games_root.is_dir()
    packages = packages or str(workspace / REPOS["r"] / "build" / "packages")
    lines = ["", "Port progress per game (fidelity from crates/tools/fidelity-baseline.txt; "
                 "missing scripts from the last `cargo xtask fidelity` sweep):",
             f"  {'game':<{PORT_PROGRESS_WIDTH}} {'fidelity OK/OPEN/FAIL':<21} {'missing scripts':>15}  port"]
    total = {"OK": 0, "OPEN": 0, "FAIL": 0}
    missing_total, ports = 0, 0
    for game in sorted(games):
        c = games[game]
        for status in total:
            total[status] += c[status]
        if game in missing:
            miss, missing_total = str(missing[game]), missing_total + missing[game]
        else:
            miss = "not measured"
        if games_found:
            port = "yes" if (games_root / game).is_dir() else "no"
            ports += port == "yes"
        else:
            port = "not measured"
        fidelity = f"{c['OK']}/{c['OPEN']}/{c['FAIL']}"
        lines.append(f"  {game:<{PORT_PROGRESS_WIDTH}} {fidelity:<21} {miss:>15}  {port}")
    fidelity_total = f"{total['OK']}/{total['OPEN']}/{total['FAIL']}"
    missing_all = all(game in missing for game in games)
    port_total = f"{ports}/{len(games)}" if games_found else "not measured"
    lines.append(f"  {'total':<{PORT_PROGRESS_WIDTH}} {fidelity_total:<21} "
                 f"{str(missing_total) if missing_all else 'not measured':>15}  {port_total}")
    # Name the missing inputs once, instead of on every row.
    if not missing_all:
        if not found:
            lines.append(f"  missing scripts: not measured (no fidelity sweep stamp; run `cargo xtask fidelity`, "
                         f"input {packages})")
        elif not present:
            lines.append(f"  missing scripts: not measured (no packages at {packages})")
        else:
            lines.append(f"  missing scripts: not measured (no per-package counts recorded for {packages})")
    if not games_found:
        lines.append(f"  ports: not measured (no games checkout at {games_root})")
    return lines


def worktrees(root=WORKSPACE):
    """One line per extra git worktree of each repo: folder, branch, uncommitted files."""
    out = []
    for repo in REPOS.values():
        try:
            r = subprocess.run(["git", "-C", str(root / repo), "worktree", "list", "--porcelain"],
                               capture_output=True, text=True, encoding="utf-8", timeout=10)
        except Exception:
            continue
        for block in r.stdout.strip().split("\n\n")[1:]:  # the first is the main checkout
            f = dict(l.split(" ", 1) if " " in l else (l, "") for l in block.splitlines())
            path = Path(f.get("worktree", ""))
            branch = f.get("branch", "").removeprefix("refs/heads/") or "(detached)"
            try:
                dirty = subprocess.run(["git", "-C", str(path), "status", "--porcelain"], capture_output=True,
                                       text=True, encoding="utf-8", timeout=10).stdout.count("\n")
            except Exception:
                dirty = 0
            out.append(f"- worktrees/{path.name}: {branch}" + (f", {dirty} uncommitted file(s)" if dirty else ""))
    return out


STATUS_TTL = 120  # seconds; claim/merge decisions still re-read the specific issue/PR
STATUS_CACHE = Path(__file__).resolve().parents[2] / "target" / "agent-status.json"


def github_status(query, cache=STATUS_CACHE, refresh=False):
    """Reuse only successful, matching snapshots. Cache errors never block a session."""
    key = hashlib.sha256(query.encode()).hexdigest()
    if not refresh:
        try:
            saved = json.loads(cache.read_text(encoding="utf-8"))
            age = time.time() - saved["at"]
            if saved["key"] == key and 0 <= age < STATUS_TTL:
                return saved["data"], int(age)
        except (OSError, ValueError, KeyError, TypeError):
            pass
    r = subprocess.run(["gh", "api", "graphql", "-f", f"query={query}"],
                       capture_output=True, text=True, encoding="utf-8", timeout=30)
    payload = json.loads(r.stdout)
    if r.returncode or payload.get("errors") or not payload.get("data"):
        raise ValueError("GitHub status query failed")
    data = payload["data"]
    temp = None
    try:
        cache.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=cache.parent, delete=False) as f:
            temp = Path(f.name)  # private permissions; replace atomically for concurrent sessions
            json.dump({"key": key, "at": time.time(), "data": data}, f)
        os.replace(temp, cache)
    except OSError:
        pass
    finally:
        if temp is not None:
            try:
                temp.unlink(missing_ok=True)
            except OSError:
                pass
    return data, 0


def start():
    warn = settings_drift()
    q = ("query {" + " ".join(f'{k}: repository(owner: "{OWNER}", name: "{v}") {{ ...R }}' for k, v in REPOS.items())
         + " " + MAIN_CI % (OWNER, REPOS["c"]) + "}" + FRAGMENT)
    try:
        data, age = github_status(q, refresh="--refresh" in sys.argv)
        # Do not mutate the cached data's shape while rendering.
        data = dict(data)
        red, ci_line = main_ci(data.pop("main", None))
    except Exception as e:  # offline or gh missing: say so, never block the session
        return emit(warn + f"Issue harness: could not reach GitHub ({type(e).__name__}). Run `gh auth status`; work from the issues once it is back.")

    ms, ready, answered, waiting, triage, claimed, prs, stuck = {}, [], [], 0, [], [], [], []
    now = datetime.now(timezone.utc)
    for k, repo in data.items():
        name = REPOS[k]
        for m in repo["milestones"]["nodes"]:
            o, c = ms.get(m["title"], (0, 0))
            ms[m["title"]] = (o + m["open"]["totalCount"], c + m["closed"]["totalCount"])
        for i in repo["ready"]["nodes"]:
            s = i["subIssuesSummary"]
            if "in-progress" in [l["name"] for l in i["labels"]["nodes"]]:
                continue  # another agent has claimed it
            if i["milestone"] and all(b["state"] == "CLOSED" for b in i["blockedBy"]["nodes"]) and s["total"] == s["completed"]:
                ready.append((ms_key(i["milestone"]["title"]), name != REPOS["c"], i["number"],
                              f"{name}#{i['number']} {i['title']} [{i['milestone']['title']}]",
                              context_block(i.get("body"))))
        for i in repo["user"]["nodes"]:
            last = i["comments"]["nodes"]
            if last and AGENT_MARK not in last[0]["body"]:
                answered.append(f"{name}#{i['number']} {i['title']}")
            else:
                waiting += 1
        triage += [f"{name}#{i['number']} {i['title']}" for i in repo["triage"]["nodes"]]
        claimed += [f"{name}#{i['number']} {i['title']}" for i in repo["claimed"]["nodes"]]
        for p in repo["prs"]["nodes"]:
            hours = stuck_ci(p, now)
            if hours:
                n, r = p["number"], f"{OWNER}/{name}"
                again = f" && gh pr merge {n} -R {r} --squash --auto" if p["autoMergeRequest"] else ""
                stuck.append(f"- `gh pr close {n} -R {r} && gh pr reopen {n} -R {r}{again}`")
            notes = [n for n, on in (("draft", p["isDraft"]), ("CONFLICTS with main", p["mergeable"] == "CONFLICTING"),
                                     ("auto-merge on", p["autoMergeRequest"]), (f"CI STUCK: queued {hours}h", hours)) if on]
            prs.append(f"{name}#{p['number']} {p['title']} ({p['headRefName']}" + "".join(f"; {n}" for n in notes) + ")")

    lines = [f"# Project status (GitHub snapshot, {age}s old; --refresh bypasses cache)", "", ci_line, "", "Milestones (closed/total across repos):"]
    for t in sorted(ms, key=lambda t: (ms_key(t), t)):
        o, c = ms[t]
        lines.append(f"- {t}: {c}/{o + c} ({100 * c // max(o + c, 1)}%)")
    lines += game_progress_lines()
    if answered:
        lines += ["", "The user answered these needs-user issues. Read the reply, act on it, remove `needs-user`:"] + [f"- {a}" for a in answered]
    if triage:
        lines += ["", f"Triage queue ({len(triage)}): give each a milestone, size, area, `blocked by` links, then remove `triage`:"] + [f"- {t}" for t in triage]
    if claimed:
        lines += ["", "In progress (claimed by an agent; do not start these):"] + [f"- {c}" for c in claimed]
    if prs:
        lines += ["", "Open PRs:"] + [f"- {p}" for p in prs]
    if stuck:
        lines += ["", "CI STUCK PRs: close and reopen starts fresh runs; reopening turns auto-merge off, so the command turns "
                  "it on again where it was on (cancel and rerun refuse such runs; client/docs/dev-setup.md, CI):"] + stuck
    wt = worktrees()
    if wt:
        lines += ["", "Worktrees (lanes keep theirs between issues):"] + wt
    lines += ["", "Next unblocked agent-ready issues (unclaimed), with their Context:"]
    for r in sorted(ready)[:10]:
        lines.append(f"- {r[3]}")
        lines += context_lines(r[4])
    lines += ["", f"{waiting} needs-user issue(s) still wait for the user.",
              "Rules: CLAUDE.md, Issues. Unless the user names other work, offer to start the first issue above."]
    emit(red + fidelity_stale() + warn + "\n".join(lines))


def emit(text):
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": text}}))


def turn_needs_findings(entries):
    """True if the last turn edited files and its final text has no Findings line."""
    edited, last_text = False, ""
    for e in entries:
        msg = e.get("message") or {}
        content = msg.get("content")
        if e.get("type") == "user":
            is_prompt = isinstance(content, str) or any(c.get("type") == "text" for c in content or [] if isinstance(c, dict))
            if is_prompt:
                edited, last_text = False, ""
        elif e.get("type") == "assistant" and isinstance(content, list):
            for c in content:
                if c.get("type") == "tool_use" and c.get("name") in EDIT_TOOLS:
                    edited = True
                elif c.get("type") == "text":
                    last_text = c.get("text", "")
    return edited and not FINDINGS.search(last_text)


def stop_needs_findings(inp, entries):
    """True if the Stop hook must block. The hook input's `last_assistant_message` is the reply being
    delivered; the transcript file can still lack it when the hook runs, so a Findings line there passes."""
    last = inp.get("last_assistant_message")
    if isinstance(last, str) and FINDINGS.search(last):
        return False
    return turn_needs_findings(entries)


def stop():
    inp = json.load(sys.stdin)
    if inp.get("stop_hook_active"):
        return
    # The final reply is authoritative; avoid parsing an entire session transcript on success.
    last = inp.get("last_assistant_message")
    if isinstance(last, str) and FINDINGS.search(last):
        return
    try:
        with open(inp["transcript_path"], encoding="utf-8") as f:
            entries = [json.loads(l) for l in f if l.strip()]
    except (OSError, KeyError, ValueError):
        entries = []  # unreadable transcript: decide from the hook input alone
    if os.environ.get("IWBTG_HOOK_LOG"):  # debugging aid for #219: what the hook saw
        with open(os.environ["IWBTG_HOOK_LOG"], "a", encoding="utf-8") as log:
            log.write(json.dumps({"keys": sorted(inp), "last": str(inp.get("last_assistant_message"))[-200:],
                                  "entries": len(entries), "needs": stop_needs_findings(inp, entries)}) + "\n")
    if stop_needs_findings(inp, entries):
        print(json.dumps({"decision": "block", "reason": (
            "Before you stop: file a GitHub issue for every bug, gap, missing test, doc drift, idea or follow-up "
            "you noticed this turn that you did not finish (`gh issue create --label triage`, with Goal and Done when; "
            "search first so you do not file duplicates). Then end your reply with one line: "
            "`Findings: <issue links>` or `Findings: none`.")}))


def test():
    def a(*parts):
        return {"type": "assistant", "message": {"content": list(parts)}}
    prompt = {"type": "user", "message": {"content": "do it"}}
    edit = {"type": "tool_use", "name": "Edit"}
    assert turn_needs_findings([prompt, a(edit), a({"type": "text", "text": "Done."})])
    assert not turn_needs_findings([prompt, a(edit), a({"type": "text", "text": "Done.\n\nFindings: none"})])
    assert not turn_needs_findings([prompt, a(edit), a({"type": "text", "text": "**Findings:** #12"})])
    assert not turn_needs_findings([prompt, a({"type": "text", "text": "Just talking."})])
    assert not turn_needs_findings([prompt, a(edit), prompt, a({"type": "text", "text": "ok"})])
    # The transcript may not hold the final reply yet (#219): the hook input's last message decides.
    stale = [prompt, a(edit), a({"type": "text", "text": "Working on it."})]
    assert stop_needs_findings({}, stale)
    assert stop_needs_findings({"last_assistant_message": "Done."}, stale)
    assert not stop_needs_findings({"last_assistant_message": "Done.\n\nFindings: none"}, stale)
    assert not stop_needs_findings({"last_assistant_message": "Done.\n\nFindings: none"}, [])
    assert stop_needs_findings({"last_assistant_message": 5}, stale)
    import tempfile
    # The workspace root is one level above a main checkout, two above a lane worktree (#1346).
    assert workspace_of(Path("/w") / REPOS["c"]) == Path("/w")
    assert workspace_of(Path("/w/worktrees") / f"{REPOS['c']}-data") == Path("/w")
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        assert settings_drift(root) == ""  # no tracked copy: nothing to compare
        (root / REPOS["c"] / "workspace").mkdir(parents=True)
        (root / REPOS["c"] / "workspace" / "settings.json").write_text('{"a": 1}')
        assert settings_drift(root).startswith("WARNING")  # root copy missing
        (root / ".claude").mkdir()
        (root / ".claude" / "settings.json").write_text('{ "a" : 1 }')
        assert settings_drift(root) == ""  # same JSON, different formatting
        (root / ".claude" / "settings.json").write_text('{"a": 2}')
        assert settings_drift(root).startswith("WARNING")
    # Port progress (#1354): fidelity from the baseline, missing scripts from the sweep stamp,
    # port presence from a `<game>/` folder; an absent input prints `not measured`, never a zero.
    with tempfile.TemporaryDirectory() as d:
        ws = Path(d)
        repo = ws / REPOS["c"]
        (repo / "crates/tools").mkdir(parents=True)
        (repo / BASELINE).write_text(
            "kaenbyou/a OK\nkaenbyou/b OPEN\nkaenbyou/c FAIL\nanimus/x OK\n", encoding="utf-8")
        (ws / REPOS["g"] / "kaenbyou").mkdir(parents=True)
        text = "\n".join(game_progress_lines(ws, repo))
        assert "kaenbyou" in text and "animus" in text and "total" in text
        assert "1/1/1" in text and "1/0/0" in text and "1/2" in text  # ports: kaenbyou only
        assert "not measured (no fidelity sweep stamp" in text
        (repo / GAME_PROGRESS_STAMP).parent.mkdir(parents=True)
        (repo / GAME_PROGRESS_STAMP).write_text(
            "missing-scripts-input absent /nope/build/packages\nmissing-scripts kaenbyou 12\n",
            encoding="utf-8")
        text = "\n".join(game_progress_lines(ws, repo))
        assert "12" in text and "not measured" in text  # animus was not measured
        assert "no packages at /nope/build/packages" in text
        (repo / BASELINE).unlink()
        assert game_progress_lines(ws, repo) == []
        # The stamp parser reads a measured packages input and a zero it did measure.
        (repo / BASELINE).write_text("a/x OK\n", encoding="utf-8")
        (repo / GAME_PROGRESS_STAMP).write_text(
            "missing-scripts-input present /p\nmissing-scripts a 0\n", encoding="utf-8")
        assert game_progress_stamp(repo)[:3] == ({"a": 0}, "/p", True)
    from unittest.mock import patch
    from types import SimpleNamespace
    with tempfile.TemporaryDirectory() as d:
        cache = Path(d) / "status.json"
        response = SimpleNamespace(returncode=0, stdout=json.dumps({"data": {"c": {"ok": True}}}))
        with patch("subprocess.run", return_value=response) as run:
            assert github_status("query", cache)[0] == {"c": {"ok": True}}
            assert github_status("query", cache)[0] == {"c": {"ok": True}}
            assert run.call_count == 1
            github_status("query", cache, refresh=True)
            assert run.call_count == 2
            github_status("changed query", cache)
            assert run.call_count == 3
            with patch("time.time", return_value=time.time() + STATUS_TTL + 1):
                github_status("changed query", cache)
            assert run.call_count == 4
        cache.write_text("broken", encoding="utf-8")
        with patch("subprocess.run", return_value=response) as run:
            github_status("query", cache)
            assert run.call_count == 1
        with patch("subprocess.run", return_value=SimpleNamespace(returncode=1, stdout='{"errors": ["offline"]}')):
            try:
                github_status("query", cache, refresh=True)
                assert False, "failed responses must not become valid snapshots"
            except ValueError:
                pass
        # Cached successes remain usable after a failed refresh; issue creation/Stop are unaffected.
        with patch("subprocess.run", side_effect=AssertionError("cache should be reused")):
            assert github_status("query", cache)[0]["c"]["ok"]
    # #788: an agent-ready issue's `## Context` is printed under its line, compactly; an issue
    # without one says so instead of printing the whole body.
    body = ("## Goal\n\nx\n\n## Context\n\n- Code: `a.rs`\n- Test: `cargo test`\n\n"
            "## Done when\n\n- [ ] x\n")
    assert context_block(body) == "- Code: `a.rs`\n- Test: `cargo test`"
    assert context_block("## Goal\n\nx\n## Done when\n") == ""
    assert context_block("") == "" and context_block(None) == ""
    assert context_block("## Contextual\n- not this\n") == ""
    assert context_block("## Context\n") == ""
    assert context_lines(context_block(body)) == ["  - Code: `a.rs`", "  - Test: `cargo test`"]
    assert context_lines("") == ["  (no ## Context block)"]
    many = "\n".join(f"- line {i}" for i in range(CONTEXT_LINES + 2))
    assert context_lines(many)[-1] == "  … (rest in the issue)"
    assert len(context_lines(many)) == CONTEXT_LINES + 1
    # Main CI (#595): a failed check is red and loud; cancelled or running commits defer to older ones.
    def commit(oid, *checks):
        return {"oid": oid * 40, "messageHeadline": f"PR {oid}", "statusCheckRollup": {"contexts": {"nodes": list(checks)}}}

    def run(name, conclusion):
        return {"__typename": "CheckRun", "name": name, "conclusion": conclusion,
                "detailsUrl": "https://github.com/o/r/actions/runs/7/job/1"}

    def ci(*commits):
        return main_ci({"ref": {"target": {"history": {"nodes": list(commits)}}}})
    red, line = ci(commit("b", run("ci-ok", None)), commit("a", run("changes", "SUCCESS"), run("ubuntu-latest", "FAILURE")))
    assert red.startswith('MAIN IS RED: CI failed on aaaaaaa "PR a" (failed: ubuntu-latest): https://github.com/o/r/actions/runs/7\n'), red
    assert "bbbbbbb" in red and line.startswith("Main CI: RED on aaaaaaa"), (red, line)
    assert ci(commit("b", run("x", "CANCELLED")), commit("a", run("x", "SUCCESS"), run("y", "SKIPPED"))) == ("", 'Main CI: green on aaaaaaa "PR a"')
    assert ci(commit("a", run("x", "CANCELLED")))[1].startswith("Main CI: no finished run")
    # #741: a cancelled run's ci-ok job reports FAILURE; the commit has no verdict, so the older one decides.
    assert ci(commit("b", run("windows-latest", "CANCELLED"), run("ci-ok", "FAILURE"), run("x", "SUCCESS")), commit("a", run("x", "SUCCESS"))) == ("", 'Main CI: green on aaaaaaa "PR a"')
    assert ci(commit("b", run("windows-latest", "CANCELLED"), run("ci-ok", "FAILURE")))[0] == ""
    red, _ = ci(commit("b", run("ubuntu-latest", "FAILURE"), run("windows-latest", "CANCELLED"), run("ci-ok", "FAILURE")), commit("a", run("x", "SUCCESS")))
    assert red.startswith("MAIN IS RED: CI failed on bbbbbbb") and "ubuntu-latest" in red, red  # fail-fast cancel
    assert ci({"oid": "c" * 40, "messageHeadline": "no checks", "statusCheckRollup": None}, commit("a", run("x", "FAILURE")))[0].startswith("MAIN IS RED")
    assert main_ci(None) == ("", "Main CI: unknown (TypeError)")  # query field missing
    assert ci({"oid": 5, "messageHeadline": "x", "statusCheckRollup": {"contexts": {"nodes": [run("x", "SUCCESS")]}}}) == ("", "Main CI: unknown (TypeError)")
    # Stuck PR CI (#666): the newest Actions run per workflow, queued over an hour; other apps' suites don't count.
    now = datetime(2026, 11, 12, 12, tzinfo=timezone.utc)

    def pr(*suites):
        return {"commits": {"nodes": [{"commit": {"checkSuites": {"nodes": list(suites)}}}]}}

    def suite(status, at, wf="CI"):
        return {"status": status, "createdAt": f"2026-11-12T{at}:00Z", "workflowRun": wf and {"workflow": {"name": wf}}}
    assert stuck_ci(pr(suite("QUEUED", "02:30"), suite("COMPLETED", "02:30", "PR checks")), now) == 9
    assert stuck_ci(pr(suite("QUEUED", "10:50")), now) == 1
    assert stuck_ci(pr(suite("QUEUED", "11:30"), suite("IN_PROGRESS", "11:30", "PR checks")), now) == 0  # just queued
    assert stuck_ci(pr(suite("QUEUED", "01:00", None)), now) == 0  # another app's suite, never runs
    assert stuck_ci(pr(suite("QUEUED", "01:00"), suite("IN_PROGRESS", "11:00")), now) == 0  # reopened: a newer run
    assert stuck_ci({"commits": {"nodes": []}}, now) == 0 and stuck_ci({}, now) == 0
    # PR #665 as GitHub returned it: the claude app's suite has no workflowRun; runs queued 2026-11-11 20:40.
    real = [{"status": "QUEUED", "createdAt": "2026-11-11T20:40:55Z", "workflowRun": None},
            {"status": "QUEUED", "createdAt": "2026-11-11T20:40:59Z", "workflowRun": {"workflow": {"name": "PR checks"}}},
            {"status": "QUEUED", "createdAt": "2026-11-11T20:40:59Z", "workflowRun": {"workflow": {"name": "CI"}}}]
    assert stuck_ci(pr(*real), now) == 15
    assert stuck_ci(pr(*real, suite("COMPLETED", "11:03", "PR checks"), suite("COMPLETED", "11:03")), now) == 0  # after close/reopen
    # Fidelity baseline (#935): sim/format commits on main after the baseline's last change.
    with tempfile.TemporaryDirectory() as d:
        repo = Path(d)

        def g(*args):
            subprocess.run(["git", "-C", d, "-c", "user.name=t", "-c", "user.email=t@t", *args], check=True, capture_output=True)

        def commit(path):
            f = repo / path
            f.parent.mkdir(parents=True, exist_ok=True)
            f.write_text(f.read_text() + "x" if f.exists() else "x")
            g("add", "."); g("commit", "-m", path)
        g("init", "-b", "main")
        commit("crates/sim/a.rs")
        assert fidelity_stale(repo, "main") == ""  # no baseline yet
        commit(BASELINE)
        assert fidelity_stale(repo, "main") == ""  # sweep is current
        commit("crates/ui/b.rs")
        assert fidelity_stale(repo, "main") == ""  # unrelated change
        commit("crates/sim/a.rs"); commit("crates/format/c.rs")
        out = fidelity_stale(repo, "main")
        assert out.startswith("FIDELITY STALE: 2 sim/format") and "cargo xtask fidelity" in out, out
        commit(BASELINE)
        assert fidelity_stale(repo, "main") == ""  # re-blessed
        # A recorded sweep (cargo xtask fidelity, pass or fail) replaces the baseline commit as the reference.
        commit("crates/sim/a.rs")
        assert fidelity_stale(repo, "main").startswith("FIDELITY STALE: 1 ")
        (repo / "target" / "xtask").mkdir(parents=True)
        stamp = repo / FIDELITY_STAMP
        stamp.write_text(subprocess.run(["git", "-C", d, "rev-parse", "main"], capture_output=True, text=True).stdout.strip() + " 1\n")
        assert fidelity_stale(repo, "main") == ""  # swept at main's tip, baseline unchanged
        commit("crates/format/c.rs")
        assert fidelity_stale(repo, "main").startswith("FIDELITY STALE: 1 ")
        stamp.write_text("0" * 40 + " 1\n")
        assert fidelity_stale(repo, "main").startswith("FIDELITY STALE: 2 ")  # unknown commit: the baseline commit stands in
        stamp.unlink()
        assert fidelity_stale(repo, "nope") == ""  # bad ref
    print("ok")


if __name__ == "__main__":
    {"start": start, "stop": stop, "--test": test}[sys.argv[1]]()
