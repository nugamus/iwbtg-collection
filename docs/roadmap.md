# Roadmap

The order of work across all three repos: client, research and games. There are no deadlines. A phase ends when its exit check passes, not on a date. The phase set is a set of gates: a later phase may run alongside an earlier one only where its entry condition already holds.

## Where to look

- **Progress: the GitHub Project "IWBTG Collection"** (github.com/users/nugamus/projects). It holds every issue from all three repos on one board.
- **Phases are milestones in `nugamus/iwbtg-collection` only.** They are named `N · Domain` (`0 · Foundation` to `10 · Release`) and every main-repo issue, open or closed, has exactly one. The Project's Milestone column follows the issue's milestone, so the board updates itself.
- **Research and games issues have no milestone.** Each carries a correct `area:*` label and one `Phase:` line naming the main-repo phase it feeds.
- **Every issue** is one piece of work. Each has a goal, its dependencies (links to other issues), a "Done when" checklist and a "User check": the exact command to run and what to look for.
- **Labels:**
  - `agent-ready`: an agent can start the issue.
  - `needs-user`: a decision, review or playtest that only the user can do.
  - `size:S/M/L`: S is one agent session and M a few. An L issue gets split before work starts.
  - `area:*`: which part of the project the issue touches.
  - `triage`: a new issue an agent filed, waiting to be sorted into a phase.
- **Sub-issues** break every large issue into steps, and `blocked by` links connect dependent issues, so the board shows what can start now.
- **Ideas** that are not in any phase yet are lines in `potential-features.md`; main-repo issues always carry a phase.

## Dispatch table

