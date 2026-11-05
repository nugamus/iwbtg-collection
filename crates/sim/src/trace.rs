//! Trace rows for comparison with the originals (docs/architecture/traces.md, "Comparing traces with
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
//! Every NaN is written as `NaN`, so a trace carries no NaN sign or payload, and [`Value::same`]
//! treats any two NaNs as equal.
//!
//! The parser accepts only what the writer produces: a number written any other way (`+1`,
//! `1.50`, `007`, `-0`, `inf` spelled differently, ...), a missing final line ending or a
//! repeated column is an error, so [`TraceTable::to_csv`] gives back the parsed bytes.
//!
//! A column is its name plus its quoting. The recorder writes a `--vars` column quoted even when
//! the name is also a fixed column (`image_index,...,"image_index"` when `--vars` asks for
//! `image_index` again), so a quoted game variable and a bare fixed column may share a name: they
//! are two columns, and [`diff`] pairs each with its own counterpart. Only a name repeated with
//! the same quoting is an error. Where a name appears twice, [`TraceTable::column`] finds the bare
//! column, and reports show the quoted one in quotes ([`TraceTable::label`]).
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

    /// Bit-exact equality: numbers compare as f64 bits (so `0.0 != -0.0`), strings compare as
    /// text, and kinds never mix except Int with Real. Any two NaNs are equal: the CSV writes
    /// every NaN as `NaN`, so NaN sign and payload are not traced.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Empty, Value::Empty) => true,
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits(),
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
    /// the recorder quotes them. `iwbtg trace run` builds our side with this. Errors when a game
    /// variable repeats; a game variable named like a fixed column is a separate column, as in the
    /// recorder's files.
    pub fn new(game_vars: &[&str]) -> Result<TraceTable, TraceError> {
        let fixed = FIXED_COLUMNS.iter().map(|name| Column {
            name: (*name).to_string(),
            quoted: false,
        });
        let vars = game_vars.iter().map(|name| Column {
            name: (*name).to_string(),
            quoted: true,
        });
        let columns: Vec<Column> = fixed.chain(vars).collect();
        if let Some(name) = duplicate_column(&columns) {
            return Err(TraceError::Invalid(format!("column {name} appears twice")));
        }
        Ok(TraceTable {
            columns,
            rows: Vec::new(),
            line_ending: LineEnding::Lf,
        })
    }

    /// Index of the column called `name`: the bare (fixed) column when a quoted game variable
    /// shares its name.
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.name == name && !c.quoted)
            .or_else(|| self.columns.iter().position(|c| c.name == name))
    }

    /// Index of the column with this name and quoting.
    pub fn column_exact(&self, name: &str, quoted: bool) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.name == name && c.quoted == quoted)
    }

    /// The column's name for reports: the bare name, or the name in quotes when it is a quoted
    /// game variable that shares its name with a bare column of this table.
    pub fn label(&self, index: usize) -> String {
        match self.columns.get(index) {
            Some(c) if c.quoted && self.columns.iter().any(|o| !o.quoted && o.name == c.name) => {
                format!("\"{}\"", c.name)
            }
            Some(c) => c.name.clone(),
            None => String::new(),
        }
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
    /// exact), and [`Value::Empty`] when empty. Quoted cells become [`Value::Str`]. A number
    /// not written exactly as the writer would write it is an error, as are a repeated column
    /// name and a last line without its line ending.
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
        if let Some(name) = duplicate_column(&columns) {
            return Err(TraceError::Parse {
                line: 1,
                message: format!("column {name} appears twice"),
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

    /// Writes the table in the recorder's format. Re-writing any table [`TraceTable::parse`]
    /// accepted gives the same bytes.
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

    /// Checks column names are unique and every row has one value per column, so later
    /// indexing and name lookups cannot go wrong.
    fn check_shape(&self, side: &str) -> Result<(), TraceError> {
        if let Some(name) = duplicate_column(&self.columns) {
            return Err(TraceError::Invalid(format!(
                "{side}: column {name} appears twice"
            )));
        }
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

/// The first column that appears more than once with the same quoting, if any, as the header
/// writes it (`x` or `"x"`).
fn duplicate_column(columns: &[Column]) -> Option<String> {
    let mut seen = BTreeSet::new();
    columns
        .iter()
        .find(|c| !seen.insert((c.name.as_str(), c.quoted)))
        .map(|c| {
            if c.quoted {
                format!("\"{}\"", c.name)
            } else {
                c.name.clone()
            }
        })
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
        let value = if is_plain_integer(&self.text)
            && let Ok(i) = self.text.parse::<i64>()
        {
            Value::Int(i)
        } else {
            match self.text.parse::<f64>() {
                Ok(r) => Value::Real(r),
                Err(_) => {
                    return Err(TraceError::Parse {
                        line,
                        message: format!("{:?} is neither a number nor a quoted string", self.text),
                    });
                }
            }
        };
        // Only the writer's own spelling round-trips: reject `+1`, `1.50`, `007`, `-0`, ...
        let canonical = value.decimal();
        if canonical != self.text {
            return Err(TraceError::Parse {
                line,
                message: format!(
                    "number {:?} is not in canonical form (expected {canonical:?})",
                    self.text
                ),
            });
        }
        Ok(value)
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

    /// Reads one record. `None` at end of input. Every line, the last included, must end with
    /// its line ending: the recorder always writes it, so a missing one means a cut-off file.
    fn record(&mut self) -> Result<Option<Vec<RawField>>, TraceError> {
        if self.pos >= self.bytes.len() {
            return Ok(None);
        }
        let mut fields = Vec::new();
        loop {
            fields.push(self.field()?);
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                None => return Err(self.error("last line has no line ending (file cut off?)")),
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

/// One frame of context for a missing row: every compared field in each trace, if that row
/// exists. Values follow [`MissingRow::fields`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFrame {
    pub frame: i64,
    pub a: Option<Vec<Value>>,
    pub b: Option<Vec<Value>>,
}

/// A row present in one trace only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingRow {
    pub row: RowKey,
    /// The trace without the row.
    pub missing_in: Side,
    /// The compared fields, by name in `a`, in the order [`RowFrame`] lists their values.
    pub fields: Vec<String>,
    /// The same object and instance around this frame, oldest first, up to
    /// [`DiffOptions::context`] frames each side, including this frame.
    pub context: Vec<RowFrame>,
}

/// The first thing that differs between two traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// Both traces have the row, and a field differs.
    Field(FieldDiff),
    /// Only one trace has the row.
    MissingRow(MissingRow),
}

impl Difference {
    /// Frame of the difference, in `a`'s numbering.
    pub fn frame(&self) -> i64 {
        match self {
            Difference::Field(d) => d.row.frame,
            Difference::MissingRow(m) => m.row.frame,
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

    // Pair up fields by name and quoting: a quoted game variable named like a fixed column pairs
    // with the other side's quoted column, the fixed column with the fixed one. A column with no
    // counterpart of the same quoting pairs with any unpaired column of that name.
    let is_key = |c: &Column| !c.quoted && (c.name == FRAME_COLUMN || c.name == OBJECT_COLUMN);
    let mut pairs = Vec::new();
    let mut used_b = BTreeSet::new();
    for (ia, column) in a.columns.iter().enumerate() {
        if is_key(column) || opts.ignore.contains(&column.name) {
            continue;
        }
        let name_b = opts.rename.get(&column.name).unwrap_or(&column.name);
        let ib = b
            .column_exact(name_b, column.quoted)
            .filter(|ib| !used_b.contains(ib))
            .or_else(|| {
                b.columns
                    .iter()
                    .enumerate()
                    .position(|(ib, c)| c.name == *name_b && !used_b.contains(&ib) && !is_key(c))
            });
        match ib {
            Some(ib) => {
                used_b.insert(ib);
                pairs.push(FieldPair {
                    name: a.label(ia),
                    name_b: b.label(ib),
                    a: ia,
                    b: ib,
                });
            }
            None => result.only_in_a.push(a.label(ia)),
        }
    }
    let ignored_b: BTreeSet<&String> = opts
        .ignore
        .iter()
        .map(|name| opts.rename.get(name).unwrap_or(name))
        .collect();
    for (ib, column) in b.columns.iter().enumerate() {
        if !is_key(column) && !used_b.contains(&ib) && !ignored_b.contains(&column.name) {
            result.only_in_b.push(b.label(ib));
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
                    result.first = Some(Difference::MissingRow(MissingRow {
                        row: key.clone(),
                        missing_in,
                        fields: pairs.iter().map(|p| p.name.clone()).collect(),
                        context: row_context(a, b, &index_a, &index_b, key, &pairs, opts.context),
                    }));
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

/// Keys of the same object and instance as `key`, from `context` frames before it to `context`
/// frames after it.
fn context_window(key: &RowKey, context: u32) -> Vec<RowKey> {
    let span = i64::from(context);
    let start = key.frame.saturating_sub(span);
    let end = key.frame.saturating_add(span);
    let mut keys = Vec::new();
    let mut frame = start;
    loop {
        keys.push(RowKey {
            frame,
            object: key.object.clone(),
            instance: key.instance,
        });
        if frame >= end {
            break;
        }
        frame += 1;
    }
    keys
}

/// The row of `table` with key `probe`, if any.
fn lookup_row<'t>(
    table: &'t TraceTable,
    index: &BTreeMap<&RowKey, usize>,
    probe: &RowKey,
) -> Option<&'t Vec<Value>> {
    index.get(probe).and_then(|&row| table.rows.get(row))
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
    let mut frames = Vec::new();
    for probe in context_window(key, context) {
        let va = lookup_row(a, index_a, &probe).and_then(|row| row.get(pair.a).cloned());
        let vb = lookup_row(b, index_b, &probe).and_then(|row| row.get(pair.b).cloned());
        if va.is_some() || vb.is_some() {
            frames.push(ContextFrame {
                frame: probe.frame,
                a: va,
                b: vb,
            });
        }
    }
    frames
}

/// Every compared field in the frames around `key`, for the same object and instance.
fn row_context(
    a: &TraceTable,
    b: &TraceTable,
    index_a: &BTreeMap<&RowKey, usize>,
    index_b: &BTreeMap<&RowKey, usize>,
    key: &RowKey,
    pairs: &[FieldPair],
    context: u32,
) -> Vec<RowFrame> {
    let pick = |row: &Vec<Value>, col: fn(&FieldPair) -> usize| -> Vec<Value> {
        pairs
            .iter()
            .map(|p| row.get(col(p)).cloned().unwrap_or(Value::Empty))
            .collect()
    };
    let mut frames = Vec::new();
    for probe in context_window(key, context) {
        let va = lookup_row(a, index_a, &probe).map(|row| pick(row, |p| p.a));
        let vb = lookup_row(b, index_b, &probe).map(|row| pick(row, |p| p.b));
        if va.is_some() || vb.is_some() {
            frames.push(RowFrame {
                frame: probe.frame,
                a: va,
                b: vb,
            });
        }
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
            Difference::MissingRow(m) => m.fmt(f),
        }
    }
}

impl fmt::Display for MissingRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side = match self.missing_in {
            Side::A => "a",
            Side::B => "b",
        };
        writeln!(
            f,
            "frame {}, {} #{}: row missing in {side}",
            self.row.frame, self.row.object, self.row.instance
        )?;
        let show_row = |values: &Option<Vec<Value>>| match values {
            Some(values) => values
                .iter()
                .map(|v| show(&Some(v.clone())))
                .collect::<Vec<_>>()
                .join(", "),
            None => "-".to_string(),
        };
        writeln!(f, "  context (frame: a | b), fields {}:", self.fields.join(", "))?;
        for c in &self.context {
            let marker = if c.frame == self.row.frame { ">" } else { " " };
            writeln!(
                f,
                "  {marker} {}: {} | {}",
                c.frame,
                show_row(&c.a),
                show_row(&c.b)
            )?;
        }
        Ok(())
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
            // Cut off before the final line ending.
            "a\n1",
            "a",
            "a\n\"s\"",
            // Repeated column names with the same quoting.
            "a,a\n1,2\n",
            "\"a\",\"a\"\n1,2\n",
        ] {
            assert!(TraceTable::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn non_canonical_numbers_are_errors() {
        for cell in [
            "-0",
            "+1",
            "007",
            "1.50",
            "1E5",
            "1e5",
            ".5",
            "1.",
            "infinity",
            "Infinity",
            "INF",
            "+inf",
            "nan",
            "-NaN",
            "99999999999999999999",
            "1.0e0",
        ] {
            let text = format!("a\n{cell}\n");
            assert!(TraceTable::parse(&text).is_err(), "{cell:?} should fail");
        }
        for cell in ["0", "-7", "81.0", "-0.0", "1e16", "1e-7", "inf", "-inf", "NaN"] {
            let text = format!("a\n{cell}\n");
            let t = TraceTable::parse(&text).unwrap_or_else(|e| panic!("{cell:?}: {e}"));
            assert_eq!(t.to_csv(), text);
        }
    }

    #[test]
    fn any_two_nans_are_the_same() {
        // A runtime NaN on x86_64 has the sign bit set; parsing "NaN" does not.
        let runtime = f64::from_bits(0xFFF8_0000_0000_0000);
        let parsed = TraceTable::parse("a\nNaN\n").expect("parse").rows[0][0].clone();
        assert!(Value::Real(runtime).same(&parsed));
        assert!(Value::Real(f64::from_bits(0x7FF0_0000_0000_0001)).same(&parsed));
        assert!(!Value::Real(0.0).same(&Value::Real(-0.0)));
        assert!(!Value::Real(f64::INFINITY).same(&parsed));
    }

    #[test]
    fn new_rejects_repeated_columns() {
        assert!(TraceTable::new(&["jump[1]", "hp"]).is_ok());
        // A game variable named like a fixed column is its own (quoted) column, as the recorder
        // writes it.
        let t = TraceTable::new(&["x", "image_index"]).expect("var named like a fixed column");
        assert_eq!(t.column("x"), Some(4));
        assert_eq!(t.column_exact("x", true), Some(18));
        assert_eq!(t.label(18), "\"x\"");
        assert_eq!(t.label(4), "x");
        assert!(TraceTable::new(&["hp", "hp"]).is_err());
        assert!(TraceTable::new(&["x", "x"]).is_err());
        let mut t = TraceTable::new(&["hp"]).expect("new");
        t.columns.push(Column {
            name: "hp".into(),
            quoted: true,
        });
        assert!(diff(&t, &t, &DiffOptions::default()).is_err());
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
        let both = || Some(vec![Value::Real(1.0), Value::Real(2.0)]);
        assert_eq!(
            d.first,
            Some(Difference::MissingRow(MissingRow {
                row: RowKey {
                    frame: 3,
                    object: "p".into(),
                    instance: 0
                },
                missing_in: Side::A,
                fields: vec!["x".into(), "v".into()],
                context: vec![
                    RowFrame {
                        frame: 1,
                        a: both(),
                        b: both(),
                    },
                    RowFrame {
                        frame: 2,
                        a: both(),
                        b: both(),
                    },
                    RowFrame {
                        frame: 3,
                        a: None,
                        b: both(),
                    },
                ],
            }))
        );
        let shown = d.to_string();
        assert!(shown.contains("row missing in a"), "{shown}");
        assert!(shown.contains("  > 3: - | 1.0, 2.0"), "{shown}");
        opts.rename.clear();
        let d = diff(&a, &b, &opts).expect("diff");
        assert_eq!(d.only_in_a, vec!["v".to_string()]);
        assert_eq!(d.only_in_b, vec!["w".to_string()]);
    }

    /// The recorder quotes a `--vars` column named like a fixed one (machinary/tr-room-13 logs
    /// `"image_index"` and `"depth"`): it parses, round-trips, and diffs pair quoted with quoted
    /// and bare with bare.
    #[test]
    fn quoted_var_named_like_a_fixed_column() {
        let text = "frame,object,image_index,\"image_index\"\n1,\"p\",0.0,5.0\n2,\"p\",1.0,5.0\n";
        let a = TraceTable::parse(text).expect("parse");
        assert_eq!(a.to_csv(), text);
        assert_eq!(a.column("image_index"), Some(2));
        assert_eq!(a.column_exact("image_index", true), Some(3));
        let mut b = a.clone();
        b.rows[1][3] = Value::Real(6.0);
        let d = diff(&a, &b, &DiffOptions::default()).expect("diff");
        let Some(Difference::Field(first)) = &d.first else {
            panic!("expected a field difference: {d}");
        };
        assert_eq!(first.field, "\"image_index\"");
        assert_eq!(first.row.frame, 2);
        assert!(d.only_in_a.is_empty() && d.only_in_b.is_empty());
        // The quoted column alone on one side still pairs by name.
        let c = TraceTable::parse("frame,object,\"image_index\"\n1,\"p\",5.0\n2,\"p\",6.0\n").expect("c");
        let d = diff(&c, &b, &DiffOptions::default()).expect("diff");
        assert!(d.first.is_none(), "{d}");
        assert_eq!(d.only_in_b, vec!["image_index".to_string()]);
    }

    /// Every `specs/engines/<engine>/traces` folder in the research repo, sorted, or `None` to
    /// skip (IWBTG_RESEARCH unset).
    #[allow(clippy::disallowed_methods)] // Test-only: the sim itself never reads the environment.
    fn research_trace_dirs() -> Option<Vec<std::path::PathBuf>> {
        let root = std::env::var("IWBTG_RESEARCH").ok()?;
        let engines = std::path::Path::new(&root).join("specs/engines");
        let entries = std::fs::read_dir(&engines).unwrap_or_else(|e| panic!("read {engines:?}: {e}"));
        let mut dirs: Vec<std::path::PathBuf> = entries
            .map(|entry| entry.expect("dir entry").path().join("traces"))
            .filter(|traces| traces.is_dir())
            .collect();
        dirs.sort();
        Some(dirs)
    }

    /// Every `.csv` under every engine's research traces folder, sorted, with its text.
    #[allow(clippy::disallowed_methods, reason = "test-only: reads research trace files")]
    fn research_csvs() -> Vec<(std::path::PathBuf, String)> {
        let Some(dirs) = research_trace_dirs() else {
            eprintln!("IWBTG_RESEARCH is not set; skipping research trace tests");
            return Vec::new();
        };
        let mut paths = Vec::new();
        let mut stack = dirs.clone();
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
        assert!(!paths.is_empty(), "no trace CSVs under {dirs:?}");
        paths
            .into_iter()
            .map(|p| {
                let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
                (p, text)
            })
            .collect()
    }

    /// Every research CSV parses and re-writes byte for byte. A header-only file is valid: the
    /// traced object never existed in the frames run (rely-the-needle-4-2/tr-room-03-a).
    #[test]
    fn research_traces_parse_and_rewrite_byte_identical() {
        let mut with_rows = 0usize;
        for (path, text) in research_csvs() {
            let table = TraceTable::parse(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
            with_rows += usize::from(!table.rows.is_empty());
            let fixed: Vec<&str> = table.columns[..FIXED_COLUMNS.len()]
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            assert_eq!(fixed, FIXED_COLUMNS, "{path:?} header");
            assert!(table.to_csv() == text, "{path:?} does not re-write byte for byte");
        }
        if research_trace_dirs().is_some() {
            assert!(with_rows > 0, "no research CSV has rows");
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
            if original.rows.is_empty() {
                continue;
            }
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
