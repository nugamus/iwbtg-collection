//! Importing packages (docs/architecture/client/import.md, #36).
//!
//! - An `.iwbz` the client opens (from the command line, which is also what an OS file
//!   association calls) is kept in the library, `library/<id>.iwbz` in the data folder, for game
//!   select (M6) to list.
//! - A package dropped on the window (an `.iwbz` file or a package folder) is checked, then the
//!   client starts again on it and this one exits. Until game select can switch games in place,
//!   that is the simplest way to open another game; a run in progress ends as on any exit (its
//!   replay is kept). A drop that does not open shows a notice with the file's name and why
//!   (`ui::notice`, #648) and the game goes on.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;

use crate::package::{self, OpenError};
use crate::storage::{self, Store};
use crate::ui::notice;

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
            Err(refused) => {
                storage::console_warn(&refused.log);
                notice::post(notice::Kind::Drop, &refused.notice);
            }
        }
    }
}

/// Why a dropped path did not open.
#[derive(Debug)]
struct Refused {
    /// The console line: the full path and every error.
    log: String,
    /// What the notice on screen says: the file's name and the reason, the first error of a
    /// broken package and how many there are (the notice cuts it to its length).
    notice: String,
}

impl Refused {
    fn not_a_package(path: &Path) -> Self {
        Self {
            log: format!(
                "{}: not a package (an .iwbz file or a folder with a game.toml)",
                path.display()
            ),
            notice: format!(
                "{} is not a game package. Drop an .iwbz file or a folder with a game.toml.",
                file_name(path)
            ),
        }
    }

    fn not_opened(path: &Path, e: &OpenError) -> Self {
        let name = file_name(path);
        let notice = match e {
            OpenError::Invalid(lines) => {
                // Errors first; a warning alone does not refuse a package. `file:line:col:
                // error: message` loses its `error:` on the card.
                let errors: Vec<String> = lines
                    .iter()
                    .filter(|l| l.contains(": error: "))
                    .map(|l| l.replacen(": error: ", ": ", 1))
                    .collect();
                let shown = if errors.is_empty() { lines.clone() } else { errors };
                // The first one, and how many more: in the narrow leaderboard column a
                // second path wraps into a wall of text.
                let first = shown.first().map_or("", String::as_str);
                match shown.len() {
                    0 | 1 => format!("{name} has an error: {first}"),
                    n => format!("{name} has {n} errors. The first: {first}"),
                }
            }
            OpenError::Files(m) => {
                // The message starts with the full path; the card names the file only.
                let full = path.display().to_string();
                let reason = m
                    .strip_prefix(&full)
                    .and_then(|r| r.strip_prefix(": "))
                    .unwrap_or(m);
                format!("{name} could not be read: {reason}")
            }
        };
        Self {
            log: format!("{}: {e}", path.display()),
            notice,
        }
    }

    fn no_restart(e: std::io::Error) -> Self {
        let log = format!("cannot restart the client: {e}");
        Self {
            notice: format!("The client could not start again: {e}"),
            log,
        }
    }
}

/// The last part of `path`, or all of it when it has none.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Checks that `path` is a package that opens, then starts a new client on it.
fn restart_on(path: &Path) -> Result<(), Refused> {
    if !(package::is_iwbz(path) || path.join("game.toml").is_file()) {
        return Err(Refused::not_a_package(path));
    }
    package::open(path, false).map_err(|e| Refused::not_opened(path, &e))?;
    let exe = std::env::current_exe().map_err(Refused::no_restart)?;
    std::process::Command::new(exe)
        .arg(path)
        .spawn()
        .map_err(Refused::no_restart)?;
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
        assert!(err.log.contains("not a package"), "{err:?}");
        assert!(
            err.notice.starts_with("Cargo.toml is not a game package"),
            "the notice names the file, not its path: {err:?}"
        );
    }

    #[test]
    fn a_broken_package_shows_its_errors_without_warnings() {
        let path = Path::new("C:/games/broken.iwbz");
        let e = OpenError::Invalid(vec![
            "game.toml:1:1: warning: no credits".to_string(),
            "rooms/start.toml:3:5: error: unknown object kind \"spikee\"".to_string(),
            "rooms/start.toml:9:1: error: missing field `width`".to_string(),
        ]);
        let r = Refused::not_opened(path, &e);
        assert_eq!(
            r.notice,
            "broken.iwbz has 2 errors. The first: rooms/start.toml:3:5: unknown object kind \"spikee\""
        );
        assert!(
            r.log.contains("warning: no credits"),
            "the console keeps every line"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_names_only_the_file() {
        let path = Path::new("C:/games/big.iwbz");
        let e = OpenError::Files(format!("{}: over 1 bytes", path.display()));
        assert_eq!(
            Refused::not_opened(path, &e).notice,
            "big.iwbz could not be read: over 1 bytes"
        );
    }
}
