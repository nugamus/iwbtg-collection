//! Timeline files to `TimelineDef` (docs/plans/sim-core.md 8.2 step 7; docs/format/v0.md,
//! Timelines). WP0 creates the file; WP7b owns it.
//!
//! `timelines/<id>.toml` holds `moments = [{ t = 0, call = "intro" }, ...]` in tick order and an
//! optional `loop`. The def keeps the ticks (`EventKind::Moment(step)` runs on the instance that
//! runs the timeline); the `call` names are for scripts (`moment_<call>`), which the script host
//! reads from the file itself. A timeline whose ticks do not ascend strictly is an error.
//! `loop = true` is accepted but the sim does not loop timelines yet: it is a warning.

use iwbtg_sim::def::TimelineDef;
use serde::Deserialize;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineFile {
    #[serde(default)]
    r#loop: bool,
    moments: Vec<Moment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Moment {
    t: u32,
    call: String,
}

/// Reads every timeline file (step 7). The result's index is the `TimelineId`; ids are the file
/// stems in path order. `load::load_game` (WP7a) calls this.
pub fn load(files: &PackageFiles, diags: &mut Vec<Diagnostic>) -> Vec<TimelineDef> {
    let mut out = Vec::new();
    for (path, bytes) in files.iter() {
        let Some(id) = path
            .strip_prefix("timelines/")
            .and_then(|p| p.strip_suffix(".toml"))
        else {
            continue;
        };
        if id.contains('/') {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)) else {
            diags.push(Diagnostic::file_error(path, "file is not valid UTF-8"));
            continue;
        };
        let file = match toml::from_str::<TimelineFile>(text) {
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
        let mut ok = true;
        for pair in file.moments.windows(2) {
            if pair[1].t <= pair[0].t {
                let at = text.find(&format!("t = {}", pair[1].t)).unwrap_or(0);
                diags.push(Diagnostic::error_at(
                    path,
                    text,
                    at,
                    format!("moments must ascend: t = {} follows t = {}", pair[1].t, pair[0].t),
                ));
                ok = false;
                break;
            }
        }
        for m in &file.moments {
            let name_ok = !m.call.is_empty()
                && m.call.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !m.call.starts_with(|c: char| c.is_ascii_digit());
            if !name_ok {
                diags.push(Diagnostic::error_at(
                    path,
                    text,
                    text.find(&m.call).unwrap_or(0),
                    format!(
                        "`call = \"{}\"` is not a function name (letters, digits, `_`)",
                        m.call
                    ),
                ));
                ok = false;
            }
        }
        if file.r#loop {
            diags.push(Diagnostic::warning_at(
                path,
                text,
                text.find("loop").unwrap_or(0),
                "`loop = true` is not supported by the sim yet; the timeline runs once",
            ));
        }
        if ok {
            out.push(TimelineDef {
                name: id.to_owned(),
                moments: file.moments.iter().map(|m| m.t).collect(),
            });
        }
    }
    out
}
