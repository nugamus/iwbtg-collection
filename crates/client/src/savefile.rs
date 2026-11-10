//! The save slot between sessions (docs/architecture/sim.md, Globals and save slot), kept through
//! `storage.rs`: a file natively, browser storage on the web. The sim keeps the slot in
//! its state and never touches files; the client writes it whenever it changes and reads it when
//! the game starts, so closing the window keeps the save like the originals' save file.
//!
//! One entry per game, `saves/<game id>.save` in the data area (postcard bytes of the sim's
//! `SaveSlot`). `--no-save-file` never reads or writes it: ranked runs and checks always
//! start from a new game.

use std::sync::OnceLock;

use bevy::prelude::*;
use iwbtg_sim::def::GameDef;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::SaveSlot;

use crate::storage::{self, Store};
use crate::view::Game;

/// The save file's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct SaveArgs {
    /// Never read or write the save file: always start a new game (ranked runs, checks).
    #[arg(long)]
    pub no_save_file: bool,
}

static OPTIONS: OnceLock<SaveArgs> = OnceLock::new();

/// Called by `main` before the game starts.
pub fn configure(args: &SaveArgs) {
    let _ = OPTIONS.set(*args);
}

fn enabled() -> bool {
    !OPTIONS.get().is_some_and(|o| o.no_save_file)
}

/// A game's save in the data area of [`storage`] (native:
/// `<data folder>/iwbtg-collection/saves/<game id>.save`; web: a `localStorage` entry).
pub fn name_for(game_id: &str) -> String {
    format!("saves/{game_id}.save")
}

/// Reads a save. `Ok(None)` is nothing saved.
pub fn read(store: &Store, name: &str, def: &GameDef) -> Result<Option<SaveSlot>, String> {
    let Some(bytes) = store.read(name)? else {
        return Ok(None);
    };
    let place = store.describe(name);
    let slot: SaveSlot =
        postcard::from_bytes(&bytes).map_err(|e| format!("{place}: not a save file ({e})"))?;
    if def.room(slot.room).is_none() {
        return Err(format!(
            "{place}: the saved room does not exist in this version of the game"
        ));
    }
    Ok(Some(slot))
}

fn write(store: &Store, name: &str, slot: &SaveSlot) -> Result<(), String> {
    let bytes = postcard::to_stdvec(slot).map_err(|e| e.to_string())?;
    // Natively written beside the file and renamed, so a crash never leaves half a save.
    store.write(name, &bytes)
}

/// Where the game starts: the saved spot when a good save exists, else a new game. A broken
/// save or unreadable storage is a warning, not an error.
pub fn start_point(def: &GameDef) -> StartPoint {
    let new_game = StartPoint::NewGame { difficulty: 0 };
    if !enabled() {
        return new_game;
    }
    let Some(store) = Store::data() else {
        return new_game;
    };
    match read(&store, &name_for(&def.id), def) {
        Ok(Some(slot)) => StartPoint::FromSave(slot),
        Ok(None) => new_game,
        Err(e) => {
            storage::warn(&format!("ignoring the save file, starting a new game: {e}"));
            new_game
        }
    }
}

/// The slot as last written, to spot changes, and where it goes (opened once, so a browser with
/// storage turned off warns once, not at every save).
#[derive(Resource)]
struct Written {
    slot: Option<SaveSlot>,
    store: Option<Store>,
}

impl Default for Written {
    fn default() -> Self {
        Written {
            slot: None,
            store: Store::data(),
        }
    }
}

/// Writes the file when the sim's slot changes; a deleted slot (a new game) deletes the file.
fn persist(game: NonSend<Game>, mut written: ResMut<Written>, mut started: Local<bool>) {
    let slot = &game.sim.world().save;
    if !*started {
        // The slot the game started with is already on disk (or was never saved).
        *started = true;
        written.slot.clone_from(slot);
        return;
    }
    if *slot == written.slot {
        return;
    }
    if let Some(store) = &written.store {
        let name = name_for(&game.sim.def().id);
        let result = match slot {
            Some(s) => write(store, &name, s),
            None => store.remove(&name),
        };
        if let Err(e) = result {
            // Keep playing; try again at the next change.
            storage::warn(&format!("could not update the save file: {e}"));
        }
    }
    written.slot.clone_from(slot);
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

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("iwbtg-save-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    const NAME: &str = "saves/x.save";

    #[test]
    fn a_save_round_trips_through_the_file() {
        let def = demo_def();
        let store = temp("round");
        write(&store, NAME, &slot()).unwrap();
        assert_eq!(read(&store, NAME, &def).unwrap(), Some(slot()));
    }

    #[test]
    fn a_missing_file_is_no_save_and_a_broken_one_is_an_error() {
        let def = demo_def();
        let store = temp("broken");
        assert_eq!(read(&store, NAME, &def).unwrap(), None);
        store.write(NAME, b"junk").unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("not a save file"));
        // A room the game does not have.
        let mut bad = slot();
        bad.room = RoomId(9999);
        write(&store, NAME, &bad).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("does not exist"));
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
        let store = temp("resume");
        write(&store, NAME, &slot).unwrap();
        let loaded = read(&store, NAME, &def).unwrap().unwrap();
        let again = Sim::new(def.clone(), options(StartPoint::FromSave(loaded))).unwrap();
        let world = again.world();
        let kid = world.get(world.kid(&again.rules().tick).unwrap()).unwrap();
        assert_eq!((kid.m.x(), kid.m.y()), (slot.x, slot.y));
    }
}
