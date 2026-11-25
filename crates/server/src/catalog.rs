//! Static game downloads (docs/server.md, Game downloads; #807, #1397).
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
//!
//! Files answer a single `Range` request so a dropped download resumes instead of restarting. The
//! parsing rules are RFC 9110's: an invalid header is ignored (full 200), a syntactically valid
//! range that lies past the end is 416, and a multi-range request is answered in full rather than
//! with a multipart body.
//!
//! Responses stream from disk with an explicit `Content-Length`; a package is never buffered whole,
//! so a 512 MB package costs socket buffers rather than its size (#1506).

use std::io::SeekFrom;
use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

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
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let dir = state.catalog_dir.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if !is_hashed_name(&name) {
        return Err(StatusCode::NOT_FOUND);
    }
    let path = dir.join(FILES_DIR).join(&name);
    let len = fs::metadata(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?
        .len();

    let requested = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_range(value, len));

    match requested {
        None | Some(ByteRange::Full) => stream(&path, &name, 0, len, None).await,
        Some(ByteRange::Satisfiable { start, end }) => {
            let content_range = format!("bytes {start}-{end}/{len}");
            stream(&path, &name, start, end - start + 1, Some(&content_range)).await
        }
        Some(ByteRange::Unsatisfiable) => Ok(range_not_satisfiable(len)),
    }
}

/// What a `Range` header asks for, already resolved against the file length.
#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    /// No usable range: answer the whole file (no header, invalid syntax, several ranges).
    Full,
    /// A single range whose bounds are inside the file.
    Satisfiable { start: u64, end: u64 },
    /// A syntactically valid range that selects no byte (e.g. `bytes=10-` on a 10-byte file).
    Unsatisfiable,
}

/// Parses one `Range: bytes=...` value against `len` (RFC 9110, section 14.1.2).
///
/// Invalid syntax, another unit and multi-range requests are [`ByteRange::Full`]: the header is
/// ignored, which the RFC allows and which keeps this handler to one code path. Only a valid
/// single range is answered with 206 or 416.
fn parse_range(value: &str, len: u64) -> ByteRange {
    let Some((unit, set)) = value.split_once('=') else {
        return ByteRange::Full;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") || set.contains(',') {
        return ByteRange::Full;
    }
    let Some((first, last)) = set.trim().split_once('-') else {
        return ByteRange::Full;
    };

    if first.is_empty() {
        // `-suffix`: the last N bytes. A zero-length suffix selects nothing.
        let Some(suffix) = digits(last) else {
            return ByteRange::Full;
        };
        if suffix == 0 || len == 0 {
            return ByteRange::Unsatisfiable;
        }
        return ByteRange::Satisfiable {
            start: len.saturating_sub(suffix),
            end: len - 1,
        };
    }

    let Some(start) = digits(first) else {
        return ByteRange::Full;
    };
    if start >= len {
        return ByteRange::Unsatisfiable;
    }
    if last.is_empty() {
        // `start-`: to the end of the file.
        return ByteRange::Satisfiable { start, end: len - 1 };
    }
    let Some(end) = digits(last) else {
        return ByteRange::Full;
    };
    if end < start {
        // A last-byte-pos before the first makes the spec invalid, so the header is ignored.
        return ByteRange::Full;
    }
    ByteRange::Satisfiable {
        start,
        end: end.min(len - 1),
    }
}

/// A non-empty run of ASCII digits, or `None` (which makes the header invalid).
///
/// `u64::from_str` would also accept a leading `+`; Range syntax allows digits only.
fn digits(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Streams `len` bytes starting at `start`, with the headers every file response carries.
///
/// The file is read through a stream, never into a `Vec`: a package costs its socket buffers
/// instead of its size, which matters because the format allows 512 MB and the container has
/// 512 MiB (#1506). A range seeks first and takes exactly its length; `Content-Length` stays
/// explicit so a client still sees the size before the body arrives.
async fn stream(
    path: &Path,
    name: &str,
    start: u64,
    len: u64,
    content_range: Option<&str>,
) -> Result<Response, StatusCode> {
    let mut file = fs::File::open(path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;
    }
    let body = Body::from_stream(ReaderStream::new(file.take(len)));
    let mut response = body.into_response();
    if content_range.is_some() {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(name)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(FILE_CACHE));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&len.to_string()).expect("a decimal length"),
    );
    if let Some(content_range) = content_range {
        headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(content_range).expect("an ASCII Content-Range"),
        );
    }
    Ok(response)
}

/// The 416 response: no body, and `Content-Range` says how long the file actually is.
fn range_not_satisfiable(len: u64) -> Response {
    let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    let headers = response.headers_mut();
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let content_range = format!("bytes */{len}");
    headers.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&content_range).expect("an ASCII Content-Range"),
    );
    response
}

/// A response with a body and the two headers the catalog carries.
pub(crate) fn cached(content_type: &'static str, cache_control: &'static str, bytes: Vec<u8>) -> Response {
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
    use super::{ByteRange, is_hashed_name, parse_range};

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn range(start: u64, end: u64) -> ByteRange {
        ByteRange::Satisfiable { start, end }
    }

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

    #[test]
    fn parses_the_three_single_range_forms() {
        assert_eq!(parse_range("bytes=0-4", 10), range(0, 4));
        assert_eq!(parse_range("bytes=5-", 10), range(5, 9));
        assert_eq!(parse_range("bytes=-3", 10), range(7, 9));
        assert_eq!(parse_range("bytes=-30", 10), range(0, 9));
    }

    #[test]
    fn clamps_a_last_byte_past_the_end() {
        assert_eq!(parse_range("bytes=8-999", 10), range(8, 9));
        assert_eq!(parse_range("bytes=0-0", 10), range(0, 0));
    }

    #[test]
    fn a_range_past_the_end_is_unsatisfiable() {
        assert_eq!(parse_range("bytes=10-", 10), ByteRange::Unsatisfiable);
        assert_eq!(parse_range("bytes=10-20", 10), ByteRange::Unsatisfiable);
        assert_eq!(parse_range("bytes=-0", 10), ByteRange::Unsatisfiable);
        // An empty file has no byte any range can select.
        assert_eq!(parse_range("bytes=0-", 0), ByteRange::Unsatisfiable);
        assert_eq!(parse_range("bytes=-5", 0), ByteRange::Unsatisfiable);
    }

    #[test]
    fn an_invalid_header_is_ignored() {
        for value in [
            "",
            "bytes",
            "bytes=",
            "bytes=abc",
            "bytes=+1-2",
            "bytes=1-2-3",
            // last-byte-pos before first-byte-pos is an invalid spec (RFC 9110 14.1.2).
            "bytes=5-2",
            // Several ranges would need a multipart body; answer in full instead.
            "bytes=0-1,3-4",
            // Another unit is not ours.
            "items=0-1",
            // Overflowing numbers are invalid, not unsatisfiable.
            "bytes=18446744073709551616-",
        ] {
            assert_eq!(parse_range(value, 10), ByteRange::Full, "for {value:?}");
        }
    }

    #[test]
    fn the_unit_is_case_insensitive() {
        assert_eq!(parse_range("Bytes=0-4", 10), range(0, 4));
    }
}
