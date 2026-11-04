//! The save slot on disk (docs/architecture.md, Globals and save slot). The sim keeps the slot in
//! its state and never touches files; the client writes it whenever it changes and reads it when
//! the game starts, so closing the window keeps the save like the originals' save file.
//!
//! One file per game: `<data folder>/iwbtg-collection/saves/<game id>.save` (postcard bytes of the
//! sim's `SaveSlot`). `--no-save-file` never reads or writes it: ranked runs and checks always
//! start from a new game.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bevy::prelude::*;
use iwbtg_sim::def::GameDef;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::SaveSlot;

use crate::view::Game;

#[derive(Clone, Copy, Default)]
pub struct Options {
    /// Never read or write the save file.
    pub disabled: bool,
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

/// Called by `main` before the game starts.
pub fn configure(options: Options) {
    let _ = OPTIONS.set(options);
}

fn enabled() -> bool {
    !OPTIONS.get().is_some_and(|o| o.disabled)
}

/// Where a game's save lives: the user's data folder.
pub fn path_for(game_id: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| {
        b.join("iwbtg-collection")
            .join("saves")
            .join(format!("{game_id}.save"))
    })
}

/// Reads a save file. `Ok(None)` is no file.
pub fn read(path: &Path, def: &GameDef) -> Result<Option<SaveSlot>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let slot: SaveSlot =
        postcard::from_bytes(&bytes).map_err(|e| format!("{}: not a save file ({e})", path.display()))?;
    if def.room(slot.room).is_none() {
        return Err(format!(
            "{}: the saved room does not exist in this version of the game",
            path.display()
        ));
    }
    Ok(Some(slot))
}

fn write(path: &Path, slot: &SaveSlot) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let bytes = postcard::to_stdvec(slot).map_err(|e| e.to_string())?;
    // Write beside it and rename, so a crash never leaves half a save.
    let tmp = path.with_extension("save.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Where the game starts: the saved spot when a good save file exists, else a new game. A broken
/// file is a warning on stderr, not an error.
pub fn start_point(def: &GameDef) -> StartPoint {
    let new_game = StartPoint::NewGame { difficulty: 0 };
    if !enabled() {
        return new_game;
    }
    let Some(path) = path_for(&def.id) else {
        return new_game;
    };
    match read(&path, def) {
        Ok(Some(slot)) => StartPoint::FromSave(slot),
        Ok(None) => new_game,
        Err(e) => {
            eprintln!("warning: ignoring the save file, starting a new game: {e}");
            new_game
        }
    }
}

/// The slot as last written, to spot changes.
#[derive(Resource, Default)]
struct Written(Option<SaveSlot>);

/// Writes the file when the sim's slot changes; a deleted slot (a new game) deletes the file.
fn persist(game: NonSend<Game>, mut written: ResMut<Written>, mut started: Local<bool>) {
    let slot = &game.sim.world().save;
    if !*started {
        // The slot the game started with is already on disk (or was never saved).
        *started = true;
        written.0.clone_from(slot);
        return;
    }
    if *slot == written.0 {
        return;
    }
    let Some(path) = path_for(&game.sim.def().id) else {
        return;
    };
    let result = match slot {
        Some(s) => write(&path, s),
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", path.display())),
            _ => Ok(()),
        },
    };
    if let Err(e) = result {
        // Keep playing; try again at the next change.
        eprintln!("warning: could not update the save file: {e}");
    }
    written.0.clone_from(slot);
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            app.init_resource::<Written>().add_systems(Update, persist);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::def::RoomId;

    fn slot() -> SaveSlot {
        SaveSlot {
            room: RoomId(0),
            x: iwbtg_sim::num::Real::new(96.0),
            y: iwbtg_sim::num::Real::new(544.0),
            flipped: false,
            facing: 1,
            difficulty: 2,
            deaths: 7,
            time: 1234,
            bosses: 0,
            items: 3,
            clear: false,
        }
    }

    fn demo_def() -> std::sync::Arc<GameDef> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo.pkg");
        crate::package::open(&dir, false).unwrap().def
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("iwbtg-save-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("x.save")
    }

    #[test]
    fn a_save_round_trips_through_the_file() {
        let def = demo_def();
        let path = temp("round");
        write(&path, &slot()).unwrap();
        assert_eq!(read(&path, &def).unwrap(), Some(slot()));
    }

    #[test]
    fn a_missing_file_is_no_save_and_a_broken_one_is_an_error() {
        let def = demo_def();
        let path = temp("broken");
        assert_eq!(read(&path, &def).unwrap(), None);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"junk").unwrap();
        assert!(read(&path, &def).unwrap_err().contains("not a save file"));
        // A room the game does not have.
        let mut bad = slot();
        bad.room = RoomId(9999);
        write(&path, &bad).unwrap();
        assert!(read(&path, &def).unwrap_err().contains("does not exist"));
    }

    #[test]
    fn touching_a_save_and_starting_again_resumes_there() {
        use iwbtg_sim::input::{Action, InputFrame};
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-save.pkg");
        let def = crate::package::open(&dir, false).unwrap().def;
        let options = |start| SimOptions {
            start,
            seed: 0,
            dll_mode: DllMode::Model,
        };
        let mut sim = Sim::new(def.clone(), options(StartPoint::NewGame { difficulty: 0 })).unwrap();
        // A new game starts with the first-play slot; walking onto the save point changes it.
        let first = sim.world().save.clone();
        let right = InputFrame {
            held: Action::Right.bit(),
            pressed: 0,
            released: 0,
        };
        for _ in 0..300 {
            sim.step(right);
            if sim.world().save != first {
                break;
            }
        }
        let slot = sim
            .world()
            .save
            .clone()
            .expect("walking onto the save point saves");
        let path = temp("resume");
        write(&path, &slot).unwrap();
        let loaded = read(&path, &def).unwrap().unwrap();
        let again = Sim::new(def.clone(), options(StartPoint::FromSave(loaded))).unwrap();
        let world = again.world();
        let kid = world.get(world.kid(&again.rules().tick).unwrap()).unwrap();
        assert_eq!((kid.m.x(), kid.m.y()), (slot.x, slot.y));
    }
}
