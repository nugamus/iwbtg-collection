//! The SQLite database (docs/server.md, Database): one pooled file, WAL mode, migrations run at
//! startup.
//!
//! [`connect`] is the only place that opens a database, so the binary and the tests get the same
//! pragmas and the same migrations. Routes reach the pool through the router state ([`crate::app`]);
//! `/healthz` takes no state and never touches the database.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

/// The migrations compiled into the binary: `migrations/` at the workspace root (#153).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// The default database file, relative to the working directory (env `IWBTG_DB` overrides it).
pub const DEFAULT_PATH: &str = "data/iwbtg.db";

/// Why the database could not be opened or migrated. Startup prints this and exits 1.
#[derive(Debug)]
pub enum DbError {
    /// The parent folder of the database file could not be created.
    CreateDir { path: PathBuf, source: std::io::Error },
    /// SQLite refused to open the file or run a startup query.
    Open { path: PathBuf, source: sqlx::Error },
    /// A migration failed; the server must not serve on a half-migrated file.
    Migrate {
        path: PathBuf,
        source: sqlx::migrate::MigrateError,
    },
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateDir { path, source } => {
                write!(
                    f,
                    "cannot create the database folder {}: {source}",
                    path.display()
                )
            }
            Self::Open { path, source } => {
                write!(f, "cannot open the SQLite database {}: {source}", path.display())
            }
            Self::Migrate { path, source } => {
                write!(f, "database migrations failed on {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateDir { source, .. } => Some(source),
            Self::Open { source, .. } => Some(source),
            Self::Migrate { source, .. } => Some(source),
        }
    }
}

/// Opens (creating it if needed) the SQLite file at `path`, sets the pragmas every connection
/// needs, and runs the migrations.
///
/// WAL lets readers and the one writer share the file without blocking each other; `foreign_keys`
/// enforces the `REFERENCES` above; `busy_timeout` waits out another process's write instead of
/// failing immediately.
pub async fn connect(path: &Path) -> Result<SqlitePool, DbError> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| DbError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|source| DbError::Open {
            path: path.to_path_buf(),
            source,
        })?;

    MIGRATOR.run(&pool).await.map_err(|source| DbError::Migrate {
        path: path.to_path_buf(),
        source,
    })?;

    // Log the count, not just "ok": it is how the operator sees a new migration land on deploy.
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await
        .map_err(|source| DbError::Open {
            path: path.to_path_buf(),
            source,
        })?;
    tracing::info!(
        db = %path.display(),
        migrations = applied,
        "database ready (migrations applied)"
    );

    Ok(pool)
}
