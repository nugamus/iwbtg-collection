//! Public replay downloads (docs/server.md, Replay downloads; #200).
//!
//! A board row links to `GET /replays/{run_id}`. Only a publicly visible run is served: the
//! verifier marked it `verified` with a tick count, and moderation has not quarantined it.
//! Everything else is one uniform `404`, so the route cannot be used to enumerate pending or
//! hidden runs. The body is the exact stored bytes, so a client can re-verify the run offline
//! with `iwbtg replay verify`.

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// The bytes of a run never change, but its visibility can (a quarantine must take effect), so
/// this is the catalog's short window rather than the hashed files' year.
pub const REPLAY_CACHE: &str = "public, max-age=60";

/// `GET /replays/{run_id}` (docs/server.md, Replay downloads; #200).
pub async fn download(State(state): State<AppState>, UrlPath(run_id): UrlPath<i64>) -> Response {
    let replay: Result<Option<Vec<u8>>, sqlx::Error> = sqlx::query_scalar(
        "SELECT r.replay FROM runs r
         LEFT JOIN run_reviews rv ON rv.run_id = r.id
         WHERE r.id = ?1
           AND r.status = 'verified'
           AND r.ticks IS NOT NULL
           AND COALESCE(rv.state, 'pending') != 'quarantined'",
    )
    .bind(run_id)
    .fetch_optional(&state.pool)
    .await;

    match replay {
        Ok(Some(bytes)) => {
            let mut response = bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(REPLAY_CACHE));
            headers.insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{run_id}.iwbr\""))
                    .expect("an ASCII header"),
            );
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            // A database failure is ours, not a missing run: do not answer 404 and deny a
            // visible replay because of it.
            tracing::error!(%error, run_id, "cannot read the stored replay");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
