//! `format_toml` on the repository's real data files: it never changes what a file means, and a
//! second run changes nothing (docs/decisions/0002-data-syntax.md, Conventions).

use std::fs;
use std::path::{Path, PathBuf};

use iwbtg_format::fmt::format_toml;

fn toml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries {
        let p = e.unwrap().path();
        if p.is_dir() {
            toml_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "toml") {
            out.push(p);
        }
    }
}

fn roots() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        manifest.join("tests/valid"),
        manifest.join("../sim/tests/golden/one_room.pkg"),
        manifest.join("../../docs/format/examples"),
    ]
}

#[test]
fn formatting_keeps_the_meaning_and_is_idempotent() {
    let mut files = Vec::new();
    for r in roots() {
        toml_files(&r, &mut files);
    }
    assert!(files.len() >= 8, "found only {} files", files.len());
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        let once = format_toml(&text).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let before: toml::Table = toml::from_str(&text).unwrap();
        let after: toml::Table =
            toml::from_str(&once).unwrap_or_else(|e| panic!("{}: output is not TOML: {e}", f.display()));
        assert_eq!(before, after, "{}: the meaning changed", f.display());
        let twice = format_toml(&once).unwrap();
        assert_eq!(once, twice, "{}: a second run changed it", f.display());
        assert!(once.ends_with('\n') && !once.contains('\r'), "{}", f.display());
    }
}
