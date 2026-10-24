//! Trace rows for comparison with the originals (docs/architecture.md, "Comparing traces with
//! the original").
//!
//! A [`TraceTable`] holds one CSV in the research recorder's format: a header, then one row per
//! frame for each traced instance. [`TraceTable::parse`] reads it, [`TraceTable::to_csv`] writes
//! it back byte for byte, and [`diff`] compares two tables bit for bit.
//!
//! The recorder (`research/tooling/opengmk-trace/iwc-trace.patch`) writes:
//! - lines ending in `\n`, including the last one;
//! - the 18 [`FIXED_COLUMNS`] bare in the header, then each game variable name quoted;
//! - integers (`frame`, `id`, `sprite_index`) in plain decimal;
//! - reals as Rust's `{:?}`, the shortest decimal that reads back to the same f64
//!   (`81.0`, `-7.347880794884119e-17`);
//! - strings always quoted, with `"` doubled;
//! - an empty field when a game variable could not be read.
//!
//! This module does no IO: callers pass and receive text.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

/// The recorder's built-in columns, in order. Game variable columns follow them.
pub const FIXED_COLUMNS: [&str; 18] = [
    "frame",
    "room",
    "object",
    "id",
    "x",
    "y",
    "xprevious",
    "yprevious",
    "hspeed",
    "vspeed",
    "speed",
    "direction",
    "gravity",
    "gravity_direction",
    "friction",
    "image_index",
    "sprite_index",
    "sprite_name",
];

/// Column that numbers the frame. Rows are matched on it.
pub const FRAME_COLUMN: &str = "frame";

/// Column that names the traced object. Rows are matched on it.
pub const OBJECT_COLUMN: &str = "object";

/// Frames of context shown on each side of a difference by default.
pub const DEFAULT_CONTEXT: u32 = 5;

/// Error reading or comparing a trace. Bad input never panics; it ends up here.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TraceError {
    /// The CSV text is malformed. `line` is 1-based.
    #[error("trace line {line}: {message}")]
    Parse { line: u32, message: String },
    /// The table is well-formed CSV but cannot be used (missing key column, ragged row, ...).
    #[error("trace: {0}")]
    Invalid(String),
}

/// One cell of a trace.
#[derive(Debug, Clone)]
pub enum Value {
    /// Unquoted decimal integer (`frame`, `id`, `sprite_index`).
    Int(i64),
    /// Unquoted real, parsed to the exact f64 its shortest decimal names.
    Real(f64),
    /// Quoted string, unescaped.
    Str(String),
    /// Empty field: the recorder could not read the variable.
    Empty,
}

impl Value {
    /// The value as a GM real, for numbers. Integers in traces are small, so the cast is exact.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            Value::Str(_) | Value::Empty => None,
        }
    }

    /// The value as the recorder writes it (strings unquoted here).
    pub fn decimal(&self) -> String {
        match self {
            Value::Int(i) => i.to_string(),
            Value::Real(r) => format!("{r:?}"),
            Value::Str(s) => s.clone(),
            Value::Empty => String::new(),
        }
    }

    /// The f64 bit pattern of a number, as `0x` and 16 upper-case hex digits.
    pub fn hex_bits(&self) -> Option<String> {
        self.as_f64().map(|r| format!("0x{:016X}", r.to_bits()))
    }

    /// Bit-exact equality: numbers compare as f64 bits (so `0.0 != -0.0` and a NaN equals only
    /// the same NaN), strings compare as text, and kinds never mix except Int with Real.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Empty, Value::Empty) => true,
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
                _ => false,
            },
        }
    }

    /// Writes the cell in the recorder's CSV form.
    fn write_csv(&self, out: &mut String) {
        match self {
            Value::Int(i) => out.push_str(&i.to_string()),
            Value::Real(r) => out.push_str(&format!("{r:?}")),
            Value::Str(s) => push_quoted(out, s),
            Value::Empty => {}
        }
    }
}

/// Bit-exact, as [`Value::same`].
impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        self.same(other)
    }
}

impl Eq for Value {}

/// ULP distance between two f64s: how many representable doubles lie between them, plus one.
/// `0.0` and `-0.0` are 0 apart. `None` when either is NaN.
pub fn ulp_distance(a: f64, b: f64) -> Option<u64> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    Some(ordered_key(a).abs_diff(ordered_key(b)))
}

