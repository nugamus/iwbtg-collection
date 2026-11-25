# Progress dashboard

`index.html` is the published build ledger: per-game implementation bars, object coverage, phase delivery and issue activity. It is generated, not hand-edited.

```sh
python -I progress/export_research.py --research ../iwbtg-collection-research
python -I progress/generate.py --gh --history
python -I progress/generate.py --render-only
```

- `exporter.json` is the per-game object-coverage export from the research checkout.
- `data.json` is the last generated snapshot (object coverage, phase counts, size points, per-game metrics, overall figures).
- `history.json` is one dated snapshot per `--history` run, deduplicated when a run adds nothing.
- `index.html` is static: no CDN, no secrets, no client-side API calls. Its Barlow and Barlow Condensed fonts are committed under `progress/fonts/` with their OFL licence and copied with the page.
- `--render-only` re-renders `index.html` from the committed `data.json` and `history.json` without refreshing or rewriting either file. Use it for local design review.
- `.github/workflows/progress.yml` runs the `--gh --history` form daily and on demand, then publishes `progress/` to GitHub Pages.

## Honesty rules

Every rate is a numerator and denominator. A missing source renders **not measured yet** and links to the follow-up issue that will fill it. The generator never infers a denominator and never fills a gap with zero.

## What the page shows now

- **First-viewport progress register** — pre-release status and three direct bars with denominators: issues closed across all phases, all port checklist items, and the public-alpha gate at phase 7. Each bar links to its full register and is labelled as a delivery measure, not a release date or a single completion score. A scope line carries the game-specific work, room and trace counts, and the passing-trace gap keeps its follow-up.
- **Coverage** — a composition summary for placed instances, then a filterable, expandable game menu led by each game's game-specific work: script-object instances and custom object types (`shortlist.md`). Each game row carries three repo-derived **implementation bars** (trace cases passing, mapped objects and scripted objects), its port checklist meter, and the expanded facts behind them. The all-games trace-case figure sits in the implementation note, not the three-cell delivery register. Recorded traces and exercised names are in the row; placed instances, the built-in share and the script share are in the expanded detail, labelled as composition rather than completion. Built-in coverage stays visible as context, not as a progress score.
- **Script API census** — the registered function count from `docs/format/script-api.md` and the census groups named as not registered in `docs/architecture/sim/scripts.md`. The implemented share of the script API is marked **not measured yet** because the census's full group list lives in the private research repo.
- **Delivery** — each phase's closed/total issues, remaining size points and open count, with entry and exit conditions behind *Entry and exit conditions*. Port issues, their *Done when* progress and their state stay in the coverage menu.
- **Activity** — recently closed, in-progress, needs-user and unblocked agent-ready issues; open work by area; a weekly issue burn-up; and the coverage history per run.
- **Sources** — regeneration commands, data sources, follow-up issues and the generated/source timestamps.

## Implementation bars

Every bar has a numerator and denominator from files in this repo, read at build time:

| Bar | Numerator | Denominator | Moves when |
|---|---|---|---|
| Trace cases | `OK` lines for the game in `crates/tools/fidelity-baseline.txt` | `*.toml` files in `crates/tools/cases/<game>/` | a case is merged or its blessed status changes |
| Mapped objects | `objects_specced` in `docs/games/<slug>.md` (exporter `objects_in_package - objects_unmapped`) | `objects_total` (exporter `objects_in_package`) | `progress/export_research.py` regenerates the exporter and the front-matter is refreshed |
| Scripted objects | `objects_scripted` in `docs/games/<slug>.md` (exporter `objects_unmapped`) | `objects_total` | the same exporter refresh |
| Port checklist | checked `- [x]` items in the port issue's *Done when* | all checklist items | the games-repo port issue changes |

Mapped and scripted objects partition the package total. A game with no case directory shows `not measured yet · no case directory`; it is not counted as zero in the all-games trace figure. A game without an exporter or front-matter value shows `not measured yet`, never zero.

The script API census has no honest denominator in this repo: `docs/architecture/sim/scripts.md` lists seven groups that are not registered, but the census's complete group inventory is in the private research repo (research#172). The page shows the registered function count from `docs/format/script-api.md` and the seven named groups, and marks the implemented share **not measured yet**.

### Snapshot caveat

