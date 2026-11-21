//! The save slot between sessions (docs/architecture/sim/world.md, Globals and save slot), kept through
//! `storage.rs`: a file natively, browser storage on the web. The sim keeps the slot in
//! its state and never touches files; the client writes it whenever it changes and reads it when
//! the game starts, so closing the window keeps the save like the originals' save file.
//!
//! One entry per game, `saves/<game id>.save` in the data area: postcard bytes of [`SaveFile`],
//! the versioned package provenance plus a [`Checkpoint`] (the same encoding a replay's
//! `FromSave` carries). The checkpoint names the room by its stable package name, resolved to
//! the package's current [`RoomId`] on load, so adding or reordering rooms does not move a save
//! into another room (adding the showcase's boot room did exactly that, #1144), and a save in a
//! room that is not gameplay is refused. The package id and revision decide whether this build
//! may read the save at all: a save from another package or another built-in revision is
//! refused rather than resumed under rules it was not written for (#1019). The version 2
//! envelope (#1019) and version 1 (#1145) migrate on read; neither recorded the precision, so
//! the migration infers it from the room's profile. A file that does not parse as a versioned
//! envelope (an older build's raw slot) is refused; the bytes are kept beside it as `<name>.old`.
//! Every refusal is one warning and a new game, never a silent resume. `--no-save-file` never
//! reads or writes it: ranked runs and checks always start from a new game.

use std::sync::OnceLock;

use bevy::prelude::*;
use iwbtg_sim::def::{GameDef, RoomId};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::{Checkpoint, CheckpointError, RoomKey, SavePrecision, SaveSlot};
use serde::{Deserialize, Serialize};

/// The only save format this build reads and writes.
const VERSION: u8 = 3;

/// The #1019 envelope: package id and revision, room name and the sim slot, but no checkpoint.
const VERSION_SAVE_SLOT: u8 = 2;

/// The #1145 envelope: the room name and the sim slot, with no provenance.
const VERSION_NAMED_ROOM: u8 = 1;

/// What a save file holds: the shared [`Checkpoint`] payload (the same versioned encoding a
/// replay's `FromSave` carries) and the package provenance that decides whether this build may
/// read it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct SaveFile {
    /// This format's version, [`VERSION`].
    version: u8,
    /// The package that wrote the save (`GameDef::id`).
    package: String,
    /// The built-in behavior revision the save was written under (`GameDef::engine_rev`).
    engine_rev: u16,
    /// The checkpoint, whose room is a stable name resolved on load.
    checkpoint: Checkpoint,
}

/// The version 2 envelope (#1019): package provenance and the sim slot under a room name.
#[derive(Serialize, Deserialize)]
struct SaveFileV2 {
    /// [`VERSION_SAVE_SLOT`], consumed to reach the fields after it.
    #[allow(dead_code, reason = "the version byte is consumed by the encoding, not read here")]
    version: u8,
    /// The package that wrote the save.
    package: String,
    /// The built-in behavior revision the save was written under.
    engine_rev: u16,
    /// `RoomDef::name`.
    room: String,
    /// The sim's saved slot.
    slot: SaveSlot,
}

/// The version 1 envelope (#1145): the sim's slot and the saved room's name, with no
/// provenance. Read only, to migrate a save written before [`SaveFileV2`] recorded it.
#[derive(Serialize, Deserialize)]
struct SaveFileV1 {
    /// [`VERSION_NAMED_ROOM`], consumed to reach the fields after it.
    #[allow(dead_code, reason = "the version byte is consumed by the encoding, not read here")]
    version: u8,
    /// `RoomDef::name`.
    room: String,
    /// The sim's saved slot.
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
    Ok(Some(decode(&bytes, def, &place)?))
}

