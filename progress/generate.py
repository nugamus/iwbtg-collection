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

Passing-trace coverage still needs converted packages and the trace runner; the
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
REPO = "nugamus/iwbtg-collection"
GAMES_REPO = "nugamus/iwbtg-collection-games"
SIZE_POINTS = {"size:S": 1, "size:M": 3, "size:L": 8}
FOLLOWUPS = {"census": 1364, "traces": 1365, "converter": 1366, "frontmatter": 1367}
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


def load_games():
    exporter = {}
    path = os.path.join(PROGRESS, "exporter.json")
    if os.path.exists(path):
        raw = json.load(open(path, encoding="utf-8"))
        exporter = raw.get("games", {}) if isinstance(raw, dict) else {}
    games = []
    for name in sorted(os.listdir(GAMES)):
        if not name.endswith(".md") or name == "README.md":
            continue
        fm = read_front_matter(os.path.join(GAMES, name))
        slug = fm.get("slug") or name[:-3]
        ex = exporter.get(slug, {}) or {}
        placed = as_int(ex.get("placed_instances"))
        builtin = as_int(ex.get("builtin_instances"))
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
            "objects_in_package": as_int(ex.get("objects_in_package")),
            "objects_unmapped": as_int(ex.get("objects_unmapped")),
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
            "port": None,
        }
        if rec["builtin_pct"] is None and placed and builtin is not None:
            rec["builtin_pct"] = round(100.0 * builtin / placed, 1)
        if rec["script_pct"] is None and placed and rec["script_instances"] is not None:
            rec["script_pct"] = round(100.0 * rec["script_instances"] / placed, 2)
        games.append(rec)
    games.sort(key=lambda g: g["order"])
    return games


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
        "games_converted": sum(1 for g in games if g.get("converted")),
        "games_converter_known": sum(1 for g in games if g.get("converted") is not None),
        "games_census": sum(1 for g in games if g["placed_instances"]),
        "games_traces": sum(1 for g in games if g["traces_recorded"]),
        "games_total": len(games),
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
    games = load_games()
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
            "overall": compute_overall(games), "followups": FOLLOWUPS}


def empty_activity():
    return {"totals": {"issues": 0, "open": 0, "closed": 0, "pct_closed": None},
            "open_prs": 0, "process": {}, "area_open": [], "recent_closed": [],
            "in_progress": [], "needs_user": [], "agent_ready": [],
            "closed_7d": 0, "opened_7d": 0, "burnup": []}


