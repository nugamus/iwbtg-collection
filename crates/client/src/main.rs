//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.

mod bindings;
mod input;
mod package;
mod view;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Plays a game package.
#[derive(Parser)]
#[command(name = "iwbtg-client", version, about = "IWBTG Collection client")]
struct Args {
    /// Package folder (a folder with a game.toml).
    package: Option<PathBuf>,
    /// Load converter output: unmapped script kinds become inert.
    #[arg(long)]
    trace_mode: bool,
    /// Frame cap: a number of frames per second, or `unlimited`. The sim ticks at the game's
    /// rate either way.
    #[arg(long, value_name = "FPS", default_value = "60")]
    fps: view::FrameCap,
    /// Change a global binding and save it: `ACTION=INPUT[,INPUT...]`, an input being a key name
    /// (`Space`, `LShift`, `F5`) or `pad:<button>` (`pad:south`, `pad:dpad-left`, `pad:stick-up`).
    /// Repeatable. Replaces that action's keys (or controller inputs). With a package, the game
    /// then starts; without one, the bindings are only saved.
    #[arg(long, value_name = "ACTION=INPUT")]
    bind: Vec<String>,
    /// Print the global bindings and exit.
    #[arg(long)]
    show_bindings: bool,
    /// Bindings file (default: the user's config folder, iwbtg-collection/bindings.toml).
    #[arg(long, value_name = "FILE")]
    bindings_file: Option<PathBuf>,
    /// Save a screenshot of the window to this PNG after a few frames and exit (for checks).
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
}

fn load_bindings(args: &Args) -> Result<bindings::Bindings, String> {
    let path = match &args.bindings_file {
        Some(p) => p.clone(),
        None => bindings::default_path().ok_or("no config folder found; pass --bindings-file")?,
    };
    let mut config = bindings::load_config(&path)?;
    if !args.bind.is_empty() {
        for spec in &args.bind {
            config.set(spec)?;
        }
        bindings::save_config(&path, &config)?;
        eprintln!("bindings saved to {}", path.display());
    }
    bindings::Bindings::from_config(&config).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    let args = Args::parse();
    let bindings = match load_bindings(&args) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if args.show_bindings {
        print!("{}", bindings.describe());
        return ExitCode::SUCCESS;
    }
    let Some(package) = &args.package else {
        if args.bind.is_empty() {
            eprintln!("give a package folder to play, or --bind / --show-bindings");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    };
    match package::open(package, args.trace_mode) {
        Ok(pkg) => match view::run(pkg, args.fps, args.screenshot, bindings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("{}: {e}", package.display());
            ExitCode::FAILURE
        }
    }
}
