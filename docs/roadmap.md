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
| M1 | Kid matches the original *(done)* | The sim reproduces rely_the_needle's traces (stand, double jump) tick for tick on Windows, Linux and WASM. |
| M2 | Sim covers the 20 games | All kid families, built-ins, scripts, sound and the replay runner are in. All 20 converter packages load and trace-check. Fidelity questions count only at F1 (the T1 alias) ([fidelity tiers](plans/fidelity-tiers.md)); F2 and F3 items move to the affected game's port issue or M5. |
| M2b | Porting pipeline | The census says how many scripts the 20 ports need; the script API covers what they use; Rhai is measured on native and WASM; port layering, clean-room authoring, the extension model and the standard ruleset/profile model are decided; one command checks the fidelity and feel contracts of every port; `docs/porting.md` is written (#23, [0005](decisions/0005-standard-mechanics.md)). |
| M3 | First playable | The Bevy client plays rely_the_needle on native and web, and the user confirms the feel. |
| M4 | Ranked-ready run | Stripped ranked mode works locally: timer and cutscene freeze, mods, retry keys, HUD, practice, results, local PBs, replays, `.iwbz` with maps. |
| M5 | 20 games | All 20 games are in Beta, each with fidelity proof and a verified full clear. |
| M6 | Game select | osu!-style game select, downloads, 5 bundled games, controls panel and settings, built from approved mockups. |
| M7 | Online | Accounts, verified submissions, leaderboards, review queue, profiles, cloud saves and engine upgrades run on the home server. |
| M8 | Ghosts and races | Ghosts of leaderboard runs and live race lobbies work. |
| M9 | Public release | Name, takedown and privacy pages, licenses, release builds, contributor docs. |
| — | Editor v1 | The in-client editor works. It starts once M4 is done and runs alongside the later milestones. |
| M10 | Community games | Players make a game in the editor and upload it; it is validated, credited, playable as Beta, and can be curated to Ranked or removed. Format 1 is frozen, and new mechanics need no client release. |
| M11 | GameMaker Studio games | A GameMaker Studio runner profile with its own CLONE_SPEC reproduces traces of 3 GMS fangames, and they are ported in Beta. |

The order of M3 and M4 versus M5: M5 ports need the M2 sim and the M2b pipeline, not the client. Ports can run in parallel with M3 and M4. M10 needs Editor v1 and M7; M11 needs M5.

### Keeping milestones balanced

Near milestones fill up with small issues while later ones hold a few large placeholders (review of 2026-11-14, #774). Two rules keep the board honest:

- **A finding goes to the milestone of the work it blocks,** not to the milestone that is open now. A one-game fidelity question goes on that game's port issue (M5), a server idea to M7, and so on. Fidelity questions get a feel tier first (#784, [fidelity tiers](plans/fidelity-tiers.md)): only F1 (the T1 alias) blocks M2.
- **A milestone is scoped before its predecessor closes.** Its large issues get sub-issues (each port follows the standard steps listed in its issue), so the board shows the real size of the work ahead.

## Modernization program

The engine models that still reproduce GameMaker 8 implementation details, tracked by [#1180](https://github.com/nugamus/iwbtg-collection/issues/1180) now that [0005](decisions/0005-standard-mechanics.md) is accepted (2026-11-21). `gm8` keeps its exact behavior behind its compatibility profile and `standard` gets the modern model; an F1 slice needs a feel contract, an F2 slice a screenshot or audio check, and an F3 slice is free to redesign ([fidelity tiers](plans/fidelity-tiers.md)). The program is parked for M4: an implementation slice does not start while an M2b seat owns the same files, and every child is size L and splits into bounded sub-issues first. The umbrella holds the detail; this table places the work.

| Workstream | Issue | Tier | `standard` gets | `gm8` keeps | Waits on |
|---|---|---|---|---|---|
| A. World transaction | [#1174](https://github.com/nugamus/iwbtg-collection/issues/1174) | F3 mechanism; F1 where spawn or event order is observed | Immediate, final destroy; generation handles; snapshot `with`; one room transaction and commit point | The old sequence behind its profile | #1149, #1152, #1153 |
| B. Entity and reference | [#1175](https://github.com/nugamus/iwbtg-collection/issues/1175) | F1 where a missing reference or pause changes an outcome; F3 otherwise | Explicit `enabled` and `keep_on_room_change`, pause as a state, typed references validated at build | Zero maps to the old sentinel | #1153, #1159 |
| C. Script semantics | [#1176](https://github.com/nugamus/iwbtg-collection/issues/1176) | F1 for ports relying on 0-defaults or save semantics; F3 (robustness) otherwise | Explicit initialization, typed values, data-only validated saves | Its semantics behind the profile | None for C1; C2 needs #1150 |
| D. Collision geometry | [#1177](https://github.com/nugamus/iwbtg-collection/issues/1177) | F1, adopted port by port with a feel contract | Baked typed colliders, one authoritative shape, analytic resolution | Exact sampling | None for D1; D2–D4 need #1151, #1152 |
| E. Presentation | [#1178](https://github.com/nugamus/iwbtg-collection/issues/1178) | F2; camera response can be F1 | Pure Draw projection, camera component, render layers, explicit transitions | Current behavior; a visible change needs the user's sign-off | None for E2–E5; E1 needs #1152 |
| F. Time service | [#1179](https://github.com/nugamus/iwbtg-collection/issues/1179) | F1 for bullet, boss and avoidance timing; F2 for ordinary animation | Dated timelines, path/tween components, one animation phase, its own ruleset value and `gameplay_hash` | Exact timeline moments, path clock and animation phase; hashes and pins unchanged | #1165 |

**Status (2026-11-21).** Slice state across the six workstreams after the wave that merged the C2 save hardening and the first two time-service slices:

- **#1174:** A1a, A2a and A2b merged; A1b in review (PR #1340, auto-merge armed); A3 waits on #1153.
- **#1175:** B5 (motion) merged; B1, B2 and B4 pending on #1153 and #1159.
- **#1176:** C1, #1106 and the C2 save hardening (#1325) merged; C2's remainder (#1202) waits on the save identity work in #1153; C3 and C4 pending.
- **#1177:** D1, D2a and D2b merged; D3 and D4 pending on #1151, #1160 and the dispatch lane's `collide/*`.
- **#1178:** E1a, E3, E2a and E4a merged; E1b, E2b, E4b, E5 and E6 parked, with E2b and E4b gated by #1160.
- **#1179:** F1 (#1165) and F2 (#1321) merged; F3, F4 and F5 wait on #1166 and the #1160 profile.

Also merged this wave: #1132 (web CI for #1078), #1330 (the physics schema slice of #1151), #1339 (the E3 identity evidence for #1153) and #459's sign-box F2, which stays `needs-user` for the modal default.

**Sequencing.** The status list names each pending slice's blocker; the child issue carries the detail. A slice that shares `room.rs`, `world/*`, `events.rs`, `def/physics.rs` or `collide/*` starts only when the lane that owns those files is done. Format 1 (#1160) still gates T3, F3–F5, E2b/E4b/E5, #1224 and #1195, and fidelity cannot run until `../iwbtg-collection-research/build/packages` exists.

## How work flows

The rules are in each repo's `CLAUDE.md`, under Issues. Two hooks in `.claude/settings.json` enforce them:

- **At session start,** the agent sees milestone progress, the next unblocked `agent-ready` issues, needs-user issues the user has answered, and the triage queue.
- **When a turn that changed files ends,** the agent must report its findings: issues filed for anything it noticed but did not finish, or "none". New issues get `triage` until they are sorted into a milestone.

1. By default an agent takes the first unblocked issue in the earliest open milestone. Issues with open sub-issues or open `blocked by` links are skipped.
2. One issue gets one branch and one PR with `Closes #<n>`. The PR template asks for docs updated, follow-up issues and the User check, and a CI check makes sure they are filled in.
3. Questions for the user go in issue comments, and the issue gets `needs-user`. The user replies on GitHub, and the next session acts on the reply.
4. CI and the agent's own User check run gate the merge; the agent or lead merges. The user tests merged UI changes on `main` in batches and files what they find (decided 2026-11-05).

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
