//! `iwbtg pkg publish`: pack a package, write `<content_hash>.iwbz` and update `catalog.json` in
//! a folder a static server serves (docs/server.md, Game downloads; #806).
//!
//! The catalog is one JSON object with a `files` array. Each entry carries the package-owned
//! fields that exist today (`game.toml` plus the two hashes); the rest of the game metadata model
//! is #805. Unknown top-level fields are preserved, so a newer catalog keeps working, and the
//! catalog holds one entry per package `id`: publishing a newer version replaces it and leaves the
//! older content-addressed file in `files/`.

use std::path::Path;

use iwbtg_format::manifest::{CreatorRole, Manifest, Permission};
use iwbtg_format::{iwbz, pkg_cmd};
use serde::Serialize;
use serde_json::{Value, json};

use crate::trace_cmd::{ToolError, load_package};

/// `iwbtg pkg publish <pkg> --catalog-dir <folder> [--trace-mode]`.
///
/// A package with errors prints them, writes nothing and returns `Ok(false)` (exit 1).
pub fn publish_main(dir: &Path, catalog_dir: &Path, trace_mode: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    let (gameplay, content) = match pkg_cmd::hashes(&files, trace_mode) {
        Ok(hashes) => hashes,
        Err(errors) => {
            for error in &errors {
                println!("{error}");
            }
            println!(
                "FAILED: the package has {} errors; nothing published",
                errors.len()
            );
            return Ok(false);
        }
    };
    let mut diagnostics = Vec::new();
    let manifest = iwbtg_format::check::load_manifest(&files, &mut diagnostics)
        .map(|(manifest, _)| manifest)
        .ok_or_else(|| {
            ToolError::Message(format!(
                "{}: game.toml does not read:\n  {}",
                dir.display(),
                diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ))
        })?;
    let bytes = iwbz::pack(&files).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
    let filename = format!("{content}.{}", iwbz::EXTENSION);
    let files_dir = catalog_dir.join("files");
    std::fs::create_dir_all(&files_dir)
        .map_err(|e| ToolError::Message(format!("{}: {e}", files_dir.display())))?;
    let blob = files_dir.join(&filename);
    write_blob(&blob, &bytes)?;

    let entry = Entry::new(&manifest, &filename, bytes.len() as u64, &gameplay, &content);
    let catalog_path = catalog_dir.join("catalog.json");
    let mut catalog = read_catalog(&catalog_path)?;
    let entries = catalog_entries(&mut catalog, &catalog_path)?;
    // One entry per package id: a newer version replaces the old one. The old file stays in
    // `files/`; a content-addressed URL never changes.
    entries.retain(|existing| {
        existing.get("id").and_then(Value::as_str) != Some(entry.id.as_str())
            && existing.get("filename").and_then(Value::as_str) != Some(filename.as_str())
    });
    entries
        .push(serde_json::to_value(&entry).map_err(|e| ToolError::Message(format!("catalog entry: {e}")))?);
    entries.sort_by(|a, b| {
        a.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("id").and_then(Value::as_str).unwrap_or(""))
    });
    let entry_count = entries.len();
    let mut text = serde_json::to_string_pretty(&catalog)
        .map_err(|e| ToolError::Message(format!("{}: {e}", catalog_path.display())))?;
    text.push('\n');
    write_atomic(&catalog_path, text.as_bytes())?;

    println!("published {} {}", entry.id, entry.version);
    println!("wrote {} ({} bytes)", blob.display(), bytes.len());
    println!("catalog: {} ({entry_count} file(s))", catalog_path.display());
    Ok(true)
}

/// One `catalog.json` file entry: the package fields that exist before #805 defines the rest.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Entry {
    id: String,
    title: String,
    version: String,
    format: u32,
    url: String,
    filename: String,
    size: u64,
    content_hash: String,
    gameplay_hash: String,
    creators: Vec<CreatorEntry>,
    source: SourceEntry,
}

impl Entry {
    fn new(manifest: &Manifest, filename: &str, size: u64, gameplay: &str, content: &str) -> Self {
        let source = manifest.source.get_ref();
        Self {
            id: manifest.id.get_ref().clone(),
            title: manifest.title.get_ref().clone(),
            version: manifest.version.get_ref().clone(),
            format: *manifest.format.get_ref(),
            url: format!("/files/{filename}"),
            filename: filename.to_owned(),
            size,
            content_hash: content.to_owned(),
            gameplay_hash: gameplay.to_owned(),
            creators: manifest
                .creators
                .get_ref()
                .iter()
                .map(|creator| CreatorEntry {
                    name: creator.name.get_ref().clone(),
                    role: role_name(creator.role).to_owned(),
                })
                .collect(),
            source: SourceEntry {
                url: source.url.get_ref().clone(),
                permission: permission_name(*source.permission.get_ref()).to_owned(),
            },
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct CreatorEntry {
    name: String,
    role: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct SourceEntry {
    url: String,
    permission: String,
}

/// The manifest's kebab-case creator role, as `game.toml` writes it.
fn role_name(role: CreatorRole) -> &'static str {
    match role {
        CreatorRole::Author => "author",
        CreatorRole::CoAuthor => "co-author",
        CreatorRole::Music => "music",
        CreatorRole::Art => "art",
        CreatorRole::Tester => "tester",
        CreatorRole::Other => "other",
    }
}

/// The manifest's kebab-case permission, as `game.toml` writes it.
fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::Granted => "granted",
        Permission::CommunityAssumed => "community-assumed",
        Permission::Pending => "pending",
        Permission::Revoked => "revoked",
    }
}

/// Reads `catalog.json`; a missing file starts an empty catalog, and an unreadable or non-object
/// one is an error (never overwritten).
fn read_catalog(path: &Path) -> Result<Value, ToolError> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let value: Value = serde_json::from_str(&text)
                .map_err(|e| ToolError::Message(format!("{}: not valid JSON: {e}", path.display())))?;
            if !value.is_object() {
                return Err(ToolError::Message(format!(
                    "{}: expected a JSON object",
                    path.display()
                )));
            }
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({ "files": [] })),
        Err(e) => Err(ToolError::Message(format!("{}: {e}", path.display()))),
    }
}

