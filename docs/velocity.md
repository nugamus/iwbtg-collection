# Velocity

Tokens and hours per merged PR, one line per week. Numbers only: no transcript content, prompts or file contents belong here.

Source: `python -I client/.claude/stats.py --summary --days 7` from the workspace root (it needs `gh` for the merged-PR count). The lead runs it once a week (lead routine) and appends one line to the table.

- **fresh** = input tokens plus cache writes, **out** = output tokens, **cache reads** are listed apart (cheap, but they grow with context). All in thousands per merged PR, summed over every lane and the lead.
- **hours** = wall time minus time spent waiting for a message, per merged PR.
- A merged PR is a merged client PR with an `issue-<n>-` branch. Research and games PRs are in other repos, so the research lane's spend shows up in the total but its own per-PR column is not meaningful.
- Counting rule: one message spans several transcript lines and is counted once (`--summary` only; the older `turns` column still counts lines).

| Week ending | Merged PRs | Fresh k/PR | Out k/PR | Cache reads M/PR | Hours/PR | Note |
|---|---|---|---|---|---|---|
| 2026-11-16 | 151 | 205 | 47 | 8.5 | 0.34 | Baseline, before the velocity issues (#788, #787, #784, #785, research#173, research#174). `ultracode` is on in the workspace settings. |

## Baseline by lane (week ending 2026-11-16)

```
lane       sessions  fresh M  cache M   out M  hours  PRs fresh k/PR  out k/PR   h/PR
data             68      6.6    220.8    1.40    9.5   50        132        28    0.2
games             1      0.1      7.0    0.04    0.4    0          -         -    0.0
lead             11      4.0    291.5    0.96   10.3   15        265        64    0.7
other             1      0.0      0.0    0.00    0.0    0          -         -    0.0
research         56      6.3    235.4    1.63    9.6    1       6336      1630    9.6
sim              68      7.1    265.1    1.60    9.4   53        133        30    0.2
ui               46      6.8    263.3    1.43   11.7   30        226        48    0.4

All lanes: 151 PRs merged in 7 days (149 matched to a lane).
Per merged PR: 205k fresh input, 47k output, 8.5M cache reads, 0.34 active hours.
```
