//! `iwbtg pkg pack` and `iwbtg pkg unpack`: the `.iwbz` package file (docs/format/iwbz.md, #36).

use std::path::{Path, PathBuf};

use iwbtg_format::iwbz;

use crate::trace_cmd::{ToolError, load_package};

/// `iwbtg pkg pack <dir> [-o <file>] [--trace-mode]`: checks the package (with its engine pack
/// bundled), then writes `<id>.iwbz` (or `out`) and prints its path, size and both hashes. A
/// package with errors prints them and writes nothing (`Ok(false)`).
pub fn pack_main(dir: &Path, out: Option<&Path>, trace_mode: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    let (gameplay, content) = match crate::pkg_cmd::hashes(&files, trace_mode) {
        Ok(h) => h,
        Err(errors) => {
            for e in &errors {
                println!("{e}");
            }
            println!("FAILED: the package has {} errors; nothing written", errors.len());
            return Ok(false);
        }
    };
    let bytes = iwbz::pack(&files).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
    let out = match out {
        Some(o) => o.to_path_buf(),
        None => PathBuf::from(format!("{}.{}", package_id(&files)?, iwbz::EXTENSION)),
    };
    std::fs::write(&out, &bytes).map_err(|e| ToolError::Message(format!("{}: {e}", out.display())))?;
    println!("wrote {} ({} files, {} bytes)", out.display(), files.iter().filter(|(p, _)| !p.starts_with("tests/")).count(), bytes.len());
    println!("gameplay_hash: {gameplay}");
    println!("content_hash: {content}");
    Ok(true)
}

/// `iwbtg pkg unpack <file> <dir>`: unpacks an `.iwbz` into a new or empty folder and prints the
/// content hash. Every name and limit is checked before anything is written.
pub fn unpack_main(file: &Path, dir: &Path) -> Result<bool, ToolError> {
    let files = read_iwbz(file)?;
    iwbz::write_dir(&files, dir).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
    let hash: String = files.content_hash().iter().map(|b| format!("{b:02x}")).collect();
    println!("unpacked {} files into {}", files.len(), dir.display());
    println!("content_hash: {hash}");
    Ok(true)
}

/// Reads and unpacks an `.iwbz` file, refusing one over the size limit before reading it.
pub fn read_iwbz(file: &Path) -> Result<iwbtg_format::PackageFiles, ToolError> {
    let err = |e: String| ToolError::Message(format!("{}: {e}", file.display()));
    let len = std::fs::metadata(file).map_err(|e| err(e.to_string()))?.len();
    if len > iwbz::MAX_ARCHIVE_BYTES {
        return Err(err(format!("over {} bytes", iwbz::MAX_ARCHIVE_BYTES)));
    }
    let bytes = std::fs::read(file).map_err(|e| err(e.to_string()))?;
    iwbz::unpack(&bytes).map_err(|e| err(e.to_string()))
}

/// True when `path` names an `.iwbz` file rather than a package folder.
pub fn is_iwbz(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(iwbz::EXTENSION))
        && path.is_file()
}

fn package_id(files: &iwbtg_format::PackageFiles) -> Result<String, ToolError> {
    files
        .get("game.toml")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("id").and_then(|v| v.as_str().map(str::to_owned)))
        .ok_or_else(|| ToolError::Message("game.toml has no `id`; give the file name with -o".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_then_unpack_gives_the_same_package() {
        let root = std::env::temp_dir().join(format!("iwbtg-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/demo-clear.pkg");
        let file = root.join("demo.iwbz");
        assert!(pack_main(&demo, Some(&file), false).unwrap());
        let out = root.join("out");
        assert!(unpack_main(&file, &out).unwrap());
        let a = load_package(&demo).unwrap();
        let b = load_package(&out).unwrap();
        assert_eq!(a.content_hash(), b.content_hash());
        // `load_package` takes the file itself too.
        assert_eq!(load_package(&file).unwrap(), b);
        assert!(unpack_main(&file, &out).is_err(), "never into a folder with files");
        std::fs::remove_dir_all(&root).ok();
    }
}
