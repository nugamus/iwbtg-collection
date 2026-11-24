//! `iwbtg`: the command-line tool for agents and CI (docs/architecture/cli.md, The iwbtg CLI).
//!
//! Commands are described in docs/architecture/cli.md.

mod assets_cmd;
mod baseline;
mod case;
mod determinism_cmd;
mod engine_cmd;
mod feel_cmd;
mod import;
mod pack_cmd;
mod profile_cmd;
mod publish_cmd;
mod replay_cmd;
mod room_cmd;
mod script_cmd;
mod sweep_cmd;
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
    /// Feel contracts: run one, or sweep a directory against a baseline.
    #[command(subcommand)]
    Feel(FeelCommand),
    /// Profiles a replay: script budget, instances and ms per tick.
    Profile(profile_cmd::ProfileArgs),
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
    /// Assets across the games repo.
    #[command(subcommand)]
    Assets(AssetsCommand),
}

#[derive(Subcommand)]
enum AssetsCommand {
    /// Writes the global asset index keyed by content hash (docs/content.md, Assets).
    Index {
        /// Games repo folder: one package folder per game, each with game.toml.
        dir: PathBuf,
        /// Write the index here instead of `<dir>/assets-index.toml`.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Change nothing: exit 1 when the index is missing or would change.
        #[arg(long)]
        check: bool,
    },
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
    /// Runs every case on every package folder (default `<research>/build/packages`) and prints a
    /// game x status matrix; with `--baseline`, exits 1 when a case got worse (#785).
    Sweep {
        /// Folder of package folders (default: `<research>/build/packages`).
        #[arg(long)]
        packages: Option<PathBuf>,
        /// Research repo (default: IWBTG_RESEARCH).
        #[arg(long)]
        research: Option<PathBuf>,
        /// Cases folder (default: IWBTG_CASES, else `crates/tools/cases` of the checkout around the current folder).
        #[arg(long)]
        cases: Option<PathBuf>,
        /// Baseline file of `<game>/<case> <STATUS>` lines to compare with.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Rewrite the baseline with this run's statuses instead of comparing.
        #[arg(long, requires = "baseline")]
        bless: bool,
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
enum FeelCommand {
    /// Runs one contract and prints its verdict and every mismatch.
    Run {
        /// Contract file.
        contract: PathBuf,
        /// The games repo: package folders for contracts that name a package id, looked up first.
        #[arg(long)]
        packages: Option<PathBuf>,
        /// Converter output (`$IWBTG_RESEARCH/build/packages`), looked up when the games repo
        /// does not have the package; loaded like the trace sweep (unmapped kinds inert).
        #[arg(long)]
        converter_packages: Option<PathBuf>,
    },
    /// Runs every contract below a folder and compares the statuses with a baseline.
    Sweep {
        /// Contracts root: `*.toml` files, each `id` equal to its path below the root.
        #[arg(long)]
        contracts: PathBuf,
        /// The games repo: package folders for contracts that name a package id, looked up first.
        #[arg(long)]
        packages: Option<PathBuf>,
        /// Converter output (`$IWBTG_RESEARCH/build/packages`), looked up when the games repo
        /// does not have the package; loaded like the trace sweep (unmapped kinds inert). A
        /// package in neither source is SKIP.
        #[arg(long)]
        converter_packages: Option<PathBuf>,
        /// Baseline file of `<area>/<name> <STATUS>` lines to compare with.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Rewrite the baseline with this run's statuses instead of comparing.
        #[arg(long, requires = "baseline")]
        bless: bool,
    },
}

#[derive(Subcommand)]
enum PkgCommand {
    /// Validates and loads a package.
    Check {
        /// Package folder or `.iwbz` file.
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
    /// Rewrites a package folder to the current format (docs/format/v1.md); the game plays the same.
    Upgrade {
        /// Package folder.
        pkg: PathBuf,
        /// Change nothing: list the files that would change and exit 1 if there are any.
        #[arg(long)]
        check: bool,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
    },
    /// Checks a package and writes it as one `.iwbz` file (docs/format/iwbz.md).
    Pack {
        /// Package folder.
        pkg: PathBuf,
        /// The file to write; default `<id>.iwbz` in the current folder.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
    },
    /// Unpacks an `.iwbz` file into a new or empty folder.
    Unpack {
        /// The `.iwbz` file.
        file: PathBuf,
        /// The folder to write.
        dir: PathBuf,
    },
    /// Packs a package, writes `<content_hash>.iwbz` and updates `catalog.json` (#806).
    Publish {
        /// Package folder or `.iwbz` file.
        pkg: PathBuf,
        /// Folder holding `catalog.json` and `files/`, as the server's `--catalog-dir` reads it.
        #[arg(long)]
        catalog_dir: PathBuf,
        /// Load converter output: unmapped script kinds become inert.
        #[arg(long)]
        trace_mode: bool,
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
    /// Checks a replay as the server does: same package and sim, and a headless run reproduces
    /// its ticks, splits, ranked time, checkpoints and final hash.
    Verify(replay_cmd::ReplayVerifyArgs),
    /// Prints a replay as text: header, splits, checkpoints and the input runs.
    Dump(replay_cmd::ReplayDumpArgs),
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
pub(crate) fn cases_dir(arg: Option<PathBuf>) -> Result<PathBuf, ToolError> {
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

/// Runs one of the package commands of `iwbtg_format::pkg_cmd` (data lane, #1120) on the package
/// folder or `.iwbz` at `dir`, read with its engine pack bundled, writing to stdout.
fn pkg_cmd(
    dir: &Path,
    command: impl FnOnce(&iwbtg_format::PackageFiles, &mut std::io::StdoutLock<'static>) -> std::io::Result<bool>,
) -> Result<bool, ToolError> {
    let files = trace_cmd::load_package(dir)?;
    command(&files, &mut std::io::stdout().lock()).map_err(|e| ToolError::Message(e.to_string()))
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
        Command::Trace(TraceCommand::Sweep {
            packages,
            research,
            cases,
            baseline,
            bless,
        }) => {
            let research = research_dir(research)?;
            let cases = cases_dir(cases)?;
            let packages = packages.unwrap_or_else(|| research.join("build/packages"));
            sweep_cmd::main(&packages, &research, &cases, baseline.as_deref(), bless)
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
        Command::Pkg(PkgCommand::Check { pkg, trace_mode }) => pkg_cmd(&pkg, |files, out| {
            iwbtg_format::pkg_cmd::check(files, trace_mode, out)
        }),
        Command::Pkg(PkgCommand::Fmt { pkg, check }) => pkg_cmd(&pkg, |files, out| {
            iwbtg_format::pkg_cmd::fmt(&pkg, files, check, out)
        }),
        Command::Pkg(PkgCommand::Upgrade {
            pkg,
            check,
            trace_mode,
        }) => {
            if pack_cmd::is_iwbz(&pkg) {
                return Err(ToolError::Message(format!(
                    "{}: upgrade the package folder, then pack it again",
                    pkg.display()
                )));
            }
            pkg_cmd(&pkg, |files, out| {
                iwbtg_format::pkg_cmd::upgrade(&pkg, files, check, trace_mode, out)
            })
        }
        Command::Pkg(PkgCommand::Hash { pkg, trace_mode }) => pkg_cmd(&pkg, |files, out| {
            iwbtg_format::pkg_cmd::hash(files, trace_mode, out)
        }),
        Command::Pkg(PkgCommand::Pack { pkg, out, trace_mode }) => {
            pack_cmd::pack_main(&pkg, out.as_deref(), trace_mode)
        }
        Command::Pkg(PkgCommand::Unpack { file, dir }) => pack_cmd::unpack_main(&file, &dir),
        Command::Pkg(PkgCommand::Publish {
            pkg,
            catalog_dir,
            trace_mode,
        }) => publish_cmd::publish_main(&pkg, &catalog_dir, trace_mode),
        Command::Script(ScriptCommand::ApiDocs { docs, check }) => {
            let docs = docs.unwrap_or_else(script_cmd::default_docs_dir);
            script_cmd::main(&docs, check)
        }
        Command::Assets(AssetsCommand::Index { dir, out, check }) => {
            assets_cmd::index_main(&dir, out.as_deref(), check)
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
        Command::Profile(args) => {
            profile_cmd::main(&args)?;
            Ok(true)
        }
        Command::Feel(FeelCommand::Run {
            contract,
            packages,
            converter_packages,
        }) => feel_cmd::run_main(
            &contract,
            feel_cmd::Sources {
                games: packages.as_deref(),
                converter: converter_packages.as_deref(),
            },
        ),
        Command::Feel(FeelCommand::Sweep {
            contracts,
            packages,
            converter_packages,
            baseline,
            bless,
        }) => feel_cmd::sweep_main(
            &contracts,
            feel_cmd::Sources {
                games: packages.as_deref(),
                converter: converter_packages.as_deref(),
            },
            baseline.as_deref(),
            bless,
        ),
        Command::Replay(ReplayCommand::Run(args)) => {
            replay_cmd::main(&args)?;
            Ok(true)
        }
        Command::Replay(ReplayCommand::Verify(args)) => replay_cmd::verify_main(&args),
        Command::Replay(ReplayCommand::Dump(args)) => {
            replay_cmd::dump_main(&args)?;
            Ok(true)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
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
