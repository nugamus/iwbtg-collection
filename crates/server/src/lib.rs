//! The leaderboard server's HTTP surface (docs/server.md).
//!
//! The binary (`src/main.rs`) opens the database ([`db::connect`]), parses the bind address and
//! serves [`app`]; the tests drive the same router in-process (`tower::ServiceExt::oneshot`), so
//! they never need a port.

pub mod auth;
pub mod catalog;
pub mod db;
pub mod health;
pub mod mail;
pub mod rate_limit;
pub mod removal;
pub mod submissions;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{get, post};
use sqlx::SqlitePool;

/// The router state: the database pool, the folder `/catalog.json` and `/files/*` are served
/// from (docs/server.md, Game downloads), the per-client request limiter (#161), and the reset
/// mailer (#160).
///
/// The catalog folder is optional: unset, those two routes answer 404 and the rest of the server
/// works, so nothing depends on the deploy having published a catalog yet.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub catalog_dir: Option<PathBuf>,
    pub limiter: Arc<rate_limit::RateLimiter>,
    pub mailer: Arc<mail::Mailer>,
}

/// The whole HTTP surface. Routes added later (accounts, submissions) go here, so the tests see
/// exactly what the binary serves.
///
/// The state is the router state: handlers take `State<AppState>` (or an extractor built on it).
/// `/healthz` takes none, so liveness works even when the database is the thing that is down.
pub fn app(pool: SqlitePool, catalog_dir: Option<PathBuf>) -> Router {
    app_with_mailer(pool, catalog_dir, mail::Mailer::disabled())
}

/// [`app`] with an explicit mailer: the binary builds one from `SMTP_URL`/`MAIL_FROM`, the reset
/// tests point one at a local mock SMTP listener (#160).
pub fn app_with_mailer(pool: SqlitePool, catalog_dir: Option<PathBuf>, mailer: mail::Mailer) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/catalog.json", get(catalog::catalog))
        .route("/files/{name}", get(catalog::file))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/reset/request", post(auth::reset_request))
        .route("/auth/reset/confirm", post(auth::reset_confirm))
        .route("/register", get(auth::register_page).post(auth::register_form))
        .route("/login", get(auth::login_page).post(auth::login_form))
        .route("/removals.json", get(removal::removals))
        // The per-route cap applies before the handler: an over-limit body never reaches the
        // decoder (#162; docs/server.md, Replays).
        .route(
            "/submissions",
            post(submissions::submit).layer(DefaultBodyLimit::max(submissions::MAX_REPLAY_BYTES)),
        )
        .with_state(AppState {
            pool,
            catalog_dir,
            limiter: Arc::new(rate_limit::RateLimiter::new()),
            mailer: Arc::new(mailer),
        })
}

/// Liveness for healthchecks.io (docs/server.md, Logs and monitoring): unauthenticated, no IO.
async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
