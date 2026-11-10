//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.
//!
//! Each module owns its command-line flags (a clap `Args` group) and its systems (a Bevy
//! `Plugin`). A new module adds one `mod` line, one flattened group and one plugin here and
//! nothing else (#530), so parallel client work doesn't collide in this file.

mod art;
mod audio;
mod input;
mod package;
mod savefile;
mod screen;
mod settings;
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
    save: savefile::SaveArgs,
}

fn main() -> ExitCode {
    let args = Args::parse();
    // The settings file gives the defaults the flags override (settings.rs). A `--screenshot`
    // check neither reads nor writes it, so it looks the same on every machine.
    settings::load(args.view.screenshot.is_some());
    if let Err(e) = screen::configure(&args.screen) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    savefile::configure(&args.save);
    let pkg = match package::open(&args.package.package, args.package.trace_mode) {
        Ok(pkg) => pkg,
        Err(e) => {
            eprintln!("{}: {e}", args.package.package.display());
            return ExitCode::FAILURE;
        }
    };
    let audio = args.audio;
    let result = view::run(pkg, args.view, |app, pkg| {
        app.add_plugins((
            screen::ScreenPlugin,
            savefile::SavePlugin,
            art::ArtPlugin(pkg.files.clone()),
            audio::AudioPlugin::new(&audio, &pkg.files),
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
