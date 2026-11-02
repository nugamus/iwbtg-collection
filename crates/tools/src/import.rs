//! `iwbtg trace import`: case skeletons, sorting traces into the three kinds of
//! docs/plans/sim-core.md 9.1. WP0 creates the file; WP8a owns it.
//!
//! The importer reads the recorder command line from the `.inputs` header
//! (`iwbtg_sim::input::trace_command`; the tracer's version string is not used) and sorts the
//! trace by its `--gml` code:
//!
//! 1. **Diff target**: no `--gml`, or `--gml` that only assigns literals (`global.difficulty=1`,
//!    `with(player){x=96;y=567.4}`, `player.s=1`), plus the DLL placeholder file some games
//!    check for. The assignments become case `setup`; `global.difficulty` before the room jump
//!    becomes the start difficulty. `--x/--y` become setup on the traced object at the jump
//!    frame.
//! 2. **Spec probe**: `--gml` that adds events or objects (`object_event_add`, the logging
//!    probes of t-ord-1, t-evo-1, t-room-1, tr-room-01-a), any other code the sim cannot run, and
//!    every `zz-*` trace. Research reads these; no case file is written.
//! 3. **Probe read**: `--gml` that reads and decodes a save file into globals. The case's
//!    columns must be mapped to `save.*` probe columns by hand.
//!
//! Original names go through the package's `tests/trace-map.toml` (rooms and objects). Until
//! `iwbtg_format::tracemap` (WP7a) has fields, the importer reads the two tables it needs itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use iwbtg_sim::input::{self, KeyMap};
use iwbtg_sim::probe::registry;
use iwbtg_sim::trace::{FIXED_COLUMNS, TraceTable};

use crate::case::{
    CaseDllMode, CaseKind, CaseStart, CaseValue, DEFAULT_AT, DEFAULT_IGNORE, SetupEntry, SetupWhat, TraceCase,
};
use crate::trace_cmd::ToolError;

/// The three kinds of plan 9.1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceKind {
    /// Kind 1: a case that must diff clean.
    DiffTarget,
    /// Kind 2: settles a spec item; research reads it, the sim does not diff it.
    SpecProbe,
    /// Kind 3: reads the save file into globals; maps to `save.*` probe columns.
    ProbeRead,
}

impl TraceKind {
    /// The line `trace import` prints after `kind: `.
    pub fn label(self) -> &'static str {
        match self {
            TraceKind::DiffTarget => "diff target",
            TraceKind::SpecProbe => "spec probe (not diffed; research reads it)",
            TraceKind::ProbeRead => "probe read (map its columns to save.* by hand)",
        }
    }
}

/// What importing one trace gave.
#[derive(Clone, Debug)]
pub struct Imported {
    /// The trace's kind.
    pub kind: TraceKind,
    /// Why it has that kind, for spec probes and probe reads.
    pub reason: Option<String>,
    /// The case skeleton, for diff targets and probe reads.
    pub case: Option<TraceCase>,
    /// Header comment lines for the case file.
    pub header: Vec<String>,
    /// Warnings for the person finishing the case.
    pub warnings: Vec<String>,
}

/// The recorder's command line, as far as the importer uses it (research
/// `tooling/opengmk-trace/README.md`, v1 and v2 options).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RecorderArgs {
    /// `--object` (repeatable or a comma list; default `player`). The first is the one `--x
    /// --y` moves.
    pub objects: Vec<String>,
    /// `--room`.
    pub room: Option<String>,
    /// `--at` (default 2).
    pub at: u32,
    /// `--from` (default 0).
    pub from: u32,
    /// `--frames`.
    pub frames: Option<u32>,
    /// `--x`, as typed.
    pub x: Option<String>,
    /// `--y`, as typed.
    pub y: Option<String>,
    /// `--gml N:CODE`, in order.
    pub gml: Vec<(u32, String)>,
    /// `--gml-every [A-B:]CODE`: the code.
    pub gml_every: Vec<String>,
    /// `--gml-end [N:|A-B:]CODE`: the code.
    pub gml_end: Vec<String>,
    /// `--skip-object`.
    pub skip_objects: Vec<String>,
    /// v2 logging options that leave the CSV as it is (`--log-events`, `--log-calls`,
    /// `--phases`, `--keep-files`).
    pub logs: Vec<String>,
    /// `--stub-dlls`.
    pub stub_dlls: bool,
    /// `--seed` (default 0).
    pub seed: u32,
}

impl RecorderArgs {
    /// The first traced object.
    pub fn object(&self) -> &str {
        self.objects.first().map_or("player", String::as_str)
    }

    /// Reads the `iwc trace ...` command.
    pub fn parse(command: &str) -> Result<RecorderArgs, String> {
        let words = shell_words(command)?;
        let mut it = words.into_iter().peekable();
        if it.next().as_deref() != Some("iwc") || it.next().as_deref() != Some("trace") {
            return Err("unsupported header: not an `iwc trace` command".into());
        }
        let mut args = RecorderArgs {
            at: DEFAULT_AT,
            ..RecorderArgs::default()
        };
        let number =
            |name: &str, v: &str| v.trim().parse::<u32>().map_err(|_| format!("{name} {v:?} is not a frame number"));
        while let Some(flag) = it.next() {
            let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--object" => args.objects.extend(value()?.split(',').map(|o| o.trim().to_string())),
                "--room" => args.room = Some(value()?),
                "--at" => args.at = number("--at", &value()?)?,
                "--from" => args.from = number("--from", &value()?)?,
                "--frames" => args.frames = Some(number("--frames", &value()?)?),
                "--x" => args.x = Some(value()?),
                "--y" => args.y = Some(value()?),
                "--seed" => args.seed = number("--seed", &value()?)?,
                "--gml" => {
                    let g = value()?;
                    let (frame, code) = g
                        .split_once(':')
                        .ok_or_else(|| format!("--gml {:?}: expected N:CODE", head(&g)))?;
                    args.gml.push((number("--gml", frame)?, code.to_string()));
                }
                "--gml-every" => args.gml_every.push(strip_frames(&value()?)),
                "--gml-end" => args.gml_end.push(strip_frames(&value()?)),
                "--skip-object" => args.skip_objects.extend(value()?.split(',').map(|o| o.trim().to_string())),
                "--log-calls" | "--phases" => {
                    let v = value()?;
                    args.logs.push(format!("{flag} {v}"));
                }
                "--log-events" => {
                    // The object list is optional.
                    let list = it.next_if(|w| !w.starts_with("--"));
                    args.logs.push(match list {
                        Some(l) => format!("--log-events {l}"),
                        None => "--log-events".to_string(),
                    });
                }
                "--keep-files" => args.logs.push(flag.clone()),
                "--stub-dlls" => args.stub_dlls = true,
                // Values the importer does not need (the CSV header names the columns).
                "--game" | "--inputs" | "--vars" | "--out" => {
                    value()?;
                }
                "--allow-dlls" => {
                    return Err("--allow-dlls traces ran native DLL code; not importable".into());
                }
                other => return Err(format!("unsupported header: unknown recorder flag {other:?}")),
            }
        }
        if args.objects.is_empty() {
            args.objects.push("player".into());
        }
        Ok(args)
    }
}

