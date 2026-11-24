#!/usr/bin/env python3
"""Build the porting-progress dashboard.

Reproducible from committed files plus `gh`: no secrets, no hand-edited numbers.

    python -I progress/generate.py                  # committed data.json + game files
    python -I progress/generate.py --gh             # refresh everything from GitHub
    python -I progress/generate.py --gh --history    # also append a dated snapshot

What is measured now, without the census/trace exporters:
  * phase progress and remaining size points, from the main repo milestones and issues;
  * issue activity, process queues, areas and a weekly burn-up, from issue timestamps;
  * per-game port-issue state, the "Done when" checklist and sub-issue progress;
  * recorded trace counts, parsed from docs/architecture/traces.md (dated source).

Object coverage (census / scripts / tick accuracy) stays "not measured yet" until the
exporters exist; those cells link to the follow-up issues.
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
NOT_MEASURED = "not measured yet"
PROJECT_STATUS = {
    "label": "Pre-release",
    "detail": "No release builds yet. The public alpha is planned for phase 7 (Online).",
}


# ----------------------------------------------------------------- data files
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
    if isinstance(value, str) and re.fullmatch(r"-?\d+", value.strip()):
        return int(value)
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
        games.append({
            "id": fm.get("id") or slug,
            "slug": slug,
            "order": as_int(fm.get("order")) or 999,
            "name": fm.get("name") or slug,
            "source": fm.get("source"),
            "engine": fm.get("engine") or "unknown",
            "lifecycle": ex.get("lifecycle") or fm.get("status") or "unknown",
            "converted": ex.get("converted", None),
            "objects_total": as_int(ex.get("objects_total", fm.get("objects_total"))),
            "objects_specced": as_int(ex.get("objects_specced", fm.get("objects_specced"))),
            "objects_scripted": as_int(ex.get("objects_scripted", fm.get("objects_scripted"))),
            "objects_in_package": as_int(ex.get("objects_in_package")),
            "traces_passing": as_int(ex.get("traces_passing", fm.get("traces_passing"))),
            "traces_total": as_int(ex.get("traces_total", fm.get("traces_total"))),
            "traces_recorded": None,
            "port": None,
        })
    games.sort(key=lambda g: g["order"])
    return games


def load_trace_counts():
    """Recorded trace CSVs per game, from the dated table in traces.md."""
    path = os.path.join(ROOT, "docs", "architecture", "traces.md")
    if not os.path.exists(path):
        return {}, None
    text = open(path, encoding="utf-8").read()
    m = re.search(r"CSVs exist \(([^)]+)\)[^:]*:\s*(.+)", text)
    if not m:
        return {}, None
    date = m.group(1).strip()
    counts = {}
    for slug, n in re.findall(r"([a-z0-9][a-z0-9-]*)\s+(\d+)", m.group(2)):
        counts[slug] = int(n)
    return counts, date


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
    if not ms:
        return None
    return ms["title"]


def slim(issue):
    return {
        "number": issue["number"],
        "title": issue["title"],
        "state": issue["state"],
        "url": issue["html_url"],
        "labels": [l["name"] for l in issue["labels"]],
        "phase": phase_of(issue),
        "updated_at": issue.get("updated_at"),
        "closed_at": issue.get("closed_at"),
        "created_at": issue.get("created_at"),
    }


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
            "state": m["state"],
            "open": len(opened),
            "closed": total - len(opened),
            "total": total,
            "pct_closed": round(100.0 * (total - len(opened)) / total, 1) if total else None,
            "size_points": points,
            "sizes": sizes,
            "scope": fields.get("scope", ""),
            "feeds_from": fields.get("feeds from", ""),
            "area_labels": fields.get("area labels", ""),
            "entry": fields.get("entry", ""),
            "exit": fields.get("exit", ""),
            "progress": fields.get("progress", ""),
        })
    return phases


def burnup(issues, now, max_weeks=16):
    created = [dt.datetime.fromisoformat(i["created_at"].replace("Z", "+00:00")) for i in issues if i.get("created_at")]
    closed = [dt.datetime.fromisoformat(i["closed_at"].replace("Z", "+00:00")) for i in issues if i.get("closed_at")]
    if not created:
        return []
    start = min(created).date()
    end = max([now.date()] + [d.date() for d in closed])
    ws = start - dt.timedelta(days=start.weekday())
    points = []
    while ws <= end:
        we = ws + dt.timedelta(days=7)
        c = sum(1 for d in created if d.date() < we)
        cl = sum(1 for d in closed if d.date() < we)
        points.append({"week": we.isoformat(), "created": c, "closed": cl})
        ws = we
    return points[-max_weeks:]


def build_activity(issues, pulls, now):
    opened = [i for i in issues if i["state"] == "open"]
    closed = [i for i in issues if i["state"] == "closed"]
    labels = lambda i: {l["name"] for l in i["labels"]}
    cutoff = now - dt.timedelta(days=14)

    def closed_at(i):
        return dt.datetime.fromisoformat(i["closed_at"].replace("Z", "+00:00")) if i.get("closed_at") else None

    recent_closed = sorted([i for i in closed if closed_at(i) and closed_at(i) >= cutoff],
                           key=lambda i: i["closed_at"], reverse=True)[:10]
    in_progress = sorted([i for i in opened if "in-progress" in labels(i)],
                         key=lambda i: i.get("updated_at") or "", reverse=True)[:8]
    needs_user = sorted([i for i in opened if "needs-user" in labels(i)],
                        key=lambda i: i.get("updated_at") or "", reverse=True)[:8]
    ready = sorted([i for i in opened if "agent-ready" in labels(i)
                    and not (labels(i) & {"blocked", "in-progress"})],
                   key=lambda i: (phase_of(i) or "99", i["number"]))[:8]
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
        "open_prs": len(pulls),
        "process": dict(process),
        "area_open": [{"area": k, "count": v} for k, v in area.most_common()],
        "recent_closed": [slim(i) for i in recent_closed],
        "in_progress": [slim(i) for i in in_progress],
        "needs_user": [slim(i) for i in needs_user],
        "agent_ready": [slim(i) for i in ready],
        "closed_7d": sum(1 for i in closed if closed_at(i) and closed_at(i) >= week),
        "opened_7d": sum(1 for i in opened if i.get("created_at")
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
        g["port"] = {
            "number": pi["number"],
            "state": pi["state"],
            "url": pi["html_url"],
            "done": done,
            "total": total,
            "sub_done": sub.get("completed", 0),
            "sub_total": sub.get("total", 0),
            "updated_at": pi.get("updated_at"),
        }


def build_data(refresh):
    games = load_games()
    trace_counts, trace_date = load_trace_counts()
    if refresh:
        milestones = gh_json(f"repos/{REPO}/milestones?state=all&per_page=100")
        issues = [i for i in gh_json(f"repos/{REPO}/issues?state=all&per_page=100")
                  if "pull_request" not in i]
        pulls = gh_json(f"repos/{REPO}/pulls?state=open&per_page=100")
        games_issues = [i for i in gh_json(f"repos/{GAMES_REPO}/issues?state=all&per_page=100")
                        if "pull_request" not in i]
        now = dt.datetime.now(dt.timezone.utc)
        phases = build_phases(milestones, issues)
        activity = build_activity(issues, pulls, now)
        build_ports(games, games_issues)
        source = "gh"
    else:
        phases, activity, source = [], empty_activity(), "game files only"
    for g in games:
        g["traces_recorded"] = trace_counts.get(g["slug"])
    ov = compute_overall(games)
    ov["traces_recorded"] = sum(c for c in trace_counts.values())
    ov["games_with_traces"] = sum(1 for g in games if g["traces_recorded"] is not None)
    ov["trace_source_date"] = trace_date
    ov["ports_tracked"] = sum(1 for g in games if g.get("port"))
    ov["ports_closed"] = sum(1 for g in games if g.get("port") and g["port"]["state"] == "closed")
    ov["ports_in_beta"] = sum(1 for g in games if (g["lifecycle"] or "").lower() in
                              ("beta", "ranked", "loved"))
    return {"generated_at": now.isoformat().replace("+00:00", "Z") if refresh else
            dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
            "source": source, "project_status": PROJECT_STATUS,
            "phases": phases, "activity": activity, "games": games,
            "overall": ov, "followups": FOLLOWUPS}


def empty_activity():
    return {"totals": {"issues": 0, "open": 0, "closed": 0, "pct_closed": None},
            "open_prs": 0, "process": {}, "area_open": [], "recent_closed": [],
            "in_progress": [], "needs_user": [], "agent_ready": [],
            "closed_7d": 0, "opened_7d": 0, "burnup": []}


def pct(num, den):
    if num is None or not den:
        return None
    return round(100.0 * num / den, 1)


def compute_overall(games):
    census = [g for g in games if g["objects_total"]]
    tick = [g for g in games if g["objects_total"] and g["traces_passing"] is not None]
    std_den_games = [g for g in census
                     if (g["objects_in_package"] is not None or g["objects_scripted"] is not None)]
    std_num = sum((g["objects_in_package"] if g["objects_in_package"] is not None
                   else g["objects_scripted"]) for g in std_den_games)
    std_den = sum(g["objects_total"] for g in std_den_games)
    tick_num = sum(g["traces_passing"] for g in tick)
    tick_den = sum(g["objects_total"] for g in tick)
    return {
        "standard_num": std_num if std_den else None,
        "standard_den": std_den if std_den else None,
        "standard_pct": pct(std_num, std_den),
        "tick_num": tick_num if tick_den else None,
        "tick_den": tick_den if tick_den else None,
        "tick_pct": pct(tick_num, tick_den),
        "games_census": len(census),
        "standard_games": len(std_den_games),
        "games_tick": len(tick),
        "games_total": len(games),
    }


# ----------------------------------------------------------------- history
def snapshot(data, at):
    a, ov = data["activity"], data["overall"]
    return {
        "at": at,
        "standard_pct": ov["standard_pct"],
        "tick_pct": ov["tick_pct"],
        "games_census": ov["games_census"],
        "games_tick": ov["games_tick"],
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
    keep = ("standard_pct", "tick_pct", "open_issues", "closed_issues", "issue_pct_closed",
            "size_points", "games_census", "games_tick")
    if hist and all(hist[-1].get(k) == snap.get(k) for k in keep) and hist[-1]["at"][:10] == at[:10]:
        return hist
    hist.append(snap)
    json.dump(hist, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    return hist


# ----------------------------------------------------------------- render
def esc(value):
    return html.escape("" if value is None else str(value))


def cell(num, den, followup):
    if num is None or not den:
        return (f'<span class="missing"><a href="https://github.com/{REPO}/issues/{followup}">'
                f'{NOT_MEASURED}</a></span>')
    return f'<strong>{pct(num, den)}%</strong> <span class="frac">({num} / {den})</span>'


def meter(value, label=""):
    v = max(0.0, min(100.0, float(value or 0)))
    return (f'<div class="meter" role="img" aria-label="{esc(label)} {v:.0f}%">'
            f'<span style="width:{v:.1f}%"></span></div>')


def phase_rows(phases):
    out = []
    for p in phases:
        pv = p["pct_closed"] if p["pct_closed"] is not None else 0
        out.append(f"""
        <tr>
          <td class="phase-cell">
            <div class="phase-title"><span class="phase-num">{esc(p['number'])}</span>{esc(p['name'])}</div>
            <div class="phase-meta">{esc(p['area_labels'])}</div>
          </td>
          <td class="wide">{meter(pv, p['title'])}
            <div class="meter-line"><span>{p['closed']} / {p['total']} closed</span><span>{pv:.0f}%</span></div>
          </td>
          <td class="num">{p['open']}</td>
          <td class="num">{p['size_points']}</td>
          <td class="gate"><details><summary>Gate</summary>
            <p><b>Entry:</b> {esc(p['entry'])}</p>
            <p><b>Exit:</b> {esc(p['exit'])}</p>
            <p><b>Scope:</b> {esc(p['scope'])}</p>
            <p><b>Feeds from:</b> {esc(p['feeds_from'])}</p>
          </details></td>
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
    return "<ul class=\"items\">" + "".join(out) + "</ul>"


