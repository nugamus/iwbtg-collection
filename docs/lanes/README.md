# Lane briefs

One page per agent-team lane: what the lane owns, how to build and test it fast, and the mistakes that cost time before. A teammate reads its lane's page right after the issue, instead of exploring the repo. The lead prompt (`TEAM_LEAD_PROMPT.md`, workspace root) assigns the lanes.

| Lane | Worktree | Page |
|---|---|---|
| ui | `worktrees/client-ui` | [ui.md](ui.md) |
| data | `worktrees/client-data` | [data.md](data.md) |
| sim | `worktrees/client-sim` | [sim.md](sim.md) |
| research | main `research/` checkout (one agent) | [research.md](research.md) |

Every lane: launch the client only with `--mute` (or with `IWBTG_MUTE=1` set; a `--screenshot` run is silent anyway), so test runs never play sound on the user's speakers (#712). Work in your worktree only, switch branches there per issue (`git switch -c issue-<n>-<name> origin/main`), run cargo in the foreground, test what you touched while working and `cargo xtask check` once before the PR; `cargo xtask pr-body` drafts the PR body (client `CLAUDE.md`, Builds). Update your page in the same PR when you learn something that would have saved you time.

## Pitfalls for every lane

- In PowerShell, never read or write files with .NET calls (`[IO.File]::...`) and relative paths: .NET resolves them against the process's start folder (the main checkout), not the `cd` location; on 2026-11-12 that wrote a teammate's edits into the main client checkout. Use the Edit tool with absolute worktree paths.