/// The catalog's `files` array, created when the catalog has none.
fn catalog_entries<'a>(catalog: &'a mut Value, path: &Path) -> Result<&'a mut Vec<Value>, ToolError> {
    let object = catalog
        .as_object_mut()
        .ok_or_else(|| ToolError::Message(format!("{}: expected a JSON object", path.display())))?;
    object
        .entry("files")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| ToolError::Message(format!("{}: `files` is not an array", path.display())))
}

/// Writes a content-addressed blob: identical bytes are already published, different bytes at the
/// same hash are refused (a content URL must never change).
fn write_blob(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    match std::fs::read(path) {
        Ok(existing) if existing == bytes => Ok(()),
        Ok(_) => Err(ToolError::Message(format!(
            "{}: already exists with different bytes; refusing to replace a content-addressed file",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => write_atomic(path, bytes),
        Err(e) => Err(ToolError::Message(format!("{}: {e}", path.display()))),
    }
}

/// Writes beside the target and renames, so a server reading the folder never sees a half file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, bytes).map_err(|e| ToolError::Message(format!("{}: {e}", temp.display())))?;
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(ToolError::Message(format!("{}: {e}", path.display())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../format/tests/valid")
            .join(name)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-publish-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn catalog(dir: &Path) -> Value {
        let text = std::fs::read_to_string(dir.join("catalog.json")).expect("catalog.json");
        serde_json::from_str(&text).expect("catalog JSON")
    }

    #[test]
    fn two_packages_publish_into_one_catalog() {
        let root = scratch("two");
        assert!(publish_main(&fixture("minimal"), &root, false).unwrap());
        assert!(publish_main(&fixture("collider"), &root, false).unwrap());
        let value = catalog(&root);
        let files = value["files"].as_array().expect("files array");
        assert_eq!(files.len(), 2);
        let ids: Vec<&str> = files.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["collider", "minimal"], "sorted by id");
        for entry in files {
            let name = entry["filename"].as_str().expect("filename");
            let hash = entry["content_hash"].as_str().expect("content_hash");
            assert_eq!(name, format!("{hash}.iwbz"));
            assert_eq!(entry["url"], format!("/files/{name}"));
            let path = root.join("files").join(name);
            assert_eq!(
                entry["size"].as_u64().unwrap(),
                std::fs::metadata(&path).unwrap().len()
            );
        }
        let before = std::fs::read(root.join("catalog.json")).unwrap();
        assert!(publish_main(&fixture("minimal"), &root, false).unwrap());
        assert_eq!(
            catalog(&root)["files"].as_array().unwrap().len(),
            2,
            "re-publishing replaces, never duplicates"
        );
        assert_eq!(
            std::fs::read(root.join("catalog.json")).unwrap(),
            before,
            "re-publishing the same package leaves the catalog byte-identical"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn publishing_keeps_unknown_catalog_fields() {
        let root = scratch("future");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("catalog.json"),
            r#"{"format": 2, "files": [], "extra": {"a": 1}}"#,
        )
        .unwrap();
        assert!(publish_main(&fixture("minimal"), &root, false).unwrap());
        let value = catalog(&root);
        assert_eq!(value["format"], 2);
        assert_eq!(value["extra"]["a"], 1);
        assert_eq!(value["files"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_invalid_catalog_is_not_overwritten() {
        let root = scratch("invalid");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("catalog.json"), "not json").unwrap();
        let err = publish_main(&fixture("minimal"), &root, false).unwrap_err();
        assert!(err.to_string().contains("catalog.json"), "{err}");
        assert_eq!(
            std::fs::read_to_string(root.join("catalog.json")).unwrap(),
            "not json"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_different_file_at_a_content_url_is_refused() {
        let root = scratch("tampered");
        assert!(publish_main(&fixture("minimal"), &root, false).unwrap());
        let entry = catalog(&root)["files"][0].clone();
        let path = root.join("files").join(entry["filename"].as_str().unwrap());
        std::fs::write(&path, b"tampered").unwrap();
        let err = publish_main(&fixture("minimal"), &root, false).unwrap_err();
        assert!(err.to_string().contains("different bytes"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), b"tampered");
        let _ = std::fs::remove_dir_all(&root);
    }
}
