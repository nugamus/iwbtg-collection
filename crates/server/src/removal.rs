//! `GET /removals.json` (docs/server.md, Removal list; #808).
//!
//! Takedowns (#68) and moderation (#800) publish `<catalog-dir>/removals.json`; the server serves
//! those exact bytes. The file is already signed and the server holds no key: the signing key
//! stays off the public host, and a client trusts the list only after checking `signature`
//! against the public key it ships. The server's job is to catch a broken publish (503) instead
//! of letting it look like "no removals" (404).
//!
//! The format and its checks live in `iwbtg-removal`, shared with `iwbtg removals sign` (#1537),
//! so the canonical text the signature covers cannot drift between producer and validator.

use std::io::ErrorKind;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use iwbtg_removal::{MAX_BYTES, parse};
use tokio::fs;

use crate::AppState;
use crate::catalog;

/// The signed list inside the configured catalog folder (docs/server.md, Removal list).
const REMOVALS_FILE: &str = "removals.json";

/// Removals must reach clients quickly, so the list is cached like the catalog: a minute.
pub const REMOVALS_CACHE: &str = "public, max-age=60";

/// `GET /removals.json` (docs/server.md, Removal list).
///
/// `404` means there are no removals (or downloads are disabled). Any published file that fails
/// [`parse`] is `503` with a logged reason: a client must not read a broken publish as "nothing
/// was taken down" and keep the package installed.
pub async fn removals(State(state): State<AppState>) -> Response {
    let Some(dir) = state.catalog_dir.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = dir.join(REMOVALS_FILE);
    let metadata = match fs::metadata(&path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(err) => {
            tracing::error!(%err, path = %path.display(), "cannot read the removal list");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    // Check the size before reading: a mistaken publish must not become an out-of-memory.
    if metadata.len() > MAX_BYTES as u64 {
        tracing::error!(
            path = %path.display(),
            len = metadata.len(),
            "removal list is larger than 1 MiB; refusing to serve it"
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let bytes = match fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::error!(%err, path = %path.display(), "cannot read the removal list");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    match parse(&bytes) {
        // Serve the file's own bytes: the signature was made over them.
        Ok(_) => catalog::cached("application/json", REMOVALS_CACHE, bytes),
        Err(reason) => {
            tracing::error!(
                reason = %reason,
                path = %path.display(),
                "removal list is invalid; refusing to serve it"
            );
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
