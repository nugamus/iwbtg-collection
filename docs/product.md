# Product decisions

What the client is for and how playing, ranking and the UI should work. The user decided these on 2026-11-02. Engine and format details live in [architecture](architecture.md) and [format v0](format/v0.md). Where those pages disagree with this one about player-facing behavior, this page wins and the other page gets updated.

## Audience

Casual fangame players and speedrunners count equally, as in osu!: browse and clear games casually, with competitive ranking on top.

## Two ways to play each game

The game list offers two buttons per game:

- **Ranked (stripped).** The game without its frame: no title screen, menus, file select or difficulty select. The first input starts the timer, and finishing the last boss or stage ends the run. This is the only mode with public leaderboards.
- **Play original.** The full game as ported, menus and all. It records personal stats only (clears, deaths, times), with no public board.

What stripped mode cuts is decided per game (see [Per-game questions](#per-game-questions)). The defaults:

- Title, menus, file select and difficulty select are removed.
- Cutscenes and dialogue can always be skipped. While one plays, the timer freezes. Dialogue that waits for a button press advances when the player presses, and advances on its own after a timeout. That way nobody gets free rest time by sitting in a text box.
- Secrets and side items stay in the game. If they allow a trick or a faster route, players may use them. If not, chasing them only costs time.

## Mods

osu!-style mods are chosen in the game list before starting:

- **Difficulty.** Hard is the default with no mod. Medium, Very Hard and Impossible are mods.
- **Sudden Death.** One death ends the run.
- **Visual mods.** For example hitboxes or hiding the background.
- **Practice assists.** Save states, slow motion and frame advance. A run with these is never ranked.

Every mod combination has its own leaderboard, and the leaderboard filters by mods. A bottom-bar toggle turns on auto-retry, which restarts the run from the start after a death (as in osu!). It is meant for Sudden Death and deathless attempts.

## Leaderboards

- Time is the main ranking. Every row also shows the run's death count.
- A separate tab ranks clears by fewest deaths, with time as the tiebreak.
- Ranked runs use a random RNG seed per run, as the originals do. The server flags reused seeds.
- Anti-cheat: the server re-simulates every replay. Top runs and flagged runs also go through manual review, which can ask for video. Runs made offline are queued and submitted when the client is back online.
- Later: a performance-points system like osu!'s pp, which ranks players across all games. This is a release-era feature and its design is open.

## Restart keys

- `R` restarts from the last save, as in the originals.
- A separate quick-retry key resets the whole run immediately.

## Practice

Practice can start from any stage, any save point or any room, and it supports save states anywhere. Practice runs are never ranked.

## In-game display

The timer, splits, death counter and input display can each be turned on or off.

## Results screen

After a clear, the results screen shows:

- The time and the run's leaderboard rank.
- Per-stage splits compared with the PB and the sum of best.
- An osu!-style graph of deaths per stage.
- Buttons to save, watch and upload the replay.

## Look and screen

- The UI is a hybrid: a modern layout with pixel fonts and IWBTG motifs as accents.
- Games render at integer scale with letterboxing. GM8 games are 800x608, about 4:3.
- Fullscreen supports 4:3, 16:9, 16:10, 21:9 and 32:9. Windowed mode offers a set of predefined resolutions. The UI adapts to the chosen aspect ratio.
- Behind the game area, as in game select, the game's background shows blurred and fills the letterbox, like osu!.

## Distribution

- The client ships with 5 games bundled. Which 5 is open (pick from the first 20 ports).
- All other games appear in the game list and download on demand. Native builds store them in a folder on disk. The web build stores them in the browser cache.
- The client works fully offline once games are downloaded.
- Platforms: Windows and Linux first. Every platform eventually, which adds macOS (aarch64) to the determinism tests.

## Social

Version 1 includes:

- Public profiles with clears, PBs and deaths.
- Ghosts of leaderboard runs.
- Live races: players join a lobby and start together. Each racer plays in their own sim and sees the others as ghosts (positions only, no shared world).

## Permissions

Games whose creators cannot be reached (`community-assumed`) may be served publicly. They are credited clearly and removed on request through the takedown process.

## Community and release

Decided 2026-11-02.

- **Open source.** The client repo (code and docs) has been public since 2026-11-02 (GitHub Actions minutes for private repos ran out, so the user made it public so CI runs). The research and games repos stay private, because they hold notes on creators' games and the ported content.
- **Public alpha.** It starts at M7, even though the client repo is already public. Nightly builds are public from then on.
- **Community.** There is a Discord server for players, porters and testers.
- **Bug reports.** They arrive in two ways, and both become `triage` issues:
  - Players post them in a Discord bug channel, and the user or an agent copies them into issues.
  - An in-client "Report bug" button sends a description, the log and the replay to the server.
- **Crash reports** are opt-in. After a crash, the client asks whether to send the log and replay. There is no usage tracking.
- **Donations.** Decided later. The project itself is never monetized.
- **Replays.** Every leaderboard replay is public: anyone can download and watch it. Ghosts and races depend on this.
- **UI language.** English and Japanese at launch. All UI strings live in translation files from the start.
- **Identity.** The UI reuses art from ported games, credited, alongside the project's own identity: a logo and wordmark that come with the new name.
- **Cheating is not tolerated.**
  - Suspicious runs are flagged silently. The player is not told, so a moderator can investigate.
  - A ban follows only once cheating is confirmed. Players are never banned on suspicion alone.
  - Banned players lose their leaderboard access.
  - Players can always report runs and profiles.
  - Usernames are checked against a blocklist.

## Dreams for later

Ideas the user wants to work out over time. Nothing here is decided.

- **Website.** An osu!-style site with user pages, game pages and leaderboards.
- **Curation for ranking.** Not every game should get ranked boards. Games reach the leaderboard through curation, with criteria still to be worked out. Possible inputs: the quality of the port, clear count, player ratings, creator permission, how well the game fits a speedrun format. This works like osu!'s ranked and loved sections.
- **Community games.** Player-made games from the level editor, uploaded to the site, which become rankable only after curation.
- **Performance points.** A cross-game skill ranking (see [Leaderboards](#leaderboards)).
- **Player ratings.** 1-10 stars, only from players who cleared the game. Delicious-fruit ratings are never imported.
- **Name.** The project gets its own original name before the public alpha (M7); the repo is already public under the working title. "IWBTG Collection" is a working title.

## Per-game questions

Stripped mode, cutscene handling, which items count as secrets and similar choices differ per game. Agents write each open question for a game into `docs/games/<game-id>.md` (see [docs/games/README.md](games/README.md)). The user answers them there.
