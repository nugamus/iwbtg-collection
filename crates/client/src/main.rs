//! `iwbtg-client <package>`: opens a game package in a window (docs/architecture.md, Crates).
//!
//! The sim ticks at the game's rate on a fixed clock, whatever the frame rate; every visible
//! instance with a sprite is drawn where the sim last put it.

mod bindings;
mod art;
mod audio;
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
    /// Blend frames between the last two ticks for smoother motion on fast displays. Off by
    /// default: the originals draw every step, so a blended frame shows positions the game never had.
    #[arg(long)]
    interpolate: bool,
    /// Hold the sound back by this many milliseconds (0 to 500) to line it up with the picture on
    /// a slow audio device. It moves playback only, never the sim.
    #[arg(long, value_name = "MS", default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=500))]
    audio_offset: u32,
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
    let Some(package) = &args.package else {
        if args.bind.is_empty() {
            eprintln!("give a package folder to play, or --bind / --show-bindings");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    };
    match package::open(package, args.trace_mode) {
        Ok(pkg) => match view::run(
            pkg,
            args.fps,
            args.interpolate,
            args.audio_offset,
            args.screenshot,
            bindings,
        ) {
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