/// Maps f64 bits to an integer that grows with the value, so adjacent doubles differ by 1.
fn ordered_key(r: f64) -> i64 {
    let magnitude = (r.to_bits() & 0x7FFF_FFFF_FFFF_FFFF) as i64;
    if r.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    }
}

/// One header column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Column name, unescaped (`x`, `jump[1]`).
    pub name: String,
    /// Whether the header writes the name in quotes. The recorder quotes game variables only.
    pub quoted: bool,
}

/// Line ending of a trace file. The recorder writes `\n`; a Windows checkout may turn it into
/// `\r\n`, so the parser remembers which one it saw and the writer repeats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// A whole trace: header columns and rows, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceTable {
    pub columns: Vec<Column>,
    /// Each row has exactly one value per column.
    pub rows: Vec<Vec<Value>>,
    pub line_ending: LineEnding,
}

impl TraceTable {
    /// An empty table with the recorder's [`FIXED_COLUMNS`] followed by `game_vars`, quoted as
    /// the recorder quotes them. `iwbtg trace run` builds our side with this.
    pub fn new(game_vars: &[&str]) -> TraceTable {
        let fixed = FIXED_COLUMNS.iter().map(|name| Column {
            name: (*name).to_string(),
            quoted: false,
        });
        let vars = game_vars.iter().map(|name| Column {
            name: (*name).to_string(),
            quoted: true,
        });
        TraceTable {
            columns: fixed.chain(vars).collect(),
            rows: Vec::new(),
            line_ending: LineEnding::Lf,
        }
    }

    /// Index of the column called `name`.
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    /// Appends a row, checking it has one value per column.
    pub fn push_row(&mut self, row: Vec<Value>) -> Result<(), TraceError> {
        if row.len() != self.columns.len() {
            return Err(TraceError::Invalid(format!(
                "row has {} values, header has {} columns",
                row.len(),
                self.columns.len()
            )));
        }
        self.rows.push(row);
        Ok(())
    }

    /// Parses recorder CSV. Unquoted cells become [`Value::Int`] when they are a plain decimal
    /// integer, [`Value::Real`] otherwise (Rust's parse is correctly rounded, so the f64 is
    /// exact), and [`Value::Empty`] when empty. Quoted cells become [`Value::Str`].
    pub fn parse(text: &str) -> Result<TraceTable, TraceError> {
        let mut reader = Reader {
            bytes: text.as_bytes(),
            pos: 0,
            line: 1,
            line_ending: None,
        };
        let header = match reader.record()? {
            Some(header) => header,
            None => {
                return Err(TraceError::Parse {
                    line: 1,
                    message: "empty file, expected a header".to_string(),
                });
            }
        };
        let mut columns = Vec::with_capacity(header.len());
        for (index, raw) in header.into_iter().enumerate() {
            if raw.text.is_empty() {
                return Err(TraceError::Parse {
                    line: 1,
                    message: format!("column {} has no name", index + 1),
                });
            }
            columns.push(Column {
                name: raw.text,
                quoted: raw.quoted,
            });
        }
        let mut rows = Vec::new();
        loop {
            let line = reader.line;
            let Some(record) = reader.record()? else {
                break;
            };
            if record.len() != columns.len() {
                return Err(TraceError::Parse {
                    line,
                    message: format!(
                        "row has {} fields, header has {} columns",
                        record.len(),
                        columns.len()
                    ),
                });
            }
            let mut row = Vec::with_capacity(record.len());
            for raw in record {
                row.push(raw.into_value(line)?);
            }
            rows.push(row);
        }
        Ok(TraceTable {
            columns,
            rows,
            line_ending: reader.line_ending.unwrap_or_default(),
        })
    }

    /// Writes the table in the recorder's format. Re-writing a parsed recorder file gives the
    /// same bytes.
    pub fn to_csv(&self) -> String {
        let eol = self.line_ending.as_str();
        let mut out = String::new();
        for (index, column) in self.columns.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            if column.quoted {
                push_quoted(&mut out, &column.name);
            } else {
                out.push_str(&column.name);
            }
        }
        out.push_str(eol);
        for row in &self.rows {
            for (index, value) in row.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                value.write_csv(&mut out);
            }
            out.push_str(eol);
        }
        out
    }

    /// Checks every row has one value per column, so later indexing cannot go wrong.
    fn check_shape(&self, side: &str) -> Result<(), TraceError> {
        for (index, row) in self.rows.iter().enumerate() {
            if row.len() != self.columns.len() {
                return Err(TraceError::Invalid(format!(
                    "{side}: row {} has {} values, header has {} columns",
                    index + 1,
                    row.len(),
                    self.columns.len()
                )));
            }
        }
        Ok(())
    }
}

