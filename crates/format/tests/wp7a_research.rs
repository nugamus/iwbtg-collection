//! Every research converter package loads in `LoadMode::Trace` with warnings only
//! (docs/plans/sim-core.md 12, WP7a done-criteria). Needs `IWBTG_RESEARCH` (the research repo
//! folder); skips when it is unset. Packages are in `<research>/build/packages/<id>/`
//! (gitignored there, built by the converter).

use std::path::PathBuf;

use iwbtg_format::PackageFiles;
use iwbtg_format::load::{LoadMode, load_report};

fn packages_dir() -> Option<PathBuf> {
    let Some(root) = std::env::var_os("IWBTG_RESEARCH") else {
        eprintln!("skipped: IWBTG_RESEARCH is not set");
        return None;
    };
    let dir = PathBuf::from(root).join("build").join("packages");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipped: {} does not exist (run the research converter)", dir.display());
        None
    }
}

#[test]
fn every_converter_package_loads_in_trace_mode() {
    let Some(dir) = packages_dir() else {
        return;
    };
    let mut games: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("packages folder is readable")
        .map(|e| e.expect("readable entry").path())
        .filter(|p| p.join("game.toml").is_file())
        .collect();
    games.sort();
    assert!(!games.is_empty(), "no packages in {}", dir.display());
    let mut failures = Vec::new();
    for game in &games {
        let files = PackageFiles::load_dir(game).expect("package reads");
        let report = load_report(
            &files,
            LoadMode::Trace {
                unmapped_kinds_inert: true,
            },
        );
        let name = game.file_name().unwrap_or_default().to_string_lossy().into_owned();
        eprintln!("{name}: {} warnings", report.diagnostics.len());
        if !report.ok() {
            let errors: Vec<String> = report
                .diagnostics
                .iter()
                .filter(|d| d.is_error())
                .take(5)
                .map(ToString::to_string)
                .collect();
            failures.push(format!("{name}:\n  {}", errors.join("\n  ")));
        }
        if name == "rely_the_needle" {
            let all: String = report.diagnostics.iter().map(|d| format!("{d}\n")).collect();
            for cr in ["CR1", "CR3", "CR4", "CR5"] {
                assert!(all.contains(&format!("converter request {cr}")), "rely names {cr}:\n{all}");
            }
            assert!(all.contains("CR1, CR2"), "rely names CR2:\n{all}");
        }
    }
    assert!(failures.is_empty(), "packages that do not load:\n{}", failures.join("\n"));
}
