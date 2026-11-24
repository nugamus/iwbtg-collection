//! Format migrations: the file rewrites `iwbtg pkg upgrade` makes (docs/format/v1.md, Upgrading a
//! format-0 package; docs/format/versioning.md).
//!
//! The loader reads every older format directly into the same `GameDef` (principle 7), so a
//! migration never runs at load. It runs only when a package is rewritten on disk. Each step is
//! one function from format `N` to `N + 1` over the file text, edited with `toml_edit` so
//! comments, key order and layout stay as the author wrote them.
//!
//! A step takes the values the loader resolved for the old package ([`Resolved`]): a migration
//! writes down what the package already plays under, it never chooses a behavior. `pkg upgrade`
//! loads the rewritten files again and requires the same `GameDef`.
//!
//! The gameplay hash is not part of a step. Whether an upgraded package keeps its
//! `gameplay_hash` is #928: the canonical form the hash reads can call [`upgrade_manifest`] on a
//! format-0 manifest (or normalize a newer one back), without touching the steps.

use toml_edit::{DocumentMut, Item, Key, Table, value};

use crate::compat::Resolved;
use crate::manifest::FORMAT_VERSION;

/// The JSON Schema folder of each format, as `#:schema` lines name it.
fn schema_dir(format: u32) -> String {
    format!("/schema/v{format}/")
}

/// The manifest's `format`, or `None` when the text is not TOML or has no integer `format`.
#[must_use]
pub fn manifest_format(text: &str) -> Option<u32> {
    let doc = text.parse::<DocumentMut>().ok()?;
    u32::try_from(doc.get("format")?.as_integer()?).ok()
}

/// Rewrites a manifest from its format to [`FORMAT_VERSION`], one step at a time. `resolved` is
/// what the loader resolved for the package as it is. Returns the text unchanged when the
/// manifest is already current.
pub fn upgrade_manifest(text: &str, resolved: &Resolved) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("game.toml: {e}"))?;
    let mut format = doc
        .get("format")
        .and_then(Item::as_integer)
        .and_then(|f| u32::try_from(f).ok())
        .ok_or("game.toml: no integer `format`")?;
    if format > FORMAT_VERSION {
        return Err(format!(
            "game.toml: format {format} is newer than this build ({FORMAT_VERSION})"
        ));
    }
    while format < FORMAT_VERSION {
        match format {
            0 => manifest_0_to_1(&mut doc, resolved),
            _ => unreachable!("one step per format below FORMAT_VERSION"),
        }
        format += 1;
        set_format(&mut doc, format);
    }
    Ok(doc.to_string())
}

/// Format 0 to 1: `engine_rev` becomes `ruleset = "gm8@<rev>"` in its place, and the inferred
/// `[compat] profile` is written. A `ruleset` or `[compat] profile` the package already wrote is
/// kept; the loader has proved they equal the resolved values.
fn manifest_0_to_1(doc: &mut DocumentMut, resolved: &Resolved) {
    let root = doc.as_table_mut();
    if root.contains_key("ruleset") {
        root.remove("engine_rev");
    } else {
        replace_key(root, "engine_rev", "ruleset", value(resolved.ruleset.as_str()));
    }
    let compat = root.entry("compat").or_insert_with(|| Item::Table(Table::new()));
    if let Some(compat) = compat.as_table_mut()
        && !compat.contains_key("profile")
    {
        compat.set_implicit(false);
        compat.insert("profile", value(resolved.profile.as_str()));
    }
}

/// Puts `new = item` where `old` was, with `old`'s comments, and removes `old`. Appends when
/// `old` is absent.
fn replace_key(table: &mut Table, old: &str, new: &str, item: Item) {
    if !table.contains_key(old) {
        table.insert(new, item);
        return;
    }
    let keys: Vec<Key> = table.iter().filter_map(|(k, _)| table.key(k).cloned()).collect();
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        if let Some(it) = table.remove(key.get()) {
            entries.push((key, it));
        }
    }
    let mut item = Some(item);
    for (key, it) in entries {
        if key.get() == old {
            let mut renamed = Key::new(new);
            *renamed.leaf_decor_mut() = key.leaf_decor().clone();
            let mut replacement = item.take().unwrap_or_else(|| it.clone());
            // Keep a trailing comment of the old value (`engine_rev = 0  # why`).
            if let (Some(to), Some(from)) = (replacement.as_value_mut(), it.as_value()) {
                *to.decor_mut() = from.decor().clone();
            }
            table.insert_formatted(&renamed, replacement);
        } else {
            table.insert_formatted(&key, it);
        }
    }
}