/// Writes `s` quoted, doubling any `"`, as the recorder's `csv_str` does.
fn push_quoted(out: &mut String, s: &str) {
    out.push('"');
    out.push_str(&s.replace('"', "\"\""));
    out.push('"');
}

/// A field as read from the file, before typing.
struct RawField {
    text: String,
    quoted: bool,
}

impl RawField {
    fn into_value(self, line: u32) -> Result<Value, TraceError> {
        if self.quoted {
            return Ok(Value::Str(self.text));
        }
        if self.text.is_empty() {
            return Ok(Value::Empty);
        }
        if is_plain_integer(&self.text)
            && let Ok(i) = self.text.parse::<i64>()
        {
            return Ok(Value::Int(i));
        }
        match self.text.parse::<f64>() {
            Ok(r) => Ok(Value::Real(r)),
            Err(_) => Err(TraceError::Parse {
                line,
                message: format!("{:?} is neither a number nor a quoted string", self.text),
            }),
        }
    }
}

/// `-?[0-9]+`: how the recorder writes integers. Reals always carry `.`, `e`, `inf` or `NaN`.
fn is_plain_integer(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Hand-written CSV reader for the recorder's dialect (RFC 4180 quoting, `\n` or `\r\n`).
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// 1-based line of `pos`, counting newlines inside quoted fields too.
    line: u32,
    /// Ending of the first line; every later line must match it.
    line_ending: Option<LineEnding>,
}

impl Reader<'_> {
    fn error(&self, message: impl Into<String>) -> TraceError {
        TraceError::Parse {
            line: self.line,
            message: message.into(),
        }
    }

    /// Reads one record. `None` at end of input. The last line may lack its line ending.
    fn record(&mut self) -> Result<Option<Vec<RawField>>, TraceError> {
        if self.pos >= self.bytes.len() {
            return Ok(None);
        }
        let mut fields = Vec::new();
        loop {
            fields.push(self.field()?);
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                None => return Ok(Some(fields)),
                Some(b'\n') => {
                    self.pos += 1;
                    self.end_line(LineEnding::Lf)?;
                    return Ok(Some(fields));
                }
                Some(b'\r') if self.bytes.get(self.pos + 1) == Some(&b'\n') => {
                    self.pos += 2;
                    self.end_line(LineEnding::CrLf)?;
                    return Ok(Some(fields));
                }
                Some(_) => return Err(self.error("unexpected character after a field")),
            }
        }
    }

    fn end_line(&mut self, ending: LineEnding) -> Result<(), TraceError> {
        match self.line_ending {
            None => self.line_ending = Some(ending),
            Some(seen) if seen != ending => {
                return Err(self.error("mixed line endings (\\n and \\r\\n)"));
            }
            Some(_) => {}
        }
        self.line = self.line.saturating_add(1);
        Ok(())
    }

    /// Reads one field, stopping before the `,`, line ending or end of input that follows it.
    fn field(&mut self) -> Result<RawField, TraceError> {
        if self.bytes.get(self.pos) == Some(&b'"') {
            self.pos += 1;
            let mut text = Vec::new();
            loop {
                match self.bytes.get(self.pos) {
                    None => return Err(self.error("quoted field is not closed")),
                    Some(b'"') if self.bytes.get(self.pos + 1) == Some(&b'"') => {
                        text.push(b'"');
                        self.pos += 2;
                    }
                    Some(b'"') => {
                        self.pos += 1;
                        break;
                    }
                    Some(&b) => {
                        if b == b'\n' {
                            self.line = self.line.saturating_add(1);
                        }
                        text.push(b);
                        self.pos += 1;
                    }
                }
            }
            // The input is a &str and we split only at ASCII bytes, so this cannot fail.
            let text = String::from_utf8(text).map_err(|_| self.error("invalid UTF-8"))?;
            return Ok(RawField { text, quoted: true });
        }
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b',' | b'\n' => break,
                b'\r' if self.bytes.get(self.pos + 1) == Some(&b'\n') => break,
                b'"' => return Err(self.error("quote inside an unquoted field")),
                _ => self.pos += 1,
            }
        }
        let text = self
            .bytes
            .get(start..self.pos)
            .and_then(|b| std::str::from_utf8(b).ok())
            .ok_or_else(|| self.error("invalid UTF-8"))?;
        Ok(RawField {
            text: text.to_string(),
            quoted: false,
        })
    }
}

