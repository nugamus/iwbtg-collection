//! `/removals.json` through the real router, in-process (#808; docs/server.md, Removal list).

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt;

use crate::support::{self, TempDir};

/// 128 hex characters: the shape of a 64-byte Ed25519 signature. The server checks the shape and
/// leaves verification to the client, which holds the public key.
const SIGNATURE: &str = "0000000000000000000000000000000000000000000000000000000000000000\
                         0000000000000000000000000000000000000000000000000000000000000000";

fn entry(package_id: &str, reason: &str) -> String {
    format!(r#"{{"package_id": "{package_id}", "reason": "{reason}", "at": "2026-11-20T00:00:00Z"}}"#)
}

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

/// Drives the router the binary serves, with (or without) a catalog folder.
async fn get(catalog_dir: Option<PathBuf>, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = iwbtg_server::app(support::memory_pool().await, catalog_dir)
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    (status, headers, body)
}

fn header<'a>(headers: &'a HeaderMap, name: &header::HeaderName) -> &'a str {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .to_str()
        .expect("header is ASCII")
}

/// A catalog folder holding `removals.json` with exactly `text`.
fn publish(tag: &str, text: &str) -> TempDir {
    let dir = TempDir::new(tag);
    std::fs::write(dir.path().join("removals.json"), text).expect("write removals");
    dir
}

#[tokio::test]
async fn a_signed_list_is_served_verbatim_with_a_short_cache() {
    let text = list(&format!(
        "{},{}",
        entry("iwbtg.a", "takedown"),
        entry("iwbtg.b", "moderation")
    ));
    let dir = publish("removals-ok", &text);
    let (status, headers, body) = get(Some(dir.path().to_path_buf()), "/removals.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, &header::CONTENT_TYPE), "application/json");
    // Removals must reach clients quickly, like the catalog.
    assert_eq!(header(&headers, &header::CACHE_CONTROL), "public, max-age=60");
    // The exact published bytes: a signature over them stays valid.
    assert_eq!(body, text.as_bytes());
}

#[tokio::test]
async fn no_catalog_folder_means_no_removals() {
    let (status, _, _) = get(None, "/removals.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_catalog_folder_without_a_list_means_no_removals() {
    let dir = TempDir::new("removals-absent");
    let (status, _, _) = get(Some(dir.path().to_path_buf()), "/removals.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_broken_publish_is_unavailable_not_empty() {
    // 404 would tell a client "nothing was taken down", leaving the package installed.
    let cases = [
        ("not-json".to_string(), "not a list at all".to_string()),
        (
            "wrong-version".to_string(),
            list("").replace("\"version\": 1", "\"version\": 2"),
        ),
        (
            "unknown-field".to_string(),
            list("").replace("\"version\": 1", "\"version\": 1, \"extra\": true"),
        ),
        (
            "unsorted-ids".to_string(),
            list(&format!(
                "{},{}",
                entry("iwbtg.b", "takedown"),
                entry("iwbtg.a", "takedown")
            )),
        ),
        ("bad-signature".to_string(), list("").replace(SIGNATURE, "abc")),
    ];
    for (tag, text) in cases {
        let dir = publish(&format!("removals-{tag}"), &text);
        let (status, _, _) = get(Some(dir.path().to_path_buf()), "/removals.json").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "for {tag}");
    }
}

#[tokio::test]
async fn a_list_over_the_byte_cap_is_unavailable() {
    let mut text = list("");
    text.push_str(&" ".repeat(1 << 20));
    let dir = publish("removals-huge", &text);
    let (status, _, _) = get(Some(dir.path().to_path_buf()), "/removals.json").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
