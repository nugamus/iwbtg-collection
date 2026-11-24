# Manifest: `game.toml`

Part of [format v0](v0.md).

```toml
#:schema ../../client/docs/format/schema/v0/game.schema.json
format = 0                       # format major version (integer)
id = "rely_the_needle"
title = "I Wanna Rely The Needle"
version = "1.0.0"                # version of our port (semver)
source_version = "4.2"           # version of the original we ported
engine_pack = "yuuutu"           # engine pack in games/_engines/, optional
engine_rev = 0                   # built-in behavior revision this package was verified against
ruleset = "gm8@0"                # optional; absent means gm8 at engine_rev (format 0, decision 0005)
runner = "gm80"                  # gm80 | gm81: the original's runner, provenance (same simulation rules)
physics = "physics.toml"
start_room = "title"
tick_rate = 50                   # default room speed; rooms may override
max_ticks = 1_080_000            # the server rejects longer replays (6 hours at 50)
object_order = [                 # event and collision order; every kind used must be listed
  "kid", "block", { kind = "spike", dir = "up" }, { kind = "spike", dir = "down" },
  "save", "warp", "platform", "trigger", "@boss1",
]
room_order = ["title", "room_1", "room_2"]   # optional; GM room order for room_goto_next/previous

[compat]                         # optional; decision 0005
profile = "gm8/a1@1"             # the package's family bundle; absent is inferred from the physics preset

[[compat.deviation]]             # one declared deviation from the ruleset's closed catalogue
id = "save.position"
value = "xy_base100_bytes"
why = "the original stores the position in bytes"
impact = "F1"
evidence = "trace:rely/room-1"
issue = 1234

[engine]                         # optional; engine settings
new_game_room = "room_1"         # where a new game starts; default start_room (with a warning)
boot = ["@world"]                # persistent kinds created before the first room; default the world kind
uninit_reads_zero = true         # GM "treat uninitialized variables as 0"; default true
float_model = "f64"              # runner number model; only f64 so far
first_runtime_id = 105106        # GM8 id of the first instance created at run time; optional (see Rooms, gm_id)
roles = { kid_bullet = "@bullet", bow = "none" }   # which kind the sim spawns for an engine role

[engine.restart]                 # the retry key (R)
kind = "room_goto_saved"         # game_restart_via_boot | room_goto_saved | showdown_load | recreate_in_place
trigger = "step_pressed"         # key_press_event | step_held | step_pressed | step_pressed_unless_paused
excluded_roles = ["title", "menu"]
# a death-only save that adds no death (wanna saveDeathTime): death_on_restart = "nothing" with kid_steps.save_death_time = true
# death_on_restart = "nothing" | "save_counters" | "save_counters_per_kid"   # per kid: one death for each live kid, none with no kid (showdown)
kid_steps = { add_time = 2, save_death_time = true, clear_frozen = true }   # kid-side steps, all off by default: add_time (ticks added to the play timer first), save_death_time, clear_frozen (frozen and frozen2, globals and every kid), release_death_sound (B: stop the death sound, restore the music volume and destroy the `death_sound` object before the retry), reset_jump_values (showdown; the new kid's jump values, speeds and jump flags), clear_triggers (showdown; the trigger arrays and `global.triggered` after the load)

[engine.reset]                   # the reset key (F2)
kind = "game_reset"              # delete_temp_and_restart | game_reset
trigger = "key_press_event"

[defaults.spike]                 # default sprite and mask of a bare built-in, per variant
sprite = "spike_up"
variants = [{ dir = "down", sprite = "spike_down" }]

[[creators]]
name = "<creator>"
role = "author"                  # author | co-author | music | art | tester | other

[source]
url = "https://delicious-fruit.com/game/19483"
permission = "community-assumed" # granted | community-assumed | pending | revoked
permission_date = 2026-10-24     # when the status was last set
permission_note = ""             # where the permission came from, or why it is pending

[budget]
ops_per_tick = 1_000_000         # script budget units per tick; hard cap 2_000_000

[timing]
start = { trigger = "run_start" }
end = { trigger = "run_end" }
stages = [
  { id = "stage1", name = "Stage 1", trigger = "stage1_clear" },
  { id = "boss", name = "Boss", trigger = "boss1_dead" },
]

[[maps]]                         # optional; see Maps
id = "medium"
name = "Medium"
difficulty = "Medium"            # the original's built-in difficulty, as the game names it

[[maps]]
id = "hard_any"
name = "Hard Any%"
difficulty = "Hard"
category = "Any%"
end = { trigger = "enter_room", room = "ending" }   # timing keys left out come from [timing]
stripped_start = { globals = { difficulty = 2 } }   # optional; overrides [stripped].start key by key

[stripped]                       # optional; ranked (stripped) mode, see Stripped mode
start = { room = "stage1_01", globals = { difficulty = 1 } }   # where a ranked run starts
cutscenes = [                    # the ranked timer is frozen while one holds
  { id = "boss_intro", kind = "@boss_intro" },
  { id = "ending", room = "ending" },
]
dialogues = [                    # text that waits for a press: frozen, and advances on its own
  { id = "shopkeeper", kind = "@textbox", advance = "shoot", timeout_ticks = 150 },
]

[stripped.cut]                   # what stripped mode leaves out
rooms = [
  { room = "intro", to = "stage1_01" },
]
kinds = ["@tutorial_sign"]
instances = [
  { room = "stage1_01", name = "save_hint" },
]

[tricks]                         # keys: research/specs/tricks.md, Format keys
distance_vines = "on"            # on | off | required (off only on a switch)

[[third_party]]
path = "music/stage1.ogg"
what = "Stage 1 music"
owner = "<rights holder>"
license = "unknown"              # SPDX id, or "unknown"
status = "to-clear"              # cleared | to-clear | excluded-from-public

[music.stage1]                   # optional per-track settings; may move to music/<id>.toml
volume = 1.0

[key_art]                        # optional; see Key art
image = "art/key.png"            # the game's art, 1920 x 1080 recommended
maps = { hard = "art/key_hard.png" }   # optional per-map override, keyed by map id

[controls]                       # optional; what the game's extra actions do
extra1 = "Teleport"              # extra1 to extra4: a label for each one the game uses
```

