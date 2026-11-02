//! Engine packs (docs/format/engine.md): shared assets of one engine family.
//!
//! A pack lives in the games repo as `_engines/<pack>/` and is bundled into each package: the
//! built package carries `engine/<pack>/engine.toml` and the pack files it uses, laid out like a
//! package (`engine/<pack>/sprites/<id>.toml` and `.png`). A package refers to a pack sprite as
//! `engine:<pack>/<id>`; the loader keys such a sprite by that text. [`bundle`] makes the copy
//! (`iwbtg pkg check` runs it on a package folder next to its `_engines/` folder).
//! Only sprites resolve through the loader today; sounds and music of a pack are carried and
//! credited but no reference to them is read yet.

use std::collections::BTreeSet;

use serde::Deserialize;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::ids::{ENGINE_PREFIX, EngineRef, check_id};
use crate::manifest::{Creator, ThirdParty, toml_error};

/// Folder inside a package that holds the bundled engine packs.
pub const ENGINE_DIR: &str = "engine/";

/// Pack subfolders whose files an `engine:` reference can name.
const ASSET_DIRS: [&str; 3] = ["sprites", "sounds", "music"];

/// The physics presets a pack can match.
pub const FAMILIES: [&str; 7] = ["A1", "A2", "A3", "A4", "A5", "B", "C"];

/// `engine/<pack>/`, the folder of a bundled pack.
pub fn pack_dir(pack: &str) -> String {
    format!("{ENGINE_DIR}{pack}/")
}

/// For `engine/<pack>/sprites/<id>.toml`: `(pack, id)`.
pub fn sprite_sidecar(path: &str) -> Option<(&str, &str)> {
    let (pack, rest) = path.strip_prefix(ENGINE_DIR)?.split_once('/')?;
    let id = rest.strip_prefix("sprites/")?.strip_suffix(".toml")?;
    (!id.contains('/')).then_some((pack, id))
}

/// True for a bundled pack file that is gameplay data (sprites, sounds, music), so the gameplay
/// hash covers it; `engine.toml` (credits) is not.
pub fn is_gameplay_file(path: &str) -> bool {
    let Some((_, rest)) = path.strip_prefix(ENGINE_DIR).and_then(|r| r.split_once('/')) else {
        return false;
    };
    ASSET_DIRS
        .iter()
        .any(|d| rest.strip_prefix(d).is_some_and(|r| r.starts_with('/')))
}

/// `engine.toml`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EnginePack {
    /// Pack id; the pack's folder name.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub id: Spanned<String>,
    /// Display title.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub title: Spanned<String>,
    /// The physics preset the pack matches: A1, A2, A3, A4, A5, B or C.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub family: Spanned<String>,
    /// Ported games that use the pack (informational).
    #[serde(default)]
    pub games: Vec<String>,
    /// `pending` while the pack has no asset files yet, `complete` once it has them.
    #[serde(default)]
    pub assets: Option<PackAssets>,
    /// Who made the pack's content.
    pub creators: Vec<Creator>,
    /// One entry per file of the pack that is not ours; paths are relative to the pack folder
    /// (`sprites/kid_idle.png`).
    #[serde(default)]
    pub third_party: Vec<ThirdParty>,
}

/// Whether a pack has its asset files yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum PackAssets {
    /// No asset files yet.
    Pending,
    /// The asset files are in.
    Complete,
}

/// Checks the bundled pack `pack` (its `engine.toml` and the credits of its files).
pub fn validate(files: &PackageFiles, pack: &str, out: &mut Vec<Diagnostic>) {
    let path = format!("{}engine.toml", pack_dir(pack));
    let Some(bytes) = files.get(&path) else {
        out.push(Diagnostic::file_error(
            &path,
            format!(
                "the engine pack `{pack}` is not in the package (`iwbtg pkg check` bundles it from `../_engines/{pack}`)"
            ),
        ));
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        out.push(Diagnostic::file_error(&path, "file is not valid UTF-8"));
        return;
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let parsed = match toml::from_str::<EnginePack>(text) {
        Ok(p) => p,
        Err(e) => {
            out.push(toml_error(&path, text, &e));
            return;
        }
    };
    let mut err = |offset: usize, msg: String| {
        out.push(Diagnostic::error_at(&path, text, offset, msg));
    };
    if parsed.id.get_ref() != pack {
        err(
            parsed.id.span().start,
            format!("id `{}` must be the folder name `{pack}`", parsed.id.get_ref()),
        );
    } else if let Err(e) = check_id(parsed.id.get_ref()) {
        err(parsed.id.span().start, e);
    }
    if parsed.title.get_ref().trim().is_empty() {
        err(parsed.title.span().start, "title must not be empty".to_owned());
    }
    if !FAMILIES.contains(&parsed.family.get_ref().as_str()) {
        err(
            parsed.family.span().start,
            format!(
                "family `{}` must be one of {}",
                parsed.family.get_ref(),
                FAMILIES.join(", ")
            ),
        );
    }
    if parsed.creators.is_empty() {
        err(0, "a pack needs at least one `[[creators]]` entry".to_owned());
    }
    let mut listed = BTreeSet::new();
    for tp in &parsed.third_party {
        let rel = tp.path.get_ref();
        let full = format!("{}{rel}", pack_dir(pack));
        if rel.starts_with(ENGINE_PREFIX) || !files.contains(&full) {
            err(
                tp.path.span().start,
                format!("third_party path `{rel}` is not a file of the pack"),
            );
        }
        if !listed.insert(rel.as_str()) {
            err(
                tp.path.span().start,
                format!("third_party path `{rel}` is listed twice"),
            );
        }
    }
    let has_assets = files
        .iter()
        .any(|(p, _)| p.starts_with(&pack_dir(pack)) && is_gameplay_file(p));
    if parsed.assets == Some(PackAssets::Complete) && !has_assets {
        err(
            parsed.id.span().start,
            "assets is `complete` but the pack has no asset files".to_owned(),
        );
    }
}

/// True when `engine:<pack>/<id>` names a file of a bundled pack (`sprites/`, `sounds/` or
/// `music/`, any extension).
pub fn resolves(files: &PackageFiles, r: EngineRef<'_>) -> bool {
    let dir = pack_dir(r.pack);
    ASSET_DIRS.iter().any(|d| {
        let prefix = format!("{dir}{d}/{}.", r.id);
        files
            .iter()
            .any(|(p, _)| p.starts_with(&prefix) && !p[prefix.len()..].contains('/'))
    })
}

/// Every `engine:<pack>/<id>` named in a package's text files, as `(pack, id)`.
pub fn references(files: &PackageFiles) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for (path, bytes) in files.iter() {
        let text_file = path.ends_with(".toml") || path.ends_with(".rhai");
        if !text_file || path.starts_with(ENGINE_DIR) || path.starts_with("tests/") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let mut rest = text;
        while let Some(at) = rest.find(ENGINE_PREFIX) {
            rest = &rest[at..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '_')))
                .unwrap_or(rest.len());
            if let Some(Ok(r)) = EngineRef::parse(&rest[..end]) {
                out.insert((r.pack.to_owned(), r.id.to_owned()));
            }
            rest = &rest[ENGINE_PREFIX.len()..];
        }
    }
    out
}

