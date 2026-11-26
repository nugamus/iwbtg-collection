#!/usr/bin/env python3
"""Build the object-coverage and progress dashboard.

    python -I progress/export_research.py --research ../iwbtg-collection-research
    python -I progress/generate.py --gh --history

`export_research.py` writes `progress/exporter.json` with real per-game object
coverage from the research checkout. `generate.py` merges that with the committed
front-matter and, with `--gh`, the live phase/issue/port data.

Object coverage is real today:
  * placed instances and built-in coverage from specs/mechanics/coverage.md;
  * custom objects from shortlist.md;
  * recorded traces and the object names they exercise from the trace CSVs.

The per-game implementation bars read repo files at build time:
  * trace cases from crates/tools/cases/<game>/*.toml, status from
    crates/tools/fidelity-baseline.txt (OK / OPEN / FAIL);
  * package objects mapped from the committed progress/exporter.json snapshot;
  * the script API census from docs/architecture/sim/scripts.md and the
    generated list in docs/format/script-api.md.

Passing-trace coverage of individual objects still needs the trace runner; the
page says so once, with the follow-up, instead of filling the table with dashes.
"""

import argparse
import datetime as dt
import html
import json
import os
import re
import subprocess
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROGRESS = os.path.join(ROOT, "progress")
GAMES = os.path.join(ROOT, "docs", "games")
CASES = os.path.join(ROOT, "crates", "tools", "cases")
BASELINE = os.path.join(ROOT, "crates", "tools", "fidelity-baseline.txt")
SCRIPTS_DOC = os.path.join(ROOT, "docs", "architecture", "sim", "scripts.md")
SCRIPT_API_DOC = os.path.join(ROOT, "docs", "format", "script-api.md")
REPO = "nugamus/iwbtg-collection"
GAMES_REPO = "nugamus/iwbtg-collection-games"
SIZE_POINTS = {"size:S": 1, "size:M": 3, "size:L": 8}
FOLLOWUPS = {"census": 1364, "traces": 1365}
PROJECT_STATUS = {
    "label": "Pre-release",
    "detail": "No release builds yet. The public alpha is planned for phase 7 (Online).",
}


def fmt(n):
    return f"{n:,}" if isinstance(n, int) else ("" if n is None else str(n))


def pctf(v, digits=1):
    return f"{v:.{digits}f}%" if v is not None else "—"


def read_front_matter(path):
    text = open(path, encoding="utf-8").read()
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end == -1:
        return {}
    data = {}
    for line in text[3:end].splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        key, value = key.strip(), value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        if value in ("null", "~", ""):
            data[key] = None
        elif value == "unknown":
            data[key] = "unknown"
        else:
            data[key] = value
    return data


def as_int(value):
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    if isinstance(value, str) and re.fullmatch(r"-?\d+", value.strip()):
        return int(value)
    return None


