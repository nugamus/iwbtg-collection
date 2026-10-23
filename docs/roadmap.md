# Roadmap

Order of work across all three repos (client, games, research). No deadline: each phase ends when its check passes, not on a date.

## 0. Setup (done)

Three private repos, lean CLAUDE.md files, clean-room spec skill and template, Potential Features.md, licenses, workspace settings with the archive guard hook.

## 1. Catalog and archive (research repo)

- Scraper (Python + Playwright) collects delicious-fruit metadata: name, creators, year, difficulty, tags, clear count, page URL, download links. Ratings are never imported.
- Gentle crawling: one request at a time, several seconds between requests, every response cached on disk, runs resume where they stopped.
- SQLite database tracks every game and every download link. Link status: `pending`, `downloaded`, `verified` (SHA-256 recorded), `dead`, `alt_found`.
- Downloader with one handler per host (direct, Mediafire, Google Drive, Mega, Dropbox, others as found). Files are stored by SHA-256 in `archive/` and backed up to storage that is not the internet-facing server.
- VirusTotal check for every downloaded file (usually the zip or rar): look up the SHA-256 first, upload only if VirusTotal has never seen it. VirusTotal scans the files inside an uploaded archive. Files unpacked from it are checked one by one only when VirusTotal could not look inside (password-protected or oversized archives). The API key lives in `research/.env` as `VT_API_KEY`. Public API limit is 4 requests/minute and 500/day, so checks run in a slow queue. The database stores detection count, scan date and report link. Flagged files go to manual review, because GameMaker 8 games often trigger false positives. Results decide how a file may be handled in research. They never affect the client, which never runs original code.
- Original game files are never run on the main machine. Static extraction reads them without executing them. Recording ground-truth traces runs the original inside Windows Sandbox (built into Windows 11 Pro) or another disposable VM, started by the user. Work happens on native Windows, not WSL2, so games run without Wine.
- Binary storage: not Git or Git LFS. GitHub LFS quotas are too small for thousands of games, and originals must never reach a public repo. The database records hash and storage location.
- Community trick and technique notes collected into `knowledge/`, each with its source.
- Scope first: no full crawl or full archive until the 20 games are ported and work (end of Phase 5). Phase 1 catalogs only a candidate pool: the cwpat.me beginner list plus targeted lookups of single games.
- Check: user approves a list of 20 popular, short, easy games picked from the candidate pool.

## 2. Clean-room pipeline (research repo)

- Group the 20 games by engine. Start with the engine most of them share.
- Extract with research tools (gm8decompiler for GameMaker 8, UndertaleModTool for GameMaker Studio). Use REA for runner-level behavior the extraction cannot show (collision rules, rounding, tick order).
- Ground-truth traces: record the original's player state (x, y, speeds, flags) every tick for scripted inputs. The recording method is chosen in this phase and documented in the first engine spec.
- Write the engine CLONE_SPEC.md with the `clean-room-spec` skill.
- Mechanic inventory (`specs/mechanics.md`): every mechanic seen across the 20 games, each marked as a built-in candidate or as needing a script. This shapes the format.
- Check: the spec covers movement, jumping, double jump, saves, death and restart, with UNKNOWNs listed.

## 3. Format v0 and simulation core (client repo)

- `docs/format/`: game package spec, following the principles in `docs/format/README.md` (readable text, built-in objects first, sandboxed scripts, never compiled). A package holds metadata, physics profile, rooms, objects, assets, and the start, end and stage triggers that define timing.
- Choose the script language and data syntax by comparing candidates on sandbox guarantees, determinism, WASM support and readability. Candidates: an embedded Rust scripting language such as Rhai, Luau, or a small language of our own. The user approves the choice.
- `crates/sim`: fixed tick, physics profile from the engine spec, collision, triggers, replay as one input state per tick.
- `crates/format`: load and validate packages. Each package manifest records creators, source URL, permission status (`granted`, `community-assumed`, `pending`, `revoked`) and every third-party asset.
- Workspace `Cargo.toml` with `license = "MIT OR Apache-2.0"`, `cargo-deny` license allow list (no GPL in the client), and a `cargo fmt` hook once Rust code exists.
- Headless replay CLI: runs a replay, prints tick count and splits.
- Check: replaying the Phase 2 inputs reproduces the original traces exactly, tick for tick, on Windows, WASM and Linux x86_64 (the server's platform).

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
- Once all 20 pass: run the full delicious-fruit and wiki crawl (`research/scraper`, ~26 h) and archive the rest of the catalog.

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
- Accounts, sign-in and security follow `docs/server.md`.
- Hosted on the home server; encrypted restic backups to remote storage.

## 9. Going public

- Takedown page and contact at the bottom of the website. Privacy page and account deletion.
- Third-party content (commercial music, Kayin's sprites, engine assets) cleared or excluded before any public distribution, from the repo or the server. Decide whether `community-assumed` games may be served.
- `REUSE.toml` (put the `**` code annotation first so later game/docs entries win), `NOTICE`, and `THIRD_PARTY_LICENSES` generated with `cargo-about` for each release.
- Non-affiliation note for the IWBTG name; consider asking Kayin.
- One removal list honored everywhere: builds skip removed games, the server stops serving them, clients delete them on sync.
- CI builds: minimal (1 game) and full (all ported games).
- Contributor docs.

## Later

Bosses and avoidance, long games, level editor, items from Potential Features.md.

## Decided

- Code: `MIT OR Apache-2.0`. Docs, format spec and research specs: `CC-BY-4.0`. Game content belongs to its creators and is not relicensed.
- Ported games and their music live in the private games repo for now. When it grows too large, they move to the home server.
- Takedown contact: a link at the bottom of the website. Set up before going public.
- Accounts are stored in our own database; the sign-in method sits behind one interface. Reference flow: https://typebeat.sh/register.
- Server stack: `docs/server.md` (Rust + axum, SQLite, Cloudflare Tunnel, Docker Compose, restic backups).
- Research runs on native Windows. Original games run only in Windows Sandbox or a VM.

## Open decisions

- Script language and data file syntax for the game format. Decided in Phase 3.
- Backup storage provider (Backblaze B2 or Hetzner Storage Box). Needed by Phase 8.