/// Copies into `files` what the package uses of one pack. `pack_files` holds the files of
/// `_engines/<pack>/`, keyed relative to that folder. Copied: `engine.toml`, and for each
/// referenced `engine:<pack>/<id>` the files `sprites|sounds|music/<id>.*`. Does nothing when the
/// package already has `engine/<pack>/` (a built package). Returns the references that found no
/// file.
pub fn bundle(files: &mut PackageFiles, pack: &str, pack_files: &PackageFiles) -> Vec<String> {
    let dir = pack_dir(pack);
    if files.iter().any(|(p, _)| p.starts_with(&dir)) {
        return Vec::new();
    }
    let refs = references(files);
    let mut missing = Vec::new();
    if let Some(toml) = pack_files.get("engine.toml") {
        let _ = files.insert(format!("{dir}engine.toml"), toml.to_vec());
    }
    for (p, id) in refs.iter().filter(|(p, _)| p == pack) {
        let mut found = false;
        for (path, bytes) in pack_files.iter() {
            let Some((d, file)) = path.split_once('/') else {
                continue;
            };
            let stem = file.rsplit_once('.').map(|(s, _)| s);
            if ASSET_DIRS.contains(&d) && stem == Some(id.as_str()) {
                let _ = files.insert(format!("{dir}{path}"), bytes.to_vec());
                found = true;
            }
        }
        if !found {
            missing.push(format!("{ENGINE_PREFIX}{p}/{id}"));
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_files() -> PackageFiles {
        let mut f = PackageFiles::new();
        f.insert("engine.toml", b"id = \"a1\"\n".to_vec()).unwrap();
        f.insert("sprites/kid.toml", b"x = 1\n".to_vec()).unwrap();
        f.insert("sprites/kid.png", b"png".to_vec()).unwrap();
        f.insert("sprites/blood.png", b"png".to_vec()).unwrap();
        f
    }

    #[test]
    fn paths() {
        assert_eq!(sprite_sidecar("engine/a1/sprites/kid.toml"), Some(("a1", "kid")));
        assert_eq!(sprite_sidecar("engine/a1/sprites/kid.png"), None);
        assert_eq!(sprite_sidecar("sprites/kid.toml"), None);
        assert!(is_gameplay_file("engine/a1/sounds/jump.wav"));
        assert!(!is_gameplay_file("engine/a1/engine.toml"));
    }

    #[test]
    fn bundle_copies_only_what_is_used() {
        let mut game = PackageFiles::new();
        game.insert(
            "game.toml",
            b"sprite = \"engine:a1/kid\"\nother = \"engine:a1/gone\"\n".to_vec(),
        )
        .unwrap();
        let missing = bundle(&mut game, "a1", &pack_files());
        assert_eq!(missing, vec!["engine:a1/gone".to_owned()]);
        assert!(game.contains("engine/a1/engine.toml"));
        assert!(game.contains("engine/a1/sprites/kid.toml"));
        assert!(game.contains("engine/a1/sprites/kid.png"));
        assert!(!game.contains("engine/a1/sprites/blood.png"));
        // A built package is left alone.
        let before = game.len();
        bundle(&mut game, "a1", &pack_files());
        assert_eq!(game.len(), before);
    }

    #[test]
    fn references_ignore_other_text() {
        let mut game = PackageFiles::new();
        game.insert(
            "rooms/a.toml",
            b"a = \"engine:b/x\" # engine:c/y\nb = \"engine:bad\"\n".to_vec(),
        )
        .unwrap();
        let r = references(&game);
        assert!(r.contains(&("b".to_owned(), "x".to_owned())));
        assert!(r.contains(&("c".to_owned(), "y".to_owned())));
        assert_eq!(r.len(), 2);
    }
}
