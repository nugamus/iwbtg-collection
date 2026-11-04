//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.

mod art;
mod input;
mod package;
mod screen;
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
    /// Start fullscreen (F11 toggles).
    #[arg(long)]
    fullscreen: bool,
    /// The shape of the area the game may use: native, 4:3, 16:9, 16:10, 21:9 or 32:9 (F10 cycles).
    /// The game is scaled by whole steps inside it; the rest is a blurred copy of the background.
    #[arg(long, value_name = "RATIO", default_value = "native")]
    ratio: String,
    /// Windowed size, WIDTHxHEIGHT (F9 cycles the presets: 800x608, 1280x720, 1600x1216, 1920x1080,
    /// 2560x1440, 3200x2432).
    #[arg(long, value_name = "WxH", value_parser = screen::parse_size)]
    window: Option<(u32, u32)>,
    /// Save a screenshot of the window to this PNG after a few frames and exit (for checks).
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let ratio = match screen::parse_ratio(&args.ratio) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("--ratio {}: {e}", args.ratio);
            return ExitCode::FAILURE;
        }
    };
    screen::configure(screen::Options {
        fullscreen: args.fullscreen,
        ratio,
        window: args.window,
    });
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
