# Progress dashboard

`index.html` is the published porting-progress page. It is generated, not hand-edited.

```sh
python -I progress/generate.py                  # committed data.json + docs/games/*.md
python -I progress/generate.py --gh             # refresh phase counts from GitHub
python -I progress/generate.py --gh --history    # also append a dated point to history.json
```

- `data.json` is the last generated snapshot (phase counts, size points, per-game metrics, overall figures).
- `history.json` is one dated snapshot per `--history` run, deduplicated when a run adds nothing.
- `index.html` is static and self-contained: no CDN, no secrets, no client-side API calls.
- `.github/workflows/progress.yml` runs the `--gh --history` form daily and on demand, then publishes `progress/` to GitHub Pages.

## Honesty rules

Every number is a numerator and denominator. A missing source renders **not measured yet** and links to the follow-up issue that will fill it. The generator never infers a denominator and never fills a gap with zero.

## What the page shows now

The page leads with what is actually measured, so it is useful before the object exporters exist:

- **Project status** — pre-release, no release builds; the public alpha is planned for phase 7. The label lives in `PROJECT_STATUS` in `generate.py`, not in the data.
- **Phase progress** — closed/total issues and remaining size points per phase, with the phase's entry and exit condition behind *Gate*.
- **What is moving** — recently closed, in-progress, needs-user and unblocked agent-ready issues, from issue labels and timestamps.
- **Open work by area** — open issues per `area:*` label.
- **Issues over time** — a weekly cumulative burn-up of issues opened and closed, from `created_at`/`closed_at`.
- **Ports** — each port issue's state, its *Done when* checklist and sub-issue progress, and the recorded trace count from `docs/architecture/traces.md` (dated source shown).
- **Object coverage** — census/spec/script/tick-accuracy cells stay `not measured yet` until the exporters land; they are in a secondary table, not the headline.

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

`status` is the lifecycle (`converted`, `trace-checked`, `Beta`, `Ranked`, `Loved`) or `unknown`. `objects_total` is the census; `objects_specced` counts objects with a behavior spec; `objects_scripted` counts objects with a package script; `traces_passing` counts objects with a passing trace; `traces_total` counts objects trace-checked. `null` and `unknown` mean not measured yet. Filling them is tracked by issue #1367.

`null` and `unknown` mean not measured yet. Filling them is tracked by issue #1367.

## Exporter file

An optional `progress/exporter.json`, written by a documented command in the research checkout, overrides the front-matter. It is the only supported way to add numbers from the converter, census or trace runner.

```json
{
  "generated_at": "2026-11-24T00:00:00Z",
  "games": {
    "rely-the-needle-4-2": {
      "converted": true,
      "objects_total": 0,
      "objects_specced": 0,
      "objects_scripted": 0,
      "objects_in_package": 0,
      "traces_passing": 0,
      "traces_total": 0,
      "lifecycle": "converted"
    }
  }
}
```

Fields:

| Field | Meaning | Follow-up |
|---|---|---|
| `converted` | the converter produced a package that loads | #1366 |
| `objects_total` | objects in the game's census | #1364 |
| `objects_specced` | objects with a behavior spec | #1364 |
| `objects_scripted` | objects with a package script | #1364 |
| `objects_in_package` | objects the converted package represents (falls back to `objects_scripted`) | #1366 |
| `traces_passing` | objects with a passing trace | #1365 |
| `traces_total` | objects trace-checked (informational) | #1365 |
| `lifecycle` | `converted`, `trace-checked`, `Beta`, `Ranked`, `Loved` or `unknown` | #1366 |

The intended research-side command (to be implemented by the follow-ups) is:

```sh
python -I tools/progress_export.py --out ../iwbtg-collection/progress/exporter.json
```

No converter, census or trace data is committed to this public repo.

## Phase metrics

- Open and closed counts come from the main repo's phase milestones.
- **Remaining size points** sum the open issues' size labels: `size:S = 1`, `size:M = 3`, `size:L = 8`. An issue with no size label counts 0 and is shown in its own column.
- The page calls this a planning estimate, never a date. The roadmap has no deadlines.

## Publishing

The workflow restores the durable `history.json` from `gh-pages`, generates, then commits `index.html`, `data.json`, `history.json` and `.nojekyll` to `gh-pages` and requests a Pages build. Pages serves that branch at <https://nugamus.github.io/iwbtg-collection/>.

It deliberately does **not** write to `main`: the `main: required checks` ruleset requires `ci-ok` and `body`, and `github-actions[bot]` is not on its bypass list. `progress/history.json` on `main` is the seed for the first run; the durable history lives on `gh-pages`, so each daily run appends a point. A failed branch push or a failed build request fails the run visibly; there is no silent fallback.

If Pages is ever reset, point it at the branch once:

```sh
gh api -X PUT repos/nugamus/iwbtg-collection/pages \
  -f build_type=legacy -f 'source[branch]=gh-pages' -f 'source[path]=/'
```

The generated page is also readable straight from the repo at `progress/index.html`.
