//! Importing packages (docs/architecture/client/import.md, #36).
//!
//! - An `.iwbz` the client opens (from the command line, which is also what an OS file
//!   association calls) is kept in the library, `library/<id>.iwbz` in the data folder, for game
//!   select (M6) to list.
//! - A package dropped on the window (an `.iwbz` file or a package folder) is checked, then the
//!   client starts again on it and this one exits. Until game select can switch games in place,
//!   that is the simplest way to open another game; a run in progress ends as on any exit (its
//!   replay is kept).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;

use crate::package;
use crate::storage::{self, Store};

/// Keeps the opened `.iwbz` and opens dropped packages.
pub struct ImportPlugin {
    /// What the client was started on: a folder or an `.iwbz` file.
    pub opened: PathBuf,
    /// The package id (`game.toml` `id`), the library file's name.
    pub id: String,
    /// False for a `--screenshot` check, which leaves no trace on the machine.
    pub keep: bool,
}

impl Plugin for ImportPlugin {
    fn build(&self, app: &mut App) {
        if self.keep
            && package::is_iwbz(&self.opened)
            && let Some(store) = Store::data()
        {
            let saved = std::fs::read(&self.opened)
                .map_err(|e| e.to_string())
                .and_then(|bytes| remember(&store, &self.id, &bytes));
            if let Err(e) = saved {
                storage::warn(&format!(
                    "could not add {} to the library: {e}",
                    self.opened.display()
                ));
            }
        }
        app.add_systems(Update, open_dropped);
    }
}

/// The library file of a package.
pub fn library_name(id: &str) -> String {
    format!("library/{id}.{}", iwbtg_format::iwbz::EXTENSION)
}

/// Stores `bytes` as the library copy of package `id`. Returns false when the same file was
/// already there (nothing written).
pub fn remember(store: &Store, id: &str, bytes: &[u8]) -> Result<bool, String> {
    let name = library_name(id);
    if store.read(&name)?.as_deref() == Some(bytes) {
        return Ok(false);
    }
    store.write(&name, bytes)?;
    Ok(true)
}

fn open_dropped(mut drops: MessageReader<FileDragAndDrop>, mut exit: MessageWriter<AppExit>) {
    for drop in drops.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = drop else {
            continue;
        };
        match restart_on(path_buf) {
            Ok(()) => {
                exit.write(AppExit::Success);
                return;
            }
            Err(e) => storage::warn(&e),
        }
    }
}

/// Checks that `path` is a package that opens, then starts a new client on it.
fn restart_on(path: &Path) -> Result<(), String> {
    if !(package::is_iwbz(path) || path.join("game.toml").is_file()) {
        return Err(format!(
            "{}: not a package (an .iwbz file or a folder with a game.toml)",
            path.display()
        ));
    }
    package::open(path, false).map_err(|e| format!("{}: {e}", path.display()))?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot restart the client: {e}"))?;
    std::process::Command::new(exe)
        .arg(path)
        .spawn()
        .map_err(|e| format!("cannot restart the client: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_keeps_one_copy_per_game() {
        let dir = std::env::temp_dir().join(format!("iwbtg-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::at(dir.clone());
        assert_eq!(remember(&store, "demo", b"one"), Ok(true));
        assert_eq!(
            remember(&store, "demo", b"one"),
            Ok(false),
            "same file: nothing written"
        );
        assert_eq!(
            remember(&store, "demo", b"two"),
            Ok(true),
            "a new version replaces it"
        );
        assert_eq!(std::fs::read(dir.join("library/demo.iwbz")).unwrap(), b"two");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_drop_that_is_not_a_package_is_refused() {
        let err = restart_on(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).unwrap_err();
        assert!(err.contains("not a package"), "{err}");
    }
}
