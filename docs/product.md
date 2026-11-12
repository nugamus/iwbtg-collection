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

osu!-style mods are chosen in the game list before starting. The full mod list is still open.

| Code | Mod | Kind | Ranked | What it does |
|---|---|---|---|---|
| SD | Sudden Death | Challenge | yes | One death ends the run. |
| HB | Hitboxes | Visual | yes | Draw the kid's and every hazard's hitbox. |
| NB | No background | Visual | yes | Plain backdrop behind the room. |
| SS | Save states | Practice | never | Save and load anywhere. |
| SL | Slow motion | Practice | never | Play at a lower speed (75% or 50%). |
| FA | Frame advance | Practice | never | Step one tick at a time. |

A run with a practice assist is never ranked; the loading card names the mod as the reason. The table lives in code (`iwbtg_sim::mods::MODS`), and the replay header stores the run's mods ([replay format](architecture/replay.md#mods)). The mod select mockup also shows Mirror and Flashlight; they are not in the list yet, and whether Hitboxes is ranked is open (#554).

Difficulty is not a mod (decided 2026-11-04). A fangame is the parent entry in game select, and each built-in difficulty and each category (Any%, 100%, ...) is a child entry, a map in [content](content.md) terms, with its own leaderboard, like beatmap difficulties under a beatmap set. Every mod combination has its own leaderboard, and the leaderboard filters by mods. A bottom-bar toggle turns on auto-retry, which restarts the run from the start after a death (as in osu!). It is meant for Sudden Death and deathless attempts.

## Leaderboards

- Time is the main ranking. Every row also shows the run's death count.
- Runs are measured in ticks at the game's fixed rate (for example 50 per second). Boards store and sort by tick count; the displayed time is derived from it and shown to hundredths. Ties go to the earlier submission. On the live in-run leaderboard you pass someone only when your tick count exceeds theirs. (2026-11-04)
- **Global** scope holds the top 100 personal bests, one per player. **Local** scope holds your own top 100 runs. Game select, the in-run leaderboard and results share both scopes. Avatars for the top 100 are downloaded and cached with a refresh policy, never fetched because a row scrolled into view. (2026-11-04)
- Offline, boards use the cached top 100 with an "Offline · cached" label. Finished runs are saved locally as pending uploads and submitted when back online; results then show placement as provisional.
- A separate tab ranks clears by fewest deaths, with time as the tiebreak.
- Ranked runs use a random RNG seed per run, as the originals do. The server flags reused seeds.
- Anti-cheat: the server re-simulates every replay. Top runs and flagged runs also go through manual review, which can ask for video. Runs made offline are queued and submitted when the client is back online.
- Later: a performance-points system like osu!'s pp, which ranks players across all games. This is a release-era feature and its design is open.

## Restart keys

