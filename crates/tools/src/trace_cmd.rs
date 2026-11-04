//! `iwbtg trace run`, `diff`, `check` (docs/plans/sim-core.md 9.4). WP0 declares; WP8a owns.
//!
//! A case runs like the recorder ran the original (research `tooling/opengmk-trace/README.md`):
//! the run starts in the case's room (`StartPoint::Room`), which stands for the recorder's room
//! jump at frame `at`; setup lines before `at` run before room entry, the others before the tick
//! of their frame (the `--gml N:` code ran "before frame N"); tick k of our run is frame
//! `at + k` and reads that frame's input from the `.inputs` file through the engine key map;
//! after each tick the probe samples (after row 13, before the room change) and frames from
//! `first_frame` on get rows. Frames before `at` ran in the original's boot rooms, which our run
//! does not have, so the comparison starts at `max(at, first_frame)`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
use iwbtg_sim::def::{GameDef, KindId, NameId, RoomId};
use iwbtg_sim::input::{InputFrame, KeyMap, parse_key_inputs};
use iwbtg_sim::num::Real;
use iwbtg_sim::probe::{self, Column, Probe, ProbeTarget, registry};
use iwbtg_sim::rules::tick::DirectLatch;
use iwbtg_sim::setup::SetupAction;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::trace::{self, DiffOptions, TraceDiff, TraceTable};
use iwbtg_sim::world::Value;
use iwbtg_sim::{Sim, SimOptions, SimStatus, StartPoint};

use crate::case::{CaseDllMode, CaseValue, SetupWhat, TraceCase};

/// Why a tools command failed.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Anything, with a message for the user.
    #[error("{0}")]
    Message(String),
    /// The case is marked `open`: it cannot run until the reason is resolved.
    #[error("OPEN: {0}")]
    Open(String),
}

/// `specs/engines/gm8/traces` under the research repo.
pub fn traces_dir(research: &Path) -> PathBuf {
    research.join("specs/engines/gm8/traces")
}

/// Reads a text file, naming it in the error.
pub fn read_text(path: &Path) -> Result<String, ToolError> {
    std::fs::read_to_string(path).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))
}

/// A case resolved against a game definition: everything the run needs, in sim terms.
#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedCase {
    /// Where the run starts.
    pub options: SimOptions,
    /// The traced kind.
    pub object: KindId,
    /// One entry per original variable column: the probe column, or `None` when not compared.
    pub columns: Vec<Option<Column>>,
    /// Setup before room entry (frames before `at`).
    pub pre_entry: Vec<SetupAction>,
    /// Setup before the tick of a frame.
    pub per_frame: BTreeMap<u32, Vec<SetupAction>>,
}