`progress/exporter.json` is generated in the research environment (`python -I progress/export_research.py --research ../iwbtg-collection-research`) and is not regenerated in this repo or by the progress workflow. The page prints its `generated_at` timestamp next to the package bars. Trace-case counts are read from `crates/tools/cases/` and `crates/tools/fidelity-baseline.txt` on every run, so they move as soon as a case or status lands. The `docs/games` front-matter mirrors the exporter snapshot (#1550); refreshing the exporter requires a matching front-matter refresh for the bars to move.

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
status: converted
objects_total: 39
objects_specced: 33
objects_scripted: 6
traces_passing: 4
traces_total: 4
---
```

`status` is the lifecycle (`converted`, `trace-checked`, `Beta`, `Ranked`, `Loved`) or `unknown`. `objects_total` is the package total; `objects_specced` counts objects resolved to a built-in behavior spec; `objects_scripted` counts objects the package writes as scripts; `traces_passing` counts recorded cases that pass; `traces_total` counts recorded cases. All 20 pages were filled from the exporter snapshot in #1550. `null` and `unknown` still mean not measured yet, never zero. The generator uses these fields for the mapped/scripted bars and falls back to `progress/exporter.json` when a field is absent.

## Object-coverage export

`progress/export_research.py` reads the research checkout and writes aggregate counts only:

```sh
python -I progress/export_research.py --research ../iwbtg-collection-research --out progress/exporter.json
```

Sources:

- `shortlist.md` — custom objects, rooms, engine, genre and difficulty per game;
- `specs/mechanics/coverage.md` — the game × built-in placed-instance coverage table (SCAN, 2026-10-24);
- `specs/engines/gm8/traces/<slug>/*.csv` — recorded traces and the distinct `object` names they exercise;
- `build/reports/summary.json` — the converter's real `iwbtg pkg check` result per game.

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
      "traces_recorded": 284, "objects_exercised": 6, "trace_objects": ["player", "..."],
      "converted": true, "objects_in_package": 39, "objects_unmapped": 6,
      "check_errors": 0, "check_warnings": 2
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
| `converted` | `true` when `pkg check` passed on the converted package, else `false`; absent when the report is missing |
| `objects_in_package` | objects in the converted package (built-in, preset, infra and unmapped) |
| `objects_unmapped` | the part of `objects_in_package` still unmapped; mapped = the difference |
| `check_errors`, `check_warnings` | `pkg check` diagnostic counts |

Converter state comes from `build/reports/summary.json` in the research checkout, which `uv run iwc convert` writes. Each entry carries the result of a real `iwbtg pkg check` on the written package (`client_check`); the roadmap text is never read. If the report is missing, the converter fields are left out and the page shows `unknown`; if the report exists but has no entry for a game, that game is `converted: false`.

Only aggregate counts are committed; no original game content is copied. Passing-trace coverage (#1365) still needs the trace runner.

## Phase metrics

- Open and closed counts come from the main repo's phase milestones.
- **Remaining size points** sum the open issues' size labels: `size:S = 1`, `size:M = 3`, `size:L = 8`. An issue with no size label counts 0 and is shown in its own column.
- The page calls this a planning estimate, never a date. The roadmap has no deadlines.

## Publishing

The workflow restores the durable `history.json` from `gh-pages`, generates with `--gh --require-gh` (a failed GitHub read fails the run instead of publishing stale data), then commits `index.html`, `data.json`, `history.json`, `fonts/` and `.nojekyll` to `gh-pages` and requests a Pages build. Pages serves that branch at <https://nugamus.github.io/iwbtg-collection/>.

CI's repository token cannot read the private games repo. The generator keeps the last committed port data and still refreshes the public-repo phases, activity and coverage; rerun `--gh` locally with games-repo access to refresh the port column.

It deliberately does **not** write to `main`: the `main: required checks` ruleset requires `ci-ok` and `body`, and `github-actions[bot]` is not on its bypass list. `progress/history.json` on `main` is the seed for the first run; the durable history lives on `gh-pages`, so each daily run appends a point. A failed branch push or a failed build request fails the run visibly; there is no silent fallback.

If Pages is ever reset, point it at the branch once:

```sh
gh api -X PUT repos/nugamus/iwbtg-collection/pages \
  -f build_type=legacy -f 'source[branch]=gh-pages' -f 'source[path]=/'
```

The generated page is also readable straight from the repo at `progress/index.html`.
