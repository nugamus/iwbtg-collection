//! Package validation, the code behind `iwbtg pkg check` (docs/format/v0.md, Validation).
//!
//! The client and the server run the same checks on load and refuse a package with any error.
//! Each file type gets its own step function; this first slice checks the manifest and the
//! package limits. Later steps (physics profile, objects, rooms, timelines, scripts, assets,
//! references across files) are added as further steps in [`check`].

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
pub use crate::files::TESTS_DIR;
use crate::manifest::{MANIFEST_PATH, Manifest};

/// Largest package, counting every shipped file (`tests/` is not shipped). 512 MiB.
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

/// Runs every validation step on a package and returns all problems found, in step order.
/// An empty list means the package is valid.
pub fn check(files: &PackageFiles) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    check_limits(files, &mut out);
    if let Some((manifest, text)) = load_manifest(files, &mut out) {
        manifest.validate(text, files, &mut out);
    }
    out
}

/// Reads and parses `game.toml`. On failure, pushes the diagnostic and returns `None`.
/// Also warns when the first line is not the `#:schema` comment. Returns the source text
/// with the manifest so later steps can point at positions in it.
pub fn load_manifest<'f>(files: &'f PackageFiles, out: &mut Vec<Diagnostic>) -> Option<(Manifest, &'f str)> {
    let text = read_text(files, MANIFEST_PATH, out)?;
    check_schema_line(MANIFEST_PATH, text, out);
    match Manifest::parse(text) {
        Ok(manifest) => Some((manifest, text)),
        Err(e) => {
            out.push(e);
            None
        }
    }
}

/// UTF-8 byte order mark. Some editors (older Windows Notepad) write it; TOML allows it.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The text of a required data file. Missing or non-UTF-8 files become diagnostics.
///
/// A leading UTF-8 byte order mark is dropped, so the `#:schema` check sees the first line and
/// columns on line 1 do not count it. All spans and positions are relative to the returned text.
fn read_text<'f>(files: &'f PackageFiles, path: &str, out: &mut Vec<Diagnostic>) -> Option<&'f str> {
    let Some(bytes) = files.get(path).map(|b| b.strip_prefix(UTF8_BOM).unwrap_or(b)) else {
        out.push(Diagnostic::file_error(
            path,
            format!("the package has no `{path}`"),
        ));
        return None;
    };
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(e) => {
            // The valid prefix locates the bad byte.
            let valid = std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or("");
            out.push(Diagnostic::error_at(
                path,
                valid,
                valid.len(),
                "file is not valid UTF-8",
            ));
            None
        }
    }
}

/// Warns when the first line is not a `#:schema` comment (docs/format/v0.md, Common rules).
/// A warning, not an error: the comment only feeds editor tooling.
fn check_schema_line(path: &str, text: &str, out: &mut Vec<Diagnostic>) {
    if !text.starts_with("#:schema ") {
        out.push(Diagnostic::warning_at(
            path,
            text,
            0,
            "the first line should be a `#:schema` comment pointing at the JSON Schema",
        ));
    }
}

/// Package-wide limits (docs/format/v0.md, Assets).
fn check_limits(files: &PackageFiles, out: &mut Vec<Diagnostic>) {
    let total: u64 = files
        .iter()
        .filter(|(path, _)| !path.starts_with(TESTS_DIR))
        .map(|(_, bytes)| u64::try_from(bytes.len()).unwrap_or(u64::MAX))
        .fold(0, u64::saturating_add);
    if total > MAX_PACKAGE_BYTES {
        out.push(Diagnostic::file_error(
            MANIFEST_PATH,
            format!("the package is {total} bytes; the limit is {MAX_PACKAGE_BYTES}"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_manifest() {
        let d = check(&PackageFiles::new());
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].to_string(),
            "game.toml:1:1: error: the package has no `game.toml`"
        );
    }

    #[test]
    fn non_utf8_manifest_points_at_bad_byte() {
        let mut files = PackageFiles::new();
        files
            .insert("game.toml", b"#:schema x\nid = \"\xff\"".to_vec())
            .unwrap();
        let d = check(&files);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].line, d[0].column), (2, 7));
        assert!(d[0].message.contains("UTF-8"));
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        let mut files = PackageFiles::new();
        files
            .insert("game.toml", b"\xEF\xBB\xBF#:schema x\nformat = -1\n".to_vec())
            .unwrap();
        let d = check(&files);
        // No `#:schema` warning; only the parse error, at the same place as without the mark.
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].is_error());
        assert_eq!((d[0].line, d[0].column), (2, 10));

        let mut files = PackageFiles::new();
        files
            .insert("game.toml", b"\xEF\xBB\xBFid = \"\xff\"".to_vec())
            .unwrap();
        let d = check(&files);
        // Column 7, not 8: the mark is not a character of line 1.
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!((d[0].line, d[0].column), (1, 7));
    }
}