/// How [`diff`] lines up two traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    /// Field names in `a` mapped to their names in `b` (from `trace-map.toml`). Unlisted fields
    /// keep their name.
    pub rename: BTreeMap<String, String>,
    /// Fields of `a` not compared (for example `id`, which never matches the original's).
    pub ignore: BTreeSet<String>,
    /// `b`'s frame number for `a`'s frame `f` is `f + frame_offset`.
    pub frame_offset: i64,
    /// Frames of context on each side of a difference.
    pub context: u32,
    /// Keep going after the first difference and fill [`TraceDiff::summary`].
    pub summary: bool,
}

impl Default for DiffOptions {
    fn default() -> DiffOptions {
        DiffOptions {
            rename: BTreeMap::new(),
            ignore: BTreeSet::new(),
            frame_offset: 0,
            context: DEFAULT_CONTEXT,
            summary: false,
        }
    }
}

/// Which trace something belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// Identifies one row: frame (in `a`'s numbering), object, and the instance's position among
/// that object's rows in that frame (0 for the first).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowKey {
    pub frame: i64,
    pub object: String,
    pub instance: u32,
}

/// One frame of context: the field's value in each trace, if that row exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFrame {
    pub frame: i64,
    pub a: Option<Value>,
    pub b: Option<Value>,
}

/// A field whose values differ in matched rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDiff {
    pub row: RowKey,
    /// Field name in `a`.
    pub field: String,
    /// Field name in `b` (differs only through [`DiffOptions::rename`]).
    pub field_b: String,
    pub a: Value,
    pub b: Value,
    /// ULP distance when both values are numbers (and not NaN).
    pub ulp: Option<u64>,
    /// The field around this frame, oldest first, up to [`DiffOptions::context`] frames each
    /// side, including this frame.
    pub context: Vec<ContextFrame>,
}

/// The first thing that differs between two traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// Both traces have the row, and a field differs.
    Field(FieldDiff),
    /// Only one trace has the row. `missing_in` names the trace without it.
    MissingRow { row: RowKey, missing_in: Side },
}

impl Difference {
    /// Frame of the difference, in `a`'s numbering.
    pub fn frame(&self) -> i64 {
        match self {
            Difference::Field(d) => d.row.frame,
            Difference::MissingRow { row, .. } => row.frame,
        }
    }
}

/// Every difference in one field, for summary mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSummary {
    /// Field name in `a`.
    pub field: String,
    /// Number of matched rows where it differs.
    pub count: u32,
    pub first_frame: i64,
    pub last_frame: i64,
    /// Largest ULP distance among numeric differences.
    pub max_ulp: Option<u64>,
}

/// Result of [`diff`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceDiff {
    /// First difference by frame. Within a frame, rows in `a`'s order, fields in `a`'s column
    /// order; rows only in `b` come after.
    pub first: Option<Difference>,
    /// Compared fields of `a` with no column in `b`.
    pub only_in_a: Vec<String>,
    /// Columns of `b` that no compared field of `a` maps to.
    pub only_in_b: Vec<String>,
    /// Summary mode only: every differing field, in `a`'s column order.
    pub summary: Vec<FieldSummary>,
    /// Summary mode only: rows present in one trace and not the other.
    pub rows_only_in_a: u32,
    pub rows_only_in_b: u32,
}

impl TraceDiff {
    /// True when the traces match: no differing value, no unmatched row, no unmatched field.
    pub fn is_clean(&self) -> bool {
        self.first.is_none() && self.only_in_a.is_empty() && self.only_in_b.is_empty()
    }
}

