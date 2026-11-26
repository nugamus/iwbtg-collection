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
    get_with(catalog_dir, uri, &[]).await
}

/// The same, with request headers (`Range`).
async fn get_with(
    catalog_dir: Option<PathBuf>,
    uri: &str,
    request_headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut request = Request::get(uri);
    for (name, value) in request_headers {
        request = request.header(*name, *value);
    }
    let response = iwbtg_server::app(support::memory_pool().await, catalog_dir)
        .oneshot(request.body(Body::empty()).unwrap())
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
    // Every file response says a dropped download may resume.
    assert_eq!(header(&headers, &header::ACCEPT_RANGES), "bytes");
    assert!(headers.get(&header::CONTENT_RANGE).is_none());
    // The body streams, so the size still has to be declared up front.
    assert_eq!(header(&headers, &header::CONTENT_LENGTH), "13");
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

/// A satisfiable single range: 206, the slice, and the bounds in `Content-Range`. The long cache
/// stays, because the bytes at these offsets can never change either.
#[tokio::test]
async fn a_range_request_returns_206_and_the_slice() {
    let dir = publish("range-206");
    let uri = format!("/files/{HASH}");
    for (range, expected, content_range) in [
        ("bytes=0-6", &b"package"[..], "bytes 0-6/13"),
        ("bytes=-5", &b"bytes"[..], "bytes 8-12/13"),
        ("bytes=8-", &b"bytes"[..], "bytes 8-12/13"),
        ("bytes=8-999", &b"bytes"[..], "bytes 8-12/13"),
        ("bytes=0-0", &b"p"[..], "bytes 0-0/13"),
    ] {
        let (status, headers, body) =
            get_with(Some(dir.path().to_path_buf()), &uri, &[("range", range)]).await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT, "{range}");
        assert_eq!(body, expected, "{range}");
        assert_eq!(
            header(&headers, &header::CONTENT_LENGTH),
            expected.len().to_string(),
            "{range}"
        );
        assert_eq!(header(&headers, &header::CONTENT_RANGE), content_range, "{range}");
        assert_eq!(header(&headers, &header::ACCEPT_RANGES), "bytes", "{range}");
        assert_eq!(
            header(&headers, &header::CACHE_CONTROL),
            "public, max-age=31536000, immutable",
            "{range}"
        );
    }
}

/// A syntactically valid range that selects no byte: 416 and the real length in `Content-Range`,
/// so the client can restart from the right place.
#[tokio::test]
async fn an_unsatisfiable_range_is_416() {
    let dir = publish("range-416");
    let uri = format!("/files/{HASH}");
    for range in ["bytes=13-", "bytes=13-99", "bytes=-0"] {
        let (status, headers, body) =
            get_with(Some(dir.path().to_path_buf()), &uri, &[("range", range)]).await;
        assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE, "{range}");
        assert_eq!(header(&headers, &header::CONTENT_RANGE), "bytes */13", "{range}");
        assert_eq!(header(&headers, &header::ACCEPT_RANGES), "bytes", "{range}");
        assert!(body.is_empty(), "{range} carried a body");
    }
}

/// Invalid syntax, another unit and several ranges are all ignored: the full 200 is the answer,
/// never a half-understood 206.
#[tokio::test]
async fn an_invalid_or_multiple_range_is_a_full_200() {
    let dir = publish("range-invalid");
    let uri = format!("/files/{HASH}");
    for range in ["bytes=", "bytes=abc", "bytes=5-2", "bytes=0-1,3-4", "items=0-1"] {
        let (status, headers, body) =
            get_with(Some(dir.path().to_path_buf()), &uri, &[("range", range)]).await;
        assert_eq!(status, StatusCode::OK, "{range}");
        assert!(
            headers.get(&header::CONTENT_RANGE).is_none(),
            "{range} answered a range"
        );
        assert_eq!(header(&headers, &header::CONTENT_LENGTH), "13", "{range}");
        assert_eq!(body, b"package bytes", "{range}");
    }
}
