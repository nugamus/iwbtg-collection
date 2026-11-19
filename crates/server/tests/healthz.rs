//! `GET /healthz` through the real router, in-process (#152).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::support;

#[tokio::test]
async fn healthz_returns_ok() {
    let response = iwbtg_server::app(support::memory_pool().await)
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ok");
}

/// Liveness must not depend on the database: a closed pool (every acquire fails) still answers.
#[tokio::test]
async fn healthz_works_without_the_database() {
    let pool = support::memory_pool().await;
    pool.close().await;

    let response = iwbtg_server::app(pool)
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_path_is_not_found() {
    let response = iwbtg_server::app(support::memory_pool().await)
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