Hand an agent a phase and a lane. The open count is from the 2026-11-24 snapshot; the [progress dashboard](https://nugamus.github.io/iwbtg-collection/) rebuilds it from `gh`.

| Phase | `area:*` labels | Lane | Entry | Exit | Open |
|---|---|---|---|---|---|
| `0 · Foundation` | `area:infra`, `area:docs` | infra (`.github/`, `.claude/`, `scripts/`) | An empty repo, or a broken build/CI/harness. | Every Foundation issue is closed and `cargo xtask check --all` plus CI pass on Windows, Linux and wasm32. | 13 |
| `1 · Behavior research` | `area:research` | research | A question about original behavior that can change what a player feels, with no spec or trace answer. | Every open F1 behavior question has a trace or a spec answer, and the gm8 CLONE_SPEC covers the mechanics the 20 games use. | 4 |
| `2 · Format & pipeline` | `area:format`, `area:tools`, `area:port` | data + research (converter) + sim (script API) | A converted package, a format gap, or a porting step the converter cannot automate. | The census says how many scripts the 20 ports need; the script API covers what they use; Rhai is measured on native and WASM; port layering, the extension model and the standard ruleset/profile model are decided; one command checks every port's contracts; `docs/porting.md` is written. | 31 |
| `3 · Sim fidelity` | `area:sim` | sim | A spec or trace answer for a mechanic, and a package that loads. | `rely_the_needle` reproduces tick for tick on Windows, Linux and WASM; all kid families, built-ins, scripts, sound and the replay runner are in; all 20 converter packages load and trace-check. | 44 |
| `4 · First playable` | `area:client`, `area:format` | ui (client core) + sim | The sim loads `rely_the_needle` and produces ticks. | The Bevy client plays `rely_the_needle` on native and web, and the user confirms the feel. | 8 |
| `5 · Client shell & UI` | `area:client`, `area:design` | ui | First playable is confirmed on native and web. | osu!-style game select, downloads, 5 bundled games, the controls panel and settings are built from approved mockups. | 21 |
| `6 · Ranked-ready run` | `area:client`, `area:sim`, `area:design` | ui + sim + data | First playable, and the ranked rules decision for mods. | Stripped ranked mode works locally: timer and cutscene freeze, mods, retry keys, HUD, practice, results, local PBs, replays and `.iwbz` with maps. | 42 |
| `7 · Online` | `area:server`, `area:client` | server + ui | A ranked-ready run and a local replay can be produced. | Accounts, verified submissions, leaderboards, review queue, profiles, cloud saves and engine upgrades run on the home server. | 57 |
| `8 · Ghosts & races` | `area:client`, `area:server` | server + ui | Online leaderboards persist replays. | Ghosts of leaderboard runs and live race lobbies work. | 10 |
| `9 · Content & ports` | `area:port`, `area:editor`, `area:sim`, `area:format` | research (ports) + ui (editor) + games repo | The sim (phase 3) and the porting pipeline (phase 2) can run a port end to end. | All 20 games are in Beta, each with fidelity proof and a verified full clear; editor v1 works; community uploads validate and can be curated; a GMS runner profile reproduces traces of 3 GMS fangames. | 52 |
| `10 · Release` | `area:infra`, `area:docs`, `area:design`, `area:server` | infra + server + docs | The online stack and content are in place enough to ship. | Name, takedown and privacy pages, licenses, release builds and contributor docs are in place. | 24 |

## Phases

| # | Phase | Scope | Gate (done when) | Feeds from |
|---|---|---|---|---|
| 0 | Foundation | Cross-cutting repo, build, CI and agent-harness work that every phase depends on. | Every Foundation issue is closed and `cargo xtask check --all` plus CI pass on Windows, Linux and wasm32. | client, research and games repos; CI; agent harness |
| 1 | Behavior research | Trace, probe and specify what the original game does, so the sim has evidence. | Every open F1 behavior question has a trace or a spec answer, and the gm8 CLONE_SPEC covers the mechanics the 20 games use. | research specs and traces, real-runner probes, humans recording inputs |
| 2 | Format & pipeline | The standard package format, converter, census, script API and porting pipeline. | The census says how many scripts the 20 ports need; the script API covers what they use; Rhai is measured on native and WASM; port layering, the extension model and the standard ruleset/profile model are decided; one command checks every port's contracts; `docs/porting.md` is written. | converter, format crates, research census, porting docs |
| 3 | Sim fidelity | The deterministic simulation reproduces the original mechanics tick for tick. | `rely_the_needle` reproduces tick for tick on Windows, Linux and WASM; all kid families, built-ins, scripts, sound and the replay runner are in; all 20 converter packages load and trace-check. | research traces, format packages, sim crate |
| 4 | First playable | One game end to end in the Bevy client, native and web. | The Bevy client plays `rely_the_needle` on native and web, and the user confirms the feel. | sim, format, client crate, the first port |
| 5 | Client shell & UI | Game select, downloads, bundled games, settings, controls panel and screens. | osu!-style game select, downloads, 5 bundled games, the controls panel and settings are built from approved mockups. | client crate, design mockups, games repo catalog |
| 6 | Ranked-ready run | Stripped ranked mode: timer, replays, local boards, HUD, practice and results. | Stripped ranked mode works locally: timer and cutscene freeze, mods, retry keys, HUD, practice, results, local PBs, replays and `.iwbz` with maps. | sim, client, format, design mockups |
| 7 | Online | Accounts, submission, leaderboards, review queue, profiles and the home server. | Accounts, verified submissions, leaderboards, review queue, profiles, cloud saves and engine upgrades run on the home server. | server crate, client, format, infra |
| 8 | Ghosts & races | Ghost playback of leaderboard runs and live race lobbies. | Ghosts of leaderboard runs and live race lobbies work. | client, server, replay format |
| 9 | Content & ports | The 20 games reaching Beta/Ranked, plus the editor, community games and GMS. | All 20 games are in Beta, each with fidelity proof and a verified full clear; editor v1 works; community uploads validate and can be curated; a GMS runner profile reproduces traces of 3 GMS fangames. | games repo ports, editor, community uploads, research CLONE_SPECs |
| 10 | Release | Packaging, installers, docs, legal and the public build. | Name, takedown and privacy pages, licenses, release builds and contributor docs are in place. | client, server, docs, legal |

The order of phases 4, 5 and 6 versus 9: phase 9 ports need phase 3 and the phase 2 pipeline, not the client. Ports can run in parallel with phases 4–6. Community games need the editor and phase 7; the GMS runner needs phase 9's first ports. Editor v1 lives under phase 9 because that is the content work it unblocks; it runs alongside the later phases.

### From the old milestones

The old `M*` milestones were replaced on 2026-11-24. The full old→new table for every issue is the reorg snapshot's `mapping.csv`; this is the shape:

| Old milestone | New phase |
|---|---|
| M1 · Kid matches the original, M2 · Sim covers the 20 games | 3 · Sim fidelity |
| M2b · Porting pipeline | 2 · Format & pipeline |
| M3 · First playable | 4 · First playable |
| M4 · Ranked-ready run | 6 · Ranked-ready run |
| M5 · 20 games, Editor v1, M10 · Community games, M11 · GameMaker Studio games | 9 · Content & ports |
| M6 · Game select | 5 · Client shell & UI |
| M7 · Online | 7 · Online |
| M8 · Ghosts and races | 8 · Ghosts & races |
| M9 · Public release | 10 · Release |
| Cross-cutting harness, CI and dev-tooling work (was spread across M1–M5) | 0 · Foundation |

Spec, trace and probe work from M2 moved to phase 1; format and converter work moved to phase 2; only the sim itself stays in phase 3.

### Keeping phases balanced

Near phases fill up with small issues while later ones hold a few large placeholders (review of 2026-11-14, #774). Two rules keep the board honest:

- **A finding goes to the phase of the work it blocks,** not to the phase that is open now. A one-game fidelity question goes on that game's port issue (phase 9), a server idea to phase 7, and so on. Fidelity questions get a feel tier first (#784, [fidelity tiers](plans/fidelity-tiers.md)): only F1 (the T1 alias) blocks phase 3.
- **A phase is scoped before its predecessor closes.** Its large issues get sub-issues (each port follows the standard steps listed in its issue), so the board shows the real size of the work ahead.
- **The dashboard is not a phase.** How far the porting pipeline has got is measured on the [progress page](https://nugamus.github.io/iwbtg-collection/), never as a gate.

## Modernization program

The engine models that still reproduce GameMaker 8 implementation details, tracked by [#1180](https://github.com/nugamus/iwbtg-collection/issues/1180) now that [0005](decisions/0005-standard-mechanics.md) is accepted (2026-11-21). `gm8` keeps its exact behavior behind its compatibility profile and `standard` gets the modern model; an F1 slice needs a feel contract, an F2 slice a screenshot or audio check, and an F3 slice is free to redesign ([fidelity tiers](plans/fidelity-tiers.md)). The program is parked for phase 6: an implementation slice does not start while a phase 2 seat owns the same files, and every child is size L and splits into bounded sub-issues first. The umbrella holds the detail; this table places the work.

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

- **At session start,** the agent sees phase progress, the next unblocked `agent-ready` issues, needs-user issues the user has answered, and the triage queue.
- **When a turn that changed files ends,** the agent must report its findings: issues filed for anything it noticed but did not finish, or "none". New issues get `triage` until they are sorted into a phase.

1. By default an agent takes the first unblocked issue in the earliest open phase. Issues with open sub-issues or open `blocked by` links are skipped.
2. One issue gets one branch and one PR with `Closes #<n>`. The PR template asks for docs updated, follow-up issues and the User check, and a CI check makes sure they are filled in.
3. Questions for the user go in issue comments, and the issue gets `needs-user`. The user replies on GitHub, and the next session acts on the reply.
4. CI and the agent's own User check run gate the merge; the agent or lead merges. The user tests merged UI changes on `main` in batches and files what they find (decided 2026-11-05).

## Research pipeline rules

These carry over from the original phase plan.

- **Crawling.** Delicious-fruit is crawled gently: one request at a time, several seconds apart, every response cached, runs that resume where they stopped. Ratings are never imported.
- **Storage.** Downloads are stored by SHA-256 in `archive/` and backed up off the internet-facing server. They never go into Git or LFS, and originals never reach a public repo.
- **VirusTotal.** Every download is checked through a slow queue (public API limit: 4 requests/minute, 500/day). Flagged files go to manual review, because GM8 games often trigger false positives.
- **Running originals.** Originals run on the main machine only through the research gate, and only once they are VirusTotal-clean.
- **Crawl scope.** Only the candidate pool is crawled until all 20 ports pass. The full crawl is a phase 9 issue.

## Decided

- **Licenses.** Code is `MIT`. Docs, the format spec and the research specs are `CC-BY-4.0`. Game content belongs to its creators.
- **Games storage.** Ported games and their music live in the private games repo for now. When it grows too large, they move to the home server.
- **Accounts.** Accounts live in our own database, with each sign-in method behind one interface. The server stack is described in `docs/server.md`.
- **Research platform.** Research runs on native Windows. Originals run only when VirusTotal-clean, through the research gate (2026-10-24).
- **Player-facing design** is in `docs/product.md`. **Porting, assets, engines, the package format and the editor** are in `docs/content.md` (2026-11-02).
- **Script language.** Rhai stays, with a checkpoint after the first boss and avoidance ports ([0001](decisions/0001-script-language.md)). Data files stay TOML.
- **Copyrighted music** is kept and removed on request.
- **Tracking.** GitHub phase milestones in the client repo, issues across the three repos, and one Project board (2026-11-02; phase set 2026-11-24).

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
