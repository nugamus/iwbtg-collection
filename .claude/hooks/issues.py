"""Issue-tracking harness for agents, shared by the client, research and games repos.

  python -I .claude/hooks/issues.py start   SessionStart: inject milestone progress, next issues,
                                            answered needs-user questions and the triage queue.
  python -I .claude/hooks/issues.py stop    Stop: a turn that changed files must end with a
                                            "Findings:" line (issues filed, or "none").
  python -I .claude/hooks/issues.py --test  Self-test of the stop check.

The same file lives in all three repos; edit it in the client repo and copy it over.
"""
import json, re, subprocess, sys

OWNER = "nugamus"
REPOS = {"c": "iwbtg-collection", "r": "iwbtg-collection-research", "g": "iwbtg-collection-games"}
AGENT_MARK = "<!-- agent -->"  # agents start every issue comment with this, so user replies stand out
EDIT_TOOLS = {"Edit", "Write", "NotebookEdit"}
FINDINGS = re.compile(r"^\W*findings\W*:", re.I | re.M)

FRAGMENT = """
fragment R on Repository {
  milestones(first: 30, states: OPEN) { nodes { title
    open: issues(states: OPEN) { totalCount } closed: issues(states: CLOSED) { totalCount } } }
  ready: issues(first: 100, states: OPEN, labels: ["agent-ready"]) { nodes { number title
    milestone { title } blockedBy(first: 20) { nodes { state } } subIssuesSummary { total completed } } }
  user: issues(first: 50, states: OPEN, labels: ["needs-user"]) { nodes { number title
    comments(last: 1) { nodes { body } } } }
  triage: issues(first: 20, states: OPEN, labels: ["triage"]) { totalCount nodes { number title } }
}"""


def ms_key(title):
    m = re.match(r"M(\d+)", title or "")
    return int(m.group(1)) if m else 99


def start():
    q = "query {" + " ".join(f'{k}: repository(owner: "{OWNER}", name: "{v}") {{ ...R }}' for k, v in REPOS.items()) + "}" + FRAGMENT
    try:
        r = subprocess.run(["gh", "api", "graphql", "-f", f"query={q}"], capture_output=True, text=True, encoding="utf-8", timeout=30)
        data = json.loads(r.stdout)["data"]
    except Exception as e:  # offline or gh missing: say so, never block the session
        return emit(f"Issue harness: could not reach GitHub ({type(e).__name__}). Run `gh auth status`; work from the issues once it is back.")

    ms, ready, answered, waiting, triage = {}, [], [], 0, []
    for k, repo in data.items():
        name = REPOS[k]
        for m in repo["milestones"]["nodes"]:
            o, c = ms.get(m["title"], (0, 0))
            ms[m["title"]] = (o + m["open"]["totalCount"], c + m["closed"]["totalCount"])
        for i in repo["ready"]["nodes"]:
            s = i["subIssuesSummary"]
            if i["milestone"] and all(b["state"] == "CLOSED" for b in i["blockedBy"]["nodes"]) and s["total"] == s["completed"]:
                ready.append((ms_key(i["milestone"]["title"]), name != REPOS["c"], i["number"], f"{name}#{i['number']} {i['title']} [{i['milestone']['title']}]"))
        for i in repo["user"]["nodes"]:
            last = i["comments"]["nodes"]
            if last and AGENT_MARK not in last[0]["body"]:
                answered.append(f"{name}#{i['number']} {i['title']}")
            else:
                waiting += 1
        triage += [f"{name}#{i['number']} {i['title']}" for i in repo["triage"]["nodes"]]

    lines = ["# Project status (from GitHub issues)", "", "Milestones (closed/total across repos):"]
    for t in sorted(ms, key=lambda t: (ms_key(t), t)):
        o, c = ms[t]
        lines.append(f"- {t}: {c}/{o + c} ({100 * c // max(o + c, 1)}%)")
    if answered:
        lines += ["", "The user answered these needs-user issues. Read the reply, act on it, remove `needs-user`:"] + [f"- {a}" for a in answered]
    if triage:
        lines += ["", f"Triage queue ({len(triage)}): give each a milestone, size, area, `blocked by` links, then remove `triage`:"] + [f"- {t}" for t in triage]
    lines += ["", "Next unblocked agent-ready issues:"] + [f"- {r[3]}" for r in sorted(ready)[:5]]
    lines += ["", f"{waiting} needs-user issue(s) still wait for the user.",
              "Rules: CLAUDE.md, Issues. Unless the user names other work, offer to start the first issue above."]
    emit("\n".join(lines))


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


def stop():
    inp = json.load(sys.stdin)
    if inp.get("stop_hook_active"):
        return
    try:
        with open(inp["transcript_path"], encoding="utf-8") as f:
            entries = [json.loads(l) for l in f if l.strip()]
    except (OSError, KeyError, ValueError):
        return
    if turn_needs_findings(entries):
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
    print("ok")


if __name__ == "__main__":
    {"start": start, "stop": stop, "--test": test}[sys.argv[1]]()
