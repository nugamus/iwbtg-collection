# The client: Pause menu

Part of [the client](../client.md).

Pause menu (`pause.rs`, `PausePlugin`, #479; docs/product.md, Pause; artboards `Pause169`, `Pause43`, `PauseBox`): Escape (`pause::PAUSE_KEY`, read straight from Bevy) or Start on a controller (`pause::PAUSE_PAD`, #624) opens and closes it. Start has no game action by default (the game's `menu` action has no controller default, see the view page), so it never reaches the sim or a replay unless the player binds `menu` (or another action) to it; then it does both, as Escape, the keyboard's default `menu` key, does (whether the tick before the menu opens sees it depends on frame timing, #758).

While it is open the `SimTick` set does not run (a run condition, as for Sudden Death), every playing voice is paused and resumed on close (a voice the game paused stays paused), and the backdrop and HUD keep running. The timer keeps running: `count` runs on the fixed clock while the menu is open and counts each tick in which the map's timer runs (started, not ended, sim running), keyed by the sim tick the pause began at (`Menu::pauses`); `publish` writes the total to `hud::Paused`, which run info adds to `Sim::ranked_ticks` and uses for the paused line. A retry clears the pauses (`pause::reset`). The replay's trailer carries them (`replays::sync_pauses`; docs/architecture/replay.md, Trailer) and the stats add them to the clear and the splits (#623).

Drawing: bevy_ui on the HUD's camera, in game pixels. The root covers the game's rectangle (`HudLayout::game`) with the dim and centres the box; every size is `PauseBox`'s times the game's scale (rebuilt when that scale changes), so the box scales with the game at every ratio. The dim (artboard `rgba(0,0,0,.5)`) and the box fill (`.94`) go through `black_over`: Bevy blends in linear light, the browser in sRGB, so the alpha becomes `1 - (1 - a)^2.2` (0.78 for the dim, 0.998 for the box), which leaves `1 - a` of the sRGB value as the browser does. The selected item is white, weight 600, and slides 4 game px right (OutQuint, 300 ms); the others are `#8d909e`, weight 500. The selector is the delicious fruit, stepping between two frames every 320 ms: the package's sprite `cherry` (its first two 21x24 frames; docs/format/v0.md, Assets) when the package has one, else a placeholder cherry drawn in code (no game art in this repo).

Items:

- Continue.
- Retry from save: closes and presses the game's `restart` action for the next tick (what R does).
- Restart run: `retry::request`, the quick-retry key.
- Quit: closes the client (there is no game select to return to yet). No results screen; the run's timer has not ended, so no clear reaches the stats. The replay is written as for any unfinished run, like a window close.

Keys: Up/Down and the keys bound to `up`/`down` move the selector; Enter, Space and the keys bound to `jump`/`accept` pick; R (the keys bound to `restart`) and the quick-retry key pick their items at once; Escape continues. The mouse selects on hover and picks on click (`Activate`). Key edges collected while the menu was open are dropped when it closes, so nothing pressed in the menu reaches the game.

Controller: `keys` reads the collector's pad state (`Collector::pad_down`, so the active-pad rule and the stick gate apply as in play) and acts on the inputs that went down since the last frame (`pad_command`). Start and B (east) continue; A (south) picks the selected item; the D-pad and the left stick move the selector; then, as for keys, the inputs bound to quick-retry pick Restart run, `restart` (Y by default) picks Retry from save, `jump`/`accept` pick and `up`/`down` move. Fixed buttons win over bindings, so B continues even though `suicide` is bound to it. The choice of Start, with `menu` moved off it, is cheap to change (#624, `revisit`).

The game's own pause (the P key, the sim's `pause` action) is a different thing: it is sim input and stays the game's in both modes, recorded in the replay like any key. A game whose pause deactivates instances keeps ticking, so its timer runs; the originals' runner-modal pauses (a message box, `keyboard_wait`) are a client freeze per game (`world` built-in, `pause = "none"`).

`--pause-preview <continue|retry|restart|quit>` (hidden) opens the menu at sim tick 25 with that item selected, for screenshot checks.