/// Decodes the envelope, checks package and revision provenance, and resolves the checkpoint's
/// stable room name to the current [`RoomId`]. Versions 1 and 2 migrate; neither recorded the
/// checkpoint or the position precision.
fn decode(bytes: &[u8], def: &GameDef, place: &str) -> Result<SaveSlot, String> {
    let version = postcard::take_from_bytes::<u8>(bytes)
        .map(|(version, _)| version)
        .map_err(|e| not_a_save(place, def, bytes, &e))?;
    match version {
        VERSION => {
            let file: SaveFile =
                postcard::from_bytes(bytes).map_err(|e| not_a_save(place, def, bytes, &e))?;
            check_provenance(&file.package, file.engine_rev, def, place)?;
            resolve(place, file.checkpoint.to_slot(def))
        }
        VERSION_SAVE_SLOT => {
            let file: SaveFileV2 =
                postcard::from_bytes(bytes).map_err(|e| not_a_save(place, def, bytes, &e))?;
            check_provenance(&file.package, file.engine_rev, def, place)?;
            migrate(place, def, file.room, file.slot)
        }
        VERSION_NAMED_ROOM => {
            let file: SaveFileV1 =
                postcard::from_bytes(bytes).map_err(|e| not_a_save(place, def, bytes, &e))?;
            // No provenance to compare: the room name is the whole identity a version 1 save
            // carries, and the migration still refuses it if the room is gone or not gameplay.
            migrate(place, def, file.room, file.slot)
        }
        other => Err(unknown_version(place, def, bytes, other)),
    }
}

fn check_provenance(
    package: &str,
    engine_rev: u16,
    def: &GameDef,
    place: &str,
) -> Result<(), String> {
    if package != def.id {
        return Err(format!(
            "{place}: the save belongs to package `{package}`, not `{}`",
            def.id
        ));
    }
    if engine_rev != def.engine_rev {
        return Err(format!(
            "{place}: the save was written under built-in revision {engine_rev}, this build reads {}",
            def.engine_rev
        ));
    }
    Ok(())
}

/// A version 1 or 2 save: build the checkpoint under the room name it stored, inferring the
/// precision from the room's profile (the old envelopes did not record it). The stored position
/// was already quantized, so the inference only labels it.
fn migrate(place: &str, def: &GameDef, room: String, mut slot: SaveSlot) -> Result<SaveSlot, String> {
    let Some(at) = def.rooms.iter().position(|r| r.name == room) else {
        return Err(format!(
            "{place}: the saved room `{room}` is not in this version of the game"
        ));
    };
    let index = u32::try_from(at).map_err(|_| format!("{place}: too many rooms"))?;
    slot.precision = SavePrecision::of_model(def.physics_in(RoomId(index)).save.position.model());
    let checkpoint = Checkpoint::from_parts(def, RoomKey(room), &slot).map_err(|e| format!("{place}: {e}"))?;
    resolve(place, checkpoint.to_slot(def))
}

fn resolve(place: &str, slot: Result<SaveSlot, CheckpointError>) -> Result<SaveSlot, String> {
    slot.map_err(|e| format!("{place}: {e}"))
}

/// Whether the bytes are a raw `SaveSlot`, the format before #1145: no envelope, so its first
/// byte is the room index, not a version. Requiring the index to fit this package's rooms keeps
/// a newer envelope from being mistaken for one.
fn raw_slot(def: &GameDef, bytes: &[u8]) -> bool {
    postcard::from_bytes::<SaveSlot>(bytes).is_ok_and(|slot| {
        usize::try_from(slot.room.0).is_ok_and(|room| room < def.rooms.len())
    })
}

fn not_a_save(place: &str, def: &GameDef, bytes: &[u8], e: &postcard::Error) -> String {
    if raw_slot(def, bytes) {
        format!(
            "{place}: an older build's save (a raw save slot); this build reads version {VERSION}"
        )
    } else {
        format!("{place}: not a save file ({e})")
    }
}

fn unknown_version(place: &str, def: &GameDef, bytes: &[u8], version: u8) -> String {
    if raw_slot(def, bytes) {
        format!(
            "{place}: an older build's save (a raw save slot); this build reads version {VERSION}"
        )
    } else {
        format!(
            "{place}: not a save file (format version {version}, this build reads {VERSION})"
        )
    }
}

