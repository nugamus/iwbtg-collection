# The client: Structure

Part of [the client](../client.md).

Structure (#530, #948): each module owns its command-line flags (a clap `Args` group) and its systems (a Bevy `Plugin`), and exposes both from one `registration::Registration` item. `main.rs` names each module once, in a list the `client_modules!` macro turns into the module declarations, the flattened command line, and the startup and plugin passes:

```rust
client_modules! {
    shell,
    bindings,
    screen,
    hud,
    // ...
}
```

The list order is the order of everything: each module's `prepare` (flag checks and settings) and `standalone` (a mode that needs no window: `--ui-gallery`, `--card-preview`, `--show-bindings`) run before the window opens, and every module's `add` runs when the app is built. A module that adds no plugin, or has no flags, uses the default method; a plugin-only module takes `registration::NoArgs` as its group. A module may add several plugins in one `add` (the shell adds the shell and the select plugins).

The cross-module flags travel in a `registration::Startup` value (`--screenshot`, `--ranked`, `--trace-mode`, `--window`, `--ratio`, the package and the select folder), so a module reads what it needs without another module's group: the stats, replays and import plugins keep nothing on a `--screenshot` check, and the shell passes `--trace-mode` to every package it opens. `prepare` also stores what its plugin reads later (the loaded bindings, the audio levels); `standalone` returns `Some` to end startup with that mode's result.

`view::run` builds the app with the window and the view's own plugin, then calls back into the list, which adds the plugins in list order. The browser entry (`web.rs`) has no command line: it builds the same `Args` with `Default` and runs the same list ([Web](web.md), #29). No plugin takes the package: since the [shell](shell.md) (#782) each module builds its run in `OnEnter(AppScreen::Run)` from `shell::LoadedPackage` and drops it in `OnExit`, and puts its run systems in `shell::InRun`. The UI kit's `ui::UiKitPlugin` is added by the screens that use it: the HUD and the gallery, which builds its own app instead of `view::run`.

Adding a feature module adds one line to the list (the macro declares `mod`) and its `Registration`; its flags, resources and systems stay in its own file, and `main.rs` gains nothing else. A test (`the_command_line_is_consistent`) catches two modules claiming the same flag.