Field rules:

| Field | Required | Meaning |
|---|---|---|
| `format` | yes | Format major version. The client reads every version up to its own. |
| `id` | yes | Package id, snake_case. Stable forever; leaderboards key on it. |
| `title` | yes | Display title. |
| `version` | yes | Semver of our port. For humans. |
| `source_version` | no | Version of the original. |
| `engine_pack` | no | Engine pack id ([engine packs](engine.md)); the pack must be bundled. |
| `engine_rev` | yes | Built-in behavior revision ([architecture](../architecture/replay.md#versions)). |
| `runner` | yes | `gm80` or `gm81`. Records the original's runner for provenance: both use the same simulation rules (research#15). A difference found later becomes a `RunnerRules` field. |
| `physics` | yes | Path to the physics profile. |
| `start_room` | yes | First room. |
| `tick_rate` | no | Default room speed, 50 if absent. |
| `max_ticks` | yes | Upper bound for a replay. |
| `object_order` | yes | Rank order of every kind placed or spawnable. An entry is a kind name or a selector `{ kind = "spike", dir = "up" }` that matches instances by their params, so originals that collapsed into one kind keep their original order (R2). An instance takes the first matching selector of its kind, else the plain entry. The research converter emits it from the original's object order. |
| `room_order` | no | GM room order for `room_goto_next` and `room_goto_previous`. Rooms not listed follow in path order. |
| `[engine]` | no | `new_game_room`, `boot`, `uninit_reads_zero`, `float_model`, `first_runtime_id` (the GM8 instance id the first instance created at run time gets: the game file's last-instance-id + 1, above every `gm_id`; see [Rooms](rooms.md)), `roles` (engine role to kind: `world`, `kid`, `kid_flipped`, `kid_bullet`, `bow`, `blood`, `blood_emitter`, `game_over`, `playerface`, `death_sound`; `"none"` when the game never spawns one), `[engine.restart]` and `[engine.reset]` (restart and reset models, sim plan 4.3). |
| `[defaults.<kind>]` | no | Default `sprite` and `mask` of a bare built-in kind, plus `variants` tables that match param values (`dir = "down"`) and give their own `sprite` or `mask`. |
| `[[creators]]` | yes, at least one | Name and role. |
| `[source].url` | yes | Original game page. |
| `[source].permission` | yes | `granted`, `community-assumed`, `pending` or `revoked`. A `revoked` package is never loaded, built or served. |
| `[source].permission_date`, `permission_note` | no | Audit trail. |
| `[budget].ops_per_tick` | no | Default 1,000,000, hard cap 2,000,000 ([0001](../decisions/0001-script-language.md#step-budget)). |
| `[timing]` | yes | `start` and `end` triggers, and `stages` in order. Each stage ends at its trigger. |
| `[[maps]]` | no | `id`, `name`, optional `difficulty`, `category`, `version`, optional `start`, `end`, `stages` that override `[timing]`, and optional `stripped_start` that overrides `[stripped].start`. See [Maps](#maps). |
| `[stripped]` | no | Ranked (stripped) mode: `start` (room and globals), `cut` (rooms, kinds, instances), `cutscenes` and `dialogues`. See [Stripped mode](#stripped-mode). |
| `[tricks]` | no | Trick switches and labels. The keys are the table in `research/specs/tricks.md`, section Format keys (`[tricks]`): a trick id without its `trick.`, `input.` or `bug.` prefix (`restart_glitch`, `ad_align`, `drift_28_second`). Each key has a type. A `switch` key is an engine behavior the package declares `on` or `off`. An `emergent` key follows from the physics profile, kid family and objects, so `on` and `required` only label it, and `off` is an error. A key that is not in the table, and a key of type `unknown` (`shift_buffering`), is an error. A switch marked `required` needs its switch on in the physics profile: the loader checks `dual_shift` and `restart_glitch`; the switches of `ad_align`, `disable_cancels`, `sub_frame_tap` are not in the profile yet, and `vdiet`, `coyote_time`, `jump_buffer`, `numpad_cancel`, `cactus_phantom_release`, `water_full_jump_1f`, `water1_low_check`, `restart_f2_same_frame` are never on. |
| `[key_art]` | no | `image` (the game's key art) and `maps` (per-map overrides, map id to path). See [Key art](#key-art). |
| `[controls]` | no | A label for each generic extra action the game uses: `extra1` to `extra4`, one line of 1 to 24 characters ("Teleport", "Nudge left"). The client shows a game's labels where the player binds keys and in the HUD's input display, and shows only the extra actions a game labels. Every extra action the game reads (its physics `events.keys`, its scripts) should have one. Labels are cosmetic: they are not in the gameplay hash. |
| `[[third_party]]` | yes, one per file that is not the creator's own work or ours | `path`, `what`, `owner`, `license`, `status`. Commercial music, Kayin's sprites and engine assets all go here. |

## Ruleset and compatibility profile

Decision [0005](../decisions/0005-standard-mechanics.md) splits a package's behavior into a **ruleset** (the engine's semantic contract), an engine-defined **compatibility profile** (a data bundle inside that ruleset plus declared deviations) and `physics.toml` (per-game tuning and event lists). Format 0 reads the first two as declarations only; they cannot change a run.

- **`ruleset = "<name>@<rev>"`.** Absent means `gm8` at the package's `engine_rev`. In format 0 the only ruleset this build implements is `gm8`, and its revision is exactly `engine_rev`; `ruleset = "gm8@0"` is therefore the explicit spelling of the absent field. `standard@1` is not implemented yet, and the format-1 shape (`ruleset@N` superseding `engine_rev`) waits on #795/#928.
- **`[compat] profile = "<ruleset>/<family>@<n>"`** (`gm8/a1@1`). Absent means the bundle of the physics preset's family: `a1` to `a5`, `b` or `c`. The known bundles are revision 1. A different family, an unknown name or another revision is an error. A profile is data: it cannot add code or supply a [runner-rule](../architecture/sim.md) value.
- **`[[compat.deviation]]`** declares one difference the package keeps from the ruleset. Every entry has `id`, `value`, `why`, `impact` (`F1`, `F2` or `F3`), `evidence` and `issue`; a missing or empty provenance field is an error. The ids form a **closed catalogue** in `crates/format/src/compat.rs`: `save.position`, `jump.cancel`, `jump.air`, `jump.water_air`, `fall_cap.rule`, `flip.model`, `platform.take`, `ice.model`, `input.walk`, `input.vine_keys`, `vine.model`, `gravity.reapply`, `slope.mirror_contact` and `slope.downhill`. Each selects an existing enum value and no other field. An unknown id, a value of the wrong type, or a value outside the entry's set is an error.
- **Format 0 resolves a declaration to what the package already does.** The declared profile must name the physics preset's family, and each deviation must name the value the game's physics profile already resolves to (and no room profile may resolve it differently). Otherwise the loader errors with what the package resolves to. Because of that, `ruleset` and `[compat]` are removed before the `gameplay_hash` is computed: writing the explicit fields keeps the hash of the package without them. **The normalization stops there:** a declaration that changes behavior is a format-1 path and must enter `gameplay_hash` (user decision 2). Format 0 cannot hash one because it rejects it; format 1, when it freezes, will hash the resolved ruleset, profile and deviations.
- `iwbtg pkg check` prints the resolved values (`ruleset: gm8@0`, `profile: gm8/a1@1`, `deviation: save.position=xy_base100_bytes`).

## Timing triggers

The triggers form one chain, evaluated at the end of every tick: `start`, then each stage in order, then `end`.

- Vocabulary: `run_start` (true at once: the run starts on its first tick), `run_end` (true in the tick `game_clear` ends the run) and `enter_room` with `room = "<id>"` (true while the kid's world is in that room).
- Each trigger fires at most once. Nothing fires before `start`. A stage is looked at only after the stage before it fired, so stages never fire out of order. Several triggers can fire in one tick. `end` can fire with stages still unreached. After `end` nothing fires.
- A split is its chain index (0 start, then the stages, then the end) and the tick it fired at. The tick is the number of ticks run so far, so `run_start` is tick 1 and a run's time is `end - start` in ticks.
- `iwbtg replay run` prints the splits as `<index>@<tick>`.
- A `.iwbr` replay (stored in `replays/` or submitted) uses replay format 2: its header carries the run's mods as a bit set (Sudden Death, visual mods, practice assists; [replay format, Mods](../architecture/replay.md#mods)). Format 1 files, which had no mods field, are no longer read. None were ever stored. Under Sudden Death a run ends on the tick of its first death, so its `end` trigger can only fire in a deathless run.

## Maps

A package holds every map of its game: each built-in difficulty, each category (Any%, 100%, True Ending) and each version, like the beatmaps of an osu! set ([content](../content.md#package-file)). Game select lists the game once and expands it into its maps. Each map has its own timing chain and its own leaderboard.

- `id` is the map's snake_case id, unique within the package. Stable forever: leaderboards key on (package id, map id, `gameplay_hash`).
- `name` is shown in game select. `difficulty` names the original's built-in difficulty, `category` the run category; both are display text and optional. `version` is the original's version the map plays, default `source_version`.
- `start`, `end` and `stages` work as in `[timing]` ([Timing triggers](#timing-triggers)); each key a map leaves out comes from `[timing]`. `stages = []` is a map with no splits. The stage rules hold per map: stage ids unique, no two stages on one trigger, no stage on the start trigger.
- Without `[[maps]]` the package has one map, id `main`, named after the game and timed by `[timing]`.
- The loader resolves each map's triggers like `[timing]`'s (the vocabulary, and `enter_room` rooms must exist) and returns every map's chain (`LoadReport::maps`). It warns when an `enter_room` room cannot be reached ([Validation](validation.md) step 7).
- `name`, `difficulty` and `category` are left out of the `gameplay_hash`, so renaming a map keeps its board; the timing keys count.
- `stripped_start` is the map's ranked start point (`{ room, globals }`, [Stripped mode](#stripped-mode)): how the map selects the original's difficulty or version once the menus are cut. Keys it leaves out come from `[stripped].start`; `globals` merge by name, the map's value winning. A Play original run picks the difficulty in the game's own menu, as the original does.
- **Running a map** (#505). A run plays one map (`SimOptions::map`; the loader puts every map's chain and merged ranked start in `GameDef::maps`). Its splits and ranked time follow that map's chain, in both play modes; a ranked (stripped) run also starts at the map's `stripped_start`. Play original still starts in the game's own menu. A run with no map given plays the first map (`main` without `[[maps]]`); a map id the package does not have stops the run before its first tick. The replay header records the map id (replay format 5), and `iwbtg replay run --map` picks it for an `.inputs` file.

## Stripped mode

Ranked (stripped) mode plays the game without its frame: no title screen, menus, file select or difficulty select ([product](../product.md#two-ways-to-play-each-game)). `[stripped]` says how, per package. What a given game cuts, where its ranked run starts and which parts are cutscenes is decided per game in `docs/games/<game-id>.md`, never guessed by a porter or agent. Play original ignores `[stripped]`. All of it counts for the `gameplay_hash`. Without `[stripped]` a ranked run starts in `[engine].new_game_room` and nothing is cut or frozen.

- **`start`**, the ranked start point: `room` is the room a ranked run starts in (default `[engine].new_game_room`, else `start_room`). `globals` are set before that room's creation, in place of what the skipped menus set (`difficulty`, a save slot, a version flag). Names are GML variable names; values are a number (or `{ bits }`), a string, or a boolean (true is 1), as in `set_global`. A map's `stripped_start` overrides it key by key ([Maps](#maps)). The ranked timer starts on the first input after the start ([product](../product.md#two-ways-to-play-each-game)); the run ends at the map's `end` trigger.
- **`[stripped.cut]`**, what is left out:
  - `rooms`: `{ room, to }`. Any room change to `room` (warp, `room_goto`, `room_goto_next`, a save load) goes to `to` instead. A `to` room cannot be cut itself, and the ranked start room cannot be cut.
  - `kinds`: placed instances of these kinds are not created when a room loads, and room `spawn` actions of them do nothing. Scripts are not affected: a script that creates a cut kind still creates it.
  - `instances`: `{ room, name }`, one placed instance by its `name` in that room.
- **`cutscenes`**: regions that freeze the ranked timer. Each has an `id` (unique among cutscenes and dialogues) and exactly one condition: `kind` (holds while any instance of that kind exists) or `room` (holds while the kid's world is in that room). Ranked time is the run's ticks outside every region. The player can always skip one: the client runs the region's ticks at once without drawing them, so the replay and the sim do not change (`Sim::in_region`, the client's skip-cutscene key, #573).
- **`dialogues`**: cutscene regions for text that waits for a press. Besides `id` and the condition they name `advance`, the action the original reads to go on (an action name: `jump`, `shoot`, `accept`, ...), and `timeout_ticks` (at least 1). While a dialogue region holds, the sim counts the ticks since the region began or since the last press of `advance`; when the count reaches `timeout_ticks`, the sim presses `advance` itself in that tick (held and pressed for one tick). The press is derived, so replays never store it, and nobody gets free rest time in a text box.
- The loader checks that every room, kind and instance name resolves, that nothing is cut twice, that region ids are unique and that the ranked start room is not cut. A start global named `difficulty` also sets the engine's difficulty (what built-ins such as the bow read), so it must be a whole number from 0 to 255.
- **In the sim** (#561). A run has a play mode, `original` or `stripped` (`SimOptions::mode`, recorded in the replay header and hashed with the world). A stripped new game creates the boot instances, sets the start's `globals` (and the engine difficulty from `difficulty`), then enters the start room. Every room entry goes through the cut rooms: a request (`room_goto`, `room_goto_next`, a warp), `game_restart`'s first room, a start point and a save load. The start is the run's map's merged start (`MapDef::start`, [Maps](#maps), #505). The client buttons are #43.
- **Regions in the sim** (#34). `stripped::begin_tick` runs at tick row 0, before the input is latched, on the world as the previous tick left it. A tick that starts with any region holding is frozen; the timer's other ticks are ranked time (`TimingState::ranked_ticks`, `Sim::ranked_ticks`, the replay trailer's `ranked_ticks`). The tick the start trigger fires in is not the timer's, the tick the end fires in is, so ranked plus frozen ticks is `end.tick - start.tick`. A dialogue's count goes up once per tick it holds, goes to 0 on a real press of `advance` (holding is no press) or when the region stops holding, and in the tick it reaches `timeout_ticks` the frame gets `advance` held and pressed and the count goes back to 0: with `timeout_ticks = 150` the first derived press is the region's 150th tick. Play original has no regions.
- **The derived press is press-only** (#574). It sets `advance` held and pressed for its one tick and adds no `released` edge on the next tick: the next tick runs exactly as the player's frame says. The reason is what the 20 games' text reads. Their text is shown by the runner's modal `show_message` box (13 pause boxes, signs, endings, RBF's quiz), which the runner's own dialog closes rather than game input, or opened by a press: signs use a "Key up pressed" event or an Up press check. No game has a Key Release event, and the only `keyboard_check_released` reads in the input survey are the kid's jump cut (research `specs/engines/gm8/CLONE_SPEC.md`, events used by the 20 games and How the 20 games read input; `specs/mechanics.md`, builtin.sign and Client features implied). So no text box waits for a release. If a port ever adds one, `advance` gets a `released` edge then, decided in that port's issue.
- **No stripped-mode query for scripts** (decided in #561, `revisit`). Scripts behave the same in both modes; everything stripped mode changes is data in `[stripped]`, so a port's two modes cannot drift apart in code and a reviewer sees every ranked difference in `game.toml`. If a game needs a script to know (for example a menu object that also runs in play rooms), a query can be added later without breaking any package.

## Key art

Key art is the picture behind every screen of the background system (game select, loading, gameplay, pause, results; [product](../product.md)) and on the game's panel in the game select list. It never changes the sim, so `[key_art]` and the image files are outside the `gameplay_hash`.

- `image` and each `maps` value are paths inside the package to a PNG (the usual place is `art/`). They follow the image rules in [Assets](assets.md): at most 4096 px on a side.
- Recommended: **1920 x 1080 (16:9)**, the size the phase 6 mockups are drawn at (`docs/design/mockups/m4/`). The client scales it to cover the window, crops what does not fit, blurs it with the player's blur slider and dims it, so keep the subject near the centre and avoid small text. A picture that is not 16:9 (within 1%) or smaller than 1280 x 720 gets a warning.
- `maps` keys are map ids. A map with an entry shows that picture; every other map and the game itself show `image`. The loader rejects a key that is not a map id of the package (`main` when there is no `[[maps]]`).
- **Fallback** when there is no key art for the game or map: the background of the first ranked room. That room is the `[timing].start` room when the start trigger is `enter_room`, else `[engine].new_game_room`, else `start_room`. Its first background layer behind the instances (the `background` key, or the first `backgrounds` entry without `foreground`; image `backgrounds/<id>.png`) is used if it has one, else its `background_color`, else the client's own default backdrop. `iwbtg_format::key_art::resolve` gives the client this choice.
- `iwbtg pkg check` checks each path (inside the package, not under `tests/`, ends in `.png`, present), decodes the image, and checks its size.
