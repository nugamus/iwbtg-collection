# The simulation: World model

Part of [the simulation](../sim.md).

## World model

The sim is shaped like GameMaker, not like an ECS. Rust enums and plain structs, matched exhaustively, so an agent sees every case and the compiler flags a missing one.

```rust
struct Instance {
    id: InstId,               // u64, from a counter starting at 100001, never reused
    kind: KindId,             // built-in kind or package object
    rank: u16,                // position of the kind in object_order
    // GM built-in variables, all Real (f64)
    x, y, xstart, ystart, xprevious, yprevious,
    hspeed, vspeed, speed, direction, gravity, gravity_direction, friction,
    image_index, image_speed, image_xscale, image_yscale, image_angle, image_alpha,
    depth: Real,
    sprite: SpriteId, mask: MaskId,
    visible: bool, solid: bool, persistent: bool,
    alarms: [i32; 12],
    group: Option<GroupId>, name: Option<NameId>,
    state: KindState,         // per-kind data, see below
    script: Option<ScriptRef>,
    vars: BTreeMap<VarName, Value>, // custom variables; values, arrays included
    alive: bool,
}

enum KindState {
    Kid(KidState), Block, FakeBlock, HiddenBlock { shown: bool },
    Spike { dir: Dir }, Killer, Save { .. }, Warp { to: WarpTarget }, KidStart,
    Platform(PlatformState), MovingBlock(MovingState), Water { variant: u8 },
    Vine { side: Side }, JumpRefresher { .. }, Slope { dir: Dir }, GravityFlip { up: bool },
    Ladder, BulletBlocker, Trigger(TriggerState), Projectile, KidBullet, Camera(CameraState),
    MusicZone { .. }, Scripted,
}
```

The sketch above is the original idea; the live types are in `crates/sim/src/world/instance.rs` and `def/builtin.rs` (moved from [the sim core plan](../../plans/sim-core.md) 1.1, 1.5, 6.1, which is history): per-owner `KindState` parts (kid, built-in, script, probe), `ClassMask` collision classes instead of parent objects, alarms as reals, and the built-in list from `research/specs/mechanics.md`.

