# Lane briefs

One page per agent-team lane: what the lane owns, how to build and test it fast, and the mistakes that cost time before. A teammate reads its lane's page right after the issue, instead of exploring the repo. The lead prompt (`TEAM_LEAD_PROMPT.md`, workspace root) assigns the lanes.

| Lane | Worktree | Page |
|---|---|---|
| ui | `worktrees/client-ui` | [ui.md](ui.md) |
| data | `worktrees/client-data` | [data.md](data.md) |
| sim | `worktrees/client-sim` | [sim.md](sim.md) |
| research | main `research/` checkout (one agent) | [research.md](research.md) |

Every lane: work in your worktree only, switch branches there per issue (`git switch -c issue-<n>-<name> origin/main`), run cargo in the foreground, test what you touched while working and the whole workspace once before the PR (client `CLAUDE.md`, Builds). Update your page in the same PR when you learn something that would have saved you time.