/// `N:CODE` or `A-B:CODE` to `CODE`; plain `CODE` (every frame) as it is.
fn strip_frames(spec: &str) -> String {
    match spec.split_once(':') {
        Some((frames, code))
            if !frames.is_empty() && frames.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ') =>
        {
            code.to_string()
        }
        _ => spec.to_string(),
    }
}

/// The first characters of a long text, for messages.
fn head(s: &str) -> String {
    let mut h: String = s.chars().take(60).collect();
    if h.len() < s.len() {
        h.push_str("...");
    }
    h
}

/// POSIX shell words: whitespace separates, `'...'` is literal, `"..."` honors `\` before
/// `"`, `\`, `$`, `` ` `` and a newline, and `\` outside quotes escapes the next character.
pub fn shell_words(s: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("unclosed ' in the recorder command".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => word.push(c),
                            Some('\n') => {}
                            Some(c) => {
                                word.push('\\');
                                word.push(c);
                            }
                            None => return Err("unclosed \" in the recorder command".into()),
                        },
                        Some(c) => word.push(c),
                        None => return Err("unclosed \" in the recorder command".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some('\n') | None => {}
                    Some(c) => word.push(c),
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// A literal a `--gml` assignment stores.
#[derive(Clone, PartialEq, Debug)]
pub enum GmlValue {
    /// A number.
    Real(f64),
    /// A string.
    Str(String),
}

/// Whose variable an assignment writes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GmlOwner {
    /// `global.name`.
    Global,
    /// `obj.name` or `with (obj) { name = ... }`.
    Object(String),
    /// A bare `name` outside any `with`: the instance the recorder runs the code in, unknown.
    Unknown,
}

/// One literal assignment.
#[derive(Clone, PartialEq, Debug)]
pub struct Assignment {
    /// Whose variable.
    pub owner: GmlOwner,
    /// The variable, with its index (`alarm[0]`).
    pub field: String,
    /// The value.
    pub value: GmlValue,
}

/// What one `--gml` code does.
#[derive(Clone, PartialEq, Debug)]
pub enum GmlUse {
    /// Literal assignments only.
    Assign(Vec<Assignment>),
    /// Writes an empty file the game checks for (`GMFMODSimple.dll` in go-the-spook-house).
    PlaceholderFile(String),
    /// Reads and decodes a file into globals.
    ReadsSave,
    /// Adds events or runs other code.
    Probe(String),
}

/// Calls that make a `--gml` a spec probe: injected events, objects and timelines.
const PROBE_CALLS: [&str; 6] = [
    "object_event_add",
    "object_event_clear",
    "object_add",
    "timeline_add",
    "timeline_moment_add",
    "path_add",
];

/// Calls that read a file.
const READ_CALLS: [&str; 6] = [
    "file_text_open_read",
    "file_bin_open",
    "file_bin_read_byte",
    "file_text_read_string",
    "ds_list_read",
    "dataDecrypt",
];

/// Sorts one `--gml` code.
pub fn analyze_gml(code: &str) -> GmlUse {
    if let Some(call) = PROBE_CALLS.iter().find(|c| contains_call(code, c)) {
        return GmlUse::Probe(format!("--gml calls {call}"));
    }
    if READ_CALLS.iter().any(|c| contains_call(code, c)) {
        return GmlUse::ReadsSave;
    }
    if let Some(name) = placeholder_file(code) {
        return GmlUse::PlaceholderFile(name);
    }
    match assignments(code) {
        Some(a) => GmlUse::Assign(a),
        None => GmlUse::Probe(format!("--gml is not plain assignments: {}", head(code.trim()))),
    }
}

/// True when `code` calls `name(`.
fn contains_call(code: &str, name: &str) -> bool {
    code.match_indices(name).any(|(i, _)| {
        let before = code[..i].chars().next_back();
        let after = code[i + name.len()..].trim_start().chars().next();
        !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') && after == Some('(')
    })
}

/// `__f=file_text_open_write('NAME'); file_text_close(__f)`: the file's name.
fn placeholder_file(code: &str) -> Option<String> {
    let flat: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = flat.strip_prefix("__f=file_text_open_write(")?;
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    let name = &rest[..end];
    let tail = rest[end + 1..].strip_prefix(");file_text_close(__f)")?;
    tail.trim_end_matches(';').is_empty().then(|| name.to_string())
}

/// GML tokens the assignment reader knows.
#[derive(Clone, PartialEq, Debug)]
enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    Punct(char),
}

fn tokens(code: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let mut chars = code.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Tok::Ident(s));
        } else if c.is_ascii_digit() {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Tok::Num(s.parse().ok()?));
        } else if c == '\'' || c == '"' {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next()? {
                    q if q == c => break,
                    ch => s.push(ch),
                }
            }
            out.push(Tok::Str(s));
        } else if "{}()[];,.=-".contains(c) {
            chars.next();
            out.push(Tok::Punct(c));
        } else {
            // Operators, comparisons: not a plain assignment.
            return None;
        }
    }
    Some(out)
}

