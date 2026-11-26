# The client (`iwbtg-client`)

Part of [Architecture](../architecture.md).

One page per module or area, so two PRs on different modules never edit the same file. A new module gets its own page here and one line in this list (alphabetical).

- [Room art](client/art.md): `art.rs`
- [Audio](client/audio.md): `audio.rs`, `audio/unlock.rs`
- [Controls panel](client/controls.md): the game picker, `[controls]` labels and rebinding (`controls.rs`, #52)
- [HUD](client/hud.md): `hud/`
- [Structure](client/main.md): how modules own their flags and plugins (`main.rs`)
- [Import](client/import.md): `.iwbz` files, the library, drag and drop (`import.rs`)
- [Input](client/input.md): the device collector, bindings and the active controller (`input.rs`, `bindings.rs`)
- [Loading card](client/loading.md): `loading/`
- [Locale](client/locale.md): UI translations, English and Japanese (`locale.rs`, `crates/client/locales/`)
- [Mods](client/mods.md): `mods.rs`
- [Overlay](client/overlay.md): the F3 debug overlay (`overlay.rs`)
- [Particles](client/particles.md): drawing `particle_system` systems (`particles/`)
- [Pause menu](client/pause.md): `pause.rs`
- [Performance](client/perf.md): the `--perf` frame measurement (`perf.rs`, #1024)
- [Player card](client/card.md): `card/`
- [Practice assists](client/practice.md): slow motion and frame advance (`practice.rs`)
- [Render fixes](client/render_fix.md): workarounds for Bevy and wgpu backend bugs (`render_fix.rs`)
- [Replays](client/replays.md): `replays.rs`
- [Results](client/results.md): `results/`
- [Retry](client/retry.md): `retry.rs`
- [Screen](client/screen.md): `screen.rs`
- [Script error screen](client/script_error.md): a halted game's error, file and line (`script_error/`)
- [Settings](client/settings.md): `settings.rs`
- [Shell](client/shell.md): screens as Bevy states, packages loaded and unloaded at run time, `--select` (`shell/`)
- [Skip](client/skip.md): skipping a cutscene or dialogue region (`skip.rs`)
- [Stats](client/stats.md): `stats/`
- [Storage](client/storage.md): `storage.rs`
- [UI](client/ui.md): the UI kit (`ui/`)
- [Visual mods](client/visual.md): hitboxes and no background (`visual.rs`)
- [The window and the view](client/view.md): opening a package, drawing the sim, the fixed clock, input and bindings, interpolation, the demo packages (`package.rs`, `view.rs`, `input.rs`, `bindings.rs`)
- [Web build](client/web.md): the browser entry, fetching an `.iwbz` and running the same client (`web.rs`; `crates/client/web/`)
