//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.

mod art;
mod audio;
mod input;
mod package;
mod screen;
mod settings;
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
    /// Hold the sound back by this many milliseconds (0 to 500) to line it up with the picture on
    /// a slow audio device. It moves playback only, never the sim. Saved in the settings file.
    #[arg(long, value_name = "MS", value_parser = clap::value_parser!(u32).range(0..=500))]
    audio_offset: Option<u32>,
    /// Start fullscreen (F11 toggles). Saved in the settings file, like `--ratio` and `--window`.
    #[arg(long)]
    fullscreen: bool,
    /// The shape of the area the game may use: native, 4:3, 16:9, 16:10, 21:9 or 32:9 (F10 cycles).
    /// The game is scaled by whole steps inside it; the rest is a blurred copy of the background.
    #[arg(long, value_name = "RATIO")]
    ratio: Option<String>,
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
    // The settings file gives the defaults; a flag on the command line overrides its entry and
    // is saved, so the next start without the flag keeps it.
    settings::load();
    let ratio = match args.ratio.as_deref().map(screen::parse_ratio) {
        Some(Ok(r)) => Some(r),
        Some(Err(e)) => {
            eprintln!("--ratio {}: {e}", args.ratio.as_deref().unwrap_or_default());
            return ExitCode::FAILURE;
        }
        None => None,
    };
    settings::update(|s| {
        s.fullscreen |= args.fullscreen;
        if let Some(r) = ratio {
            s.ratio = r;
        }
        if args.window.is_some() {
            s.window = args.window;
        }
        if let Some(ms) = args.audio_offset {
            s.audio_offset_ms = ms;
        }
    });
    let saved = settings::get();
    screen::configure(screen::Options {
        fullscreen: saved.fullscreen,
        ratio: saved.ratio,
        window: saved.window,
    });
    match package::open(&args.package, args.trace_mode) {
        Ok(pkg) => match view::run(
            pkg,
            args.fps,
            args.interpolate,
            saved.audio_offset_ms,
            args.screenshot,
        ) {
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
