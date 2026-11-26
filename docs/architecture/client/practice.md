# The client: Practice

Part of [the client](../client.md).

## Practice mode

A practice run (`Practice`, #39) can start anywhere in a package, and it is never ranked
(docs/product.md, Practice). Game select's **Practice** button opens a panel (`select/practice.rs`)
with three pickers for the picked map:

- **Stage:** each stage whose `[timing]` trigger enters a room, as `StartPoint::Room` in that
  room. A stage without a room (an `end` on a room-less trigger) is shown faded.
- **Save:** the game's own save (`savefile::start_point`) when there is one, "Start of the
game", and every placed `save` object of a play room (`save::point_key` gives its stable key;
the panel builds a `Checkpoint` from it), as `StartPoint::FromSave` (#1579).
- **Room:** every room with `role = "play"`, as `StartPoint::Room`.

Start stages the package and map with that explicit start (`shell::start_map`'s `start`), sets
`Practice`, and the command line's Play original/Ranked actions clear it again. A practice run:

- shows the Unranked pill and the loading card's `Practice runs stay off the leaderboard.`
- is never put on a stats board (`stats` checks the flag), and
- is never recorded as a replay (`replays` skips it: restoring a save state breaks an
  input-only replay).

The panel reads the whole package once when it opens (the list keeps only the strip it draws),
so a very large package can take a moment there. `--practice-preview` (hidden, with `--select`)
opens it at start for screenshots.

## Save states

Save states (`Save states` mod, `SS`; F5 saves, F6 loads) are a practice aid: F5 stores the sim's
whole world (`Sim::snapshot`) in memory for the run, F6 restores it (`Sim::restore`) between
steps. They are available in a practice run and in any run with the Save states mod. Nothing is
written to disk, and a run that uses them is practice: never ranked, never recorded. Play
original and Ranked runs without `SS` ignore the keys.

## Practice assists

The practice assists that act on the clock (`practice.rs`, `PracticePlugin`, #551; [product](../../product.md#mods)): Slow motion (`--mods SL`) and Frame advance (`--mods FA`), picked like every mod ([mods](mods.md)). Neither changes what a tick computes, only when ticks run, so the replay plays back the same at normal speed. Both are practice mods, so the run is never ranked: the start line says `unranked: <mod name>` (`ModSet::unranked_by`, `mods::describe`) and the HUD's mod row shows the Unranked pill and the codes, until the loading card (#495) shows it. They come from the run's `mods::RunMods` and `SlowSpeed`, read when the run opens (#903; `--mods` and `--slow-speed` only seed them, game select's mod select sheet changes them). When a run opens the plugin prints how to play them, for example `Slow motion at 50%; Frame advance: the frame-advance key (F by default) runs one tick`.

- **Slow motion.** Its speed is a client setting, not stored in the replay: `[practice] slow_speed` in the [settings](settings.md) file (75 or 50, default 75), which the mod select sheet's 75% / 50% pills write when picked, so the next start keeps it (#904). `--slow-speed <75|50>` (also `50%`; anything else is refused) overrides it for one run and is not saved (`practice::first_speed`), as `--auto-retry` does for `[retry] auto`. When a run opens (`practice::start`, before the view's `RunSteps::Core`) the plugin sets `view::ClockSpeed` (0.75, 0.5, or 1 without Slow motion), from which `view::enter` sets the first tick's timestep; `view::follow_room_speed` multiplies the room's rate by it after every tick, so a 50 Hz game ticks at 37.5 or 25 Hz. Every voice plays at the same factor ([audio](audio.md)), so music stays on the tick clock. Stats count play time from the fixed timestep, so a slowed run's play time is the real time played.
- **Frame advance.** The run's first tick runs (so the room is on screen), then `SimTick` is gated until a step is pending. `frame-advance` is a client key (`bindings::ClientKey`, F by default, no controller default; `--bind frame-advance=...`); the collector counts its presses (`Collector::take_advance`), which never enter an `InputFrame`. `take` turns them into pending steps before `SimTick` in each fixed step, and `spend` (in `SimTick`, after `view::tick`) uses one when a tick ran. So one press is one tick, two presses before a fixed step are two ticks on two fixed steps, and a press while something else holds the sim (the pause menu, a pending retry) waits for it. Input collected between steps reaches the next tick with its edges. Looping tunes (`audio::MusicSync`) are held paused; `audio::sync_music` keeps their position on the tick count. One-shot sounds play.
- **Both.** With `SL+FA` each press still runs one tick; the slower clock only delays when within a fixed step it lands.
