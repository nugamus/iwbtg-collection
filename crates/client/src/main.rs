//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! This first slice shows the start room: every visible instance with a sprite is drawn where
//! the sim put it. The tick loop that runs the sim is the next step (#145).

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
    /// Save a screenshot of the window to this PNG after a few frames and exit (for checks).
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match package::open(&args.package, args.trace_mode) {
        Ok(pkg) => match view::run(pkg, args.screenshot) {
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