/// Resolves a case's names against `def`. `var_columns` are the original CSV's variable column
/// names (after the 18 fixed ones). Every column or setup line that does not resolve is listed in
/// one error, so a case can be fixed in one pass.
pub fn resolve_case(
    case: &TraceCase,
    def: &GameDef,
    var_columns: &[String],
) -> Result<ResolvedCase, ToolError> {
    let mut errors = Vec::new();
    let room = def
        .rooms
        .iter()
        .position(|r| r.name == case.start.room)
        .and_then(|i| u32::try_from(i).ok())
        .map(RoomId);
    if room.is_none() {
        errors.push(format!("start room {:?} is not in the package", case.start.room));
    }
    let object = kind_named(def, &case.object);
    if object.is_none() {
        errors.push(format!("object {:?} is not a kind of the package", case.object));
    }
    let mut columns = Vec::with_capacity(var_columns.len());
    for name in var_columns {
        if case.ignore.contains(name) || case.unprobed.contains_key(name) {
            columns.push(None);
            continue;
        }
        let expr = case.vars.get(name).map_or(name.as_str(), String::as_str);
        match registry::column(expr, def) {
            Ok(c) => columns.push(Some(c)),
            Err(e) => {
                errors.push(format!("column {name:?}: {e}"));
                columns.push(None);
            }
        }
    }
    let mut pre_entry = Vec::new();
    let mut per_frame: BTreeMap<u32, Vec<SetupAction>> = BTreeMap::new();
    for (i, s) in case.setup.iter().enumerate() {
        let mut actions = Vec::new();
        match &s.what {
            SetupWhat::Fields { target, set } => match setup_target(def, target) {
                Ok(t) => {
                    for (field, v) in set {
                        match registry::resolve(field, def) {
                            Some(f) => actions.push(SetupAction::SetField {
                                target: t.clone(),
                                field: f,
                                value: value(v),
                            }),
                            None => errors.push(format!("setup {}: no field {field:?}", i + 1)),
                        }
                    }
                }
                Err(e) => errors.push(format!("setup {}: {e}", i + 1)),
            },
            SetupWhat::Global { name, value: v } => match def.var_names.get(name) {
                Some(var) => actions.push(SetupAction::SetGlobal {
                    var: *var,
                    value: value(v),
                }),
                None => errors.push(format!(
                    "setup {}: the package declares no global {name:?} (engine state such as the difficulty \
                     is the start point; others are OPEN)",
                    i + 1
                )),
            },
            SetupWhat::Save { field, value: v } => match registry::save_field(field) {
                Some(f) => actions.push(SetupAction::SetSave {
                    field: f,
                    value: value(v),
                }),
                None => errors.push(format!("setup {}: no save field {field:?}", i + 1)),
            },
        }
        if s.frame < case.at {
            pre_entry.extend(actions);
        } else {
            per_frame.entry(s.frame).or_default().extend(actions);
        }
    }
    if !errors.is_empty() {
        return Err(ToolError::Message(format!(
            "case {} does not resolve against the package:\n  {}",
            case.source,
            errors.join("\n  ")
        )));
    }
    let (Some(room), Some(object)) = (room, object) else {
        return Err(ToolError::Message("unreachable: room and object resolved".into()));
    };
    Ok(ResolvedCase {
        options: SimOptions {
            start: StartPoint::Room {
                room,
                difficulty: case.start.difficulty,
            },
            seed: case.seed,
            dll_mode: match case.dll_mode {
                CaseDllMode::Model => DllMode::Model,
                CaseDllMode::Stub => DllMode::Stub,
            },
        },
        object,
        columns,
        pre_entry,
        per_frame,
    })
}

