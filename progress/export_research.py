#!/usr/bin/env python3
"""Export real per-game object coverage from the research checkout.

The converter packages are not committed, but the research repo already holds the
census evidence:

  * `shortlist.md` — custom objects, rooms, engine, genre and difficulty per game;
  * `specs/mechanics.md` — the game x built-in placed-instance coverage table;
  * `specs/engines/gm8/traces/<slug>/*.csv` — the recorded traces, and the object
    names each trace exercises.

This script turns those into `progress/exporter.json`, which `progress/generate.py`
reads. It writes aggregate counts only; no original game data is copied.

    python -I progress/export_research.py \
        --research ../iwbtg-collection-research \
        --out progress/exporter.json
"""

import argparse
import csv
import datetime as dt
import glob
import json
import os
import re
import sys

ABBR = ["RTN", "JUN", "KOT", "KAE", "LEM", "RORI", "WAN", "FED", "LT", "MIKU",
        "LR", "POR", "MAC", "NY", "ANI", "FT", "RBF", "SHO", "SPK", "XIA"]
SLUGS = ["rely-the-needle-4-2", "go-the-jun-g-world", "deride-kotaro-from-aslant",
         "kaenbyou", "eat-the-lemon", "eat-the-rori-girl", "wanna",
         "be-fed-up-with-path-w", "little-trap", "clear-the-easy-miku-2",
         "little-runner", "poronkusema", "machinary", "eat-the-new-year",
         "animus", "help-the-fairy-tale", "get-the-rainbow-feather", "showdown",
         "go-the-spook-house", "keep-celebrating-for-xia"]


def num(cell):
    cell = cell.strip()
    if not cell or cell == "-":
        return 0
    return int(cell)


def parse_shortlist(path):
    rows = {}
    for line in open(path, encoding="utf-8"):
        if not re.match(r"^\| \d+ \|", line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 10:
            continue
        order = int(cells[0])
        rows[order] = {
            "genre": cells[2],
            "difficulty": float(cells[3]) if cells[3] else None,
            "rooms": int(cells[4]) if cells[4].isdigit() else None,
            "custom_objects": int(cells[5]) if cells[5].isdigit() else None,
            "engine": cells[6],
        }
    return rows


def parse_mechanics(path):
    rows = {}
    for line in open(path, encoding="utf-8"):
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 23:
            continue
        name = cells[0]
        if name == "Built-in" or not name:
            continue
        vals = cells[1:21]
        if not all(re.fullmatch(r"-|\d+", v) for v in vals):
            continue
        rows[name] = vals
    return rows


def trace_stats(trace_root, slug):
    files = sorted(glob.glob(os.path.join(trace_root, slug, "*.csv")))
    objects = set()
    for path in files:
        try:
            with open(path, encoding="utf-8", newline="") as f:
                reader = csv.DictReader(f)
                field = next((n for n in (reader.fieldnames or []) if n.strip().lower() == "object"), None)
                if not field:
                    continue
                for row in reader:
                    value = (row.get(field) or "").strip()
                    if value:
                        objects.add(value)
        except (OSError, UnicodeDecodeError, csv.Error):
            continue
    return len(files), len(objects), sorted(objects)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--research", default="../iwbtg-collection-research")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "exporter.json"))
    args = ap.parse_args()
    research = os.path.abspath(args.research)
    shortlist = parse_shortlist(os.path.join(research, "shortlist.md"))
    mechanics = parse_mechanics(os.path.join(research, "specs", "mechanics.md"))
    trace_root = os.path.join(research, "specs", "engines", "gm8", "traces")
    if "All placed instances" not in mechanics:
        raise SystemExit("mechanics.md coverage table not found")
    games = {}
    totals = {"placed": 0, "builtin": 0, "script": 0, "client": 0, "kid": 0,
              "custom": 0, "traces": 0, "exercised": 0, "rooms": 0}
    all_trace_objects = set()
    for i, (abbr, slug) in enumerate(zip(ABBR, SLUGS)):
        placed = num(mechanics["All placed instances"][i])
        script = num(mechanics["script objects"][i])
        client = num(mechanics["client.difficulty_warp"][i]) + num(mechanics["client.system"][i])
        kid = num(mechanics["placed kid (player)"][i])
        builtin = sum(num(v[i]) for name, v in mechanics.items() if name.startswith("builtin."))
        builtin_types = sum(1 for name, v in mechanics.items()
                            if name.startswith("builtin.") and v[i] not in ("", "-"))
        traces, exercised, names = trace_stats(trace_root, slug)
        sl = shortlist.get(i + 1, {})
        rec = {
            "order": i + 1,
            "abbr": abbr,
            "genre": sl.get("genre"),
            "difficulty": sl.get("difficulty"),
            "rooms": sl.get("rooms"),
            "custom_objects": sl.get("custom_objects"),
            "engine": sl.get("engine"),
            "placed_instances": placed,
            "builtin_instances": builtin,
            "builtin_pct": round(100.0 * builtin / placed, 1) if placed else None,
            "builtin_types": builtin_types,
            "script_instances": script,
            "script_pct": round(100.0 * script / placed, 2) if placed else None,
            "client_instances": client,
            "kid_instances": kid,
            "traces_recorded": traces,
            "objects_exercised": exercised,
            "trace_objects": names,
        }
        games[slug] = rec
        totals["placed"] += placed
        totals["builtin"] += builtin
        totals["script"] += script
        totals["client"] += client
        totals["kid"] += kid
        totals["custom"] += sl.get("custom_objects") or 0
        totals["traces"] += traces
        totals["exercised"] += exercised
        totals["rooms"] += sl.get("rooms") or 0
        all_trace_objects.update(names)
    overall = {
        "games": len(games),
        "placed_instances": totals["placed"],
        "builtin_instances": totals["builtin"],
        "builtin_pct": round(100.0 * totals["builtin"] / totals["placed"], 1),
        "script_instances": totals["script"],
        "script_pct": round(100.0 * totals["script"] / totals["placed"], 2),
        "client_instances": totals["client"],
        "kid_instances": totals["kid"],
        "custom_objects": totals["custom"],
        "rooms": totals["rooms"],
        "traces_recorded": totals["traces"],
        "objects_exercised": totals["exercised"],
        "objects_exercised_unique": len(all_trace_objects),
        "games_with_traces": sum(1 for g in games.values() if g["traces_recorded"]),
        "games_with_census": sum(1 for g in games.values() if g["placed_instances"]),
    }
    out = {
        "generated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
                         .isoformat().replace("+00:00", "Z"),
        "source": {
            "shortlist": "shortlist.md",
            "mechanics": "specs/mechanics.md",
            "traces": "specs/engines/gm8/traces",
        },
        "games": games,
        "overall": overall,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    json.dump(out, open(args.out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print(f"wrote {args.out}: {len(games)} games, {overall['placed_instances']} placed instances, "
          f"{overall['builtin_pct']}% built-in, {overall['traces_recorded']} traces")


if __name__ == "__main__":
    main()
