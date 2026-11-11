//! Trace case files `crates/tools/cases/<package-id>/<case>.toml` (docs/plans/sim-core.md 9.2).
//! WP0 creates the file; WP8a owns it (the format and the importer; case contents are WP11's).
//!
//! A case says how to run one research trace on our sim and how to compare the result. It holds
//! package ids and research trace names only, never creator content. `iwbtg trace import`
//! writes the skeleton; a person finishes it by hand.
//!
//! ```toml
//! source = "kaenbyou/t-alarm-1"     # research/specs/engines/gm8/traces/<source>.{csv,inputs}
//! kind = "diff"                     # diff | probe_read
//! start = { room = "template", difficulty = 0 }
//! at = 2                            # recorder --at: the room jump; tick k of our run is frame at + k
//! first_frame = 2                   # recorder --from: first frame with rows
//! object = "@bullet"                # our kind for the traced object
//! original_object = "bullet"        # the recorder's --object, written back into our rows
//! dll_mode = "stub"                 # recorder --stub-dlls
//! seed = 0                          # recorder --seed (default 0)
//! setup = [
//!   { frame = 1, global = "grav", value = 0.0 },                  # frame < at: before room entry
//!   { frame = 3, target = "kid", set = { x = 96.0, y = 567.4 } }, # before the tick of frame 3
//!   { frame = 3, save = "deaths", value = 2.0 },
//! ]
//! vars = { "player.x" = "kid.x" }   # original column -> probe expression in our names
//! ignore = ["id", "sprite_index", "sprite_name"]
//! latch = { "global.tr_x" = [3, 20] }  # recorder `--gml` reads: the column holds the `vars` value
//!                                    # of the frame before the last read frame (0 before the first)
//! split = { "global.sv" = ["room:3", "x:3", "_:1"] }  # a byte-dump column (`"0 1 41 0 ..."`) read
//!                                    # as columns `global.sv.room`, `global.sv.x`, ...: each field is
//!                                    # that many base-100 digit bytes (1-8); `_` bytes are dropped
//! unprobed = { "bbox_left" = "no probe column (OPEN)" }   # not compared, reported by check
//! rooms = { template = "rTemplate" }                     # our room id -> original room name
//! open = "why the case cannot run yet"                   # optional; check skips it
//! notes = ["free text"]
//! ```
//!
//! Setup targets are `kid`, a kind name (the first instance of that kind) or `named:<name>` (a
//! room-file instance name). Field, global and save names resolve through
//! `iwbtg_sim::probe::registry` when the case runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

/// The recorder's default `--at` (research `tooling/opengmk-trace/README.md`).
pub const DEFAULT_AT: u32 = 2;

/// Columns no case compares by default: ids and sprite indices never match the original's, and
/// `GameDef` has no sprite names.
pub const DEFAULT_IGNORE: [&str; 3] = ["id", "sprite_index", "sprite_name"];

/// One trace case: source trace, start point, first frame, traced object, DLL mode, setup,
/// variable map, ignored columns.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceCase {
    /// `<game>/<trace>` under the research traces folder.
    pub source: String,
    /// How the case is compared (plan 9.1 kinds 1 and 3).
    pub kind: CaseKind,
    /// Why the case cannot run yet; `trace check` reports it as OPEN instead of running it.
    pub open: Option<String>,
    /// Our start room and difficulty (`StartPoint::Room`).
    pub start: CaseStart,
    /// The recorder's `--at`: the frame whose tick is the first in the room.
    pub at: u32,
    /// The recorder's `--from`: the first frame with rows.
    pub first_frame: u32,
    /// The recorder's `--frames`, when it cut the run short of the `.inputs` end.
    pub end_frame: Option<u32>,
    /// Our kind for the traced object.
    pub object: String,
    /// The traced object's original name, written into our `object` column.
    pub original_object: String,
    /// DLL adapters modeled or stubbed.
    pub dll_mode: CaseDllMode,
    /// RNG seed (recorder `--seed`).
    pub seed: u32,
    /// Setup actions, in file order.
    pub setup: Vec<SetupEntry>,
    /// Original column name to probe expression; a column not listed uses its own name.
    pub vars: BTreeMap<String, String>,
    /// Columns not compared.
    pub ignore: Vec<String>,
    /// Columns the recorder's own `--gml` wrote only before the listed frames (a save file read
    /// into a global): the column keeps the `vars` value of the frame before the last read frame,
    /// 0 before the first.
    pub latch: BTreeMap<String, Vec<u32>>,
    /// Byte-dump columns of the original (a recorder `--gml` that read a save file back as
    /// space-separated decimal bytes), split into one column per field before anything else reads
    /// the original. See [`SplitField`].
    pub split: BTreeMap<String, Vec<SplitField>>,
    /// Columns not compared because no probe column exists for them yet, with the reason.
    pub unprobed: BTreeMap<String, String>,
    /// Our room id to the original room name, for the `room` column.
    pub rooms: BTreeMap<String, String>,
    /// Free text for the person finishing the case.
    pub notes: Vec<String>,
}

