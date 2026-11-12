//! The canonical formatter for package data files (`iwbtg pkg fmt`; docs/decisions/0002-data-
//! syntax.md, Conventions; docs/format/v0.md, Common rules).
//!
//! It works on the document with `toml_edit`, so comments, the order of keys and tables, and the
//! order of every list stay as they are. What it sets:
//!
//! - LF line ends, no byte order mark, exactly one newline at the end of the file.
//! - `key = value` spacing: no indentation before a key, one space around `=`, no space before a
//!   trailing comment's `#` beyond what the author wrote (comment alignment is kept).
//! - Floats in their shortest form that parses back to the same bits (`0.1`, `0.0000001`, `3.0`).
//!   Integers and strings are kept as written (`180_000`, `0x10`).
//! - Inline tables as `{ key = value, ... }` on one line.
//! - Short arrays on one line as `[1, 2, 3]`. An array that is already spread over several lines
//!   stays that way (it is somebody's layout), and an array with a comment inside is left alone.
//! - Lists of inline tables that are data rows (room `instances`, `views`, `on_create`, tile
//!   `layers` and `tiles`, timeline `moments`, `object_order`, `stages`, and `[stripped]`'s
//!   `cutscenes`, `dialogues` and cut `rooms` and `instances`) with one row per line,
//!   indented two spaces, with a trailing comma, so a one-instance edit is a one-line diff. Within
//!   a row the keys the format names come first in a fixed order (`kind, x, y, name, group,
//!   xscale, yscale, angle, visible, depth, run, step, gm_id` for instances); other keys follow in the
//!   order they were written.
//!
//! It never reorders a list and never merges or splits rows, because instance order is creation
//! order. Running it twice changes nothing.

use toml_edit::{Array, DocumentMut, Formatted, InlineTable, Item, Table, Value};

/// Lists of inline tables laid out one row per line, with the key order of a row.
const ROW_LISTS: [(&str, &[&str]); 11] = [
    (
        "instances",
        &[
            "room", "kind", "x", "y", "name", "group", "xscale", "yscale", "angle", "visible", "depth",
            "run", "step", "gm_id",
        ],
    ),
    ("views", &["visible", "view", "port", "follow", "border", "speed"]),
    ("on_create", &["do"]),
    ("layers", &["depth", "tileset", "tiles"]),
    ("tiles", &["x", "y", "src", "run"]),
    ("moments", &["t", "call"]),
    ("object_order", &["kind"]),
    ("stages", &["id", "name", "trigger", "room"]),
    ("rooms", &["room", "to"]),
    ("cutscenes", &["id", "kind", "room"]),
    ("dialogues", &["id", "kind", "room", "advance", "timeout_ticks"]),
];

/// The canonical form of a data file, or why it is not TOML.
pub fn format_toml(text: &str) -> Result<String, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    table(doc.as_table_mut());
    let mut out = doc.to_string();
    let trimmed = out.trim_end_matches(['\n', ' ', '\t']).len();
    out.truncate(trimmed);
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn table(t: &mut Table) {
    for (mut key, item) in t.iter_mut() {
        let name = key.get().to_owned();
        // Only `key = value` lines: a table header's key has its own layout.
        if item.is_value() {
            let decor = key.leaf_decor_mut();
            let dropped_indent = decor.prefix().and_then(|p| p.as_str()).map(drop_indent);
            if let Some(p) = dropped_indent {
                decor.set_prefix(p);
            }
            decor.set_suffix(" ");
        }
        item_in_table(&name, item);
    }
}

fn item_in_table(name: &str, item: &mut Item) {
    match item {
        Item::Value(v) => {
            normalize_value_decor(v);
            value(name, v, 0);
        }
        Item::Table(t) => table(t),
        Item::ArrayOfTables(a) => {
            for t in a.iter_mut() {
                table(t);
            }
        }
        Item::None => {}
    }
}

/// The part of a leading text that is kept: comment and blank lines, without the indentation of
/// the line the key is on.
fn drop_indent(prefix: &str) -> String {
    match prefix.rfind('\n') {
        Some(i) => prefix[..=i].to_owned(),
        None => String::new(),
    }
}

/// One space after `=`, and a trailing comment keeps its alignment but loses trailing blanks.
fn normalize_value_decor(v: &mut Value) {
    let suffix = v.decor().suffix().and_then(|s| s.as_str()).map(str::to_owned);
    let decor = v.decor_mut();
    decor.set_prefix(" ");
    if let Some(s) = suffix {
        let kept = if s.contains('#') {
            let s = s.trim_end();
            if s.starts_with(char::is_whitespace) {
                s.to_owned()
            } else {
                format!(" {s}")
            }
        } else {
            String::new()
        };
        decor.set_suffix(kept);
    }
}

fn has_comment(v: &Value) -> bool {
    let d = v.decor();
    let in_decor =
        |r: Option<&toml_edit::RawString>| r.and_then(|r| r.as_str()).is_some_and(|s| s.contains('#'));
    in_decor(d.prefix()) || in_decor(d.suffix())
}

fn array_has_comment(a: &Array) -> bool {
    a.iter().any(has_comment) || a.trailing().as_str().is_some_and(|s| s.contains('#'))
}

