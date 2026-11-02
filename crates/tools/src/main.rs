//! `iwbtg`: the command-line tool for agents and CI (docs/architecture.md, The iwbtg CLI).
//!
//! Commands are planned in docs/plans/sim-core.md 9.4: WP8a owns `trace` and `pkg`, WP8b
//! `replay`. WP7a wired `pkg check` early for its user check; WP8a owns the command line.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

// WP0 scaffolding: the command modules exist so their owners only fill them in.
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod case;
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod import;
mod pkg_cmd;
#[allow(dead_code, reason = "WP0 scaffold: WP8b wires the command")]
mod replay_cmd;
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod trace_cmd;

/// The IWBTG Collection command-line tool.
#[derive(Parser)]
#[command(name = "iwbtg", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Package commands.
    #[command(subcommand)]
    Pkg(PkgCommand),
}

#[derive(Subcommand)]
enum PkgCommand {
    /// Load and check a package; prints diagnostics, then OK or the error count.
    Check(pkg_cmd::PkgCheckArgs),
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Pkg(PkgCommand::Check(args)) => pkg_cmd::main(&args),
    }
}