/// One field of a [`TraceCase::split`] column: `width` bytes, each a base-100 digit, most
/// significant first (the Group A save layout, CLONE_SPEC "Group A layout"). The field becomes the
/// column `<column>.<name>`; a field named `_` is skipped.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SplitField {
    /// The field name, or `_`.
    pub name: String,
    /// Bytes in the field: 1 to 8, or up to 255 for `_`.
    pub width: u8,
}

impl SplitField {
    /// The name of a skipped field.
    pub const SKIP: &'static str = "_";

    /// `name:width`.
    fn parse(text: &str) -> Result<SplitField, String> {
        let (name, width) = text
            .split_once(':')
            .ok_or_else(|| format!("field {text:?}: expected \"<name>:<bytes>\""))?;
        let name = name.trim();
        let max = if name == Self::SKIP { u8::MAX } else { 8 };
        let width: u8 = width
            .trim()
            .parse()
            .ok()
            .filter(|w| (1..=max).contains(w))
            .ok_or_else(|| format!("field {text:?}: the byte count must be 1 to {max}"))?;
        let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(format!(
                "field {text:?}: the name must be letters, digits and `_`"
            ));
        }
        Ok(SplitField {
            name: name.to_string(),
            width,
        })
    }
}

/// Which of the plan's trace kinds a case is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaseKind {
    /// Kind 1: must diff clean.
    #[default]
    Diff,
    /// Kind 3: the recorder decoded the save file into globals; columns map to `save.*`.
    ProbeRead,
}

impl CaseKind {
    /// The name in case files.
    pub fn name(self) -> &'static str {
        match self {
            CaseKind::Diff => "diff",
            CaseKind::ProbeRead => "probe_read",
        }
    }
}

/// DLL mode as case files write it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaseDllMode {
    /// Modeled adapters.
    #[default]
    Model,
    /// Every DLL call returns 0 or "", like `--stub-dlls`.
    Stub,
}

/// Start room and difficulty.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CaseStart {
    /// Our room id.
    pub room: String,
    /// Difficulty index.
    pub difficulty: u8,
}

/// One setup line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetupEntry {
    /// The recorder frame whose `--gml` it replaces: before room entry when `< at`, else before
    /// that frame's tick.
    pub frame: u32,
    /// What it sets.
    pub what: SetupWhat,
}

/// What a setup line sets.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetupWhat {
    /// Fields of one instance, in name order.
    Fields {
        /// `kid`, a kind name or `named:<name>`.
        target: String,
        /// Field name to value.
        set: BTreeMap<String, CaseValue>,
    },
    /// A global.
    Global {
        /// Its name, without `global.`.
        name: String,
        /// The value.
        value: CaseValue,
    },
    /// A save slot field.
    Save {
        /// `exists`, `room`, `x`, ... (`registry::save_field`).
        field: String,
        /// The value.
        value: CaseValue,
    },
}

/// A setup value. Reals keep their exact bits, so cases compare with `Eq`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CaseValue {
    /// A real, as f64 bits.
    Real(u64),
    /// A string.
    Str(String),
}

impl CaseValue {
    /// A real value.
    pub fn real(r: f64) -> CaseValue {
        CaseValue::Real(r.to_bits())
    }
}