/// Sets `format = <n>`, keeping the value's comments.
fn set_format(doc: &mut DocumentMut, format: u32) {
    if let Some(v) = doc.get_mut("format").and_then(Item::as_value_mut) {
        let decor = v.decor().clone();
        *v = toml_edit::Value::from(i64::from(format));
        *v.decor_mut() = decor;
    }
}

/// Points a data file's `#:schema` line at the current format's schema folder. Other lines and a
/// file without the line are unchanged.
#[must_use]
pub fn upgrade_schema_line(text: &str) -> String {
    let current = schema_dir(FORMAT_VERSION);
    let Some(first) = text.lines().next() else {
        return text.to_owned();
    };
    if !first.starts_with("#:schema ") {
        return text.to_owned();
    }
    let mut line = first.to_owned();
    for old in 0..FORMAT_VERSION {
        line = line.replace(&schema_dir(old), &current);
    }
    format!("{line}{}", &text[first.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved() -> Resolved {
        Resolved {
            ruleset: "gm8@0".to_owned(),
            profile: "gm8/c@1".to_owned(),
            deviations: Vec::new(),
            explicit: false,
        }
    }

    #[test]
    fn zero_to_one_keeps_comments_and_the_key_position() {
        let text = "#:schema ../schema/v0/game.schema.json\nformat = 0 # major\nid = \"x\"\n\
                    # why this revision\nengine_rev = 0  # verified\nrunner = \"gm81\"\n\n[timing]\n\
                    start = \"a\"\n";
        let out = upgrade_manifest(&upgrade_schema_line(text), &resolved()).expect("upgrade");
        assert_eq!(
            out,
            "#:schema ../schema/v1/game.schema.json\nformat = 1 # major\nid = \"x\"\n\
             # why this revision\nruleset = \"gm8@0\"  # verified\nrunner = \"gm81\"\n\n[timing]\n\
             start = \"a\"\n\n[compat]\nprofile = \"gm8/c@1\"\n"
        );
        assert_eq!(upgrade_manifest(&out, &resolved()).expect("current"), out);
    }

    #[test]
    fn zero_to_one_keeps_what_the_package_declared() {
        let text = "format = 0\nengine_rev = 0\nruleset = \"gm8@0\"\n\n[[compat.deviation]]\n\
                    id = \"save.position\"\nvalue = \"floor_nudge\"\n";
        let out = upgrade_manifest(text, &resolved()).expect("upgrade");
        assert!(!out.contains("engine_rev"), "{out}");
        assert_eq!(out.matches("ruleset").count(), 1, "{out}");
        let table: toml::Table = toml::from_str(&out).expect("valid TOML");
        assert_eq!(table["compat"]["profile"].as_str(), Some("gm8/c@1"));
        assert_eq!(
            table["compat"]["deviation"][0]["id"].as_str(),
            Some("save.position")
        );
    }

    #[test]
    fn only_the_schema_folder_of_the_first_line_changes() {
        assert_eq!(
            upgrade_schema_line("#:schema ../../schema/v0/room.schema.json\nx = \"/schema/v0/\"\n"),
            "#:schema ../../schema/v1/room.schema.json\nx = \"/schema/v0/\"\n"
        );
        assert_eq!(upgrade_schema_line("x = 1\n"), "x = 1\n");
        assert_eq!(upgrade_schema_line(""), "");
    }

    #[test]
    fn a_newer_format_is_refused() {
        assert!(upgrade_manifest("format = 99\n", &resolved()).is_err());
        assert!(upgrade_manifest("id = \"x\"\n", &resolved()).is_err());
    }
}
