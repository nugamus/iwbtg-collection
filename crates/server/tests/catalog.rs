//! `/catalog.json` and `/files/<sha256>` through the real router, in-process (#807;
//! docs/server.md, Game downloads).

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt;

use crate::support::{self, TempDir};

/// SHA-256 of the empty input: any 64 lowercase hex characters would do.
const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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

/// A catalog folder with a catalog and one published file.
fn publish(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    std::fs::create_dir_all(dir.path().join("files")).expect("create files folder");
    std::fs::write(dir.path().join("catalog.json"), br#"{"files":[]}"#).expect("write catalog");
    std::fs::write(files_dir(dir.path()).join(HASH), b"package bytes").expect("write file");
    std::fs::write(files_dir(dir.path()).join(format!("{HASH}.iwbz")), b"zipped").expect("write file");
    dir
}

fn files_dir(dir: &Path) -> PathBuf {
    dir.join("files")
}

#[tokio::test]
async fn catalog_is_served_with_a_short_cache() {
    let dir = publish("catalog-json");
    let (status, headers, body) = get(Some(dir.path().to_path_buf()), "/catalog.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, &header::CONTENT_TYPE), "application/json");
    // The catalog changes as games are published, so it may not be cached for long.
    assert_eq!(header(&headers, &header::CACHE_CONTROL), "public, max-age=60");
    assert_eq!(body, br#"{"files":[]}"#);
}

#[tokio::test]
async fn a_hashed_file_is_served_with_an_immutable_cache() {
    let dir = publish("file-plain");
    let (status, headers, body) = get(Some(dir.path().to_path_buf()), &format!("/files/{HASH}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, &header::CONTENT_TYPE),
        "application/octet-stream"
    );
    // The name is the hash: these bytes can never change under this URL.
    assert_eq!(
        header(&headers, &header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(body, b"package bytes");
}

#[tokio::test]
async fn a_file_with_an_extension_keeps_its_type() {
    let dir = publish("file-ext");
    let (status, headers, body) = get(Some(dir.path().to_path_buf()), &format!("/files/{HASH}.iwbz")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, &header::CONTENT_TYPE), "application/zip");
    assert_eq!(body, b"zipped");
}

#[tokio::test]
async fn the_catalog_is_404_without_a_configured_folder() {
    let dir = publish("unconfigured");
    // The folder exists and holds the catalog, but the router was not told to serve it.
    let (status, _, _) = get(None, "/catalog.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = get(None, &format!("/files/{HASH}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    drop(dir);
}

#[tokio::test]
async fn a_missing_catalog_or_file_is_404() {
    let dir = TempDir::new("empty");
    std::fs::create_dir_all(files_dir(dir.path())).expect("create files folder");

    let (status, _, _) = get(Some(dir.path().to_path_buf()), "/catalog.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A well-formed hash that was never published.
    let (status, _, _) = get(Some(dir.path().to_path_buf()), &format!("/files/{HASH}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_name_that_is_not_a_hash_is_404() {
    let dir = publish("not-a-hash");
    let uppercase = format!("/files/{}", HASH.to_uppercase());
    for uri in [
        "/files/notes.txt",
        "/files/catalog.json",
        "/files/../catalog.json",
        "/files/..%2Fcatalog.json",
        "/files/%2E%2E%2Fcatalog.json",
        "/files/..%5Ccatalog.json",
        &uppercase,
    ] {
        let (status, _, body) = get(Some(dir.path().to_path_buf()), uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} was served");
        assert!(
            !body.windows(b"files".len()).any(|window| window == b"files"),
            "{uri} leaked content"
        );
    }
}

/// The traversal attempt that matters: a real file one folder up must not be readable.
#[tokio::test]
async fn traversal_cannot_reach_a_file_outside_the_files_folder() {
    let dir = TempDir::new("traversal");
    std::fs::create_dir_all(files_dir(dir.path())).expect("create files folder");
    let secret = b"do not serve me";
    std::fs::write(dir.path().join("secret.txt"), secret).expect("write the secret");

    for uri in [
        "/files/../secret.txt",
        "/files/..%2Fsecret.txt",
        "/files/%2e%2e%2fsecret.txt",
        "/files/..%5Csecret.txt",
    ] {
        let (status, _, body) = get(Some(dir.path().to_path_buf()), uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} was served");
        assert_ne!(&body[..], secret, "{uri} leaked the file");
    }
}