/// Why a case file was rejected.
#[derive(Debug, thiserror::Error)]
pub enum CaseError {
    /// Not TOML, or not the case schema.
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    /// A value is not allowed.
    #[error("{0}")]
    Invalid(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    source: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    open: Option<String>,
    start: RawStart,
    #[serde(default)]
    at: Option<u32>,
    first_frame: u32,
    #[serde(default)]
    end_frame: Option<u32>,
    object: String,
    #[serde(default)]
    original_object: Option<String>,
    #[serde(default)]
    dll_mode: Option<String>,
    #[serde(default)]
    seed: u32,
    #[serde(default)]
    setup: Vec<RawSetup>,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    ignore: Option<Vec<String>>,
    #[serde(default)]
    latch: BTreeMap<String, Vec<u32>>,
    #[serde(default)]
    split: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    unprobed: BTreeMap<String, String>,
    #[serde(default)]
    rooms: BTreeMap<String, String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStart {
    room: String,
    #[serde(default)]
    difficulty: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSetup {
    frame: u32,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    set: Option<BTreeMap<String, RawValue>>,
    #[serde(default)]
    global: Option<String>,
    #[serde(default)]
    save: Option<String>,
    #[serde(default)]
    value: Option<RawValue>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl RawValue {
    fn into_case(self) -> CaseValue {
        match self {
            #[allow(
                clippy::cast_precision_loss,
                reason = "GML reals are f64; case integers are small"
            )]
            RawValue::Int(i) => CaseValue::real(i as f64),
            RawValue::Float(f) => CaseValue::real(f),
            RawValue::Bool(b) => CaseValue::real(if b { 1.0 } else { 0.0 }),
            RawValue::Str(s) => CaseValue::Str(s),
        }
    }
}

impl TraceCase {
    /// Reads a case file.
    pub fn parse(text: &str) -> Result<TraceCase, CaseError> {
        let raw: RawCase = toml::from_str(text)?;
        let invalid = |m: String| Err(CaseError::Invalid(m));
        let kind = match raw.kind.as_deref() {
            None | Some("diff") => CaseKind::Diff,
            Some("probe_read") => CaseKind::ProbeRead,
            Some(other) => return invalid(format!("kind {other:?}: expected \"diff\" or \"probe_read\"")),
        };
        let dll_mode = match raw.dll_mode.as_deref() {
            None | Some("model") => CaseDllMode::Model,
            Some("stub") => CaseDllMode::Stub,
            Some(other) => return invalid(format!("dll_mode {other:?}: expected \"model\" or \"stub\"")),
        };
        let mut setup = Vec::with_capacity(raw.setup.len());
        for (i, s) in raw.setup.into_iter().enumerate() {
            let what = match (s.target, s.set, s.global, s.save, s.value) {
                (Some(target), Some(set), None, None, None) => SetupWhat::Fields {
                    target,
                    set: set.into_iter().map(|(k, v)| (k, v.into_case())).collect(),
                },
                (None, None, Some(name), None, Some(value)) => SetupWhat::Global {
                    name,
                    value: value.into_case(),
                },
                (None, None, None, Some(field), Some(value)) => SetupWhat::Save {
                    field,
                    value: value.into_case(),
                },
                _ => {
                    return invalid(format!(
                        "setup entry {}: expected `target` and `set`, `global` and `value`, or `save` and `value`",
                        i + 1
                    ));
                }
            };
            setup.push(SetupEntry { frame: s.frame, what });
        }
        let mut split = BTreeMap::new();
        for (column, fields) in raw.split {
            let fields = fields
                .iter()
                .map(|f| SplitField::parse(f))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CaseError::Invalid(format!("split {column:?}: {e}")))?;
            if fields.is_empty() {
                return invalid(format!("split {column:?}: no fields"));
            }
            split.insert(column, fields);
        }
        Ok(TraceCase {
            original_object: raw.original_object.unwrap_or_else(|| raw.object.clone()),
            source: raw.source,
            kind,
            open: raw.open,
            start: CaseStart {
                room: raw.start.room,
                difficulty: raw.start.difficulty,
            },
            at: raw.at.unwrap_or(DEFAULT_AT),
            first_frame: raw.first_frame,
            end_frame: raw.end_frame,
            object: raw.object,
            dll_mode,
            seed: raw.seed,
            setup,
            vars: raw.vars,
            ignore: raw
                .ignore
                .unwrap_or_else(|| DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect()),
            latch: raw.latch,
            split,
            unprobed: raw.unprobed,
            rooms: raw.rooms,
            notes: raw.notes,
        })
    }

    /// Writes the case file, with `header` lines as leading comments. [`TraceCase::parse`] reads
    /// the result back to an equal case.
    pub fn to_toml(&self, header: &[String]) -> String {
        let mut out = String::new();
        for line in header {
            let _ = writeln!(out, "# {line}");
        }
        let _ = writeln!(out, "source = {}", quote(&self.source));
        let _ = writeln!(out, "kind = {}", quote(self.kind.name()));
        if let Some(open) = &self.open {
            let _ = writeln!(out, "open = {}", quote(open));
        }
        let _ = writeln!(
            out,
            "start = {{ room = {}, difficulty = {} }}",
            quote(&self.start.room),
            self.start.difficulty
        );
        let _ = writeln!(out, "at = {}", self.at);
        let _ = writeln!(out, "first_frame = {}", self.first_frame);
        if let Some(end) = self.end_frame {
            let _ = writeln!(out, "end_frame = {end}");
        }
        let _ = writeln!(out, "object = {}", quote(&self.object));
        let _ = writeln!(out, "original_object = {}", quote(&self.original_object));
        let dll = match self.dll_mode {
            CaseDllMode::Model => "model",
            CaseDllMode::Stub => "stub",
        };
        let _ = writeln!(out, "dll_mode = {}", quote(dll));
        let _ = writeln!(out, "seed = {}", self.seed);
        if self.setup.is_empty() {
            let _ = writeln!(out, "setup = []");
        } else {
            let _ = writeln!(out, "setup = [");
            for s in &self.setup {
                let body = match &s.what {
                    SetupWhat::Fields { target, set } => {
                        let fields: Vec<String> = set
                            .iter()
                            .map(|(k, v)| format!("{} = {}", key(k), value(v)))
                            .collect();
                        format!("target = {}, set = {{ {} }}", quote(target), fields.join(", "))
                    }
                    SetupWhat::Global { name, value: v } => {
                        format!("global = {}, value = {}", quote(name), value(v))
                    }
                    SetupWhat::Save { field, value: v } => {
                        format!("save = {}, value = {}", quote(field), value(v))
                    }
                };
                let _ = writeln!(out, "  {{ frame = {}, {body} }},", s.frame);
            }
            let _ = writeln!(out, "]");
        }
        let ignore: Vec<String> = self.ignore.iter().map(|s| quote(s)).collect();
        let _ = writeln!(out, "ignore = [{}]", ignore.join(", "));
        if !self.notes.is_empty() {
            let _ = writeln!(out, "notes = [");
            for n in &self.notes {
                let _ = writeln!(out, "  {},", quote(n));
            }
            let _ = writeln!(out, "]");
        }
        if !self.latch.is_empty() {
            let rows: Vec<String> = self
                .latch
                .iter()
                .map(|(k, v)| {
                    let frames: Vec<String> = v.iter().map(u32::to_string).collect();
                    format!("{} = [{}]", quote(k), frames.join(", "))
                })
                .collect();
            let _ = writeln!(out, "latch = {{ {} }}", rows.join(", "));
        }
        if !self.split.is_empty() {
            let rows: Vec<String> = self
                .split
                .iter()
                .map(|(k, v)| {
                    let fields: Vec<String> = v
                        .iter()
                        .map(|f| quote(&format!("{}:{}", f.name, f.width)))
                        .collect();
                    format!("{} = [{}]", quote(k), fields.join(", "))
                })
                .collect();
            let _ = writeln!(out, "split = {{ {} }}", rows.join(", "));
        }
        // Tables last: every key after a `[table]` header belongs to that table.
        write_table(&mut out, "vars", &self.vars);
        write_table(&mut out, "unprobed", &self.unprobed);
        write_table(&mut out, "rooms", &self.rooms);
        out
    }
}

/// `[name]` and one `"key" = "value"` line per entry.
fn write_table(out: &mut String, name: &str, table: &BTreeMap<String, String>) {
    let _ = writeln!(out, "\n[{name}]");
    for (k, v) in table {
        let _ = writeln!(out, "{} = {}", quote(k), quote(v));
    }
}

/// A TOML basic string.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A key: bare when TOML allows it, else quoted.
fn key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_string()
    } else {
        quote(k)
    }
}

