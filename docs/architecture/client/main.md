# The client: Structure

Part of [the client](../client.md).

Structure (#530): each module owns its command-line flags, a clap `Args` group flattened into `main.rs` (`package::PackageArgs`, `view::ViewArgs`, `audio::AudioArgs`, `screen::ScreenArgs`, `ui::UiArgs`, `hud::HudArgs`, `mods::ModsArgs`, `savefile::SaveArgs`, `retry::RetryArgs`), and its systems, a Bevy `Plugin` (`screen::ScreenPlugin`, `art::ArtPlugin`, `audio::AudioPlugin`; `hud::HudPlugin`; the UI kit's `ui::UiKitPlugin` is added by the screens that use it: the HUD, and the gallery, which builds its own app instead of `view::run`). `view::run` builds the app with the window, the sim and the view's own systems, then calls back into `main.rs`, which adds the plugins in one list. A new module adds one `mod` line, one group and one plugin to `main.rs`; its flags, resources and systems stay in its own file. A test (`the_command_line_is_consistent`) catches two modules claiming the same flag.
