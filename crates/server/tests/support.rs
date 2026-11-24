//! Shared helpers for the integration tests (listed in `tests/it.rs`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

/// A unique folder under the OS temp dir, removed when dropped. SQLite needs a real file for the
/// WAL-mode tests (`sqlite::memory:` ignores WAL).
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("iwbtg-server-test-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort: on Windows a still-open file can block the delete.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A throwaway pool for tests that need the router's state but not a schema.
pub async fn memory_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory SQLite")
}
