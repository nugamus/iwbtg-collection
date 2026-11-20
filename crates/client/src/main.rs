//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture/client/view.md).
//! `iwbtg-client --select <folder>`: a placeholder list of the packages in a folder; quitting a
//! game goes back to it (docs/architecture/client/shell.md).
//! `iwbtg-client --ui-gallery`: the UI kit's component gallery (docs/architecture/client/ui.md).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.
//!
//! Each module owns its command-line flags and its systems; it exposes both from one
//! `Registration` item (a clap `Args` group and a Bevy `Plugin`). The list below is the only
//! place a module is named: the `client_modules!` macro declares it, flattens its group into the
//! command line, and runs its startup checks and plugin in list order. A new module adds one
//! line here and a `Registration` in its own file, so parallel client work doesn't collide in
//! this file (#530, docs/architecture/client/main.md).

// The browser build opens only the gallery until the web client loads packages, so the game
// path is compiled there but not called.
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]

mod input;
mod registration;
mod render_fix;
mod settings;
mod storage;

use std::process::ExitCode;

use clap::Parser;

// One line per feature module. The order is the order startup checks and plugins run: the shell
// first (the other plugins use its states), the bindings before the screen flags (`--show-bindings`
// ends startup), the screens and the UI before their standalone modes (`--ui-gallery`,
// `--card-preview`), and the run plugins last.
client_modules! {
    shell,
    bindings,
    screen,
    hud,
    ui,
    card,
    package,
    savefile,
    audio,
    mods,
    view,
    art,
    visual,
    practice,
    retry,
    skip,
    pause,
    results,
    stats,
    replays,
    import,
    overlay,
    loading,
    particles,
    script_error,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    let mut args = Args::parse();
    // The settings file gives the defaults the flags override (settings.rs). A `--screenshot`
    // check neither reads nor writes it, so it looks the same on every machine.
    let isolated = args.view.screenshot.is_some();
    settings::load(isolated);
    // What the modules need from each other's flags before any plugin is built.
    let startup = registration::Startup {
        isolated,
        screenshot: args.view.screenshot.clone(),
        package: args.package.package.clone(),
        library: args.shell.select.clone(),
        ranked: args.view.ranked,
        trace_mode: args.package.trace_mode,
        window: args.screen.window,
        ratio: args
            .screen
            .ratio
            .as_deref()
            .and_then(|r| screen::parse_ratio(r).ok().flatten()),
    };
    // Validate the flags and open any standalone mode (gallery, card preview, `--show-bindings`).
    if let Some(result) = args.prepare(&startup) {
        return exit_code(result);
    }
    let first = args.package.package.clone();
    let trace_mode = args.package.trace_mode;
    let result = view::run(first.as_deref(), args.view.clone(), trace_mode, move |app| {
        args.add_plugins(app, &startup)
    });
    exit_code(result)
}

fn exit_code(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// In a browser there is no command line and no package folder yet: the build opens the UI
/// gallery (docs/dev-setup.md, UI gallery in a browser).
#[cfg(target_arch = "wasm32")]
fn main() {
    #[cfg(target_os = "unknown")]
    input::watch_unmapped_pads();
    // Without --screenshot the gallery has no size check, so it cannot return an error here.
    let _ = ui::gallery::run(None, false, None, None);
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    /// Clap checks the flattened groups only when it builds the command: two modules with the same
    /// flag or group name fail here instead of at start-up.
    #[test]
    fn the_command_line_is_consistent() {
        super::Args::command().debug_assert();
    }
}
