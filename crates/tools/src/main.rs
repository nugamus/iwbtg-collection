//! `iwbtg`: the command-line tool for agents and CI (docs/architecture.md, The iwbtg CLI).
//!
//! Commands are planned in docs/plans/sim-core.md 9.4: WP8a owns `trace` and `pkg`, WP8b
//! `replay`.

mod case;
mod determinism_cmd;
mod engine_cmd;
mod import;
mod pkg_cmd;
mod replay_cmd;
mod room_cmd;
mod script_cmd;
mod trace_cmd;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::trace_cmd::ToolError;

/// Where the cases folder is looked for when `--cases` is not given: `crates/tools/cases` in the
/// current folder or the nearest folder above it (so any folder of a checkout works).
const CASES_IN_CHECKOUT: &str = "crates/tools/cases";

#[derive(Parser)]
#[command(
    name = "iwbtg",
    version,
    about = "IWBTG Collection tools: packages, traces, replays"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Research trace cases: run, compare, import.
    #[command(subcommand)]
    Trace(TraceCommand),
    /// Game packages.
    #[command(subcommand)]
    Pkg(PkgCommand),
    /// Headless replays.
    #[command(subcommand)]
    Replay(ReplayCommand),
    /// Rooms of a package.
    #[command(subcommand)]
    Room(RoomCommand),
    /// Cross-platform equality checks.
    #[command(subcommand)]
    Determinism(DeterminismCommand),
    /// Engine revisions.
    #[command(subcommand)]
    Engine(EngineCommand),
    /// The script language.
    #[command(subcommand)]
    Script(ScriptCommand),
}

