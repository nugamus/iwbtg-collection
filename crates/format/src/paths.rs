//! Path files to `PathDef` (docs/plans/sim-core.md 8.2 step 7, 8.3; docs/format/v0.md, Paths).
//! WP0 creates the file; WP7b owns it.
//!
//! `paths/<id>.toml`: `points = [[x, y], [x, y, speed], ...]` (the speed factor defaults to
//! 100, GM's default), `closed`, `smooth` and `precision` (default 4). At least two points. A
//! smooth path loads, but `path::advance` refuses it (`Unsupported`, M8), so it is a warning.

use iwbtg_sim::def::PathDef;
use iwbtg_sim::num::Real;
use serde::Deserialize;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::number::Number;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathFile {
    points: Vec<Vec<Number>>,
    #[serde(default)]
    closed: bool,
    #[serde(default)]
    smooth: bool,
    #[serde(default = "default_precision")]
    precision: u32,
}

const fn default_precision() -> u32 {
    4
}

/// GM's default point speed factor.
const DEFAULT_SPEED: f64 = 100.0;

/// Reads every path file (step 7). The result's index is the `PathId`; ids are the file stems in
/// path order. `load::load_game` (WP7a) calls this.
pub fn load(files: &PackageFiles, diags: &mut Vec<Diagnostic>) -> Vec<PathDef> {
    let mut out = Vec::new();
    for (path, bytes) in files.iter() {
        let Some(id) = path.strip_prefix("paths/").and_then(|p| p.strip_suffix(".toml")) else {
            continue;
        };
        if id.contains('/') {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)) else {
            diags.push(Diagnostic::file_error(path, "file is not valid UTF-8"));
            continue;
        };
        let file = match toml::from_str::<PathFile>(text) {
            Ok(f) => f,
            Err(e) => {
                diags.push(Diagnostic::error_at(
                    path,
                    text,
                    e.span().map_or(0, |s| s.start),
                    e.message().to_owned(),
                ));
                continue;
            }
        };
        if file.points.len() < 2 {
            diags.push(Diagnostic::error_at(
                path,
                text,
                text.find("points").unwrap_or(0),
                "a path needs at least two points",
            ));
            continue;
        }
        let mut points = Vec::with_capacity(file.points.len());
        let mut ok = true;
        for p in &file.points {
            match p.as_slice() {
                [x, y] => points.push((Real::new(x.0), Real::new(y.0), Real::new(DEFAULT_SPEED))),
                [x, y, s] => points.push((Real::new(x.0), Real::new(y.0), Real::new(s.0))),
                _ => {
                    diags.push(Diagnostic::error_at(
                        path,
                        text,
                        text.find("points").unwrap_or(0),
                        "a point is [x, y] or [x, y, speed]",
                    ));
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        if file.smooth {
            diags.push(Diagnostic::warning_at(
                path,
                text,
                text.find("smooth").unwrap_or(0),
                "smooth paths are not supported by the sim yet (M8); following this path is an error",
            ));
        }
        out.push(PathDef {
            name: id.to_owned(),
            points,
            closed: file.closed,
            smooth: file.smooth,
            precision: file.precision,
        });
    }
    out
}
