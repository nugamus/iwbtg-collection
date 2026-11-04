//! Lint: GML comparisons go through `num::gm_*` (docs/architecture.md, Sim purity).
//!
//! `Real`'s own `==` compares bits, and `a.get() < b.get()` or `x.get() == 0.0` compare plain
//! doubles. GM8 compares with its own rule (an epsilon, `NumRules`), so wherever the original
//! compared reals in GML, built-ins and the kid call `gm_eq` .. `gm_ge`. Both forms compile
//! without a warning, so this test reads the source of `src/builtins/` and `src/kid/` and fails
//! on a raw comparison that is not allowed.
//!
//! A line is flagged when it compares (`==`, `!=`, `<`, `>`, `<=`, `>=` with spaces around it) and
//! either calls `.get()` on a `Real` (an empty-argument `get`) or compares with a `Real::...`
//! constant. A raw comparison that is bookkeeping and not a GML comparison (counters, our own
//! state machine) carries the marker `gm-compare: ok (<why>)` in a comment on the same line or the
//! line above. There is no baseline: every site is fixed or marked. The lint cannot see a comparison of two `Real` values that never call `.get()` (for
//! example `inst.m.hspeed() == other`): review still has to catch those.

// A source scan: the test, not the sim, reads files and the environment. Not for wasm32 (no file system).
#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#![allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, reads the source files and the environment"
)]

use std::fs;
use std::path::{Path, PathBuf};

const MARKER: &str = "gm-compare: ok";
const OPERATORS: [&str; 6] = [" == ", " != ", " <= ", " >= ", " < ", " > "];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The part of a line that is code: everything before a `//` comment.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or(line)
}

/// True when the line is a raw comparison of reals.
fn is_raw_real_comparison(line: &str) -> bool {
    let c = code(line);
    if !OPERATORS.iter().any(|op| c.contains(op)) {
        return false;
    }
    let get = c.contains(".get()");
    let constant = [" == Real::", " != Real::"].iter().any(|p| c.contains(p))
        || c.trim_start().starts_with("Real::") && (c.contains(" == ") || c.contains(" != "));
    get || constant
}

/// `(path relative to src/, trimmed line)` of every flagged, unmarked line.
fn flagged(root: &Path) -> Vec<(String, String)> {
    let src = root.join("src");
    let mut files = Vec::new();
    rust_files(&src.join("builtins"), &mut files);
    rust_files(&src.join("kid"), &mut files);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            // Unit tests sit at the end of a file and may compare anything.
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            let t = line.trim_start();
            if t.starts_with("//") || !is_raw_real_comparison(line) {
                continue;
            }
            let marked = line.contains(MARKER) || i.checked_sub(1).is_some_and(|p| lines[p].contains(MARKER));
            if !marked {
                out.push((rel.clone(), line.trim().to_owned()));
            }
        }
    }
    out
}

fn entry((path, line): &(String, String)) -> String {
    format!("{path}: {line}")
}

#[test]
fn raw_comparisons_of_reals_are_marked() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let found: Vec<String> = flagged(root).iter().map(entry).collect();
    assert!(
        found.is_empty(),
        "raw comparison of reals (GML comparisons go through num::gm_eq .. gm_ge with the run's NumRules;          a comparison that is bookkeeping, not GML, gets a `{MARKER} (<why>)` comment on the line or the          line above):
  {}",
        found.join("
  ")
    );
}

#[test]
fn the_detector_sees_the_raw_forms_and_the_marker_and_gm_calls_pass() {
    assert!(is_raw_real_comparison("if a.get() == b.get() {"));
    assert!(is_raw_real_comparison("    if s.delay.get() > 0.0 {"));
    assert!(is_raw_real_comparison("if probe == Real::ZERO || ok {"));
    assert!(is_raw_real_comparison(
        "let dir = if h.get() < 0.0 { -1.0 } else { 1.0 };"
    ));
    assert!(!is_raw_real_comparison("if gm_eq(a, b, &ctx.rules.num) {"));
    assert!(!is_raw_real_comparison("let n = counter.get();"));
    assert!(!is_raw_real_comparison("fn f() -> Vec<Real> { x.get() }"));
    assert!(!is_raw_real_comparison(
        "if a == b { // a.get() == 1 in a comment"
    ));
    assert!(!is_raw_real_comparison("let x = map.get(\"k\");"));
}