#[derive(Subcommand)]
enum ScriptCommand {
    /// Generates docs/format/script-api.md and the Rhai stubs from the registered API.
    ApiDocs {
        /// The docs folder to write to (default: this checkout's `docs/`).
        #[arg(long)]
        docs: Option<PathBuf>,
        /// Change nothing: exit 1 when a generated file differs from the committed one.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
enum EngineCommand {
    /// Re-runs a package's stored replays and trace cases on a new engine_rev; prints SAFE or the differences.
    Check {
        /// Package folder.
        pkg: PathBuf,
        /// The revision to test (default: the newest this build knows).
        #[arg(long)]
        to: Option<u16>,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
        /// Research repo for the original traces (default: IWBTG_RESEARCH); without one, trace cases are not run.
        #[arg(long)]
        research: Option<PathBuf>,
        /// Cases folder (default: IWBTG_CASES, else `crates/tools/cases` of the checkout around the current folder).
        #[arg(long)]
        cases: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TraceCommand {
    /// Runs one case and prints our trace CSV.
    Run {
        /// Package folder.
        pkg: PathBuf,
        /// Case file, or case name under the cases folder.
        case: String,
        /// Write the CSV here instead of printing it.
        #[arg(short = 'o', long = "out")]
        out: Option<PathBuf>,
        /// Sample after every tick phase (not available yet).
        #[arg(long)]
        stages: bool,
        /// Research repo (default: IWBTG_RESEARCH).
        #[arg(long)]
        research: Option<PathBuf>,
        /// Cases folder (default: IWBTG_CASES, else `crates/tools/cases` of the checkout around the current folder).
        #[arg(long)]
        cases: Option<PathBuf>,
    },
    /// Compares two trace CSVs bit for bit.
    Diff {
        /// The original trace.
        original: PathBuf,
        /// Our trace.
        ours: PathBuf,
        /// Columns not compared.
        #[arg(long, default_values_t = ["id".to_string(), "sprite_index".to_string()])]
        ignore: Vec<String>,
        /// Frames of context around the first difference.
        #[arg(long, default_value_t = iwbtg_sim::trace::DEFAULT_CONTEXT)]
        context: u32,
        /// List every differing field.
        #[arg(long)]
        summary: bool,
    },
    /// Runs every case of a package; fails on any difference.
    Check {
        /// Package folder.
        pkg: PathBuf,
        /// Research repo (default: IWBTG_RESEARCH).
        #[arg(long)]
        research: Option<PathBuf>,
        /// Cases folder (default: IWBTG_CASES, else `crates/tools/cases` of the checkout around the current folder).
        #[arg(long)]
        cases: Option<PathBuf>,
    },
    /// Writes a case skeleton for a research trace and prints its kind.
    Import {
        /// Research trace path, with or without `.csv`/`.inputs`.
        trace: PathBuf,
        /// Package id, or package folder.
        pkg: String,
        /// Research repo (default: found above the trace path).
        #[arg(long)]
        research: Option<PathBuf>,
        /// Cases folder (default: IWBTG_CASES, else `crates/tools/cases` of the checkout around the current folder).
        #[arg(long)]
        cases: Option<PathBuf>,
        /// Overwrite an existing case file.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum PkgCommand {
    /// Validates and loads a package.
    Check {
        /// Package folder.
        pkg: PathBuf,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
    },
    /// Formats every data file of a package in the canonical layout (docs/decisions/0002).
    Fmt {
        /// Package folder.
        pkg: PathBuf,
        /// Change nothing: list the files that would change and exit 1 if there are any.
        #[arg(long)]
        check: bool,
    },
    /// Prints the gameplay hash (leaderboards key on it) and the content hash (downloads).
    Hash {
        /// Package folder.
        pkg: PathBuf,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
    },
}

#[derive(Subcommand)]
enum RoomCommand {
    /// Prints an ASCII map of a room with a legend line.
    Show {
        /// Package folder.
        pkg: PathBuf,
        /// Room id (`rooms/<id>.toml`).
        room: String,
        /// Pixels per character.
        #[arg(long, default_value_t = 32)]
        cell: u32,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
    },
}

#[derive(Subcommand)]
enum DeterminismCommand {
    /// Runs the golden corpus and prints one line per case: name, ticks, status, state hash.
    Report {
        /// The golden folder (crates/sim/tests/golden): holds `expected.txt`.
        corpus: PathBuf,
        /// Also compare with `<corpus>/expected.txt`; exit 1 on a difference.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
enum ReplayCommand {
    /// Runs a replay or `.inputs` file headless and prints ticks, splits and the final hash.
    Run(replay_cmd::ReplayRunArgs),
}

/// `--research`, else IWBTG_RESEARCH.
fn research_dir(arg: Option<PathBuf>) -> Result<PathBuf, ToolError> {
    arg.or_else(|| std::env::var_os("IWBTG_RESEARCH").map(PathBuf::from))
        .ok_or_else(|| {
            ToolError::Message("no research repo: pass --research <dir> or set IWBTG_RESEARCH".into())
        })
}

/// The cases folder: `--cases`, else IWBTG_CASES, else `crates/tools/cases` in the current folder
/// or the nearest folder above it. Decided at run time, never from the build machine's paths, so
/// a moved checkout and a wasm32 build (with the folder preopened) work.
fn cases_dir(arg: Option<PathBuf>) -> Result<PathBuf, ToolError> {
    let env = std::env::var_os("IWBTG_CASES").map(PathBuf::from);
    let cwd = std::env::current_dir().ok();
    resolve_cases(arg, env, cwd.as_deref())
}

fn resolve_cases(
    arg: Option<PathBuf>,
    env: Option<PathBuf>,
    cwd: Option<&Path>,
) -> Result<PathBuf, ToolError> {
    if let Some(dir) = arg.or(env) {
        return Ok(dir);
    }
    if let Some(cwd) = cwd {
        for dir in cwd.ancestors() {
            let candidate = dir.join(CASES_IN_CHECKOUT);
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    Err(ToolError::Message(format!(
        "no cases folder: pass --cases <dir>, set IWBTG_CASES, or run inside a checkout that has {CASES_IN_CHECKOUT}"
    )))
}

/// Runs a command; `Ok(false)` means it ran and found problems.
fn run(cli: Cli) -> Result<bool, ToolError> {
    match cli.command {
        Command::Trace(TraceCommand::Run {
            pkg,
            case,
            out,
            stages,
            research,
            cases,
        }) => {
            let research = research_dir(research)?;
            let cases = cases_dir(cases)?;
            trace_cmd::run_main(&pkg, &case, out.as_deref(), stages, &research, &cases)?;
            Ok(true)
        }
        Command::Trace(TraceCommand::Diff {
            original,
            ours,
            ignore,
            context,
            summary,
        }) => trace_cmd::diff_main(&original, &ours, &ignore, context, summary),
        Command::Trace(TraceCommand::Check { pkg, research, cases }) => {
            let research = research_dir(research)?;
            let cases = cases_dir(cases)?;
            trace_cmd::check_main(&pkg, &research, &cases)
        }
        Command::Trace(TraceCommand::Import {
            trace,
            pkg,
            research,
            cases,
            force,
        }) => {
            let research = research.or_else(|| std::env::var_os("IWBTG_RESEARCH").map(PathBuf::from));
            let cases = cases_dir(cases)?;
            import::main(&trace, &pkg, research.as_deref().map(Path::new), &cases, force)?;
            Ok(true)
        }
        Command::Pkg(PkgCommand::Check { pkg, trace_mode }) => pkg_cmd::main(&pkg, trace_mode),
        Command::Pkg(PkgCommand::Fmt { pkg, check }) => pkg_cmd::fmt_main(&pkg, check),
        Command::Pkg(PkgCommand::Hash { pkg, trace_mode }) => pkg_cmd::hash_main(&pkg, trace_mode),
        Command::Script(ScriptCommand::ApiDocs { docs, check }) => {
            let docs = docs.unwrap_or_else(script_cmd::default_docs_dir);
            script_cmd::main(&docs, check)
        }
        Command::Determinism(DeterminismCommand::Report { corpus, check }) => {
            determinism_cmd::main(&corpus, check)
        }
        Command::Room(RoomCommand::Show {
            pkg,
            room,
            cell,
            trace_mode,
        }) => room_cmd::show_main(&pkg, &room, cell, trace_mode),
        Command::Engine(EngineCommand::Check {
            pkg,
            to,
            trace_mode,
            research,
            cases,
        }) => {
            let research = research.or_else(|| std::env::var_os("IWBTG_RESEARCH").map(PathBuf::from));
            // Cases only matter when there is a research repo to run them against.
            let cases = match cases_dir(cases) {
                Ok(dir) => dir,
                Err(e) if research.is_some() => return Err(e),
                Err(_) => PathBuf::from(CASES_IN_CHECKOUT),
            };
            let traces = engine_cmd::TraceSource {
                research: research.as_deref(),
                cases: &cases,
            };
            engine_cmd::main(&pkg, to, trace_mode, Some(traces))
        }
        Command::Replay(ReplayCommand::Run(args)) => {
            replay_cmd::main(&args)?;
            Ok(true)
        }
    }
}

/// The work package named by a WP0 stub panic (`not implemented: WP6a: world::init_state`).
fn unmerged_stub(payload: &(dyn std::any::Any + Send)) -> Option<String> {
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())?;
    msg.strip_prefix("not implemented: ")
        .filter(|rest| rest.starts_with("WP"))
        .map(str::to_owned)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Until every work package has merged, running the sim can reach a WP0 stub. Say so plainly
    // instead of ending with a bare panic.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(cli))) {
        Ok(Ok(true)) => ExitCode::SUCCESS,
        Ok(Ok(false)) => ExitCode::from(1),
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
        Err(payload) => match unmerged_stub(payload.as_ref()) {
            Some(stub) => {
                eprintln!("error: this needs {stub}, which is not merged yet");
                ExitCode::from(3)
            }
            None => std::panic::resume_unwind(payload),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-cases-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_flag_beats_the_environment_beats_the_checkout() {
        let root = scratch("order");
        std::fs::create_dir_all(root.join(CASES_IN_CHECKOUT)).unwrap();
        let flag = Some(PathBuf::from("flag"));
        let env = Some(PathBuf::from("env"));
        assert_eq!(
            resolve_cases(flag.clone(), env.clone(), Some(&root)).unwrap(),
            PathBuf::from("flag")
        );
        assert_eq!(
            resolve_cases(None, env, Some(&root)).unwrap(),
            PathBuf::from("env")
        );
        assert_eq!(
            resolve_cases(None, None, Some(&root)).unwrap(),
            root.join(CASES_IN_CHECKOUT)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_moved_checkout_is_found_from_a_folder_below_it() {
        let root = scratch("below");
        std::fs::create_dir_all(root.join(CASES_IN_CHECKOUT)).unwrap();
        let below = root.join("crates/sim/src");
        std::fs::create_dir_all(&below).unwrap();
        assert_eq!(
            resolve_cases(None, None, Some(&below)).unwrap(),
            root.join(CASES_IN_CHECKOUT)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_cases_folder_is_an_error_that_says_what_to_do() {
        let root = scratch("none");
        let Err(ToolError::Message(m)) = resolve_cases(None, None, Some(&root)) else {
            panic!("expected an error");
        };
        assert!(m.contains("--cases") && m.contains("IWBTG_CASES"), "{m}");
        std::fs::remove_dir_all(&root).ok();
    }
}