def refresh_local(data):
    games = load_games()
    old = {g.get("slug"): g for g in data.get("games", [])}
    for g in games:
        if g["slug"] in old:
            g["port"] = old[g["slug"]].get("port")
    data["games"] = games
    data["overall"] = compute_overall(games)
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
            "objects_exercised", "open_issues", "closed_issues", "issue_pct_closed", "size_points")
    if hist and all(hist[-1].get(k) == snap.get(k) for k in keep) and hist[-1]["at"][:10] == at[:10]:
        return hist
    hist.append(snap)
    json.dump(hist, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    return hist


# ----------------------------------------------------------------- render
def esc(value):
    return html.escape("" if value is None else str(value))


def mini_meter(value, label=""):
    v = max(0.0, min(100.0, float(value or 0)))
    return (f'<span class="mini-track" role="img" aria-label="{esc(label)} {v:.1f}%">'
            f'<span style="width:{v:.1f}%"></span></span>')


def coverage_chart(games):
    rows = sorted([g for g in games if g["builtin_pct"] is not None],
                  key=lambda g: g["builtin_pct"])
    def block(items):
        out = []
        for g in items:
            out.append(f"""
            <div class="covrow" title="{fmt(g['builtin_instances'])} of {fmt(g['placed_instances'])} placed instances">
              <span class="covname">{esc(g['name'])}<span class="covslug">{esc(g['slug'])}</span></span>
              <span class="covtrack"><span style="width:{g['builtin_pct']:.1f}%"></span></span>
              <span class="covvalue">{g['builtin_pct']:.1f}%</span>
              <span class="covsub">{fmt(g['placed_instances'])}</span>
            </div>""")
        return "".join(out)
    mid = (len(rows) + 1) // 2
    return f'<div class="covgrid"><div>{block(rows[:mid])}</div><div>{block(rows[mid:])}</div></div>'


def coverage_rows(games):
    out = []
    for g in games:
        port = g.get("port")
        if port:
            state = ('<span class="pill mid"><span class="ico">◐</span>open</span>'
                     if port["state"] == "open" else
                     '<span class="pill good"><span class="ico">●</span>closed</span>')
            port_cell = f'<a href="{esc(port["url"])}">games#{port["number"]}</a> {state}'
            if port["total"]:
                port_cell += f'<div class="portmeta">{port["done"]}/{port["total"]} done</div>'
        else:
            port_cell = '<span class="muted">no port issue</span>'
        traces = (f'{fmt(g["traces_recorded"])}' if g["traces_recorded"] is not None
                  else '<span class="muted">none</span>')
        if g.get("converted") is True:
            conv_cell = (f'<span class="pill good"><span class="ico">●</span>converted</span>'
                         f'<div class="frac">{fmt(g["objects_in_package"])} objects</div>')
        elif g.get("converted") is False:
            conv_cell = '<span class="pill mid"><span class="ico">○</span>not converted</span>'
        else:
            conv_cell = '<span class="muted">unknown</span>'
        out.append(f"""
        <tr data-name="{esc((g['name'] + ' ' + g['slug']).lower())}"
            data-cov="{g['builtin_pct'] if g['builtin_pct'] is not None else -1}"
            data-traces="{1 if g['traces_recorded'] else 0}">
          <td class="num muted">{esc(g['order'])}</td>
          <td><a href="{esc(g['source'] or '#')}">{esc(g['name'])}</a>
            <div class="slug">{esc(g['slug'])} · {esc(g['genre'] or g['engine'])}</div></td>
          <td class="num">{fmt(g['placed_instances'])}</td>
          <td class="covecell">{mini_meter(g['builtin_pct'], g['name'])}
            <span class="covnum">{pctf(g['builtin_pct'])}</span>
            <span class="frac">{fmt(g['builtin_instances'])} / {fmt(g['placed_instances'])}</span></td>
          <td class="num">{fmt(g['script_instances'])}<div class="frac">{pctf(g['script_pct'], 2)}</div></td>
          <td class="num">{fmt(g['custom_objects'])}</td>
          <td class="num">{traces}</td>
          <td class="num">{fmt(g['objects_exercised'])}</td>
          <td>{conv_cell}</td>
          <td>{port_cell}</td>
        </tr>""")
    return "".join(out)


def phase_rows(phases):
    out = []
    for p in phases:
        pv = p["pct_closed"] if p["pct_closed"] is not None else 0
        out.append(f"""
        <tr>
          <td class="phase-cell"><div class="phase-title"><span class="phase-num">{esc(p['number'])}</span>{esc(p['name'])}</div>
            <div class="phase-meta">{esc(p['area_labels'])}</div></td>
          <td class="wide">{mini_meter(pv, p['title'])}
            <div class="meter-line"><span>{p['closed']} / {p['total']} closed</span><span>{pv:.0f}%</span></div></td>
          <td class="num">{p['open']}</td>
          <td class="num">{p['size_points']}</td>
          <td class="gate"><details><summary>Gate</summary>
            <p><b>Entry:</b> {esc(p['entry'])}</p><p><b>Exit:</b> {esc(p['exit'])}</p>
            <p><b>Scope:</b> {esc(p['scope'])}</p></details></td>
        </tr>""")
    return "".join(out)


def issue_list(items, empty):
    if not items:
        return f'<p class="empty">{esc(empty)}</p>'
    out = []
    for i in items:
        phase = f'<span class="chip">{esc(i["phase"].split(" · ")[0])}</span>' if i.get("phase") else ""
        out.append(f'<li>{phase}<a href="{esc(i["url"])}">#{i["number"]}</a> '
                   f'<span class="item-title">{esc(i["title"])}</span></li>')
    return '<ul class="items">' + "".join(out) + "</ul>"


def area_bars(rows):
    if not rows:
        return '<p class="empty">No open issues with an area label.</p>'
    top = max(r["count"] for r in rows) or 1
    return "".join(
        f'<div class="hbar-row"><span class="hbar-label">{esc(r["area"])}</span>'
        f'<span class="hbar-track"><span class="hbar-fill" style="width:{100.0*r["count"]/top:.1f}%"></span></span>'
        f'<span class="hbar-value">{r["count"]}</span></div>' for r in rows)


def burnup_chart(points):
    if len(points) < 2:
        return '<p class="empty">The burn-up needs at least two weeks of issue history.</p>'
    w, h, x0, y0, x1, y1 = 760, 200, 46, 12, 700, 164
    top = max(max(p["created"] for p in points), 1)
    top = int((top + 9) // 10 * 10)
    n = len(points)
    xs = [x0 + (x1 - x0) * i / (n - 1) for i in range(n)]
    y = lambda v: y1 - (y1 - y0) * v / top
    path = lambda key: " ".join(("M" if i == 0 else "L") + f"{x:.1f},{y(p[key]):.1f}"
                                for i, (x, p) in enumerate(zip(xs, points)))
    grid = "".join(
        f'<line x1="{x0}" y1="{y(v):.1f}" x2="{x1}" y2="{y(v):.1f}" class="grid"/>'
        f'<text x="{x0-8}" y="{y(v)+4:.1f}" class="axis" text-anchor="end">{v}</text>'
        for v in range(0, top + 1, max(10, top // 5)))
    ticks = "".join(f'<text x="{x:.0f}" y="{y1+18}" class="axis" text-anchor="middle">{esc(p["week"][5:])}</text>'
                    for x, p in zip(xs[:: max(1, n // 6)], points[:: max(1, n // 6)]))
    last = points[-1]
    return f"""
    <div class="legend"><span class="key"><i class="line-key blue"></i>Opened</span>
      <span class="key"><i class="line-key orange"></i>Closed</span></div>
    <div class="chart-wrap"><svg viewBox="0 0 {w} {h+28}" role="img" aria-label="Issues opened and closed by week">
      {grid}
      <path d="{path('created')}" fill="none" stroke="var(--blue)" stroke-width="2" stroke-linejoin="round"/>
      <path d="{path('closed')}" fill="none" stroke="var(--orange)" stroke-width="2" stroke-linejoin="round"/>
      <circle cx="{xs[-1]:.1f}" cy="{y(last['created']):.1f}" r="4" fill="var(--blue)" stroke="var(--surface)" stroke-width="2"/>
      <circle cx="{xs[-1]:.1f}" cy="{y(last['closed']):.1f}" r="4" fill="var(--orange)" stroke="var(--surface)" stroke-width="2"/>
      <text x="{x1+6}" y="{y(last['created'])+4:.1f}" class="end-label">opened {last['created']}</text>
      <text x="{x1+6}" y="{y(last['closed'])+4:.1f}" class="end-label">closed {last['closed']}</text>
      {ticks}</svg></div>"""


def coverage_history(hist):
    measured = [h for h in hist if h.get("builtin_pct") is not None]
    if not measured:
        return '<p class="empty">The first history point is this run; the daily workflow appends the next one.</p>'
    def spark(key, colour):
        w, hgt, x0, y0, x1, y1 = 330, 74, 8, 6, 322, 64
        n = len(hist)
        x = lambda i: x0 + (x1 - x0) * i / max(1, n - 1)
        y = lambda v: y1 - (y1 - y0) * max(0, min(100, v)) / 100
        segs, cur = [], []
        for i, h in enumerate(hist):
            v = h.get(key)
            if v is None:
                if cur:
                    segs.append(cur); cur = []
            else:
                cur.append((i, v))
        if cur:
            segs.append(cur)
        marks = []
        for seg in segs:
            if len(seg) == 1:
                i, v = seg[0]
                marks.append(f'<circle cx="{x(i):.1f}" cy="{y(v):.1f}" r="3" fill="{colour}"/>')
            else:
                d = " ".join(("M" if j == 0 else "L") + f"{x(i):.1f},{y(v):.1f}"
                             for j, (i, v) in enumerate(seg))
                marks.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="2"/>')
        return f'<svg viewBox="0 0 {w} {hgt}" aria-label="{key} over time">{"".join(marks)}</svg>'
    return (f'<div class="spark"><div><span class="sub">Built-in coverage</span>{spark("builtin_pct", "var(--blue)")}</div>'
            f'<div><span class="sub">Script-object share</span>{spark("script_pct", "var(--orange)")}</div></div>')


def stack(ov):
    placed = ov["placed_instances"] or 1
    builtin = 100.0 * (ov["builtin_instances"] or 0) / placed
    script = 100.0 * (ov["script_instances"] or 0) / placed
    other = 100.0 * ((ov.get("client_instances") or 0) + (ov.get("kid_instances") or 0)) / placed
    return (f'<div class="stack" role="img" aria-label="Placed instances by coverage">'
            f'<span class="seg builtin" style="width:{builtin:.2f}%"></span>'
            f'<span class="seg script" style="width:{script:.2f}%"></span>'
            f'<span class="seg other" style="width:{other:.2f}%"></span></div>')


def render(data, hist):
    a, ov = data["activity"], data["overall"]
    ps = data.get("project_status", PROJECT_STATUS)
    games, phases = data["games"], data["phases"]
    totals = a["totals"]
    points = sum(p["size_points"] for p in phases)
    chips = "".join(f'<span class="chip">{esc(k)} {v}</span>'
                    for k, v in sorted(a["process"].items()) if v)
    replacements = {
        "__TITLE__": "IWBTG Collection — object coverage",
        "__GENERATED__": esc(data["generated_at"]),
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
        "__EXERCISED__": fmt(ov["objects_exercised"]),
        "__EXERCISED_UNIQUE__": fmt(ov["objects_exercised_unique"]),
        "__ROOMS__": fmt(ov["rooms"]),
        "__OTHER__": fmt((ov.get("client_instances") or 0) + (ov.get("kid_instances") or 0)),
        "__STACK__": stack(ov),
        "__COVERAGE_CHART__": coverage_chart(games),
        "__COVERAGE_ROWS__": coverage_rows(games),
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
        "__POINTS__": str(points), "__OPEN_PRS__": str(a["open_prs"]),
        "__PROCESS__": chips,
        "__CENSUS__": str(FOLLOWUPS["census"]), "__TRACEFU__": str(FOLLOWUPS["traces"]),
        "__CONVERTER__": str(FOLLOWUPS["converter"]), "__FRONTMATTER__": str(FOLLOWUPS["frontmatter"]),
    }
    page = TEMPLATE
    for key, value in replacements.items():
        page = page.replace(key, value)
    return page


TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root{
  color-scheme:light;
  --page:#f9f9f7; --surface:#fcfcfb; --surface-2:#f3f2ee;
  --ink:#0b0b0b; --ink-2:#52514e; --muted:#898781;
  --grid:#e1e0d9; --line:rgba(11,11,11,.10);
  --blue:#2a78d6; --orange:#eb6834; --track:#cde2fb; --gray:#c3c2b7;
  --good:#0ca30c; --warn:#fab219; --crit:#d03b3b;
}
@media (prefers-color-scheme: dark){
  :root:not([data-theme="light"]){
    color-scheme:dark;
    --page:#0d0d0d; --surface:#1a1a19; --surface-2:#141413;
    --ink:#f0efec; --ink-2:#c3c2b7; --muted:#898781;
    --grid:#2c2c2a; --line:rgba(255,255,255,.10);
    --blue:#3987e5; --orange:#d95926; --track:#184f95; --gray:#383835;
    --good:#0ca30c; --warn:#fab219; --crit:#d03b3b;
  }
}
:root[data-theme="dark"]{
  color-scheme:dark;
  --page:#0d0d0d; --surface:#1a1a19; --surface-2:#141413;
  --ink:#f0efec; --ink-2:#c3c2b7; --muted:#898781;
  --grid:#2c2c2a; --line:rgba(255,255,255,.10);
  --blue:#3987e5; --orange:#d95926; --track:#184f95; --gray:#383835;
  --good:#0ca30c; --warn:#fab219; --crit:#d03b3b;
}
*{box-sizing:border-box}
body{margin:0;background:var(--page);color:var(--ink);
  font:16px/1.55 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:1140px;margin:0 auto;padding:38px 24px 72px}
a{color:var(--blue);text-decoration:none}
a:hover{text-decoration:underline}
h1{font-size:42px;line-height:1.08;letter-spacing:-.025em;margin:6px 0 8px;font-weight:680}
h2{font-size:22px;line-height:1.2;margin:44px 0 8px;letter-spacing:-.015em}
h3{font-size:15px;margin:0 0 8px}
p{color:var(--ink-2);margin:0 0 10px}
.eyebrow{font-size:12px;letter-spacing:.14em;text-transform:uppercase;color:var(--muted)}
.head{display:flex;justify-content:space-between;gap:20px;align-items:flex-start}
.lede{max-width:780px;font-size:16.5px}
.statusrow{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin-top:14px}
.pill{display:inline-flex;gap:6px;align-items:center;border:1px solid var(--line);
  border-radius:999px;padding:3px 10px;font-size:12px;white-space:nowrap}
.pill.mid{color:var(--warn);border-color:rgba(250,178,25,.5)}
.pill.good{color:var(--good);border-color:rgba(12,163,12,.45)}
.pill.muted{color:var(--muted)}
.ico{font-size:11px}
button.theme{background:var(--surface);color:var(--ink-2);border:1px solid var(--line);
  border-radius:8px;padding:8px 12px;font:inherit;font-size:13px;cursor:pointer;white-space:nowrap}
.summary{display:grid;grid-template-columns:1.35fr 1fr;gap:18px;margin:28px 0 8px}
.hero{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:24px 26px}
.hero .label{font-size:13px;color:var(--muted)}
.hero .figure{font-size:64px;font-weight:680;line-height:1;letter-spacing:-.035em;margin:10px 0 6px}
.hero .sub{font-size:14px;color:var(--ink-2)}
.stack{display:flex;gap:2px;height:14px;margin:20px 0 8px;background:var(--surface)}
.seg{display:block;height:100%;border-radius:2px}
.seg.builtin{background:var(--blue)} .seg.script{background:var(--orange)} .seg.other{background:var(--gray)}
.legend{display:flex;flex-wrap:wrap;gap:16px;font-size:12.5px;color:var(--ink-2);margin-top:8px}
.key{display:inline-flex;align-items:center;gap:7px}
.swatch{width:10px;height:10px;border-radius:2px;display:inline-block}
.stats{display:grid;grid-template-columns:1fr 1fr;gap:12px}
.stat{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:14px 16px}
.stat span{display:block;font-size:12px;color:var(--muted)}
.stat b{display:block;font-size:25px;font-weight:650;margin:4px 0 2px;letter-spacing:-.02em}
.stat em{font-style:normal;font-size:12px;color:var(--muted)}
.note{font-size:12.5px;color:var(--muted)}
.callout{background:var(--surface);border:1px solid var(--line);border-left:3px solid var(--warn);
  border-radius:10px;padding:14px 16px;margin:18px 0;font-size:13.5px;color:var(--ink-2)}
.callout b{color:var(--ink)}
table{width:100%;border-collapse:collapse;background:var(--surface);border:1px solid var(--line);
  border-radius:12px;overflow:hidden;font-size:13.5px}
th,td{padding:8px 10px;border-bottom:1px solid var(--line);text-align:left;vertical-align:middle}
th{font-size:11px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);
  background:var(--surface-2);font-weight:600}
tr:last-child td{border-bottom:0}
td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.slug{font-size:12px;color:var(--muted);margin-top:2px}
.frac{font-size:11.5px;color:var(--muted);font-weight:400}
.mini-track{display:inline-block;width:92px;height:7px;background:var(--track);border-radius:4px;
  overflow:hidden;vertical-align:middle}
.mini-track>span{display:block;height:100%;background:var(--blue);border-radius:0 4px 4px 0}
.covecell{min-width:190px;white-space:nowrap}
.covnum{font-weight:600;margin-left:8px;font-variant-numeric:tabular-nums}
.covecell .frac{margin-left:8px}
.covrow{display:grid;grid-template-columns:230px 1fr 60px 100px;gap:12px;align-items:center;
  padding:6px 0;border-bottom:1px solid var(--line)}
.covrow:last-child{border-bottom:0}
.covname{font-size:13px;line-height:1.25}
.covslug{display:block;font-size:11px;color:var(--muted)}
.covtrack{height:10px;background:var(--track);border-radius:4px;overflow:hidden}
.covtrack>span{display:block;height:100%;background:var(--blue);border-radius:0 4px 4px 0}
.covvalue{text-align:right;font-variant-numeric:tabular-nums;font-size:13px}
.covsub{text-align:right;font-size:11.5px;color:var(--muted);font-variant-numeric:tabular-nums}
.chartbox{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:16px 18px}
.covgrid{display:grid;grid-template-columns:1fr 1fr;gap:0 30px}
.grid2{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;align-items:start}
.card{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:14px 16px}
.card h3{font-size:13px;color:var(--muted);text-transform:uppercase;letter-spacing:.05em}
.items{list-style:none;margin:6px 0 0;padding:0}
.items li{padding:7px 0;border-bottom:1px solid var(--line);font-size:13px;line-height:1.4}
.items li:last-child{border-bottom:0}
.item-title{color:var(--ink-2)}
.empty{color:var(--muted);font-size:13px;margin:8px 0 0}
.chip{display:inline-block;border:1px solid var(--line);border-radius:6px;padding:1px 7px;
  font-size:11px;color:var(--ink-2);margin-right:6px;white-space:nowrap}
.meter-line{display:flex;justify-content:space-between;font-size:12px;color:var(--muted);margin-top:4px}
.phase-cell{min-width:230px}
.phase-title{font-weight:600}
.phase-num{display:inline-block;min-width:24px;color:var(--muted);font-variant-numeric:tabular-nums}
.phase-meta{font-size:11.5px;color:var(--muted);margin-top:2px}
.gate summary{cursor:pointer;color:var(--ink-2);font-size:12px}
.gate p{font-size:12.5px;margin:6px 0}
.hbar-row{display:grid;grid-template-columns:140px 1fr 40px;gap:10px;align-items:center;padding:5px 0;font-size:13px}
.hbar-label{color:var(--ink-2);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.hbar-track{height:14px;background:var(--track);border-radius:4px;overflow:hidden}
.hbar-fill{display:block;height:100%;background:var(--blue);border-radius:0 4px 4px 0}
.hbar-value{text-align:right;font-variant-numeric:tabular-nums;color:var(--ink-2)}
.filters{display:flex;flex-wrap:wrap;gap:10px;align-items:center;margin:12px 0}
.filters input,.filters select{background:var(--surface);color:var(--ink);border:1px solid var(--line);
  border-radius:8px;padding:8px 10px;font:inherit;font-size:13px}
.filters input{min-width:250px}
.filters input[type="checkbox"]{min-width:0;width:auto}
.filters label{font-size:12.5px;color:var(--ink-2);display:inline-flex;gap:6px;align-items:center}
svg{display:block;width:100%;height:auto}
.grid{stroke:var(--grid);stroke-width:1}
.axis{fill:var(--muted);font-size:10px}
.end-label{fill:var(--ink-2);font-size:11px}
.spark{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.spark .sub{font-size:12px;color:var(--muted)}
.portmeta{font-size:11px;color:var(--muted);margin-top:2px;white-space:nowrap}
td:last-child{white-space:nowrap}
code{background:var(--surface-2);border:1px solid var(--line);border-radius:4px;padding:1px 5px;font-size:12.5px}
footer{margin-top:52px;padding-top:18px;border-top:1px solid var(--line);font-size:12.5px;color:var(--muted)}
@media(max-width:900px){.summary,.grid2,.spark,.covgrid{grid-template-columns:1fr}.cards{grid-template-columns:1fr 1fr}
  .covrow{grid-template-columns:1fr 1fr 60px}.covsub{display:none}}
@media(max-width:600px){.stats,.cards{grid-template-columns:1fr}h1{font-size:32px}}
</style>
</head>
<body>
<main>
<header>
  <div class="head">
    <div>
      <div class="eyebrow">IWBTG Collection</div>
      <h1>Object coverage</h1>
      <p class="lede">How much of each game is covered by the engine's documented built-in behavior, how much is game-specific script work, and how much of it the recorded traces actually exercise.</p>
    </div>
    <button class="theme" id="theme" type="button">Light / dark</button>
  </div>
  <div class="statusrow">
    <span class="pill mid"><span class="ico">◐</span>__STATUS__</span>
    <span class="pill muted">__STATUS_DETAIL__</span>
    <span class="pill muted">generated __GENERATED__ · source __SOURCE__</span>
  </div>
</header>

<section class="summary">
  <div class="hero">
    <div class="label">Built-in object coverage</div>
    <div class="figure">__BUILTIN_PCT__</div>
    <div class="sub">__BUILTIN__ of __PLACED__ placed instances are covered by a documented built-in</div>
    __STACK__
    <div class="legend">
      <span class="key"><i class="swatch" style="background:var(--blue)"></i>Built-in __BUILTIN__</span>
      <span class="key"><i class="swatch" style="background:var(--orange)"></i>Script __SCRIPT__ (__SCRIPT_PCT__)</span>
      <span class="key"><i class="swatch" style="background:var(--gray)"></i>Client / kid __OTHER__</span>
    </div>
  </div>
  <div class="stats">
    <div class="stat"><span>Custom objects</span><b>__CUSTOM__</b><em>across 20 games</em></div>
    <div class="stat"><span>Script-object instances</span><b>__SCRIPT__</b><em>__SCRIPT_PCT__ of placed</em></div>
    <div class="stat"><span>Recorded traces</span><b>__TRACES__</b><em>across __GAMES_TRACES__ games</em></div>
    <div class="stat"><span>Objects exercised by traces</span><b>__EXERCISED_UNIQUE__</b><em>distinct names across all games</em></div>
  </div>
</section>

<h2>Coverage by game</h2>
<p>Sorted from lowest to highest built-in coverage. The bar is the share of placed instances covered by a built-in; the number after it is the placed-instance total.</p>
<div class="chartbox">__COVERAGE_CHART__</div>

<h2>Every game</h2>
<div class="filters">
  <input id="q" type="search" placeholder="Filter by game name or slug">
  <select id="sort"><option value="order">Shortlist order</option><option value="cov">Built-in coverage (low first)</option><option value="covdesc">Built-in coverage (high first)</option><option value="traces">Recorded traces</option><option value="name">Name</option></select>
  <label><input id="traces" type="checkbox"> Only games with traces</label>
</div>
<table id="games">
<thead><tr><th class="num">#</th><th>Game</th><th class="num">Placed</th><th>Built-in coverage</th><th class="num">Script</th><th class="num">Custom</th><th class="num">Traces</th><th class="num">Exercised</th><th>Converter</th><th>Port</th></tr></thead>
<tbody>__COVERAGE_ROWS__</tbody>
</table>
<p class="note">Placed instances and built-in coverage: <code>specs/mechanics/coverage.md</code> (SCAN, 2026-10-24). Custom objects: <code>shortlist.md</code>. Traces and exercised objects: the recorded CSVs under <code>specs/engines/gm8/traces/&lt;slug&gt;/</code>. <code>progress/export_research.py</code> regenerates these counts.</p>

<div class="callout"><b>Passing-trace coverage is not measured yet.</b> Recorded traces and the object names they exercise are shown above; proving an object tick-accurate needs a converted package and the trace runner. That measurement is <a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">#__TRACEFU__</a>. The census exporter is <code>progress/export_research.py</code>.</div>

<h2>Phase progress</h2>
<table>
<thead><tr><th>Phase</th><th>Progress</th><th class="num">Open</th><th class="num">Points</th><th>Gate</th></tr></thead>
<tbody>__PHASE_ROWS__</tbody>
</table>
<p class="note">__OPEN__ open issues, __CLOSED__ closed (__ISSUE_PCT__), __POINTS__ remaining size points (S=1, M=3, L=8).</p>

<h2>What is moving</h2>
<div class="cards">
  <div class="card"><h3>Recently closed</h3>__RECENT_CLOSED__</div>
  <div class="card"><h3>In progress</h3>__IN_PROGRESS__</div>
  <div class="card"><h3>Waiting on you</h3>__NEEDS_USER__</div>
  <div class="card"><h3>Ready to start</h3>__READY__</div>
</div>

<div class="grid2">
  <section><h2>Open work by area</h2><p class="note">An issue can carry more than one area label.</p>__AREA_BARS__</section>
  <section><h2>Issues over time</h2><p class="note">Cumulative issues opened and closed by week.</p>__BURNUP__</section>
</div>

<h2>Coverage history</h2>
__COVERAGE_HISTORY__
<p class="note">One dated point per workflow run; the line breaks at a run with no measurement.</p>

<footer>
  <p>Reproduce: <code>python -I progress/export_research.py --research ../iwbtg-collection-research</code>, then <code>python -I progress/generate.py --gh --history</code>. The workflow restores history from <code>gh-pages</code>, regenerates and publishes there; it never writes <code>main</code>.</p>
  <p>Sources: <code>shortlist.md</code> · <code>specs/mechanics/coverage.md</code> · recorded trace CSVs · <code>build/reports/summary.json</code> (converter check). Follow-ups: <a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">passing-trace coverage #__TRACEFU__</a> · <a href="https://github.com/nugamus/iwbtg-collection/issues/__CONVERTER__">converter state #__CONVERTER__</a>.</p>
</footer>
</main>
<script>
(function(){
  const root=document.documentElement;
  try{const saved=localStorage.getItem('iwbtg-theme'); if(saved) root.dataset.theme=saved;}catch(e){}
  const b=document.getElementById('theme');
  if(b) b.addEventListener('click',()=>{
    const next=(root.dataset.theme|| (matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'))==='dark'?'light':'dark';
    root.dataset.theme=next; try{localStorage.setItem('iwbtg-theme',next);}catch(e){}
  });
})();
(function(){
  const q=document.getElementById('q'), sort=document.getElementById('sort'), traces=document.getElementById('traces');
  if(!q) return;
  const body=document.querySelector('#games tbody');
  const all=[...body.querySelectorAll('tr')];
  function num(r,k){return parseFloat(r.dataset[k]||'0');}
  function apply(){
    const text=q.value.trim().toLowerCase();
    all.forEach(r=>{
      const okText=!text||(r.dataset.name||'').includes(text);
      const okTrace=!traces.checked||r.dataset.traces==='1';
      r.hidden=!(okText&&okTrace);
    });
    const vis=all.filter(r=>!r.hidden);
    const mode=sort.value;
    vis.sort((a,b)=>{
      if(mode==='cov') return num(a,'cov')-num(b,'cov');
      if(mode==='covdesc') return num(b,'cov')-num(a,'cov');
      if(mode==='traces') return num(b,'traces')-num(a,'traces');
      if(mode==='name') return (a.dataset.name||'').localeCompare(b.dataset.name||'');
      return all.indexOf(a)-all.indexOf(b);
    });
    vis.forEach(r=>body.appendChild(r));
  }
  q.addEventListener('input',apply); sort.addEventListener('change',apply); traces.addEventListener('change',apply);
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
    args = ap.parse_args()
    at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    data = None
    refreshed = False
    if args.require_gh and not args.gh:
        raise SystemExit("--require-gh needs --gh")
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
