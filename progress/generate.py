#!/usr/bin/env python3
"""Build the porting-progress dashboard.

Reproducible from committed files plus `gh`: no secrets, no hand-edited numbers.
Every number is a numerator/denominator pair; a missing source renders
"not measured yet" and links to the follow-up issue that will fill it.

    python -I progress/generate.py              # read committed data.json + game files
    python -I progress/generate.py --gh         # refresh phase counts from GitHub
    python -I progress/generate.py --gh --history   # also append a dated snapshot

Optional per-game data comes from `progress/exporter.json` (see README.md).
Per-game front-matter lives in `docs/games/<slug>.md`.
"""

import argparse
import datetime as dt
import html
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROGRESS = os.path.join(ROOT, "progress")
GAMES = os.path.join(ROOT, "docs", "games")
REPO = "nugamus/iwbtg-collection"
SIZE_POINTS = {"size:S": 1, "size:M": 3, "size:L": 8}
FOLLOWUPS = {"census": 1364, "traces": 1365, "converter": 1366, "frontmatter": 1367}
NOT_MEASURED = "not measured yet"


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
        rec = {
            "id": fm.get("id") or slug,
            "slug": slug,
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
        }
        games.append(rec)
    return games


def pct(num, den):
    if num is None or not den:
        return None
    return round(100.0 * num / den, 1)


def gh_json(args):
    out = subprocess.run(["gh", "api", "--paginate", "--slurp"] + args,
                         capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise RuntimeError(out.stderr.strip()[:300])
    pages = json.loads(out.stdout)
    return [x for page in pages for x in page]


def gh_phases():
    milestones = gh_json([f"repos/{REPO}/milestones?state=all&per_page=100"])
    issues = gh_json([f"repos/{REPO}/issues?state=all&per_page=100"])
    issues = [i for i in issues if "pull_request" not in i]
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
            hit = None
            for s in ("size:L", "size:M", "size:S"):
                if s in labels:
                    hit = s
                    break
            if hit:
                sizes[hit] += 1
                points += SIZE_POINTS[hit]
            else:
                sizes["unknown"] += 1
        phases.append({
            "number": int(m["title"].split(" · ")[0]) if " · " in m["title"] else None,
            "title": m["title"],
            "state": m["state"],
            "open": len(opened),
            "closed": len(rows) - len(opened),
            "size_points": points,
            "sizes": sizes,
        })
    return phases


def load_phases(refresh):
    if refresh:
        try:
            return gh_phases(), "gh"
        except Exception as exc:  # keep the page reproducible offline
            print(f"warning: GitHub read failed ({exc}); using committed data.json", file=sys.stderr)
    path = os.path.join(PROGRESS, "data.json")
    if os.path.exists(path):
        old = json.load(open(path, encoding="utf-8"))
        return old.get("phases", []), "committed data.json"
    return [], "none"


def overall(games):
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
        "games_tick": len(tick),
        "games_total": len(games),
    }


def snapshot(phases, ov, at):
    return {
        "at": at,
        "standard_pct": ov["standard_pct"],
        "tick_pct": ov["tick_pct"],
        "games_census": ov["games_census"],
        "games_tick": ov["games_tick"],
        "open_issues": sum(p["open"] for p in phases),
        "size_points": sum(p["size_points"] for p in phases),
        "phases": [{"number": p["number"], "title": p["title"], "open": p["open"],
                    "closed": p["closed"], "size_points": p["size_points"]} for p in phases],
    }