/// The literal assignments of `code`, or `None` when it does anything else.
fn assignments(code: &str) -> Option<Vec<Assignment>> {
    let toks = tokens(code)?;
    let mut p = Parser {
        toks,
        pos: 0,
        out: Vec::new(),
    };
    while p.pos < p.toks.len() {
        p.stmt(None)?;
    }
    Some(p.out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    out: Vec<Assignment>,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn punct(&mut self, c: char) -> Option<()> {
        (self.next()? == Tok::Punct(c)).then_some(())
    }

    fn ident(&mut self) -> Option<String> {
        match self.next()? {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }

    /// One statement in the scope of `with`'s object, if any.
    fn stmt(&mut self, scope: Option<&str>) -> Option<()> {
        match self.peek()? {
            Tok::Punct(';') => {
                self.pos += 1;
                Some(())
            }
            Tok::Punct('{') => {
                self.pos += 1;
                while self.peek()? != &Tok::Punct('}') {
                    self.stmt(scope)?;
                }
                self.pos += 1;
                Some(())
            }
            Tok::Ident(w) if w == "with" => {
                self.pos += 1;
                self.punct('(')?;
                let obj = self.ident()?;
                self.punct(')')?;
                self.stmt(Some(&obj))
            }
            Tok::Ident(_) => self.assign(scope),
            _ => None,
        }
    }

    /// `lvalue = literal`.
    fn assign(&mut self, scope: Option<&str>) -> Option<()> {
        let first = self.ident()?;
        if matches!(
            first.as_str(),
            "var" | "if" | "for" | "repeat" | "while" | "do" | "with" | "other" | "self" | "all"
        ) {
            return None;
        }
        let (owner, mut field) = if self.peek() == Some(&Tok::Punct('.')) {
            self.pos += 1;
            let f = self.ident()?;
            if first == "global" {
                (GmlOwner::Global, f)
            } else {
                (GmlOwner::Object(first), f)
            }
        } else {
            match scope {
                Some(obj) => (GmlOwner::Object(obj.to_string()), first),
                None => (GmlOwner::Unknown, first),
            }
        };
        if self.peek() == Some(&Tok::Punct('[')) {
            self.pos += 1;
            let Tok::Num(n) = self.next()? else {
                return None;
            };
            self.punct(']')?;
            field = format!("{field}[{n}]");
        }
        self.punct('=')?;
        let value = match self.next()? {
            Tok::Num(n) => GmlValue::Real(n),
            Tok::Str(s) => GmlValue::Str(s),
            Tok::Punct('-') => match self.next()? {
                Tok::Num(n) => GmlValue::Real(-n),
                _ => return None,
            },
            Tok::Ident(w) if w == "true" => GmlValue::Real(1.0),
            Tok::Ident(w) if w == "false" => GmlValue::Real(0.0),
            // `xprevious = x` after `x = 2250` in the same scope.
            Tok::Ident(w) => self
                .out
                .iter()
                .rev()
                .find(|a| a.owner == owner && a.field == w)
                .map(|a| a.value.clone())?,
            _ => return None,
        };
        // A value followed by an operator (`i*32`) is not a literal.
        if !matches!(
            self.peek(),
            None | Some(Tok::Punct(';' | '}')) | Some(Tok::Ident(_))
        ) {
            return None;
        }
        self.out.push(Assignment { owner, field, value });
        Some(())
    }
}

/// Rooms and objects of a package's `tests/trace-map.toml`, by original name.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceNames {
    /// Original room name to our room id.
    pub rooms: BTreeMap<String, String>,
    /// Original object name to our kind and whether the object is a variant (params).
    pub objects: BTreeMap<String, (String, bool)>,
}

impl TraceNames {
    /// Reads the `[rooms]` and `[objects]` tables.
    pub fn parse(text: &str) -> Result<TraceNames, String> {
        let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut names = TraceNames::default();
        if let Some(rooms) = table.get("rooms").and_then(|v| v.as_table()) {
            for (orig, v) in rooms {
                if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                    names.rooms.insert(orig.clone(), id.to_string());
                }
            }
        }
        if let Some(objects) = table.get("objects").and_then(|v| v.as_table()) {
            for (orig, v) in objects {
                if let Some(kind) = v.get("kind").and_then(|k| k.as_str())
                    && !kind.is_empty()
                {
                    let variant = v
                        .get("params")
                        .and_then(|p| p.as_table())
                        .is_some_and(|p| !p.is_empty());
                    names.objects.insert(orig.clone(), (kind.to_string(), variant));
                }
            }
        }
        Ok(names)
    }
}

/// The package folder for `pkg`: `pkg` itself when it is a folder, else the converter package
/// under `<research>/build/packages` whose `game.toml` has `id = pkg`.
pub fn find_package(pkg: &str, research: Option<&Path>) -> Option<PathBuf> {
    let p = Path::new(pkg);
    if p.join("game.toml").is_file() {
        return Some(p.to_path_buf());
    }
    let packages = research?.join("build/packages");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&packages)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    dirs.sort();
    dirs.into_iter().find(|d| package_id(d).as_deref() == Some(pkg))
}

/// The `id` of the package in folder `dir`.
pub fn package_id(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("game.toml")).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get("id")?.as_str().map(str::to_string)
}

/// The research repo root above a trace path (`.../research/specs/engines/gm8/traces/...`).
pub fn research_root_of(trace: &Path) -> Option<PathBuf> {
    trace
        .ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == "specs"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// `<game>/<trace>` for a path under a `traces` folder, without extension.
fn source_of(trace: &Path) -> Option<String> {
    let name = trace.file_name()?.to_str()?;
    let game = trace.parent()?.file_name()?.to_str()?;
    Some(format!("{game}/{name}"))
}

/// The trace path without a `.csv` or `.inputs` extension.
pub fn trace_stem(path: &Path) -> PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some("csv" | "inputs") => path.with_extension(""),
        _ => path.to_path_buf(),
    }
}