- `R` restarts from the last save, as in the originals.
- A separate quick-retry key resets the whole run immediately: the key left of `1` (`` ` `` on a US layout), as in osu!. It is a client key, not a game action, so games and replays never see it; a retry is a new run. It is rebindable with the other bindings (key and controller input, `quick-retry` in the bindings table of the settings file or `--bind quick-retry=...`).
- Auto-retry restarts the run from the start right after a death. It is the `[retry] auto` setting; the `--auto-retry` flag overrides it for one run. The bottom-bar toggle is #558.

## Practice

Practice can start from any stage, any save point or any room, and it supports save states anywhere. Practice runs are never ranked.

## Game select and loading

Modelled on osu!lazer song select (decided 2026-11-04). The list of games is on the right; selecting a game expands its difficulty and category entries. The left shows the selected entry's header (title, creator, year, stage count, average clear time, global clear rate, your PB and deaths, tags, status, star difficulty rating) and Details / Ranking tabs. Each game panel in the list carries the game's own background art. Search, group and sort sit on top. The footer has Back and Options on the left, Random and Mods on the right, and the player card in the bottom-right corner (avatar, name, overall rank, games cleared, deaths, play time, a collection bar). Mods open as a sheet over game select, grouped by kind, and say which leaderboard the run will count for. A loading card (title, entry, creator, stars, your PB, the global #1, active mods, and an Unranked pill with the reason when it applies) shows for at least 1.5 s and starts the run by itself; Esc goes back.

## In-game display

Three columns (decided 2026-11-04):

- **Left: live leaderboard.** About 7 rows. You start at #1 at 0:00 and fall as your time passes other entries' final times; your row shows your elapsed time and stays centred. When you clear a stage, your row shows your split against the player directly above you for 3 s (the pace flash); at #1 it compares with the run directly below, since no one is above (2026-11-11, #494, revisit). Below the top 100 your rank keeps counting and the list shows the runs directly above you.
- **Centre: the game.** Never stretched. At 16:9 it fills the window height; at 4:3 it shrinks so both panels keep their room; on ultrawide the panels stop at a maximum width and the rest is background art.
- **Right: run info.** Timer, total deaths, stage of total, deaths this stage, an input display, and a row of active mod icons. An unranked run shows an Unranked pill for the whole run; a plain ranked run shows nothing there.

Splits, PB comparison and death graphs are not on the HUD; they belong on the results screen. Every HUD element can be turned on or off, Shift+Tab hides the whole HUD, and a UI scale setting (75–150%) scales the side panels only. Small windows collapse the leaderboard to rank and avatar first, then stack the run info and hide the input display.

Layout at each shape (artboards `Hud169`, `Hud43`, `Hud219`, `HudSmall`): at 16:9 the game fills the window height (1080p: 1.78x, panels about 249 px each side). When filling the height would leave the panels less than their room (204 px left, 180 px right; 4:3, 16:10), the game shrinks instead (1024x768: 0.8x). At 21:9 and wider each panel stops at 360 px and the rest is background art. Windows of 960x540 or less give each panel 124 px and use the collapsed panels, so the game stays as big as it can. Hiding the HUD with Shift+Tab gives the game the whole window. An "Integer scale" display option (off by default) uses the largest whole scale that fits instead; otherwise the game is scaled to fill its room with sharp (nearest) pixels. The UI scale never changes the game's size; on a narrow panel it grows only as far as the panel allows.

## Pause

The pause menu is the game's own PAUSE box (decided 2026-11-04): black box, double white border, pixel type, the delicious fruit as the selector, drawn in game pixels over the frozen, darkened game. Options: Continue, Retry from save (what R does), Restart run (the quick-retry key), Quit. Escape opens and closes it. The ranked timer keeps running while paused: paused time counts as run time, and the HUD says `Paused · timer running`. The replay records each pause (when and how long), and the run's time on the stats and leaderboards includes it, as the HUD showed it. Sound holds while paused. Quit discards the run: no results screen and no clear. The selector is the package's own `cherry` sprite when it has one, else our placeholder. The game's own P-key pause is separate: it is game input, part of the run and its replay.

## Music sync

The tick clock is the authority for music. When music playback drifts more than 2 ticks (40 ms at 50 Hz) from where the tick clock says it should be, the client seeks it back, so avoidance-game bullets stay on the beat after lag spikes or on slow PCs. Smaller drift is left alone, so normal play has no audible skips. Pausing and the death and restart rules seek exactly. Replays and ghosts count ticks only, so music sync never affects timing.

## Results screen

Only cleared runs get a results screen; quit or abandoned runs do not. One page (decided 2026-11-04):

- The final time as the biggest element, with the leaderboard placement next to it. Unranked runs show an Unranked pill instead of a placement.
- Rank change: placement before and after, PB before and after with the delta ("First clear" when there was no PB), and account totals.
- Rivals: the in-run leaderboard frozen at your final position, with #1 pinned.
- The player card, expanded: overall rank and its trend, collection progress, and what this run changed (deaths, play time, entries cleared).
- Splits (segment, split, delta against the PB, gold for a best-ever segment), sum of best and possible time save; deaths per stage; a death timeline. A per-stage death heatmap is optional.
- Back, Retry (hold to retry), Watch replay, Save replay, Upload replay.

Ahead and behind always use + and − signs as well as colour; gold best segments also get a marker.

## Look and screen

The approved screen designs (game select, mod select, loading, HUD, pause, results, player card) are in [design/mockups/m4](design/mockups/m4/README.md). Build from them.

- Modernise IWBTG without losing its identity: structure, motion and behaviour from osu!lazer and modern rhythm games, look from IWBTG and real fangame art. No pixel font for UI text and no bordered box around every element; pixel type stays in game-space parts such as the pause box. Shared language: 0.2 horizontal shear on rows, panels and pills with upright text, 10 px corner radius, OutQuint easing for most transitions. (2026-11-04)
- Games keep their aspect ratio and are never stretched; they fill the room the HUD leaves them, or use integer scale when the player turns that option on (In-game display). GM8 games are 800x608, about 4:3.
- Fullscreen supports 4:3, 16:9, 16:10, 21:9 and 32:9. Windowed mode offers a set of predefined resolutions. The UI adapts to the chosen aspect ratio.
- One background system serves every screen (select, loading, gameplay, pause, results): the selected game's art, a user blur slider, and a dim that keeps text readable. It fills the space beside the game, so there are no black bars.

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