/// A setup value. Reals are written as TOML floats that read back to the same bits.
fn value(v: &CaseValue) -> String {
    match v {
        CaseValue::Str(s) => quote(s),
        CaseValue::Real(bits) => {
            let r = f64::from_bits(*bits);
            if r.is_nan() {
                "nan".to_string()
            } else if r.is_infinite() {
                if r > 0.0 { "inf" } else { "-inf" }.to_string()
            } else {
                // `{:?}` always writes a `.0` or an exponent (`1e16`, `5e-324`), so TOML reads a
                // float with the same bits.
                format!("{r:?}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TraceCase {
        let mut set = BTreeMap::new();
        set.insert("x".into(), CaseValue::real(96.0));
        set.insert("y".into(), CaseValue::real(567.4));
        set.insert("alarm[0]".into(), CaseValue::real(-0.0));
        TraceCase {
            source: "kaenbyou/t-alarm-1".into(),
            kind: CaseKind::Diff,
            open: Some("needs \"quotes\"\nand lines".into()),
            start: CaseStart {
                room: "template".into(),
                difficulty: 1,
            },
            at: 2,
            first_frame: 2,
            end_frame: Some(80),
            object: "@bullet".into(),
            original_object: "bullet".into(),
            dll_mode: CaseDllMode::Stub,
            seed: 7,
            setup: vec![
                SetupEntry {
                    frame: 1,
                    what: SetupWhat::Global {
                        name: "grav".into(),
                        value: CaseValue::real(1e-7),
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
                    frame: 4,
                    what: SetupWhat::Save {
                        field: "room".into(),
                        value: CaseValue::Str("x".into()),
                    },
                },
            ],
            vars: [("player.x".to_string(), "kid.x".to_string())]
                .into_iter()
                .collect(),
            ignore: vec!["id".into()],
            latch: [("global.tr_x".to_string(), vec![3, 20])].into_iter().collect(),
            split: [(
                "global.sv".to_string(),
                vec![
                    SplitField {
                        name: "x".into(),
                        width: 3,
                    },
                    SplitField {
                        name: "_".into(),
                        width: 1,
                    },
                ],
            )]
            .into_iter()
            .collect(),
            unprobed: [("bbox_left".to_string(), "no probe column".to_string())]
                .into_iter()
                .collect(),
            rooms: [("template".to_string(), "rTemplate".to_string())]
                .into_iter()
                .collect(),
            notes: vec!["n".into()],
        }
    }

    #[test]
    fn round_trips() {
        let case = sample();
        let text = case.to_toml(&["header".into()]);
        assert!(text.starts_with("# header\n"), "{text}");
        let back = TraceCase::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(back, case, "{text}");
        let empty = TraceCase {
            source: "a/b".into(),
            start: CaseStart {
                room: "r".into(),
                difficulty: 0,
            },
            object: "kid".into(),
            original_object: "player".into(),
            ..TraceCase::default()
        };
        let text = empty.to_toml(&[]);
        assert_eq!(TraceCase::parse(&text).expect("parse"), empty, "{text}");
        for r in [
            f64::NAN,
            f64::INFINITY,
            -f64::INFINITY,
            1e16,
            5e-324,
            -0.0,
            0.1 + 0.2,
        ] {
            let v = value(&CaseValue::real(r));
            let parsed: toml::Table = toml::from_str(&format!("v = {v}")).expect("toml");
            let back = parsed["v"].as_float().expect("float");
            assert!(
                back.to_bits() == r.to_bits() || (r.is_nan() && back.is_nan()),
                "{r:?} -> {v}"
            );
        }
    }

    #[test]
    fn defaults_and_errors() {
        let case = TraceCase::parse(
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"kid\"\n\
             setup = [{ frame = 1, global = \"g\", value = 2 }, { frame = 2, target = \"kid\", set = { visible = true } }]\n",
        )
        .expect("parse");
        assert_eq!(case.at, DEFAULT_AT);
        assert_eq!(case.kind, CaseKind::Diff);
        assert_eq!(case.dll_mode, CaseDllMode::Model);
        assert_eq!(case.original_object, "kid");
        assert_eq!(case.ignore, DEFAULT_IGNORE.map(String::from).to_vec());
        assert_eq!(
            case.setup[0].what,
            SetupWhat::Global {
                name: "g".into(),
                value: CaseValue::real(2.0)
            }
        );
        let SetupWhat::Fields { set, .. } = &case.setup[1].what else {
            panic!("fields");
        };
        assert_eq!(set["visible"], CaseValue::real(1.0));
        for bad in [
            "source = 1",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nkind = \"x\"\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\ndll_mode = \"real\"\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nsetup = [{ frame = 1, global = \"g\" }]\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nbogus = 1\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nsplit = { s = [\"x\"] }\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nsplit = { s = [\"x:9\"] }\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nsplit = { s = [\"a.b:1\"] }\n",
            "source = \"g/t\"\nstart = { room = \"r\" }\nfirst_frame = 2\nobject = \"k\"\nsplit = { s = [] }\n",
        ] {
            assert!(TraceCase::parse(bad).is_err(), "{bad:?}");
        }
    }
}
