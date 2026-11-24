//! The leaderboard server's HTTP surface (docs/server.md).
//!
//! The binary (`src/main.rs`) opens the database ([`db::connect`]), parses the bind address and
//! serves [`app`]; the tests drive the same router in-process (`tower::ServiceExt::oneshot`), so
//! they never need a port.

pub mod catalog;
pub mod db;

use std::path::PathBuf;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use sqlx::SqlitePool;

/// The router state: the database pool and the folder `/catalog.json` and `/files/*` are served
/// from (docs/server.md, Game downloads).
///
/// The catalog folder is optional: unset, those two routes answer 404 and the rest of the server
/// works, so nothing depends on the deploy having published a catalog yet.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub catalog_dir: Option<PathBuf>,
}

/// The whole HTTP surface. Routes added later (accounts, submissions) go here, so the tests see
/// exactly what the binary serves.
///
/// The state is the router state: handlers take `State<AppState>` (or an extractor built on it).
/// `/healthz` takes none, so liveness works even when the database is the thing that is down.
pub fn app(pool: SqlitePool, catalog_dir: Option<PathBuf>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/catalog.json", get(catalog::catalog))
        .route("/files/{name}", get(catalog::file))
        .with_state(AppState { pool, catalog_dir })
}

/// Liveness for healthchecks.io (docs/server.md, Logs and monitoring): unauthenticated, no IO.
async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