def as_float(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def load_trace_cases():
    """Count trace cases per case directory and their blessed status.

    The denominator is the committed `crates/tools/cases/<dir>/*.toml` files;
    the numerator is the OK lines in `crates/tools/fidelity-baseline.txt`.
    Both live in this repo, so a bar moves when a merged case or status lands.
    """
    if not os.path.isdir(CASES):
        return {}
    status = {}
    if os.path.exists(BASELINE):
        for line in open(BASELINE, encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) != 2 or "/" not in parts[0]:
                continue
            game, case = parts[0].split("/", 1)
            status[(game, case)] = parts[1]
    out = {}
    for name in sorted(os.listdir(CASES)):
        path = os.path.join(CASES, name)
        if not os.path.isdir(path):
            continue
        cases = sorted(f[:-5] for f in os.listdir(path) if f.endswith(".toml"))
        known = {case: status[(name, case)] for case in cases if (name, case) in status}
        counts = Counter(known.values())
        out[name] = {
            "dir": name,
            "recorded": len(cases),
            "ok": counts.get("OK", 0),
            "open": counts.get("OPEN", 0),
            "fail": counts.get("FAIL", 0),
            "other": sum(v for key, v in counts.items() if key not in ("OK", "OPEN", "FAIL")),
            "unclassified": len(cases) - len(known),
            "pass_pct": round(100.0 * counts.get("OK", 0) / len(cases), 1) if cases else None,
        }
    return out


def case_dir_for(slug, dirs):
    """Map a game slug to its case directory without guessing across games."""
    normal = slug.replace("-", "_")
    if normal in dirs:
        return normal
    # rely-the-needle-4-2 -> rely_the_needle; only a unique prefix match counts.
    matches = [d for d in dirs if normal.startswith(d + "_")]
    return matches[0] if len(matches) == 1 else None


def load_script_api():
    """Read the generated API list and the census groups not yet registered."""
    info = {
        "registered_functions": None,
        "planned_groups": [],
        "planned_count": None,
        "missing": "the census's full group list (research#172) is not in this repo",
    }
    if os.path.exists(SCRIPT_API_DOC):
        text = open(SCRIPT_API_DOC, encoding="utf-8").read()
        section = re.search(r"\n## Functions\n(.*?)(?=\n## |\Z)", text, re.S)
        if section:
            names = set()
            for line in section.group(1).splitlines():
                hit = re.match(r"- `([A-Za-z_][A-Za-z0-9_$]*)\s*\(", line)
                if hit:
                    names.add(hit.group(1))
            info["registered_functions"] = len(names)
    if os.path.exists(SCRIPTS_DOC):
        text = open(SCRIPTS_DOC, encoding="utf-8").read()
        section = re.search(r"### Census groups not yet registered\n(.*?)(?=\n### |\n## |\Z)",
                            text, re.S)
        if section:
            for line in section.group(1).splitlines():
                if not line.startswith("|"):
                    continue
                cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
                if (len(cells) < 3 or cells[0].startswith("Census")
                        or set(cells[0]) <= {"-", ":", " "}):
                    continue
                # The table keeps a landed group for the record; a status that
                # starts "Registered" is no longer work to do (paths, #1530).
                status = re.sub(r"[*_`]", "", cells[2]).strip()
                if status.lower().startswith("registered"):
                    continue
                issue = re.search(r"#(\d+)", cells[2])
                info["planned_groups"].append({
                    "name": cells[0],
                    "counts": cells[1],
                    "issue": int(issue.group(1)) if issue else None,
                })
            info["planned_count"] = len(info["planned_groups"])
    return info


def load_games():
    exporter = {}
    exporter_meta = {"generated_at": None, "source": None}
    path = os.path.join(PROGRESS, "exporter.json")
    if os.path.exists(path):
        raw = json.load(open(path, encoding="utf-8"))
        if isinstance(raw, dict):
            exporter = raw.get("games", {}) or {}
            exporter_meta = {"generated_at": raw.get("generated_at"), "source": raw.get("source")}
    trace_cases = load_trace_cases()
    case_dirs = set(trace_cases)
    games = []
    for name in sorted(os.listdir(GAMES)):
        if not name.endswith(".md") or name == "README.md":
            continue
        fm = read_front_matter(os.path.join(GAMES, name))
        slug = fm.get("slug") or name[:-3]
        ex = exporter.get(slug, {}) or {}
        placed = as_int(ex.get("placed_instances"))
        builtin = as_int(ex.get("builtin_instances"))
        in_package = as_int(ex.get("objects_in_package"))
        unmapped = as_int(ex.get("objects_unmapped"))
        total_objects = as_int(fm.get("objects_total"))
        if total_objects is None:
            total_objects = in_package
        specced = as_int(fm.get("objects_specced"))
        if specced is None:
            specced = (in_package - unmapped
                       if in_package is not None and unmapped is not None else None)
        scripted = as_int(fm.get("objects_scripted"))
        if scripted is None:
            scripted = unmapped
        case_dir = case_dir_for(slug, case_dirs)
        rec = {
            "id": fm.get("id") or slug,
            "slug": slug,
            "order": as_int(fm.get("order")) or 999,
            "name": fm.get("name") or slug,
            "source": fm.get("source"),
            "engine": ex.get("engine") or fm.get("engine") or "unknown",
            "genre": ex.get("genre"),
            "difficulty": as_float(ex.get("difficulty")),
            "lifecycle": ex.get("lifecycle") or fm.get("status") or "unknown",
            "converted": ex.get("converted"),
            "rooms": as_int(ex.get("rooms")),
            "placed_instances": placed,
            "builtin_instances": builtin,
            "builtin_pct": as_float(ex.get("builtin_pct")),
            "builtin_types": as_int(ex.get("builtin_types")),
            "script_instances": as_int(ex.get("script_instances")),
            "script_pct": as_float(ex.get("script_pct")),
            "client_instances": as_int(ex.get("client_instances")),
            "kid_instances": as_int(ex.get("kid_instances")),
            "custom_objects": as_int(ex.get("custom_objects", fm.get("custom_objects"))),
            "traces_recorded": as_int(ex.get("traces_recorded", fm.get("traces_total"))),
            "objects_exercised": as_int(ex.get("objects_exercised")),
            "trace_objects": ex.get("trace_objects") or [],
            "trace_cases": trace_cases.get(case_dir) if case_dir else None,
            "trace_case_dir": case_dir,
            "objects_in_package": total_objects,
            "objects_unmapped": scripted,
            "objects_mapped": specced,
            "objects_scripted": scripted,
            "objects_mapped_pct": (round(100.0 * specced / total_objects, 1)
                                   if specced is not None and total_objects else None),
            "objects_scripted_pct": (round(100.0 * scripted / total_objects, 1)
                                     if scripted is not None and total_objects else None),
            "check_errors": as_int(ex.get("check_errors")),
            "check_warnings": as_int(ex.get("check_warnings")),
            "port": None,
        }
        if rec["builtin_pct"] is None and placed and builtin is not None:
            rec["builtin_pct"] = round(100.0 * builtin / placed, 1)
        if rec["script_pct"] is None and placed and rec["script_instances"] is not None:
            rec["script_pct"] = round(100.0 * rec["script_instances"] / placed, 2)
        games.append(rec)
    games.sort(key=lambda g: g["order"])
    return games, exporter_meta


# ----------------------------------------------------------------- GitHub
def gh_json(path):
    out = subprocess.run(["gh", "api", "--paginate", "--slurp", path],
                         capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise RuntimeError(out.stderr.strip()[:300])
    return [x for page in json.loads(out.stdout) for x in page]


def parse_milestone_description(desc):
    fields = {}
    for line in (desc or "").splitlines():
        m = re.match(r"(Scope|Feeds from|Area labels|Entry|Exit \(done when\)|Progress):\s*(.*)", line)
        if m:
            fields[m.group(1).lower().replace(" (done when)", "")] = m.group(2).strip()
    return fields


def phase_of(issue):
    ms = issue.get("milestone")
    return ms["title"] if ms else None


def slim(issue):
    return {"number": issue["number"], "title": issue["title"], "state": issue["state"],
            "url": issue["html_url"], "labels": [l["name"] for l in issue["labels"]],
            "phase": phase_of(issue), "updated_at": issue.get("updated_at"),
            "closed_at": issue.get("closed_at"), "created_at": issue.get("created_at")}


def build_phases(milestones, issues):
    by_ms = {}
    for i in issues:
        if i.get("milestone"):
            by_ms.setdefault(i["milestone"]["title"], []).append(i)
    phases = []
    for m in sorted(milestones, key=lambda m: m["number"]):
        rows = by_ms.get(m["title"], [])
        opened = [i for i in rows if i["state"] == "open"]
        sizes = {"size:S": 0, "size:M": 0, "size:L": 0, "unknown": 0}
        points = 0
        for i in opened:
            labels = {l["name"] for l in i["labels"]}
            hit = next((s for s in ("size:L", "size:M", "size:S") if s in labels), None)
            if hit:
                sizes[hit] += 1
                points += SIZE_POINTS[hit]
            else:
                sizes["unknown"] += 1
        fields = parse_milestone_description(m.get("description"))
        total = len(rows)
        phases.append({
            "number": int(m["title"].split(" · ")[0]) if " · " in m["title"] else None,
            "title": m["title"],
            "name": m["title"].split(" · ", 1)[1] if " · " in m["title"] else m["title"],
            "state": m["state"], "open": len(opened), "closed": total - len(opened), "total": total,
            "pct_closed": round(100.0 * (total - len(opened)) / total, 1) if total else None,
            "size_points": points, "sizes": sizes,
            "scope": fields.get("scope", ""), "feeds_from": fields.get("feeds from", ""),
            "area_labels": fields.get("area labels", ""), "entry": fields.get("entry", ""),
            "exit": fields.get("exit", ""), "progress": fields.get("progress", ""),
        })
    return phases


def burnup(issues, now, max_weeks=16):
    created = [dt.datetime.fromisoformat(i["created_at"].replace("Z", "+00:00"))
               for i in issues if i.get("created_at")]
    closed = [dt.datetime.fromisoformat(i["closed_at"].replace("Z", "+00:00"))
              for i in issues if i.get("closed_at")]
    if not created:
        return []
    start = min(created).date()
    end = max([now.date()] + [d.date() for d in closed])
    ws = start - dt.timedelta(days=start.weekday())
    points = []
    while ws <= end:
        we = ws + dt.timedelta(days=7)
        points.append({"week": we.isoformat(),
                       "created": sum(1 for d in created if d.date() < we),
                       "closed": sum(1 for d in closed if d.date() < we)})
        ws = we
    return points[-max_weeks:]


def build_activity(issues, pulls, now):
    opened = [i for i in issues if i["state"] == "open"]
    closed = [i for i in issues if i["state"] == "closed"]
    labels = lambda i: {l["name"] for l in i["labels"]}
    cutoff = now - dt.timedelta(days=14)
    closed_at = lambda i: (dt.datetime.fromisoformat(i["closed_at"].replace("Z", "+00:00"))
                           if i.get("closed_at") else None)
    recent_closed = sorted([i for i in closed if closed_at(i) and closed_at(i) >= cutoff],
                           key=lambda i: i["closed_at"], reverse=True)[:8]
    in_progress = sorted([i for i in opened if "in-progress" in labels(i)],
                         key=lambda i: i.get("updated_at") or "", reverse=True)[:6]
    needs_user = sorted([i for i in opened if "needs-user" in labels(i)],
                        key=lambda i: i.get("updated_at") or "", reverse=True)[:6]
    ready = sorted([i for i in opened if "agent-ready" in labels(i)
                    and not (labels(i) & {"blocked", "in-progress"})],
                   key=lambda i: (phase_of(i) or "99", i["number"]))[:6]
    process = Counter()
    for i in opened:
        for name in ("agent-ready", "in-progress", "needs-user", "triage", "blocked", "revisit"):
            if name in labels(i):
                process[name] += 1
    area = Counter()
    for i in opened:
        for l in labels(i):
            if l.startswith("area:"):
                area[l] += 1
    week = now - dt.timedelta(days=7)
    return {
        "totals": {"issues": len(issues), "open": len(opened), "closed": len(closed),
                   "pct_closed": round(100.0 * len(closed) / len(issues), 1) if issues else None},
        "open_prs": len(pulls), "process": dict(process),
        "area_open": [{"area": k, "count": v} for k, v in area.most_common()],
        "recent_closed": [slim(i) for i in recent_closed],
        "in_progress": [slim(i) for i in in_progress],
        "needs_user": [slim(i) for i in needs_user],
        "agent_ready": [slim(i) for i in ready],
        "closed_7d": sum(1 for i in closed if closed_at(i) and closed_at(i) >= week),
        "opened_7d": sum(1 for i in issues if i.get("created_at")
                         and dt.datetime.fromisoformat(i["created_at"].replace("Z", "+00:00")) >= week),
        "burnup": burnup(issues, now),
    }


def build_ports(games, games_issues):
    by_order = {}
    for i in games_issues:
        m = re.match(r"Port #(\d+):", i["title"])
        if m:
            by_order[int(m.group(1))] = i
    for g in games:
        pi = by_order.get(g["order"])
        if not pi:
            continue
        body = pi.get("body") or ""
        sec = re.search(r"## Done when(.*?)(?:\n## |\Z)", body, re.S)
        section = sec.group(1) if sec else body
        done = len(re.findall(r"- \[[xX]\]", section))
        total = len(re.findall(r"- \[[ xX]\]", section))
        sub = pi.get("sub_issues_summary") or {}
        g["port"] = {"number": pi["number"], "state": pi["state"], "url": pi["html_url"],
                     "done": done, "total": total,
                     "sub_done": sub.get("completed", 0), "sub_total": sub.get("total", 0),
                     "updated_at": pi.get("updated_at")}


def compute_overall(games):
    def total(key):
        return sum(g.get(key) or 0 for g in games)
    placed = total("placed_instances")
    builtin = total("builtin_instances")
    script = total("script_instances")
    cases = [g["trace_cases"] for g in games if g.get("trace_cases")]
    trace_ok = sum(c["ok"] for c in cases)
    trace_recorded = sum(c["recorded"] for c in cases)
    packages = [g for g in games
                if g.get("objects_mapped") is not None and g.get("objects_in_package")]
    package_mapped = sum(g["objects_mapped"] for g in packages)
    package_scripted = sum(g.get("objects_scripted") or 0 for g in packages)
    package_objects = sum(g["objects_in_package"] for g in packages)
    return {
        "placed_instances": placed,
        "builtin_instances": builtin,
        "builtin_pct": round(100.0 * builtin / placed, 1) if placed else None,
        "script_instances": script,
        "script_pct": round(100.0 * script / placed, 2) if placed else None,
        "client_instances": total("client_instances"),
        "kid_instances": total("kid_instances"),
        "custom_objects": total("custom_objects"),
        "rooms": total("rooms"),
        "traces_recorded": total("traces_recorded"),
        "objects_exercised": total("objects_exercised"),
        "objects_exercised_unique": len({o for g in games for o in (g.get("trace_objects") or [])}),
        "games_census": sum(1 for g in games if g["placed_instances"]),
        "games_traces": sum(1 for g in games if g["traces_recorded"]),
        "games_total": len(games),
        "trace_cases_ok": trace_ok,
        "trace_cases_recorded": trace_recorded,
        "trace_cases_pct": round(100.0 * trace_ok / trace_recorded, 1) if trace_recorded else None,
        "games_with_cases": len(cases),
        "package_objects_mapped": package_mapped if packages else None,
        "package_objects_scripted": package_scripted if packages else None,
        "package_objects": package_objects if packages else None,
        "package_mapped_pct": (round(100.0 * package_mapped / package_objects, 1)
                               if package_objects else None),
        "package_scripted_pct": (round(100.0 * package_scripted / package_objects, 1)
                                 if package_objects else None),
        "games_with_package": len(packages),
    }


def preserve_ports(games):
    path = os.path.join(PROGRESS, "data.json")
    if not os.path.exists(path):
        return
    old = json.load(open(path, encoding="utf-8"))
    by = {g.get("slug"): g for g in old.get("games", [])}
    for g in games:
        if g["slug"] in by:
            g["port"] = by[g["slug"]].get("port")


def build_data(refresh):
    games, exporter_meta = load_games()
    if refresh:
        milestones = gh_json(f"repos/{REPO}/milestones?state=all&per_page=100")
        issues = [i for i in gh_json(f"repos/{REPO}/issues?state=all&per_page=100")
                  if "pull_request" not in i]
        pulls = gh_json(f"repos/{REPO}/pulls?state=open&per_page=100")
        now = dt.datetime.now(dt.timezone.utc)
        phases = build_phases(milestones, issues)
        activity = build_activity(issues, pulls, now)
        try:
            games_issues = [i for i in gh_json(f"repos/{GAMES_REPO}/issues?state=all&per_page=100")
                            if "pull_request" not in i]
            build_ports(games, games_issues)
        except Exception as exc:
            # The private games repo is not readable with the workflow's repository token.
            # Keep the last committed port data; the public-repo data still refreshes.
            print(f"warning: games repo read failed ({exc}); keeping committed port data",
                  file=sys.stderr)
            preserve_ports(games)
        source = "gh"
        at = now.isoformat().replace("+00:00", "Z")
    else:
        phases, activity, source = [], empty_activity(), "game files only"
        at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    return {"generated_at": at, "source": source, "project_status": PROJECT_STATUS,
            "phases": phases, "activity": activity, "games": games,
            "overall": compute_overall(games), "followups": FOLLOWUPS,
            "exporter": exporter_meta, "script_api": load_script_api()}


def empty_activity():
    return {"totals": {"issues": 0, "open": 0, "closed": 0, "pct_closed": None},
            "open_prs": 0, "process": {}, "area_open": [], "recent_closed": [],
            "in_progress": [], "needs_user": [], "agent_ready": [],
            "closed_7d": 0, "opened_7d": 0, "burnup": []}


def refresh_local(data):
    games, exporter_meta = load_games()
    old = {g.get("slug"): g for g in data.get("games", [])}
    for g in games:
        if g["slug"] in old:
            g["port"] = old[g["slug"]].get("port")
    data["games"] = games
    data["overall"] = compute_overall(games)
    data["exporter"] = exporter_meta
    data["script_api"] = load_script_api()
    return data


# ----------------------------------------------------------------- history
def snapshot(data, at):
    a, ov = data["activity"], data["overall"]
    return {
        "at": at,
        "builtin_pct": ov["builtin_pct"],
        "script_pct": ov["script_pct"],
        "placed_instances": ov["placed_instances"],
        "traces_recorded": ov["traces_recorded"],
        "objects_exercised": ov["objects_exercised"],
        "trace_cases_ok": ov["trace_cases_ok"],
        "trace_cases_recorded": ov["trace_cases_recorded"],
        "package_mapped_pct": ov["package_mapped_pct"],
        "package_scripted_pct": ov["package_scripted_pct"],
        "open_issues": a["totals"]["open"],
        "closed_issues": a["totals"]["closed"],
        "issue_pct_closed": a["totals"]["pct_closed"],
        "size_points": sum(p["size_points"] for p in data["phases"]),
        "phases": [{"number": p["number"], "title": p["title"], "open": p["open"],
                    "closed": p["closed"], "size_points": p["size_points"]} for p in data["phases"]],
    }


def maybe_history(data, at):
    path = os.path.join(PROGRESS, "history.json")
    hist = json.load(open(path, encoding="utf-8")) if os.path.exists(path) else []
    snap = snapshot(data, at)
    keep = ("builtin_pct", "script_pct", "placed_instances", "traces_recorded",
            "objects_exercised", "trace_cases_ok", "trace_cases_recorded",
            "package_mapped_pct", "package_scripted_pct", "open_issues", "closed_issues",
            "issue_pct_closed",
            "size_points")
    if hist and all(hist[-1].get(k) == snap.get(k) for k in keep) and hist[-1]["at"][:10] == at[:10]:
        return hist
    hist.append(snap)
    json.dump(hist, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    return hist


# ----------------------------------------------------------------- render
def esc(value):
    return html.escape("" if value is None else str(value))


def human_time(value):
    if not value:
        return "—"
    try:
        when = dt.datetime.fromisoformat(str(value).replace("Z", "+00:00"))
        return when.strftime("%Y-%m-%d %H:%M:%S UTC")
    except (TypeError, ValueError):
        return str(value)


def short_week(value):
    try:
        week = dt.date.fromisoformat(str(value))
        return f"{week.strftime('%b')} {week.day}"
    except (TypeError, ValueError):
        return str(value)


def width_pct(value):
    try:
        return max(0.0, min(100.0, float(value)))
    except (TypeError, ValueError):
        return 0.0


def stack(ov):
    placed = ov["placed_instances"] or 0
    builtin = ov.get("builtin_instances") or 0
    script = ov.get("script_instances") or 0
    other = (ov.get("client_instances") or 0) + (ov.get("kid_instances") or 0)
    def share(value):
        return 100.0 * value / placed if placed else 0.0
    label = (f"Placed instances by coverage: {fmt(builtin)} built-in, "
             f"{fmt(script)} script, {fmt(other)} client or kid, of {fmt(placed)} total.")
    return (f'<div class="coverage-stack" role="img" aria-label="{esc(label)}">'
            f'<span class="seg builtin" style="width:{share(builtin):.2f}%"></span>'
            f'<span class="seg script" style="width:{share(script):.2f}%"></span>'
            f'<span class="seg other" style="width:{share(other):.2f}%"></span></div>')


def impl_bar(label, pct, value, missing=None, tone="cases"):
    """One measured per-game bar, or an explicit not-measured state."""
    if pct is None:
        reason = missing or "source missing"
        return (f'<span class="impl-row {tone}">'
                f'<span class="impl-label">{esc(label)}</span>'
                f'<span class="impl-track is-missing" aria-hidden="true"></span>'
                f'<span class="impl-value is-missing">not measured yet<small>· {esc(reason)}</small></span>'
                f'</span>')
    width = width_pct(pct)
    return (f'<span class="impl-row {tone}">'
            f'<span class="impl-label">{esc(label)}</span>'
            f'<span class="impl-track" aria-hidden="true">'
            f'<span style="width:{width:.1f}%"></span></span>'
            f'<span class="impl-value">{esc(value)}<small>· {pct:.1f}%</small></span>'
            f'</span>')


def trace_bar(game):
    tc = game.get("trace_cases")
    if tc is None:
        return impl_bar("Trace cases", None, "", "no case directory")
    if not tc["recorded"]:
        return impl_bar("Trace cases", None, "", "no cases recorded")
    bits = [f'{tc["ok"]} / {tc["recorded"]} OK']
    for key, label in (("open", "OPEN"), ("fail", "FAIL"), ("other", "other")):
        if tc[key]:
            bits.append(f'{tc[key]} {label}')
    if tc["unclassified"]:
        bits.append(f'{tc["unclassified"]} unclassified')
    return impl_bar("Trace cases", tc["pass_pct"], " · ".join(bits))


def package_bar(game):
    if game.get("objects_in_package") is None or game.get("objects_mapped") is None:
        return impl_bar("Mapped objects", None, "", "no converter report", "package")
    value = f'{game["objects_mapped"]} / {game["objects_in_package"]} mapped'
    return impl_bar("Mapped objects", game.get("objects_mapped_pct"), value,
                    tone="package")


def scripted_bar(game):
    if game.get("objects_in_package") is None or game.get("objects_scripted") is None:
        return impl_bar("Scripted objects", None, "", "no converter report", "scripted")
    value = f'{game["objects_scripted"]} / {game["objects_in_package"]} scripted'
    return impl_bar("Scripted objects", game.get("objects_scripted_pct"), value,
                    tone="scripted")


def api_surface(api):
    if not api or api.get("planned_count") is None:
        return ('<p class="api-note">The script API census is '
                '<b>not measured yet</b>: the census table could not be read.</p>')
    groups = []
    for group in api["planned_groups"]:
        link = ""
        if group.get("issue"):
            link = (f' · <a href="https://github.com/{REPO}/issues/{group["issue"]}">'
                    f'#{group["issue"]}</a>')
        groups.append(f'<li><b>{esc(group["name"])}</b> '
                      f'<span class="api-counts">{esc(group["counts"])} scripts / objects / games</span>'
                      f'{link}</li>')
    registered = (f'{api["registered_functions"]:,} registered functions'
                  if api.get("registered_functions") is not None
                  else "the generated function list is missing")
    return f"""
        <div class="api-head api-subhead">
          <span class="reg-label">Script API census</span>
          <span class="composition-figure">share not measured yet</span>
        </div>
        <p class="api-note"><code>docs/format/script-api.md</code> lists <b>{registered}</b>. The census names <b>{api["planned_count"]} groups not yet registered</b>. Their counts are scripts / objects / games. The implemented share of the script API is <b>not measured yet</b>: {esc(api["missing"])}.</p>
        <ul class="api-groups">{''.join(groups)}</ul>"""


def exporter_label(data):
    meta = data.get("exporter") or {}
    if meta.get("generated_at"):
        return human_time(meta["generated_at"])
    return "not recorded"


def implementation_summary(ov):
    recorded = ov.get("trace_cases_recorded") or 0
    if recorded:
        cases = (f'Across the {ov.get("games_with_cases") or 0} games with cases, '
                 f'{fmt(ov.get("trace_cases_ok") or 0)} of {fmt(recorded)} cases pass')
    else:
        cases = "No trace cases are recorded in this repo yet"
    if ov.get("package_objects"):
        packages = (f'across games with a package, '
                    f'{fmt(ov.get("package_objects_mapped") or 0)} of '
                    f'{fmt(ov["package_objects"])} objects are mapped and '
                    f'{fmt(ov.get("package_objects_scripted") or 0)} are scripted')
    else:
        packages = "package object mapping is not measured yet"
    return f'{cases}; {packages}.'


def coverage_rows(games, exporter_at):
    out = []
    for g in games:
        pct = g["builtin_pct"]
        builtin_pct = pctf(pct) if pct is not None else "not measured yet"
        builtin = fmt(g["builtin_instances"]) if g["builtin_instances"] is not None else "not measured yet"
        placed = fmt(g["placed_instances"]) if g["placed_instances"] is not None else "not measured yet"
        script_instances = fmt(g["script_instances"]) if g["script_instances"] is not None else "not measured yet"
        script_pct = pctf(g["script_pct"], 2) if g["script_pct"] is not None else "not measured yet"
        custom = fmt(g["custom_objects"]) if g["custom_objects"] is not None else "not measured yet"
        traces = fmt(g["traces_recorded"]) if g["traces_recorded"] is not None else "not measured yet"
        exercised = fmt(g["objects_exercised"]) if g["objects_exercised"] is not None else "not measured yet"
        order = esc(g["order"])
        tc = g.get("trace_cases")
        case_pct = tc["pass_pct"] if tc else None
        if tc and tc["recorded"]:
            trace_fact = [f'<b>{tc["ok"]} / {tc["recorded"]}</b> OK ({tc["pass_pct"]:.1f}%)']
            for key, label in (("open", "OPEN"), ("fail", "FAIL"), ("other", "other")):
                if tc[key]:
                    trace_fact.append(f'{tc[key]} {label}')
            if tc["unclassified"]:
                trace_fact.append(f'{tc["unclassified"]} unclassified')
            trace_fact = " · ".join(trace_fact)
        elif tc:
            trace_fact = "no cases recorded"
        else:
            trace_fact = '<span class="missing">not measured yet</span> · no case directory'
        if g.get("objects_mapped") is not None and g.get("objects_in_package"):
            package_fact = (f'<b>{g["objects_mapped"]} / {g["objects_in_package"]}</b> '
                            f'mapped ({g["objects_mapped_pct"]:.1f}%)')
        else:
            package_fact = '<span class="missing">not measured yet</span> · no converter report'
        if g.get("objects_scripted") is not None and g.get("objects_in_package"):
            scripted_fact = (f'<b>{g["objects_scripted"]} / {g["objects_in_package"]}</b> '
                             f'scripted ({g["objects_scripted_pct"]:.1f}%)')
        else:
            scripted_fact = '<span class="missing">not measured yet</span> · no converter report'
        if g.get("check_errors") is not None:
            converted = ("converted" if g.get("converted") is True
                         else "not converted" if g.get("converted") is False
                         else "conversion unknown")
            check_fact = (f'{converted} · <b>{g["check_errors"]}</b> errors · '
                          f'<b>{g["check_warnings"]}</b> warnings')
        else:
            check_fact = '<span class="missing">not measured yet</span> · no converter report'
        case_source = (f'<br><span class="fact-source">crates/tools/cases/{esc(g["trace_case_dir"])}</span>'
                       if g.get("trace_case_dir") else "")
        port = g.get("port")
        if port:
            state = "open" if port["state"] == "open" else "closed"
            done = f'{port["done"]}/{port["total"]} done' if port["total"] else "checklist not measured yet"
            port_pct = 100.0 * port["done"] / port["total"] if port["total"] else 0.0
            if port["total"]:
                slots = "".join(
                    f'<span class="port-step{" filled" if i < port["done"] else ""}"></span>'
                    for i in range(port["total"])
                )
                port_meter = f'<span class="port-meter" aria-hidden="true">{slots}</span>'
                row_progress = (f'<span class="row-progress">'
                                f'<span class="row-progress-label">Port checklist</span>'
                                f'{port_meter}'
                                f'<span class="row-progress-state">{done} · {port_pct:.0f}%</span>'
                                f'</span>')
            else:
                port_meter = ""
                row_progress = ('<span class="row-progress">'
                                '<span class="row-progress-label">Port checklist</span>'
                                '<span class="row-progress-state missing">checklist not measured yet</span>'
                                '</span>')
            port_summary = (f'games#{port["number"]} '
                            f'<span class="state state-{state}"><span class="state-mark" aria-hidden="true"></span>{state}</span>')
            port_detail = f'games#{port["number"]} · {state} · {done}{port_meter}'
            port_link = (f' · <a href="{esc(port["url"])}">Port issue games#{port["number"]}</a>')
        else:
            port_summary = '<span class="missing">no port issue</span>'
            port_detail = "no port issue"
            row_progress = ""
            port_link = ""
        source = esc(g["source"] or "#")
        name = esc(g["name"])
        slug = esc(g["slug"])
        meta = esc(g["genre"] or g["engine"])
        out.append(f"""
        <li class="game-item" data-name="{esc((g['name'] + ' ' + g['slug']).lower())}"
            data-cov="{pct if pct is not None else -1}"
            data-custom="{g['custom_objects'] if g['custom_objects'] is not None else -1}"
            data-script="{g['script_instances'] if g['script_instances'] is not None else -1}"
            data-cases="{case_pct if case_pct is not None else -1}"
            data-traces="{1 if g['traces_recorded'] else 0}">
          <details class="game-entry">
            <summary class="game-summary">
              <span class="game-identity">
                <span class="game-order" aria-hidden="true">{order}</span>
                <span class="game-name">{name}</span>
                <span class="game-meta">{slug} · {meta}</span>
              </span>
              <span class="game-work">
                <span class="metric-label">Game-specific work</span>
                <span class="metric-value"><b>{script_instances}</b> script instances<br><b>{custom}</b> custom types</span>
              </span>
              <span class="game-evidence">
                <span class="metric-label">Evidence</span>
                <span class="metric-value"><b>{traces}</b> traces<br><b>{exercised}</b> exercised</span>
              </span>
              <span class="game-port">
                <span class="metric-label">Port</span>
                <span class="metric-value">{port_summary}</span>
              </span>
              <span class="game-composition">
                <span class="metric-label">Composition</span>
                <span class="metric-value">{builtin_pct} built-in<br>{builtin} / {placed} placed</span>
              </span>
              <span class="game-toggle" aria-hidden="true"></span>
              <span class="impl-bars">
                {trace_bar(g)}
                {package_bar(g)}
                {scripted_bar(g)}
              </span>
              {row_progress}
            </summary>
            <div class="game-detail">
              <dl class="game-facts">
                <div><dt>Census order</dt><dd>{order}</dd></div>
                <div><dt>Placed instances</dt><dd>{placed}</dd></div>
                <div><dt>Built-in composition</dt><dd><b>{builtin_pct}</b> · {builtin} / {placed}</dd></div>
                <div><dt>Script instances</dt><dd><b>{script_instances}</b> · {script_pct} of placed</dd></div>
                <div><dt>Custom object types</dt><dd><b>{custom}</b></dd></div>
                <div><dt>Recorded traces</dt><dd><b>{traces}</b></dd></div>
                <div><dt>Objects exercised</dt><dd><b>{exercised}</b></dd></div>
                <div><dt>Trace cases</dt><dd>{trace_fact}{case_source}</dd></div>
                <div><dt>Mapped objects</dt><dd>{package_fact}</dd></div>
                <div><dt>Scripted objects</dt><dd>{scripted_fact}</dd></div>
                <div><dt>Converter check</dt><dd>{check_fact}</dd></div>
                <div><dt>Port</dt><dd>{port_detail}</dd></div>
              </dl>
              <p class="game-explain"><strong>What the bars mean.</strong> <b>Trace cases</b> counts committed cases under <code>crates/tools/cases/</code> whose status in <code>crates/tools/fidelity-baseline.txt</code> is OK, over the cases recorded; it rises when a case is added and passes. <b>Mapped objects</b> and <b>scripted objects</b> partition the converted package: mapped means an object resolves to a built-in behavior spec, scripted means the package writes it as a script. They are implementation counts, not completion scores. Built-in coverage and script share are composition shares of all placed instances.</p>
              <p class="game-source">Mapped and scripted counts are the committed exporter snapshot of <b>{exporter_at}</b>; regenerating it needs the research checkout.</p>
              <p class="game-links"><a href="{source}">Game page</a>{port_link}</p>
            </div>
          </details>
        </li>""")
    return "".join(out)


def phase_rows(phases):
    out = []
    for p in phases:
        pv = p["pct_closed"] if p["pct_closed"] is not None else 0
        pct_text = f"{pv:.1f}%" if p["pct_closed"] is not None else "not measured yet"
        number = p["number"] if p["number"] is not None else "—"
        sizes = p.get("sizes") or {}
        size_bits = [f"{label} {sizes[key]}" for label, key in
                     (("S", "size:S"), ("M", "size:M"), ("L", "size:L")) if sizes.get(key)]
        if sizes.get("unknown"):
            size_bits.append(f"{sizes['unknown']} unlabelled")
        size_text = " · ".join(size_bits) if size_bits else "no open sizes"
        alpha = p["number"] == 7
        alpha_tag = '<span class="alpha-tag">Public alpha planned here</span>' if alpha else ""
        area = f'<p class="phase-area">{esc(p["area_labels"])}</p>' if p["area_labels"] else ""
        out.append(f"""
        <li class="phase{' alpha' if alpha else ''}">
          <div class="phase-name">
            <span class="phase-kicker">Phase {esc(number)}</span>
            <h3>{esc(p['name'])}</h3>
            {area}
            {alpha_tag}
          </div>
          <div class="phase-body">
            <div class="phase-values"><span>{p['closed']} / {p['total']} closed</span><span>{pct_text}</span></div>
            <span class="progress-track" aria-hidden="true"><span class="progress-fill" style="width:{pv:.1f}%"></span></span>
            <div class="phase-meta">
              <span>{p["open"]} open</span>
              <span>{p["size_points"]} size points left</span>
              <span>{esc(size_text)}</span>
            </div>
            <details class="gate"><summary>Entry and exit conditions</summary>
              <dl>
                <div><dt>Entry</dt><dd>{esc(p['entry'])}</dd></div>
                <div><dt>Exit</dt><dd>{esc(p['exit'])}</dd></div>
                <div><dt>Scope</dt><dd>{esc(p['scope'])}</dd></div>
              </dl>
            </details>
          </div>
        </li>""")
    return "".join(out)


def issue_list(items, empty):
    if not items:
        return f'<p class="empty">{esc(empty)}</p>'
    out = []
    for i in items:
        phase = ""
        if i.get("phase"):
            phase = f'<span class="issue-phase">Phase {esc(i["phase"].split(" · ")[0])}</span>'
        out.append(f'<li>{phase}<a class="issue-number" href="{esc(i["url"])}">#{i["number"]}</a> '
                   f'<span class="issue-title">{esc(i["title"])}</span></li>')
    return '<ul class="issue-list">' + "".join(out) + "</ul>"


def area_bars(rows):
    if not rows:
        return '<p class="empty">No open issues with an area label.</p>'
    top = max(r["count"] for r in rows) or 1
    out = []
    for r in rows:
        width = 100.0 * r["count"] / top
        out.append(f'<div class="area-row"><span class="area-label">{esc(r["area"])}</span>'
                   f'<span class="area-track" aria-hidden="true"><span class="area-fill" '
                   f'style="width:{width:.1f}%"></span></span>'
                   f'<span class="area-count">{r["count"]}</span></div>')
    return '<div class="area-chart">' + "".join(out) + "</div>"


def burnup_chart(points):
    if len(points) < 2:
        return '<p class="empty">The burn-up needs at least two weeks of issue history.</p>'
    top = max(max(p["created"] for p in points), max(p["closed"] for p in points), 1)
    rows = []
    for p in points:
        opened = 100.0 * p["created"] / top
        closed = 100.0 * p["closed"] / top
        rows.append(f"""
          <tr>
            <th scope="row"><time datetime="{esc(p['week'])}">{esc(short_week(p['week']))}</time></th>
            <td><span class="bar-cell"><span class="bar-track" aria-hidden="true"><span class="bar-fill opened" style="width:{opened:.1f}%"></span></span><span class="bar-value">{p['created']}</span></span></td>
            <td><span class="bar-cell"><span class="bar-track" aria-hidden="true"><span class="bar-fill closed" style="width:{closed:.1f}%"></span></span><span class="bar-value">{p['closed']}</span></span></td>
          </tr>""")
    return f"""
        <table class="flow-table">
          <caption class="visually-hidden">Cumulative issues opened and closed by weekly snapshot</caption>
          <thead><tr><th scope="col">Week</th><th scope="col">Opened</th><th scope="col">Closed</th></tr></thead>
          <tbody>{''.join(rows)}</tbody>
        </table>"""


def coverage_history(hist):
    measured = [h for h in hist if h.get("builtin_pct") is not None]
    if not measured:
        return '<p class="empty">The first history point is this run; the daily workflow appends the next one.</p>'
    rows = []
    for h in hist:
        at = human_time(h.get("at"))
        builtin = pctf(h.get("builtin_pct")) if h.get("builtin_pct") is not None else '<span class="missing">not measured yet</span>'
        script = pctf(h.get("script_pct"), 2) if h.get("script_pct") is not None else '<span class="missing">not measured yet</span>'
        placed = fmt(h.get("placed_instances")) if h.get("placed_instances") is not None else "—"
        trace = (f'{fmt(h.get("trace_cases_ok"))} / {fmt(h.get("trace_cases_recorded"))}'
                 if h.get("trace_cases_recorded") is not None
                 else '<span class="missing">not measured yet</span>')
        if h.get("package_mapped_pct") is not None:
            package = f'{pctf(h.get("package_mapped_pct"))} mapped'
            if h.get("package_scripted_pct") is not None:
                package += f' · {pctf(h.get("package_scripted_pct"))} scripted'
        else:
            package = '<span class="missing">not measured yet</span>'
        rows.append(f"""
          <tr>
            <th scope="row"><time datetime="{esc(h.get('at'))}">{esc(at)}</time></th>
            <td class="num" data-label="Placed">{placed}</td>
            <td class="num" data-label="Built-in coverage">{builtin}</td>
            <td class="num" data-label="Script-object share">{script}</td>
            <td class="num" data-label="Trace cases">{trace}</td>
            <td class="num" data-label="Package objects">{package}</td>
          </tr>""")
    return f"""
        <table class="history-table">
          <caption class="visually-hidden">Coverage history by workflow run</caption>
          <thead><tr><th scope="col">Run</th><th scope="col" class="num">Placed</th><th scope="col" class="num">Built-in coverage</th><th scope="col" class="num">Script-object share</th><th scope="col" class="num">Trace cases</th><th scope="col" class="num">Package objects</th></tr></thead>
          <tbody>{''.join(rows)}</tbody>
        </table>"""


def render(data, hist):
    a, ov = data["activity"], data["overall"]
    ps = data.get("project_status", PROJECT_STATUS)
    phases = data["phases"]
    totals = a["totals"]
    points = sum(p["size_points"] for p in phases)
    ported = [g["port"] for g in data["games"] if g.get("port")]
    port_done = sum(p["done"] for p in ported)
    port_total = sum(p["total"] for p in ported)
    port_pct = round(100.0 * port_done / port_total, 1) if port_total else None
    phase7 = next((p for p in phases if p["number"] == 7), None)
    phase7_pct = phase7["pct_closed"] if phase7 else None
    process_order = ["in-progress", "needs-user", "agent-ready", "blocked", "triage", "revisit"]
    process_items = sorted(a["process"].items(),
                           key=lambda kv: (process_order.index(kv[0]) if kv[0] in process_order else 99, kv[0]))
    chips = "".join(f'<span class="process-chip"><b>{v}</b> {esc(k)}</span>'
                    for k, v in process_items if v)
    replacements = {
        "__TITLE__": "IWBTG Collection — build ledger",
        "__GENERATED__": esc(human_time(data["generated_at"])),
        "__SOURCE__": esc(data["source"]),
        "__STATUS__": esc(ps["label"]),
        "__STATUS_DETAIL__": esc(ps["detail"]),
        "__BUILTIN_PCT__": pctf(ov["builtin_pct"]),
        "__BUILTIN__": fmt(ov["builtin_instances"]),
        "__PLACED__": fmt(ov["placed_instances"]),
        "__SCRIPT__": fmt(ov["script_instances"]),
        "__SCRIPT_PCT__": pctf(ov["script_pct"], 2),
        "__CUSTOM__": fmt(ov["custom_objects"]),
        "__TRACES__": fmt(ov["traces_recorded"]),
        "__GAMES_TRACES__": str(ov["games_traces"]),
        "__GAMES_TOTAL__": str(ov["games_total"]),
        "__EXERCISED__": fmt(ov["objects_exercised"]),
        "__EXERCISED_UNIQUE__": fmt(ov["objects_exercised_unique"]),
        "__ROOMS__": fmt(ov["rooms"]),
        "__OTHER__": fmt((ov.get("client_instances") or 0) + (ov.get("kid_instances") or 0)),
        "__STACK__": stack(ov),
        "__COVERAGE_ROWS__": coverage_rows(data["games"], exporter_label(data)),
        "__API_SURFACE__": api_surface(data.get("script_api") or {}),
        "__EXPORTER_DATE__": esc(exporter_label(data)),
        "__PACKAGE_HEAD__": (f'{fmt(ov["package_objects_mapped"])} mapped · '
                             f'{fmt(ov.get("package_objects_scripted") or 0)} scripted '
                             f'of {fmt(ov["package_objects"])} package objects'
                             if ov.get("package_objects") else
                             "package object mapping not measured yet"),
        "__IMPLEMENTATION_SUMMARY__": implementation_summary(ov),
        "__PHASE_ROWS__": phase_rows(phases),
        "__AREA_BARS__": area_bars(a["area_open"]),
        "__BURNUP__": burnup_chart(a["burnup"]),
        "__COVERAGE_HISTORY__": coverage_history(hist),
        "__RECENT_CLOSED__": issue_list(a["recent_closed"], "Nothing closed in the last 14 days."),
        "__IN_PROGRESS__": issue_list(a["in_progress"], "No issue is claimed right now."),
        "__NEEDS_USER__": issue_list(a["needs_user"], "No question is waiting on the user."),
        "__READY__": issue_list(a["agent_ready"], "No unblocked agent-ready issue."),
        "__OPEN__": str(totals["open"]), "__CLOSED__": str(totals["closed"]),
        "__ISSUES__": str(totals["issues"]), "__ISSUE_PCT__": pctf(totals["pct_closed"]),
        "__ISSUE_WIDTH__": f'{totals["pct_closed"]:.1f}%' if totals["pct_closed"] is not None else "0%",
        "__PORT_DONE__": str(port_done), "__PORT_TOTAL__": str(port_total),
        "__PORT_PCT__": pctf(port_pct), "__PORT_WIDTH__": f"{port_pct:.1f}%" if port_pct is not None else "0%",
        "__PHASE7_NAME__": esc(phase7["name"]) if phase7 else "not found",
        "__PHASE7_PCT__": pctf(phase7_pct),
        "__PHASE7_WIDTH__": f"{phase7_pct:.1f}%" if phase7_pct is not None else "0%",
        "__PHASE7_CLOSED__": str(phase7["closed"]) if phase7 else "—",
        "__PHASE7_TOTAL__": str(phase7["total"]) if phase7 else "—",
        "__PHASE7_POINTS__": str(phase7["size_points"]) if phase7 else "—",
        "__POINTS__": str(points), "__OPEN_PRS__": str(a["open_prs"]),
        "__PROCESS__": chips,
        "__CENSUS__": str(FOLLOWUPS["census"]), "__TRACEFU__": str(FOLLOWUPS["traces"]),
    }
    page = TEMPLATE
    for key, value in replacements.items():
        page = page.replace(key, value)
    return "\n".join(line.rstrip() for line in page.splitlines()) + "\n"


TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<meta name="description" content="A generated public ledger of object coverage, phase gates, ports and issue activity for IWBTG Collection.">
<script>
(function(){
  try {
    var saved = localStorage.getItem('iwbtg-theme');
    if (saved === 'light' || saved === 'dark') {
      document.documentElement.dataset.theme = saved;
    }
  } catch (e) {}
})();
</script>
<style>
@font-face{font-family:"Barlow";src:url("fonts/Barlow-Regular.ttf") format("truetype");font-weight:400;font-style:normal;font-display:swap}
@font-face{font-family:"Barlow";src:url("fonts/Barlow-Medium.ttf") format("truetype");font-weight:500;font-style:normal;font-display:swap}
@font-face{font-family:"Barlow";src:url("fonts/Barlow-SemiBold.ttf") format("truetype");font-weight:600;font-style:normal;font-display:swap}
@font-face{font-family:"Barlow Condensed";src:url("fonts/BarlowCondensed-SemiBold.ttf") format("truetype");font-weight:600;font-style:normal;font-display:swap}
@font-face{font-family:"Barlow Condensed";src:url("fonts/BarlowCondensed-Bold.ttf") format("truetype");font-weight:700;font-style:normal;font-display:swap}

:root{
  color-scheme:light;
  --desk:#e7ecea;
  --paper:#f4f6f5;
  --paper-2:#ecefed;
  --paper-3:#e2e7e4;
  --ink:#121815;
  --ink-2:#44514a;
  --muted:#5b6862;
  --rule:#c8d1cc;
  --rule-strong:#7c8983;
  --cherry:#c01834;
  --cherry-strong:#8f1027;
  --cherry-soft:#f2d5da;
  --graphite:#26332d;
  --track:#e2e7e4;
  --good:#146b47;
  --warn:#7a5800;
  --crit:#a31c2e;
  --selection:#f1d3d9;
  --focus:#121815;
}
@media (prefers-color-scheme: dark){
  :root:not([data-theme="light"]){
    color-scheme:dark;
    --desk:#1b2220;
    --paper:#232a29;
    --paper-2:#29312f;
    --paper-3:#303936;
    --ink:#f0f3ef;
    --ink-2:#c4cdc8;
    --muted:#9faaa4;
    --rule:#3f4a46;
    --rule-strong:#7d8984;
    --cherry:#ff5f76;
    --cherry-strong:#ffa0af;
    --cherry-soft:#4a2730;
    --graphite:#dde5e0;
    --track:#343d3a;
    --good:#71d7a1;
    --warn:#f0c86a;
    --crit:#ff8999;
    --selection:#6a2833;
    --focus:#f0f3ef;
  }
}
:root[data-theme="dark"]{
  color-scheme:dark;
  --desk:#1b2220;
  --paper:#232a29;
  --paper-2:#29312f;
  --paper-3:#303936;
  --ink:#f0f3ef;
  --ink-2:#c4cdc8;
  --muted:#9faaa4;
  --rule:#3f4a46;
  --rule-strong:#7d8984;
  --cherry:#ff5f76;
  --cherry-strong:#ffa0af;
  --cherry-soft:#4a2730;
  --graphite:#dde5e0;
  --track:#343d3a;
  --good:#71d7a1;
  --warn:#f0c86a;
  --crit:#ff8999;
  --selection:#6a2833;
  --focus:#f0f3ef;
}
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%;scrollbar-color:var(--rule-strong) var(--paper-2)}
p{text-wrap:pretty}
body{margin:0;background:var(--desk);color:var(--ink);caret-color:var(--cherry);
  font-family:"Barlow",ui-sans-serif,-apple-system,"Segoe UI",Roboto,sans-serif;
  font-size:16px;line-height:1.55;text-rendering:optimizeLegibility}
::selection{background:var(--selection);color:var(--ink)}
a{color:var(--cherry);text-decoration:underline;text-decoration-thickness:.06em;text-underline-offset:.18em}
a:hover{color:var(--cherry-strong);text-decoration-thickness:.1em}
:focus-visible{outline:2px solid var(--focus);outline-offset:3px}
.skip-link{position:absolute;left:12px;top:-80px;z-index:20;background:var(--paper);color:var(--ink);padding:10px 14px;border:1px solid var(--rule-strong);font-weight:600}
.skip-link:focus{top:12px}
.nowrap{white-space:nowrap}
.visually-hidden{position:absolute!important;width:1px!important;height:1px!important;padding:0!important;margin:-1px!important;overflow:hidden!important;clip:rect(0,0,0,0)!important;white-space:nowrap!important;border:0!important}
.stage{max-width:1320px;margin:0 auto;background:var(--paper);display:grid;grid-template-columns:208px minmax(0,1fr);min-height:100vh}
.spine{position:sticky;top:0;align-self:start;height:100vh;max-height:100vh;overflow:auto;padding:28px 22px 22px;border-right:1px solid var(--rule);display:flex;flex-direction:column;gap:0}
.wordmark{display:block;color:var(--ink);text-decoration:none;font-family:"Barlow Condensed",sans-serif;font-weight:700;font-size:31px;line-height:.9;letter-spacing:-.01em}
.wordmark span{display:block}
.wordmark span+span{color:var(--ink-2);font-size:26px;font-weight:600}
.spine nav{display:flex;flex-direction:column;margin-top:42px}
.spine nav a{display:block;padding:10px 0;color:var(--ink-2);text-decoration:none;border-top:1px solid var(--rule)}
.spine nav a:hover{color:var(--cherry)}
.nav-name{display:block;font-size:14px;font-weight:500}
.nav-note{display:block;margin-top:1px;font-size:11px;color:var(--muted);line-height:1.3}
.spine-meta{margin-top:auto;padding-top:28px;font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;font-size:11px;line-height:1.5;color:var(--muted);overflow-wrap:anywhere}
.spine-meta span{display:block}
.sheet{min-width:0;padding:0 clamp(22px,5vw,64px)}
.masthead{padding-top:26px}
.masthead-top{display:flex;justify-content:space-between;align-items:flex-start;gap:20px;border-bottom:1px solid var(--rule);padding-bottom:14px}
.masthead-meta{display:flex;flex-wrap:wrap;gap:6px 18px;font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;font-size:11px;line-height:1.5;color:var(--muted);overflow-wrap:anywhere}
.masthead-meta b{font-weight:500;color:var(--ink-2)}
.theme{display:flex;align-items:center;gap:8px;flex-wrap:wrap;justify-content:flex-end}
.theme-label{font-size:11px;letter-spacing:.12em;text-transform:uppercase;color:var(--muted);font-weight:500}
.theme-options{display:flex;border:1px solid var(--rule-strong);padding:2px}
.theme-options button{appearance:none;border:0;border-left:1px solid var(--rule-strong);background:transparent;color:var(--ink-2);font:500 12px/1 "Barlow",sans-serif;padding:8px 10px;min-height:34px;cursor:pointer}
.theme-options button:first-child{border-left:0}
.theme-options button:hover{background:var(--paper-2);color:var(--ink)}
.theme-options button[aria-pressed="true"]{background:var(--ink);color:var(--paper)}
.theme-options button[aria-pressed="true"]:hover{background:var(--ink);color:var(--paper)}
.title-row{display:grid;grid-template-columns:minmax(0,1fr) 300px;gap:clamp(28px,4vw,56px);align-items:start;padding:clamp(30px,4vw,48px) 0 34px}
h1{font-family:"Barlow Condensed",sans-serif;font-size:clamp(44px,5.2vw,78px);line-height:.94;letter-spacing:-.018em;font-weight:700;margin:0;max-width:16ch}
.lede{max-width:62ch;margin:18px 0 0;color:var(--ink-2);font-size:17px;line-height:1.55}
.status-slab{border-left:1px solid var(--rule);padding-left:24px;min-width:0}
.seal{display:inline-flex;align-items:center;justify-content:center;justify-self:start;padding:8px 12px;border:2px solid var(--cherry);outline:1px solid var(--cherry);outline-offset:3px;color:var(--cherry);font-family:"Barlow Condensed",sans-serif;font-size:21px;font-weight:700;letter-spacing:.08em;text-transform:uppercase;line-height:1;transform:rotate(-1.5deg)}
.status-detail{margin:20px 0 18px;font-size:14px;line-height:1.5;color:var(--ink-2)}
.provenance{display:grid;grid-template-columns:auto minmax(0,1fr);gap:4px 12px;margin:0;font-size:11px;line-height:1.5}
.provenance dt{color:var(--muted);text-transform:uppercase;letter-spacing:.08em}
.provenance dd{margin:0;color:var(--ink-2);font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;overflow-wrap:anywhere}
.register{display:grid;grid-template-columns:minmax(0,1.6fr) minmax(0,1fr) minmax(0,1fr);border-top:2px solid var(--ink);border-bottom:1px solid var(--rule-strong)}
.reg-cell{padding:22px 0 26px;min-width:0}
.reg-cell+.reg-cell{border-left:1px solid var(--rule);padding-left:24px;margin-left:24px}
.reg-label{font-size:11px;font-weight:500;letter-spacing:.12em;text-transform:uppercase;color:var(--muted)}
.reg-figure{font-family:"Barlow Condensed",sans-serif;font-size:clamp(52px,5.4vw,78px);line-height:.86;letter-spacing:-.02em;font-weight:700;margin:10px 0 8px;font-variant-numeric:tabular-nums}
.reg-sub{margin:0;font-size:14px;line-height:1.45;color:var(--ink-2)}
.reg-small{margin:10px 0 0;font-size:12px;line-height:1.45;color:var(--muted)}
.coverage-stack{display:flex;height:12px;margin:18px 0 8px;background:var(--track);overflow:hidden;gap:2px}
.seg{display:block;height:100%;min-width:2px}
.seg.builtin{background:var(--graphite)}
.seg.script{background:var(--cherry)}
.seg.other{background:var(--rule-strong)}
.stack-legend{display:flex;flex-wrap:wrap;gap:4px 14px;font-size:11.5px;line-height:1.4;color:var(--ink-2)}
.stack-legend span{display:inline-flex;align-items:center;gap:6px;white-space:nowrap}
.swatch{display:inline-block;width:8px;height:8px;background:var(--rule-strong);flex:0 0 auto}
.swatch.builtin{background:var(--graphite)}.swatch.script{background:var(--cherry)}.swatch.other{background:var(--rule-strong)}
.gap-note{margin:16px 0 0;padding-top:12px;border-top:1px solid var(--rule);max-width:65ch;font-size:12.5px;line-height:1.5;color:var(--ink-2)}
.gap-note strong{color:var(--ink)}
.process-chips{display:flex;flex-wrap:wrap;gap:4px 14px;margin-top:12px;font-size:11.5px;line-height:1.4;color:var(--muted)}
.process-chip b{font-weight:600;color:var(--ink-2);font-variant-numeric:tabular-nums}
.activity-process{margin:0 0 18px}
.progress-cell{display:block;color:var(--ink);text-decoration:none}
.progress-cell .reg-label,.progress-cell .reg-figure,.progress-cell .reg-sub{display:block}
.progress-cell .progress-track>span{display:block;height:100%;background:var(--graphite)}
.progress-cell:hover{background:var(--paper-2)}
.progress-cell:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
.progress-alpha .progress-track>span{background:var(--cherry)}
.progress-link{display:inline-block;margin-top:10px;color:var(--cherry);font-size:12.5px;line-height:1.4;text-decoration:underline;text-decoration-color:var(--rule-strong);text-underline-offset:.16em}
.progress-cell:hover .progress-link{text-decoration-color:var(--cherry)}
.progress-note{margin:12px 0 0;max-width:65ch;font-size:12.5px;line-height:1.5;color:var(--muted)}
.scope-line{margin:10px 0 0;max-width:80ch;font-size:12.5px;line-height:1.55;color:var(--ink-2)}
.scope-line .reg-label{display:inline;margin-right:8px}
.port-meter{display:flex;gap:3px;height:8px;margin-top:0;max-width:none;background:none;overflow:visible}
.port-step{flex:1;min-width:4px;background:var(--track);border:1px solid var(--rule)}
.port-step.filled{background:var(--graphite);border-color:var(--graphite)}
.game-facts .port-meter{margin-top:8px;max-width:260px}
.row-progress{grid-column:1/-1;display:grid;grid-template-columns:auto minmax(0,1fr) auto;gap:8px 12px;align-items:center;margin-top:8px}
.row-progress-label{font-size:11px;line-height:1.3;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500}
.row-progress-state{font-size:11.5px;line-height:1.4;color:var(--ink-2);font-variant-numeric:tabular-nums;white-space:nowrap}
.impl-bars{grid-column:1/-1;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px 24px;margin-top:10px}
.impl-row{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:2px 8px;align-items:baseline;min-width:0}
.impl-label{grid-column:1;font-size:11px;line-height:1.3;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500;white-space:nowrap}
.impl-track{grid-column:1/-1;grid-row:2;display:block;height:8px;min-width:40px;margin-top:2px;background:var(--track);border:1px solid var(--rule);overflow:hidden}
.impl-track>span{display:block;height:100%;background:var(--graphite)}
.impl-row.package .impl-track>span{background:var(--ink-2)}
.impl-row.scripted .impl-track>span{background:var(--cherry)}
.impl-track.is-missing{border-color:var(--rule-strong)}
.impl-value{grid-column:2;font-size:11.5px;line-height:1.35;color:var(--ink-2);font-variant-numeric:tabular-nums;text-align:right;white-space:normal}
.impl-value small{display:inline;margin-left:6px;font-size:11px;color:var(--muted)}
.impl-value.is-missing{color:var(--muted);white-space:normal}
.fact-source{font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;font-size:11px;color:var(--muted)}
.api-surface{margin:0 0 26px;padding:16px 0 14px;border-top:1px solid var(--rule-strong);border-bottom:1px solid var(--rule-strong)}
.api-head{display:flex;justify-content:space-between;align-items:baseline;gap:10px 24px;flex-wrap:wrap}
.api-subhead{margin-top:20px;padding-top:14px;border-top:1px solid var(--rule)}
.api-note{margin:10px 0 0;max-width:65ch;font-size:12.5px;line-height:1.55;color:var(--ink-2)}
.api-note strong{color:var(--ink)}
.api-groups{list-style:none;margin:10px 0 0;padding:0;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:4px 24px;font-size:12px;line-height:1.45;color:var(--ink-2)}
.api-groups b{font-weight:600;color:var(--ink)}
.api-counts{color:var(--muted);font-variant-numeric:tabular-nums}
.section{padding-top:clamp(58px,7vw,84px);scroll-margin-top:20px}
.section-head{display:grid;grid-template-columns:minmax(170px,.72fr) minmax(0,1.28fr);gap:clamp(24px,4vw,52px);align-items:start;border-top:2px solid var(--ink);padding-top:18px;margin-bottom:28px}
.section-head h2{font-family:"Barlow Condensed",sans-serif;font-size:clamp(38px,4vw,50px);line-height:.95;letter-spacing:-.015em;font-weight:700;margin:0}
.section-head p{margin:2px 0 0;max-width:58ch;color:var(--ink-2);font-size:15px;line-height:1.55}
.note{font-size:12.5px;line-height:1.5;color:var(--muted);margin:12px 0 0}
.filters{display:grid;grid-template-columns:minmax(0,1fr) 210px auto;gap:18px 24px;align-items:end;padding:14px 0 15px;border-top:1px solid var(--rule);border-bottom:1px solid var(--rule-strong)}
.field{display:flex;flex-direction:column;gap:5px;min-width:0}
.field label,.check-label{font-size:11px;letter-spacing:.1em;text-transform:uppercase;color:var(--muted);font-weight:500}
.field input,.field select{width:100%;min-height:38px;border:0;border-bottom:1px solid var(--rule-strong);border-radius:0;background:transparent;color:var(--ink);font:400 15px/1.3 "Barlow",sans-serif;padding:6px 0}
.field input::placeholder{color:var(--muted);opacity:1}
.field select{cursor:pointer}
.check{display:flex;align-items:center;gap:8px;padding-bottom:8px;font-size:13px;line-height:1.35;color:var(--ink-2);min-height:38px}
.check input{width:16px;height:16px;margin:0;accent-color:var(--cherry);flex:0 0 auto}
.filter-status{grid-column:1/-1;margin:0;font-size:12px;line-height:1.4;color:var(--muted);font-variant-numeric:tabular-nums}
.noscript{margin:0 0 18px;padding:10px 0;border-top:1px solid var(--rule);border-bottom:1px solid var(--rule);font-size:12.5px;color:var(--ink-2)}
.composition{margin:0 0 26px;padding:16px 0 14px;border-top:1px solid var(--ink);border-bottom:1px solid var(--rule-strong)}
.composition-head{display:flex;justify-content:space-between;align-items:baseline;gap:10px 24px;flex-wrap:wrap}
.composition-figure{font-family:"Barlow",sans-serif;font-size:14px;line-height:1.3;font-weight:600;color:var(--ink-2);font-variant-numeric:tabular-nums}
.composition .coverage-stack{margin:14px 0 8px}
.composition-note{margin:10px 0 0;max-width:66ch;font-size:12.5px;line-height:1.55;color:var(--ink-2)}
.composition-note strong{color:var(--ink)}
.game-menu-head{display:grid;grid-template-columns:minmax(0,1.45fr) minmax(0,1.1fr) minmax(0,.95fr) minmax(0,1.1fr) minmax(0,1fr) 18px;gap:18px;padding:8px 0;border-bottom:1px solid var(--ink);font-size:11px;line-height:1.3;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);font-weight:500}
.game-menu{list-style:none;margin:0;padding:0}
.game-item{border-bottom:1px solid var(--rule)}
.game-entry>summary{list-style:none}
.game-entry>summary::-webkit-details-marker{display:none}
.game-summary{display:grid;grid-template-columns:minmax(0,1.45fr) minmax(0,1.1fr) minmax(0,.95fr) minmax(0,1.1fr) minmax(0,1fr) 18px;gap:18px;align-items:center;padding:16px 0;cursor:pointer;position:relative}
.game-summary:hover{background:var(--paper-2)}
.game-summary:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
.game-identity{min-width:0;padding-right:24px}
.game-order{display:inline-block;min-width:22px;font-size:11px;color:var(--muted);font-variant-numeric:tabular-nums}
.game-name{font-size:14px;line-height:1.3;font-weight:500;color:var(--ink);overflow-wrap:anywhere}
.game-meta{display:block;margin-top:2px;font-size:11.5px;line-height:1.35;color:var(--muted);overflow-wrap:anywhere}
.metric-label{position:absolute!important;width:1px!important;height:1px!important;padding:0!important;margin:-1px!important;overflow:hidden!important;clip:rect(0,0,0,0)!important;white-space:nowrap!important;border:0!important}
.metric-value{display:block;font-size:12.5px;line-height:1.45;color:var(--ink-2);font-variant-numeric:tabular-nums;overflow-wrap:anywhere}
.metric-value b{font-weight:600;color:var(--ink)}
.game-composition .metric-value{font-size:11.5px;color:var(--muted)}
.game-toggle{position:relative;width:16px;height:16px;justify-self:end;flex:0 0 auto}
.game-toggle::before,.game-toggle::after{content:"";position:absolute;background:var(--ink-2);transition:opacity .15s ease}
.game-toggle::before{width:12px;height:1px;left:2px;top:7px}
.game-toggle::after{width:1px;height:12px;left:7px;top:2px}
.game-entry[open] .game-toggle::after{opacity:0}
.game-detail{padding:0 0 22px}
.game-facts{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px 24px;margin:0;padding:14px 0 0;border-top:1px solid var(--rule)}
.game-facts dt{font-size:11px;line-height:1.35;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500}
.game-facts dd{margin:3px 0 0;font-size:13px;line-height:1.45;color:var(--ink-2);font-variant-numeric:tabular-nums;overflow-wrap:anywhere}
.game-facts dd b{font-weight:600;color:var(--ink)}
.game-explain{margin:16px 0 0;padding-top:12px;border-top:1px solid var(--rule);max-width:65ch;font-size:12.5px;line-height:1.55;color:var(--ink-2)}
.game-explain strong{color:var(--ink)}
.game-links{margin:10px 0 0;font-size:12.5px;line-height:1.5;color:var(--muted)}
.game-links a{overflow-wrap:anywhere}
.state{display:inline-flex;align-items:center;gap:5px;font-size:11px;line-height:1.4;text-transform:uppercase;letter-spacing:.06em;white-space:nowrap;color:var(--ink-2)}
.state-mark{display:inline-block;width:7px;height:7px;border:1px solid currentColor;flex:0 0 auto}
.state-open{color:var(--warn)}.state-open .state-mark{border-color:var(--warn)}
.state-closed{color:var(--good)}.state-closed .state-mark{background:var(--good);border-color:var(--good)}
.missing{color:var(--muted)}
.source-note{margin:14px 0 0;max-width:65ch;font-size:12px;line-height:1.55;color:var(--muted)}
.game-source{margin:8px 0 0;max-width:65ch;font-size:12px;line-height:1.5;color:var(--muted)}
.source-note code{font-size:.96em}
code{font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;background:var(--paper-2);padding:1px 5px;border-bottom:1px solid var(--rule);font-size:.92em;overflow-wrap:anywhere}
.glossary{margin-top:34px;border-top:1px solid var(--rule-strong);padding-top:16px}
.glossary summary{font-family:"Barlow Condensed",sans-serif;font-size:26px;line-height:1;font-weight:600;cursor:pointer;color:var(--ink)}
.glossary summary:hover{color:var(--cherry)}
.glossary dl{display:grid;grid-template-columns:1fr 1fr;gap:14px 32px;margin:18px 0 0}
.glossary dt{font-weight:600;font-size:13.5px;color:var(--ink)}
.glossary dd{margin:2px 0 0;font-size:13px;line-height:1.5;color:var(--ink-2)}
.delivery-summary{display:flex;flex-wrap:wrap;gap:4px 22px;margin:0 0 18px;font-size:13.5px;line-height:1.5;color:var(--ink-2);font-variant-numeric:tabular-nums}
.delivery-summary b{font-weight:600;color:var(--ink)}
.phase-list{list-style:none;margin:0;padding:0;border-top:1px solid var(--rule-strong)}
.phase{display:grid;grid-template-columns:minmax(170px,220px) minmax(0,1fr);gap:clamp(22px,4vw,44px);padding:22px 0 24px;border-bottom:1px solid var(--rule)}
.phase-name{min-width:0}
.phase-kicker{font-size:11px;line-height:1;letter-spacing:.1em;text-transform:uppercase;color:var(--muted);font-weight:500}
.phase-name h3{font-family:"Barlow Condensed",sans-serif;font-size:27px;line-height:1;font-weight:600;margin:6px 0 0;letter-spacing:-.005em}
.phase-area{margin:6px 0 0;font-size:11.5px;line-height:1.4;color:var(--muted);overflow-wrap:anywhere}
.alpha-tag{display:inline-block;margin-top:9px;padding-bottom:1px;border-bottom:1px solid var(--cherry);color:var(--cherry);font-size:11px;line-height:1.4;letter-spacing:.07em;text-transform:uppercase;font-weight:500}
.phase-body{min-width:0}
.phase-values{display:flex;justify-content:space-between;gap:12px;font-size:13px;line-height:1.4;color:var(--ink-2);font-variant-numeric:tabular-nums}
.progress-track{display:block;height:8px;background:var(--track);margin:8px 0;overflow:hidden}
.progress-fill{display:block;height:100%;background:var(--graphite)}
.phase.alpha .progress-fill{background:var(--cherry)}
.phase-meta{display:flex;flex-wrap:wrap;gap:4px 16px;font-size:11.5px;line-height:1.4;color:var(--muted);font-variant-numeric:tabular-nums}
.gate{margin-top:9px;font-size:13px;line-height:1.5;color:var(--ink-2)}
.gate summary{cursor:pointer;color:var(--ink-2);font-size:12px;line-height:1.5;padding:6px 0;min-height:24px;text-decoration:underline;text-decoration-color:var(--rule-strong);text-underline-offset:.16em}
.gate summary:hover{color:var(--cherry)}
.gate dl{display:grid;grid-template-columns:118px minmax(0,1fr);gap:8px 16px;margin:12px 0 0;padding-top:12px;border-top:1px solid var(--rule)}
.gate dl div{display:contents}
.gate dt{font-size:11px;line-height:1.4;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);padding-top:2px}
.gate dd{margin:0;font-size:13px;line-height:1.5}
.dispatch{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));border-top:1px solid var(--rule-strong);border-bottom:1px solid var(--rule-strong)}
.dispatch-col{padding:20px 20px 20px 0;min-width:0}
.dispatch-col+.dispatch-col{border-left:1px solid var(--rule);padding-left:20px}
.dispatch-col h3{font-size:11px;line-height:1.3;letter-spacing:.09em;text-transform:uppercase;font-weight:600;color:var(--ink);margin:0 0 10px}
.issue-list{list-style:none;margin:0;padding:0}
.issue-list li{display:grid;grid-template-columns:auto minmax(0,1fr);gap:2px 8px;padding:12px 0;border-top:1px solid var(--rule);font-size:12.5px;line-height:1.35}
.issue-list li:first-child{border-top:0;padding-top:12px}
.issue-phase{grid-column:1/-1;font-size:11px;line-height:1.3;letter-spacing:.07em;text-transform:uppercase;color:var(--muted)}
.issue-number{font-weight:600;white-space:nowrap;font-variant-numeric:tabular-nums}
.issue-title{color:var(--ink-2);min-width:0;overflow-wrap:anywhere}
.empty{margin:8px 0 0;font-size:12.5px;line-height:1.45;color:var(--muted)}
.analysis{display:grid;grid-template-columns:1fr 1fr;gap:0;margin-top:34px;border-top:1px solid var(--rule-strong)}
.analysis>section{padding-top:20px;min-width:0}
.analysis>section+section{border-left:1px solid var(--rule);padding-left:28px;margin-left:28px}
.analysis h3{font-family:"Barlow Condensed",sans-serif;font-size:28px;line-height:1;font-weight:600;margin:0 0 7px}
.analysis .note{margin-top:0}
.area-chart{margin-top:16px}
.area-row{display:grid;grid-template-columns:minmax(78px,138px) minmax(0,1fr) 38px;gap:10px;align-items:center;padding:6px 0;font-size:12.5px;line-height:1.3}
.area-label{color:var(--ink-2);overflow-wrap:anywhere}
.area-track{display:block;height:12px;background:var(--track);overflow:hidden}
.area-fill{display:block;height:100%;background:var(--graphite)}
.area-count{text-align:right;font-variant-numeric:tabular-nums;color:var(--ink-2)}
.flow-table{width:100%;border-collapse:collapse;margin-top:16px;font-size:12.5px;line-height:1.35;table-layout:fixed}
.flow-table th,.flow-table td{padding:7px 5px;border-bottom:1px solid var(--rule);vertical-align:middle;text-align:left}
.flow-table thead th{border-bottom:1px solid var(--rule-strong);font-size:11px;line-height:1.25;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500}
.flow-table th[scope="row"]{font-weight:500;color:var(--ink-2);white-space:nowrap;font-variant-numeric:tabular-nums;width:64px}
.bar-cell{display:flex;align-items:center;gap:7px;min-width:0}
.bar-track{display:block;height:8px;background:var(--track);flex:1 1 auto;min-width:24px;overflow:hidden}
.bar-fill{display:block;height:100%}
.bar-fill.opened{background:var(--graphite)}
.bar-fill.closed{background:var(--cherry)}
.bar-value{font-variant-numeric:tabular-nums;min-width:3ch;text-align:right;color:var(--ink-2)}
.history-section{margin-top:48px;padding-top:18px;border-top:2px solid var(--ink)}
.history-section h3{font-family:"Barlow Condensed",sans-serif;font-size:30px;line-height:1;font-weight:600;margin:0 0 7px}
.history-table{width:100%;border-collapse:collapse;margin-top:16px;font-size:12.5px;line-height:1.4;table-layout:fixed}
.history-table th,.history-table td{padding:9px 6px;border-bottom:1px solid var(--rule);text-align:left;vertical-align:top}
.history-table thead th{border-bottom:1px solid var(--rule-strong);font-size:11px;line-height:1.25;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500}
.history-table th[scope="row"]{font-weight:400;color:var(--ink-2);font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace;font-size:11px;overflow-wrap:anywhere}
.history-table td.num,.history-table th.num{text-align:right;font-variant-numeric:tabular-nums}
.sources{margin-top:clamp(66px,8vw,96px);border-top:2px solid var(--ink);padding-top:22px;padding-bottom:44px;scroll-margin-top:20px}
.sources h2{font-family:"Barlow Condensed",sans-serif;font-size:clamp(34px,3.6vw,44px);line-height:.95;font-weight:700;margin:0 0 22px}
.source-grid{display:grid;grid-template-columns:1.2fr 1fr 1fr;gap:clamp(24px,4vw,44px)}
.source-block h3{font-size:11px;line-height:1.3;letter-spacing:.09em;text-transform:uppercase;font-weight:600;color:var(--ink);margin:0 0 8px}
.source-block p,.source-block li{font-size:13px;line-height:1.5;color:var(--ink-2)}
.source-block p{margin:0 0 8px}
.source-block ul{list-style:none;margin:0;padding:0}
.source-block li{padding:12px 0;border-top:1px solid var(--rule)}
.source-block li:first-child{border-top:0;padding-top:0}
.source-block li a{overflow-wrap:anywhere}
.footer-bottom{display:flex;flex-wrap:wrap;justify-content:space-between;gap:8px 20px;margin-top:30px;padding-top:12px;border-top:1px solid var(--rule);font-size:11px;line-height:1.4;color:var(--muted)}
.footer-bottom span{overflow-wrap:anywhere}
@media (prefers-reduced-motion:no-preference){html{scroll-behavior:smooth}}
@media (max-width:1280px){
  .stage{display:block;min-height:0}
  .spine{position:static;height:auto;max-height:none;overflow:visible;padding:16px 28px;border-right:0;border-bottom:1px solid var(--rule-strong);display:flex;flex-direction:row;flex-wrap:wrap;align-items:center;gap:10px 24px}
  .wordmark{display:flex;align-items:baseline;gap:6px;font-size:24px}
  .wordmark span{display:inline}
  .wordmark span+span{font-size:22px}
  .spine nav{margin:0 0 0 auto;flex-direction:row;flex-wrap:wrap}
  .spine nav a{padding:6px 10px;border-top:0;border-left:1px solid var(--rule);font-size:13px}
  .spine nav a:first-child{border-left:0}
  .nav-note{display:none}
  .spine-meta{display:none}
  .sheet{padding:0 28px}
}
@media (max-width:1000px){
  .game-menu-head{display:none}
  .game-summary{grid-template-columns:1fr 1fr;gap:10px 18px;padding:14px 28px 14px 0}
  .game-identity{grid-column:1/-1;padding-right:0}
  .game-toggle{position:absolute;right:0;top:16px}
  .metric-label{position:static!important;width:auto!important;height:auto!important;padding:0!important;margin:0 0 2px!important;overflow:visible!important;clip:auto!important;white-space:normal!important;border:0!important;display:block;font-size:11px;line-height:1.3;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);font-weight:500}
  .game-facts{grid-template-columns:1fr 1fr}
  .impl-bars{grid-template-columns:1fr}
  .game-summary:hover{background:transparent}
}
@media (max-width:760px){
  .masthead-top{align-items:flex-start}
  .theme{width:100%;justify-content:flex-start}
  .title-row{grid-template-columns:1fr;gap:26px;padding-top:32px}
  h1{max-width:17ch}
  .status-slab{border-left:0;border-top:1px solid var(--rule);padding-left:0;padding-top:20px;display:grid;grid-template-columns:auto minmax(0,1fr);gap:0 24px;align-items:start}
  .seal{grid-row:1}
  .status-detail{margin:2px 0 14px}
  .provenance{grid-column:1/-1;border-top:1px solid var(--rule);padding-top:12px}
  .register{grid-template-columns:1fr}
  .reg-cell+.reg-cell{border-left:0;border-top:1px solid var(--rule);padding-left:0;margin-left:0;padding-top:20px}
  .section-head{grid-template-columns:1fr;gap:12px}
  .filters{grid-template-columns:1fr}
  .filter-status{grid-column:auto}
  .check{padding-bottom:0}
  .game-facts{grid-template-columns:1fr}
  .impl-bars{grid-template-columns:1fr}
  .impl-label{white-space:normal}
  .impl-value{white-space:normal}
  .api-groups{grid-template-columns:1fr}
  .glossary dl{grid-template-columns:1fr}
  .phase{grid-template-columns:1fr;gap:12px}
  .phase-name h3{font-size:25px}
  .dispatch{grid-template-columns:1fr 1fr}
  .dispatch-col:nth-child(3){border-left:0;padding-left:0}
  .dispatch-col:nth-child(n+3){border-top:1px solid var(--rule);padding-top:18px}
  .analysis{grid-template-columns:1fr}
  .analysis>section+section{border-left:0;padding-left:0;margin-left:0;border-top:1px solid var(--rule);padding-top:18px;margin-top:22px}
  .source-grid{grid-template-columns:1fr}
  .history-table thead{position:absolute!important;width:1px!important;height:1px!important;padding:0!important;margin:-1px!important;overflow:hidden!important;clip:rect(0,0,0,0)!important;white-space:nowrap!important;border:0!important}
  .history-table,.history-table tbody{display:block;width:100%}
  .history-table tr{display:grid;grid-template-columns:1fr 1fr;gap:0 18px;border-bottom:1px solid var(--rule);padding:10px 0}
  .history-table th[scope="row"]{grid-column:1/-1;padding:0 0 5px;border:0}
  .history-table td{display:block;border:0;padding:5px 0;text-align:left}
  .history-table td::before{content:attr(data-label);display:block;font-size:11px;line-height:1.3;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);margin-bottom:2px}
  .history-table td.num{text-align:left}
}
@media (max-width:520px){
  .spine{display:block;padding:12px 20px;gap:0}
  .wordmark{display:flex;align-items:baseline;gap:6px;font-size:24px}
  .wordmark span{display:inline}
  .wordmark span+span{font-size:22px}
  .spine nav{margin:9px 0 0;display:flex;flex-wrap:nowrap}
  .spine nav a{flex:1 1 auto;padding:6px 0;border-left:1px solid var(--rule);border-top:0;text-align:center;font-size:12px}
  .spine nav a:first-child{border-left:0}
  .sheet{padding:0 20px}
  .masthead-meta{display:none}
  .masthead-top{justify-content:flex-start;padding-bottom:10px}
  .theme{justify-content:flex-start}
  .theme-options button{padding:8px 8px}
  .title-row{padding:22px 0 20px;gap:18px}
  .lede{margin-top:14px;font-size:15.5px}
  h1{font-size:clamp(38px,11vw,52px)}
  .status-slab{grid-template-columns:auto minmax(0,1fr);gap:0 16px}
  .seal{font-size:18px;padding:7px 10px}
  .status-detail{margin:2px 0 8px;font-size:13.5px}
  .provenance{grid-column:1/-1;display:flex;flex-wrap:wrap;gap:2px 10px;align-items:baseline;padding-top:8px}
  .reg-figure{font-size:58px}
  .game-summary{padding-top:12px;padding-bottom:12px}
  .row-progress{grid-template-columns:1fr auto}
  .row-progress .port-meter{grid-column:1/-1;grid-row:2}
  .game-facts{grid-template-columns:1fr}
  .dispatch{grid-template-columns:1fr}
  .dispatch-col+.dispatch-col{border-left:0;padding-left:0;border-top:1px solid var(--rule);padding-top:18px}
  .gate dl{grid-template-columns:1fr}
  .gate dl div{display:block;padding-top:6px}
  .gate dt{padding-top:0}
  .flow-table{font-size:11.5px}
  .flow-table th[scope="row"]{width:46px}
  .bar-cell{gap:4px}
  .bar-value{min-width:2ch}
  .footer-bottom{display:block}
  .footer-bottom span{display:block;margin-top:4px}
}
@media (max-width:350px){
  .status-slab{grid-template-columns:1fr}
  .seal{grid-row:auto}
  .status-detail{margin-top:12px}
  .provenance{grid-column:auto;display:flex;flex-wrap:wrap;gap:2px 8px}
}
</style>
</head>
<body>
<a class="skip-link" href="#main">Skip to the register</a>
<div class="stage">
  <aside class="spine" aria-label="Project and contents">
    <a class="wordmark" href="#top"><span>IWBTG</span><span>Collection</span></a>
    <nav aria-label="Sections">
      <a href="#coverage"><span class="nav-name">Coverage</span><span class="nav-note">games, objects, traces</span></a>
      <a href="#delivery"><span class="nav-name">Delivery</span><span class="nav-note">phase gates and ports</span></a>
      <a href="#activity"><span class="nav-name">Activity</span><span class="nav-note">issues, areas, history</span></a>
      <a href="#sources"><span class="nav-name">Sources</span><span class="nav-note">method and follow-ups</span></a>
    </nav>
    <div class="spine-meta">
      <span>Generated __GENERATED__</span>
      <span>Source: __SOURCE__</span>
    </div>
  </aside>
  <div class="sheet" id="top">
    <header class="masthead">
      <div class="masthead-top">
        <div class="masthead-meta">
          <span>Generated <b>__GENERATED__</b></span>
          <span>Source <b>__SOURCE__</b></span>
        </div>
        <div class="theme">
          <span class="theme-label" id="theme-label">Theme</span>
          <div class="theme-options" role="group" aria-labelledby="theme-label">
            <button type="button" data-theme-choice="system" aria-pressed="true">System</button>
            <button type="button" data-theme-choice="light" aria-pressed="false">Light</button>
            <button type="button" data-theme-choice="dark" aria-pressed="false">Dark</button>
          </div>
          <span class="visually-hidden" id="theme-status" aria-live="polite"></span>
        </div>
      </div>
      <div class="title-row">
        <div>
          <h1>Rebuilding I Wanna Be The Guy fangames as readable packages.</h1>
          <p class="lede">An open-source client being built to play __GAMES_TOTAL__ fangames without their original binaries. Games are re-authored as TOML data plus sandboxed Rhai scripts, then checked against recorded traces; this register reports what is measured, what is open, and what is not measured yet.</p>
        </div>
        <aside class="status-slab" aria-label="Project status">
          <span class="seal">__STATUS__</span>
          <p class="status-detail">__STATUS_DETAIL__</p>
          <dl class="provenance">
            <dt>Generated</dt><dd>__GENERATED__</dd>
            <dt>Source</dt><dd>__SOURCE__</dd>
          </dl>
        </aside>
      </div>
      <section class="register progress-register" id="progress" aria-label="Project progress">
        <a class="reg-cell progress-cell" href="#delivery">
          <span class="reg-label">Issues closed · all phases</span>
          <span class="reg-figure">__ISSUE_PCT__</span>
          <span class="progress-track" aria-hidden="true"><span style="width:__ISSUE_WIDTH__"></span></span>
          <span class="reg-sub">__CLOSED__ of __ISSUES__ · __OPEN__ open · __POINTS__ size points left</span>
          <span class="progress-link">Open the phase ladder</span>
        </a>
        <a class="reg-cell progress-cell" href="#coverage">
          <span class="reg-label">Port checklists · all games</span>
          <span class="reg-figure">__PORT_PCT__</span>
          <span class="progress-track" aria-hidden="true"><span style="width:__PORT_WIDTH__"></span></span>
          <span class="reg-sub">__PORT_DONE__ of __PORT_TOTAL__ Done when items across __GAMES_TOTAL__ ports</span>
          <span class="progress-link">Open the game menu</span>
        </a>
        <a class="reg-cell progress-cell progress-alpha" href="#delivery">
          <span class="reg-label">Public alpha gate · __PHASE7_NAME__</span>
          <span class="reg-figure">__PHASE7_PCT__</span>
          <span class="progress-track" aria-hidden="true"><span style="width:__PHASE7_WIDTH__"></span></span>
          <span class="reg-sub">__PHASE7_CLOSED__ of __PHASE7_TOTAL__ issues closed · __PHASE7_POINTS__ points left</span>
          <span class="progress-link">See all phase gates</span>
        </a>
      </section>
      <p class="progress-note">Direct ratios from the issue and port data; not a single completion score or a release date. The per-game implementation bars sit in Coverage.</p>
      <p class="scope-line"><span class="reg-label">Scope</span> __GAMES_TOTAL__ games · __SCRIPT__ script instances · __CUSTOM__ custom object types · __ROOMS__ rooms · __TRACES__ recorded traces across __GAMES_TRACES__ of __GAMES_TOTAL__ games · __EXERCISED__ objects exercised (__EXERCISED_UNIQUE__ distinct names).</p>
      <p class="gap-note"><strong>Passing-trace coverage is not measured yet.</strong> The follow-up is <a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">issue #__TRACEFU__</a>.</p>
    </header>

    <main id="main">
      <section class="section" id="coverage" aria-labelledby="coverage-title">
        <div class="section-head">
          <h2 id="coverage-title">Coverage</h2>
          <p>What is built-in, what is game-specific, and what evidence exists. Each game row carries three repo-derived implementation bars — <b>trace cases passing</b>, <b>mapped objects</b> and <b>scripted objects</b> — plus its port checklist. The menu leads with each game's <b>game-specific work</b> and keeps built-in coverage as composition, not completion. Passing-trace coverage of individual objects is still not measured yet; it needs the trace runner (<a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">#__TRACEFU__</a>).</p>
        </div>
        <section class="composition" aria-label="Composition of placed instances">
          <div class="composition-head">
            <span class="reg-label">Composition of __PLACED__ placed instances</span>
            <span class="composition-figure">__BUILTIN_PCT__ built-in</span>
          </div>
          __STACK__
          <div class="stack-legend">
            <span><i class="swatch builtin"></i>Built-in __BUILTIN__</span>
            <span><i class="swatch script"></i>Script __SCRIPT__ (__SCRIPT_PCT__)</span>
            <span><i class="swatch other"></i>Client / kid __OTHER__</span>
          </div>
          <p class="composition-note"><strong>These are composition shares, not completion.</strong> Placed instances count every placement, not just distinct object types; a low script share is normal. The game-specific work is each game's script instances and custom object types.</p>
        </section>
        <section class="api-surface" aria-label="Implementation measures and script API census">
          <div class="api-head">
            <span class="reg-label">Implementation measures</span>
            <span class="composition-figure">__PACKAGE_HEAD__</span>
          </div>
          <p class="api-note">Each game row's <b>Trace cases</b> bar is OK cases over committed cases in <code>crates/tools/cases/</code> and <code>crates/tools/fidelity-baseline.txt</code>. Its <b>Mapped objects</b> and <b>Scripted objects</b> bars partition the converted package: mapped resolves to a built-in behavior spec, scripted is written by the package as a script. Package data is the committed exporter snapshot of <b>__EXPORTER_DATE__</b>: <code>progress/exporter.json</code> is generated in the research environment and is not regenerated here. __IMPLEMENTATION_SUMMARY__</p>
          __API_SURFACE__
        </section>
        <noscript><p class="noscript">Filtering and the theme choice need JavaScript. Every game, figure and note in this register is still present and readable below.</p></noscript>
        <div class="filters">
          <div class="field">
            <label for="q">Find a game</label>
            <input id="q" type="search" placeholder="Name or slug" autocomplete="off" spellcheck="false" aria-controls="games">
          </div>
          <div class="field">
            <label for="sort">Order</label>
            <select id="sort" aria-controls="games">
              <option value="order">Shortlist order</option>
              <option value="custom">Custom object types (most first)</option>
              <option value="script">Script instances (most first)</option>
              <option value="cases">Trace cases passing (high first)</option>
              <option value="cov">Built-in coverage (low first)</option>
              <option value="covdesc">Built-in coverage (high first)</option>
              <option value="traces">Recorded traces</option>
              <option value="name">Name</option>
            </select>
          </div>
          <label class="check" for="traces"><input id="traces" type="checkbox" aria-controls="games"> Only games with recorded traces</label>
          <p class="filter-status" id="filter-status" aria-live="polite">Showing __GAMES_TOTAL__ of __GAMES_TOTAL__ games, in shortlist order.</p>
        </div>
        <div class="game-menu-head" aria-hidden="true">
          <span>Game</span><span>Game-specific work</span><span>Evidence</span><span>Port</span><span>Composition</span><span></span>
        </div>
        <ul class="game-menu" id="games">__COVERAGE_ROWS__</ul>
        <p class="source-note">Placed instances and built-in coverage: <code>specs/mechanics/coverage.md</code> (SCAN, 2026-10-24). Custom objects: <code>shortlist.md</code>. Traces and exercised objects: the recorded CSVs under <code>specs/engines/gm8/traces/&lt;slug&gt;/</code>. Trace cases: <code>crates/tools/cases/</code> and <code>crates/tools/fidelity-baseline.txt</code>. Mapped and scripted objects: <code>progress/exporter.json</code> (snapshot of __EXPORTER_DATE__) and the <code>docs/games</code> front-matter. <code>progress/export_research.py</code> regenerates the research-derived counts.</p>
        <details class="glossary" open>
          <summary>Terms in this register</summary>
          <dl>
            <div><dt>Placed instances</dt><dd>Every object instance placed in a room in the census. This is the denominator for built-in and script shares.</dd></div>
            <div><dt>Built-in coverage</dt><dd>The share of placed instances whose behavior a documented engine built-in covers. This is a composition share, not a completion score.</dd></div>
            <div><dt>Script-object instances</dt><dd>Placed instances whose behavior comes from a game-specific package script instead of a built-in. The script share is a share of all placed instances, not a progress score.</dd></div>
            <div><dt>Recorded traces</dt><dd>Input and state recordings captured from the original. A count of evidence, not proof that behavior is tick-accurate.</dd></div>
            <div><dt>Objects exercised</dt><dd>Distinct object names that appear in those traces. Exercised does not mean tick-accurate.</dd></div>
            <div><dt>Custom object types</dt><dd>Distinct custom object types placed in the game, counted by the census from <code>shortlist.md</code>.</dd></div>
            <div><dt>Client / kid</dt><dd>Placed client objects and kid instances that are neither covered by a built-in nor counted as script objects.</dd></div>
            <div><dt>Trace cases passing</dt><dd>Committed trace cases whose status in <code>crates/tools/fidelity-baseline.txt</code> is <code>OK</code>, over the cases recorded in <code>crates/tools/cases/&lt;game&gt;/</code>. A case is one recorded scenario; a passing case is evidence, not proof that every object in it is tick-accurate.</dd></div>
            <div><dt>Mapped objects</dt><dd>Objects resolved to a built-in behavior spec over objects in the converted package, from <code>progress/exporter.json</code> and the <code>docs/games</code> front-matter. The snapshot is generated in the research environment; its date is shown beside the bars.</dd></div>
            <div><dt>Scripted objects</dt><dd>Objects the converted package writes as scripts over objects in the package. Mapped and scripted objects partition the package total.</dd></div>
            <div><dt>Passing-trace coverage</dt><dd>The share of individual objects proven tick-accurate by a passing trace. Not measured yet; it needs the trace runner (<a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">#__TRACEFU__</a>). This is different from the per-game <i>trace cases passing</i> bar.</dd></div>
            <div><dt>Not measured yet</dt><dd>A value the repo cannot supply honestly. The page names the missing input and, where one exists, the follow-up issue; it never prints zero or an estimate.</dd></div>
            <div><dt>Tick-accurate</dt><dd>The rebuilt object reproduces the original's behavior on every simulation tick, proven by a passing trace.</dd></div>
            <div><dt>Port</dt><dd>Each game's port issue in the games repository, with the progress of its <i>Done when</i> checklist.</dd></div>
            <div><dt>Phase gate</dt><dd>A milestone with entry and exit conditions. Delivery measures closed issues against the phase total and remaining size points, not a date.</dd></div>
            <div><dt>Size points</dt><dd>A planning weight from issue labels: S=1, M=3, L=8. An unlabelled issue counts 0 points. This is not time or a deadline.</dd></div>
            <div><dt>Process labels</dt><dd><code>agent-ready</code> can start without a blocker; <code>in-progress</code> is claimed; <code>needs-user</code> waits on a decision; <code>triage</code> is unsorted; <code>blocked</code> cannot proceed; <code>revisit</code> is deferred.</dd></div>
          </dl>
        </details>
      </section>

      <section class="section" id="delivery" aria-labelledby="delivery-title">
        <div class="section-head">
          <h2 id="delivery-title">Delivery</h2>
          <p>Phase gates run from Foundation to Release. A phase is a sequence, not a calendar; no deadlines are shown. The public alpha is planned to open at phase 7 (Online). Port issues and their <i>Done when</i> checklists stay in the coverage register's Port column.</p>
        </div>
        <p class="delivery-summary"><span><b>__OPEN__</b> open</span><span><b>__CLOSED__</b> closed of __ISSUES__ (__ISSUE_PCT__)</span><span><b>__POINTS__</b> remaining size points</span><span><b>__OPEN_PRS__</b> open PRs</span></p>
        <ol class="phase-list">__PHASE_ROWS__</ol>
        <p class="note">Phase progress is closed issues over the milestone total. Size points sum the open issues' size labels; an unlabelled issue counts 0. The roadmap has no deadlines.</p>
      </section>

      <section class="section" id="activity" aria-labelledby="activity-title">
        <div class="section-head">
          <h2 id="activity-title">Activity</h2>
          <p>What closed, what is claimed, what needs a decision and what an agent can start without a blocker. Each list is the most recent or highest-priority selection the generator makes, not the complete set of issues in that state.</p>
        </div>
        <div class="process-chips activity-process" aria-label="Open issues by process label">__PROCESS__</div>
        <div class="dispatch">
          <section class="dispatch-col" aria-labelledby="recent-title"><h3 id="recent-title">Recently closed</h3>__RECENT_CLOSED__</section>
          <section class="dispatch-col" aria-labelledby="progress-title"><h3 id="progress-title">In progress</h3>__IN_PROGRESS__</section>
          <section class="dispatch-col" aria-labelledby="waiting-title"><h3 id="waiting-title">Waiting on you</h3>__NEEDS_USER__</section>
          <section class="dispatch-col" aria-labelledby="ready-title"><h3 id="ready-title">Ready to start</h3>__READY__</section>
        </div>
        <div class="analysis">
          <section aria-labelledby="area-title">
            <h3 id="area-title">Open work by area</h3>
            <p class="note">An issue can carry more than one area label, so these counts do not sum to __OPEN__.</p>
            __AREA_BARS__
          </section>
          <section aria-labelledby="burn-title">
            <h3 id="burn-title">Issues over time</h3>
            <p class="note">Cumulative issues opened and closed at each weekly snapshot. Each count is printed beside its bar.</p>
            __BURNUP__
          </section>
        </div>
        <section class="history-section" id="history" aria-labelledby="history-title">
          <h3 id="history-title">Coverage history</h3>
          <p class="note">One dated point per workflow run, with its placed-instance denominator. Trace cases and package mapping appear from the run that first recorded them; older points are marked not measured yet, never zero.</p>
          __COVERAGE_HISTORY__
        </section>
      </section>

      <footer class="sources" id="sources" aria-labelledby="sources-title">
        <h2 id="sources-title">Sources and method</h2>
        <div class="source-grid">
          <section class="source-block">
            <h3>Reproduce this page</h3>
            <p>From the repository root:</p>
            <p><code>python -I progress/export_research.py --research ../iwbtg-collection-research</code></p>
            <p><code>python -I progress/generate.py --gh --history</code></p>
            <p>For a local re-render from the committed data, without refreshing it: <code>python -I progress/generate.py --render-only</code>.</p>
          </section>
          <section class="source-block">
            <h3>Data sources</h3>
            <ul>
              <li><code>progress/data.json</code> — last generated snapshot.</li>
              <li><code>progress/history.json</code> — one dated point per run.</li>
              <li><code>progress/exporter.json</code> — per-game census, trace and package counts (snapshot of __EXPORTER_DATE__).</li>
              <li><code>crates/tools/cases/</code> and <code>crates/tools/fidelity-baseline.txt</code> — trace cases and their blessed status.</li>
              <li><code>docs/format/script-api.md</code> and <code>docs/architecture/sim/scripts.md</code> — the registered script API and the census groups not yet registered.</li>
              <li><code>shortlist.md</code>, <code>specs/mechanics/coverage.md</code> and the recorded trace CSVs in the research repository.</li>
            </ul>
            <p>The source line records how the snapshot was refreshed: <code>gh</code> is the GitHub API; a <code>stale:</code> prefix means a refresh failed and the last committed data was kept.</p>
          </section>
          <section class="source-block">
            <h3>Follow-ups</h3>
            <ul>
              <li><a href="https://github.com/nugamus/iwbtg-collection/issues/__CENSUS__">#__CENSUS__</a> — census export.</li>
              <li><a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">#__TRACEFU__</a> — passing-trace coverage, not measured yet.</li>
            </ul>
          </section>
        </div>
        <div class="footer-bottom">
          <span>Generated __GENERATED__ · source __SOURCE__</span>
          <span>Static, self-contained page. No client-side API calls and no external requests. Fonts are local, credited OFL assets.</span>
        </div>
      </footer>
    </main>
  </div>
</div>
<script>
(function(){
  const root = document.documentElement;
  const key = 'iwbtg-theme';
  const buttons = [...document.querySelectorAll('[data-theme-choice]')];
  const status = document.getElementById('theme-status');
  function savedChoice(){
    try {
      const saved = localStorage.getItem(key);
      return (saved === 'light' || saved === 'dark' || saved === 'system') ? saved : 'system';
    } catch (e) { return 'system'; }
  }
  function apply(choice, persist){
    if (choice === 'light' || choice === 'dark') {
      root.dataset.theme = choice;
    } else {
      delete root.dataset.theme;
    }
    if (persist) {
      try { localStorage.setItem(key, choice); } catch (e) {}
    }
    buttons.forEach(button => {
      button.setAttribute('aria-pressed', String(button.dataset.themeChoice === choice));
    });
    if (status) {
      const label = choice === 'system' ? 'System theme' : choice.charAt(0).toUpperCase() + choice.slice(1) + ' theme';
      status.textContent = label + ' selected.';
    }
  }
  const initial = savedChoice();
  apply(initial, false);
  buttons.forEach(button => button.addEventListener('click', () => apply(button.dataset.themeChoice, true)));
})();
(function(){
  const q = document.getElementById('q');
  const sort = document.getElementById('sort');
  const traces = document.getElementById('traces');
  const status = document.getElementById('filter-status');
  const body = document.getElementById('games');
  if (!q || !sort || !traces || !body) return;
  const all = [...body.querySelectorAll('.game-item')];
  function num(item, key){ return parseFloat(item.dataset[key] || '0'); }
  function original(a, b){ return all.indexOf(a) - all.indexOf(b); }
  function apply(){
    const text = q.value.trim().toLowerCase();
    all.forEach(item => {
      const okText = !text || (item.dataset.name || '').includes(text);
      const okTrace = !traces.checked || item.dataset.traces === '1';
      item.hidden = !(okText && okTrace);
    });
    const visible = all.filter(item => !item.hidden);
    const mode = sort.value;
    visible.sort((a, b) => {
      if (mode === 'custom') return num(b, 'custom') - num(a, 'custom') || original(a, b);
      if (mode === 'script') return num(b, 'script') - num(a, 'script') || original(a, b);
      if (mode === 'cases') return num(b, 'cases') - num(a, 'cases') || original(a, b);
      if (mode === 'cov') return num(a, 'cov') - num(b, 'cov') || original(a, b);
      if (mode === 'covdesc') return num(b, 'cov') - num(a, 'cov') || original(a, b);
      if (mode === 'traces') return num(b, 'traces') - num(a, 'traces') || original(a, b);
      if (mode === 'name') return (a.dataset.name || '').localeCompare(b.dataset.name || '') || original(a, b);
      return original(a, b);
    });
    visible.forEach(item => body.appendChild(item));
    if (status) {
      const order = sort.options[sort.selectedIndex] ? sort.options[sort.selectedIndex].text : '';
      const traceNote = traces.checked ? ' Recorded traces only.' : '';
      status.textContent = 'Showing ' + visible.length + ' of ' + all.length + ' games, ordered by ' + order.toLowerCase() + '.' + traceNote;
    }
  }
  q.addEventListener('input', apply);
  sort.addEventListener('change', apply);
  traces.addEventListener('change', apply);
})();
</script>
</body>
</html>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gh", action="store_true", help="refresh from GitHub")
    ap.add_argument("--history", action="store_true", help="append a dated snapshot")
    ap.add_argument("--require-gh", action="store_true",
                    help="fail instead of falling back when --gh cannot read GitHub")
    ap.add_argument("--render-only", action="store_true",
                    help="re-render index.html from committed data and history without refreshing or writing them")
    args = ap.parse_args()
    if args.require_gh and not args.gh:
        raise SystemExit("--require-gh needs --gh")
    if args.render_only:
        if args.gh or args.history or args.require_gh:
            raise SystemExit("--render-only cannot be combined with --gh, --history or --require-gh")
        data_path = os.path.join(PROGRESS, "data.json")
        hist_path = os.path.join(PROGRESS, "history.json")
        data = json.load(open(data_path, encoding="utf-8"))
        hist = json.load(open(hist_path, encoding="utf-8")) if os.path.exists(hist_path) else []
        page = render(data, hist)
        open(os.path.join(PROGRESS, "index.html"), "w", encoding="utf-8", newline="").write(page)
        print(f"rendered progress/index.html ({len(data['games'])} games, "
              f"{len(data['phases'])} phases, source={data['source']})")
        return
    at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    data = None
    refreshed = False
    if args.gh:
        try:
            data = build_data(True)
            refreshed = True
        except Exception as exc:
            if args.require_gh:
                raise SystemExit(f"GitHub refresh failed: {exc}")
            print(f"warning: GitHub read failed ({exc}); using committed data.json", file=sys.stderr)
    if data is None:
        path = os.path.join(PROGRESS, "data.json")
        if os.path.exists(path):
            data = json.load(open(path, encoding="utf-8"))
            old_source = data.get("source", "committed data.json")
            data = refresh_local(data)
            data["source"] = (f"stale: {old_source}" if args.gh
                              else "committed data.json + local files")
        else:
            data = build_data(False)
            refreshed = True
    if refreshed:
        data["generated_at"] = at
    json.dump(data, open(os.path.join(PROGRESS, "data.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    hist_path = os.path.join(PROGRESS, "history.json")
    if args.history and refreshed:
        hist = maybe_history(data, at)
    else:
        hist = (json.load(open(hist_path, encoding="utf-8"))
                if os.path.exists(hist_path) else [])
    page = render(data, hist)
    open(os.path.join(PROGRESS, "index.html"), "w", encoding="utf-8", newline="").write(page)
    print(f"wrote progress/data.json, progress/index.html ({len(data['games'])} games, "
          f"{len(data['phases'])} phases, source={data['source']})")


if __name__ == "__main__":
    main()