/// Imports one trace from its `.inputs` and `.csv` text. `names` maps original names (the
/// package's trace map), `None` keeps them as they are with a warning.
pub fn import_text(
    source: &str,
    inputs: &str,
    csv: &str,
    names: Option<&TraceNames>,
) -> Result<Imported, ToolError> {
    let command = input::trace_command(inputs).ok_or_else(|| {
        ToolError::Message(format!(
            "{source}: unsupported header: no `# iwc trace` command line"
        ))
    })?;
    let args = RecorderArgs::parse(&command).map_err(|e| ToolError::Message(format!("{source}: {e}")))?;
    let table = TraceTable::parse(csv).map_err(|e| ToolError::Message(format!("{source}.csv: {e}")))?;
    let keys = input::parse_key_inputs_lenient(inputs, &KeyMap::iwbtg())
        .map_err(|e| ToolError::Message(format!("{source}.inputs: {e}")))?;
    let trace_name = source.rsplit('/').next().unwrap_or(source);

    let mut header = vec![
        format!("Imported from research trace {source} by `iwbtg trace import`; finish by hand (plan 9.2)."),
        format!(
            "Recorder: --object {} --room {} --at {} --from {}{}{}{}",
            args.objects.join(","),
            args.room.as_deref().unwrap_or("-"),
            args.at,
            args.from,
            if args.stub_dlls { " --stub-dlls" } else { "" },
            match (&args.x, &args.y) {
                (Some(x), Some(y)) => format!(" --x {x} --y {y}"),
                _ => String::new(),
            },
            if args.gml.is_empty() {
                String::new()
            } else {
                format!(" ({} --gml)", args.gml.len())
            },
        ),
    ];
    let mut warnings = Vec::new();

    // Sort by the --gml code.
    let mut kind = TraceKind::DiffTarget;
    let mut reason = None;
    let mut assigns: Vec<(u32, Assignment)> = Vec::new();
    let mut notes = Vec::new();
    if trace_name.starts_with("zz-") {
        kind = TraceKind::SpecProbe;
        reason = Some("zz-* traces settle spec items".to_string());
    }
    for (frame, code) in &args.gml {
        match analyze_gml(code) {
            GmlUse::Probe(why) => {
                kind = TraceKind::SpecProbe;
                reason.get_or_insert(why);
            }
            GmlUse::ReadsSave if kind != TraceKind::SpecProbe => {
                kind = TraceKind::ProbeRead;
                reason.get_or_insert_with(|| {
                    format!("--gml at frame {frame} reads the save file into globals")
                });
            }
            GmlUse::ReadsSave => {}
            GmlUse::PlaceholderFile(name) => {
                notes.push(format!(
                    "--gml at frame {frame} writes an empty {name:?} the game checks for; the package's DLL model must match"
                ));
            }
            GmlUse::Assign(a) => assigns.extend(a.into_iter().map(|a| (*frame, a))),
        }
    }
    // v2 options: code every frame, or after a frame's events.
    let mut repeated_open = Vec::new();
    for (flag, codes) in [("--gml-every", &args.gml_every), ("--gml-end", &args.gml_end)] {
        for code in codes {
            match analyze_gml(code) {
                GmlUse::Probe(why) => {
                    kind = TraceKind::SpecProbe;
                    reason.get_or_insert(format!("{flag}: {why}"));
                }
                GmlUse::ReadsSave if kind != TraceKind::SpecProbe => {
                    kind = TraceKind::ProbeRead;
                    reason.get_or_insert_with(|| format!("{flag} reads the save file into globals"));
                }
                GmlUse::ReadsSave => {}
                GmlUse::Assign(_) | GmlUse::PlaceholderFile(_) => repeated_open.push(format!(
                    "{flag} code runs every frame or after a frame's events; setup runs once before a tick (OPEN)"
                )),
            }
        }
    }
    if kind == TraceKind::SpecProbe {
        return Ok(Imported {
            kind,
            reason,
            case: None,
            header,
            warnings,
        });
    }

    let room_of = |orig: &str| -> String {
        match names.and_then(|n| n.rooms.get(orig)) {
            Some(id) => id.clone(),
            None => orig.to_string(),
        }
    };
    let mut open: Vec<String> = Vec::new();
    // Original object name to our setup target or probe owner.
    let object_of = |orig: &str, open: &mut Vec<String>| -> String {
        match names.and_then(|n| n.objects.get(orig)) {
            Some((kind, variant)) => {
                if *variant {
                    open.push(format!(
                        "object {orig} is a variant of kind {kind} (params): a first-of or count over the kind is not the object (OPEN)"
                    ));
                }
                kind.clone()
            }
            None => {
                if names.is_some() {
                    open.push(format!("object {orig} is not in the trace map"));
                }
                orig.to_string()
            }
        }
    };
    if names.is_none() {
        warnings
            .push("no trace map found: room and object names are the original ones; map them by hand".into());
    }

    let room_orig = args.room.clone().unwrap_or_default();
    if args.room.is_none() {
        open.push(
            "no --room: the trace starts from the game's boot rooms, which are client UI (plan 4.2)".into(),
        );
    }
    let start_room = room_of(&room_orig);
    let object = object_of(args.object(), &mut open);
    if args.objects.len() > 1 {
        open.push(format!(
            "several traced objects ({}): a case traces one kind",
            args.objects.join(", ")
        ));
    }
    if !args.skip_objects.is_empty() {
        open.push(format!(
            "--skip-object {} changed the room; a case cannot leave placed instances out",
            args.skip_objects.join(",")
        ));
    }
    for log in &args.logs {
        notes.push(format!("recorder {log}: logs only, the CSV is the same"));
    }
    let target_of = |kind: &str| {
        if kind == "kid" {
            "kid".to_string()
        } else {
            kind.to_string()
        }
    };

    let mut difficulty = 0u8;
    let mut setup: Vec<SetupEntry> = Vec::new();
    for (frame, a) in assigns {
        let value = match &a.value {
            GmlValue::Real(r) => CaseValue::real(*r),
            GmlValue::Str(s) => CaseValue::Str(s.clone()),
        };
        match &a.owner {
            GmlOwner::Global if a.field == "difficulty" && frame < args.at => match &a.value {
                GmlValue::Real(r) if (0.0..=255.0).contains(r) && r.fract().abs() < f64::EPSILON => {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "checked range"
                    )]
                    let d = *r as u8;
                    difficulty = d;
                }
                _ => open.push(format!(
                    "global.difficulty = {:?} is not a difficulty index",
                    a.value
                )),
            },
            GmlOwner::Global => setup.push(SetupEntry {
                frame,
                what: SetupWhat::Global {
                    name: a.field.clone(),
                    value,
                },
            }),
            GmlOwner::Object(obj) => {
                let target = target_of(&object_of(obj, &mut open));
                push_field(&mut setup, frame, &target, &a.field, value);
            }
            GmlOwner::Unknown => open.push(format!(
                "--gml at frame {frame} assigns {} in the recorder's own instance, which setup cannot name",
                a.field
            )),
        }
    }
    if let (Some(x), Some(y)) = (&args.x, &args.y) {
        let (Ok(xv), Ok(yv)) = (x.parse::<f64>(), y.parse::<f64>()) else {
            return Err(ToolError::Message(format!(
                "{source}: --x {x:?} --y {y:?} are not numbers"
            )));
        };
        let target = target_of(&object);
        for (f, v) in [("x", xv), ("y", yv), ("xprevious", xv), ("yprevious", yv)] {
            push_field(&mut setup, args.at, &target, f, CaseValue::real(v));
        }
        notes.push("--x/--y: the recorder creates the object when none exists; setup only moves one".into());
    }
    if !keys.unmapped.is_empty() {
        open.push(format!(
            "the inputs press {}, which no client action stands for (games read actions, never raw keys; \
             the animus A/D nudge, trick.ad_align): OPEN until the game has an action for them",
            keys.unmapped.join(", ")
        ));
    }

    // Columns: our probe expression per original variable column.
    let mut vars = BTreeMap::new();
    let mut unprobed = BTreeMap::new();
    let empty_def = names_only_def();
    for column in table.columns.iter().skip(FIXED_COLUMNS.len()) {
        match translate_column(&column.name, &mut |o| object_of(o, &mut open), empty_def.as_ref()) {
            Ok(ours) if ours == column.name => {}
            Ok(ours) => {
                vars.insert(column.name.clone(), ours);
            }
            Err(why) => {
                unprobed.insert(column.name.clone(), why);
            }
        }
    }
    if !unprobed.is_empty() {
        warnings.push(format!(
            "{} column(s) have no probe column and are not compared",
            unprobed.len()
        ));
    }
    if kind == TraceKind::ProbeRead {
        open.push(
            "probe read: map the decoded globals to save.* columns in [vars], then drop this line".into(),
        );
    }

    // Frame order; the header's order within a frame (a stable sort).
    setup.sort_by_key(|s| s.frame);
    let mut rooms = BTreeMap::new();
    if !room_orig.is_empty() && start_room != room_orig {
        rooms.insert(start_room.clone(), room_orig.clone());
    }
    open.sort();
    open.dedup();
    let case = TraceCase {
        source: source.to_string(),
        kind: if kind == TraceKind::ProbeRead {
            CaseKind::ProbeRead
        } else {
            CaseKind::Diff
        },
        open: (!open.is_empty()).then(|| open.join("; ")),
        start: CaseStart {
            room: start_room,
            difficulty,
        },
        at: args.at,
        first_frame: args.from,
        end_frame: args.frames,
        object,
        original_object: args.object().to_string(),
        dll_mode: if args.stub_dlls {
            CaseDllMode::Stub
        } else {
            CaseDllMode::Model
        },
        seed: args.seed,
        setup,
        vars,
        ignore: DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect(),
        unprobed,
        rooms,
        notes,
    };
    if args.from < args.at {
        header.push(format!(
            "Frames {}..{} ran before the room jump (boot rooms); they are not compared.",
            args.from,
            args.at.saturating_sub(1)
        ));
    }
    Ok(Imported {
        kind,
        reason,
        case: Some(case),
        header,
        warnings,
    })
}

