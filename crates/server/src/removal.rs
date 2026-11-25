//! The signed removal list (docs/server.md, Removal list; #808).
//!
//! Takedowns (#68) and moderation (#800) write `<catalog-dir>/removals.json`; the server serves
//! those exact bytes at `GET /removals.json`. The file is already signed and the server holds no
//! key: the signing key stays off the public host, and a client trusts the list only after
//! checking `signature` against the public key it ships. The server's job is to catch a broken
//! publish (503) instead of letting it look like "no removals" (404).
//!
//! The signature covers canonical text rather than the JSON bytes, so formatting is free:
//!
//! ```text
//! iwbtg-removals-v1\n
//! <key_id>\n
//! <generated_at>\n
//! <package_id>\t<reason>\t<at>\n   one line per entry, sorted by package_id
//! ```
//!
//! The checks in [`validate`] are what make that text unambiguous: every field is a restricted
//! ASCII alphabet with no tab, newline or space, ids are unique and sorted, and unknown JSON
//! fields are rejected, so nothing unsigned can ride along. The response body is the file's own
//! bytes, never a re-serialization, so the signature a publisher computed stays valid.

use std::io::ErrorKind;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::fs;

use crate::AppState;
use crate::catalog;

/// The signed list inside the configured catalog folder (docs/server.md, Removal list).
const REMOVALS_FILE: &str = "removals.json";

/// Pinned prefixes the canonical signed text starts with; a format change is a new prefix.
const SIGNED_PREFIX: &str = "iwbtg-removals-v1\n";

/// Removals must reach clients quickly, so the list is cached like the catalog: a minute.
pub const REMOVALS_CACHE: &str = "public, max-age=60";

/// What one request may pull into memory.
const MAX_BYTES: usize = 1 << 20;

/// A list past this is a publish mistake, not a real takedown set.
const MAX_ENTRIES: usize = 4_096;

/// The published list, already signed (docs/server.md, Removal list).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedList {
    /// Format version; only `1` is understood.
    pub version: u32,
    /// Which client-embedded public key verifies `signature`.
    pub key_id: String,
    /// When the list was produced, UTC ISO-8601.
    pub generated_at: String,
    /// Removed packages, sorted by `package_id`.
    pub removals: Vec<Removal>,
    /// Ed25519 signature over [`signed_bytes`], 128 lowercase hex characters.
    pub signature: String,
}

/// One removed package.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Removal {
    /// The package to delete and hide.
    pub package_id: String,
    /// Why: `takedown`, `moderation`, or another lower-case code a client may ignore.
    pub reason: String,
    /// When this entry was added, UTC ISO-8601.
    pub at: String,
}

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
                reason,
                path = %path.display(),
                "removal list is invalid; refusing to serve it"
            );
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// Parses and validates a published list. The reason is logged and never includes file content.
pub fn parse(bytes: &[u8]) -> Result<SignedList, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("larger than 1 MiB");
    }
    let list: SignedList = serde_json::from_slice(bytes).map_err(|_| "not a valid removal list")?;
    validate(&list)?;
    Ok(list)
}

/// Checks everything [`signed_bytes`] relies on.
pub fn validate(list: &SignedList) -> Result<(), &'static str> {
    if list.version != 1 {
        return Err("unsupported version");
    }
    if !is_bounded(&list.key_id, 32, is_id_byte) {
        return Err("key_id is not a short id");
    }
    if !is_bounded(&list.generated_at, 64, is_timestamp_byte) {
        return Err("generated_at is not a timestamp");
    }
    if list.removals.len() > MAX_ENTRIES {
        return Err("more than 4096 entries");
    }
    for removal in &list.removals {
        if !is_bounded(&removal.package_id, 64, is_id_byte) {
            return Err("a package_id is not a short id");
        }
        if !is_bounded(&removal.reason, 32, is_reason_byte) {
            return Err("a reason is not a lower-case code");
        }
        if !is_bounded(&removal.at, 64, is_timestamp_byte) {
            return Err("an entry timestamp is not a timestamp");
        }
    }
    // Unique and sorted: the signed text has one canonical order, and a client applies a set.
    if !list
        .removals
        .windows(2)
        .all(|pair| pair[0].package_id < pair[1].package_id)
    {
        return Err("entries are not unique and sorted by package_id");
    }
    if list.signature.len() != 128
        || !list
            .signature
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("signature is not 128 lowercase hex characters");
    }
    Ok(())
}