def area_bars(rows):
    if not rows:
        return '<p class="empty">No open issues with an area label.</p>'
    top = max(r["count"] for r in rows) or 1
    out = []
    for r in rows:
        w = 100.0 * r["count"] / top
        out.append(f'<div class="hbar-row"><span class="hbar-label">{esc(r["area"])}</span>'
                   f'<span class="hbar-track"><span class="hbar-fill" style="width:{w:.1f}%"></span></span>'
                   f'<span class="hbar-value">{r["count"]}</span></div>')
    return "".join(out)


def burnup_chart(points):
    if len(points) < 2:
        return '<p class="empty">The burn-up needs at least two weeks of issue history.</p>'
    w, h, x0, y0, x1, y1 = 780, 210, 52, 14, 720, 170
    top = max(max(p["created"] for p in points), 1)
    top = int((top + 9) // 10 * 10)
    n = len(points)
    xs = [x0 + (x1 - x0) * i / (n - 1) for i in range(n)]
    def y(v): return y1 - (y1 - y0) * v / top
    def path(key):
        return " ".join(("M" if i == 0 else "L") + f"{x:.1f},{y(p[key]):.1f}"
                        for i, (x, p) in enumerate(zip(xs, points)))
    grid = "".join(
        f'<line x1="{x0}" y1="{y(v):.1f}" x2="{x1}" y2="{y(v):.1f}" class="grid"/>'
        f'<text x="{x0-8}" y="{y(v)+4:.1f}" class="axis" text-anchor="end">{v}</text>'
        for v in range(0, top + 1, max(10, top // 5)))
    ticks = "".join(f'<text x="{x:.0f}" y="{y1+18}" class="axis" text-anchor="middle">{esc(p["week"][5:])}</text>'
                    for x, p in zip(xs[:: max(1, n // 7)], points[:: max(1, n // 7)]))
    last = points[-1]
    ends = (f'<text x="{x1+6}" y="{y(last["created"])+4:.1f}" class="end-label">opened {last["created"]}</text>'
            f'<text x="{x1+6}" y="{y(last["closed"])+4:.1f}" class="end-label">closed {last["closed"]}</text>')
    hits = "".join(
        f'<rect class="hit" data-i="{i}" x="{x0 + (x1-x0)*i/n:.1f}" y="{y0}" '
        f'width="{(x1-x0)/n:.1f}" height="{y1-y0}" fill="transparent"/>'
        for i in range(n))
    return f"""
    <div class="legend">
      <span class="key"><i class="line-key blue"></i>Opened</span>
      <span class="key"><i class="line-key orange"></i>Closed</span>
    </div>
    <div class="chart-wrap">
      <svg id="burnup" viewBox="0 0 {w} {h+30}" role="img" aria-label="Cumulative issues opened and closed by week">
        {grid}
        <path d="{path('created')}" fill="none" stroke="#3987e5" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>
        <path d="{path('closed')}" fill="none" stroke="#d95926" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>
        <circle cx="{xs[-1]:.1f}" cy="{y(last['created']):.1f}" r="4" fill="#3987e5" stroke="#1a1a19" stroke-width="2"/>
        <circle cx="{xs[-1]:.1f}" cy="{y(last['closed']):.1f}" r="4" fill="#d95926" stroke="#1a1a19" stroke-width="2"/>
        {ends}{ticks}{hits}
      </svg>
      <div id="burn-tip" class="tip" hidden></div>
    </div>
    <details class="table-view"><summary>Table view</summary>
      <table><thead><tr><th>Week ending</th><th class="num">Opened</th><th class="num">Closed</th><th class="num">Open</th></tr></thead>
      <tbody>{"".join(f'<tr><td>{esc(p["week"])}</td><td class="num">{p["created"]}</td><td class="num">{p["closed"]}</td><td class="num">{p["created"]-p["closed"]}</td></tr>' for p in points)}</tbody></table>
    </details>"""


def port_rows(games):
    out = []
    for g in games:
        p = g.get("port")
        if p:
            state_pill = ('<span class="pill mid"><span class="ico">◐</span>open</span>' if p["state"] == "open"
                          else '<span class="pill good"><span class="ico">●</span>closed</span>')
            port = f'<a href="{esc(p["url"])}">games#{p["number"]}</a> {state_pill}'
            pv = 100.0 * p["done"] / p["total"] if p["total"] else 0
            done = (f'<div class="mini-meter"><span style="width:{pv:.1f}%"></span></div>'
                    f'<span class="frac">{p["done"]} / {p["total"]}</span>')
            subs = (f'{p["sub_done"]} / {p["sub_total"]}' if p["sub_total"] else
                    '<span class="muted">none yet</span>')
        else:
            port = '<span class="muted">no port issue</span>'
            done = '<span class="muted">—</span>'
            subs = '<span class="muted">—</span>'
        traces = (f'{g.get("traces_recorded")}' if g.get("traces_recorded") is not None
                  else '<span class="muted">—</span>')
        status = g.get("lifecycle") or "unknown"
        if status.lower() in ("unknown", ""):
            status_html = cell(0, 0, FOLLOWUPS["frontmatter"])
        else:
            status_html = (f'<span class="pill '
                           f'{"good" if status.lower() in ("beta", "ranked", "loved") else "muted"}">'
                           f'{esc(status)}</span>')
        row = f"""
        <tr data-name="{esc((g['name'] + ' ' + g['slug']).lower())}"
            data-state="{esc(p['state'] if p else 'none')}"
            data-traces="{1 if g.get('traces_recorded') else 0}">
          <td class="num muted">{esc(g['order'])}</td>
          <td><a href="{esc(g['source'] or '#')}">{esc(g['name'])}</a><div class="slug">{esc(g['slug'])}</div></td>
          <td>{port}</td>
          <td class="wide">{done}</td>
          <td class="num">{subs}</td>
          <td class="num">{traces}</td>
          <td>{status_html}</td>
        </tr>"""
        out.append(row)
    return "".join(out)


def coverage_rows(games):
    out = []
    for g in games:
        out.append(f"""
        <tr>
          <td>{esc(g['name'])}<div class="slug">{esc(g['slug'])}</div></td>
          <td>{("converted" if g["converted"] else "not converted") if g["converted"] is not None
               else cell(0, 0, FOLLOWUPS["converter"])}</td>
          <td class="num">{cell(g['objects_specced'], g['objects_total'], FOLLOWUPS['census'])}</td>
          <td class="num">{cell(g['objects_scripted'], g['objects_total'], FOLLOWUPS['census'])}</td>
          <td class="num">{cell(g['traces_passing'], g['objects_total'], FOLLOWUPS['traces'])}</td>
        </tr>""")
    return "".join(out)


def coverage_history(hist):
    measured = [h for h in hist if h.get("standard_pct") is not None or h.get("tick_pct") is not None]
    if not measured:
        return ('<p class="empty">No measured object-coverage points yet. The exporters are the '
                f'<a href="https://github.com/{REPO}/issues/{FOLLOWUPS["census"]}">census</a>, '
                f'<a href="https://github.com/{REPO}/issues/{FOLLOWUPS["traces"]}">trace</a> and '
                f'<a href="https://github.com/{REPO}/issues/{FOLLOWUPS["converter"]}">converter</a> follow-ups.</p>')
    def line(key, colour):
        w, hgt, x0, y0, x1, y1 = 340, 90, 30, 8, 330, 74
        n = len(hist)
        def x(i):
            return x0 + (x1 - x0) * i / max(1, n - 1)
        def y(v):
            return y1 - (y1 - y0) * max(0, min(100, v)) / 100
        segs, cur = [], []
        for i, h in enumerate(hist):
            v = h.get(key)
            if v is None:
                if cur:
                    segs.append(cur)
                    cur = []
            else:
                cur.append((i, v))
        if cur:
            segs.append(cur)
        if not segs:
            return '<p class="empty">No measured point for this measure yet.</p>'
        marks = []
        for seg in segs:
            if len(seg) == 1:
                i, v = seg[0]
                marks.append(f'<circle cx="{x(i):.1f}" cy="{y(v):.1f}" r="3" fill="{colour}"/>')
            else:
                d = " ".join(("M" if j == 0 else "L") + f"{x(i):.1f},{y(v):.1f}"
                             for j, (i, v) in enumerate(seg))
                marks.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="2"/>')
        return (f'<svg viewBox="0 0 {w} {hgt}" role="img" aria-label="{key} over time">'
                f'{"".join(marks)}</svg>')
    return (f'<div class="spark"><div><span class="sub">Objects in the standard format</span>'
            f'{line("standard_pct", "#3987e5")}</div>'
            f'<div><span class="sub">Objects tick-accurate</span>{line("tick_pct", "#d95926")}</div></div>')


def render(data, hist):
    a, ov = data["activity"], data["overall"]
    ps = data.get("project_status", PROJECT_STATUS)
    phases, games = data["phases"], data["games"]
    totals = a["totals"]
    points = sum(p["size_points"] for p in phases)
    pct_closed = totals["pct_closed"] or 0
    status_ico = "◐"
    process = a["process"]
    chips = "".join(f'<span class="chip">{esc(k)} {v}</span>'
                    for k, v in sorted(process.items()) if v)
    return TEMPLATE.replace("__TITLE__", "IWBTG Collection — porting progress") \
        .replace("__GENERATED__", esc(data["generated_at"])) \
        .replace("__SOURCE__", esc(data["source"])) \
        .replace("__STATUS__", esc(ps["label"])) \
        .replace("__STATUS_DETAIL__", esc(ps["detail"])) \
        .replace("__STATUS_ICO__", status_ico) \
        .replace("__PCT_CLOSED__", f"{pct_closed:.0f}") \
        .replace("__CLOSED__", str(totals["closed"])) \
        .replace("__ISSUES__", str(totals["issues"])) \
        .replace("__OPEN__", str(totals["open"])) \
        .replace("__POINTS__", str(points)) \
        .replace("__OPEN_PRS__", str(a["open_prs"])) \
        .replace("__PORTS__", str(ov["ports_tracked"])) \
        .replace("__BETA__", str(ov["ports_in_beta"])) \
        .replace("__TRACES__", str(ov["traces_recorded"])) \
        .replace("__TRACE_DATE__", esc(ov["trace_source_date"] or "unknown")) \
        .replace("__CLOSED_7D__", str(a["closed_7d"])) \
        .replace("__OPENED_7D__", str(a["opened_7d"])) \
        .replace("__PROCESS__", chips) \
        .replace("__PHASE_ROWS__", phase_rows(phases)) \
        .replace("__AREA_BARS__", area_bars(a["area_open"])) \
        .replace("__BURNUP__", burnup_chart(a["burnup"])) \
        .replace("__RECENT_CLOSED__", issue_list(a["recent_closed"], "Nothing closed in the last 14 days.")) \
        .replace("__IN_PROGRESS__", issue_list(a["in_progress"], "No issue is claimed right now.")) \
        .replace("__NEEDS_USER__", issue_list(a["needs_user"], "No question is waiting on the user.")) \
        .replace("__READY__", issue_list(a["agent_ready"], "No unblocked agent-ready issue.")) \
        .replace("__PORT_ROWS__", port_rows(games)) \
        .replace("__COVERAGE_ROWS__", coverage_rows(games)) \
        .replace("__COVERAGE_HISTORY__", coverage_history(hist)) \
        .replace("__STANDARD__", cell(ov["standard_num"], ov["standard_den"], FOLLOWUPS["census"])) \
        .replace("__TICK__", cell(ov["tick_num"], ov["tick_den"], FOLLOWUPS["traces"])) \
        .replace("__STANDARD_GAMES__", str(ov.get("standard_games", 0))) \
        .replace("__TICK_GAMES__", str(ov.get("games_tick", 0))) \
        .replace("__BURN_JSON__", json.dumps(a["burnup"])) \
        .replace("__CENSUS__", str(FOLLOWUPS["census"])) \
        .replace("__TRACEFU__", str(FOLLOWUPS["traces"])) \
        .replace("__CONVERTER__", str(FOLLOWUPS["converter"])) \
        .replace("__FRONTMATTER__", str(FOLLOWUPS["frontmatter"]))


TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root{
  color-scheme: dark;
  --page:#0d0d0d; --surface:#1a1a19; --surface-2:#141413;
  --ink:#f0efec; --ink-2:#c3c2b7; --muted:#898781;
  --grid:#2c2c2a; --baseline:#383835; --line:rgba(255,255,255,.10);
  --blue:#3987e5; --orange:#d95926; --track:#184f95;
  --good:#0ca30c; --warn:#fab219; --serious:#ec835a; --crit:#d03b3b;
}
*{box-sizing:border-box}
html{background:var(--page)}
body{margin:0;background:var(--page);color:var(--ink);
  font:15px/1.55 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:1180px;margin:0 auto;padding:40px 24px 72px}
a{color:var(--blue);text-decoration:none}
a:hover{text-decoration:underline}
p{color:var(--ink-2);margin:0 0 10px}
h1{font-size:40px;line-height:1.1;letter-spacing:-.02em;margin:10px 0 8px;font-weight:680}
h2{font-size:20px;margin:42px 0 12px;letter-spacing:-.01em}
h3{font-size:15px;margin:0 0 8px;color:var(--ink)}
.eyebrow{font-size:12px;letter-spacing:.14em;text-transform:uppercase;color:var(--muted)}
.lede{max-width:760px;font-size:16px}
.statusline{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px;align-items:center}
.pill{display:inline-flex;gap:6px;align-items:center;border:1px solid var(--line);
  border-radius:999px;padding:3px 10px;font-size:12px;white-space:nowrap}
.pill.good{color:var(--good);border-color:rgba(12,163,12,.45)}
.pill.mid{color:var(--warn);border-color:rgba(250,178,25,.45)}
.pill.muted{color:var(--muted)}
.ico{font-size:11px}
.chip{display:inline-block;border:1px solid var(--line);border-radius:6px;padding:1px 7px;
  font-size:11px;color:var(--ink-2);margin-right:6px;white-space:nowrap}
.kpis{display:grid;grid-template-columns:1.5fr repeat(4,1fr);gap:14px;margin:28px 0 8px}
.kpi{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:16px 18px}
.kpi .label{font-size:12px;color:var(--muted);text-transform:uppercase;letter-spacing:.06em}
.kpi .value{font-size:26px;font-weight:650;margin-top:6px}
.kpi .sub{font-size:12.5px;color:var(--muted);margin-top:4px}
.kpi.hero .figure{font-size:56px;font-weight:680;line-height:1;margin:8px 0 12px;letter-spacing:-.03em}
.meter{height:8px;background:var(--track);border-radius:4px;overflow:hidden;margin:10px 0 6px}
.meter>span{display:block;height:100%;background:var(--blue);border-radius:4px}
.meter-line{display:flex;justify-content:space-between;font-size:12px;color:var(--muted)}
.note{font-size:12.5px;color:var(--muted)}
code{background:var(--surface-2);border:1px solid var(--line);border-radius:4px;padding:1px 5px;font-size:12.5px}
.muted{color:var(--muted)}
.frac{color:var(--muted);font-size:12px}
.slug{color:var(--muted);font-size:12px;margin-top:2px}
table{width:100%;border-collapse:collapse;background:var(--surface);
  border:1px solid var(--line);border-radius:12px;overflow:hidden}
th,td{padding:10px 12px;border-bottom:1px solid var(--line);text-align:left;vertical-align:middle}
th{font-size:11px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);
  background:var(--surface-2);font-weight:600}
tr:last-child td{border-bottom:0}
td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
td.wide{min-width:180px}
.phase-cell{min-width:230px}
.phase-title{font-weight:600}
.phase-num{display:inline-block;min-width:24px;color:var(--muted);font-variant-numeric:tabular-nums}
.phase-meta{font-size:11.5px;color:var(--muted);margin-top:2px}
.gate summary{cursor:pointer;color:var(--ink-2);font-size:12px}
.gate p{font-size:12.5px;margin:6px 0}
details.table-view{margin-top:10px;font-size:12.5px;color:var(--ink-2)}
details.table-view summary{cursor:pointer;color:var(--muted);margin-bottom:8px}
details.table-view table{font-size:12.5px}
.grid2{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;align-items:start}
.card{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:14px 16px}
.card h3{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);font-weight:600}
.items{list-style:none;margin:6px 0 0;padding:0}
.items li{padding:7px 0;border-bottom:1px solid var(--line);font-size:13px;line-height:1.4}
.items li:last-child{border-bottom:0}
.item-title{color:var(--ink-2)}
.empty{color:var(--muted);font-size:13px;margin:8px 0 0}
.hbar-row{display:grid;grid-template-columns:140px 1fr 40px;gap:10px;align-items:center;
  padding:5px 0;font-size:13px}
.hbar-label{color:var(--ink-2);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.hbar-track{height:14px;background:var(--track);border-radius:4px;overflow:hidden}
.hbar-fill{display:block;height:100%;background:var(--blue);border-radius:0 4px 4px 0}
.hbar-value{text-align:right;font-variant-numeric:tabular-nums;color:var(--ink-2)}
.legend{display:flex;gap:18px;margin:4px 0 6px;font-size:12.5px;color:var(--ink-2)}
.key{display:inline-flex;align-items:center;gap:7px}
.line-key{display:inline-block;width:18px;height:2px;border-radius:1px}
.line-key.blue{background:var(--blue)} .line-key.orange{background:var(--orange)}
.chart-wrap{position:relative}
svg{display:block;width:100%;height:auto}
.grid{stroke:var(--grid);stroke-width:1}
.axis{fill:var(--muted);font-size:10px}
.end-label{fill:var(--ink-2);font-size:11px}
.hit{cursor:crosshair}
.tip{position:fixed;pointer-events:none;background:var(--surface);border:1px solid var(--line);
  border-radius:8px;padding:8px 10px;font-size:12px;box-shadow:0 6px 24px rgba(0,0,0,.45);z-index:10}
.tip b{color:var(--ink);font-size:13px}
.spark{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.spark .sub{font-size:12px;color:var(--muted)}
.mini-meter{height:6px;background:var(--track);border-radius:3px;overflow:hidden;margin-bottom:3px;min-width:90px}
.mini-meter>span{display:block;height:100%;background:var(--blue);border-radius:3px}
.filters{display:flex;flex-wrap:wrap;gap:10px;align-items:center;margin:10px 0}
.filters input,.filters select{background:var(--surface);color:var(--ink);border:1px solid var(--line);
  border-radius:8px;padding:7px 10px;font:inherit;font-size:13px}
.filters input{min-width:240px}
.filters label{font-size:12.5px;color:var(--ink-2);display:inline-flex;gap:6px;align-items:center}
footer{margin-top:48px;padding-top:18px;border-top:1px solid var(--line);font-size:12.5px;color:var(--muted)}
@media(max-width:900px){
  .kpis{grid-template-columns:1fr 1fr}
  .cards{grid-template-columns:1fr 1fr}
  .grid2,.spark{grid-template-columns:1fr}
}
@media(max-width:600px){.kpis,.cards{grid-template-columns:1fr}h1{font-size:32px}}
</style>
</head>
<body>
<main>
<header>
  <div class="eyebrow">IWBTG Collection</div>
  <h1>Porting progress</h1>
  <p class="lede">What is built, what is moving, and what is still honest gap. Every number is a numerator and denominator; a missing source says <b>not measured yet</b> and links the issue that will fill it.</p>
  <div class="statusline">
    <span class="pill mid"><span class="ico">__STATUS_ICO__</span>__STATUS__</span>
    <span class="pill muted">__STATUS_DETAIL__</span>
    <span class="pill muted">generated __GENERATED__ · source __SOURCE__</span>
  </div>
</header>

<section class="kpis">
  <div class="kpi hero">
    <div class="label">Issues closed</div>
    <div class="figure">__PCT_CLOSED__%</div>
    <div class="meter"><span style="width:__PCT_CLOSED__%"></span></div>
    <div class="sub">__CLOSED__ of __ISSUES__ tracked issues</div>
  </div>
  <div class="kpi"><div class="label">Open issues</div><div class="value">__OPEN__</div>
    <div class="sub">__CLOSED_7D__ closed / __OPENED_7D__ opened in 7 days</div></div>
  <div class="kpi"><div class="label">Remaining size points</div><div class="value">__POINTS__</div>
    <div class="sub">S=1 · M=3 · L=8 · no size=0</div></div>
  <div class="kpi"><div class="label">Ports tracked</div><div class="value">__PORTS__ / 20</div>
    <div class="sub">__BETA__ in Beta or Ranked</div></div>
  <div class="kpi"><div class="label">Recorded traces</div><div class="value">__TRACES__</div>
    <div class="sub">from traces.md, __TRACE_DATE__</div></div>
</section>
<p class="note">__PROCESS__ · __OPEN_PRS__ open pull requests</p>

<h2>Phase progress</h2>
<p class="note">A phase is a gate, not a date. Bars show closed issues; the size points are the open issues' S/M/L labels. Open the gate for its entry and exit condition.</p>
<table>
<thead><tr><th>Phase</th><th>Progress</th><th class="num">Open</th><th class="num">Points</th><th>Gate</th></tr></thead>
<tbody>__PHASE_ROWS__</tbody>
</table>

<h2>What is moving</h2>
<div class="cards">
  <div class="card"><h3>Recently closed</h3>__RECENT_CLOSED__</div>
  <div class="card"><h3>In progress</h3>__IN_PROGRESS__</div>
  <div class="card"><h3>Waiting on you</h3>__NEEDS_USER__</div>
  <div class="card"><h3>Ready to start</h3>__READY__</div>
</div>

<div class="grid2">
  <section>
    <h2>Open work by area</h2>
    <p class="note">An issue can carry more than one area label.</p>
    __AREA_BARS__
  </section>
  <section>
    <h2>Issues over time</h2>
    <p class="note">Cumulative issues opened and closed by week. One axis, same unit.</p>
    __BURNUP__
  </section>
</div>

<h2>Ports</h2>
<p class="note">Useful now: the port issue, its <em>Done when</em> checklist and sub-issue progress, and the recorded trace count. Object coverage is still a gap; it sits in the table below.</p>
<div class="filters">
  <input id="port-search" type="search" placeholder="Filter by game name or slug">
  <select id="port-state"><option value="all">All port states</option><option value="open">Open</option><option value="closed">Closed</option><option value="none">No issue</option></select>
  <label><input id="port-traces" type="checkbox"> Has recorded traces</label>
</div>
<table id="port-table">
<thead><tr><th class="num">#</th><th>Game</th><th>Port issue</th><th>Done when</th><th class="num">Sub-issues</th><th class="num">Traces</th><th>Lifecycle</th></tr></thead>
<tbody>__PORT_ROWS__</tbody>
</table>

<details class="table-view">
  <summary>Object coverage (census, scripts, tick accuracy) — mostly not measured yet</summary>
  <p class="note">These need the census, converter and trace exporters. The page never fills a gap with a guess.</p>
  <table>
  <thead><tr><th>Game</th><th>Converter</th><th class="num">Specified</th><th class="num">Scripted</th><th class="num">Tick-accurate</th></tr></thead>
  <tbody>__COVERAGE_ROWS__</tbody>
  </table>
  <p class="note">Overall: __STANDARD__ objects in the standard format (measured across __STANDARD_GAMES__ of 20 games) · __TICK__ objects tick-accurate (measured across __TICK_GAMES__ games).</p>
</details>

<h2>Coverage history</h2>
__COVERAGE_HISTORY__

<footer>
  <p>Reproduce: <code>python -I progress/generate.py</code> reads the committed data, the per-game front-matter and an optional <code>progress/exporter.json</code>; <code>--gh --history</code> refreshes from GitHub and appends a dated point. The same command runs in <code>.github/workflows/progress.yml</code>.</p>
  <p>Follow-ups: census <a href="https://github.com/nugamus/iwbtg-collection/issues/__CENSUS__">#__CENSUS__</a>, trace coverage <a href="https://github.com/nugamus/iwbtg-collection/issues/__TRACEFU__">#__TRACEFU__</a>, converter state <a href="https://github.com/nugamus/iwbtg-collection/issues/__CONVERTER__">#__CONVERTER__</a>, per-game front-matter <a href="https://github.com/nugamus/iwbtg-collection/issues/__FRONTMATTER__">#__FRONTMATTER__</a>.</p>
</footer>
</main>
<script>
const BURN = __BURN_JSON__;
(function(){
  const svg = document.getElementById('burnup'), tip = document.getElementById('burn-tip');
  if(!svg || !tip) return;
  const hits = svg.querySelectorAll('.hit');
  function show(ev, i){
    const p = BURN[i]; if(!p) return;
    tip.textContent = '';
    const b = document.createElement('b'); b.textContent = p.week; tip.appendChild(b);
    const line = document.createElement('div');
    line.textContent = `opened ${p.created} · closed ${p.closed} · open ${p.created - p.closed}`;
    tip.appendChild(line);
    tip.hidden = false;
    tip.style.left = Math.min(ev.clientX + 14, window.innerWidth - 220) + 'px';
    tip.style.top = (ev.clientY + 14) + 'px';
  }
  hits.forEach(h => {
    h.addEventListener('pointermove', e => show(e, +h.dataset.i));
    h.addEventListener('pointerleave', () => { tip.hidden = true; });
  });
})();
(function(){
  const q = document.getElementById('port-search');
  const state = document.getElementById('port-state');
  const traces = document.getElementById('port-traces');
  if(!q) return;
  const rows = [...document.querySelectorAll('#port-table tbody tr')];
  function apply(){
    const text = q.value.trim().toLowerCase();
    rows.forEach(r => {
      const okText = !text || (r.dataset.name || '').includes(text);
      const okState = state.value === 'all' || r.dataset.state === state.value;
      const okTrace = !traces.checked || r.dataset.traces === '1';
      r.hidden = !(okText && okState && okTrace);
    });
  }
  q.addEventListener('input', apply); state.addEventListener('change', apply); traces.addEventListener('change', apply);
})();
</script>
</body>
</html>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gh", action="store_true", help="refresh from GitHub")
    ap.add_argument("--history", action="store_true", help="append a dated snapshot")
    args = ap.parse_args()
    at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    data = None
    if args.gh:
        try:
            data = build_data(True)
        except Exception as exc:
            print(f"warning: GitHub read failed ({exc}); using committed data.json", file=sys.stderr)
    if data is None:
        path = os.path.join(PROGRESS, "data.json")
        if os.path.exists(path):
            data = json.load(open(path, encoding="utf-8"))
        else:
            data = build_data(False)
    data["generated_at"] = at
    json.dump(data, open(os.path.join(PROGRESS, "data.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    hist = maybe_history(data, at) if args.history else (
        json.load(open(os.path.join(PROGRESS, "history.json"), encoding="utf-8"))
        if os.path.exists(os.path.join(PROGRESS, "history.json")) else [])
    page = render(data, hist)
    open(os.path.join(PROGRESS, "index.html"), "w", encoding="utf-8", newline="").write(page)
    print(f"wrote progress/data.json, progress/index.html ({len(data['games'])} games, "
          f"{len(data['phases'])} phases, source={data['source']})")


if __name__ == "__main__":
    main()
