//! `GET /healthz` through the real router, in-process (#152).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok() {
    let response = iwbtg_server::app()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn unknown_path_is_not_found() {
    let response = iwbtg_server::app()
        .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
