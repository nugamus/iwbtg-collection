//! `iwbtg replay run`: ticks, splits, final hash (docs/plans/sim-core.md 9.4). WP0 declares the
//! entry points; WP8b owns the file. WP8a's `main.rs` adds the `replay run` subcommand with
//! [`ReplayRunArgs`] and calls [`main`]; everything else about the command lives here.
#![allow(unused_variables, reason = "WP0 stub: WP8b fills the bodies")]

use std::path::PathBuf;

use iwbtg_format::PackageFiles;
use iwbtg_sim::SimOptions;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::timing::Split;

use crate::trace_cmd::ToolError;

/// Arguments of `iwbtg replay run <pkg> <replay|.inputs> [--start new-game --difficulty N]`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct ReplayRunArgs {
    /// Package folder.
    pub pkg: PathBuf,
    /// A `.iwbr` replay or an action-named `.inputs` file.
    pub input: PathBuf,
    /// Where the run starts (`new-game`).
    #[arg(long, default_value = "new-game")]
    pub start: String,
    /// Difficulty for `--start new-game`.
    #[arg(long, default_value_t = 0)]
    pub difficulty: u8,
}

/// What a headless run gives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplaySummary {
    /// Ticks run.
    pub ticks: u64,
    /// Splits from the `[timing]` triggers.
    pub splits: Vec<Split>,
    /// The final state hash.
    pub hash: [u8; 32],
}

/// `iwbtg replay run`: reads the package and the input, runs [`run`] and prints `ticks: N`,
/// `splits: ...` and `hash: <hex>`.
pub fn main(args: &ReplayRunArgs) -> Result<(), ToolError> {
    unimplemented!("WP8b: replay_cmd::main")
}

/// Runs a package headless on these input frames, one per tick, from `options`.
pub fn run(
    pkg: &PackageFiles,
    frames: &[InputFrame],
    options: SimOptions,
) -> Result<ReplaySummary, ToolError> {
    unimplemented!("WP8b: replay_cmd::run")
}
