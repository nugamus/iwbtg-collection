//! The SQLite file, its schema and the startup migrations (#153; docs/server.md, Schema).

use iwbtg_server::db;

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

/// The connect options are per connection, not just per pool: `journal_mode` is a file property,
/// but `foreign_keys`, `busy_timeout` and `synchronous` are set anew on each connection
/// (docs/server.md, Schema).
#[tokio::test]
async fn every_connection_has_the_pragmas() {
    let dir = TempDir::new("pragmas");
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("connect and migrate");

    // Hold the pool's whole maximum at once, so each is a distinct connection.
    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(pool.acquire().await.expect("acquire a connection"));
    }

    for (index, mut connection) in connections.into_iter().enumerate() {
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut *connection)
            .await
            .expect("read journal_mode");
        assert_eq!(journal_mode.to_lowercase(), "wal", "connection {index}");

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut *connection)
            .await
            .expect("read foreign_keys");
        assert_eq!(
            foreign_keys, 1,
            "connection {index} does not enforce foreign keys"
        );

        let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&mut *connection)
            .await
            .expect("read busy_timeout");
        assert_eq!(busy_timeout, 5_000, "connection {index}");

        // NORMAL = 1 in SQLite's integer enum.
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut *connection)
            .await
            .expect("read synchronous");
        assert_eq!(synchronous, 1, "connection {index} is not synchronous = normal");
    }
}

/// Rewrites the migration ledger so it no longer matches the compiled-in migration, the way an
/// edited migration file looks to a database that already ran the old one.
async fn drift_migration_checksum(path: &std::path::Path) {
    let pool = db::connect(path).await.expect("connect and migrate");
    sqlx::query("UPDATE _sqlx_migrations SET checksum = x'00'")
        .execute(&pool)
        .await
        .expect("tamper with the recorded checksum");
    pool.close().await;
}

#[tokio::test]
async fn a_failed_migration_stops_startup() {
    let dir = TempDir::new("drift");
    let path = dir.path().join("iwbtg.db");
    drift_migration_checksum(&path).await;

    let err = db::connect(&path)
        .await
        .expect_err("a migration that no longer matches must stop startup");
    assert!(
        matches!(err, db::DbError::Migrate { .. }),
        "wrong error for a failed migration: {err:?}"
    );
    assert!(
        err.to_string().contains("database migrations failed"),
        "unclear startup error: {err}"
    );
}

/// The failure path end to end: the binary exits before the listener binds, so a half-migrated
/// file is never served (docs/server.md, Schema).
#[tokio::test]
async fn a_failed_migration_exits_before_listening() {
    let dir = TempDir::new("no-serve");
    let path = dir.path().join("iwbtg.db");
    drift_migration_checksum(&path).await;

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_iwbtg-server"))
        .arg("--db")
        .arg(&path)
        .arg("--bind")
        .arg("127.0.0.1:0")
        .output()
        .expect("run the server binary");

    assert!(
        !output.status.success(),
        "the server exited successfully on a failed migration"
    );
    let logs = String::from_utf8_lossy(&output.stdout);
    assert!(
        logs.contains("database migrations failed"),
        "no migration error in the log: {logs}"
    );
    assert!(
        !logs.contains("listening"),
        "the listener bound before the migration failed: {logs}"
    );
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