fn write(store: &Store, name: &str, def: &GameDef, slot: &SaveSlot) -> Result<(), String> {
    // `from_slot` refuses a room this build does not have or that is not gameplay, so this build
    // never writes a save it cannot read.
    let checkpoint = Checkpoint::from_slot(def, slot).map_err(|e| e.to_string())?;
    let file = SaveFile {
        version: VERSION,
        package: def.id.clone(),
        engine_rev: def.engine_rev,
        checkpoint,
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
    start_from(&store, def)
}

/// The read and the warning without the data folder: [`start_point`] with a store, so tests
/// can exercise the refusal path.
fn start_from(store: &Store, def: &GameDef) -> StartPoint {
    let new_game = StartPoint::NewGame { difficulty: 0 };
    let name = name_for(&def.id);
    match read(store, &name, def) {
        Ok(Some(slot)) => StartPoint::FromSave(slot),
        Ok(None) => new_game,
        Err(e) => {
            match keep_unusable(store, &name) {
                Some(kept) => storage::warn(&format!(
                    "ignoring the save file, starting a new game: {e}; the old file is kept at {kept}"
                )),
                None => storage::warn(&format!("ignoring the save file, starting a new game: {e}")),
            }
            new_game
        }
    }
}

/// Keeps a copy of a save this build cannot use beside it, so the bytes survive the new game
/// (a later build, or the player, can still recover them). Returns where the copy went.
fn keep_unusable(store: &Store, name: &str) -> Option<String> {
    let bytes = store.read(name).ok().flatten()?;
    let kept = format!("{name}.old");
    match store.write(&kept, &bytes) {
        Ok(()) => Some(store.describe(&kept)),
        Err(e) => {
            storage::warn(&format!("could not keep a copy of the unusable save file: {e}"));
            None
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
            precision: Default::default(),
            point: None,
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

    /// The current envelope for `room`, with `def`'s provenance.
    fn envelope(def: &GameDef, room: &str, slot: SaveSlot) -> SaveFile {
        let mut checkpoint = Checkpoint::from_slot(def, &slot).unwrap();
        checkpoint.room = RoomKey(room.into());
        SaveFile {
            version: VERSION,
            package: def.id.clone(),
            engine_rev: def.engine_rev,
            checkpoint,
        }
    }

    fn showcase() -> std::sync::Arc<GameDef> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
        crate::package::open(&dir, false).unwrap().def
    }

    fn room_index(def: &GameDef, name: &str) -> u32 {
        def.rooms
            .iter()
            .position(|r| r.name == name)
            .and_then(|at| u32::try_from(at).ok())
            .unwrap_or_else(|| panic!("the package has a room `{name}`"))
    }

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
        let file = envelope(&def, "nowhere", slot());
        store.write(NAME, &postcard::to_stdvec(&file).unwrap()).unwrap();
        assert!(
            read(&store, NAME, &def)
                .unwrap_err()
                .contains("is not in this version")
        );
        // A format this build does not read.
        let mut file = envelope(&def, "stage", slot());
        file.version = VERSION + 1;
        store.write(NAME, &postcard::to_stdvec(&file).unwrap()).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("format version"));
    }

    /// #1144: the save names its room, so a save in a play room resolves to that room now; a
    /// save in the showcase's boot room (an older build's courtyard index) is refused instead
    /// of resuming into an empty room.
    #[test]
    fn a_save_resolves_its_room_by_name_and_refuses_a_non_play_room() {
        let def = showcase();
        let store = temp("room-name");
        let mut saved = slot();
        saved.room = RoomId(room_index(&def, "hall"));
        write(&store, NAME, &def, &saved).unwrap();
        assert_eq!(read(&store, NAME, &def).unwrap(), Some(saved.clone()));

        // Room 0 is the boot room now; a save that names it cannot be resumed.
        let boot = envelope(&def, "boot", saved);
        store.write(NAME, &postcard::to_stdvec(&boot).unwrap()).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("is not gameplay"));

        // The write side refuses it too, so this build never writes a save it cannot read.
        let mut in_boot = slot();
        in_boot.room = RoomId(0);
        assert!(write(&store, NAME, &def, &in_boot).unwrap_err().contains("not gameplay"));
    }

    /// #1019: a save names its room, so a package update that reorders or inserts rooms keeps
    /// resuming in that room even though the numeric index moved.
    #[test]
    fn reordered_and_inserted_rooms_keep_the_saved_room() {
        let def = showcase();
        let store = temp("reorder");

        // Reorder: hall moves from index 2 to 1.
        let mut reordered = (*def).clone();
        let hall = room_index(&def, "hall") as usize;
        reordered.rooms.swap(0, hall);
        assert_eq!(room_index(&reordered, "hall"), 0);

        let mut saved = slot();
        saved.room = RoomId(u32::try_from(hall).unwrap());
        write(&store, NAME, &def, &saved).unwrap();
        let mut want = saved.clone();
        want.room = RoomId(0);
        assert_eq!(read(&store, NAME, &reordered).unwrap(), Some(want));

        // Insert: a new room pushes hall from index 2 to 3.
        let mut inserted = (*def).clone();
        inserted
            .rooms
            .insert(0, iwbtg_sim::def::RoomDef::new("inserted", 800, 608));
        assert_eq!(room_index(&inserted, "hall"), 3);

        let mut want = saved.clone();
        want.room = RoomId(3);
        assert_eq!(read(&store, NAME, &inserted).unwrap(), Some(want));
        assert_eq!(inserted.rooms[3].name, "hall");
    }

    /// A room the update removed cannot be resumed: one warning and a new game, never a
    /// different room's index.
    #[test]
    fn a_removed_room_is_refused() {
        let def = showcase();
        let store = temp("removed");
        let mut saved = slot();
        saved.room = RoomId(room_index(&def, "hall"));
        write(&store, NAME, &def, &saved).unwrap();

        let mut removed = (*def).clone();
        removed.rooms.retain(|r| r.name != "hall");
        assert!(
            read(&store, NAME, &removed)
                .unwrap_err()
                .contains("is not in this version")
        );
    }

    /// A save written by another package or another built-in revision is refused rather than
    /// resumed under rules it was not written for.
    #[test]
    fn a_save_from_another_package_or_revision_is_refused() {
        let def = demo_def();
        let store = temp("provenance");
        write(&store, NAME, &def, &slot()).unwrap();

        let mut other_package = (*def).clone();
        other_package.id = "another_game".into();
        let error = read(&store, NAME, &other_package).unwrap_err();
        assert!(
            error.contains(&format!("belongs to package `{}`", def.id)),
            "{error}"
        );

        let mut other_revision = (*def).clone();
        other_revision.engine_rev += 1;
        let error = read(&store, NAME, &other_revision).unwrap_err();
        assert!(error.contains("built-in revision"), "{error}");
    }

    /// A package change that leaves the saved room and the rules alone (a cosmetic or an
    /// unrelated room update) keeps the save: the envelope checks identity, not a whole-package
    /// hash.
    #[test]
    fn an_unrelated_package_change_keeps_the_save() {
        let def = demo_def();
        let store = temp("cosmetic");
        write(&store, NAME, &def, &slot()).unwrap();

        let mut changed = (*def).clone();
        changed.max_ticks += 1;
        changed.rooms[0].width += 1;
        assert_eq!(read(&store, NAME, &changed).unwrap(), Some(slot()));
    }

    /// A save from the #1145 build (version 1, no provenance) migrates on read; its room name
    /// still decides whether it can be resumed.
    #[test]
    fn a_version_1_envelope_migrates() {
        let def = showcase();
        let store = temp("migrate");
        let mut saved = slot();
        saved.room = RoomId(room_index(&def, "hall"));
        let old = SaveFileV1 {
            version: VERSION_NAMED_ROOM,
            room: "hall".into(),
            slot: saved.clone(),
        };
        store.write(NAME, &postcard::to_stdvec(&old).unwrap()).unwrap();
        assert_eq!(read(&store, NAME, &def).unwrap(), Some(saved));

        // A migrated file that names a room this build no longer has is still refused.
        let old = SaveFileV1 {
            version: VERSION_NAMED_ROOM,
            room: "gone".into(),
            slot: slot(),
        };
        store.write(NAME, &postcard::to_stdvec(&old).unwrap()).unwrap();
        assert!(read(&store, NAME, &def).unwrap_err().contains("is not in this version"));
    }

    /// The #1019 envelope (version 2) migrates to the shared checkpoint, and its stored room
    /// name, not the old numeric index, still decides the room.
    #[test]
    fn a_version_2_envelope_migrates() {
        let def = showcase();
        let store = temp("migrate-v2");
        let mut saved = slot();
        saved.room = RoomId(room_index(&def, "hall"));
        let old = SaveFileV2 {
            version: VERSION_SAVE_SLOT,
            package: def.id.clone(),
            engine_rev: def.engine_rev,
            room: "hall".into(),
            slot: saved.clone(),
        };
        store.write(NAME, &postcard::to_stdvec(&old).unwrap()).unwrap();
        assert_eq!(read(&store, NAME, &def).unwrap(), Some(saved.clone()));

        // A reordered package resolves the name, not the stored index.
        let mut reordered = (*def).clone();
        let hall = room_index(&def, "hall") as usize;
        reordered.rooms.swap(0, hall);
        let mut want = saved;
        want.room = RoomId(0);
        assert_eq!(read(&store, NAME, &reordered).unwrap(), Some(want));
    }

    /// A checkpoint whose room is gone or not gameplay starts a new game, keeps the bytes, and
    /// is never resumed: the same refusal path as any other incompatible save (#1019).
    #[test]
    fn an_unresolvable_room_starts_a_new_game_and_keeps_the_file() {
        let def = showcase();
        for room in ["gone", "boot"] {
            let store = temp(&format!("unresolvable-{room}"));
            let name = name_for(&def.id);
            let mut saved = slot();
            saved.room = RoomId(room_index(&def, "hall"));
            let mut checkpoint = Checkpoint::from_slot(&def, &saved).unwrap();
            checkpoint.room = RoomKey(room.into());
            let file = SaveFile {
                version: VERSION,
                package: def.id.clone(),
                engine_rev: def.engine_rev,
                checkpoint,
            };
            store
                .write(&name, &postcard::to_stdvec(&file).unwrap())
                .unwrap();
            assert!(matches!(
                start_from(&store, &def),
                StartPoint::NewGame { difficulty: 0 }
            ));
            assert!(store.read(&format!("{name}.old")).unwrap().is_some());
        }
    }

    /// A checkpoint whose save point moved, was removed or was renamed starts a new game and
    /// keeps the bytes: the identity refusal is the room refusal's path, never a fallback.
    #[test]
    fn a_stale_save_point_starts_a_new_game_and_keeps_the_file() {
        let def = demo_def();
        let store = temp("stale-point");
        let name = name_for(&def.id);
        let mut checkpoint = Checkpoint::from_slot(&def, &slot()).unwrap();
        checkpoint.point = Some("name:gone".into());
        let file = SaveFile {
            version: VERSION,
            package: def.id.clone(),
            engine_rev: def.engine_rev,
            checkpoint,
        };
        store
            .write(&name, &postcard::to_stdvec(&file).unwrap())
            .unwrap();
        assert!(matches!(
            start_from(&store, &def),
            StartPoint::NewGame { difficulty: 0 }
        ));
        assert!(store.read(&format!("{name}.old")).unwrap().is_some());
    }

    /// A raw `SaveSlot` (the format before #1145) has no room identity to trust: it starts a
    /// new game, and its bytes are kept beside the save so they can be recovered.
    #[test]
    fn a_legacy_raw_slot_is_refused_and_kept() {
        let def = demo_def();
        let store = temp("legacy");
        let name = name_for(&def.id);
        let bytes = postcard::to_stdvec(&slot()).unwrap();
        store.write(&name, &bytes).unwrap();

        assert!(read(&store, &name, &def).unwrap_err().contains("raw save slot"));
        assert!(matches!(
            start_from(&store, &def),
            StartPoint::NewGame { difficulty: 0 }
        ));
        assert_eq!(store.read(&format!("{name}.old")).unwrap(), Some(bytes));
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
