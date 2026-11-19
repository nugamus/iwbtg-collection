//! The SQLite file, its schema and the startup migrations (#153; docs/server.md, Schema).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use iwbtg_server::db;
use tower::ServiceExt;

use crate::support::TempDir;

/// Every table docs/server.md names, plus the migration ledger sqlx keeps.
const TABLES: [&str; 6] = [
    "accounts",
    "identities",
    "password_credentials",
    "sessions",
    "runs",
    "verify_jobs",
];

async fn table_names(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("read sqlite_master")
}

#[tokio::test]
async fn connect_creates_the_folder_and_runs_the_migrations() {
    let dir = TempDir::new("migrate");
    // The folder does not exist yet: `connect` creates it.
    let path = dir.path().join("nested").join("iwbtg.db");
    let pool = db::connect(&path).await.expect("connect and migrate");

    assert!(path.exists(), "database file {} was not created", path.display());

    let tables = table_names(&pool).await;
    for expected in TABLES {
        assert!(
            tables.iter().any(|name| name == expected),
            "missing table {expected}: {tables:?}"
        );
    }

    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await
        .expect("count migrations");
    assert!(applied >= 1, "no migration was recorded");
}

#[tokio::test]
async fn connecting_twice_does_not_rerun_migrations() {
    let dir = TempDir::new("idempotent");
    let path = dir.path().join("iwbtg.db");

    let first = db::connect(&path).await.expect("first connect");
    let applied_after_first: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&first)
        .await
        .expect("count migrations");
    first.close().await;

    let second = db::connect(&path).await.expect("second connect");
    let applied_after_second: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&second)
        .await
        .expect("count migrations");
    assert_eq!(applied_after_first, applied_after_second);
}

#[tokio::test]
async fn journal_mode_is_wal() {
    let dir = TempDir::new("wal");
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("connect and migrate");

    // WAL is a property of the file, so any connection reports it.
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .expect("read journal_mode");
    assert_eq!(mode.to_lowercase(), "wal");
}

#[tokio::test]
async fn pragmas_are_set_on_the_pool() {
    let dir = TempDir::new("pragmas");
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("connect and migrate");

    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("read foreign_keys");
    assert_eq!(foreign_keys, 1);

    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .expect("read busy_timeout");
    assert_eq!(busy_timeout, 5_000);
}

#[tokio::test]
async fn an_uncreatable_folder_is_a_clear_error() {
    let dir = TempDir::new("broken");
    // A file where the database's folder should go: create_dir_all cannot succeed.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a folder").expect("write blocker file");

    let err = db::connect(&blocker.join("iwbtg.db"))
        .await
        .expect_err("a file cannot become a folder");
    assert!(
        err.to_string().contains("cannot create the database folder"),
        "unclear startup error: {err}"
    );
}

#[tokio::test]
async fn healthz_answers_with_the_pool_state() {
    let dir = TempDir::new("healthz");
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("connect and migrate");

    let response = iwbtg_server::app(pool)
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ok");
}
