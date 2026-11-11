# Porting, assets, engines and creation

How games get into the collection: porting, the shared asset library, engines, the package file and the level editor. The user decided these on 2026-11-02. Player-facing play and ranking are in [product](product.md). The file-level spec is [format v0](format/v0.md), and the items in [Format changes needed](#format-changes-needed) must reach it before format 1.

## Porting

- **Fidelity.** Quirks that players rely on are never fixed. Bugs that nobody wants (crashes, softlocks, broken graphics, typos) are fixed when a trace shows that positions do not change. Every fix is logged in `docs/games/<game-id>.md`.
- **Which version.** We port the best version of a game. When a game is submitted, its creator (or the porter) picks the version. Keeping the same version is preferred, so records stay historical. If a better version turns up, it replaces the old one, and the old board is frozen as a viewable legacy board.
- **Other runners.** GameMaker 8 comes first and GameMaker Studio later, each with its own CLONE_SPEC and runner rules.
- **Odd sizes.** A game keeps its native resolution and room speed. It is integer-scaled, and the letterbox adapts.
- **Who ports.** For now, the user and agents port as many games as they can. Porting opens to other people as the project matures. There are no votes: whoever wants a game ported ports it.
- **First-year goal.** 20 polished ports. Quality comes before count.

### Proving a port

Agents never play whole games, because that is slow and unreliable. Proof comes from two sources:

1. **Fidelity.** A human records inputs on the original, through the research gate's trace recorder. Short segments are enough. The same inputs are replayed in our port and the traces are diffed tick by tick. Agents write only short scripted inputs to test physics.
2. **Clearability.** A human records a full clear in our port, as a replay that the headless CLI verifies. During Beta, clears submitted by players count here too.

A human playtest also checks the feel.

### Lifecycle

Games move through the same stages as osu! beatmaps:

- **Beta.** A new port. It is playable and unranked, and player clears count as proof that it can be cleared.
- **Ranked.** Fidelity has been proven and curation has accepted the game. It has public leaderboards.
- **Loved.** Fun to play, but not suited to leaderboards.

If a physics bug is found in a Ranked port, the bug is fixed and the server re-runs every stored replay. Runs that still clear are kept.

### Paper trail

Each port keeps:

- **`docs/games/<game-id>.md`:** open questions, fixes made and known differences from the original.
- **Trace reports:** stored in the package's `tests/` folder.
- **A changelog per version:** shown on the game page.

**OPEN:** the user wants the porting pipeline to be efficient and well made, not slow. A deeper design pass on the pipeline is needed: converter automation, which steps agents own, and where humans are needed.

### Language

The original text always ships. An optional English language pack is machine-translated as well as possible. Only text needed to play the game is translated (dialogue, menus, quiz questions). Game names, titles and subtitles are never translated.

### Music

Commercial music in fangames (Mega Man, Touhou and so on) is kept and removed on request, like `community-assumed` games.

## Assets

- **Storage.** Each package keeps its own assets in its own folder, so a takedown removes exactly that folder. A global index, keyed by content hash, lists every asset across all packages. It powers the editor's library and finds duplicates. When a game is taken down, assets with the same hash in other games stay, because games really do share assets.
- **Origin credit.** When the same asset appears in several games, the game with the earliest release date is credited as its origin and the others are listed as "also in". People can correct this.
- **Reuse.** Creators of new games may use any asset from any ported game, and credit is added automatically. This follows fangame culture. Original creators can lock their game's assets so they cannot be reused (per-game opt-out).
- **Takedowns.** A takedown removes only that game. Assets reused in other games stay.
- **Library.** The editor's library can be browsed by source game and by type and tags. It has visual search (similar-looking assets, detected duplicates) and personal favorites and collections.
- **Tagging.** An agent tags assets while porting. The community can tag them later.
- **Skins.** Any game can get improved or HD art through skins, which the home server hosts. Kid skins are allowed, and creators can lock their game against skins. **OPEN:** the skin system as a whole (format, ranked rules, hosting).

## Engines

- **Engine packs.** Each pack matches a real fangame engine (for example YoYoYo, Seph or Kayin classic). A pack bundles the physics profile, kid sprites and sounds, default built-ins and menus. Creators pick a pack when starting a new game.
- **Custom engines.** Creators may change physics or build their own engine. Any game whose physics differ from its pack's original is marked **custom physics** on its game page. A custom engine can be published as a pack for others to use, with credit.
- **Medleys.** Each room may name its own physics profile. First check how the original medley actually does it. If it mixes several engines, mark it custom physics.
- **Upgrades: automatic when provably safe.** Decided by Claude on the user's request, 2026-11-02. The sim keeps every engine revision's behavior, selected by the package's `engine_rev`, so an old revision never stops working. When a new revision ships, the server tests each game on it:
  - Every stored replay is re-run, and ports also re-run their original traces.
  - If every replay gives the same tick count and final state hash, and every trace still matches, the game moves to the new revision automatically. Nothing a player could notice has changed.
  - If anything differs, the game stays on its old revision, and its porter or creator gets the list of differences.
  - They can then opt in. Runs that still clear on the new revision stay on the board, and the others move to a legacy board.

  This way an engine fix never silently changes how a game feels or invalidates a record.
- **Shared scripts.** A versioned script library (for example `lib:bullet_patterns@1`) holds boss patterns, menu templates and avoidance helpers. Patterns that repeat across games get promoted to built-ins.
- **Expanded scope.** New games may go beyond GM8 limits: other resolutions, tick rates and bigger sprites. Changing the original engine's physics makes the game a custom engine.

## Package file

- A game ships as one `.iwbz` file: a zip with its own extension, like osu!'s `.osz`. Double-clicking it imports the game.
- One `.iwbz` holds the whole game: every ported version, every difficulty and every category. The game appears only once in game select.
- **Maps.** As with osu! beatmap sets, selecting a game expands it into its maps: each built-in difficulty (Medium, Hard and so on) and each category (Any%, 100%, True Ending), and versions. Each map has its own end trigger, star difficulty rating and leaderboard. Difficulty is a map, not a mod (user, 2026-11-04; [product](product.md#mods)).
- **Data syntax.** TOML stays. It is readable, and agents and people can both edit it.
- **Script language: Rhai stays.** Decided by Claude on the user's request, 2026-11-02. See [0001](decisions/0001-script-language.md#revisited-2026-11-02).

## Level editor

- The editor is built into the client. Any game has an Edit button, and playtesting runs on the real sim.
- No-code tools come first, so a whole game can be made without code:
  - A room painter.
  - Visual trigger wiring ("when the kid touches X, do Y to group Z").
  - A timeline editor with the music waveform, for avoidance.
  - A boss builder: HP, phases and attacks from a pattern library.
- Scripting is for advanced work such as complex bosses. The editor supports it with templates and docs.
- Ported games open read-only and can be forked into a remix, with credit.
- Playtesting supports:
  - Starting from the cursor.
  - Editing while playing.
  - Physics and hitbox overlays.
  - A clear check: a game cannot be published until its creator records a full clear.

## Game list data

- **Tags.** A fixed core set (needle, avoidance, adventure, boss rush, gimmick, trap, puzzle, medley) plus free tags.
- **Difficulty.** Computed from play data: deaths and time per stage across players, like osu!'s star rating.
- **Game page:**
  - Creators and history (release date, version, port notes).
  - The trick list.
  - Previews of each stage.
  - Port status (Beta, Ranked or Loved, the fidelity proof and known differences).
- **Music preview.** The porter picks the track and start point with the picker tool.
- **Creators.** Original creators can claim their game with a verified account. Once claimed, they can edit the page text, set the asset-reuse opt-out, approve versions and request takedown.

## Saves, controls and play details

- **Saves.** Play original has the original's save slots, kept locally and synced to the player's account.
- **Audio offset.** A global audio offset with a calibration screen. It moves audio playback only, never the sim.
- **Controls.** Decided 2026-11-02. There is one global set of bindings, defaulting to the fangame standard (Shift to jump, Z to shoot, R to restart).
  - **Extra actions.** Some games need keys beyond the standard set, for example an item key or a menu key. The action set gains four generic actions, `extra1` to `extra4`, with global default keys. A game that uses one gives it a label in its package, for example "extra1: Use item". The input bits widen from `u16` to `u32` to make room.
  - **Controls panel.** The settings page has a game picker: type to search, or scroll the list. It shows the selected game's labeled actions, highlights anything unusual, and lets players rebind the global keys from there.
  - **Before play.** The client shows a notice when a game uses extra actions.
- **Stripped-mode visuals.** Blood, GAME OVER and screen shake stay. Only menus and cutscenes are cut.
- **Death.** It works as in the original: GAME OVER, then R. The timer keeps running.

## Format changes needed

Changes [format v0](format/v0.md) needs before format 1:

- **Maps.** Versions and categories inside one package, each with its own timing triggers. Done: `[[maps]]` ([v0](format/v0.md#maps)); a map starts the original's difficulty through `stripped_start`.
- **Stripped mode.** Done: `[stripped]` ([v0](format/v0.md#stripped-mode)) with the ranked start point, the cut list and cutscene and dialogue regions. The sim side is #34, #43 and #561.
- **Physics per room,** for medleys.
- **Language packs.** Text that can be translated is kept apart from the rest.
- **Unusual controls.** Fields for the control hints.
- **Creator locks.** Flags for asset reuse and skins.
- **Script library references** (`lib:<name>@<version>`).
- **The `.iwbz` container.** Done: [iwbz.md](format/iwbz.md) (#36); the OS file association comes with the release installers (#647).
