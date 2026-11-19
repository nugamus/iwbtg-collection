//! The leaderboard server's HTTP surface (docs/server.md).
//!
//! The binary (`src/main.rs`) opens the database ([`db::connect`]), parses the bind address and
//! serves [`app`]; the tests drive the same router in-process (`tower::ServiceExt::oneshot`), so
//! they never need a port.

pub mod db;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use sqlx::SqlitePool;

/// The whole HTTP surface. Routes added later (accounts, submissions, downloads) go here, so the
/// tests see exactly what the binary serves.
///
/// The pool is the router state: handlers take `State<SqlitePool>` (or an extractor built on it).
/// `/healthz` takes none, so liveness works even when the database is the thing that is down.
pub fn app(pool: SqlitePool) -> Router {
    Router::new().route("/healthz", get(healthz)).with_state(pool)
}

/// Liveness for healthchecks.io (docs/server.md, Logs and monitoring): unauthenticated, no IO.
async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
