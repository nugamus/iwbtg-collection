//! `iwbtg engine check`: is a package safe to move to a new engine revision? (docs/content.md,
//! Engines: "Upgrades: automatic when provably safe".)
//!
//! Re-runs every stored replay (`tests/replays/*.iwbr` in the package) and every trace case of
//! the package on the package's own `engine_rev` and on the new one, and compares:
//!
//! - a replay is the same when tick count, splits and final state hash are equal;
//! - a trace case is the same when our trace under the new revision equals ours under the old
//!   one (so a case that already differed from the original does not block, and one that
//!   matched keeps matching).
//!
//! Output is one line per replay and case, then `SAFE`, or `NOT SAFE: <n> difference(s)` after
//! the differences. A package with neither replays nor trace cases proves nothing, so it is not
//! safe either (`NOT PROVEN`). Exit 0 for SAFE, 1 otherwise, 2 when something could not run.

use std::path::{Path, PathBuf};

use iwbtg_format::{LoadMode, PackageFiles};
use iwbtg_sim::SimOptions;
use iwbtg_sim::rules::MAX_ENGINE_REV;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::trace;

use crate::replay_cmd::{ReplaySummary, RunInput, load_strict, replay_input, run_def};
use crate::trace_cmd::{ToolError, package_id, read_case, run_case_rev};

/// Where stored replays live in a package (docs/format/v0.md).
const REPLAYS_DIR: &str = "tests/replays/";

/// The result of one check.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Outcome {
    /// The package's own revision.
    pub from: u16,
    /// The revision tested.
    pub to: u16,
    /// One line per replay and trace case.
    pub lines: Vec<String>,
    /// Replays and cases that were run.
    pub checked: usize,
    /// Of those, how many gave a different result.
    pub differences: usize,
}

impl Outcome {
    /// True when something was checked and nothing differed.
    pub fn is_safe(&self) -> bool {
        self.checked > 0 && self.differences == 0
    }

    /// The report text.
    pub fn report(&self) -> String {
        let mut out = format!("engine_rev {} -> {}\n", self.from, self.to);
        for l in &self.lines {
            out.push_str(l);
            out.push('\n');
        }
        if self.is_safe() {
            out.push_str("SAFE\n");
        } else if self.checked == 0 {
            out.push_str("NOT PROVEN: the package has no stored replays and no trace cases to re-run\n");
        } else {
            out.push_str(&format!("NOT SAFE: {} difference(s)\n", self.differences));
        }
        out
    }
}

/// Where trace cases are read from.
#[derive(Clone, Copy, Debug)]
pub struct TraceSource<'a> {
    /// The research repo (original traces).
    pub research: Option<&'a Path>,
    /// The cases folder (`<cases>/<package id>/*.toml`).
    pub cases: &'a Path,
}