/// A definition with no kinds and no variables, so `registry::resolve` tells built-in variables
/// the probe cannot read (`bbox_left`) from names a package may define (`djump`).
fn names_only_def() -> Option<iwbtg_sim::GameDef> {
    let mut b = iwbtg_sim::def::testing::GameDefBuilder::new("import");
    b.room(iwbtg_sim::def::RoomDef::new("room", 1, 1));
    b.build().ok()
}

/// Adds `field = value` to the frame's setup line for `target`, or starts one.
fn push_field(setup: &mut Vec<SetupEntry>, frame: u32, target: &str, field: &str, value: CaseValue) {
    if let Some(SetupEntry {
        what: SetupWhat::Fields { set, .. },
        ..
    }) = setup
        .iter_mut()
        .rev()
        .find(|s| s.frame == frame && matches!(&s.what, SetupWhat::Fields { target: t, .. } if t == target))
    {
        set.insert(field.to_string(), value);
        return;
    }
    let mut set = BTreeMap::new();
    set.insert(field.to_string(), value);
    setup.push(SetupEntry {
        frame,
        what: SetupWhat::Fields {
            target: target.to_string(),
            set,
        },
    });
}

/// An original column expression in our names, or why there is no probe column for it.
fn translate_column(
    name: &str,
    object_of: &mut dyn FnMut(&str) -> String,
    empty_def: Option<&iwbtg_sim::GameDef>,
) -> Result<String, String> {
    let plain_field = |f: &str| -> Result<(), String> {
        match empty_def.map(|d| registry::resolve(f, d)) {
            Some(None) => Err(format!("no probe field for `{f}` (OPEN)")),
            _ => Ok(()),
        }
    };
    if let Some(arg) = call_arg(name, "instance_number") {
        if arg.contains(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            return Err("instance_number of an expression".into());
        }
        let kind = object_of(arg);
        return Ok(format!("instance_number({kind})"));
    }
    for func in [
        "keyboard_check_pressed",
        "keyboard_check_released",
        "keyboard_check_direct",
        "keyboard_check",
    ] {
        if let Some(arg) = call_arg(name, func) {
            let code = key_arg(arg).ok_or_else(|| format!("key {arg:?} is not a key the recorder names"))?;
            if matches!(code, input::vk::LEFT_SHIFT | input::vk::RIGHT_SHIFT) {
                return Err("a one-hand Shift read; actions fold both hands (input.dual_shift)".into());
            }
            let action = KeyMap::iwbtg()
                .action(code)
                .ok_or_else(|| format!("key {arg} has no client action"))?;
            return Ok(format!("{func}({})", action.name()));
        }
    }
    if name.contains('(') {
        return Err("a function call; the probe reads variables only (OPEN)".into());
    }
    if let Some(g) = name.strip_prefix("global.") {
        if g.contains(['.', '[']) {
            return Err("a global array or instance path".into());
        }
        if !g.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("an expression; the probe reads variables only (OPEN)".into());
        }
        return Ok(name.to_string());
    }
    if let Some((owner, field)) = name.split_once('.') {
        if owner.is_empty() || field.contains('.') {
            return Err("an instance path the probe cannot follow".into());
        }
        plain_field(field)?;
        let kind = object_of(owner);
        let owner = if kind == "kid" { "kid".to_string() } else { kind };
        return Ok(format!("{owner}.{field}"));
    }
    if name.contains(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']')) {
        return Err("an expression; the probe reads variables only (OPEN)".into());
    }
    plain_field(name)?;
    Ok(name.to_string())
}