/// Formats one value in place. `name` is the key it belongs to (it picks a row list), `depth`
/// the nesting of row lists around it.
fn value(name: &str, v: &mut Value, depth: usize) {
    match v {
        Value::Float(f) => {
            let x = *f.value();
            if x.is_finite() {
                let decor = f.decor().clone();
                let mut nf = Formatted::new(x);
                *nf.decor_mut() = decor;
                *f = nf;
            }
        }
        Value::InlineTable(t) => inline_table(t, &[], depth),
        Value::Array(a) => array(name, a, depth),
        _ => {}
    }
}

fn inline_table(t: &mut InlineTable, order: &[&str], depth: usize) {
    if !order.is_empty() {
        t.sort_values_by(|k1, _, k2, _| rank(order, k1.get()).cmp(&rank(order, k2.get())));
    }
    t.fmt();
    for (key, v) in t.iter_mut() {
        let name = key.get().to_owned();
        value(&name, v, depth);
    }
}

fn rank(order: &[&str], key: &str) -> usize {
    order.iter().position(|k| *k == key).unwrap_or(order.len())
}

fn array(name: &str, a: &mut Array, depth: usize) {
    let row_list = ROW_LISTS.iter().find(|(n, _)| *n == name);
    let all_tables = !a.is_empty() && a.iter().all(Value::is_inline_table);
    if let (Some((_, order)), true) = (row_list, all_tables) {
        if array_has_comment(a) {
            return;
        }
        let indent = "  ".repeat(depth + 1);
        for v in a.iter_mut() {
            if let Value::InlineTable(t) = v {
                inline_table(t, order, depth + 1);
            }
            let d = v.decor_mut();
            d.set_prefix(format!("\n{indent}"));
            d.set_suffix("");
        }
        a.set_trailing_comma(true);
        a.set_trailing(format!("\n{}", "  ".repeat(depth)));
        return;
    }
    if array_has_comment(a) {
        return;
    }
    let multiline = a.to_string().contains('\n');
    for v in a.iter_mut() {
        value("", v, depth);
    }
    if !multiline {
        a.fmt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> String {
        format_toml(s).unwrap()
    }

    #[test]
    fn instances_go_one_per_line_in_the_fixed_key_order() {
        let out = fmt(
            "size=[800,608]\ninstances=[{x=1,y=2,kind=\"spike\",dir=\"up\",group=\"g\"},{ kind = \"block\", y = 5, x = 4 }]\n",
        );
        assert_eq!(
            out,
            "size = [800, 608]\ninstances = [\n  { kind = \"spike\", x = 1, y = 2, group = \"g\", dir = \"up\" },\n  { kind = \"block\", x = 4, y = 5 },\n]\n"
        );
    }

    #[test]
    fn nested_row_lists_indent_one_level_deeper() {
        let out = fmt(
            "layers=[{tileset=\"t\",depth=5,tiles=[{src=[0,0],y=2,x=1},{x=3,y=4,src=[1,0],run=[2,1]}]}]\n",
        );
        assert_eq!(
            out,
            "layers = [\n  { depth = 5, tileset = \"t\", tiles = [\n    { x = 1, y = 2, src = [0, 0] },\n    { x = 3, y = 4, src = [1, 0], run = [2, 1] },\n  ] },\n]\n"
        );
    }

    #[test]
    fn comments_and_the_order_of_everything_else_survive() {
        let src = "# top\nid = \"a\"   # why\n\n[engine]\n  restart = { kind = \"x\",   trigger = \"y\" }\n# before z\nz = 1\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "# top\nid = \"a\"   # why\n\n[engine]\nrestart = { kind = \"x\", trigger = \"y\" }\n# before z\nz = 1\n"
        );
    }

    #[test]
    fn floats_take_their_shortest_form_and_integers_stay() {
        let out = fmt("a = 0.10\nb = 1.5e0\nc = 3.0\nd = 180_000\ne = 0x10\nf = 1e-7\n");
        assert_eq!(
            out,
            "a = 0.1\nb = 1.5\nc = 3.0\nd = 180_000\ne = 0x10\nf = 0.0000001\n"
        );
    }

    #[test]
    fn crlf_bom_and_trailing_blank_lines_go() {
        assert_eq!(fmt("\u{feff}a = 1\r\nb = 2\r\n\r\n\r\n"), "a = 1\nb = 2\n");
        assert_eq!(fmt("a = 1"), "a = 1\n");
    }

    #[test]
    fn a_multiline_array_and_an_array_with_a_comment_are_left_alone() {
        let src = "order = [\n  \"kid\", \"block\",\n  \"spike\",\n]\nmoments = [\n  # the start\n  { call = \"b\", t = 1 },\n]\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn it_is_idempotent() {
        let src = "size=[800,608]\n[[views]]\nvisible=true\ninstances=[{x=1,y=2,kind=\"spike\"}, {kind=\"b\",x=0.30,y=0}]\nlayers=[{depth=1,tileset=\"t\",tiles=[{x=0,y=0,src=[0,0]}]}]\n";
        let once = fmt(src);
        assert_eq!(fmt(&once), once);
    }

    #[test]
    fn not_toml_is_an_error_with_a_position() {
        assert!(format_toml("a = \n").is_err());
    }
}