fn hex(h: &[u8]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn short(s: &ReplaySummary) -> String {
    format!("{} ticks, hash {}", s.ticks, &hex(&s.hash)[..16])
}

/// One replay on one revision.
fn run_replay(
    files: &PackageFiles,
    mode: LoadMode,
    rev: u16,
    input: &RunInput,
) -> Result<ReplaySummary, ToolError> {
    let mut def = load_strict(files, mode)?;
    def.engine_rev = rev;
    let options = SimOptions {
        start: input.start(0),
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map.clone(),
    };
    run_def(def, &input.frames, options, input.mods)
}

fn check_replay(
    files: &PackageFiles,
    mode: LoadMode,
    path: &str,
    bytes: &[u8],
    from: u16,
    to: u16,
) -> (String, bool) {
    let input = match replay_input(bytes, path) {
        Ok(x) => x,
        Err(e) => return (format!("replay {path}: DIFF cannot run: {e}"), true),
    };
    let old = run_replay(files, mode, from, &input);
    let new = run_replay(files, mode, to, &input);
    match (old, new) {
        (Ok(a), Ok(b)) if a == b => (format!("replay {path}: same ({})", short(&a)), false),
        (Ok(a), Ok(b)) => {
            let mut why = Vec::new();
            if a.ticks != b.ticks {
                why.push(format!("ticks {} -> {}", a.ticks, b.ticks));
            }
            if a.splits != b.splits {
                why.push("splits differ".to_owned());
            }
            if a.hash != b.hash {
                why.push(format!("hash {} -> {}", &hex(&a.hash)[..16], &hex(&b.hash)[..16]));
            }
            (format!("replay {path}: DIFF {}", why.join(", ")), true)
        }
        (Err(e), _) => (
            format!("replay {path}: DIFF does not run on rev {from}: {e}"),
            true,
        ),
        (_, Err(e)) => (format!("replay {path}: DIFF does not run on rev {to}: {e}"), true),
    }
}

/// Case files of the package under `<cases>/<package id>/`, in name order.
fn case_paths(cases: &Path, id: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(cases.join(id))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

/// Tests the package on engine revision `to`. `traces` is where trace cases come from; when the
/// package has cases but no research repo is given, that is an error (they cannot be run, so
/// SAFE would be a claim without proof).
pub fn check(
    files: &PackageFiles,
    to: u16,
    trace_mode: bool,
    traces: Option<TraceSource<'_>>,
) -> Result<Outcome, ToolError> {
    if to > MAX_ENGINE_REV {
        return Err(ToolError::Message(format!(
            "engine_rev {to} is not known to this build (up to {MAX_ENGINE_REV})"
        )));
    }
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    let from = load_strict(files, mode)?.engine_rev;
    let mut out = Outcome {
        from,
        to,
        lines: Vec::new(),
        checked: 0,
        differences: 0,
    };
    for (path, bytes) in files.iter() {
        if !(path.starts_with(REPLAYS_DIR) && path.ends_with(".iwbr")) {
            continue;
        }
        let (line, differs) = check_replay(files, mode, path, bytes, from, to);
        out.lines.push(line);
        out.checked += 1;
        out.differences += usize::from(differs);
    }
    if let Some(src) = traces {
        let id = package_id(files)?;
        let paths = case_paths(src.cases, &id);
        if !paths.is_empty() && src.research.is_none() {
            return Err(ToolError::Message(format!(
                "the package has {} trace case(s): pass --research <dir> or set IWBTG_RESEARCH",
                paths.len()
            )));
        }
        for path in paths {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let case = read_case(&path)?;
            let Some(research) = src.research else { continue };
            let old = run_case_rev(files, &case, research, Some(from));
            let new = run_case_rev(files, &case, research, Some(to));
            match (old, new) {
                (Err(ToolError::Open(why)), _) | (_, Err(ToolError::Open(why))) => {
                    out.lines.push(format!("trace {name}: skipped (OPEN: {why})"));
                }
                (Ok(a), Ok(b)) => {
                    let d = trace::diff(
                        &a,
                        &b,
                        &crate::trace_cmd::case_diff_options(&case, trace::DEFAULT_CONTEXT),
                    )
                    .map_err(|e| ToolError::Message(e.to_string()))?;
                    out.checked += 1;
                    if d.is_clean() {
                        out.lines.push(format!("trace {name}: same"));
                    } else {
                        out.differences += 1;
                        out.lines
                            .push(format!("trace {name}: DIFF\n{d}").trim_end().to_owned());
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    out.checked += 1;
                    out.differences += 1;
                    out.lines.push(format!("trace {name}: DIFF cannot run: {e}"));
                }
            }
        }
    }
    Ok(out)
}

/// `iwbtg engine check <pkg> [--to N]`: prints the report; `Ok(false)` unless SAFE.
pub fn main(
    pkg: &Path,
    to: Option<u16>,
    trace_mode: bool,
    traces: Option<TraceSource<'_>>,
) -> Result<bool, ToolError> {
    let files = crate::trace_cmd::load_package(pkg)?;
    let outcome = check(&files, to.unwrap_or(MAX_ENGINE_REV), trace_mode, traces)?;
    print!("{}", outcome.report());
    Ok(outcome.is_safe())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden() -> PackageFiles {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden/one_room.pkg");
        PackageFiles::load_dir(dir).unwrap()
    }

    #[test]
    fn a_revision_this_build_does_not_know_is_an_error() {
        let e = check(&golden(), MAX_ENGINE_REV + 1, false, None).unwrap_err();
        assert!(e.to_string().contains("not known"), "{e}");
    }

    #[test]
    fn a_package_with_nothing_to_re_run_is_not_proven() {
        let outcome = check(&golden(), MAX_ENGINE_REV, false, None).unwrap();
        assert_eq!(outcome.checked, 0);
        assert!(!outcome.is_safe());
        assert!(outcome.report().contains("NOT PROVEN"));
    }

    #[test]
    fn same_results_are_safe_and_a_changed_hash_is_not() {
        let ok = Outcome {
            from: 0,
            to: 1,
            lines: vec!["replay tests/replays/a.iwbr: same (1 ticks, hash ab)".into()],
            checked: 1,
            differences: 0,
        };
        assert!(ok.is_safe());
        assert!(ok.report().ends_with("SAFE\n"));
        let bad = Outcome { differences: 1, ..ok };
        assert!(!bad.is_safe());
        assert!(bad.report().contains("NOT SAFE: 1 difference(s)"));
    }

    /// A stored replay of the golden package: recorded here from its `.inputs`, then checked.
    #[test]
    fn a_stored_replay_of_the_golden_package_is_safe_on_the_same_revision() {
        use iwbtg_sim::input::parse_action_inputs;
        use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Replay, StartMode, Trailer};
        let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden");
        let text = std::fs::read_to_string(golden_dir.join("one_room.inputs")).unwrap();
        let parsed = parse_action_inputs(&text).unwrap();
        let end = usize::try_from(parsed.end).unwrap() + 1;
        let mut frames = parsed.frames;
        frames.truncate(end);
        let mut files = golden();
        let replay = Replay {
            header: Header {
                magic: MAGIC,
                replay_format: REPLAY_FORMAT,
                sim_version: iwbtg_sim::SIM_VERSION,
                package_id: "golden".into(),
                gameplay_hash: [0; 32],
                engine_rev: 0,
                rng_seed: 0,
                start_mode: StartMode::NewGame,
                play_mode: iwbtg_sim::stripped::PlayMode::Original,
                map: "main".into(),
                mods: iwbtg_sim::mods::ModSet::NONE,
                client_build_id: String::new(),
            },
            trailer: Trailer {
                total_ticks: frames.len() as u64,
                ..Trailer::default()
            },
            inputs: frames,
        };
        let bytes = replay.encode().unwrap();
        files.insert("tests/replays/one_room.iwbr", bytes).unwrap();
        let attempt = std::panic::catch_unwind(|| check(&files, MAX_ENGINE_REV, false, None));
        match attempt {
            Ok(outcome) => {
                let outcome = outcome.unwrap();
                assert!(outcome.is_safe(), "{}", outcome.report());
                assert!(
                    outcome
                        .report()
                        .contains("replay tests/replays/one_room.iwbr: same")
                );
            }
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                assert!(msg.starts_with("not implemented: WP"), "unexpected panic: {msg}");
            }
        }
    }
}