/// `f(arg)` to `arg`.
fn call_arg<'e>(expr: &'e str, func: &str) -> Option<&'e str> {
    expr.strip_prefix(func)?.strip_prefix('(')?.strip_suffix(')')
}

/// A key argument as GML writes it: `16`, `vk_shift`, `ord('A')`.
fn key_arg(arg: &str) -> Option<u8> {
    use input::vk;
    let arg = arg.trim();
    if let Ok(n) = arg.parse::<u8>() {
        return Some(n);
    }
    if let Some(ch) = call_arg(arg, "ord") {
        let ch = ch.trim_matches(|c| c == '\'' || c == '"');
        let mut chars = ch.chars();
        let c = chars.next()?;
        return (chars.next().is_none() && c.is_ascii_alphanumeric()).then(|| c.to_ascii_uppercase() as u8);
    }
    Some(match arg {
        "vk_shift" => vk::SHIFT,
        "vk_control" => vk::CTRL,
        "vk_alt" => vk::ALT,
        "vk_lshift" => vk::LEFT_SHIFT,
        "vk_rshift" => vk::RIGHT_SHIFT,
        "vk_left" => vk::LEFT,
        "vk_right" => vk::RIGHT,
        "vk_up" => vk::UP,
        "vk_down" => vk::DOWN,
        "vk_enter" => vk::ENTER,
        "vk_escape" => vk::ESCAPE,
        "vk_space" => vk::SPACE,
        "vk_f2" => vk::F2,
        _ => return None,
    })
}

