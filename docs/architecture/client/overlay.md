# The client: Overlay

Part of [the client](../client.md).

The F3 debug overlay (`overlay.rs`, `OverlayPlugin`, #598): ticks per second (and the game's current rate, `game N`), frames per second, the tick, the sim's status when it halted or ended, the room (name and size), the kid's x, y, hspeed and vspeed (or `no kid`), and the input the last tick saw (`Collector::describe`: actions, and the controller inputs that are down). The counts come from `view::Stats`, which the view's tick adds to and `overlay::show` turns into rates once a second.

Off by default. F3 shows and hides it and saves the choice (`hud.overlay` in the settings file, [Settings](settings.md)). `--overlay` shows it for one run without saving. A `--screenshot` check never draws it unless `--overlay` is given, whatever the setting.

Drawing: one bevy_ui panel on the HUD's camera, so the game's scale never applies; the UI scale does. The panel is a kit glass box (`tokens::GLASS`, `tokens::SOFT` text, `Type::barlow(12, 500)` with tabular figures), at most 260 UI px wide, `GlobalZIndex` 15: over the HUD's columns (10), under the pause menu (20) and the notices (30). `place` puts it, after `screen::apply`, from `HudLayout`:

- HUD shown: the bottom of the run info column (the right column), one notice margin in. Run info sits in the column's middle and the leaderboard column holds the sound note at its top and the notices at its bottom, so this is the free corner. In a small window (960x540) the column is short and the panel covers run info's lower rows; it never covers the game.
- HUD hidden: the window's top-left corner (the sound note goes top right then), as wide as the space beside the game when that is at least 120 UI px (`MIN_BESIDE`). With less (a 4:3 window with the HUD hidden, where the game fills the width) it is the one case that draws over the game.

The `the_panel_stays_off_the_game` unit test checks the placement at 16:9, 4:3, 21:9 and 960x540 with the HUD shown and hidden.