- **Storage.** Instances live in a `Vec<Instance>` sorted by id. Lookup by id is a binary search. Runtime ids come from `World::next_id`, which starts at `World::first_runtime_id` (`[engine].first_runtime_id`, else one above the highest placed `gm_id`, else 100001) and only grows, so a runtime instance is appended. A room-placed instance with a GM8 id (`gm_id`, #669) gets that id instead; it is below every runtime id, so it goes in at its sorted place and id order is not creation order for it (TICK-027..TICK-035, "Placed instance ids"). Room entry skips a placed instance whose id is already alive (a carried persistent instance), register R29. The list is private to `world`: other code reads it through `World::instances` and changes an instance only through `World::get_mut`, which keeps the broadphase grid in step (see Collision). Event dispatch does not keep per-rank lists: each phase snapshots `events::dispatch_order` once (register R2). The GM instance list order is separate: `World::instance_list` walks every stored instance depth largest first, then by the room-entry rank in `World::room_entry`: the room's placed instances in file order, the carried persistent ones in their previous list order, then the instances created since (Create events of placed instances, Room Start, later ticks) in creation order (research#142, ROOM-075 "Instance list order"). The room transaction takes the old ranks at `Commit` and sets the new ones after `Placed` (`World::set_room_entry`); a restored persistent room's instances rank after the carried ones, in id order (not traced). `room_entry` is hashed, since it decides "the first". `with (all)`, `instance_id[]`/`instance_count` (not built yet, #791, #997) and the first-instance queries all use this one order.
- **Creation and destruction.** New instances are appended. Destroy runs the Destroy event, then applies `RunnerRules::lifetime` (`LifetimeRules`, A1a of #1174); the rest of the destroying event still runs, and the instance gets no later events (TICK-074, Confirmed (recorder)). Under `gm8` (`DestroyedVisibility::HalfDead`, the default) it sets `alive = false` and stays in the list until the end of the step (Confirmed, manual 403_05), so the sweep at the end of the tick removes it with `retain`, which keeps order. Under `standard` (`Immediate`) `World::remove` drops it from the store at once after its Destroy event. Every reader asks one predicate, `World::visible_instance`/`World::visible`: `exists`, `first_of`, `count_of`, `iter_alive`/`iter_visible`, and (A1b of #1174) the collide pair gather and queries, event dispatch and `with` walks, the script `instances`/collision guards, draw order, view follow and timelines. No reader re-derives `alive && active` or goes through `exists`. `TickRules::dead_in_queries` stays at false (the gm8 R22 opt-in; no package uses it), and the collision phase skips every remaining pair that has a destroyed instance as self or as other (R22, Confirmed (recorder), research T-DEAD-1). A stale `InstId` is never reused, and the script layer turns a dead handle into a typed error; a native generation is not needed.
- **Persistence scopes.** `world::Persistence` (E2 of #1153) is the explicit per-instance/kind scope: `Room` (rebuilt by the room on entry; the default), `Run` (GM `persistent`: carried into the new room), `Save` (the `save` built-in: writes the checkpoint; not carried). `Instance::persistence` is derived at spawn and on `change_kind` (a `save` base wins over the persistent flag) and is not serialized or hashed; `Instance::persistent` stays the flag the carry reads. Three different things: `RoomDef::persistent` keeps a room's own placed instances while the player is away and restores them on return (`World::persistent_rooms`), `Persistence::Run` carries an instance to any room, and `Persistence::Save` writes the checkpoint.
- **Values.** A custom variable and a global hold a `Value` (`world/instance.rs`): `Real`, `Str`, `Inst`, `None` or `Array(Vec<Value>)`. An array is dense and may hold arrays; it has at most `MAX_ARRAY_SIZE` = 10,000 elements, the cap the script host sets with `set_max_array_size(10_000)` (decision 0001), and `Value::array`/`Value::array_push` are the checked ways to build or grow one (the 10,001st element is an `Unsupported` error). Assignment copies (the GML 8 rule): each variable owns its array, so writing one never changes another (`Value::assigned`). A bare write to a variable that already holds an array writes element 0 and keeps the other elements (`world::assign_bare`; GM8, research `creation-code.md` Global arrays, Confirmed by recorder, #1133, contract `script/global-array-bare-write`, register R31): room creation code's bare and index-0 `set_global` rows and a `gm8` script's `this.<var> = v` use it; under the `standard` script value model `this.<var> = v` replaces the value. Arrays are part of the hashed state ([hash](hash.md)) and of `World`'s postcard encoding; the save slot holds no variables, so no array ever goes into a save file. Scripts read and write arrays through a `VarArray` (`script/array.rs`, #997): indexing, `len`, `for v in a`, and `push`/`pop` (#1383).
- **Roles replace parent objects.** Collision queries ask for roles (`solid`, `killer`, `platform`, `water`, `vine`, `stops_bullets`, ...) instead of GM parent objects. Each kind declares its roles.
- **Package objects.** A package object (`@boss1`) is `KindState::Scripted` or inherits a built-in with `base = "killer"`, `base = "trigger"` and so on. A scripted object with a base keeps the base's native behavior and roles; the script adds events. Collision with the kid works through roles, never through script code.
- **The kid** is a built-in. Scripts read it, can kill it and can set documented knobs (for example gravity flip). They never replace its physics. Its physics profile is per room (#33): kid code reads `GameDef::physics_in(room)` (in a `Ctx`, `ctx.physics()`), the room's own profile from `GameDef::room_physics` or else the game's `GameDef::physics`, never `def.physics` directly. The kid kind's event set is built once from the game's profile, so the loader requires every room profile to have the same one.
- **Globals and save slot.** Game-wide variables persist across rooms, like GM globals. The save slot (room, position, gravity, flags) is part of the sim state. The client (`savefile.rs`, through `storage.rs`: docs/architecture/client/storage.md) writes the slot to `<data folder>/iwbtg-collection/saves/<game id>.save` (Windows: `%LOCALAPPDATA%`; elsewhere `$XDG_DATA_HOME` or `~/.local/share`; on the web a `localStorage` entry) whenever the sim's slot changes, as a versioned envelope around a [`Checkpoint`](rooms.md#saves): the stable room name, ruleset/profile, position precision, position, counters and the save object's stable key (`name:<declared name>` or `at:<room>/<kind>@<xbits>,<ybits>` from its exact spawn position; `None` for a first-play or creation-code save), plus the package id and built-in revision. A room-file `save` instance gets that key in `Ctx::spawn` (`Instance::point`); the save built-in copies it onto the slot with `save::set_point`. On load the key is resolved against the room data and a moved, removed, renamed or malformed point is refused, never mapped to a different instance. `World.save` itself stays the resolved runtime slot and the state hash does not change: the `precision` and `point` fields on the slot and on `Instance` are not serialized or hashed. The file is written beside itself and renamed, so a crash never leaves half a save. A slot the sim deletes (a new game) deletes the file. At start the client hands the checkpoint to `StartPoint::FromSave`, and the sim resolves its room name to the current room and begins from the saved spot; a missing file is a new game, and a broken or incompatible file (another package or revision, a room the game does not have, or an older build's raw slot) is a warning (stderr, the browser console, and the storage notice) and a new game, never a crash and never a resume in another room. The unusable bytes are copied beside the file as `<name>.old`. A cosmetic or unrelated-room package update keeps the save. `--no-save-file` never reads or writes it. Ranked replays always start from a new game.
- **Sampling the tick of a room change (#450).** `tick::run_tick` is `run_rows` (rows 0-14) then `finish_tick` (rows 15-16, the room change); `Sim::step_rows` and `Sim::finish_step` expose the split. The recorder's row of a room-change tick shows the old room's instances (kid/B.md Restart timing, ROOM-063 D/E retry), so the trace runner samples between the two when a room change is pending. `Sim::step` and the world after it are unchanged.

- **Retry, reset and saves (WP4b).** The `world` built-in calls `restart::run_restart` (R) and `run_reset` (F2) at its rank, in the phase the lifecycle policy's trigger names (`GameDef::lifecycle()`, decision 0005, #1149). `game_restart_via_boot` moves (or creates) the kid at the saved spot, counts a death when `death_on_restart = "save_counters"`, requests `game_restart` and begins `restart::InitPhase`, the engine-owned stand-in for the temp file: it survives the restart, `restart::boot_step` (world Step) counts `retry_delay_ticks` (`boot_ticks` in the profile) down and on the last init tick loads the slot and requests the saved room (timeline N, N+1, N+2). The phase is not a package room; the gm8 profile still enters the Boot-role room because an old replay's hash observes it. A delay of 0 is permanently `Unsupported` (R17; #225). F2's temp-marker reset stops every sound before `game_restart` (ROOM-045, ROOM-046; kid/A4.md "F2 pressed"); the kaenbyou / eat-the-lemon `gameReset` has no recorded stop, and its global list is the research trace request nugamus/iwbtg-collection-research#270. The other kinds recreate the kid and request the saved room at once; showdown has no `game_restart` (R26, settled by the recorder). Kid-side extras (B's timer and `reverse`, showdown's `frozen`) wait for `kid::api`. `save::quantize` has six position models; `save_now` reads the kid through its instance only. `view::follow_update` runs the runner view follow (border halved when larger than half the view, clamp, speed limit; speed 0 freezes an axis).
- **Output queue.** Audio and visual effects (`play_sound`, `play_music`, screen shake, blood) are events pushed to `output`. The client consumes them. `Sim::step` clears `output` first, except the first step (tick 0), which keeps what `Sim::new` produced (the first room's start: music, sounds); read `output` after each step and never after `Sim::new`. The sim never reads anything back from audio or rendering.
- **Room creation code (#629).** `room::run_action` runs a room's `on_create` rows in step 6 of the room start. `RoomAction::When` wraps a row with its condition (`RoomWhen`: difficulty below, kid exists, kind absent). An indexed `set_global` (`global.stage[global.savenum]`) writes element `index` of the global: element 0 is the variable itself — GM8 aliases `a` and `a[0]` (research#130, Confirmed), so an index-0 write is the bare assignment (`world::assign_bare`: when the global already holds an array it writes element 0 and keeps the rest, #1133) and no `stage[<n>]` variable exists — and a higher index makes the global a dense `Value::Array` whose element 0 is the scalar already stored (unset when the global was unset) and whose skipped slots are unset (`world::set_element`, the same growth rule a script write uses, #1382): a read follows the run's init policy, 0 under `gm8` and a script error under `standard`. `world::set_element` enforces the sim's `MAX_ARRAY_SIZE` cap (10,000; below GM8's own limits), so a write past it is `Unsupported`. `save_game` with no kid is `save::save_without_kid` (x 0, y 0).

## Instance and world reference

The live text of plan sections 1.1-1.6. The [sim core plan](../../plans/sim-core.md) keeps the same text as history; this page is the authority.

### Instances

```rust
pub struct InstId(pub u64);           // counter, first id 100_001, never reused (manual: ids >= 100000)

pub struct Instance {
    pub id: InstId,
    pub kind: KindId,                 // built-in kind or package object (preset/scripted)
    pub rank: u16,                    // dispatch rank from object_order, fixed at creation
    pub classes: ClassMask,           // GM parent emulation: what collision targets and queries see
    pub alive: bool,                  // false after destroy; swept at end of tick
    pub active: bool,                 // false while deactivated (xia pause, machinary)
    pub persistent: bool,
    pub solid: bool,                  // per instance, may change at run time (Confirmed)
    pub visible: bool,
    pub depth: Real,
    pub m: Motion,
    pub img: Image,                   // sprite, mask, image_index, image_speed, xscale, yscale, angle, alpha
    pub alarms: [Real; 12],           // <= 0 is idle; default -1 (Inferred)
    pub timeline: Option<TimelineRun>,
    pub path: Option<PathRun>,
    pub state: KindState,             // per-owner state, see below
    pub vars: BTreeMap<VarId, Value>, // custom variables (scripts, creation-code params)
}
```

- `ClassMask(u128)` replaces GM parent objects. A collision event or query "with object X" becomes "with class X". Each built-in declares default classes; a package object may add classes (`counts_as`). Examples: `platform` has `PLATFORM | BLOCK` in 17 games and `PLATFORM | SOLIDBLOCK` in eat-the-rori-girl and help-the-fairy-tale (COL-078, Inferred "Platforms run the kid's block event"). Spikes and cherries have `KILLER`. This keeps "a collision event with a parent also fires for its children" (Confirmed, manual 205_00) without a parent tree in the sim.
- Class bits (named list fixed; in `def/class.rs`): `KID, KID_FLIPPED, KID_BULLET, BLOCK, SOLIDBLOCK, PLATFORM, KILLER, WATER1, WATER2, WATER3, VINE_L, VINE_R, ROOM_CHANGER, EDGE_WARP, SAVE, SLIP, SLOPE, SLOPE_WALL, LADDER, FAKE_BLOCK, BULLET_BLOCKER, TRIGGER, TRAP_STOPPER, MOVING_BLOCK, CONVEYOR, PARAM_ZONE, BOW, BLOOD, SLOPE_MIRROR` (bits 0-28), bits 29-31 reserved, and 96 unnamed bits for package-defined classes (bits 32-127). The first 16 package bits (32-47) keep their original positions; decision 0004 raised the count from 16 to 96. A new named bit uses a reserved bit.
- `Value` is `Real(Real) | Str(String) | Inst(InstId) | None`. Reading an unset custom variable returns `Real(0.0)` when the game has "treat uninitialized as 0" (19 games, Confirmed) and is a `SimError` for keep-celebrating-for-xia (Confirmed off). Field: `GameDef::uninit_reads_zero`.
- **Variable ids** (Design, determinism). `VarId(u32)` values for every name known at load (object params, creation-code params, script identifiers the loader can see) are assigned by `crates/format` from the **sorted** name list, so they never depend on run order. Names first seen at run time (scripts) go through `World::var_names: BTreeMap<String, VarId>`, which is hashed and saved with the world. No interner outside `GameDef` and `World`.
- **Storage and ids.** `World.instances: Vec<Instance>` in creation order, so sorted by `InstId`; lookup is a binary search (`World::index_of`), no map. `world.next_id` is hashed state and uses `checked_add` (section 10.3). Our ids never match the original's (trace ids such as 115781); trace diff ignores the `id` column. Persistent instances keep their ids across rooms, so they stay "created earlier" than the new room's instances (Inferred). The GM instance list order is a separate, hashed `room_entry` rank keyed by id (`World::instance_list`): depth largest first, then the placed instances in file order, the carried persistent ones in their previous order, then later creations (research#142).

### Ranks and deterministic iteration

- **Rank.** Every instance carries `rank: u16`. `crates/format` computes it from `object_order` at load, per placed instance, and `GameDef::rank_for(kind, variant)` gives it for spawned instances. `object_order` entries may be selectors (`{ kind = "spike", dir = "up" }`) so collapsed original objects keep their original index order (converter OPEN 13). The loader rejects a package where any kind that can be spawned (including engine kinds: bow, blood, blood_emitter, game_over, playerface, kid_bullet) has no rank (CR4).
- **Dispatch order** (UNKNOWN U2 "and whether depth matters", register R2): one function decides it, and it has one order.

  ```rust
  // events.rs
  pub fn dispatch_order(w: &World, filter: impl Fn(&Instance) -> bool) -> Vec<InstId>;
  ```

  `object_order` rank, then instance id (decision 0005). T-ORD-1 is recorded (`rely-the-needle-4-2/t-ord-1`) and confirms that depth does not matter. The `gm_id`-vs-file order for a placed instance is OPEN (research#277); the walk keys on instance id.
- **Snapshot per phase.** Each phase takes `dispatch_order` once at its start and walks that list, skipping instances that are dead or inactive by the time their turn comes. Whether instances created during the phase join it is R9: they join when their place in the order comes after the creator's object (Confirmed by the recorder, TICK-112 U10).
- **"The first instance"** (R21). Every place that picks one instance out of several (queries that return an id, `World::kid`, `player.x` reads in built-ins, view follow, saveGame's kid) calls `World::first_of`, which sorts candidates by `TickRules::first_rule` (`FirstRule::InstanceList` default: the first in the GM instance list, depth largest first then room entry, research#142; test-only alternatives `DispatchOrder`, the R2 event order, and `LowestId`). Trace `eat-the-lemon/famb-multikid-restart*` (recorded) settles it. The broadphase grid never decides order.
- **Draw order** (Confirmed manual: depth descending, then creation order). Used by the Draw-slot phase ([tick pipeline](tick.md) row 12) and the client.
- **Classes and layers are declared data** (E3 of #1153, decision 0005). A collision class is a set bit whose assignment comes from data: the built-in's default classes, a param pick (`param_classes`), the object's `counts_as`, or a package class name — never an instance's id. A draw layer is the declared `depth` (kind default, room-file instance override, or a runtime `set depth`); `events::draw_order` sorts by depth descending and uses the instance id only to break ties inside one layer, so an id is a runtime handle, not a layer. The data behind all of it (`object_order`, room-file order, `depth`, classes) is package data, so a change is visible in `gameplay_hash`.

### World and context

```rust
pub struct World {
    pub tick: u64,
    pub room: RoomId,
    pub room_tick: u64,
    pub room_speed: Real,                 // GM variable; legacy per-room clock data (R14, R30): file speed on entry, 50 after world Room Start
    pub instances: Vec<Instance>,
    pub next_id: u64,
    pub var_names: BTreeMap<String, VarId>, // run-time names only (variable ids)
    pub globals: BTreeMap<VarId, Value>,
    pub engine: EngineGlobals,            // per-owner sub-structs, see below
    pub rng: Rng,
    pub input: InputState,                // this tick's frame plus the direct-read latch
    pub views: [View; 8],
    pub pending: Option<RoomRequest>,     // one slot, last request wins (Inferred, R13)
    pub persistent_rooms: BTreeMap<RoomId, Vec<Instance>>,
    pub save: Option<SaveSlot>,           // None = "save file missing"
    pub sound: SoundState,                // hashed
    pub output: Vec<OutputEvent>,         // not hashed
    pub status: SimStatus,
}

pub struct Ctx<'a> {
    pub def: &'a GameDef,
    pub rules: &'a RunnerRules,
    pub w: &'a mut World,
}
```

Every built-in, the kid, rooms and scripts work through `&mut Ctx` and `InstId`s. No `Rc<RefCell>` outside the script host. The broadphase grid ([collision](collision.md)) lives in `World` as a cache field marked `#[serde(skip)]`, rebuilt on room entry and on savestate restore, never hashed.

```rust
impl World {
    pub fn get(&self, id: InstId) -> Option<&Instance>;          // alive or dead-this-tick
    pub fn get_mut(&mut self, id: InstId) -> Option<&mut Instance>;
    pub fn exists(&self, id: InstId) -> bool;                    // visible with dead_in_queries false
    pub fn visible(&self, id: InstId, dead_in_queries: bool) -> bool; // the one predicate for a stored id
    pub fn iter_alive(&self) -> impl Iterator<Item = &Instance>; // creation order, alive
    pub fn iter_visible(&self) -> impl Iterator<Item = &Instance>; // the predicate
    pub fn first_of(&self, class: ClassMask, rules: &TickRules) -> Option<InstId>;  // R21
    pub fn count_of(&self, class: ClassMask) -> u32;
    pub fn kid(&self, rules: &TickRules) -> Option<InstId>;      // first_of(KID | KID_FLIPPED)
}
impl Ctx<'_> {
    pub fn spawn(&mut self, kind: KindId, x: Real, y: Real, init: SpawnInit) -> Result<InstId, SimError>; // runs Create now (Inferred)
    pub fn spawn_engine(&mut self, k: EngineKind, x: Real, y: Real, init: SpawnInit) -> Result<Option<InstId>, SimError>;
    pub fn destroy(&mut self, id: InstId) -> Flow;                                    // runs Destroy now
    pub fn change_kind(&mut self, id: InstId, kind: KindId, run_events: bool) -> Flow; // instance_change
    pub fn request_room(&mut self, req: RoomRequest);                                 // deferred (Confirmed)
    pub fn emit(&mut self, ev: OutputEvent);
}
pub struct SpawnInit { pub params: Params, pub variant: Option<VariantSel>, pub run_create: bool }
pub enum Flow { Continue, AbortTick }   // AbortTick after a room request, per R8

// def/game.rs: engine kinds the sim spawns by role, so built-ins never hard-code kind ids
pub enum EngineKind { World, Kid, KidFlipped, KidBullet, Bow, Blood, BloodEmitter, GameOver, Playerface }
impl GameDef { pub fn engine_kind(&self, k: EngineKind) -> Option<KindId>; }
```

### Per-owner state

Each shared container is a struct or enum of per-owner parts. Each part's type lives in its owner's module, starts empty (`#[derive(Default)]`), and only its owner adds fields.

```rust
// world/instance.rs
pub enum KindState { None, Kid(Box<kid::KidState>), Builtin(builtins::BuiltinState), Script(script::ScriptState), Probe(events::ProbeState) }
// world/mod.rs
pub struct EngineGlobals {
    pub run: run_globals::RunGlobals, // difficulty, deaths, time, save slot number, clear flag
    pub kid: kid::KidGlobals,         // grav flag, frozen, reverse, game-side key flags
    pub mech: builtins::MechGlobals,  // trigger models, boss/item flags
}
// output.rs
pub enum OutputEvent { Room(room::RoomOutput), Kid(kid::KidOutput), Builtin(builtins::BuiltinOutput), Sound(sound::SoundOutput), Script(script::ScriptOutput) }
```

`builtins::BuiltinState` is an enum in `builtins/state.rs`; a new built-in adds a variant there and in its own `builtins/<kind>.rs` file.

### Kid access

Other built-ins never touch `KidState` fields directly. They call these functions in `kid/api.rs`:

```rust
pub fn facing(w: &World, kid: InstId) -> i8;
pub fn refresh_air_jump(ctx: &mut Ctx, kid: InstId);              // water, refreshers, platforms
pub fn set_on_platform(ctx: &mut Ctx, kid: InstId, on: bool);
pub fn apply_param_zone(ctx: &mut Ctx, kid: InstId, z: &ParamZone);
pub fn bow_snapshot(w: &World, kid: InstId) -> BowSnapshot;      // x, y, xscale, yscale, alpha
pub fn kill(ctx: &mut Ctx, kid: InstId, cause: KillCause) -> Flow;
pub fn read_field(w: &World, kid: InstId, f: KidField) -> Option<Value>; // probe registry
```

A kid script slot (decision [0004](../../decisions/0004-extension-model.md#kid-script-slots)) runs a function of the current room profile's `[script] file` at a fixed position in an event list, through [`kid::run_action`]/[`kid::run_collision`] and the same script host as any other script, with `this` bound to the kid's `Inst`. The fields a script may read and write are the decision's table: the generic instance fields plus the kid-only set in `KidState` (`djump`, `curJumps`, `tjump`, `onPlatform`, `ladder`, `frozen`, `frozen2`, `muteki`, `tempHspeed`, `maxSpeed`, `maxVspeed`, `maxFallSpeed`, `scale`), with `ladderud`, `ladderjump`, `yflag`, `flipped`, `facing`, `jump`/`jump2`/`jump3`, `on_slope_last`, `walk_h` and `onVineL`/`onVineR` read only. `facing` follows a write to `image_xscale`; `yflag` and `flipped` stay behind the gravity knobs so the mask and gravity cannot disagree. The kid's own actions still own the per-tick physics: a slot composes with them and does not replace them.