def maybe_history(phases, ov, at):
    path = os.path.join(PROGRESS, "history.json")
    hist = json.load(open(path, encoding="utf-8")) if os.path.exists(path) else []
    snap = snapshot(phases, ov, at)
    keep = {k: snap[k] for k in ("standard_pct", "tick_pct", "open_issues", "size_points",
                                 "games_census", "games_tick")}
    if hist:
        last = {k: hist[-1].get(k) for k in keep}
        if last == keep and hist[-1]["at"][:10] == at[:10]:
            return hist
    hist.append(snap)
    json.dump(hist, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    return hist


def esc(value):
    return html.escape("" if value is None else str(value))


def cell(num, den, followup):
    if num is None or not den:
        return (f'<span class="missing"><a href="https://github.com/{REPO}/issues/{followup}">'
                f'{NOT_MEASURED}</a></span>')
    return f'<strong>{pct(num, den)}%</strong> <span class="frac">({num} / {den})</span>'


def lifecycle_class(value):
    v = (value or "unknown").lower()
    if v in ("ranked", "loved"):
        return "good"
    if v in ("beta", "trace-checked", "converted"):
        return "mid"
    return "muted"


def chart(hist, key, label, colour):
    pts = [(h["at"][:10], h.get(key)) for h in hist if h.get(key) is not None]
    if not pts:
        return '<p class="muted">No measured points yet.</p>'
    if len(pts) == 1:
        d, v = pts[0]
        return f'<p class="muted">One point ({esc(d)}: {esc(v)}%). The next run adds a line.</p>'
    w, h, x0, y0, x1, y1 = 760, 190, 46, 12, 748, 158
    n = len(pts)
    xs = [x0 + (x1 - x0) * i / (n - 1) for i in range(n)]
    ys = [y1 - (y1 - y0) * max(0.0, min(100.0, float(v))) / 100.0 for _, v in pts]
    path = " ".join(("M" if i == 0 else "L") + f"{x:.1f},{y:.1f}" for i, (x, y) in enumerate(zip(xs, ys)))
    grid = "".join(
        f'<line x1="{x0}" y1="{y1 - (y1-y0)*p/100:.0f}" x2="{x1}" y2="{y1 - (y1-y0)*p/100:.0f}" class="grid"/>'
        f'<text x="{x0-6}" y="{y1 - (y1-y0)*p/100 + 4:.0f}" class="axis" text-anchor="end">{p}</text>'
        for p in (0, 25, 50, 75, 100))
    ticks = "".join(f'<text x="{x:.0f}" y="{y1+18}" class="axis" text-anchor="middle">{esc(d[5:])}</text>'
                    for x, (d, _) in zip(xs[:: max(1, n // 6)], pts[:: max(1, n // 6)]))
    dots = "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.5" fill="{colour}"/>' for x, y in zip(xs, ys))
    return (f'<svg viewBox="0 0 {w} {h+28}" role="img" aria-label="{esc(label)} over time">'
            f'{grid}<path d="{path}" fill="none" stroke="{colour}" stroke-width="2"/>{dots}{ticks}</svg>')


def open_chart(hist):
    pts = [(h["at"][:10], h.get("open_issues")) for h in hist if h.get("open_issues") is not None]
    if not pts:
        return '<p class="muted">No points yet.</p>'
    w, h, x0, y0, x1, y1 = 760, 150, 46, 12, 748, 120
    top = max(v for _, v in pts) or 1
    n = len(pts)
    bw = (x1 - x0) / n * 0.6
    bars = []
    for i, (d, v) in enumerate(pts):
        cx = x0 + (x1 - x0) * (i + 0.5) / n
        bh = (y1 - y0) * v / top
        bars.append(f'<rect x="{cx-bw/2:.1f}" y="{y1-bh:.1f}" width="{bw:.1f}" height="{bh:.1f}" fill="#7c8cff"/>')
        bars.append(f'<text x="{cx:.0f}" y="{y1+16}" class="axis" text-anchor="middle">{esc(d[5:])}</text>')
    return (f'<svg viewBox="0 0 {w} {h+20}" role="img" aria-label="Open issues over time">'
            f'<line x1="{x0}" y1="{y1}" x2="{x1}" y2="{y1}" class="grid"/>'
            + "".join(bars) +
            f'<text x="{x0-6}" y="{y0+4}" class="axis" text-anchor="end">{top}</text></svg>')


def render(phases, games, ov, hist, at, source):
    total_open = sum(p["open"] for p in phases)
    total_closed = sum(p["closed"] for p in phases)
    total_points = sum(p["size_points"] for p in phases)
    known = sum(p["sizes"]["size:S"] + p["sizes"]["size:M"] + p["sizes"]["size:L"] for p in phases)
    unknown = sum(p["sizes"]["unknown"] for p in phases)

    phase_rows = "".join(
        f'<tr><td>{esc(p["title"])}</td><td class="num">{p["open"]}</td>'
        f'<td class="num">{p["closed"]}</td><td class="num">{p["size_points"]}</td>'
        f'<td class="num">{p["sizes"]["size:S"]}</td><td class="num">{p["sizes"]["size:M"]}</td>'
        f'<td class="num">{p["sizes"]["size:L"]}</td><td class="num">{p["sizes"]["unknown"]}</td></tr>'
        for p in phases)

    game_rows = "".join(
        f'<tr><td><a href="{esc(g["source"] or "https://github.com/" + REPO + "/tree/main/docs/games")}">'
        f'{esc(g["name"])}</a><div class="slug">{esc(g["slug"])}</div></td>'
        f'<td>{esc(g["engine"])}</td>'
        f'<td>{("converted" if g["converted"] else "not converted") if g["converted"] is not None else cell(0,0,FOLLOWUPS["converter"])}</td>'
        f'<td class="num">{cell(g["objects_specced"], g["objects_total"], FOLLOWUPS["census"])}</td>'
        f'<td class="num">{cell(g["objects_scripted"], g["objects_total"], FOLLOWUPS["census"])}</td>'
        f'<td class="num">{cell(g["traces_passing"], g["objects_total"], FOLLOWUPS["traces"])}</td>'
        f'<td><span class="pill {lifecycle_class(g["lifecycle"])}">{esc(g["lifecycle"])}</span></td></tr>'
        for g in games)

    std_cell = cell(ov["standard_num"], ov["standard_den"], FOLLOWUPS["census"])
    tick_cell = cell(ov["tick_num"], ov["tick_den"], FOLLOWUPS["traces"])
    measured = (f'measured across {ov["games_census"]} of {ov["games_total"]} games'
                if ov["games_census"] else f'measured across 0 of {ov["games_total"]} games')

    return f"""<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>IWBTG Collection — porting progress</title>
<style>
:root{{--bg:#0f1117;--panel:#171a23;--line:#2a2f3d;--text:#e7e9ee;--muted:#9aa3b5;--accent:#7c8cff;--good:#4ade80;--mid:#fbbf24}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--bg);color:var(--text);font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}}
main{{max-width:1080px;margin:0 auto;padding:32px 20px 64px}}
h1{{font-size:26px;margin:0 0 4px}} h2{{font-size:19px;margin:34px 0 10px}}
p{{color:var(--muted)}} a{{color:var(--accent)}}
.cards{{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:14px;margin:22px 0}}
.card{{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:16px}}
.card .big{{font-size:28px;font-weight:650;color:var(--text);margin:6px 0}}
.card .sub{{font-size:12.5px;color:var(--muted)}}
table{{width:100%;border-collapse:collapse;background:var(--panel);border:1px solid var(--line);border-radius:10px;overflow:hidden}}
th,td{{padding:9px 10px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}}
th{{font-size:12px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);background:#12151d}}
tr:last-child td{{border-bottom:0}}
td.num{{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}}
.frac,.slug,.muted{{color:var(--muted);font-size:12.5px}}
.slug{{margin-top:2px}}
.missing a{{color:var(--mid)}}
.pill{{display:inline-block;padding:2px 8px;border-radius:999px;font-size:12px;border:1px solid var(--line)}}
.pill.good{{color:var(--good);border-color:#2f6b45}} .pill.mid{{color:var(--mid);border-color:#6b5a2f}} .pill.muted{{color:var(--muted)}}
.grid{{stroke:#2a2f3d;stroke-width:1}} .axis{{fill:#9aa3b5;font-size:10px}}
.note{{font-size:12.5px;color:var(--muted);margin-top:8px}}
.two{{display:grid;grid-template-columns:1fr 1fr;gap:18px}}
@media(max-width:800px){{.two{{grid-template-columns:1fr}}}}
code{{background:#12151d;border:1px solid var(--line);border-radius:4px;padding:1px 5px;font-size:12.5px}}
</style>
<main>
<h1>IWBTG Collection — porting progress</h1>
<p>Generated {esc(at)} from <strong>{esc(source)}</strong>. Every number is a numerator and denominator. A missing source says <span class="missing">{NOT_MEASURED}</span> and links to the issue that will fill it. No hand-edited values.</p>

<div class="cards">
  <div class="card"><div class="sub">Objects in the standard format</div><div class="big">{std_cell}</div>
    <div class="sub">{esc(measured)}</div></div>
  <div class="card"><div class="sub">Objects tick-accurate</div><div class="big">{tick_cell}</div>
    <div class="sub">measured across {ov["games_tick"]} of {ov["games_total"]} games</div></div>
  <div class="card"><div class="sub">Open / closed issues in phases</div><div class="big">{total_open} / {total_closed}</div>
    <div class="sub">{total_points} remaining size points</div></div>
</div>

<h2>Phases</h2>
<table>
<thead><tr><th>Phase</th><th class="num">Open</th><th class="num">Closed</th><th class="num">Size points</th>
<th class="num">S</th><th class="num">M</th><th class="num">L</th><th class="num">No size</th></tr></thead>
<tbody>{phase_rows}</tbody></table>
<p class="note"><strong>How long:</strong> remaining work is shown as open issue counts and size points, a planning estimate, not a date. There are no deadlines. Convention: <code>size:S = 1</code>, <code>size:M = 3</code>, <code>size:L = 8</code>; an issue with no size label counts 0 points. {known} open issues carry a size; {unknown} do not.</p>

<h2>Per game</h2>
<p>All {len(games)} games from <code>shortlist.json</code>. <em>Specified</em> = objects with a behavior spec ÷ census. <em>Scripted</em> = objects with a package script ÷ census. <em>Tick-accurate</em> = objects with a passing trace ÷ census. The census and trace exporters are follow-up issues, so most cells are honest gaps on day one.</p>
<table>
<thead><tr><th>Game</th><th>Engine</th><th>Converter</th><th class="num">Specified</th><th class="num">Scripted</th><th class="num">Tick-accurate</th><th>Lifecycle</th></tr></thead>
<tbody>{game_rows}</tbody></table>

<h2>History</h2>
<div class="two">
  <div><h3>Coverage over time</h3>{chart(hist, "standard_pct", "Objects in the standard format", "#7c8cff")}
    <p class="note">Objects in the standard format, %. Gaps mean that run had no measurement.</p></div>
  <div><h3>Tick accuracy over time</h3>{chart(hist, "tick_pct", "Objects tick-accurate", "#4ade80")}
    <p class="note">Objects tick-accurate, %. Gaps mean that run had no measurement.</p></div>
</div>
<h3>Open issues over time</h3>
{open_chart(hist)}
<p class="note">One dated snapshot per generator run with <code>--history</code>. The daily workflow appends one after each refresh.</p>

<h2>Reproduce this page</h2>
<p>From the client repo: <code>python -I progress/generate.py</code> reads the committed <code>data.json</code>, the per-game front-matter and an optional <code>progress/exporter.json</code>. <code>python -I progress/generate.py --gh --history</code> refreshes phase counts from GitHub and appends a history point. The same command runs in <code>.github/workflows/progress.yml</code> and publishes to GitHub Pages. Exporter schema and commands are in <code>progress/README.md</code>.</p>
<p class="note">Follow-ups: census <a href="https://github.com/{REPO}/issues/{FOLLOWUPS["census"]}">#{FOLLOWUPS["census"]}</a>, trace coverage <a href="https://github.com/{REPO}/issues/{FOLLOWUPS["traces"]}">#{FOLLOWUPS["traces"]}</a>, converter state <a href="https://github.com/{REPO}/issues/{FOLLOWUPS["converter"]}">#{FOLLOWUPS["converter"]}</a>, per-game front-matter <a href="https://github.com/{REPO}/issues/{FOLLOWUPS["frontmatter"]}">#{FOLLOWUPS["frontmatter"]}</a>.</p>
</main>
</html>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gh", action="store_true", help="refresh phase counts from GitHub")
    ap.add_argument("--history", action="store_true", help="append a dated snapshot to history.json")
    args = ap.parse_args()

    at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    games = load_games()
    phases, source = load_phases(args.gh)
    ov = overall(games)
    data = {"generated_at": at, "source": source, "phases": phases, "games": games,
            "overall": ov, "followups": FOLLOWUPS}
    json.dump(data, open(os.path.join(PROGRESS, "data.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    hist = maybe_history(phases, ov, at) if args.history else (
        json.load(open(os.path.join(PROGRESS, "history.json"), encoding="utf-8"))
        if os.path.exists(os.path.join(PROGRESS, "history.json")) else [])
    page = render(phases, games, ov, hist, at, source)
    open(os.path.join(PROGRESS, "index.html"), "w", encoding="utf-8", newline="").write(page)
    print(f"wrote progress/data.json, progress/index.html ({len(games)} games, {len(phases)} phases, source={source})")


if __name__ == "__main__":
    main()
