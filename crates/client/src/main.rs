//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture/client.md).
//! `iwbtg-client --ui-gallery`: the UI kit's component gallery (docs/architecture/client.md, UI).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.
//!
//! Each module owns its command-line flags (a clap `Args` group) and its systems (a Bevy
//! `Plugin`). A new module adds one `mod` line, one flattened group and one plugin here and
//! nothing else (#530), so parallel client work doesn't collide in this file.

// The browser build opens only the gallery until the web client loads packages, so the game
// path is compiled there but not called.
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]

mod art;
mod audio;
mod input;
mod mods;
mod package;
mod retry;
mod savefile;
mod screen;
mod settings;
mod stats;
mod storage;
mod ui;
mod view;

use std::process::ExitCode;

use clap::Parser;

/// Plays a game package.
#[derive(Parser)]
#[command(name = "iwbtg-client", version, about = "IWBTG Collection client")]
struct Args {
    #[command(flatten)]
    package: package::PackageArgs,
    #[command(flatten)]
    view: view::ViewArgs,
    #[command(flatten)]
    audio: audio::AudioArgs,
    #[command(flatten)]
    screen: screen::ScreenArgs,
    #[command(flatten)]
    ui: ui::UiArgs,
    #[command(flatten)]
    mods: mods::ModsArgs,
    #[command(flatten)]
    save: savefile::SaveArgs,
    #[command(flatten)]
    retry: retry::RetryArgs,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    let args = Args::parse();
    // The settings file gives the defaults the flags override (settings.rs). A `--screenshot`
    // check neither reads nor writes it, so it looks the same on every machine.
    settings::load(args.view.screenshot.is_some());
    if let Err(e) = screen::configure(&args.screen) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    if args.ui.ui_gallery {
        // `configure` has checked the ratio.
        let ratio = args
            .screen
            .ratio
            .as_deref()
            .and_then(|r| screen::parse_ratio(r).ok().flatten());
        ui::gallery::run(
            args.view.screenshot,
            args.ui.reduced_motion,
            args.screen.window,
            ratio,
        );
        return ExitCode::SUCCESS;
    }
    // Clap requires the package unless `--ui-gallery` is given.
    let Some(dir) = args.package.package else {
        return ExitCode::FAILURE;
    };
    savefile::configure(&args.save);
    let pkg = match package::open(&dir, args.package.trace_mode) {
        Ok(pkg) => pkg,
        Err(e) => {
            eprintln!("{}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    let audio = args.audio;
    let run_mods = args.mods.mods;
    let retry = args.retry;
    // A `--screenshot` check keeps no stats, as it keeps no settings.
    let keep_stats = args.view.screenshot.is_none();
    eprintln!("{}", mods::describe(run_mods));
    let result = view::run(pkg, args.view, |app, pkg| {
        app.add_plugins((
            screen::ScreenPlugin,
            savefile::SavePlugin,
            art::ArtPlugin(pkg.files.clone()),
            audio::AudioPlugin::new(&audio, &pkg.files),
            mods::ModsPlugin(run_mods),
            retry::RetryPlugin(retry),
            stats::StatsPlugin {
                keep: keep_stats,
                mods: run_mods,
            },
        ));
    });
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
    ui::gallery::run(None, false, None, None);
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
