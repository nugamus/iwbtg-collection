# Progress dashboard

`index.html` is the published object-coverage and progress page. It is generated, not hand-edited.

```sh
python -I progress/export_research.py --research ../iwbtg-collection-research
python -I progress/generate.py --gh --history
```

- `exporter.json` is the per-game object-coverage export from the research checkout.
- `data.json` is the last generated snapshot (object coverage, phase counts, size points, per-game metrics, overall figures).
- `history.json` is one dated snapshot per `--history` run, deduplicated when a run adds nothing.
- `index.html` is static and self-contained: no CDN, no secrets, no client-side API calls.
- `.github/workflows/progress.yml` runs the `--gh --history` form daily and on demand, then publishes `progress/` to GitHub Pages.

## Honesty rules

Every number is a numerator and denominator. A missing source renders **not measured yet** and links to the follow-up issue that will fill it. The generator never infers a denominator and never fills a gap with zero.

## What the page shows now

- **Object coverage** — per game: placed instances and the share covered by a documented built-in (`specs/mechanics.md`), script-object instances, custom object types (`shortlist.md`), recorded traces, and the distinct object names those traces exercise.
- **Passing-trace coverage** — not measured yet: it needs converted packages and the trace runner (#1365). Recorded traces and the objects they exercise are shown instead.
- **Phase progress** — closed/total issues and remaining size points per phase, with the entry and exit condition behind *Gate*.
- **Ports** — each port issue, its *Done when* progress, and its state, joined into the coverage table.
- **What is moving** — recently closed, in-progress, needs-user and unblocked agent-ready issues.
- **Open work by area** and a weekly issue burn-up.
- **Coverage history** — built-in coverage and script-object share per run.

## Per-game front-matter

Each file in `docs/games/<slug>.md` starts with:

```yaml
---
id: rely-the-needle-4-2
game_id: 322
order: 1
name: "I Wanna Rely The Needle 4.2"
slug: rely-the-needle-4-2
source: https://delicious-fruit.com/game/19483
engine: gm8
status: unknown
objects_total: null
objects_specced: null
objects_scripted: null
traces_passing: null
traces_total: null
---
```

`status` is the lifecycle (`converted`, `trace-checked`, `Beta`, `Ranked`, `Loved`) or `unknown`. `objects_total` is the census; `objects_specced` counts objects with a behavior spec; `objects_scripted` counts objects with a package script; `traces_passing` counts objects with a passing trace; `traces_total` counts objects trace-checked. `null` and `unknown` mean not measured yet. `progress/exporter.json` is read first; these fields are the fallback and the place for package-based metrics later. Filling them is tracked by issue #1367.

## Object-coverage export

`progress/export_research.py` reads the research checkout and writes aggregate counts only:

```sh
python -I progress/export_research.py --research ../iwbtg-collection-research --out progress/exporter.json
```

Sources:

- `shortlist.md` — custom objects, rooms, engine, genre and difficulty per game;
- `specs/mechanics.md` — the game × built-in placed-instance coverage table (SCAN, 2026-10-24);
- `specs/engines/gm8/traces/<slug>/*.csv` — recorded traces and the distinct `object` names they exercise.

Schema:

```json
{
  "generated_at": "2026-11-24T00:00:00Z",
  "games": {
    "rely-the-needle-4-2": {
      "order": 1, "abbr": "RTN", "genre": "Needle", "difficulty": 31.24, "rooms": 17,
      "engine": "gm8 8.0", "custom_objects": 5,
      "placed_instances": 1997, "builtin_instances": 1994, "builtin_pct": 99.4,
      "builtin_types": 18, "script_instances": 3, "script_pct": 0.15,
      "client_instances": 0, "kid_instances": 0,
      "traces_recorded": 284, "objects_exercised": 6, "trace_objects": ["player", "..."]
    }
  },
  "overall": { }
}
```

| Field | Meaning |
|---|---|
| `placed_instances` | every placed instance in the game (SCAN) |
| `builtin_instances`, `builtin_pct` | placed instances covered by a built-in row |
| `builtin_types` | distinct built-in kinds placed |
| `script_instances`, `script_pct` | script-object instances and their share |
| `client_instances`, `kid_instances` | client objects and placed kids |
| `custom_objects` | custom object types from `shortlist.md` |
| `traces_recorded` | CSV files in the game's trace directory |
| `objects_exercised`, `trace_objects` | distinct object names in those CSVs |
| `rooms`, `genre`, `difficulty`, `engine` | `shortlist.md` |

Only aggregate counts are committed; no original game content is copied. Passing-trace coverage (#1365) and converter state (#1366) still need converted packages.

## Phase metrics

- Open and closed counts come from the main repo's phase milestones.
- **Remaining size points** sum the open issues' size labels: `size:S = 1`, `size:M = 3`, `size:L = 8`. An issue with no size label counts 0 and is shown in its own column.
- The page calls this a planning estimate, never a date. The roadmap has no deadlines.

## Publishing

The workflow restores the durable `history.json` from `gh-pages`, generates with `--gh --require-gh` (a failed GitHub read fails the run instead of publishing stale data), then commits `index.html`, `data.json`, `history.json` and `.nojekyll` to `gh-pages` and requests a Pages build. Pages serves that branch at <https://nugamus.github.io/iwbtg-collection/>.

It deliberately does **not** write to `main`: the `main: required checks` ruleset requires `ci-ok` and `body`, and `github-actions[bot]` is not on its bypass list. `progress/history.json` on `main` is the seed for the first run; the durable history lives on `gh-pages`, so each daily run appends a point. A failed branch push or a failed build request fails the run visibly; there is no silent fallback.

If Pages is ever reset, point it at the branch once:

```sh
gh api -X PUT repos/nugamus/iwbtg-collection/pages \
  -f build_type=legacy -f 'source[branch]=gh-pages' -f 'source[path]=/'
```

The generated page is also readable straight from the repo at `progress/index.html`.