/// `iwbtg trace import <trace> <pkg>`: imports the trace, writes the case for diff targets and
/// probe reads under `cases/<pkg id>/`, and prints the case path and `kind: ...`.
pub fn main(
    trace: &Path,
    pkg: &str,
    research: Option<&Path>,
    cases: &Path,
    force: bool,
) -> Result<(), ToolError> {
    let stem = trace_stem(trace);
    let source = source_of(&stem).ok_or_else(|| {
        ToolError::Message(format!(
            "{}: not a trace path (<traces>/<game>/<trace>)",
            trace.display()
        ))
    })?;
    let read = |ext: &str| {
        let p = stem.with_extension(ext);
        std::fs::read_to_string(&p).map_err(|e| ToolError::Message(format!("{}: {e}", p.display())))
    };
    let (inputs, csv) = (read("inputs")?, read("csv")?);
    let research = research
        .map(Path::to_path_buf)
        .or_else(|| research_root_of(&stem));
    let package = find_package(pkg, research.as_deref());
    let names = match &package {
        Some(dir) => {
            let path = dir.join("tests/trace-map.toml");
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(
                    TraceNames::parse(&text)
                        .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?,
                ),
                Err(_) => None,
            }
        }
        None => None,
    };
    let pkg_id = package
        .as_deref()
        .and_then(package_id)
        .unwrap_or_else(|| pkg.to_string());
    let imported = import_text(&source, &inputs, &csv, names.as_ref())?;
    if let Some(case) = &imported.case {
        let name = source.rsplit('/').next().unwrap_or(&source);
        let dir = cases.join(&pkg_id);
        let path = dir.join(format!("{name}.toml"));
        if path.exists() && !force {
            // Cases are finished by hand: never overwrite one without --force.
            println!("case: {} (exists, kept; pass --force to overwrite)", path.display());
        } else {
            std::fs::create_dir_all(&dir).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
            std::fs::write(&path, case.to_toml(&imported.header))
                .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
            println!("case: {}", path.display());
        }
    }
    println!("kind: {}", imported.kind.label());
    if let Some(reason) = &imported.reason {
        println!("reason: {reason}");
    }
    if let Some(open) = imported.case.as_ref().and_then(|c| c.open.as_ref()) {
        println!("open: {open}");
    }
    for w in &imported.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_follow_posix_quoting() {
        assert_eq!(
            shell_words(r#"iwc trace --vars 'a,b' --gml '1:global.log='"'"''"'"';' x\ y "q\"r""#)
                .expect("words"),
            vec![
                "iwc",
                "trace",
                "--vars",
                "a,b",
                "--gml",
                "1:global.log='';",
                "x y",
                "q\"r"
            ]
        );
        assert_eq!(shell_words("a 'b\nc' d").expect("words"), vec!["a", "b\nc", "d"]);
        assert!(shell_words("a 'b").is_err());
        assert_eq!(shell_words("  ").expect("words"), Vec::<String>::new());
        assert_eq!(shell_words("a ''").expect("words"), vec!["a", ""]);
    }

    #[test]
    fn recorder_args() {
        let a = RecorderArgs::parse(
            "iwc trace --game 339 --inputs t.inputs --object bullet --vars 'alarm[0],player.x' --room rTemplate --from 2 \
             --gml '3:with(player){x=96;y=567.4}' --stub-dlls --out t",
        )
        .expect("args");
        assert_eq!(a.object(), "bullet");
        assert_eq!(a.room.as_deref(), Some("rTemplate"));
        assert_eq!((a.at, a.from, a.stub_dlls), (2, 2, true));
        assert_eq!(a.gml, vec![(3, "with(player){x=96;y=567.4}".to_string())]);
        assert!(RecorderArgs::parse("iwc trace --object x --bogus 1").is_err());
        assert!(RecorderArgs::parse("iwc trace --room r").is_err());
        assert!(RecorderArgs::parse("other").is_err());
    }

    fn assigns(code: &str) -> Vec<Assignment> {
        match analyze_gml(code) {
            GmlUse::Assign(a) => a,
            other => panic!("{code}: {other:?}"),
        }
    }

    #[test]
    fn gml_sorting() {
        let a = assigns("with (player) { x = 2250; y = 1175.2; xprevious = x; yprevious = y; }");
        assert_eq!(a.len(), 4);
        assert_eq!(a[2].field, "xprevious");
        assert_eq!(a[2].value, GmlValue::Real(2250.0));
        assert_eq!(a[3].owner, GmlOwner::Object("player".into()));
        let a = assigns("1:global.difficulty=0; global.savenum=1;".trim_start_matches("1:"));
        assert_eq!(a[1].owner, GmlOwner::Global);
        let a = assigns("with(objBossSampleBuffed){alarm[11]=-0.5}");
        assert_eq!(a[0].field, "alarm[11]");
        assert_eq!(a[0].value, GmlValue::Real(-0.5));
        assert_eq!(assigns("player.s=1")[0].owner, GmlOwner::Object("player".into()));
        assert_eq!(assigns("global.rs=''")[0].value, GmlValue::Str(String::new()));
        assert_eq!(assigns("global.frozen = false;")[0].value, GmlValue::Real(0.0));
        for probe in [
            "global.log=''; object_event_add(world,3,1,\"x\")",
            "with(cherry){if(speed<0){direction+=30}}",
            "instance_create(320, 288, spikeUp);",
            "killPlayer(); with(objDeathSound){start_vol=10000}",
            "var i; for(i=0;i<10;i+=1) instance_create(i*32,576,block);",
            "with(player){hspeed=0; x=100+1}",
        ] {
            assert!(matches!(analyze_gml(probe), GmlUse::Probe(_)), "{probe}");
        }
        assert_eq!(
            analyze_gml("var f; f=file_bin_open(\"save1\",0); global.b=file_bin_read_byte(f);"),
            GmlUse::ReadsSave
        );
        assert_eq!(
            analyze_gml("__f=file_text_open_write('GMFMODSimple.dll'); file_text_close(__f)"),
            GmlUse::PlaceholderFile("GMFMODSimple.dll".into())
        );
    }

    #[test]
    fn columns_translate_through_the_map() {
        let def = names_only_def();
        assert!(def.is_some());
        let mut map = |o: &str| match o {
            "player" => "kid".to_string(),
            "bullet" => "@bullet".to_string(),
            o => o.to_string(),
        };
        let mut t = |n: &str| translate_column(n, &mut map, def.as_ref());
        assert_eq!(t("player.x"), Ok("kid.x".into()));
        assert_eq!(
            t("instance_number(bullet)"),
            Ok("instance_number(@bullet)".into())
        );
        assert_eq!(t("keyboard_check(16)"), Ok("keyboard_check(jump)".into()));
        assert_eq!(
            t("keyboard_check_direct(vk_right)"),
            Ok("keyboard_check_direct(right)".into())
        );
        assert_eq!(t("alarm[0]"), Ok("alarm[0]".into()));
        assert_eq!(t("djump"), Ok("djump".into()));
        assert_eq!(t("global.grav"), Ok("global.grav".into()));
        for bad in [
            "bbox_left",
            "keyboard_check_direct(vk_lshift)",
            "floor(image_index)",
            "global.pa.x",
            "global.death[1]",
            "execute_string(global.q1)",
            "global.b0 div 256",
            "player.bbox_left",
        ] {
            assert!(t(bad).is_err(), "{bad}");
        }
    }

    const T_ALARM: &str = "# trace t-alarm-1.csv: kaenbyou (game #339), OpenGMK unknown + iwc-trace.patch\n\
        # iwc trace --game 339 --inputs t-alarm-1.inputs --object bullet --vars 'alarm[0],player.x' --room rTemplate --from 2 \
        --gml '3:with(player){x=96;y=567.4;xprevious=96;yprevious=567.4}' --gml '1:global.difficulty=1;global.grav=0' --stub-dlls --out t-alarm-1\n\
        frame 20: press Z\nframe 21: release Z\nframe 80: end\n";
    const T_ALARM_CSV: &str = "frame,room,object,id,x,y,xprevious,yprevious,hspeed,vspeed,speed,direction,gravity,gravity_direction,friction,image_index,sprite_index,sprite_name,\"alarm[0]\",\"player.x\",\"bbox_left\"\n";

    #[test]
    fn imports_a_diff_target_with_setup() {
        let names = TraceNames::parse(
            "[rooms]\n\"rTemplate\" = { id = \"template\" }\n[objects]\n\"player\" = { kind = \"kid\" }\n\"bullet\" = { kind = \"@bullet\" }\n",
        )
        .expect("map");
        let i = import_text("kaenbyou/t-alarm-1", T_ALARM, T_ALARM_CSV, Some(&names)).expect("import");
        assert_eq!(i.kind, TraceKind::DiffTarget);
        let case = i.case.expect("case");
        assert_eq!(case.open, None);
        assert_eq!(case.start.room, "template");
        assert_eq!(case.start.difficulty, 1);
        assert_eq!(case.object, "@bullet");
        assert_eq!(case.original_object, "bullet");
        assert_eq!(case.dll_mode, CaseDllMode::Stub);
        assert_eq!(case.vars.get("player.x").map(String::as_str), Some("kid.x"));
        assert!(!case.vars.contains_key("alarm[0]"), "same name needs no entry");
        assert!(case.unprobed.contains_key("bbox_left"));
        assert_eq!(case.rooms.get("template").map(String::as_str), Some("rTemplate"));
        assert_eq!(case.setup.len(), 2);
        assert_eq!(
            case.setup[0].what,
            SetupWhat::Global {
                name: "grav".into(),
                value: CaseValue::real(0.0)
            }
        );
        let SetupWhat::Fields { target, set } = &case.setup[1].what else {
            panic!("fields");
        };
        assert_eq!((target.as_str(), case.setup[1].frame, set.len()), ("kid", 3, 4));
        // The skeleton writes and reads back.
        let text = case.to_toml(&i.header);
        assert_eq!(TraceCase::parse(&text).expect("parse"), case);
        // Without a trace map the names stay original and the importer warns.
        let i = import_text("kaenbyou/t-alarm-1", T_ALARM, T_ALARM_CSV, None).expect("import");
        assert!(!i.warnings.is_empty());
        assert_eq!(i.case.expect("case").object, "bullet");
    }

    #[test]
    fn spec_probes_and_unmapped_keys() {
        let probe = T_ALARM.replace(
            "--gml '1:global.difficulty=1;global.grav=0'",
            "--gml '1:object_event_add(world,0,0,\"x\")'",
        );
        let i = import_text("kaenbyou/t-x", &probe, T_ALARM_CSV, None).expect("import");
        assert_eq!(i.kind, TraceKind::SpecProbe);
        assert!(i.case.is_none());
        let i = import_text("kaenbyou/zz-anything", T_ALARM, T_ALARM_CSV, None).expect("import");
        assert_eq!(i.kind, TraceKind::SpecProbe);
        let ad = T_ALARM.replace("frame 20: press Z", "frame 20: press D");
        let i = import_text("animus/a-ad-nudge", &ad, T_ALARM_CSV, None).expect("import");
        assert_eq!(i.kind, TraceKind::DiffTarget);
        let open = i.case.and_then(|c| c.open).expect("open");
        assert!(open.contains("press D"), "{open}");
        assert!(import_text("x/y", "frame 1: end\n", T_ALARM_CSV, None).is_err());
    }

    /// Every research trace imports into one of the three kinds, with the package's trace map
    /// where the converter package exists; the plan's named traces get their kinds; each case
    /// skeleton writes and reads back. Skips when IWBTG_RESEARCH is unset.
    #[test]
    fn every_research_trace_imports() {
        let Some(root) = std::env::var_os("IWBTG_RESEARCH").map(PathBuf::from) else {
            eprintln!("skipped: IWBTG_RESEARCH is not set");
            return;
        };
        let traces = root.join("specs/engines/gm8/traces");
        let mut games: Vec<PathBuf> = std::fs::read_dir(&traces)
            .expect("traces dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        games.sort();
        // Research game folders use the trace names; the converter packages their ids.
        let package_ids = [
            ("rely-the-needle-4-2", "rely_the_needle"),
            ("clear-the-easy-miku-2", "clear_the_easy_miku_2"),
            ("deride-kotaro-from-aslant", "deride_kotaro_from_aslant"),
        ];
        let mut kinds: BTreeMap<String, TraceKind> = BTreeMap::new();
        for game in &games {
            let game_name = game.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let pkg = package_ids
                .iter()
                .find(|(g, _)| *g == game_name)
                .map_or_else(|| game_name.replace('-', "_"), |(_, p)| p.to_string());
            let names = find_package(&pkg, Some(&root))
                .and_then(|d| std::fs::read_to_string(d.join("tests/trace-map.toml")).ok())
                .map(|t| TraceNames::parse(&t).expect("trace map"));
            let mut stems: Vec<PathBuf> = std::fs::read_dir(game)
                .expect("game dir")
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "inputs"))
                .map(|p| p.with_extension(""))
                .collect();
            stems.sort();
            for stem in stems {
                let source = source_of(&stem).expect("source");
                let read = |ext: &str| std::fs::read_to_string(format!("{}.{ext}", stem.display())).expect("read");
                let i = import_text(&source, &read("inputs"), &read("csv"), names.as_ref())
                    .unwrap_or_else(|e| panic!("{source}: {e}"));
                if let Some(case) = &i.case {
                    let text = case.to_toml(&i.header);
                    let back = TraceCase::parse(&text).unwrap_or_else(|e| panic!("{source}: {e}\n{text}"));
                    assert_eq!(&back, case, "{source}");
                } else {
                    assert_eq!(i.kind, TraceKind::SpecProbe, "{source}");
                }
                kinds.insert(source, i.kind);
            }
        }
        assert!(kinds.len() > 400, "only {} traces", kinds.len());
        let kind = |s: &str| *kinds.get(s).unwrap_or_else(|| panic!("no trace {s}"));
        for s in ["kaenbyou/t-alarm-1", "kaenbyou/stand", "rely-the-needle-4-2/stand", "rely-the-needle-4-2/djump"] {
            assert_eq!(kind(s), TraceKind::DiffTarget, "{s}");
        }
        for s in [
            "rely-the-needle-4-2/t-ord-1",
            "rely-the-needle-4-2/t-evo-1",
            "rely-the-needle-4-2/t-room-1",
            "rely-the-needle-4-2/tr-room-01-a",
        ] {
            assert_eq!(kind(s), TraceKind::SpecProbe, "{s}");
        }
        assert_eq!(kind("kaenbyou/famb-save-restart-a"), TraceKind::ProbeRead);
        let count = |k: TraceKind| kinds.values().filter(|v| **v == k).count();
        eprintln!(
            "imported {} traces: {} diff targets, {} spec probes, {} probe reads",
            kinds.len(),
            count(TraceKind::DiffTarget),
            count(TraceKind::SpecProbe),
            count(TraceKind::ProbeRead)
        );
    }
}