/// Row keys of a table, in file order, with `frame_offset` subtracted so they use `a`'s
/// numbering.
fn row_keys(table: &TraceTable, side: &str, frame_offset: i64) -> Result<Vec<RowKey>, TraceError> {
    let missing = |name: &str| TraceError::Invalid(format!("{side} has no {name:?} column"));
    let frame_col = table.column(FRAME_COLUMN).ok_or_else(|| missing(FRAME_COLUMN))?;
    let object_col = table
        .column(OBJECT_COLUMN)
        .ok_or_else(|| missing(OBJECT_COLUMN))?;
    let mut seen: BTreeMap<(i64, String), u32> = BTreeMap::new();
    let mut keys = Vec::with_capacity(table.rows.len());
    for (index, row) in table.rows.iter().enumerate() {
        let frame = match row.get(frame_col) {
            Some(Value::Int(f)) => f.checked_sub(frame_offset).ok_or_else(|| {
                TraceError::Invalid(format!("{side} row {}: frame out of range", index + 1))
            })?,
            _ => {
                return Err(TraceError::Invalid(format!(
                    "{side} row {}: frame is not an integer",
                    index + 1
                )));
            }
        };
        let object = row.get(object_col).map(Value::decimal).unwrap_or_default();
        let count = seen.entry((frame, object.clone())).or_insert(0);
        keys.push(RowKey {
            frame,
            object,
            instance: *count,
        });
        *count = count.saturating_add(1);
    }
    Ok(keys)
}

/// A field compared in both traces: its name in `a` and column indices in `a` and `b`.
struct FieldPair {
    name: String,
    name_b: String,
    a: usize,
    b: usize,
}

/// Compares two traces bit for bit. Rows match by (frame, object, instance order), fields by
/// name through [`DiffOptions::rename`]. `frame` and `object` are the match key and are not
/// compared again. Returns the first difference with its context, and in summary mode every
/// differing field.
pub fn diff(a: &TraceTable, b: &TraceTable, opts: &DiffOptions) -> Result<TraceDiff, TraceError> {
    a.check_shape("a")?;
    b.check_shape("b")?;
    let keys_a = row_keys(a, "a", 0)?;
    let keys_b = row_keys(b, "b", opts.frame_offset)?;
    let index_a: BTreeMap<&RowKey, usize> = keys_a.iter().enumerate().map(|(i, k)| (k, i)).collect();
    let index_b: BTreeMap<&RowKey, usize> = keys_b.iter().enumerate().map(|(i, k)| (k, i)).collect();

    let mut result = TraceDiff::default();

    // Pair up fields by name.
    let is_key = |name: &str| name == FRAME_COLUMN || name == OBJECT_COLUMN;
    let mut pairs = Vec::new();
    let mut used_b = BTreeSet::new();
    for (ia, column) in a.columns.iter().enumerate() {
        if is_key(&column.name) || opts.ignore.contains(&column.name) {
            continue;
        }
        let name_b = opts.rename.get(&column.name).unwrap_or(&column.name);
        match b.column(name_b) {
            Some(ib) => {
                used_b.insert(ib);
                pairs.push(FieldPair {
                    name: column.name.clone(),
                    name_b: name_b.clone(),
                    a: ia,
                    b: ib,
                });
            }
            None => result.only_in_a.push(column.name.clone()),
        }
    }
    let ignored_b: BTreeSet<&String> = opts
        .ignore
        .iter()
        .map(|name| opts.rename.get(name).unwrap_or(name))
        .collect();
    for (ib, column) in b.columns.iter().enumerate() {
        if !is_key(&column.name) && !used_b.contains(&ib) && !ignored_b.contains(&column.name) {
            result.only_in_b.push(column.name.clone());
        }
    }

    // Visit rows by frame: `a`'s rows in file order, then rows only in `b`. The sort is stable,
    // so file order holds within a frame.
    let mut order: Vec<(&RowKey, Option<usize>, Option<usize>)> = keys_a
        .iter()
        .enumerate()
        .map(|(ia, key)| (key, Some(ia), index_b.get(key).copied()))
        .collect();
    order.extend(
        keys_b
            .iter()
            .enumerate()
            .filter(|(_, key)| !index_a.contains_key(key))
            .map(|(ib, key)| (key, None, Some(ib))),
    );
    order.sort_by_key(|(key, ..)| key.frame);

    let mut summary: Vec<Option<FieldSummary>> = vec![None; pairs.len()];
    'rows: for (key, row_a, row_b) in order {
        let (row_a, row_b) = match (row_a, row_b) {
            (Some(ia), Some(ib)) => (ia, ib),
            (ia, _) => {
                let missing_in = if ia.is_some() { Side::B } else { Side::A };
                match missing_in {
                    Side::A => result.rows_only_in_b = result.rows_only_in_b.saturating_add(1),
                    Side::B => result.rows_only_in_a = result.rows_only_in_a.saturating_add(1),
                }
                if result.first.is_none() {
                    result.first = Some(Difference::MissingRow {
                        row: key.clone(),
                        missing_in,
                    });
                }
                if !opts.summary {
                    break 'rows;
                }
                continue;
            }
        };
        let (Some(values_a), Some(values_b)) = (a.rows.get(row_a), b.rows.get(row_b)) else {
            continue;
        };
        for (pair, slot) in pairs.iter().zip(summary.iter_mut()) {
            let (Some(va), Some(vb)) = (values_a.get(pair.a), values_b.get(pair.b)) else {
                continue;
            };
            if va.same(vb) {
                continue;
            }
            let ulp = match (va.as_f64(), vb.as_f64()) {
                (Some(x), Some(y)) => ulp_distance(x, y),
                _ => None,
            };
            if result.first.is_none() {
                let context = context_frames(a, b, &index_a, &index_b, key, pair, opts.context);
                result.first = Some(Difference::Field(FieldDiff {
                    row: key.clone(),
                    field: pair.name.clone(),
                    field_b: pair.name_b.clone(),
                    a: va.clone(),
                    b: vb.clone(),
                    ulp,
                    context,
                }));
            }
            if !opts.summary {
                break 'rows;
            }
            let entry = slot.get_or_insert_with(|| FieldSummary {
                field: pair.name.clone(),
                count: 0,
                first_frame: key.frame,
                last_frame: key.frame,
                max_ulp: None,
            });
            entry.count = entry.count.saturating_add(1);
            entry.last_frame = key.frame;
            entry.max_ulp = entry.max_ulp.max(ulp);
        }
    }
    result.summary = summary.into_iter().flatten().collect();
    Ok(result)
}

