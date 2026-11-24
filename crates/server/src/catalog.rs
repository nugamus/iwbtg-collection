//! Static game downloads (docs/server.md, Game downloads; #807).
//!
//! The configured folder holds the catalog and the content-addressed blobs:
//!
//! ```text
//! <catalog-dir>/
//!   catalog.json
//!   files/<sha256>[.<ext>]
//! ```
//!
//! A file name is accepted only when it is a lowercase SHA-256 with an optional short extension,
//! so `..`, encoded slashes and every other traversal shape fail the check before the filesystem
//! is touched: there is no path to escape. File names are their own hash, so their contents can
//! never change under a URL; the catalog can, so it gets a short cache instead.

use std::path::Path;

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::fs;

use crate::AppState;

/// The folder and file names inside the configured directory.
const CATALOG_FILE: &str = "catalog.json";
const FILES_DIR: &str = "files";

/// Hashed files never change under their URL, so caches may keep them for a year (immutable).
pub const FILE_CACHE: &str = "public, max-age=31536000, immutable";

/// The catalog lists files and their hashes, so it may change; a minute keeps downloads fresh
/// without putting every start-up on the origin.
pub const CATALOG_CACHE: &str = "public, max-age=60";

/// `GET /catalog.json` (docs/server.md, Game downloads).
pub async fn catalog(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let dir = state.catalog_dir.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let bytes = fs::read(dir.join(CATALOG_FILE))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(cached("application/json", CATALOG_CACHE, bytes))
}

/// `GET /files/<sha256>[.<ext>]` (docs/server.md, Game downloads).
///
/// A name that is not a hash is 404, not 400: there is nothing to distinguish a scanner's probe
/// from a client's typo, and a uniform answer leaks no layout.
pub async fn file(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> Result<Response, StatusCode> {
    let dir = state.catalog_dir.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if !is_hashed_name(&name) {
        return Err(StatusCode::NOT_FOUND);
    }
    let bytes = fs::read(dir.join(FILES_DIR).join(&name))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(cached(content_type(&name), FILE_CACHE, bytes))
}

/// A 200 response with a body and the two headers every download carries.
fn cached(content_type: &'static str, cache_control: &'static str, bytes: Vec<u8>) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
    response
}

/// `<64 lowercase hex>` with an optional `.<alphanumeric ext>`, and nothing else.
///
/// The strict alphabet is the traversal defence: no separator, no `.` inside the hash, no `..`
/// component can pass, so `dir.join(name)` always stays in `dir/files`.
fn is_hashed_name(name: &str) -> bool {
    let (hash, ext) = match name.split_once('.') {
        Some((hash, ext)) => (hash, Some(ext)),
        None => (name, None),
    };
    hash.len() == 64
        && hash.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        && ext.is_none_or(|ext| {
            !ext.is_empty() && ext.len() <= 16 && ext.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

/// The `Content-Type` for a validated name: enough to let a browser use an asset directly, with
/// the package blob as the default.
fn content_type(name: &str) -> &'static str {
    match Path::new(name).extension().and_then(|ext| ext.to_str()) {
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("zip" | "iwbz") => "application/zip",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::is_hashed_name;

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn accepts_a_hash_with_or_without_an_extension() {
        assert!(is_hashed_name(HASH));
        assert!(is_hashed_name(&format!("{HASH}.iwbz")));
        assert!(is_hashed_name(&format!("{HASH}.png")));
    }

    #[test]
    fn rejects_everything_that_is_not_a_hash() {
        for name in [
            "",
            "notes.txt",
            "..",
            "../secret",
            "..%2Fsecret",
            "/etc/passwd",
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.a/b",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.tar.gz",
        ] {
            assert!(!is_hashed_name(name), "accepted {name:?}");
        }
    }
}
