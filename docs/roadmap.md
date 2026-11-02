# Roadmap

The order of work across all three repos: client, research and games. There are no deadlines. A milestone ends when its "done when" check passes, not on a date.

## Where to look

- **Progress: the GitHub Project "IWBTG Collection"** (github.com/users/nugamus/projects). It holds every issue from all three repos on one board.
- **Each milestone** exists in every repo that has work for it, under the same title. Its progress bar shows how much of it is closed.
- **Every issue** is one piece of work. Each has a goal, its dependencies (links to other issues), a "Done when" checklist and a "User check": the exact command to run and what to look for.
- **Labels:**
  - `agent-ready`: an agent can start the issue.
  - `needs-user`: a decision, review or playtest that only the user can do.
  - `size:S/M/L`: S is one agent session and M a few. An L issue gets split before work starts.
  - `area:*`: which part of the project the issue touches.
  - `triage`: a new issue an agent filed, waiting to be sorted into a milestone.
- **Sub-issues** break every large issue into steps, and `blocked by` links connect dependent issues, so the board shows what can start now.
- **Ideas** that are not in any milestone yet are issues without a milestone (the backlog) or lines in `Potential Features.md`.

## Milestones

| # | Milestone | Done when |
|---|---|---|
| 0 | Setup and research groundwork *(done)* | Repos, specs pipeline, scraper, 20-game shortlist, GM8 CLONE_SPEC, first traces, converter, sim contracts (WP0). |
| M1 | Kid matches the original | The sim reproduces rely_the_needle's traces (stand, double jump) tick for tick on Windows, Linux and WASM. |
| M2 | Sim covers the 20 games | All kid families, built-ins, scripts, sound and the replay runner are in. All 20 converter packages load and trace-check. The porting pipeline design is approved. |
| M3 | First playable | The Bevy client plays rely_the_needle on native and web, and the user confirms the feel. |
| M4 | Ranked-ready run | Stripped ranked mode works locally: timer and cutscene freeze, mods, retry keys, HUD, practice, results, local PBs, replays, `.iwbz` with maps. |
| M5 | 20 games | All 20 games are in Beta, each with fidelity proof and a verified full clear. |
| M6 | Game select | osu!-style game select, downloads, 5 bundled games, controls panel and settings, built from approved mockups. |
| M7 | Online | Accounts, verified submissions, leaderboards, review queue, profiles, cloud saves and engine upgrades run on the home server. |
| M8 | Ghosts and races | Ghosts of leaderboard runs and live race lobbies work. |
| M9 | Public release | Name, takedown and privacy pages, licenses, release builds, contributor docs. |
| — | Editor v1 | The in-client editor works. It starts once M4 is done and runs alongside the later milestones. |

The order of M3 and M4 versus M5: M5 ports need the M2 sim and the porting pipeline, not the client. Ports can run in parallel with M3 and M4.

## How work flows

The rules are in each repo's `CLAUDE.md`, under Issues. Two hooks in `.claude/settings.json` enforce them:

- **At session start,** the agent sees milestone progress, the next unblocked `agent-ready` issues, needs-user issues the user has answered, and the triage queue.
- **When a turn that changed files ends,** the agent must report its findings: issues filed for anything it noticed but did not finish, or "none". New issues get `triage` until they are sorted into a milestone.

1. By default an agent takes the first unblocked issue in the earliest open milestone. Issues with open sub-issues or open `blocked by` links are skipped.
2. One issue gets one branch and one PR with `Closes #<n>`. The PR template asks for docs updated, follow-up issues and the User check, and a CI check makes sure they are filled in.
3. Questions for the user go in issue comments, and the issue gets `needs-user`. The user replies on GitHub, and the next session acts on the reply.
4. The user runs the User check, then merges.

## Research pipeline rules

These carry over from the original phase plan.

- **Crawling.** Delicious-fruit is crawled gently: one request at a time, several seconds apart, every response cached, runs that resume where they stopped. Ratings are never imported.
- **Storage.** Downloads are stored by SHA-256 in `archive/` and backed up off the internet-facing server. They never go into Git or LFS, and originals never reach a public repo.
- **VirusTotal.** Every download is checked through a slow queue (public API limit: 4 requests/minute, 500/day). Flagged files go to manual review, because GM8 games often trigger false positives.
- **Running originals.** Originals run on the main machine only through the research gate, and only once they are VirusTotal-clean.
- **Crawl scope.** Only the candidate pool is crawled until all 20 ports pass. The full crawl is an M5 issue.

## Decided

- **Licenses.** Code is `MIT OR Apache-2.0`. Docs, the format spec and the research specs are `CC-BY-4.0`. Game content belongs to its creators.
- **Games storage.** Ported games and their music live in the private games repo for now. When it grows too large, they move to the home server.
- **Accounts.** Accounts live in our own database, with each sign-in method behind one interface. The server stack is described in `docs/server.md`.
- **Research platform.** Research runs on native Windows. Originals run only when VirusTotal-clean, through the research gate (2026-10-24).
- **Player-facing design** is in `docs/product.md`. **Porting, assets, engines, the package format and the editor** are in `docs/content.md` (2026-11-02).
- **Script language.** Rhai stays, with a checkpoint after the first boss and avoidance ports ([0001](decisions/0001-script-language.md)). Data files stay TOML.
- **Copyrighted music** is kept and removed on request.
- **Tracking.** GitHub milestones and issues across the three repos, with one Project board (2026-11-02).

## Open decisions

Each open decision is a `needs-user` issue:

- Review the 20-game shortlist.
- The porting pipeline design.
- The 5 bundled games.
- The project name.
- Backup provider.
- Skins.
- Curation criteria.
- pp.