/// The field's values in the frames around `key`, for the same object and instance.
fn context_frames(
    a: &TraceTable,
    b: &TraceTable,
    index_a: &BTreeMap<&RowKey, usize>,
    index_b: &BTreeMap<&RowKey, usize>,
    key: &RowKey,
    pair: &FieldPair,
    context: u32,
) -> Vec<ContextFrame> {
    let span = i64::from(context);
    let start = key.frame.saturating_sub(span);
    let end = key.frame.saturating_add(span);
    let lookup = |table: &TraceTable, index: &BTreeMap<&RowKey, usize>, probe: &RowKey, col| {
        index
            .get(probe)
            .and_then(|&row| table.rows.get(row))
            .and_then(|row| row.get(col))
            .cloned()
    };
    let mut frames = Vec::new();
    let mut frame = start;
    loop {
        let probe = RowKey {
            frame,
            object: key.object.clone(),
            instance: key.instance,
        };
        let va = lookup(a, index_a, &probe, pair.a);
        let vb = lookup(b, index_b, &probe, pair.b);
        if va.is_some() || vb.is_some() {
            frames.push(ContextFrame { frame, a: va, b: vb });
        }
        if frame >= end {
            break;
        }
        frame += 1;
    }
    frames
}

/// Optional value for display: the decimal, or `-` when the row is absent.
fn show(value: &Option<Value>) -> String {
    match value {
        Some(Value::Str(s)) => format!("{s:?}"),
        Some(v) => v.decimal(),
        None => "-".to_string(),
    }
}

impl fmt::Display for FieldDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = if self.field == self.field_b {
            self.field.clone()
        } else {
            format!("{} (b: {})", self.field, self.field_b)
        };
        writeln!(
            f,
            "frame {}, {} #{}, field {}",
            self.row.frame, self.row.object, self.row.instance, field
        )?;
        for (label, value) in [("a", &self.a), ("b", &self.b)] {
            let bits = value.hex_bits().unwrap_or_else(|| "-".to_string());
            writeln!(f, "  {label}: {}  bits {bits}", show(&Some(value.clone())))?;
        }
        match self.ulp {
            Some(ulp) => writeln!(f, "  ULP distance: {ulp}")?,
            None => writeln!(f, "  ULP distance: -")?,
        }
        writeln!(f, "  context (frame: a | b):")?;
        for c in &self.context {
            let marker = if c.frame == self.row.frame { ">" } else { " " };
            writeln!(f, "  {marker} {}: {} | {}", c.frame, show(&c.a), show(&c.b))?;
        }
        Ok(())
    }
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Difference::Field(d) => d.fmt(f),
            Difference::MissingRow { row, missing_in } => {
                let side = match missing_in {
                    Side::A => "a",
                    Side::B => "b",
                };
                writeln!(
                    f,
                    "frame {}, {} #{}: row missing in {side}",
                    row.frame, row.object, row.instance
                )
            }
        }
    }
}