fn kind_named(def: &GameDef, name: &str) -> Option<KindId> {
    def.kinds
        .iter()
        .position(|k| k.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(KindId)
}

/// `kid`, `named:<name>` or a kind name.
fn setup_target(def: &GameDef, target: &str) -> Result<ProbeTarget, String> {
    if target == "kid" {
        return Ok(ProbeTarget::Kid);
    }
    if let Some(name) = target.strip_prefix("named:") {
        return def
            .names
            .iter()
            .position(|n| n == name)
            .and_then(|i| u32::try_from(i).ok())
            .map(|i| ProbeTarget::Named(NameId(i)))
            .ok_or_else(|| format!("no instance named {name:?}"));
    }
    kind_named(def, target)
        .map(|k| ProbeTarget::First(iwbtg_sim::collide::Target::Kind(k)))
        .ok_or_else(|| format!("no kind {target:?}"))
}

fn value(v: &CaseValue) -> Value {
    match v {
        CaseValue::Real(bits) => Value::Real(Real::from_bits(*bits)),
        CaseValue::Str(s) => Value::Str(s.clone()),
    }
}

/// The frames our run ticks: `at` through the last frame of the inputs (or `end_frame`).
pub fn frame_range(case: &TraceCase, inputs_end: u32) -> std::ops::RangeInclusive<u32> {
    let last = case.end_frame.map_or(inputs_end, |e| e.min(inputs_end));
    case.at..=last
}

/// Runs a resolved case on `def` with these input frames and returns our trace, with the
/// original's variable columns (`var_columns`) and line ending.
pub fn execute(
    case: &TraceCase,
    def: Arc<GameDef>,
    resolved: &ResolvedCase,
    frames: &[InputFrame],
    inputs_end: u32,
    var_columns: &[String],
) -> Result<TraceTable, ToolError> {
    let names: Vec<&str> = var_columns.iter().map(String::as_str).collect();
    let mut table = TraceTable::new(&names).map_err(|e| ToolError::Message(e.to_string()))?;
    let probe = Probe {
        object: iwbtg_sim::collide::Target::Kind(resolved.object),
        columns: resolved.columns.iter().flatten().cloned().collect(),
    };
    let mut sim = Sim::new_with_setup(def, resolved.options.clone(), &resolved.pre_entry)
        .map_err(|e| ToolError::Message(format!("{}: sim did not start: {e}", case.source)))?;
    // The recorder has no keyboard_check_direct latch (CLONE_SPEC input, Confirmed (recorder)).
    sim.set_direct_latch(DirectLatch::None);
    for frame in frame_range(case, inputs_end) {
        for s in resolved.per_frame.get(&frame).into_iter().flatten() {
            sim.apply_setup(s)
                .map_err(|e| ToolError::Message(format!("{}: frame {frame}: setup: {e}", case.source)))?;
        }
        let input = usize::try_from(frame)
            .ok()
            .and_then(|f| frames.get(f).copied())
            .unwrap_or(InputFrame::EMPTY);
        // On the tick of a room change the recorder samples before the change (the old room's
        // instances, the kid already moved; kid/B.md Restart timing, #450).
        let room_change = sim.step_rows(input);
        let mut sampled = Vec::new();
        if room_change && frame >= case.first_frame {
            sampled = probe::sample(&mut sim, &probe, i64::from(frame));
        }
        let step = sim.finish_step();
        if let SimStatus::Halted(e) = &step.status {
            return Err(ToolError::Message(format!(
                "{}: frame {frame}: the sim halted: {e}",
                case.source
            )));
        }
        if frame >= case.first_frame {
            if !room_change {
                sampled = probe::sample(&mut sim, &probe, i64::from(frame));
            }
            for row in sampled {
                table
                    .push_row(splice(case, &resolved.columns, row))
                    .map_err(|e| ToolError::Message(e.to_string()))?;
            }
        }
        if step.status == SimStatus::Ended {
            break;
        }
    }
    latch_columns(case, &mut table);
    Ok(table)
}

/// The case's `latch` columns: a row shows the column's value as of the frame before the last
/// read frame at or before it, 0 before the first read (the recorder's `--gml` copied it then).
fn latch_columns(case: &TraceCase, table: &mut TraceTable) {
    for (name, reads) in &case.latch {
        let Some(col) = table.columns.iter().position(|c| &c.name == name) else {
            continue;
        };
        let series: Vec<(i64, trace::Value)> = table
            .rows
            .iter()
            .filter_map(|r| match r.first() {
                Some(trace::Value::Int(f)) => Some((*f, r[col].clone())),
                _ => None,
            })
            .collect();
        for row in &mut table.rows {
            let Some(trace::Value::Int(frame)) = row.first().cloned() else {
                continue;
            };
            let read = reads.iter().copied().filter(|r| i64::from(*r) <= frame).max();
            row[col] = read
                .and_then(|r| {
                    series
                        .iter()
                        .rev()
                        .find(|(f, _)| *f < i64::from(r))
                        .map(|(_, v)| v.clone())
                })
                .unwrap_or(trace::Value::Real(0.0));
        }
    }
}

/// A full row from a probe row: the fixed columns with the original room and object names, then
/// one cell per original variable column (empty where nothing is compared).
fn splice(case: &TraceCase, columns: &[Option<Column>], row: Vec<trace::Value>) -> Vec<trace::Value> {
    let fixed = trace::FIXED_COLUMNS.len();
    let mut values = row.into_iter();
    let mut out: Vec<trace::Value> = values.by_ref().take(fixed).collect();
    if let Some(trace::Value::Str(room)) = out.get_mut(1)
        && let Some(orig) = case.rooms.get(room.as_str())
    {
        *room = orig.clone();
    }
    if let Some(trace::Value::Str(object)) = out.get_mut(2)
        && *object == case.object
    {
        *object = case.original_object.clone();
    }
    for c in columns {
        out.push(match c {
            Some(_) => values.next().unwrap_or(trace::Value::Empty),
            None => trace::Value::Empty,
        });
    }
    out
}

/// The original's variable column names.
fn var_columns(original: &TraceTable) -> Vec<String> {
    original
        .columns
        .iter()
        .skip(trace::FIXED_COLUMNS.len())
        .map(|c| c.name.clone())
        .collect()
}

/// Runs one case against its package and returns our trace table.
pub fn run_case(pkg: &PackageFiles, case: &TraceCase, research: &Path) -> Result<TraceTable, ToolError> {
    run_case_rev(pkg, case, research, None)
}

/// Like [`run_case`], on engine revision `engine_rev` instead of the package's own when given.
pub fn run_case_rev(
    pkg: &PackageFiles,
    case: &TraceCase,
    research: &Path,
    engine_rev: Option<u16>,
) -> Result<TraceTable, ToolError> {
    if let Some(open) = &case.open {
        return Err(ToolError::Open(open.clone()));
    }
    let (original, inputs) = read_trace(case, research)?;
    let parsed = parse_key_inputs(&inputs, &KeyMap::iwbtg())
        .map_err(|e| ToolError::Message(format!("{}.inputs: {e}", case.source)))?;
    let mut def = load_game(
        pkg,
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        },
    )
    .map_err(|diags| {
        ToolError::Message(format!(
            "the package does not load:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        ))
    })?;
    if let Some(rev) = engine_rev {
        def.engine_rev = rev;
    }
    let vars = var_columns(&original);
    let resolved = resolve_case(case, &def, &vars)?;
    let mut ours = execute(case, Arc::new(def), &resolved, &parsed.frames, parsed.end, &vars)?;
    ours.line_ending = original.line_ending;
    Ok(ours)
}

/// The original CSV and `.inputs` text of a case.
fn read_trace(case: &TraceCase, research: &Path) -> Result<(TraceTable, String), ToolError> {
    let base = traces_dir(research).join(&case.source);
    let csv_path = PathBuf::from(format!("{}.csv", base.display()));
    let inputs_path = PathBuf::from(format!("{}.inputs", base.display()));
    let original = TraceTable::parse(&read_text(&csv_path)?)
        .map_err(|e| ToolError::Message(format!("{}: {e}", csv_path.display())))?;
    Ok((original, read_text(&inputs_path)?))
}

/// The original rows a case compares: frames from `max(at, first_frame)` on.
pub fn compared_original(case: &TraceCase, original: &TraceTable) -> TraceTable {
    let start = i64::from(case.at.max(case.first_frame));
    let mut t = original.clone();
    t.rows
        .retain(|row| matches!(row.first(), Some(trace::Value::Int(f)) if *f >= start));
    t
}

/// How a case's two traces are compared: its `ignore` and `unprobed` columns are skipped.
pub fn case_diff_options(case: &TraceCase, context: u32) -> DiffOptions {
    let mut ignore: BTreeSet<String> = case.ignore.iter().cloned().collect();
    ignore.extend(case.unprobed.keys().cloned());
    DiffOptions {
        ignore,
        context,
        ..DiffOptions::default()
    }
}

/// Runs a case and diffs it against the original.
pub fn check_case(pkg: &PackageFiles, case: &TraceCase, research: &Path) -> Result<TraceDiff, ToolError> {
    let ours = run_case(pkg, case, research)?;
    let (original, _) = read_trace(case, research)?;
    let original = compared_original(case, &original);
    trace::diff(&original, &ours, &case_diff_options(case, trace::DEFAULT_CONTEXT))
        .map_err(|e| ToolError::Message(e.to_string()))
}

/// The package id from `game.toml`.
pub fn package_id(files: &PackageFiles) -> Result<String, ToolError> {
    let mut diags = Vec::new();
    match iwbtg_format::check::load_manifest(files, &mut diags) {
        Some((m, _)) => Ok(m.id.get_ref().clone()),
        None => Err(ToolError::Message(format!(
            "game.toml does not read:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        ))),
    }
}

/// Loads a package folder.
pub fn load_package(dir: &Path) -> Result<PackageFiles, ToolError> {
    let mut files =
        PackageFiles::load_dir(dir).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
    bundle_engine_pack(dir, &mut files)?;
    Ok(files)
}

/// Bundles the engine pack of a package folder (docs/format/engine.md); see
/// `iwbtg_format::engine_pack::bundle_sibling`.
pub fn bundle_engine_pack(dir: &Path, files: &mut PackageFiles) -> Result<(), ToolError> {
    let pack_dir = dir.display();
    let missing = iwbtg_format::engine_pack::bundle_sibling(dir, files)
        .map_err(|e| ToolError::Message(format!("{pack_dir}: {e}")))?;
    for m in missing {
        eprintln!("warning: `{m}` is not in the engine pack");
    }
    Ok(())
}

/// A case by path, or by name under `<cases>/<pkg id>/`.
pub fn find_case(cases: &Path, pkg_id: &str, case: &str) -> PathBuf {
    let p = Path::new(case);
    if p.is_file() {
        return p.to_path_buf();
    }
    let name = case.strip_suffix(".toml").unwrap_or(case);
    cases.join(pkg_id).join(format!("{name}.toml"))
}

/// Reads a case file.
pub fn read_case(path: &Path) -> Result<TraceCase, ToolError> {
    TraceCase::parse(&read_text(path)?).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))
}

/// `iwbtg trace run`: prints our CSV, or writes it to `out`.
pub fn run_main(
    pkg: &Path,
    case: &str,
    out: Option<&Path>,
    stages: bool,
    research: &Path,
    cases: &Path,
) -> Result<(), ToolError> {
    if stages {
        return Err(ToolError::Message(
            "--stages needs a per-phase sample hook in the tick pipeline (WP1), which does not exist yet (OPEN)".into(),
        ));
    }
    let files = load_package(pkg)?;
    let id = package_id(&files)?;
    let case = read_case(&find_case(cases, &id, case))?;
    let ours = run_case(&files, &case, research)?;
    match out {
        Some(path) => std::fs::write(path, ours.to_csv())
            .map_err(|e| ToolError::Message(format!("{}: {e}", path.display()))),
        None => {
            print!("{}", ours.to_csv());
            Ok(())
        }
    }
}

/// `iwbtg trace diff`: prints the comparison; `Ok(false)` when the traces differ.
pub fn diff_main(
    a: &Path,
    b: &Path,
    ignore: &[String],
    context: u32,
    summary: bool,
) -> Result<bool, ToolError> {
    let parse = |p: &Path| {
        TraceTable::parse(&read_text(p)?).map_err(|e| ToolError::Message(format!("{}: {e}", p.display())))
    };
    let (ta, tb) = (parse(a)?, parse(b)?);
    let opts = DiffOptions {
        ignore: ignore.iter().cloned().collect(),
        context,
        summary,
        ..DiffOptions::default()
    };
    let d = trace::diff(&ta, &tb, &opts).map_err(|e| ToolError::Message(e.to_string()))?;
    print!("{d}");
    Ok(d.is_clean())
}

/// `iwbtg trace check`: every case of the package; `Ok(false)` on any difference or error, or
/// when there are no cases.
pub fn check_main(pkg: &Path, research: &Path, cases: &Path) -> Result<bool, ToolError> {
    let files = load_package(pkg)?;
    let id = package_id(&files)?;
    let dir = cases.join(&id);
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    if paths.is_empty() {
        println!("no cases found in {}", dir.display());
        return Ok(false);
    }
    let mut ok = true;
    let (mut passed, mut open) = (0u32, 0u32);
    for path in &paths {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let case = match read_case(path) {
            Ok(c) => c,
            Err(e) => {
                println!("{name} ERROR: {e}");
                ok = false;
                continue;
            }
        };
        match check_case(&files, &case, research) {
            Ok(d) if d.is_clean() => {
                passed += 1;
                if case.unprobed.is_empty() {
                    println!("{name} OK");
                } else {
                    println!(
                        "{name} OK ({} column(s) not compared: no probe column)",
                        case.unprobed.len()
                    );
                }
            }
            Ok(d) => {
                ok = false;
                println!("{name} DIFF");
                print!("{d}");
            }
            Err(ToolError::Open(reason)) => {
                open += 1;
                println!("{name} OPEN: {reason}");
            }
            Err(e) => {
                ok = false;
                println!("{name} ERROR: {e}");
            }
        }
    }
    println!(
        "{passed} OK, {open} OPEN, {} failed, of {}",
        paths.len() - usize::try_from(passed + open).unwrap_or(0),
        paths.len()
    );
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::{CaseStart, SetupEntry};
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, KindDef, RoomDef, VarId};
    use iwbtg_sim::probe::{FieldRef, FixedColumn, KeyRead, SaveField};

    fn def() -> GameDef {
        let mut b = GameDefBuilder::new("cases");
        b.kind(KindDef::new("kid", Builtin::Kid));
        b.kind(KindDef::new("@bullet", Builtin::KidBullet));
        b.room(RoomDef::new("template", 800, 608));
        b.def_mut().var_names.insert("grav".into(), VarId(0));
        b.build().expect("def")
    }

    fn case() -> TraceCase {
        let mut set = BTreeMap::new();
        set.insert("x".to_string(), CaseValue::real(96.0));
        TraceCase {
            source: "kaenbyou/t-alarm-1".into(),
            start: CaseStart {
                room: "template".into(),
                difficulty: 1,
            },
            at: 2,
            first_frame: 2,
            object: "@bullet".into(),
            original_object: "bullet".into(),
            dll_mode: CaseDllMode::Stub,
            setup: vec![
                SetupEntry {
                    frame: 1,
                    what: SetupWhat::Global {
                        name: "grav".into(),
                        value: CaseValue::real(0.0),
                    },
                },
                SetupEntry {
                    frame: 3,
                    what: SetupWhat::Fields {
                        target: "kid".into(),
                        set,
                    },
                },
                SetupEntry {
                    frame: 2,
                    what: SetupWhat::Save {
                        field: "deaths".into(),
                        value: CaseValue::real(3.0),
                    },
                },
            ],
            vars: [
                ("player.x".to_string(), "kid.x".to_string()),
                (
                    "keyboard_check(16)".to_string(),
                    "keyboard_check(jump)".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            ignore: vec!["id".into()],
            unprobed: [("bbox_left".to_string(), "none".to_string())]
                .into_iter()
                .collect(),
            rooms: [("template".to_string(), "rTemplate".to_string())]
                .into_iter()
                .collect(),
            ..TraceCase::default()
        }
    }

    #[test]
    fn resolves_columns_setup_and_start() {
        let vars: Vec<String> = [
            "alarm[0]",
            "player.x",
            "bbox_left",
            "keyboard_check(16)",
            "image_index",
        ]
        .map(String::from)
        .to_vec();
        let r = resolve_case(&case(), &def(), &vars).expect("resolve");
        assert_eq!(
            r.options.start,
            StartPoint::Room {
                room: RoomId(0),
                difficulty: 1
            }
        );
        assert_eq!(r.options.dll_mode, DllMode::Stub);
        assert_eq!(r.object, KindId(1));
        assert_eq!(
            r.columns,
            vec![
                Some(Column::Field {
                    target: ProbeTarget::Row,
                    field: FieldRef::Alarm(0)
                }),
                Some(Column::Field {
                    target: ProbeTarget::Kid,
                    field: FieldRef::Fixed(FixedColumn::X)
                }),
                None,
                Some(Column::Key(KeyRead::Held(iwbtg_sim::input::Action::Jump))),
                Some(Column::Field {
                    target: ProbeTarget::Row,
                    field: FieldRef::Fixed(FixedColumn::ImageIndex)
                }),
            ]
        );
        // Frame 1 < at runs before room entry; frame 2 (= at) after it, before the first tick.
        assert_eq!(r.pre_entry.len(), 1);
        assert!(matches!(
            r.pre_entry[0],
            SetupAction::SetGlobal { var: VarId(0), .. }
        ));
        assert!(matches!(
            r.per_frame[&2][..],
            [SetupAction::SetSave {
                field: SaveField::Deaths,
                ..
            }]
        ));
        assert!(matches!(
            r.per_frame[&3][..],
            [SetupAction::SetField {
                target: ProbeTarget::Kid,
                ..
            }]
        ));
    }

    #[test]
    fn unresolved_names_are_listed_together() {
        let mut c = case();
        c.start.room = "nowhere".into();
        c.vars.insert("hp".into(), "boss.hp".into());
        c.setup.push(SetupEntry {
            frame: 5,
            what: SetupWhat::Global {
                name: "difficulty".into(),
                value: CaseValue::real(1.0),
            },
        });
        let Err(ToolError::Message(m)) = resolve_case(&c, &def(), &["hp".to_string()]) else {
            panic!("expected an error");
        };
        assert!(
            m.contains("nowhere") && m.contains("\"hp\"") && m.contains("difficulty"),
            "{m}"
        );
    }

    #[test]
    fn frames_and_splicing() {
        let mut c = case();
        assert_eq!(frame_range(&c, 80), 2..=80);
        c.end_frame = Some(40);
        assert_eq!(frame_range(&c, 80), 2..=40);
        let fixed = || {
            let mut row: Vec<trace::Value> = (0..18).map(|_| trace::Value::Real(0.0)).collect();
            row[1] = trace::Value::Str("template".into());
            row[2] = trace::Value::Str("@bullet".into());
            row
        };
        let mut row = fixed();
        row.push(trace::Value::Real(40.0));
        let out = splice(
            &c,
            &[Some(Column::Count(iwbtg_sim::collide::Target::Any)), None],
            row,
        );
        assert_eq!(out.len(), 20);
        assert_eq!(out[1], trace::Value::Str("rTemplate".into()));
        assert_eq!(out[2], trace::Value::Str("bullet".into()));
        assert_eq!(out[18], trace::Value::Real(40.0));
        assert_eq!(out[19], trace::Value::Empty);
        // Frames before the room jump are not compared.
        let original = TraceTable::parse("frame,object\n0,\"w\"\n1,\"w\"\n2,\"w\"\n").expect("t");
        c.first_frame = 0;
        assert_eq!(compared_original(&c, &original).rows.len(), 1);
        let opts = case_diff_options(&c, 5);
        assert!(opts.ignore.contains("id") && opts.ignore.contains("bbox_left"));
    }

    #[test]
    fn open_cases_do_not_run() {
        let mut c = case();
        c.open = Some("A/D keys".into());
        let files = PackageFiles::new();
        assert!(matches!(
            run_case(&files, &c, Path::new("nowhere")),
            Err(ToolError::Open(r)) if r == "A/D keys"
        ));
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;

    #[test]
    fn latch_holds_the_value_before_the_last_read() {
        let mut table = TraceTable::new(&["v"]).unwrap();
        let fixed = trace::FIXED_COLUMNS.len();
        for f in 2..8_i64 {
            let mut row = vec![trace::Value::Empty; fixed + 1];
            row[0] = trace::Value::Int(f);
            row[fixed] = trace::Value::Real(f as f64 * 10.0);
            table.push_row(row).unwrap();
        }
        let case = TraceCase {
            latch: [("v".to_string(), vec![4, 6])].into_iter().collect(),
            ..TraceCase::default()
        };
        latch_columns(&case, &mut table);
        let got: Vec<f64> = table.rows.iter().map(|r| r[fixed].as_f64().unwrap()).collect();
        assert_eq!(got, vec![0.0, 0.0, 30.0, 30.0, 50.0, 50.0]);
    }

    #[test]
    fn load_package_bundles_the_pack_next_to_the_package() {
        let root = std::env::temp_dir().join(format!("iwbtg-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pkg = root.join("game_a");
        let pack = root.join("_engines/a1");
        std::fs::create_dir_all(pkg.join("rooms")).unwrap();
        std::fs::create_dir_all(pack.join("sprites")).unwrap();
        std::fs::write(
            pkg.join("game.toml"),
            "engine_pack = \"a1\"\n[defaults.kid]\nsprite = \"engine:a1/kid\"\n",
        )
        .unwrap();
        std::fs::write(pack.join("engine.toml"), "id = \"a1\"\n").unwrap();
        std::fs::write(pack.join("sprites/kid.toml"), "frames = 1\n").unwrap();
        std::fs::write(pack.join("sprites/kid.png"), b"png").unwrap();
        std::fs::write(pack.join("sprites/unused.png"), b"png").unwrap();
        let files = load_package(&pkg).unwrap();
        assert!(files.contains("engine/a1/engine.toml"));
        assert!(files.contains("engine/a1/sprites/kid.png"));
        assert!(files.contains("engine/a1/sprites/kid.toml"));
        assert!(
            !files.contains("engine/a1/sprites/unused.png"),
            "only what the package uses"
        );
        assert!(
            !pkg.join("engine").exists(),
            "nothing is written to the package folder"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
