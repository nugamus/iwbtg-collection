//! The leaderboard server's HTTP surface (docs/server.md).
//!
//! The binary (`src/main.rs`) only parses the bind address and serves [`app`]; the tests drive the
//! same router in-process (`tower::ServiceExt::oneshot`), so they never need a port.

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

/// The whole HTTP surface. Routes added later (accounts, submissions, downloads) go here, so the
/// tests see exactly what the binary serves.
pub fn app() -> Router {
    Router::new().route("/healthz", get(healthz))
}

/// Liveness for healthchecks.io (docs/server.md, Logs and monitoring): unauthenticated, no IO.
async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