impl fmt::Display for TraceDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_clean() {
            return writeln!(f, "traces match");
        }
        if !self.only_in_a.is_empty() {
            writeln!(f, "fields only in a: {}", self.only_in_a.join(", "))?;
        }
        if !self.only_in_b.is_empty() {
            writeln!(f, "fields only in b: {}", self.only_in_b.join(", "))?;
        }
        if let Some(first) = &self.first {
            writeln!(f, "first difference:")?;
            first.fmt(f)?;
        }
        if !self.summary.is_empty() || self.rows_only_in_a > 0 || self.rows_only_in_b > 0 {
            writeln!(f, "summary:")?;
            for s in &self.summary {
                let ulp = s.max_ulp.map_or_else(|| "-".to_string(), |u| u.to_string());
                writeln!(
                    f,
                    "  {}: {} rows, frames {}..{}, max ULP {ulp}",
                    s.field, s.count, s.first_frame, s.last_frame
                )?;
            }
            if self.rows_only_in_a > 0 {
                writeln!(f, "  rows only in a: {}", self.rows_only_in_a)?;
            }
            if self.rows_only_in_b > 0 {
                writeln!(f, "  rows only in b: {}", self.rows_only_in_b)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "frame,room,object,id,x,\"say \"\"hi\"\"\",\"v\"\n\
        2,\"room1\",\"player\",7,81.0,\"a,b\",-7.347880794884119e-17\n\
        3,\"room1\",\"player\",7,81.5,\"\",\n\
        3,\"room1\",\"player\",8,1e16,\"x\ny\",NaN\n";

    #[test]
    fn parses_types_and_quoting() {
        let t = TraceTable::parse(SAMPLE).expect("parse");
        assert_eq!(t.columns.len(), 7);
        assert_eq!(t.columns[5].name, "say \"hi\"");
        assert!(t.columns[5].quoted);
        assert!(!t.columns[0].quoted);
        assert_eq!(t.rows[0][0], Value::Int(2));
        assert_eq!(t.rows[0][5], Value::Str("a,b".into()));
        assert_eq!(
            t.rows[0][6].as_f64().map(f64::to_bits),
            Some((-7.347880794884119e-17f64).to_bits())
        );
        assert_eq!(t.rows[1][6], Value::Empty);
        assert_eq!(t.rows[2][5], Value::Str("x\ny".into()));
        assert_eq!(t.to_csv(), SAMPLE);
    }

    #[test]
    fn crlf_round_trips() {
        let crlf = SAMPLE.replace('\n', "\r\n");
        // The quoted "x\ny" became "x\r\ny", which is field content, not a line ending.
        let t = TraceTable::parse(&crlf).expect("parse");
        assert_eq!(t.line_ending, LineEnding::CrLf);
        assert_eq!(t.to_csv(), crlf);
    }

    #[test]
    fn bad_input_is_an_error() {
        for bad in [
            "",
            "a,b\n1\n",
            "a\n\"open\n",
            "a\nx\"y\n",
            "a\nhello\n",
            "a\n1\r\n",
            "a,\n1,2\n",
            "a\n\"q\"x\n",
        ] {
            assert!(TraceTable::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn ulp_distance_counts_doubles() {
        assert_eq!(ulp_distance(1.0, f64::from_bits(1.0f64.to_bits() + 1)), Some(1));
        assert_eq!(ulp_distance(0.0, -0.0), Some(0));
        assert_eq!(ulp_distance(f64::from_bits(1), -f64::from_bits(1)), Some(2));
        assert_eq!(ulp_distance(f64::NAN, 1.0), None);
    }

    #[test]
    fn diff_reports_rename_ignore_and_missing_rows() {
        let a =
            TraceTable::parse("frame,object,id,x,\"v\"\n1,\"p\",5,1.0,2.0\n2,\"p\",5,1.0,2.0\n").expect("a");
        let b = TraceTable::parse(
            "frame,object,id,x,\"w\"\n11,\"p\",9,1.0,2.0\n12,\"p\",9,1.0,2.0\n13,\"p\",9,1.0,2.0\n",
        )
        .expect("b");
        let mut opts = DiffOptions {
            frame_offset: 10,
            ..DiffOptions::default()
        };
        opts.rename.insert("v".into(), "w".into());
        opts.ignore.insert("id".into());
        let d = diff(&a, &b, &opts).expect("diff");
        assert!(d.only_in_a.is_empty() && d.only_in_b.is_empty());
        assert_eq!(
            d.first,
            Some(Difference::MissingRow {
                row: RowKey {
                    frame: 3,
                    object: "p".into(),
                    instance: 0
                },
                missing_in: Side::A,
            })
        );
        opts.rename.clear();
        let d = diff(&a, &b, &opts).expect("diff");
        assert_eq!(d.only_in_a, vec!["v".to_string()]);
        assert_eq!(d.only_in_b, vec!["w".to_string()]);
    }

    /// Path of the research repo's GM8 traces, or `None` to skip (IWBTG_RESEARCH unset).
    #[allow(clippy::disallowed_methods)] // Test-only: the sim itself never reads the environment.
    fn research_traces() -> Option<std::path::PathBuf> {
        let root = std::env::var("IWBTG_RESEARCH").ok()?;
        Some(std::path::Path::new(&root).join("specs/engines/gm8/traces"))
    }

    /// Every `.csv` under the research traces folder, sorted, with its text.
    fn research_csvs() -> Vec<(std::path::PathBuf, String)> {
        let Some(dir) = research_traces() else {
            eprintln!("IWBTG_RESEARCH is not set; skipping research trace tests");
            return Vec::new();
        };
        let mut paths = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let entries = std::fs::read_dir(&d).unwrap_or_else(|e| panic!("read {d:?}: {e}"));
            for entry in entries {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "csv") {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        assert!(!paths.is_empty(), "no trace CSVs under {dir:?}");
        paths
            .into_iter()
            .map(|p| {
                let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
                (p, text)
            })
            .collect()
    }

    #[test]
    fn research_traces_parse_and_rewrite_byte_identical() {
        for (path, text) in research_csvs() {
            let table = TraceTable::parse(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
            assert!(!table.rows.is_empty(), "{path:?} has no rows");
            let fixed: Vec<&str> = table.columns[..FIXED_COLUMNS.len()]
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            assert_eq!(fixed, FIXED_COLUMNS, "{path:?} header");
            assert!(table.to_csv() == text, "{path:?} does not re-write byte for byte");
        }
    }

    #[test]
    fn research_traces_self_diff_is_clean() {
        let opts = DiffOptions {
            summary: true,
            ..DiffOptions::default()
        };
        for (path, text) in research_csvs() {
            let table = TraceTable::parse(&text).expect("parse");
            let d = diff(&table, &table, &opts).expect("diff");
            assert!(d.is_clean(), "{path:?}:\n{d}");
            assert!(d.summary.is_empty());
        }
    }

    #[test]
    fn research_traces_one_ulp_nudge_is_found() {
        for (path, text) in research_csvs() {
            let original = TraceTable::parse(&text).expect("parse");
            // Nudge `y` in a row near the middle by one ULP, then go through text again.
            let y = original.column("y").expect("y column");
            let row = original.rows.len() / 2;
            let mut nudged = original.clone();
            let Value::Real(old) = nudged.rows[row][y] else {
                panic!("{path:?}: y is not a real");
            };
            nudged.rows[row][y] = Value::Real(f64::from_bits(old.to_bits() + 1));
            let nudged = TraceTable::parse(&nudged.to_csv()).expect("reparse");
            let Value::Int(frame) = original.rows[row][0] else {
                panic!("{path:?}: frame is not an integer");
            };

            let d = diff(&original, &nudged, &DiffOptions::default()).expect("diff");
            let Some(Difference::Field(first)) = &d.first else {
                panic!("{path:?}: expected a field difference, got {d}");
            };
            assert_eq!(first.row.frame, frame, "{path:?}");
            assert_eq!(first.field, "y", "{path:?}");
            assert_eq!(first.ulp, Some(1), "{path:?}");
            assert!(first.context.len() <= 11 && first.context.len() > 1);
            assert!(first.context.iter().all(|c| (c.frame - frame).abs() <= 5));
            assert_ne!(first.a.hex_bits(), first.b.hex_bits());
            let shown = d.to_string();
            assert!(shown.contains(&format!("frame {frame}")), "{shown}");

            let opts = DiffOptions {
                summary: true,
                ..DiffOptions::default()
            };
            let d = diff(&original, &nudged, &opts).expect("diff");
            assert_eq!(
                d.summary,
                vec![FieldSummary {
                    field: "y".into(),
                    count: 1,
                    first_frame: frame,
                    last_frame: frame,
                    max_ulp: Some(1),
                }],
                "{path:?}"
            );
        }
    }
}
