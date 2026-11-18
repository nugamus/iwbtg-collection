//! `PackageFiles`: a package as an in-memory map of path to bytes (docs/architecture.md, Crates).
//!
//! The client, the server and WASM all parse from this map, so they share one code path and the
//! parser never touches a filesystem. Only [`PackageFiles::load_dir`] reads disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Folder inside a package that is never shipped or hashed (docs/format/v0.md, Package layout).
pub const TESTS_DIR: &str = "tests/";

/// Package folders whose files have a fixed role (docs/format/v0.md, Package layout). A file in
/// one of them is never the physics profile.
pub const CONTENT_DIRS: [&str; 10] = [
    "objects/",
    "scripts/",
    "timelines/",
    "rooms/",
    "sprites/",
    "backgrounds/",
    "sounds/",
    "music/",
    "fonts/",
    "art/",
];

/// Why a package could not be put into memory. Problems with file contents are
/// [`crate::Diagnostic`]s instead.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A path is not a valid package path (see [`check_path`]).
    #[error("invalid package path `{path}`: {reason}")]
    InvalidPath {
        /// The rejected path.
        path: String,
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A file or folder name on disk is not valid UTF-8.
    #[error("file name is not UTF-8: {0}")]
    NotUtf8(PathBuf),
    /// Two paths differ only in ASCII case (`CREDITS.md` and `credits.md`). On Windows and
    /// macOS they name the same file, so unpacking one package there would lose a file.
    #[error("`{path}` and `{other}` differ only in case; package paths must be unique ignoring case")]
    CaseClash {
        /// The path being added.
        path: String,
        /// The path already in the package.
        other: String,
    },
    /// Packages hold only plain files and folders.
    #[error("symbolic links and special files are not allowed in a package: {0}")]
    NotAFile(PathBuf),
    /// Reading the disk failed.
    #[error("cannot read {path}: {source}")]
    Io {
        /// What was being read.
        path: PathBuf,
        /// The OS error.
        source: std::io::Error,
    },
}

/// Every file of one package, keyed by package-relative path with `/` separators
/// (`rooms/stage1_01.toml`). Sorted, so iteration order is the same everywhere. No two paths
/// are equal ignoring ASCII case, so a package unpacks to the same files on every file system.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageFiles {
    files: BTreeMap<String, Vec<u8>>,
    /// Every path in ASCII lowercase, mapped to the path itself.
    folded: BTreeMap<String, String>,
}

impl PackageFiles {
    /// An empty package.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one file. Returns the previous contents, if any. A path equal to one
    /// already there ignoring ASCII case, but not the same, is [`LoadError::CaseClash`].
    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) -> Result<Option<Vec<u8>>, LoadError> {
        let path = path.into();
        if let Err(reason) = check_path(&path) {
            return Err(LoadError::InvalidPath { path, reason });
        }
        let folded = path.to_ascii_lowercase();
        match self.folded.get(&folded) {
            Some(other) if *other != path => {
                return Err(LoadError::CaseClash {
                    other: other.clone(),
                    path,
                });
            }
            Some(_) => {}
            None => {
                self.folded.insert(folded, path.clone());
            }
        }
        Ok(self.files.insert(path, bytes))
    }

    /// The bytes of one file.
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// True when the package has this file.
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// All files in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files.iter().map(|(p, b)| (p.as_str(), b.as_slice()))
    }

    /// Number of files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when there are no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The download hash of the package: SHA-256 over every shipped file (docs/format/versioning.md).
    /// `tests/` is not shipped and is left out. Files are taken in path order; each
    /// contributes its path and its bytes, each as an 8-byte little-endian length and then the
    /// bytes, so moving bytes between files changes the hash. Text files (`.toml`, `.rhai`,
    /// `.md`, `.txt`, `.json`) have CRLF line ends turned into LF first, so a checkout with
    /// Windows line ends hashes like one with Unix line ends; every other file (PNG, Ogg) is
    /// hashed as it is.
    pub fn content_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"iwbtg-content-hash v0\0");
        for (path, bytes) in self.iter().filter(|(p, _)| !p.starts_with(TESTS_DIR)) {
            let text = ["toml", "rhai", "md", "txt", "json"].iter().any(|e| {
                path.rsplit_once('.')
                    .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(e))
            });
            let bytes: std::borrow::Cow<[u8]> = if text {
                lf_normalized(bytes).into()
            } else {
                bytes.into()
            };
            h.update((path.len() as u64).to_le_bytes());
            h.update(path.as_bytes());
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(&*bytes);
        }
        h.finalize().into()
    }

    /// Reads every file under `dir` into memory, recursively.
    ///
    /// Symbolic links are refused, so a package cannot point outside its folder.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let mut files = Self::new();
        load_into(&mut files, dir.as_ref(), "")?;
        Ok(files)
    }
}

