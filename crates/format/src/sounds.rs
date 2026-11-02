//! Sound entries to `SoundDef` (kind, length in ticks; docs/plans/sim-core.md 7, 8.3). WP0
//! creates the file; WP7b owns it.
//!
//! Every `sounds/<id>.ogg` is a normal sound and every `music/<id>.ogg` a MULTIMEDIA one (at
//! most one copy plays, S2). Ids are the file stems, sounds first then music, each in path
//! order; a stem used in both folders is an error (params name sounds by id alone). An optional
//! sidecar `<folder>/<id>.toml` gives `length_ticks` (the song length times the tick rate,
//! CR9), `volume` (default 1), `pan` (default 0) and `kind` (`normal` or `multimedia`) when the
//! folder's default is wrong. A sound without `length_ticks` gets length 0 and one warning
//! names them all: `sound_isplaying` needs the length (CR9).

use iwbtg_sim::def::{SoundDef, SoundKind};
use iwbtg_sim::num::Real;
use serde::Deserialize;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::number::Number;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    length_ticks: Option<u32>,
    volume: Option<Number>,
    pan: Option<Number>,
    kind: Option<KindName>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum KindName {
    Normal,
    Multimedia,
}

/// Reads every sound entry (step 7). The result's index is the `SoundId`. `trace` is
/// `LoadMode::Trace`: an id used in both folders is a warning, the sound wins. `load::load_game`
/// (WP7a) calls this.
pub fn load(files: &PackageFiles, trace: bool, diags: &mut Vec<Diagnostic>) -> Vec<SoundDef> {
    let mut out: Vec<SoundDef> = Vec::new();
    let mut no_length: Vec<String> = Vec::new();
    for (folder, default_kind) in [("sounds", SoundKind::Normal), ("music", SoundKind::Multimedia)] {
        for (path, _) in files.iter() {
            // A package sound is `sounds/<id>.ogg`; a bundled engine pack sound is
            // `engine/<pack>/sounds/<id>.ogg`, named `engine:<pack>/<id>` (docs/format/engine.md).
            let (base, stem, name) =
                if let Some((pack, id)) = crate::engine_pack::asset_file(path, folder, ".ogg") {
                    (
                        crate::engine_pack::pack_dir(pack),
                        id,
                        format!("{}{pack}/{id}", crate::ids::ENGINE_PREFIX),
                    )
                } else if let Some(id) = path
                    .strip_prefix(folder)
                    .and_then(|p| p.strip_prefix('/'))
                    .and_then(|p| p.strip_suffix(".ogg"))
                {
                    (String::new(), id, id.to_owned())
                } else {
                    continue;
                };
            if stem.contains('/') {
                continue;
            }
            let id = name.as_str();
            let side_stem = format!("{base}{folder}/{stem}");
            if out.iter().any(|s| s.name == id) {
                let message = format!("`{id}` is both a sound and a music track; ids must be unique");
                diags.push(if trace {
                    Diagnostic::file_warning(path, message)
                } else {
                    Diagnostic::file_error(path, message)
                });
                continue;
            }
            let side_path = format!("{side_stem}.toml");
            let mut def = SoundDef {
                name: id.to_owned(),
                kind: default_kind,
                length_ticks: 0,
                volume: Real::ONE,
                pan: Real::ZERO,
            };
            match files.get(&side_path) {
                None => no_length.push(id.to_owned()),
                Some(bytes) => {
                    let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
                    else {
                        diags.push(Diagnostic::file_error(&side_path, "file is not valid UTF-8"));
                        continue;
                    };
                    match toml::from_str::<Sidecar>(text) {
                        Ok(s) => {
                            match s.length_ticks {
                                Some(n) => def.length_ticks = n,
                                None => no_length.push(id.to_owned()),
                            }
                            if let Some(v) = s.volume {
                                def.volume = Real::new(v.0);
                            }
                            if let Some(p) = s.pan {
                                def.pan = Real::new(p.0);
                            }
                            match s.kind {
                                Some(KindName::Normal) => def.kind = SoundKind::Normal,
                                Some(KindName::Multimedia) => def.kind = SoundKind::Multimedia,
                                None => {}
                            }
                        }
                        Err(e) => {
                            diags.push(Diagnostic::error_at(
                                &side_path,
                                text,
                                e.span().map_or(0, |s| s.start),
                                e.message().to_owned(),
                            ));
                            continue;
                        }
                    }
                }
            }
            out.push(def);
        }
    }
    if !no_length.is_empty() {
        let shown: Vec<&str> = no_length.iter().take(12).map(String::as_str).collect();
        let more = no_length.len().saturating_sub(shown.len());
        let tail = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        diags.push(Diagnostic {
            file: "game.toml".to_owned(),
            line: 1,
            column: 1,
            severity: crate::diag::Severity::Warning,
            message: format!(
                "converter request CR9: sounds have no `length_ticks` sidecar (length 0) ({} item{}): {}{tail}",
                no_length.len(),
                if no_length.len() == 1 { "" } else { "s" },
                shown.join(", ")
            ),
        });
    }
    out
}