/// The exact bytes `signature` covers. The client and the signing tool implement this same rule;
/// it is text, not JSON, so a publisher may reformat the file freely.
pub fn signed_bytes(list: &SignedList) -> Vec<u8> {
    let mut text = String::from(SIGNED_PREFIX);
    text.push_str(&list.key_id);
    text.push('\n');
    text.push_str(&list.generated_at);
    text.push('\n');
    for removal in &list.removals {
        text.push_str(&removal.package_id);
        text.push('\t');
        text.push_str(&removal.reason);
        text.push('\t');
        text.push_str(&removal.at);
        text.push('\n');
    }
    text.into_bytes()
}

/// A non-empty value of at most `max` bytes, every byte accepted by `allowed`.
fn is_bounded(value: &str, max: usize, allowed: impl Fn(u8) -> bool) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(allowed)
}

/// Package ids and key ids: letters, digits, `.`, `_` and `-`.
fn is_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// A reason code: lower-case letters, digits and `-`.
fn is_reason_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
}

/// A timestamp: printable ASCII without spaces, so no tab or newline can split the signed text.
fn is_timestamp_byte(byte: u8) -> bool {
    (0x21..=0x7e).contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::{SignedList, parse, signed_bytes};

    /// 128 hex characters: the shape of a 64-byte Ed25519 signature.
    const SIGNATURE: &str = "0000000000000000000000000000000000000000000000000000000000000000\
                             0000000000000000000000000000000000000000000000000000000000000000";

    fn list(removals: &str) -> String {
        format!(
            r#"{{
  "version": 1,
  "key_id": "2026-11",
  "generated_at": "2026-11-24T12:00:00Z",
  "removals": [{removals}],
  "signature": "{SIGNATURE}"
}}"#
        )
    }

    fn entry(id: &str, reason: &str, at: &str) -> String {
        format!(r#"{{"package_id": "{id}", "reason": "{reason}", "at": "{at}"}}"#)
    }

    #[test]
    fn a_well_formed_list_parses() {
        let text = list(&format!(
            "{},{}",
            entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
            entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z")
        ));
        let parsed = parse(text.as_bytes()).expect("valid list");
        assert_eq!(parsed.removals.len(), 2);
        assert_eq!(parsed.removals[1].package_id, "iwbtg.b");
    }

    #[test]
    fn an_empty_list_is_valid() {
        parse(list("").as_bytes()).expect("a list may lift every removal");
    }

    #[test]
    fn the_signed_text_is_the_documented_canonical_form() {
        let parsed: SignedList = parse(
            list(&format!(
                "{},{}",
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z")
            ))
            .as_bytes(),
        )
        .expect("valid list");
        assert_eq!(
            String::from_utf8(signed_bytes(&parsed)).expect("utf-8"),
            "iwbtg-removals-v1\n\
             2026-11\n\
             2026-11-24T12:00:00Z\n\
             iwbtg.a\ttakedown\t2026-11-20T00:00:00Z\n\
             iwbtg.b\tmoderation\t2026-11-21T00:00:00Z\n"
        );
    }

    #[test]
    fn rejects_a_version_it_does_not_understand() {
        let text = list("").replace("\"version\": 1", "\"version\": 2");
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_an_unknown_field() {
        let text = list("").replace("\"version\": 1", "\"version\": 1, \"extra\": true");
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_an_entry_whose_id_could_split_the_signed_text() {
        let text = list(&entry("iwbtg.a\\tb", "takedown", "2026-11-20T00:00:00Z"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_reason_that_is_not_a_code() {
        let text = list(&entry("iwbtg.a", "Takedown", "2026-11-20T00:00:00Z"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_timestamp_with_a_space() {
        let text = list(&entry("iwbtg.a", "takedown", "2026-11-20 00:00:00"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_duplicate_or_unsorted_ids() {
        for entries in [
            format!(
                "{},{}",
                entry("iwbtg.b", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")
            ),
            format!(
                "{},{}",
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.a", "moderation", "2026-11-21T00:00:00Z")
            ),
        ] {
            assert!(parse(list(&entries).as_bytes()).is_err(), "accepted {entries}");
        }
    }

    #[test]
    fn rejects_a_signature_that_is_not_64_bytes_of_hex() {
        for signature in ["", "abc", &"A".repeat(128), &"g".repeat(128)] {
            let text = list("").replace(SIGNATURE, signature);
            assert!(parse(text.as_bytes()).is_err(), "accepted {signature:?}");
        }
    }

    #[test]
    fn rejects_a_list_over_the_byte_cap() {
        let mut text = list("");
        text.push_str(&" ".repeat(1 << 20));
        assert!(parse(text.as_bytes()).is_err());
    }
}
