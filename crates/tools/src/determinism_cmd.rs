//! `iwbtg determinism report <corpus>`: runs the golden corpus and prints one line per case
//! (docs/architecture.md, The iwbtg CLI; docs/plans/sim-core.md 10.6).
//!
//! The cases are `crates/sim/tests/golden/mod.rs`, included here so the CLI and the sim's own
//! test run the same code. `<corpus>` is the golden folder: it holds `expected.txt`, the pinned
//! lines that `--check` compares against.

use std::path::Path;

use crate::trace_cmd::{ToolError, read_text};

#[path = "../../sim/tests/golden/mod.rs"]
mod golden;

/// Prints the report. With `check`, also compares it with `<corpus>/expected.txt`; `Ok(false)`
/// means a difference (the differing lines go to stderr).
pub fn main(corpus: &Path, check: bool) -> Result<bool, ToolError> {
    let report = golden::report();
    print!("{report}");
    if !check {
        return Ok(true);
    }
    let expected = read_text(&corpus.join(golden::EXPECTED_FILE))?.replace("\r\n", "\n");
    if expected == report {
        return Ok(true);
    }
    for (want, got) in expected.lines().zip(report.lines()) {
        if want != got {
            eprintln!("expected: {want}\n     got: {got}");
        }
    }
    if expected.lines().count() != report.lines().count() {
        eprintln!(
            "expected {} cases, ran {}",
            expected.lines().count(),
            report.lines().count()
        );
    }
    Ok(false)
}
