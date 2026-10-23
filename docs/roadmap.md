# Roadmap

Order of work across both repos. No deadline: each phase ends when its check passes, not on a date.

## 0. Setup (done)

Two private repos, lean CLAUDE.md files, clean-room spec skill and template, Potential Features.md.

## 1. Catalog and archive (research repo)

- Scraper (Python + Playwright) collects delicious-fruit metadata: name, creators, year, difficulty, tags, clear count, page URL, download links. Ratings are never imported.
- Gentle crawling: one request at a time, several seconds between requests, every response cached on disk, runs resume where they stopped.
- SQLite database tracks every game and every download link. Link status: `pending`, `downloaded`, `verified` (SHA-256 recorded), `dead`, `alt_found`.
- Downloader with one handler per host (direct, Mediafire, Google Drive, Mega, Dropbox, others as found). Files are stored by SHA-256 in `archive/` and mirrored to the home server.
- VirusTotal check for every archived file and every file unpacked from it: look up the SHA-256 first, upload only if VirusTotal has never seen it. Public API limit is 4 requests/minute and 500/day, so checks run in a slow queue. The database stores detection count, scan date and report link. Flagged files go to manual review, because GameMaker 8 games often trigger false positives. Results decide how a file may be handled in research. They never affect the client, which never runs original code.
- Original game files are never run on the main machine. Static extraction reads them without executing them. Recording ground-truth traces runs the original inside a disposable VM only.
- Binary storage: not Git or Git LFS. GitHub LFS quotas are too small for thousands of games, and originals must never reach a public repo. The database records hash and storage location.
- Community trick and technique notes collected into `knowledge/`, each with its source.
- Check: user approves a list of 20 popular, short, easy games picked from the catalog.

## 2. Clean-room pipeline (research repo)

- Group the 20 games by engine. Start with the engine most of them share.
- Extract with research tools (gm8decompiler for GameMaker 8, UndertaleModTool for GameMaker Studio). Use REA for runner-level behavior the extraction cannot show (collision rules, rounding, tick order).
- Ground-truth traces: record the original's player state (x, y, speeds, flags) every tick for scripted inputs. The recording method is chosen in this phase and documented in the first engine spec.
- Write the engine CLONE_SPEC.md with the `clean-room-spec` skill.
- Check: the spec covers movement, jumping, double jump, saves, death and restart, with UNKNOWNs listed.

## 3. Format v0 and simulation core (client repo)

- `docs/format/`: game package spec. A package holds metadata, physics profile, rooms, objects, assets, and the start, end and stage triggers that define timing.
- `crates/sim`: fixed tick, physics profile from the engine spec, collision, triggers, replay as one input state per tick.
- `crates/format`: load and validate packages.
- Headless replay CLI: runs a replay, prints tick count and splits.
- Check: replaying the Phase 2 inputs reproduces the original traces exactly, tick for tick, on native and WASM.

## 4. First playable (client repo)

- Bevy client plays one small test package.
- Physics at fixed tick; rendering interpolated. Frame cap setting: 60, unlimited, or custom slider.
- Client actions, rebinding, controller support. Popup if a game menu tries to rebind.
- Native and web builds.
- Check: user plays it on both builds and confirms feel and known tricks against the original.

## 5. First ported game, then the rest of the 20

- One game end to end: game spec (engine spec plus differences), conversion into our format, fixes.
- Ranked only after a full clear in our port.
- Check per game: full clear recorded as a replay that the headless CLI verifies.

## 6. Game select screen

- Design pass first: mockups for user approval before code, built from art in the ported games.
- osu!-style layout: selected game's background full screen, game list on the right, leaderboard on the left, filters top right, middle kept open.
- Sort and filter by difficulty, our rating, tags, creator, year, clear count.
- Music preview with a picker tool for track and start point (first piece of the editor).
- Every game listed; double-click downloads it from the home server.

## 7. Results and local stats

- Per-stage splits, local best per stage, performance graph against best run.
- Toggle: compare against PB run or sum of best.

## 8. Leaderboard server

- Full-game runs with per-stage breakdown, submitted as replays.
- Server re-simulates with `sim` and stores the verified tick count.
- Anomaly flags and input-copy detection send runs to a review queue; nothing is auto-rejected on suspicion alone.
- Players are stored behind an ID so an account system can plug in later.
- Hosted on the home server, backups pushed to a remote Git server.

## 9. Going public

- Code license chosen. Published takedown process and contact.
- One removal list honored everywhere: builds skip removed games, the server stops serving them, clients delete them on sync.
- CI builds: minimal (1 game) and full (all ported games).
- Contributor docs.

## Later

Bosses and avoidance, long games, level editor, items from Potential Features.md.

## Decided

- Ported games and their music live in this repo (`games/`) for now. When the repo grows too large, they move to the home server.
- Takedown contact: a link at the bottom of the website. Set up before going public.
- Accounts are stored in our own database. The sign-in method will change over time, so it sits behind one interface. Reference for the flow: https://typebeat.sh/register.

## Open decisions

- Code license.
- Server stack and database.
