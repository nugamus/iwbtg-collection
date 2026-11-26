//! `GET /replays/{run_id}`: public downloads, only for a publicly visible run (#200;
//! docs/server.md, Replay downloads).

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use sqlx::SqlitePool;
use tower::ServiceExt;

use iwbtg_server::db;

use crate::support::TempDir;

const REPLAY: &[u8] = b"the exact replay bytes";

async fn fixture(tag: &str) -> (TempDir, SqlitePool, Router) {
    let dir = TempDir::new(tag);
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("open and migrate the database");
    sqlx::query("INSERT INTO accounts (id, username, username_canonical) VALUES (1, 'Player', 'player')")
        .execute(&pool)
        .await
        .expect("insert the account");
    let router = iwbtg_server::app(pool.clone(), None);
    (dir, pool, router)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).expect("a request"))
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

async fn insert_run(pool: &SqlitePool, status: &str, ticks: Option<i64>, tag: u8) -> i64 {
    let gameplay = [7u8; 32];
    let replay_sha256 = [tag; 32];
    sqlx::query_scalar(
        "INSERT INTO runs
             (account_id, package_id, map_id, mods, gameplay_hash, replay, replay_sha256, ticks, status)
         VALUES (1, 'test.pkg', 'main', 0, ?1, ?2, ?3, ?4, ?5)
         RETURNING id",
    )
    .bind(gameplay.as_slice())
    .bind(REPLAY)
    .bind(replay_sha256.as_slice())
    .bind(ticks)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert a run")
}

async fn review(pool: &SqlitePool, run_id: i64, state: &str) {
    sqlx::query("INSERT INTO run_reviews (run_id, state) VALUES (?1, ?2)")
        .bind(run_id)
        .bind(state)
        .execute(pool)
        .await
        .expect("insert a review");
}

#[tokio::test]
async fn a_verified_replay_is_served_unchanged() {
    let (_dir, pool, router) = fixture("replays-verified").await;
    let run_id = insert_run(&pool, "verified", Some(120), 1).await;

    let (status, headers, body) = get(&router, &format!("/replays/{run_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, REPLAY, "the stored bytes are served exactly");
    assert_eq!(
        header(&headers, &header::CONTENT_TYPE),
        "application/octet-stream"
    );
    // The bytes never change, but visibility does: a short window lets a quarantine take effect.
    assert_eq!(header(&headers, &header::CACHE_CONTROL), "public, max-age=60");
    assert_eq!(
        header(&headers, &header::CONTENT_DISPOSITION),
        format!("attachment; filename=\"{run_id}.iwbr\"")
    );
}

#[tokio::test]
async fn a_pending_or_rejected_run_is_not_downloadable() {
    let (_dir, pool, router) = fixture("replays-hidden").await;
    let pending = insert_run(&pool, "pending", None, 2).await;
    let rejected = insert_run(&pool, "rejected", Some(130), 3).await;

    for run_id in [pending, rejected] {
        let (status, _, body) = get(&router, &format!("/replays/{run_id}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "run {run_id}");
        assert!(body.is_empty());
    }
}

#[tokio::test]
async fn a_quarantined_run_is_not_downloadable() {
    let (_dir, pool, router) = fixture("replays-quarantined").await;
    let run_id = insert_run(&pool, "verified", Some(120), 4).await;
    review(&pool, run_id, "quarantined").await;

    let (status, _, body) = get(&router, &format!("/replays/{run_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.is_empty());
}

#[tokio::test]
async fn a_flagged_but_visible_run_is_downloadable() {
    let (_dir, pool, router) = fixture("replays-visible").await;
    let requested = insert_run(&pool, "verified", Some(120), 5).await;
    review(&pool, requested, "video_requested").await;
    let cleared = insert_run(&pool, "verified", Some(130), 6).await;
    review(&pool, cleared, "cleared").await;

    for run_id in [requested, cleared] {
        let (status, _, body) = get(&router, &format!("/replays/{run_id}")).await;
        assert_eq!(status, StatusCode::OK, "run {run_id}");
        assert_eq!(body, REPLAY);
    }
}

#[tokio::test]
async fn an_unknown_run_is_404() {
    let (_dir, _pool, router) = fixture("replays-unknown").await;
    let (status, _, body) = get(&router, "/replays/9999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.is_empty());
}
