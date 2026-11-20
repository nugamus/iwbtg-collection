//! The save slot between sessions (docs/architecture/sim/world.md, Globals and save slot), kept through
//! `storage.rs`: a file natively, browser storage on the web. The sim keeps the slot in
//! its state and never touches files; the client writes it whenever it changes and reads it when
//! the game starts, so closing the window keeps the save like the originals' save file.
//!
//! One entry per game, `saves/<game id>.save` in the data area: postcard bytes of [`SaveFile`],
//! the sim's `SaveSlot` plus a format version and the saved room's package name. The name is
//! resolved to the package's current [`RoomId`] on load, so adding or reordering rooms does not
//! move a save into another room (adding the showcase's boot room did exactly that, #1144), and
//! a save in a room that is not gameplay is refused. A file that does not parse as this format
//! (an older build's raw slot) is a warning and a new game. `--no-save-file` never reads or
//! writes it: ranked runs and checks always start from a new game.

use std::sync::OnceLock;

use bevy::prelude::*;
use iwbtg_sim::def::{GameDef, RoomId, RoomRole};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::SaveSlot;
use serde::{Deserialize, Serialize};

/// The only save format this build reads and writes.
const VERSION: u8 = 1;

/// What a save file holds: the sim's slot and the saved room's package name.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct SaveFile {
    /// This format's version, [`VERSION`].
    version: u8,
    /// `RoomDef::name`, resolved to the package's current room on load.
    room: String,
    /// The sim's saved slot, whose own `room` is resolved from `room` when read.
    slot: SaveSlot,
}

use crate::storage::{self, Store};
use crate::view::Game;

/// The save file's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct SaveArgs {
    /// Never read or write the save file: always start a new game (ranked runs, checks).
    #[arg(long)]
    pub no_save_file: bool,
}

/// The save file's place in the module list (main.rs): its flags and plugin. A ranked run always
/// turns the file off, so `prepare` folds that into the flags before the plugin reads them.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = SaveArgs;

    fn prepare(args: &mut SaveArgs, startup: &crate::registration::Startup) -> Result<(), String> {
        args.no_save_file |= startup.ranked;
        configure(args);
        Ok(())
    }

    fn add(
        app: &mut bevy::prelude::App,
        _args: SaveArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(SavePlugin);
    }
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
    let file: SaveFile =
        postcard::from_bytes(&bytes).map_err(|e| format!("{place}: not a save file ({e})"))?;
    if file.version != VERSION {
        return Err(format!(
            "{place}: save format version {}, this build reads {VERSION}",
            file.version
        ));
    }
    let Some(at) = def.rooms.iter().position(|r| r.name == file.room) else {
        return Err(format!(
            "{place}: the saved room `{}` is not in this version of the game",
            file.room
        ));
    };
    if def.rooms[at].role != RoomRole::Play {
        return Err(format!(
            "{place}: the saved room `{}` is not gameplay",
            file.room
        ));
    }
    let room = u32::try_from(at).map_err(|_| format!("{place}: too many rooms"))?;
    let mut slot = file.slot;
    slot.room = RoomId(room);
    Ok(Some(slot))
}

fn write(store: &Store, name: &str, def: &GameDef, slot: &SaveSlot) -> Result<(), String> {
    let room = def
        .room(slot.room)
        .ok_or_else(|| "the saved room is not in this version of the game".to_owned())?;
    let file = SaveFile {
        version: VERSION,
        room: room.name.clone(),
        slot: slot.clone(),
    };
    let bytes = postcard::to_stdvec(&file).map_err(|e| e.to_string())?;
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
    /// The run's first frame has been seen.
    started: bool,
}

impl Default for Written {
    fn default() -> Self {
        Written {
            slot: None,
            store: Store::data(),
            started: false,
        }
    }
}

/// Writes the file when the sim's slot changes; a deleted slot (a new game) deletes the file.
fn persist(game: NonSend<Game>, mut written: ResMut<Written>) {
    let slot = &game.sim.world().save;
    if !written.started {
        // The slot the game started with is already on disk (or was never saved).
        written.started = true;
        written.slot.clone_from(slot);
        return;
    }
    if *slot == written.slot {
        return;
    }
    if let Some(store) = &written.store {
        let name = name_for(&game.sim.def().id);
        let result = match slot {
            Some(s) => write(store, &name, game.sim.def(), s),
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
            // Each run starts from the slot its game opened with.
            crate::shell::run_resource(app, |_| Written::default());
            app.add_systems(Update, persist.in_set(crate::shell::InRun));
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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
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
        write(&store, NAME, &def, &slot()).unwrap();
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
        let file = SaveFile {
            version: VERSION,
            room: "nowhere".into(),
            slot: slot(),
        };
        store.write(NAME, &postcard::to_stdvec(&file).unwrap()).unwrap();
        assert!(
            read(&store, NAME, &def)
                .unwrap_err()
                .contains("is not in this version")
        );
        // A format this build does not read.
        let file = SaveFile {
            version: VERSION + 1,
            room: "room_1".into(),
            slot: slot(),
        };
        store.write(NAME, &postcard::to_stdvec(&file).unwrap()).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("save format version"));
    }

    /// #1144: the save names its room, so a save in a play room resolves to that room now; a
    /// save in the showcase's boot room (an older build's courtyard index) is refused instead
    /// of resuming into an empty room.
    #[test]
    fn a_save_resolves_its_room_by_name_and_refuses_a_non_play_room() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
        let def = crate::package::open(&dir, false).unwrap().def;
        let store = temp("room-name");
        let hall = def
            .rooms
            .iter()
            .position(|r| r.name == "hall")
            .and_then(|at| u32::try_from(at).ok())
            .expect("the showcase has a hall");
        let mut saved = slot();
        saved.room = RoomId(hall);
        write(&store, NAME, &def, &saved).unwrap();
        assert_eq!(read(&store, NAME, &def).unwrap(), Some(saved.clone()));

        // Room 0 is the boot room now; a save that names it cannot be resumed.
        let boot = SaveFile {
            version: VERSION,
            room: "boot".into(),
            slot: saved,
        };
        store.write(NAME, &postcard::to_stdvec(&boot).unwrap()).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("is not gameplay"));
    }

    #[test]
    fn touching_a_save_and_starting_again_resumes_there() {
        use iwbtg_sim::input::{Action, InputFrame};
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/save.pkg");
        let def = crate::package::open(&dir, false).unwrap().def;
        let options = |start| SimOptions {
            start,
            seed: 0,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
            map: None,
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
        write(&store, NAME, &def, &slot).unwrap();
        let loaded = read(&store, NAME, &def).unwrap().unwrap();
        let again = Sim::new(def.clone(), options(StartPoint::FromSave(loaded))).unwrap();
        let world = again.world();
        let kid = world.get(world.kid(&again.rules().tick).unwrap()).unwrap();
        assert_eq!((kid.m.x(), kid.m.y()), (slot.x, slot.y));
    }
}
