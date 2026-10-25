//! `iwbtg`: the command-line tool for agents and CI (docs/architecture.md, The iwbtg CLI).
//!
//! Commands are planned in docs/plans/sim-core.md 9.4: WP8a owns `trace` and `pkg`, WP8b
//! `replay`.

// WP0 scaffolding: the command modules exist so their owners only fill them in. Nothing calls
// them until WP8a wires up the command line.
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod case;
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod import;
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod pkg_cmd;
#[allow(dead_code, reason = "WP0 scaffold: WP8b wires the command")]
mod replay_cmd;
#[allow(dead_code, reason = "WP0 scaffold: WP8a wires the commands")]
mod trace_cmd;

fn main() {
    println!("iwbtg {}", env!("CARGO_PKG_VERSION"));
}