/// `bytes` with every CR LF pair (0x0D 0x0A) turned into a single LF.
pub(crate) fn lf_normalized(bytes: &[u8]) -> Vec<u8> {
    const CR: u8 = 0x0D;
    const LF: u8 = 0x0A;
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == CR && bytes.get(i + 1) == Some(&LF) {
            i += 1;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

impl TryFrom<BTreeMap<String, Vec<u8>>> for PackageFiles {
    type Error = LoadError;

    /// Takes a ready map (for example from an unzipped download), checking every path.
    fn try_from(map: BTreeMap<String, Vec<u8>>) -> Result<Self, LoadError> {
        let mut files = Self::new();
        for (path, bytes) in map {
            files.insert(path, bytes)?;
        }
        Ok(files)
    }
}

fn load_into(files: &mut PackageFiles, dir: &Path, prefix: &str) -> Result<(), LoadError> {
    let io = |source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(io)? {
        entries.push(entry.map_err(io)?);
    }
    for entry in entries {
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| LoadError::NotUtf8(path.clone()))?;
        let rel = format!("{prefix}{name}");
        let kind = entry.file_type().map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            load_into(files, &path, &format!("{rel}/"))?;
        } else if kind.is_file() {
            let bytes = std::fs::read(&path).map_err(|source| LoadError::Io {
                path: path.clone(),
                source,
            })?;
            files.insert(rel, bytes)?;
        } else {
            return Err(LoadError::NotAFile(path));
        }
    }
    Ok(())
}

/// Checks a package-relative path: `/`-separated, not absolute, no empty, `.` or `..` parts,
/// no backslash or colon (a colon marks an `engine:` reference, never a file).
pub fn check_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("path is empty");
    }
    if path.starts_with('/') {
        return Err("path must be relative to the package folder");
    }
    if path.contains('\\') {
        return Err("use `/` as the separator, not `\\`");
    }
    if path.contains(':') {
        return Err("path must not contain `:`");
    }
    if path.chars().any(char::is_control) {
        return Err("path must not contain control characters");
    }
    for part in path.split('/') {
        match part {
            "" => return Err("path has an empty part (`//` or a trailing `/`)"),
            "." | ".." => return Err("path must not contain `.` or `..` parts"),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_rejects_bad_paths() {
        let mut files = PackageFiles::new();
        for bad in [
            "",
            "/game.toml",
            "rooms\\a.toml",
            "../x",
            "a//b",
            "a/",
            "./a",
            "c:/x",
            "engine:p/x",
        ] {
            assert!(files.insert(bad, Vec::new()).is_err(), "{bad:?} accepted");
        }
        assert!(files.insert("rooms/stage1_01.toml", b"x".to_vec()).is_ok());
        assert_eq!(files.get("rooms/stage1_01.toml"), Some(&b"x"[..]));
        assert!(files.contains("rooms/stage1_01.toml"));
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn insert_rejects_paths_equal_ignoring_case() {
        let mut files = PackageFiles::new();
        files.insert("CREDITS.md", b"a".to_vec()).expect("first");
        let err = files.insert("credits.md", b"b".to_vec()).expect_err("case clash");
        assert!(
            matches!(&err, LoadError::CaseClash { path, other } if path == "credits.md" && other == "CREDITS.md"),
            "{err}"
        );
        assert!(files.insert("Rooms/A.toml", Vec::new()).is_ok());
        assert!(files.insert("rooms/a.toml", Vec::new()).is_err());
        // The same path again still replaces it.
        assert_eq!(
            files.insert("CREDITS.md", b"c".to_vec()).expect("replace"),
            Some(b"a".to_vec())
        );
        assert_eq!(files.get("CREDITS.md"), Some(&b"c"[..]));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn try_from_map_checks_paths() {
        let mut map = BTreeMap::new();
        map.insert("game.toml".to_owned(), Vec::new());
        assert!(PackageFiles::try_from(map.clone()).is_ok());
        map.insert("../evil".to_owned(), Vec::new());
        assert!(PackageFiles::try_from(map).is_err());
    }

    #[test]
    fn load_dir_reads_nested_files_with_slash_paths() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/valid/minimal");
        let files = PackageFiles::load_dir(dir).expect("fixture loads");
        assert!(files.contains("game.toml"));
        assert!(files.contains("rooms/title.toml"));
    }
}
