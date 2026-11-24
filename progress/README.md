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

## Per-game front-matter

Each file in `docs/games/<slug>.md` starts with:

```yaml
id: rely-the-needle-4-2
game_id: 322
name: "I Wanna Rely The Needle 4.2"
slug: rely-the-needle-4-2
source: https://delicious-fruit.com/game/19483
engine: gm8
status: unknown          # lifecycle or `unknown`
objects_total: null       # census
objects_specced: null     # objects with a behavior spec
objects_scripted: null    # objects with a package script
traces_passing: null      # objects with a passing trace
traces_total: null        # objects trace-checked (informational)
```

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

The workflow uses the `github-pages` environment and Actions deployment (`actions/deploy-pages`). If Pages is not enabled for the repository, enable it once:

```sh
gh api -X POST repos/nugamus/iwbtg-collection/pages -f build_type=workflow
```

The generated page is also readable straight from the repo at `progress/index.html`.
