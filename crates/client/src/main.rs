//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.

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
    package: PathBuf,
    /// Load converter output: unmapped script kinds become inert.
    #[arg(long)]
    trace_mode: bool,
    /// Frame cap: a number of frames per second, or `unlimited`. The sim ticks at the game's
    /// rate either way.
    #[arg(long, value_name = "FPS", default_value = "60")]
    fps: view::FrameCap,
    /// Blend frames between the last two ticks for smoother motion on fast displays. Off by
    /// default: the originals draw every step, so a blended frame shows positions the game never had.
    #[arg(long)]
    interpolate: bool,
    /// Save a screenshot of the window to this PNG after a few frames and exit (for checks).
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match package::open(&args.package, args.trace_mode) {
        Ok(pkg) => match view::run(pkg, args.fps, args.interpolate, args.screenshot) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("{}: {e}", args.package.display());
            ExitCode::FAILURE
        }
    }
}
