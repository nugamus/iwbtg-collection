# Plan: simulation core for the 20 prototype games

Status: plan, revision 2, 2026-10-24. Written by Claude for parallel implementation by agents. Nothing here is code yet. Revision 2 applies the clean-room, determinism and coverage reviews: each critique was re-checked against the spec text before it went in (section 17 lists what changed).

Goal: `crates/sim` runs any of the 20 prototype games (`research/shortlist.md`) from a v0 package and reproduces the research traces tick for tick, bit for bit. `crates/format` turns a package into a `GameDef`. `crates/tools` runs a package against a `.inputs` file and diffs the result against a recorded CSV, and runs replays headless.

Related: [architecture](../architecture.md), [format v0](../format/v0.md), [format principles](../format/README.md), [0001 script language](../decisions/0001-script-language.md), [0002 data syntax](../decisions/0002-data-syntax.md), [roadmap](../roadmap.md) Phase 3.

## Sources and rules for every agent working from this plan

- GameMaker behavior comes **only** from `research/specs/`: `engines/gm8/CLONE_SPEC.md` (sections "tick loop", "motion", "collision", "numbers", "rooms", "sprites and animation", "input", "sound"), `engines/gm8/kid/{A1,A2,A3,A4,A5,B,C}.md`, `mechanics.md`, `tricks.md`, and the CSV traces under `engines/gm8/traces/`. Never read OpenGMK, Jtool or renex source, or anything under `research/extracted/`, while designing or writing client code (client `CLAUDE.md`).
- Specs and traces change while traces are recorded. Each item names its spec section so an agent can re-read the current text. When the spec changes a rule, the code changes in the one place the [register](#15-unknown-and-inferred-register) names for it. A default changes only after the spec records a trace result; agents may read a recorded CSV to check their code, but they do not turn their own reading of a probe trace into a rule (that is spec work for research).
- **OpenGMK-derived number rules.** The existing `crates/sim/src/num/` was written before the clean-room rule. `INVESTIGATION.md` (last section) names these rules as taken from OpenGMK source: the 1e-13 epsilon compare, truthiness `>= 0.5`, `sign` with epsilon, `mod` as a − trunc(a/b)·b, floored `div`, round −0 → +0, and the `max` tie rules. The doc comments in `num/round.rs` also cite the recorder for `floor`/`ceil` signed 

eros and `min`/`max`, and `num/trig.rs` and `num/tests.rs` claim the recorder uses the UCRT for `sin`/`cos` (no spec says so). Each of these is **OPEN** behind a `NumRules` field (register N1, N2, N4, N11-N13) except `div`, which the spec Confirms truncates toward zero, so the code changes (N8). WP2 audits every `num` doc comment that cites OpenGMK or the UCRT and replaces it with an N-id.
- `sim` stays pure: no Bevy, no IO, no clock, no threads, no `HashMap`/`HashSet`, no `f32`, no platform float math, no `std::hash` hashers in state hashing. Enforced by `crates/sim/clippy.toml` and lint attributes (section 10.5), not by convention. `usize` never enters hashed or serialized state.
- Labels: **Confirmed** (spec Confirmed section or manual), **Inferred** (spec Inferred section), **Trace** (seen in a recorded CSV; the recorder is OpenGMK, so still not proof for the real runner), **UNKNOWN** (spec UNKNOWN; the code isolates it), **Design** (our own choice, no GM behavior claimed), **Conflict** (two spec passages disagree; reported to research, code isolates both readings).

## 0. Overview

### What already exists

| Module | State |
|---|---|
| `sim::num` | `Real`, banker's `round`, conversions, `gm_eq`..`gm_ge`, `gm_bool`, libm trig, exact-bits table for 0/90/180/270 degrees, `point_direction`, `lengthdir_*`. Compare, truth, sign, min/max and div rules are const or hard-coded (OpenGMK provenance, see above). |
| `sim::input` | `Action` (12 actions, no Pause/Mute/Reset), `InputFrame { held, pressed, released }`, `.inputs` parser for key and action files, `KeyMap`. 897 lines. |
| `sim::replay` | `.iwbr` encode/decode, canonical. |
| `sim::trace` | `TraceTable` parse/write of the recorder CSV, bit-exact `diff`. `std::fs` only under `#[cfg(test)]`. |
| `format` | `PackageFiles`, manifest parse and validation, diagnostics with line and column. |
| `tools` | `iwbtg` binary stub. |
| research converter | `research/build/packages/<id>/` for all 20 games: `game.toml`, `physics.toml` (draft), `rooms/`, `objects/`, `sprites/`, `tests/trace-map.toml`. Gaps that block M1 are in section 8.4. |
| traces | About 250 CSVs in 17 game folders (keep-celebrating-for-xia has none yet). Inventory and how each kind is used: section 9.1. |

### Work packages at a glance

| WP | Name | Wave | Depends on |
|---|---|---|---|
| WP0 | Contracts, scaffold, purity config | 0 | none |
| WP1 | World, events, tick pipeline, state hash | 1 | WP0 |
| WP2 | Numbers, RNG, motion, paths | 1 | WP0 |
| WP3 | Collision | 1 | WP0 |
| WP4a | Room entry and room change sequence | 1 | WP0 |
| WP7a | Loader: sprites, masks, rooms, objects, order | 1 | WP0 |
| WP8a | Trace runner, probe, setup, CLI | 1 | WP0 |
| WP4b | Restart models, saves, views | 2 | WP4a |
| WP5a | Kid core and family A1 | 2 | WP2, WP3 (WP4b for first-play save) |
| WP6a | Built-ins tier 1 | 2 | WP2, WP3, WP4b |
| WP9 | Script host (minimal, IWS profile) | 2 | WP1 |
| WP10 | Sound model and DLL adapters | 2 | WP1 |
| WP7b | Loader: physics, params, timelines, paths, schemas | 3 | WP5a, WP6a, WP10 |
| WP8b | Replay runner and timing triggers | 3 | WP8a, WP4b |
| WP11 | Integration and milestones | 3, then ongoing | all merged WPs |
| WP5b | Kid family B | 3 | WP5a |
| WP5c | Kid families A2-A5, C; slopes, flips, ladders, ice | 4 | WP5a |
| WP6b | Built-ins tier 2 | 4 | WP6a |
| WP6c | Built-ins tier 3 | 5 | WP6b, WP9 |

Merge order: WP0 alone. Then each wave in any order inside the wave. A WP can start the day WP0 merges, because WP0 freezes every cross-WP interface; later WPs code against stubs and turn on their integration tests when the stubbed WP merges. Milestone checks are not part of any WP's done-criteria except WP11, so no WP waits on another to be "done".

### Milestones

Each is a set of trace cases that must diff clean. WP11 owns them.

- **M1 rely stand.** `rely-the-needle-4-2/stand`, `rely-the-needle-4-2/djump` (both `--room room1`). Needs WP0-WP4b, WP5a, WP6a (rely slice, section 6.2), WP7a, WP7b, WP8a, and converter requests CR1, CR3, CR4 (section 8.4).
- **M1b rely right.** `rely-the-needle-4-2/right` (`--room rSelectStage --x 96 --y 567.4`). Adds CR2 (`warp_start`) or the WP7a inert-kind trace mode.
- **M2 kaenbyou basics.** `kaenbyou/{stand, t-speed-1, famb-spawn-settle, right, djump, t-mot-06, t-anim-1, t-alarm-1}`. Adds WP5b, WP10 (DLL stub mode, the traces use `--stub-dlls`), the kid Room Start (bow, view angle) and B End Step `slope_b`.
- **M3a rely traces.** Every diff-target trace of rely-the-needle-4-2 (section 9.1), plus CR5 (warp params).
- **M3b rely full clear.** A full-clear `.inputs` replayed by `iwbtg replay run` reaches the end trigger. Recording that replay needs the Phase 4 Bevy client and input recording, so M3b waits for Phase 4; this plan only delivers the headless runner (WP8b).

## 1. World model and instance storage

Spec: CLONE_SPEC "tick loop" (Creation and destruction in the middle of a step; Instance order within one event), "rooms" (Instance creation and destruction, Draw order). Architecture "World model".

### 1.1 Instances

```rust
// crates/sim/src/world/instance.rs  (WP0 declares, WP1 owns)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
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
    pub m: Motion,                    // WP2
    pub img: Image,                   // sprite, mask, image_index, image_speed, xscale, yscale, angle, alpha
    pub alarms: [Real; 12],           // <= 0 is idle; default -1 (Inferred)
    pub timeline: Option<TimelineRun>,
    pub path: Option<PathRun>,
    pub state: KindState,             // per-owner state, see 1.5
    pub vars: BTreeMap<VarId, Value>, // custom variables (scripts, creation-code params)
}
```

- `ClassMask(u64)` replaces GM parent objects. A collision event or query "with object X" becomes "with class X". Each built-in declares default classes; a package object may add classes (`counts_as`, section 8.3). Examples: `platform` has `PLATFORM | BLOCK` in 17 games and `PLATFORM | SOLIDBLOCK` in eat-the-rori-girl and help-the-fairy-tale (CLONE_SPEC collision Inferred "Platforms run the kid's block event"). Spikes and cherries have `KILLER`. This keeps "a collision event with a parent also fires for its children" (Confirmed, manual 205_00) without a parent tree in the sim.
- Class bits: WP0 fixes the whole list in `def/class.rs` (no later appends): `KID, KID_FLIPPED, KID_BULLET, BLOCK, SOLIDBLOCK, PLATFORM, KILLER, WATER1, WATER2, WATER3, VINE_L, VINE_R, ROOM_CHANGER, EDGE_WARP, SAVE, SLIP, SLOPE, SLOPE_WALL, LADDER, FAKE_BLOCK, BULLET_BLOCKER, TRIGGER, TRAP_STOPPER, MOVING_BLOCK, CONVEYOR, PARAM_ZONE, BOW, BLOOD`, plus 16 unnamed bits for package-defined classes. A WP that needs a new named bit asks the integrator (WP11), who uses a reserved bit.
- `Value` is `Real(Real) | Str(String) | Inst(InstId) | None`. Reading an unset custom variable returns `Real(0.0)` when the game has "treat uninitialized as 0" (19 games, Confirmed) and is a `SimError` for keep-celebrating-for-xia (Confirmed off). Field: `GameDef::uninit_reads_zero`.
- **Variable ids** (Design, determinism). `VarId(u32)` values for every name known at load (object params, creation-code params, script identifiers the loader can see) are assigned by `crates/format` from the **sorted** name list, so they never depend on run order. Names first seen at run time (scripts) go through `World::var_names: BTreeMap<String, VarId>`, which is hashed and saved with the world. No interner outside `GameDef` and `World`.

### 1.2 Storage and ids

- `World.instances: Vec<Instance>` in creation order, so sorted by `InstId`. Lookup is a binary search (`World::index_of`). No map.
- Creation appends. Destroy runs the Destroy event at once, then sets `alive = false`; the instance runs no further events and `instance_exists` no longer counts it (Inferred, CLONE_SPEC rooms "After instance_destroy, instance_exists stops counting"). `instance_exists` and `object.variable` skip an instance destroyed earlier in the step (Confirmed (recorder), showdown `a3-t18-b.csv` row 42). Whether collision events and the query functions still see it is not in the spec yet (trace request T-DEAD-1, research#7): register R22, switch `TickRules::dead_in_queries` (default false, Design). The sweep at the end of the tick removes it with `retain`, which keeps order (Confirmed: destroyed instances stay in the instance list until the end of the step).
- Persistent instances keep their ids across rooms, so they stay "created earlier" than the new room's instances (Inferred, rooms "Persistent instances count as created before").
- `world.next_id` is hashed state and uses `checked_add` (section 10.3). Our ids never match the original's (trace ids such as 115781); trace diff ignores the `id` column (section 9).

### 1.3 Ranks and deterministic iteration

- **Rank.** Every instance carries `rank: u16`. `crates/format` computes it from `object_order` at load, per placed instance, and `GameDef::rank_for(kind, variant)` gives it for spawned instances. `object_order` entries may be selectors (`{ kind = "spike", dir = "up" }`) so collapsed original objects keep their original index order (converter OPEN 13). The loader rejects a package where any kind that can be spawned (including engine kinds: bow, blood, blood_emitter, game_over, playerface, kid_bullet) has no rank (CR4).
- **Dispatch order** (UNKNOWN U2 "and whether depth matters", register R2): one function decides it.

  ```rust
  // crates/sim/src/events.rs (WP1)
  pub fn dispatch_order(w: &World, rules: &TickRules, filter: impl Fn(&Instance) -> bool) -> Vec<InstId>;
  pub enum DispatchOrder { RankThenCreation, Creation, DepthThenRankThenCreation }
  ```

  Default `RankThenCreation` (Inferred: object index order, then creation order). Alternatives `Creation` and `DepthThenRankThenCreation` (the spec lists depth as UNKNOWN, and T-ORD-1 gives two blocks different depths on purpose). T-ORD-1 is recorded (`rely-the-needle-4-2/t-ord-1`); the default changes when the spec records its result.
- **Snapshot per phase.** Each phase takes `dispatch_order` once at its start and walks that list, skipping instances that are dead or inactive by the time their turn comes. Whether instances created during the phase join it is R9 (`TickRules::new_instances_join_phase`): default `LaterInOrder`, they join when their place in the order comes after the creator's object (Confirmed by the recorder, CLONE_SPEC U10).
- **"The first instance"** (R21). The manual says only "the first is returned" (instance_position) and "only the first is followed" (views) without defining "first". Every place that picks one instance out of several (queries that return an id, `World::kid`, `player.x` reads in built-ins, view follow, saveGame's kid) calls `World::first_of`, which sorts candidates by `TickRules::first_rule` (`DispatchOrder` default, alternative `LowestId`). Trace `eat-the-lemon/famb-multikid-restart*` (recorded) settles it. The broadphase grid never decides order.
- **Draw order** (Confirmed manual: depth descending, then creation order). Used by the Draw-slot phase (2.1 row 12) and the client.

### 1.4 World and context

```rust
// crates/sim/src/world/mod.rs (WP0 declares the struct, WP1 owns)
pub struct World {
    pub tick: u64,
    pub room: RoomId,
    pub room_tick: u64,
    pub room_speed: Real,                 // GM variable; file speed on entry, 50 after world Room Start
    pub instances: Vec<Instance>,
    pub next_id: u64,
    pub var_names: BTreeMap<String, VarId>, // run-time names only (1.1)
    pub globals: BTreeMap<VarId, Value>,
    pub engine: EngineGlobals,            // per-owner sub-structs, 1.5
    pub rng: Rng,                         // WP2
    pub input: InputState,                // this tick's frame plus the direct-read latch (WP1)
    pub views: [View; 8],                 // WP4b
    pub pending: Option<RoomRequest>,     // one slot, last request wins (Inferred, R13)
    pub persistent_rooms: BTreeMap<RoomId, Vec<Instance>>,  // WP4a
    pub save: Option<SaveSlot>,           // WP4b; None = "save file missing"
    pub sound: SoundState,                // WP10, hashed (section 7)
    pub output: Vec<OutputEvent>,         // not hashed
    pub status: SimStatus,
}

pub struct Ctx<'a> {
    pub def: &'a GameDef,
    pub rules: &'a RunnerRules,
    pub w: &'a mut World,
}
```

Every built-in, the kid, rooms and scripts work through `&mut Ctx` and `InstId`s. No `Rc<RefCell>` outside the script host. The broadphase grid (section 3.4) lives in `World` as a cache field marked `#[serde(skip)]`, rebuilt on room entry and on savestate restore, never hashed.

```rust
// world/api.rs (WP0 signatures, WP1 implementation; WP0 ships a working minimal version for tests)
impl World {
    pub fn get(&self, id: InstId) -> Option<&Instance>;          // alive or dead-this-tick
    pub fn get_mut(&mut self, id: InstId) -> Option<&mut Instance>;
    pub fn exists(&self, id: InstId) -> bool;                    // alive and active
    pub fn iter_alive(&self) -> impl Iterator<Item = &Instance>; // creation order
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

// def/game.rs (WP0): engine kinds the sim spawns by role, so built-ins never hard-code kind ids
pub enum EngineKind { World, Kid, KidFlipped, KidBullet, Bow, Blood, BloodEmitter, GameOver, Playerface }
impl GameDef { pub fn engine_kind(&self, k: EngineKind) -> Option<KindId>; }
```

### 1.5 Per-owner state (prevents file collisions)

WP0 declares each shared container as a struct or enum of per-owner parts. Each part's type lives in its owner's file, starts empty (`#[derive(Default)]`), and only its owner adds fields.

```rust
// world/instance.rs (WP0)
pub enum KindState { None, Kid(Box<kid::KidState>), Builtin(builtins::BuiltinState), Script(script::ScriptState), Probe(events::ProbeState) }
// world/mod.rs (WP0)
pub struct EngineGlobals {
    pub run: run_globals::RunGlobals, // WP4b: difficulty, deaths, time, save slot number, clear flag
    pub kid: kid::KidGlobals,         // WP5a: grav flag, frozen, reverse, game-side key flags
    pub mech: builtins::MechGlobals,  // WP6a: trigger models, boss/item flags
}
// output.rs (WP0)
pub enum OutputEvent { Room(room::RoomOutput), Kid(kid::KidOutput), Builtin(builtins::BuiltinOutput), Sound(sound::SoundOutput), Script(script::ScriptOutput) }
```

`builtins::BuiltinState` is an enum in `builtins/state.rs` (WP6a owns; WP6b and WP6c add variants in their own `builtins/<kind>.rs` files and one line each in `state.rs`, merged in wave order).

### 1.6 Kid access for other WPs

WP6 built-ins never touch `KidState` fields directly. They call these functions (WP0 signatures, WP5a bodies) in `kid/api.rs`:

```rust
pub fn facing(w: &World, kid: InstId) -> i8;
pub fn refresh_air_jump(ctx: &mut Ctx, kid: InstId);              // water, refreshers, platforms
pub fn set_on_platform(ctx: &mut Ctx, kid: InstId, on: bool);
pub fn apply_param_zone(ctx: &mut Ctx, kid: InstId, z: &ParamZone);
pub fn bow_snapshot(w: &World, kid: InstId) -> BowSnapshot;      // x, y, xscale, yscale, alpha
pub fn kill(ctx: &mut Ctx, kid: InstId, cause: KillCause) -> Flow;
pub fn read_field(w: &World, kid: InstId, f: KidField) -> Option<Value>; // probe registry (9.3)
```

## 2. Tick pipeline

Spec: CLONE_SPEC "tick loop" (Confirmed order, Inferred per-tick algorithm, U1-U12), "motion", "sprites and animation" (Inferred placement of the animation update), "input". Architecture "Tick pipeline" is replaced by this section (WP0 updates `architecture.md`).

### 2.1 Order

`Sim::step(InputFrame) -> StepResult` runs `tick::run_tick`. Each phase is one function in `tick.rs` (WP1). Phase placement that the spec marks Inferred or UNKNOWN is a field of `TickRules`, never an `if` scattered in built-ins.

| # | Phase | Status | Isolation |
|---|---|---|---|
| 0 | Latch input: build held/pressed/released for the tick; `direct_latch |= pressed` per key | Inferred (sample once before Begin Step; same-tick reaction is Trace: djump/right row 10) | `InputState::latch` |
| 1 | Capture `xprevious/yprevious` if `prev_capture = TickStart` (default) | Confirmed (recorder) U1 (R1) | `TickRules::prev_capture` |
| 2 | Begin Step | Confirmed order | |
| 3 | Timelines advance, moments run | Slot Confirmed (recorder) U6 (R6); moments for speed > 1 UNKNOWN | `TickRules::timeline_slot = AfterBeginStep` |
| 4 | Alarms: for each alarm the object handles, a value > 0 loses 1 and fires when that leaves it at or below 0; a value of exactly 0 becomes -1 without firing | Confirmed order; rule Confirmed (recorder) (U3, R3) | `alarm::tick(&mut Real) -> bool` |
| 5 | Keyboard events: held, then press, then release (per instance; keys in action-bit order) | Confirmed slot; inner order UNKNOWN U11 (R4) | `events::key_event_order()` |
| 6 | Mouse events | Confirmed slot; no gameplay use in the 20 games | no-op phase kept for order; takes no client value of any kind (no `f32`, no cursor position) |
| 7 | Step | Confirmed | |
| 8 | Motion for every instance: (capture prev if `BeforeMotion`), friction, gravity vector, path, `x += hspeed`, `y += vspeed`, polar recompute | Slot Confirmed; inner order Inferred (U5 tick, U2 motion) | `motion::apply_auto_motion` (WP2) |
| 8b | Animation if `anim_slot = AfterMotion` | Alternative; the recorder puts it at row 13 (R7) | `anim::advance` |
| 9 | Outside Room, Intersect Boundary | Confirmed (recorder) slot (U4, R5): after motion and End of Path, Outside Room first | `TickRules::boundary_slot = AfterMotion`; alternative `AfterCollision` |
| 10 | Collision events with the solid put-back rule | Confirmed (manual); pairing Inferred/UNKNOWN (C7, C7b, C8, C9, R12) | `events::collision_phase` + `collide::solid_resolve` |
| 11 | End Step | Confirmed | |
| 12 | Draw slot (`Late` handlers): instances in **draw order** (depth descending, then creation; 1.3), **visible instances only** | Confirmed (manual: Draw runs only for visible instances; draw order by depth); Draw changes state, so it runs headless too (R15) | `events::run_draw_phase` |
| 13 | Animation if `anim_slot = AfterDraw` (default): `image_index += image_speed`, wrap, Animation End | Confirmed (recorder) (R7): slot, wrap `>=` and subtract once | `anim::advance(&mut Image, frames, &TickRules) -> bool` |
| 14 | Destroy sweep | Design (manual: removed at end of step) | `World::sweep` |
| 15 | Room change if requested (section 4.2) | Confirmed sequence; abort rule Confirmed (recorder) (R8) | `room::perform_change` (WP4a) |
| 16 | `tick += 1` (checked), timing triggers (WP8b), checkpoint hash every 500 ticks | Design | `tick.rs`, `timing.rs`, `hash.rs` |

The recorder samples after phase 13 and before phase 15 ("after End Step, drawing and the animation update", CLONE_SPEC tick Inferred 13). That README wording is also the only support for row 13, and the sprites section's own Inferred text puts the update in row 8b. t-anim-1 samples at end of tick, so it cannot tell the two apart; ANIM-SAVE-END decides (R7). `Sim` exposes the sample point to the probe (section 9).

### 2.2 Rules inside the pipeline

- **Room change requests.** `room_goto`, `room_restart`, `game_restart` and `game_end` set `World.pending` and return `Flow::AbortTick` to the dispatcher after the requesting handler finishes (Confirmed: the rest of the action runs; Inferred: no further events this tick, R8). The handler's own remaining code still runs, which the engines rely on (warp sets `player.x/y` after `room_goto`, Confirmed). The pipeline then jumps to phases 14 and 15. Alternative `TickRules::room_request = AbortTick | FinishPhase | FinishTick` (T-ROOM-1 recorded; TR-ROOM-02 not yet).
- **Deferral grain** (rooms UNKNOWN 1, R25). The manual says the change happens "after the current action". The grain is the code action: the rest of the requesting code action runs and later actions of the same event do not (Confirmed (recorder), `tr-room-02-c.csv` row 13). A D&D Go to Room followed by more actions is untraced. Our handlers are one unit each, so we apply the request at the end of the handler (Design); a ported event whose actions follow a room-change code action must be written so that order does not matter, or flagged.
- **Instances created mid-phase** take part in every later phase of the same tick (Inferred; Trace t-alarm-1: the bullet created in the kid's Step moved 16 px on its creation tick, x 98 to 114). Whether they run the current phase: R9.
- **Create runs inside `spawn`** before it returns (Inferred). Room Start for instances created during Room Start: R10, default no (snapshot).
- **Alarm assignment of fractional values** (1.3, 1.5, kaenbyou, jun-g, little-runner): stored through `num::alarm_from_real` (R3b). Round half to even (Confirmed (recorder): `kaenbyou/num-alarm-half-b`, `go-the-jun-g-world/num-eps-alpha`).
- **Alarm only ticks when the object handles that alarm** (Confirmed manual). Each kind declares `EventSet::alarms: u16`.
- **Inactive instances** get no events, are not drawn and are ignored by queries (Confirmed manual 403_06).
- **Direct key reads** (Inferred input: one latch bit per key, shared by every direct read, consumed by each read). Every built-in and kid action that ports a `keyboard_check_direct` calls `InputState::direct(&mut self, Action) -> bool`; nothing else reads or clears the latch. Ladder code that reads Up twice in one Step (little-trap, rainbow, rori, fairy-tale) makes two calls. The world built-in's dummy reads (kaenbyou, eat-the-lemon, xia) are `direct` calls from the world's Step, enabled by its `latch_cleared_by_world` param (WP6a), not a kid field.
- **Real-time variables** (R28, Design). GML reads of `fps`, `current_time`, `time_span`-style clocks (kaenbyou and eat-the-lemon world Step cheat check, numbers "round, frac and real") return fixed values derived from ticks: `fps = tick_rate`, clocks = `tick * 1000 / tick_rate` ms. Documented per built-in that reads them.
- **GM 8.1** (wanna, go-the-spook-house, keep-celebrating-for-xia): `RunnerRules::for_runner(Runner::Gm81, rev)` returns the same rules as `Gm80` until T-EVO-2 shows a difference (U12, R11). Any difference becomes a field, never a branch on `runner` inside built-ins.
- **Halting.** A script error, a non-finite value (section 10.2), an overflow (10.3) or an invariant break sets `SimStatus::Halted(SimError)` and freezes the world. The sim never panics on any input (wasm32-unknown-unknown builds abort on panic, section 10.6).

### 2.3 Events and dispatch

```rust
// crates/sim/src/events.rs (WP0 declares the enums, WP1 owns dispatch)
pub enum EventKind {
    Create, Destroy, BeginStep, Alarm(u8), KeyHeld(Action), KeyPress(Action), KeyRelease(Action),
    Step, OutsideRoom, IntersectBoundary, Collision { with: ClassMask, other: InstId }, EndStep,
    Late, AnimationEnd, RoomStart, RoomEnd, GameStart, User(u8), Moment(u32), EndOfPath,
}
pub struct EventSet {           // what a kind handles; built once per kind at load
    pub begin_step: bool, pub step: bool, pub end_step: bool, pub late: bool,
    pub alarms: u16, pub key_held: u16, pub key_press: u16, pub key_release: u16,
    pub collisions: SmallVec<[ClassMask; 4]>,   // handler classes, in handler order
    pub outside_room: bool, pub intersect_boundary: bool, pub animation_end: bool,
    pub room_start: bool, pub room_end: bool, pub user: u16,
}
pub fn event_set(def: &GameDef, kind: KindId) -> &EventSet;            // cached in GameDef
pub fn dispatch(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;    // match kind.builtin -> handler
pub fn run_phase(ctx: &mut Ctx, phase: Phase) -> Flow;                 // snapshot + dispatch
pub fn run_draw_phase(ctx: &mut Ctx) -> Flow;                          // draw order, visible only
pub fn collision_phase(ctx: &mut Ctx) -> Flow;
```

`dispatch` matches on `Builtin`: `Builtin::Kid => kid::on_event`, other built-ins `=> builtins::on_event`, and any kind with a script `=> script::on_event` after the base handler (WP9). Keys are actions: the GM "Key Q pressed" kid event is `KeyPress(Suicide)`, "Key R pressed" on world is `KeyPress(Restart)`, P is `KeyPress(Pause)` (CLONE_SPEC input, Inferred mapping; Pause, Mute and Reset are new actions, section 2.4).

**Collision phase** (Confirmed rule; pairing Inferred or UNKNOWN; all in `events::collision_phase`):

1. For each instance `a` in dispatch order, build the list of pairs `(a, b, handler class c)`: every alive, active `b != a` overlapping `a` whose classes intersect a handler class `c` of `a`.
2. Sort the pairs by `CollideRules::pair_order` (R12). Default `OtherRankThenId`: by `b`'s rank, then `b`'s id (Inferred, collision "Order of collision events": object order, then creation order among the others). Alternative `HandlerListOrder`: by `a`'s handler list, then `b` in dispatch order.
3. One `b` matching two handler classes of `a` (a platform is `PLATFORM | BLOCK`, and the kid handles both) gives two events. Both run (C8 default, UNKNOWN), in `a`'s handler-list order for that `b` (Design, R12b). T-COL-06 decides (`help-the-fairy-tale/t-col-06-*` recorded).
4. If `b.solid`: before the pair, re-check that `a` at its current position still overlaps `b`; skip it if not (Inferred, "Overlap is re-checked before each pair's event", C7). Then `collide::solid_resolve`: put `a` at `(xprevious, yprevious)`, run the event, then `x += hspeed; y += vspeed` from wherever the event left it; if `a` overlaps `b` again (or any solid, C9 switch), put it back where the event left it (Inferred "What previous place and new position mean").
5. If `b` is not solid: run the event at the current position, no put-back (Confirmed manual). Whether the pair is re-checked first (does a killer see the kid before or after a block event moved him) is UNKNOWN (collision UNKNOWN, 5th bullet): switch `CollideRules::recheck_nonsolid` (C7b), default `true` (Design: one rule for both cases), settled by T-COL-06.
6. Skip a pair whose `b` died earlier in the phase (R22 decides whether a dead `b` still counts in queries; for event pairs a dead `b` is always skipped, Inferred: it runs no further events and is invisible to `instance_exists`).
7. `b`'s own collision handlers fire when the loop reaches `b` (both instances get their own event, Confirmed).

### 2.4 Input actions

`sim::input::Action` gains three actions (WP0): `Pause` (P: kaenbyou `jump.blocked_by_pause_press`, B and xia pause, showdown keyboard_wait), `Mute` (Ctrl+M: sets the game's music flag, which changes `sound_stop_all` calls and so sim state, section 7) and `Reset` (F2: game reset; Groups A, B, C delete temp and game_restart, kaenbyou/lemon gameReset). They take bits 12-14. Escape and Alt+F4 stay client-only (quit), never sim input. `.inputs` and `.iwbr` accept the new bits; WP0 checks `replay.rs` and the `.inputs` parser round-trip them.

## 3. Motion, numbers and collision

### 3.1 Motion (WP2)

Spec: CLONE_SPEC "motion" (Confirmed variables and conventions; Inferred conversion, gravity vector, friction, order; U1-U11).

```rust
// crates/sim/src/motion.rs
pub struct Motion { /* fields private; read accessors for each GM variable */ }
impl Motion {
    pub fn new_at(x: Real, y: Real) -> Motion;      // speed 0, direction 0, gravity 0, gravity_direction 270 (Inferred defaults)
    pub fn x(&self) -> Real;  pub fn set_x(&mut self, v: Real) -> Result<(), SimError>;   // also y, xstart...
    pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError>;  // recompute speed and direction
    pub fn set_vspeed(&mut self, v: Real) -> Result<(), SimError>;
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError>;   // recompute hspeed and vspeed
    pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError>;
    pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
    pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
    pub fn set_gravity(&mut self, g: Real) -> Result<(), SimError>;
    pub fn set_gravity_direction(&mut self, d: Real) -> Result<(), SimError>;
    pub fn set_friction(&mut self, f: Real) -> Result<(), SimError>;
    pub fn capture_previous(&mut self);
}
pub fn apply_auto_motion(m: &mut Motion, path: Option<&mut PathRun>, def: &GameDef, rules: &MotionRules) -> Result<MotionEvents, SimError>;
```

Every setter rejects a non-finite value with `SimError::NonFinite { what }` (section 10.2).

- **Coupling** (Inferred formulas, Trace-backed bits): setting h or v recomputes `speed = sqrt(h*h + v*v)` and `direction = radtodeg(atan2(-v, h))`, plus 360 when negative. Trace checks: hspeed `-7.347880794884119e-17`, vspeed `0` gives direction `180.0` and speed `7.347880794884119e-17` (`kaenbyou/stand.csv` rows 3-9); hspeed 16, vspeed 0 gives direction `-0.0` (`kaenbyou/t-alarm-1.csv`). So the negative-zero direction is kept and only values `< 0` wrap.
- Setting speed or direction recomputes `h = speed * dcos(direction)`, `v = -speed * dsin(direction)` (Inferred). Speed and direction are stored as written, so a negative speed survives a later direction change (U4, M3) and the direction survives speed 0 (U5, M4). `Motion::recompute_polar` and `recompute_cartesian` are the only places to change after T-MOT-05/06 (`eat-the-rori-girl/t-mot-05-*`, `kaenbyou/t-mot-06*` recorded).
- **Auto motion** (one function, Inferred order, M1): friction, then gravity as a vector (`h += g*dcos(gd)`, `v -= g*dsin(gd)`), then path, then `x += h`, `y += v`, no rounding. The gravity residue `-7.347880794884119e-17` comes out of `dcos(270)` (Trace). Gravity written in Step is used the same tick (Inferred).
- **Paths** (`path.rs`): `PathDef { points: Vec<(Real, Real, Real)>, closed, smooth, precision }`, `PathRun { path, position, speed, end_action, absolute, scale, xstart, ystart }`. `path::advance` is the only path code; its rule is UNKNOWN (U9, U-MECH-4). Tier 2.

### 3.2 Numbers (WP2)

Spec: CLONE_SPEC "numbers" (Confirmed operators, if/repeat rounding, div/mod; Inferred binary64, banker's round, exact compare, truthiness, RNG; UNKNOWN list); INVESTIGATION last section.

Every rule with OpenGMK provenance or an open spec status is a field of `NumRules` in `RunnerRules` (`rules/num.rs`), never a `const`, so a rule settled later becomes a new `engine_rev` while replays verified under the old rule still verify (architecture "Versions").

```rust
// crates/sim/src/rules/num.rs (WP0 declares, WP2 owns)
pub struct NumRules {
    pub compare: CompareMode,        // N1: Exact | Epsilon(Real)
    pub truth: Truth,                // N2: GtHalf | GeHalf
    pub sign: SignMode,              // N11: Exact | Epsilon (same epsilon as compare)
    pub min_max_ties: MinMaxTies,    // N12: which operand wins on equal values and on -0/+0
    pub round_neg_zero: NegZero,     // N4: round(-0.4) gives +0 or -0
    pub floor_ceil_zero: NegZero,    // N13: floor/ceil keep signed zero or give +0
    pub alarm_from_real: AlarmConv,  // R3b
    pub byte_from_real: ByteConv,    // N5
    pub string_decimals: StrRound,   // N6
    pub float_model: FloatModel,     // N3: F64 only for now; per game from game.toml [engine]
}
// crates/sim/src/num (WP2): every rule-dependent function takes the rules
pub fn gm_eq(a: Real, b: Real, r: &NumRules) -> bool;   // also gm_ne, gm_lt, gm_le, gm_gt, gm_ge
pub fn gm_bool(x: Real, r: &NumRules) -> bool;
pub fn sign(x: Real, r: &NumRules) -> Real;
pub fn gm_min(a: Real, b: Real, r: &NumRules) -> Real;   pub fn gm_max(a: Real, b: Real, r: &NumRules) -> Real;
pub fn gm_div(a: Real, b: Real) -> Result<Real, NumError>;  // trunc(a / b): Confirmed (manual "rounded toward zero")
pub fn gm_mod(a: Real, b: Real) -> Result<Real, NumError>;  // a - gm_div(a, b) * b: Confirmed formula
pub fn alarm_from_real(v: Real, r: &NumRules) -> Real;
pub fn byte_from_real(v: Real, r: &NumRules) -> u8;
pub fn gm_string_real(v: Real, r: &NumRules) -> String;
```

- Defaults for engine_rev 0 keep current behavior where the spec has no answer yet (compare `Epsilon(1e-13)`, truth `GeHalf`, sign `Epsilon`), so nothing moves before the traces are analyzed. `num-eps-slip*` (little-trap) and `num-eps-alpha` (jun-g) are recorded; when the spec records the result, the default changes. **Why `sign` matters:** slip blocks subtract `slip * sign(hspeed)` (numbers Confirmed, slip rule). The spec's predicted ±5.55e-17 creep (numbers Inferred) only happens with an exact `sign`; the epsilon `sign` stops at a value that small. So N11 must follow N1, and both are tested on that sequence.
- `gm_div` changes now: the spec Confirms truncation (`-7 div 2 = -3`), and the current code floors. Only fractional operands stay Inferred (N8). `gm_mod` keeps its formula but is defined through `gm_div`.
- All built-in comparisons the original made in GML go through `gm_*`; plain Rust comparisons are only for our own bookkeeping. A `clippy::float_cmp` deny in `sim` keeps raw `==` on `Real` out of built-ins (section 10.5).
- **Numeric literals** (N14). Confirmed (recorder), research#104: the recorder, and so every golden trace, reads a GML decimal literal as all its digits as one integer, times the double nearest 10^-k (k digits after the dot). For 0.4, 0.45, 0.017 and the expression `1/15` that is the nearest double / the correctly rounded quotient (172 release cuts bit-exact in 14 games; gravity 0.4 in `kaenbyou/stand`), but not for every literal: 31 literals at 149 sites in 18 of the 20 games read one ulp above, e.g. `0.3` is 0.30000000000000004, `0.95` is 0.9500000000000001, `0.7` is 0.7000000000000001. Game code shows it: showdown's slip ice multiplies tempHspeed by `0.95` each tick, and every step of `traces/showdown/a3-t19-a.csv` / `-b.csv` fits 0.9500000000000001 (3.0 gives 2.85; the nearest double gives 2.8499999999999996). The list: `python -I specs/engines/gm8/traces/analysis/literal_parse.py -v` in the research checkout. The real runner's parser is still open (research#8); if it reads the nearest double, the converter rule flips and recorder-based goldens that use the affected literals are re-pinned.
  - **Where the model is applied: the converter** (option (a) of #564; converter work: research#109). The converter writes each GML decimal literal as its recorder-model double, in the shortest form that reads back to the same bits (`0.9500000000000001`), into profiles, params and scripts. TOML and Rhai read decimals correctly rounded, and the canonical formatter already keeps the shortest round-trip form (format v0, common rules), so that text is the exact double and a package shows the value the sim uses. A decimal a person writes by hand means the nearest double, as everywhere else in Rust and TOML; nothing reinterprets it. The sim never parses GML. Built-in Rust constants that port a GML literal hold the recorder value too (ice `temp_decay` default 0.9500000000000001, the GAMEOVER overlay cap 0.6000000000000001); `num::gml_literal` states the rule and the tests pin it (`num::tests::gml_literal_uses_the_recorder_model`, `wp5c_ice_ladder::temp_hspeed_glide_matches_the_showdown_trace`, which replays the a3-t19 tempHspeed column bit for bit).
  - WP7b's test compares every profile constant of the 20 packages with traced bits where a trace shows it. Computed values are written as the expression's inputs plus an action that computes them at run time in binary64, in the original's operation order, never pre-computed by the converter.
- **Short-circuit.** GML evaluates both sides of `&&`, `||` and `^^` (Confirmed), so a random call on the right always draws. Built-ins that port such an expression evaluate both operands first, then combine. Scripts: section 11.

### 3.3 RNG (WP2)

- `rng.rs`: `Rng { seed: u32 }`, `next_unit() -> Real` (seed = seed × 134775813 + 1 mod 2^32 with `wrapping_mul`/`wrapping_add`; value `seed / 2^32`; Inferred, N9), `random(x)`, `random_range(a, b)`, `irandom(x)`, `irandom_range(a, b)`, `choose(n)`. One draw per call (Inferred). Seed source: replay header. Signed/unsigned scaling and start seed are UNKNOWN (trace num-rng, not recorded yet).
- **Hidden RNG consumers draw from the gameplay RNG** (Confirmed: blood picks frames and gravity with `choose` and `random`; playerface uses `irandom(3)`). The blood emitter and playerface (WP6) make the same draws in the sim and send the drawn values to the client as output events; only drawing stays in the client. **Draw interleaving**, from the spec's description of `blood.events/Create` and `bloodEmitter.events/Step`: for each blood in one emitter Step, the blood's Create draws (subimage `choose(0,1,2)`, then gravity `random`) inside `spawn`, then the emitter draws its direction (`irandom(35)*10`) and speed (`random(6)` or `random(8)`). WP6a writes this order as a test.
- **Particles** (N10). Particle functions appear in 6 games, and kaenbyou's death blood is a particle stream (`kid/B.md` Death). Whether particle systems draw from the gameplay RNG is UNKNOWN (numbers UNKNOWN: "which runner internals draw from the RNG"). Switch `NumRules::particles_draw_rng`; no GM default is claimed. engine_rev 0 uses `false` as a placeholder, documented as Design, so B-family particle blood and spook oPatFakeBlock particles are client output only until a trace shows otherwise.

### 3.4 Collision (WP3)

Spec: CLONE_SPEC "collision" (Confirmed masks, kid mask, spikes, function contracts, solid event; Inferred bbox formula, banker's rounding, overlap test, move_contact_solid, previous place; UNKNOWN list), "sprites and animation" (masks under transforms).

**Masks** (built by `crates/format`, consumed here):

```rust
// def/sprite.rs (WP0)
pub struct SpriteDef { pub frames: u16, pub size: (u32, u32), pub origin: (i32, i32), pub masks: MaskSet }
pub enum MaskSet { Shared(MaskId), PerFrame(Vec<MaskId>) }   // "separate collision masks" (Confirmed)
pub struct Mask { pub bbox: IRect /* sprite space, inclusive */, pub shape: MaskShape }
pub enum MaskShape { Rect, Bits(BitMask) /* rows of u64 words, bbox-relative */, Empty }
```

- Pixel in mask: alpha compared to tolerance, `>` by default (C5, UNKNOWN; one const in `format::masks`). Only pixels inside the bbox count; a manual bbox is shared by all frames (Confirmed). Disk and diamond are rasterized into `Bits` inside the bbox (Inferred). `left > right` boxes (sprBow) give `Empty` (Inferred).
- Which frame's mask: `floor(image_index) mod frames` at the time of the check (Inferred, C6). Where the animation update sits (R7) changes which frame a cherry uses during Collision.

**Placement and bbox** (all in `collide/bbox.rs`, the only place that rounds):

```rust
pub fn place_origin(v: Real, rules: &CollideRules) -> i64;               // banker's round (Inferred), C2
pub fn instance_box(def: &GameDef, inst: &Instance, at: (Real, Real), rules: &CollideRules) -> Result<Option<PlacedMask<'_>>, SimError>;
pub struct PlacedMask<'a> { pub bbox: IRect /* room space, inclusive, i64 */, pub mask: &'a Mask, pub xf: Xform }
```

- Unit scale, angle 0: `bbox_left = round(x) + (mask.left - origin.x)` and so on (Inferred). The kid gives `round(x)-5 .. round(x)+5`, `round(y)-12 .. round(y)+8`.
- Scale, mirror and rotation: `collide::xform::map_box` and `map_pixel` (C3, C4 UNKNOWN). Default: offsets relative to the origin are mirrored `d -> -d` and rotated by multiples of 90 degrees exactly; fractional scale (mini spikes 0.5) scales offsets and floors; other angles return `SimError::Unsupported` in engine_rev 0, so a package that needs them waits for MASK-ROTATE traces or routes the object to a script. Kaenbyou and eat-the-lemon need the 90/180/270 case for every engine spike (`kaenbyou/famb-spike-mask-*` recorded).
- Place tests round the tested coordinate (`round(y + 1)`), not `round(y) + 1` (Inferred "Standing heights"; C2 switch `PlaceRound::TestedCoord | RoundThenAdd`).

**Overlap**: inclusive boxes, then a pixel test over the intersection when either shape is `Bits` (Inferred). `collide/overlap.rs`.

**Queries** (`collide/query.rs`), each excluding `me`, skipping inactive instances, skipping dead ones unless `TickRules::dead_in_queries` (R22), and returning `World::first_of` order (R21):

```rust
pub enum Target { Class(ClassMask), Kind(KindId), Inst(InstId), Any }
pub fn place_free(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError>;   // solids only (Confirmed)
pub fn place_empty(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError>;
pub fn place_meeting(ctx: &Ctx, me: InstId, x: Real, y: Real, t: Target) -> Result<bool, SimError>;
pub fn instance_place(ctx: &Ctx, me: InstId, x: Real, y: Real, t: Target) -> Result<Option<InstId>, SimError>;
pub fn instance_position(ctx: &Ctx, x: Real, y: Real, t: Target) -> Result<Option<InstId>, SimError>;
pub fn collision_point(ctx: &Ctx, x: Real, y: Real, t: Target, prec: bool, notme: Option<InstId>) -> Result<Option<InstId>, SimError>;
pub fn collision_rectangle(ctx: &Ctx, x1: Real, y1: Real, x2: Real, y2: Real, t: Target, prec: bool, notme: Option<InstId>) -> Result<Option<InstId>, SimError>;
pub fn collision_line(ctx: &Ctx, x1: Real, y1: Real, x2: Real, y2: Real, t: Target, prec: bool, notme: Option<InstId>) -> Result<Option<InstId>, SimError>;
pub fn distance_to_object(ctx: &Ctx, me: InstId, t: Target) -> Result<Option<Real>, SimError>;          // C13
pub fn move_contact_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError>;   // C10, C11
pub fn move_outside_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError>;
pub fn move_bounce_solid(ctx: &mut Ctx, me: InstId, precise: bool) -> Result<(), SimError>; // C14, tier 3
pub fn solid_resolve(ctx: &mut Ctx, a: InstId, b: InstId, run: impl FnOnce(&mut Ctx) -> Flow) -> Result<Flow, SimError>;
```

- `move_contact_solid` (Inferred): unit step `(dcos d, -dsin d)`; repeat up to `maxdist` times (no limit when negative): if one step further hits a solid, stop, else step. Fractional `maxdist` and `maxdist == 0` are C11 (`collide::contact::mcs_step_limit`). Start already overlapping: the manual says do not move, OpenGMK docs say the start is not checked (C10, `CollideRules::mcs_start = ManualNoMove` default; `little-runner/t-col-07-*` recorded). Non-axis directions (kaenbyou playerSlope) use the same loop; their sub-pixel result is UNKNOWN. A negative `maxdist` loop is bounded by the room size plus the mask size (Design), then `SimError::Unsupported`.
- `distance_to_object`: Confirmed (recorder): `distance_to_object` = sqrt(dx*dx + dy*dy) where dx and dy are the edge gaps of the rounded reported bboxes (dx = other.left - self.right if > 0, else self.left - other.right if > 0, else 0); touching boxes give 1 because the edges are inclusive (`traces/deride-kotaro-from-aslant/trick-vine-kick-and-djump-d.csv` rows 108-110 give sqrt(5), sqrt(26), sqrt(65); `traces/showdown/a3-t16-c.csv` row 13; `bug-right-vine-miss-f.csv`). The switches below stay, with these defaults. `kid/B.md` and `kid/A3.md` note that diagonal or sub-pixel positions give fractional distances, which integer boxes cannot. So C13 has two switches: `DistBoxes::RoundedInclusive | Unrounded` (boxes from C1 rounding, or from unrounded `x + offset`) and `DistOffset::None | PlusOne`. The x and y gaps are combined as `sqrt(dx*dx + dy*dy)` (Confirmed; alternative `Max`). Defaults: the reported boxes (`DistBoxes::Reported`, each edge rounded; a left-facing kid's mirrored box x-7 is the box used), `PlusOne` and `Euclid`. Evidence also in T-COL-10 and the recorded `deride-kotaro-from-aslant/bug-right-vine-miss-*`, `animus/a-vine-right-*`, `kaenbyou/famb-vines-*`. `None` when no instance exists (A3 U4: what GM returns is UNKNOWN; callers treat `None` as a large value).
- `collision_rectangle/line` corner inclusivity and fractional corners: C12, one function each.
- **Broadphase** (`collide/grid.rs`, Design): uniform 64 px grid per room. All cell math in `i64` with `i64::try_from` and checked arithmetic; a value that does not fit is `SimError`, never an `as usize` cast (wasm32 has a 32-bit `usize`). Static instances are bucketed at room start, moving ones re-bucketed when their box changes, out-of-room instances go in an overflow list. Results are re-sorted by `first_of` order before use, so the grid never changes outcomes. A grid-equals-brute-force test runs on every CI target including wasm32 (section 10.6). Rooms up to 16800 x 8512 and 12,805 instances per game fit.

## 4. Rooms

Spec: CLONE_SPEC "rooms" (Confirmed change requests, room start sequence, persistence, views, game_restart, saves; Inferred items; UNKNOWN list), mechanics.md "camera", "save", "warp", "edge_warp".

### 4.1 Room data and loading

```rust
// def/room.rs (WP0)
pub struct RoomDef {
    pub name: String, pub width: u32, pub height: u32,
    pub role: RoomRole,                     // Boot | Title | Menu | Select | Play (section 8.3, CR6)
    pub file_speed: u32,                    // room_speed on entry, before world Room Start (Inferred, R14)
    pub persistent: bool,
    pub views_enabled: bool, pub views: [ViewDef; 8],
    pub instances: Vec<PlacedInstance>,     // file order = creation order (Inferred, rooms: "created in the order they are listed")
    pub on_create: Vec<RoomAction>,         // room creation code, as data (section 8.3)
}
pub struct PlacedInstance { pub kind: KindId, pub rank: u16, pub x: Real, pub y: Real,
    pub xscale: Real, pub yscale: Real, pub angle: Real, pub params: Params,
    pub name: Option<NameId>, pub group: Option<GroupId> }
```

`RoomRole` replaces every "not rInit/rMenu/rTitle/rSelectStage" test in the engines: kid Alarm 0 play timer, bow self-destroy, the R handler's "unless menu room", B Create save, spook saveExe skip, rely's killPlayer special case in rSelectStage. The loader never sorts `instances`.

### 4.2 Room change sequence (WP4a, `room.rs`)

```rust
pub enum RoomRequest { Goto(RoomId), Next, Previous, Restart, GameRestart, GameEnd }
pub enum StartPoint {
    NewGame { difficulty: u8 },           // what warpStart does: delete save, set difficulty, go to GameDef::new_game_room
    Room { room: RoomId, difficulty: u8 }, // trace cases: the recorder's --room jump
    FromSave(SaveSlot),
}
pub fn start_game(ctx: &mut Ctx, start: &StartPoint) -> Result<(), SimError>;  // boot instances, first room, Game Start
pub fn perform_change(ctx: &mut Ctx, req: RoomRequest) -> Result<(), SimError>;
```

- `start_game` first creates the persistent instances listed in `GameDef::boot` (the `world` built-in, Design: it stands in for rInit), then enters the start room. Title, menu and select rooms are client UI for now (Design, CR7); a run enters gameplay through `StartPoint::NewGame`.
- `perform_change` does the manual's 7 steps (Confirmed):
  1. Room End for every instance (dispatch order).
  2. Remove non-persistent instances, no Destroy event. Deactivated persistent instances count as non-persistent here: they do not move to the next room (Confirmed, manual 403_06). If the old room is persistent, store its instances in `persistent_rooms[old]` first.
  3. Keep persistent instances (ids unchanged).
  4. If the new room is not persistent or not visited: create placed instances in file order; for each, apply its params (instance creation code) then run Create. If it is a restored persistent room, put its saved instances back without Create (Inferred, R18).
  5. First room only: Game Start for every instance (no game in the 20 uses it, Confirmed).
  6. Room creation actions (`on_create`).
  7. Room Start for every instance in dispatch order (snapshot; R10).
- Also: `room_speed = file_speed` at entry (Inferred, R14), views reset from `ViewDef`, transition reset (visual only, `RoomOutput::Transition`; transitions spend no ticks, Inferred R17b). `Restart` and `Goto(current)` rebuild a non-persistent room (Inferred). Persistent room `Restart` is out of scope (Confirmed unused).
- **Requests raised during the sequence** (R23, rooms UNKNOWN). kaenbyou and eat-the-lemon call `loadGame` (room_goto) from rLoad's room creation code (Confirmed that they do; what follows is UNKNOWN: do Room Start events still run, is the room drawn, is a tick spent). Switch `RoomRules::nested_request`: `ChainAfterSequence` (Design default: finish all 7 steps, then run `perform_change` again before the next tick) or `NextTick` (rLoad gets one tick). A loop bound of 8 chained changes per tick, then `SimError`. `kaenbyou/tr-room-12` is recorded and settles it.
- **`instance_destroy()` in room creation code** (R24, rooms UNKNOWN: which instance is "self"). Showdown's stage rooms call it. Confirmed (recorder): it destroys nothing and the rest of the code runs (showdown `a3-t18-a` rows 42-43, `a3-t20`, `tr-room-14`), so `RoomAction::DestroySelf` is a no-op.

### 4.3 Restart and game_restart (WP4b)

- `GameRestart` (Inferred): remove all instances (persistent too), forget persistent room states, reset globals per `RoomRules::globals_survive_restart` (UNKNOWN, R16; `little-runner/tr-room-04` recorded), start the first room again. Sounds keep playing (Inferred, sound section; S4).
- The engines' init rooms and world objects are not ported as GM code. `GameDef::restart: RestartModel` describes the retry path; the `world` built-in (WP6a) runs it **at the world's rank**, in the phase its trigger names:

  ```rust
  // def/game.rs (WP0), behavior in room.rs (WP4b)
  pub struct RestartModel { pub trigger: RestartTrigger, pub kind: RestartKind, pub excluded_roles: RoleSet,
      pub boot_ticks: u8, pub flipped_load_dy: Real /* +4 */, pub death_on_restart: DeathOnRestart,
      pub kid_steps: RetryKidSteps /* B: add_time 2, save_death_time, clear_frozen; showdown: clear_frozen, reset_jump_values (#224) */ }
  pub enum RestartTrigger { KeyPressEvent, StepHeld, StepPressed { skip_while_paused: bool } }
  pub enum RestartKind { GameRestartViaBoot, RoomGotoSaved, ShowdownLoad, RecreateInPlace }
  ```

  | Kind | Trigger | Games | Steps (spec source) |
  |---|---|---|---|
  | `GameRestartViaBoot` | `KeyPressEvent` (keyboard phase); wanna `StepHeld` (world Step, every tick while held, A3 I7) | Groups A (minus showdown, see below), B, C: 15 games | Optional death-only save (little-runner, wanna saveDeathTime); create kid at (0,0) in the old room if none; set the requested room and position (saveExe); game_restart (last request wins, R13); `boot_ticks` ticks in a boot room with no gameplay instances; then the saved room at the saved position, plus `flipped_load_dy` when flipped (10 Group A games: rely and 9 others; not rori, fairy-tale, showdown). Rooms "game_restart" Confirmed list; timeline N, N+1, N+2 Inferred (TR-ROOM-01; `rely-the-needle-4-2/tr-room-01-a` recorded). |
  | `RoomGotoSaved` | kaenbyou, eat-the-lemon `KeyPressEvent`; go-the-spook-house `StepPressed` (world Step, rooms Confirmed) | 3 | Destroy kid and bow, create kid, `Goto(saved room)`, set position (rooms Confirmed). B adds: +2 to the play timer, saveDeathTime, clear frozen; the new kid's Create sees the stale `reverse` (kid/B.md R and Death). Spook's saveExe skips the goto in Title/Menu roles. |
  | `ShowdownLoad` | `KeyPressEvent` | showdown | kid/A3.md showdown "Restart": clear frozen on both kid kinds, add a death and saveDeathTime and destroy each, loadGame (create kid at (0,0), reset jump values and the listed globals), write temp, saveExe, loadDeathTime; **no game_restart**. |
  | `RecreateInPlace` | `StepPressed { skip_while_paused: true }` | keep-celebrating-for-xia | Destroy kid, create kid at the saved spot in the current room, then `Goto(saved room)` (rooms Confirmed). |

- **R26 (settled).** Showdown's R has no game_restart (research PR #98; recorder `traces/showdown/a3-t18-a.csv` f41-43). The rooms section now counts 15 of the 20 games with a full game restart, matching this plan.
- `boot_ticks` is the single number to change after TR-ROOM-01. Whether the boot tick exists at all is UNKNOWN (rooms: "whether a room change consumes a tick").
- `room_restart` (kaenbyou warpStart, xia death in difficulty room) is `RoomRequest::Restart`.
- `Reset` (F2): Groups A, B, C delete the save's temp marker and game_restart; kaenbyou/lemon gameReset resets globals and goes to the boot room (rooms Confirmed). `GameDef::reset: ResetModel` mirrors `RestartModel`.

### 4.4 Saves (WP4b, `save.rs`)

The sim never touches files. The save is sim state; the client stores its bytes.

```rust
// `flipped` is `global.reverse` for a `yflag` game (family B), the kid's flip state otherwise (#267).
pub struct SaveSlot { pub room: RoomId, pub x: Real, pub y: Real, pub flipped: bool, pub facing: i8,
    pub difficulty: u8, pub deaths: u32, pub time: u32, pub bosses: u16, pub items: u16, pub clear: bool }
pub enum SavedPosition { XyBase100Bytes, XyBase256Bytes, XyText2Dec, XyPrevious, FixedPoint, FloorNudge }
pub fn save_now(ctx: &mut Ctx, kid: InstId, model: SavedPosition, fixed: Option<(Real, Real)>) -> Result<(), SimError>;
pub fn save_counters(ctx: &mut Ctx) -> Result<(), SimError>;        // death-only save
pub fn quantize(x: Real, y: Real, model: SavedPosition, r: &NumRules) -> (Real, Real);
pub fn load_into(ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError>;
```

- `quantize` reproduces what the original's save format does to a position (rooms Confirmed group table): base-100 bytes (13 games; last byte through `byte_from_real`, N5), base-256 div/mod bytes (little-runner, N8), `string()` two decimals (machinary, eat-the-new-year, N6), xprevious/yprevious exact (kaenbyou, eat-the-lemon, N7), fixed point (poronkusema), floor with the three-step +1 nudge using `place_free` (xia). This decides the respawn v-align (tricks `trick.respawn_valign`). Recorded and waiting for the spec: `kaenbyou/tr-room-08` (N7), `little-runner/num-divmod-save-*`, `eat-the-new-year/num-string-save-*`, `machinary/tr-room-07-a`, `little-runner/tr-room-06`.
- Death-only saves update counters only. `deaths` and `time` use `checked_add` (10.3).
- `World.save = None` means "save file missing"; the kid Create's first-play save (rely Create step 4, B Create step 4) tests it.
- The client persists `SaveSlot` with postcard; `StartPoint::FromSave` restores it.

### 4.5 Views and camera (WP4b `view.rs`, camera kinds in WP6a)

- `View { enabled, visible, x, y, w, h, port, follow: Option<Target>, hborder, vborder, hspeed, vspeed, angle }` per room from the room file. `view_xview` without index is view 0 (Inferred).
- Runner follow (eat-the-lemon, animus): `view::follow_update`, once per tick before the Draw slot; follows `World::first_of` of the target (R21) (Inferred algorithm; check order and phase UNKNOWN, R19; `animus/tr-room-09-*`, `eat-the-lemon/tr-room-09-*` recorded). Camera objects are the `camera` built-in with a `mode` parameter (WP6a); their Step placement gives the 1-tick lag the spec infers (`little-runner/tr-room-11` recorded).
- Views affect gameplay through `view_xview` reads (GAMEOVER position, camera-relative spawns), so they are hashed sim state.

## 5. The kid

Spec: `kid/A1.md` .. `kid/C.md`, mechanics.md "Physics profile fields needed by the kid families", tricks.md. Built-in `kid` (WP5a core, WP5b B, WP5c the rest).

### 5.1 Approach

The kid is one built-in whose events run an **ordered list of named actions** read from `physics.toml`. Each family's events are written as such lists. Every spec rule maps to either a field value or a named action (a Rust function in `kid/actions/`). Nothing family-specific hides in an `if family == B`. A `preset` key fills defaults per family (`kid/presets.rs`), but every field can be overridden.

```rust
// crates/sim/src/def/physics.rs (WP0 skeleton, WP5a owns; serde Deserialize, deny_unknown_fields)
pub struct PhysicsProfile {
    pub preset: KidFamily,                 // A1 A2 A3 A4 A5 B C: documentation and defaults
    pub numbers: KidNumbers,
    pub input: KidInput,
    pub events: KidEvents,                 // ordered action lists per event
    pub jump: JumpRules, pub fall_cap: FallCap, pub gravity: GravityRules,
    pub platform: PlatformRules, pub vine: VineRules, pub water: WaterRules,
    pub ice: IceRules, pub slope: SlopeModel, pub ladder: LadderRules,
    pub flip: FlipModel, pub death: DeathRules, pub bullet: BulletRules,
    pub save: SaveRules, pub misc: KidMisc,
}
// float_model moved to game.toml [engine] -> NumRules::float_model (N3): it is runner FPU state, not a kid field.
// crates/sim/src/kid/mod.rs (WP5a)
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;
pub fn event_set(p: &PhysicsProfile) -> EventSet;
pub mod presets { pub fn apply(p: &mut PhysicsProfileDraft) -> PhysicsProfile; }
```

`KidState` (in `kid/state.rs`, WP5a-owned, fields added by WP5b/WP5c in their own sub-structs `KidStateB`, `KidStateExt`) holds: `djump, cur_jumps, tjump, on_platform, ladder, ladder_ud, ladder_jump, frozen, frozen2, yflag, flipped, facing, temp_hspeed, run_speed, jump_speeds, max_vspeed, max_fall_speed, fade_alpha, invincible, scale, on_slope_last`. Game-side latched key flags (wanna, rori, fairy-tale, showdown) live in `KidGlobals` because they persist while no kid exists (A3 Inferred, I6).

### 5.2 physics.toml (v0.1, replaces the v0 draft section)

```toml
#:schema ../../client/docs/format/schema/v0/physics.schema.json
preset = "A1"                 # A1 | A2 | A3 | A4 | A5 | B | C

[numbers]
jumps = [8.5, 7.0]            # ground jump, then air jumps in order (B: jump[1], jump[2]; showdown adds 6.0)
gravity = 0.4
run_speed = 3.0
max_vspeed = 9.0
max_fall_speed = 8.0          # read only by fall_cap = "shift_then_clamp"
release_mult = 0.45           # 0.5 in machinary, eat-the-new-year
sprite_threshold = 0.05       # 0.5 in go-the-spook-house
spawn_offset = [17, 23]       # machinary [16, 23]
image_speed_idle = 0.2

[input]
walk = "direct_latch"         # direct_latch | sampled; see tricks trick.restart_glitch
tap_fix = false               # xia directionalTapFix
vine_keys = "edges"           # edges | latched_flags (wanna, rori, fairy-tale, showdown)

[events]
create = ["create_defaults", "ensure_world", "create_bow", "first_play_save", "arm_play_timer"]
room_start = []                # B: ["create_bow", "set_view_angle"]
step = ["gravity", "walk", "air_sprite", "fall_cap", "shoot", "jump", "release",
        "ad_nudge", "vines", "flip_swap", "platform_gravity_zero", "fade_in"]   # `gravity` stands for the ladder action's step 4 until WP5c builds `ladder` (see below)
end_step = ["ladder_sprites"]
alarms = [{ n = 0, run = ["play_timer"] }]   # B: cancel poll (unused while cancels are on)
outside_room = []              # B: ["kill_unless_room_changer"]
keys = [{ action = "suicide", on = "press", run = ["kill"] }]   # machinary H/T/W, xia C: more rows
collisions = [
  { with = "block", run = "block_standard" },
  { with = "solidblock", run = "block_standard" },
  { with = "platform", run = "platform_standard" },
  { with = "killer", run = "kill" },
]
boundary = "intersect_origin"  # intersect_origin | outside_room_kill | end_step_origin | none

[jump]
ground_classes = ["block", "solidblock", "water1"]   # 1 px in gravity direction
ground_also = ["platform_flag", "ladder_jump"]
ground_sets_djump = true        # false in A4
air = "djump"                   # djump | cur_jumps (B) | djump_then_tjump (showdown)
water_air = "water2_without_flag"  # none | water2_without_flag | water_unlimited (B) | water3_keeps (C)
slope_force_within = 0          # rainbow, rori, fairy-tale: 2
release_twice_while_tjump = false
blocked_by_pause_press = false  # kaenbyou: true (Action::Pause)

[fall_cap]
rule = "clamp_abs"              # clamp_abs | clamp_against_gravity (B) | clamp_down_only (spook) | shift_then_clamp (A4)

[gravity]
reapply = "step"                # step | step_copy_grav (wanna) | end_step_slope (B) | end_step (little-runner) | create_only (showdown, xia)
block_landing_zeroes = true
platform_zero_in_step = true    # rely only (Step action 6)

[platform]
rule = "half_vspeed"            # y - vspeed/2 <= platform.y
snap = 9
take = "vspeed"                 # vspeed | delta_y (B: other.y - other.yprevious) | yspeed_flipped (C)
hold_probe = 4

[vine]
model = "distance_asym"         # distance_asym (A, B) | distance_sym_early (C) | none
left_below = 2.0
right_equal = 1.0
slide = 2.0
kick = [15.0, -9.0]
drop = 3.0

# [water], [ice], [slope], [ladder], [flip], [death], [bullet], [save], [misc]: see 5.3
```

Event lists name actions; a family's lists come from its spec's event sections in order. The `create` list must cover the spec's Create steps including the first-play save (`first_play_save`: when `World.save` is `None`, call `save::save_now`; rely Create step 4, B Create step 4) and "create world if missing" (`ensure_world`, via `EngineKind::World`).

### 5.3 Spec rule to field or code path

Each row is one spec rule; the last column is the field or action that implements it. "A1 Step 3.5" means kid/A1.md "Step", action 3, step 5. The WP column says who implements it.

| Spec rule | Families | Field / action | WP |
|---|---|---|---|
| Kid mask 11x21, origin (17,23); player2 mask origin (17,11); xia flip mask origin (17,8) | all | sprite `mask` of the kid kind; `[flip] mask_flipped` | 5a |
| Create defaults (jump 8.5, jump2 7, maxSpeed 3, gravity 0.4, maxVspeed 9, image_speed 0.2, djump true / curJumps 1) | all | `numbers.*`, `jump.air`, action `create_defaults` | 5a |
| Create: world if missing, bow, first-play save when no save exists, alarm[0] = room_speed | A1, A5, B | actions `ensure_world`, `create_bow`, `first_play_save`, `arm_play_timer` | 5a |
| B Create does not set gravity (0 until End Step) | B | `gravity.reapply = "end_step_slope"` and create gravity 0 | 5b |
| B Room Start: create bow, set view angle | B | `events.room_start` actions `create_bow`, `set_view_angle` | 5b |
| Create alpha 0 and +0.1 per Step (rely) | A1 rely | action `fade_in`, `misc.fade_in = true` | 5a |
| Gravity back to its constant every Step unless on a ladder (A1 Step action 2 step 4; the rely profile has no ladder) | A1 (rely) | action `gravity` (`gravity.reapply = "step"`); WP5a | 5a |
| Ladder: up/down while overlapping ladder grabs, moves 3 px, gravity/vspeed 0, jump leaves, else gravity 0.4 | A1 (little-trap, rainbow) | action `ladder` (two `direct` reads where the original reads twice), `[ladder] entry = "up_or_down" | "up_only"`. A profile that has ladders lists `ladder` instead of `gravity`: `ladder` includes the gravity reset (step 4), so the two are never listed together | 5c |
| Walk: right wins, frozen forces 0, facing unless near vine, vine-blocked direction keeps old hspeed | A, B, C | action `walk` (`InputState::direct` under `input.walk = "direct_latch"`) | 5a |
| Slip blocks: probe 4 px, read slip 10 px below, add/subtract `slip*sign(hspeed)`, no clamp | A1, A2, B | `[ice] model = "slip"`; uses `num::sign(.., &NumRules)` (N11) | 5c |
| Showdown ice: tempHspeed, decay 0.95, zero below 0.2 | A3 showdown | action `ice_temp_hspeed` | 5c |
| C slide block adds its h; slip clamped to 3 | C | action `slide_block`, `[ice] clamp = true` | 5c |
| Air sprite thresholds ±0.05 unless onPlatform; onPlatform cleared when no platform 4 px below | all | action `air_sprite`, `numbers.sprite_threshold`, `platform.hold_probe` | 5a |
| Fall cap variants (a)-(d) | per game | `fall_cap.rule` | 5a (a), 5b (b), 5c (c, d) |
| Shoot, jump, release in that order in one Step | all | list order | 5a |
| Ground jump classes and extras | per game | `jump.ground_classes`, `jump.ground_also` | 5a |
| A4 ground jump does not set djump | A4 | `jump.ground_sets_djump = false` | 5c |
| B jump counter (first jump sets 2, walking off spends first, air jump uses jump[curJumps], water unlimited) | B | `jump.air = "cur_jumps"`, `jump.water_air = "water_unlimited"` | 5b |
| B jump ignores press when P pressed this tick (kaenbyou) | B kaenbyou | `jump.blocked_by_pause_press = true` (reads `pressed(Pause)`) | 5b |
| B disable-cancel switch with polling Alarm 0 | B | `jump.cancel = "edge" | "poll_alarm"` | 5b |
| Showdown triple jump | A3 showdown | `jump.air = "djump_then_tjump"`, `jump.release_twice_while_tjump`, `numbers.jumps[2]` | 5c |
| Release: `vspeed < 0` (against gravity in B, C) times release_mult | all | action `release` | 5a |
| A/D nudge ±1 while block 1 px below | animus, easy-miku-2 | action `ad_nudge`, `misc.ad_nudge` | 5c |
| Vines A/B (asym distances, slide, kick, drop) | A1, A2, B | action `vines`, `vine.model = "distance_asym"`; uses C13 | 5a |
| Vines C (both sides <= 1, flags before jump code) | C | actions `vine_probe` + `vines` | 5c |
| Vine keys as game-side latched flags | wanna, rori, fairy-tale, showdown | `input.vine_keys = "latched_flags"`, `KidGlobals` flags | 5c |
| Flip by object swap at y-4 | A (11 games) | action `flip_swap`, `[flip] model = "swap_object"`, rank via selector `{kind="kid", flipped=true}` | 5c |
| Flip by yflag (B) | B | `[flip] model = "yflag"` | 5b |
| Flip by grav sign (C) | C | `[flip] model = "grav_sign"` | 5c |
| Gravity re-set variants | per game | `gravity.reapply` | 5a/5b/5c |
| Rely Step action 6: gravity 0 while onPlatform | A1 rely | action `platform_gravity_zero` | 5a |
| Block collision 3 steps (h contact, v contact + landing refresh, diagonal) | all | collision run `block_standard` | 5a |
| Block landing gravity 0 | per game | `gravity.block_landing_zeroes`, per collision entry `zero_gravity = true` | 5a |
| Flipped refresh on ceiling contact; xia treats hspeed 0 as right when flipped | B, C | `block_standard` reads `flip.model` | 5b, 5c |
| Slope step-up, slope events, downhill snaps | rainbow, rori, fairy-tale, fed-up, poronkusema | `block_slope_step`, `slope_por_rise`, `downhill_snap_*`, `[slope] model` | 5c |
| B End Step playerSlope (always runs; restores gravity even without slopes, Confirmed) | B | end-step action `slope_b` | 5b |
| C slope code in Step (unused in xia rooms) | C | action `slope_c` (tier 3) | 5c |
| Little-runner End Step: gravity 0.4 every tick, unstuck | A5 | end-step action `gravity_and_unstuck` | 5c |
| Platform collision | all | collision run `platform_standard`, `[platform]` | 5a |
| poronkusema movingBlock collision | A1 poronkusema | collision run `moving_block_por` (tier 3) | 5c |
| Killer: collision event / Step before motion (B) / End Step (wanna, spook, xia) | per game | `events.collisions` entry `kill`, or step/end-step action `killer_check` | 5a/5b/5c |
| Edge handling per family; B Outside Room kill unless roomChanger | per game | `events.boundary`, `events.outside_room`, `[death] edge_*` | 5a/5b/5c |
| RORI/FT boundary looks up roomTo at the origin point first | rori, fairy-tale | `[death] edge_lookup_point_first` (K2) | 5c |
| Suicide: Key Q pressed / held in Step (wanna) / press in Step (xia) / none | per game | `events.keys` or step action `suicide_check` | 5a/5c |
| Other kid key events (machinary H, T, W; xia C teleport) | machinary, xia | `events.keys` rows | 5c |
| Death: per-family order of sound, blood emitter, destroy kid, deaths +1, death-only save, GAMEOVER; rely kill in Select role (goto rSelectStage + destroy) | all | `kid::api::kill` + `[death]` (sound ids, gameover offset, `stop_music`, `select_room_reset`) | 5a/5b |
| Frozen: blocks input / skips Step (spook) / countdown (wanna) | per game | `misc.frozen` | 5a/5c |
| Bullet params (speed 16, life, max, spawn offset, block checks) | per game | `[bullet]` fields, built-in `kid_bullet` (WP6a) | 5a |
| Play timer in kid Alarm 0 every room_speed ticks outside menu roles | A | `events.alarms` action `play_timer` (counts into `RunGlobals::time`) | 5a |
| B play timer +2 per tick in world Step, +2 on R | B | world built-in param `timer = "b_centis"` | 6a |
| Machinary depth layers and scale | A4 machinary | `misc.scale_layers` (tier 3) | 5c |
| Multi-kid rooms (eat-the-lemon) | B lemon | several kids; "the kid" = `World::first_of` (R21); killPlayer destroys all; saveGame stores the first | 5b |
| Param zones | rainbow, showdown | `kid::api::apply_param_zone` called by built-in `kid_param_zone` | 5c |

### 5.4 Tricks switches (tricks.md)

Only SWITCHABLE tricks get switches; EMERGENT ones must fall out of the physics and become test cases.

| Trick id | Switch | Default for the 20 games |
|---|---|---|
| `trick.ad_align` | `misc.ad_nudge` | on in animus, easy-miku-2; off elsewhere |
| `input.dual_shift` | `sim::input` per-key edges (built) | on |
| `input.cactus_phantom_release` | `.inputs` and replay allow a release without press | off until UNKNOWN resolves |
| `input.numpad_cancel` | none: no OS key layer in the client (CLONE_SPEC input Inferred) | off |
| `trick.disable_cancels` | `jump.cancel` | edge (both B games) |
| `trick.coyote_time`, `trick.jump_buffer` | none (no game in the set has them) | off |
| `bug.right_vine_miss` | `vine.model` | asym in A and B, sym in C |
| `bug.vine_on_ground` | none until specified | — |
| `bug.water_full_jump_1f`, `bug.water1_low_check` | not modeled (not seen in A, C) | off |
| `trick.restart_glitch` | `input.walk = "direct_latch"`, world param `latch_cleared_by_world` | latch on; cleared in kaenbyou, lemon, xia (Inferred) |
| `input.sub_frame_tap` | same latch plus `input.tap_fix` | latch on; xia tap_fix off |
| `trick.edge_kill_timing` | `events.boundary` | per family |
| `trick.killer_check_timing` | killer action placement | per family |
| `trick.vdiet` | `NumRules::float_model` (no binary32) | off |

`game.toml [tricks]` keeps the labels for game select; the sim reads only the fields above. WP7b adds a validator check: a trick marked `required` must have its switch on.

## 6. Other built-ins

Spec: mechanics.md "Built-in library", the kid files' "Objects the kid touches", CLONE_SPEC rooms (playerStart, warps), sprites (bow, blood), numbers (hidden RNG consumers). One file per built-in in `crates/sim/src/builtins/`, documented in `docs/format/builtins/<kind>.md`.

```rust
// crates/sim/src/builtins/mod.rs (WP6a)
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;   // match on Builtin
pub fn event_set(b: Builtin, kind: &KindDef) -> EventSet;
pub fn param_specs(b: Builtin) -> &'static [ParamSpec];               // read by crates/format to validate
pub fn init_state(b: Builtin, params: &Params) -> Result<BuiltinState, ParamError>;
```

`def/builtin.rs` holds the `Builtin` enum. WP0 lists every variant named in this section (so WP7a can parse kinds), WP6a owns the file afterwards.

### 6.1 Table

Tiers: tier 1 is needed by M1, M1b, M2 and game 1 (WP6a); tier 2 by games 2-11 (WP6b); tier 3 by the rest (WP6c). Instance counts are placed instances from the mechanics.md coverage table.

| Built-in | Tier | Games (placed) | Behavior source | Notes |
|---|---|---|---|---|
| `world` (engine controller) | 1 | all 20 | rooms (room_speed 50 at Room Start), input (R, F2, P, Ctrl+M, latch clearing), kid specs (timers), restart table | Not a GM port: the engine services the infra objects did, at the rank of the original world object (`@world` in object_order). Runs `RestartModel` at its trigger (4.3), `ResetModel`, the world-Step dummy `direct` reads (`latch_cleared_by_world`), B timer, pause (xia deactivation; I5 io_clear), mute flag. Fixed real-time reads (R28). |
| `kid_start` | 1 | all, 370 | mechanics `player_start` | Create moves itself by the spawn offset; Room Start creates the kid if none and gravity normal; flipped variants (animus). |
| `block` | 1 | all, 63,678 | mechanics `block` | Solid flag, mask variants, `visual_spawns_collider` (rely objBLOCK, CR1), slip value, debug visibility. |
| `spike` | 1 | 18, 14,962 | mechanics `spike`; collision spikes Confirmed | Precise mask from its sprite; `dir` picks the sprite; B auto rotation (90/180/270 + offsets 16/15). Trap hooks in tier 2. |
| `kid_bullet` | 1 | all | kid specs "Shooting" | Create arms alarm 0 = life; hspeed set after Create by the shooter; block check before moving (rely) or collision destroy; stop-and-die (poronkusema). |
| `blood_emitter` | 1 | all | kid specs "Death"; sprites Inferred (BLOOD-COUNT); numbers hidden consumers | **Alarm plus Step**, not a hard-coded count: Create sets alarm 0 = `life` (20) and Step spawns `per_step` blood (40; showdown 5 per hit for 5 ticks; A4 probability form), alarm 0 destroys. The number of Steps (19 or 20) then follows from R9 and the creation phase (sprites Inferred). B (kaenbyou) uses `mode = "particles"` (kid/B.md Death), RNG use per N10. |
| `blood` | 1 | all | numbers hidden consumers | Create draws `choose(0,1,2)` then gravity `random` (3.3 order); output `KidOutput::Blood`. Sticks to solids per mechanics. |
| `game_over` | 1 | all | kid specs "Death" | Delay, view-relative position; `Late` handler for fades. |
| `bow` | 1 | most | sprites "Bow" | Step copies the kid's snapshot (`kid::api::bow_snapshot`, 1-tick lag Inferred); destroyed unless difficulty 0; Begin Step copy in wanna. `go-the-jun-g-world/draw-bow-lag-*` recorded. |
| `save` | 1 | 18, 768 | mechanics `save`, kid specs "Saving" | Trigger enum (shot, touch+press S/Z, held S keyboard event (kaenbyou), touch, auto, step overlap), cooldown, difficulty filter, saved position model (`save::save_now`), blink via image_speed 0.017 and Animation End. |
| `warp` | 1 | 18, 486 | mechanics `warp` | `room_to`, `warp_xy` (0,0 = destroy kid and bow), transition. Calls `request_room`, then sets kid position in the same handler (Confirmed deferral). |
| `difficulty_warp` | 1 | rely and others (warpStart) | rooms (warpStart), mechanics `warp` | Sets difficulty from param `dif`, deletes the save, goes to the new-game room; `StartPoint::NewGame` does the same (CR2). |
| `camera` | 1 | 15, 193 | mechanics `camera`; rooms Views | `mode = screen_snap | snap_offset | smooth_follow | end_step_clamp`. Not placed in rely's play rooms. |
| `killer` | 1 | most | mechanics `spike`, `cherry` | KILLER class with its own sprite; `cherry` adds image_speed 1/15, outside-room destroy, bounce (tier 3). |
| `platform` | 1 | 6, 109 | mechanics `platform` | No events; classes `PLATFORM | BLOCK` or `| SOLIDBLOCK`. Kid does the work. |
| `playerface` | 2 | 11 | numbers hidden consumers | Create: `image_index = irandom(3)` (RNG draw even when never shown). |
| `moving_platform` | 2 | 12, 341 | mechanics `moving_platform` | Yuuutu step algorithm; `carry_rule` variants. |
| `water` | 2 | 9, 2,510 | mechanics `water`, kid specs | Collision with kid: cap, refresh via `kid::api::refresh_air_jump`. |
| `vine` | 2 | 8, 499 | mechanics `vine` | No events; classes VINE_L/R. |
| `fake_block` | 2 | 11, 496 | mechanics `fake_block` | Solid when no kid, vanish on touch/approach, reverses moving platforms. |
| `hidden_block` | 2 | 8, 445 | mechanics `hidden_block` | Reveal at distance < 2 (C13). |
| `trigger` | 2 | 13, 666 | mechanics `trigger` | Five models; state in `MechGlobals`. |
| `trap` | 2 | 13, 1,362 | mechanics `trap` | Launch forms velocity / angle / path / move_to; stop chains; depth reveal. |
| `drop_platform`, `gravity_flip`, `edge_warp`, `bullet_blocker`, `slope` | 2 | 4-6 each | mechanics | As mechanics.md. |
| `music` | 2 | 14, 261 | mechanics `music`; sound Inferred | Calls the sound model (section 7); `every_step_if_not_playing` reads `sound_isplaying` from it. |
| `boss_gate`, `boss_item`, `secret_item`, `item_viewer`, `game_clear` | 2 | 4-9 each | mechanics | Flags in `MechGlobals`; `game_clear` raises the `run_end` timing trigger. |
| `jump_refresher`, `spring`, `conveyor`, `kid_param_zone` | 3 | showdown, rainbow, little-trap | mechanics | |
| `breakable_block`, `trap_stopper`, `orbit`, `path_platform`, `time_limit`, `sign`, `ladder`, `decor` | 3 | 1-5 each | mechanics | `sign` raises a client text box output; `decor` has no behavior. |
| `scripted` | 3 | 1,712 placed | mechanics "Scripts that stay hand-written" | Through WP9. |

### 6.2 Rely slice (first playable path)

The smallest set that makes rely-the-needle-4-2's play rooms run: `world`, `kid_start`, `block` (with `visual_spawns_collider`), `spike`, `kid_bullet`, `save` (touch + press S/Z), `warp`, `blood_emitter`, `blood`, `game_over`, `bow`, `difficulty_warp`. `camera`, `platform` and `killer` are not placed in rely's play rooms (A1 "Placed in reference rooms"), so WP6a does them after the slice. WP6a also covers kaenbyou's tier-1 needs for M2 (held-S save, B spike rotation, B particle blood mode).

## 7. Sound model (WP10)

Spec: CLONE_SPEC "sound" (Inferred: `sound_isplaying` changes game state in 15 games; the replacement contract; UNKNOWN list). Audio output is the client's job; whether a sound **is playing** is sim state.

```rust
// crates/sim/src/sound.rs (WP10)
pub struct SoundState { /* playing copies: (SoundId, start_tick, looping, kind); DLL adapter state; hashed */ }
pub fn play(ctx: &mut Ctx, s: SoundId, looping: bool) -> Result<(), SimError>;
pub fn stop(ctx: &mut Ctx, s: SoundId);  pub fn stop_all(ctx: &mut Ctx);
pub fn is_playing(w: &World, def: &GameDef, s: SoundId, rules: &SoundRules) -> bool;
pub fn tick(ctx: &mut Ctx);              // ends non-looping copies whose length in ticks has passed
// crates/sim/src/sound/supersound.rs, gmfmod.rs (WP10): DLL adapters
pub enum DllMode { Model, Stub }         // Stub = recorder --stub-dlls: every call returns 0 or ""
// def/sound.rs (WP10): SoundDef { kind: Normal | Multimedia, length_ticks: u32, volume: Real, pan: Real }
```

- Model per the spec's replacement contract (Design, derived from Inferred items): copies start at the call's tick; `length_ticks` = song length × tick rate, from the package (CR9); at most one MULTIMEDIA copy (what a second start does is UNKNOWN, S2); `is_playing` latency 0 ticks (UNKNOWN, S1); game_restart keeps sounds (Inferred, S4).
- SuperSound (kaenbyou, eat-the-lemon) and GMFMODSimple (go-the-spook-house, xia) adapters: getters return exactly the last value set (UNKNOWN, S5). Their values decide fader object lifetimes, so they affect instance ids and hashes. `SimOptions::dll_mode = Stub` reproduces `--stub-dlls` traces (every kaenbyou trace uses it); `Model` is for play and replays. CleanMem calls are no-ops.
- The `Mute` action flips the game's music flag through the world built-in; music objects then call `stop_all`, which is sim state.
- As built (WP10, #20): `SoundState { copies, supersound, gmfmod }`; the extra `sound::volume` (output only). `SoundOutput::{Load, Play, Stop, StopAll, Pause, Volume, Pitch}` with a `SoundTarget` (built-in id, SuperSound handle, FMOD sound, instance or group). A copy plays the ticks `start .. start + length_ticks`; `is_playing` computes this itself, `tick` (row 16) only drops ended copies. S1 delays only the start. S2 and S3 apply to MULTIMEDIA copies. S4 is read by `game_restart` (`keep_on_restart = false` stops all). The adapters take the arguments of the GML `external_call` (strings for SuperSound, numbers for FMOD); because the DLLs do not know a sound length, the script host passes `length_ticks` at load (0 = unknown, plays until stopped; the package side is OPEN).
- Output: `SoundOutput::{Play, Stop, StopAll, Volume, Pitch}` for the client. Music sync to the tick clock is a client feature (sound Inferred contract item 5, "The user should accept or reject this").

## 8. Package loading (`crates/format`, WP7a and WP7b)

Spec: format v0, converter `FORMAT_NOTES.md`. The sim never reads files: `format` parses `PackageFiles` (path to bytes) and returns `sim::def::GameDef`.

### 8.1 API

```rust
// crates/format/src/load.rs (WP7a, including the calls to the WP7b steps; WP0 fixed their signatures, section 18)
pub fn load_game(files: &PackageFiles, mode: LoadMode) -> Result<GameDef, Vec<Diagnostic>>;
pub enum LoadMode { Strict, Trace { unmapped_kinds_inert: bool } }  // Trace: unmapped script kinds load as inert with a warning
// crates/format/src/sprites.rs, masks.rs (WP7a)
pub fn load_sprite(id: &str, png: &[u8], sidecar: &SpriteSidecar) -> Result<(SpriteDef, Vec<Mask>), Diagnostic>;
pub fn build_mask(rgba: &[u8], w: u32, h: u32, shape: CollisionShape, bbox: Option<IRect>, tolerance: u8) -> Mask;
pub const ALPHA_IN_MASK: AlphaRule = AlphaRule::Greater;   // C5 (format-side, read only at load; a change bumps gameplay_hash)
// crates/format/src/{rooms,objects,order,vars}.rs (WP7a), {physics,params,timelines,paths,sounds}.rs (WP7b)
pub fn resolve_ranks(order: &[OrderEntry], kinds: &[KindDef]) -> Result<RankTable, Diagnostic>;
pub fn intern_vars(names: impl Iterator<Item = String>) -> VarTable;   // sorted (1.1)
pub fn load_trace_map(text: &str) -> Result<TraceMap, Diagnostic>;     // used by tools only
```

### 8.2 Steps

1. Manifest (exists). `[engine]` (restart, reset, boot, uninit_reads_zero, float_model, new_game_room).
2. Sprites: decode PNG strips (`png` crate), split frames, build masks per `collision`, `bbox`, `alpha_tolerance`, `separate`. Mask bits go into `GameDef.masks`; image pixels never reach the sim.
3. Kinds: built-ins from `sim::def::Builtin`, presets and scripted objects from `objects/*.toml` (base, params, sprite, mask, solid, visible, depth, persistent, classes). Objects with `status = "infra"` in the trace map, or `script = "world" | "init"`, map to built-ins or are dropped, never loaded as scripts (CR3). In `LoadMode::Trace`, a kind whose script does not exist loads as inert (no events, its mask still collides) with a warning.
4. Rooms: expand `run` in row-major order without reordering; params per instance; views; file speed; role; `on_create` actions.
5. `object_order` to `RankTable`. Every kind placed **or spawnable** must have a rank: placed kinds, `GameDef::engine_kind` targets, and kinds named in params that spawn (CR4).
6. Variable names interned sorted (1.1).
7. (WP7b) physics: `toml::from_str::<PhysicsProfileDraft>` with spans as diagnostics, then `kid::presets::apply`. A legacy draft profile (`rules`, `jumps`, `step_sets`, `air_jump_restore`) maps to its family preset with a warning. Params validated against `builtins::param_specs`. Timelines, paths, sounds (`SoundDef`).
8. Cross-reference checks: warp targets, trigger names, sprite/mask ids.
9. `gameplay_hash` over the canonical parsed form (architecture "Versions").

### 8.3 Format additions this plan needs

WP7a (rooms, objects, order) and WP7b (physics, params, paths, sounds) propose these in `docs/format/v0.md` and send them to the converter owner (research).

| Addition | Why | Converter OPEN |
|---|---|---|
| `object_order` selectors (`{ kind = "spike", dir = "up" }`, `{ kind = "kid", flipped = true }`) | collapsed objects keep original index order (R2) | 13 |
| Kind default sprites per variant | spike `dir` must pick the original sprite and mask | — |
| `objects/*.toml`: `persistent`, `mask`, `counts_as` (classes) | GM persistence, object-level masks, parent emulation | 9, 10 |
| Sprite sidecar: `collision = "disk" | "diamond"`, `separate_masks`, zero-frame sprites | 20 DISK sprites, 1,510 separate-mask sprites, 7 empty sprites | 9 |
| Room: `file_speed`, `role`, `[[views]]`, `background_color`, `backgrounds` | R14, 4.1 roles, view follow, visuals | 4, 5, 6 |
| Room `on_create` actions and instance creation code as params | showdown, jun-g, spook, kaenbyou/lemon rLoad; warps | 7 |
| `paths/<id>.toml` | 554 paths in 16 games | 10 |
| Sounds: `kind = "normal" | "multimedia"`, `length_ticks` | section 7 | — |
| `game.toml [engine]`: `restart`, `reset`, `boot`, `uninit_reads_zero`, `float_model`, `new_game_room` | 4.2, 4.3, N3 | 3 |
| `game.toml [timing]` triggers with a defined vocabulary (`run_start`, `run_end`, `enter_room`) | WP8b | — |

### 8.4 Converter requests (gates for milestones)

Checked against `research/build/packages/rely_the_needle/` on 2026-10-24. The client never edits converter output; it accepts the current output only through `LoadMode::Trace` and the legacy physics mapping.

| Id | Request | Evidence | Gate |
|---|---|---|---|
| CR1 | Map rely objBLOCK (`@block_2`, `script = "block_2"`, ~600 instances, the floor under the room1 spawn) to the `block` preset with `visual_spawns_collider` and `collider` (the plain block kind it spawns; `kid_collides = false` on the visual). CR16 to CR18 (tier 1 and tier 2 built-in params, kind references): `research/specs/converter-requests.md` | `objects/block_2.toml`; only 19 plain `block` instances in the package | M1 |
| CR2 | Map `warp_start` to `difficulty_warp` with `dif` from creation code, or mark it inert-safe | `objects/warp_start.toml`, placed in rSelectStage | M1b |
| CR3 | `status = "infra"` objects (world, init, blood, bow, GAMEOVER, bloodEmitter, playerface, menus) become built-ins or are dropped, never `script =` | `objects/world.toml`, `init.toml`; trace-map `kind = ""` rows | M1 |
| CR4 | `object_order` lists every spawnable engine kind with its original index (bow 25, GAMEOVER 51, blood 11, bloodEmitter 175, playerface 608 in rely) | trace-map rows with `kind = ""` | M1 |
| CR5 | Instance creation code to params for warps (`roomTo`, `warpX`, `warpY`) | room_1 "1 instance with creation code, not converted" | M3a |
| CR6 | Room `role` (boot, title, menu, select, play) | 4.1 | M1 (defaults to play) |
| CR7 | `start_room` stays the title for game select art, plus `[engine] new_game_room = "room_1"` | `start_room = "title"` | M3b |
| CR8 | physics.toml in the 5.2 form | 5.2 | after WP5a |
| CR9 | Sound kind and length in ticks (ffprobe duration × 50); also for files a game loads through SuperSound or GMFMODSimple, which sit in `sounds/` or `music/` and are named by stem (`GameDef::dll_sound_length`, #288) | section 7 | WP10 tests on packages |
| CR10 | Kind names and object bases that are not built-ins of 6.1 (they load inert in Trace mode) | loader warning CR10 | M3a |
| CR11 | Write `counts_as` instead of the legacy object key `roles` | loader warning CR11 | M3a |
| CR12 | `[defaults.<kind>]` sprites for bare built-ins in game.toml (not only in tests/trace-map.toml) | loader warning CR12 | M3a |
| CR13 | Room `file_speed` in each room file (not only in tests/trace-map.toml) | loader warning CR13 | M3a |
| CR14 | `room_order` in game.toml (not only in tests/trace-map.toml) | loader warning CR14 | M3a |
| CR15 | Sprite strips at most 4096 px on a side (split or re-pack wider strips) | loader warning CR15 | M3a |
| CR16 | Tier 1 params per built-in: `block` `kid_collides`, `slip`, `mask`; `kid_start` `shift_at`, `grav_gated`, `creates`, `offset_x`, `offset_y`; `kid_bullet` `block_check`, `life`; `spike` `dir`, `size`, `auto_rotate`, `destroys_bullets`; and add the built-ins that exist to `known_kinds` | research/specs/converter-requests.md CR16; `docs/format/builtins/{block,kid_start,kid_bullet,spike}.md`; kid/A1.md to A4.md, B.md | M3a (a game's own trace check shows no unknown param for these) |
| CR17 | Tier 2 params: `vine` `side`; `water` `variant`, `fall_cap`, `refresh`, `air_jump`; `moving_platform` `hspeed`, `vspeed`, `bounce`, `carry_rule`; `fake_block`, `hidden_block`, `trigger`, `trap`, `edge_warp`, `drop_platform`, `gravity_flip`, `bullet_blocker`, `slope`, `boss_gate`, `boss_item`, `secret_item`, `item_viewer`, `game_clear`, `music` params; objects a built-in cannot express yet stay unmapped | research/specs/converter-requests.md CR17 (client PRs #253, #257, #266, #271); `docs/format/builtins/<kind>.md` | M3a |
| CR18 | A kind-typed param (`collider`, `watched_object`, `trigger_kind`, `group`) is the id of a preset written `"@id"`, or a bare built-in name; an unknown bare name is inert in Trace mode (CR10) and an error in Strict mode | research/specs/converter-requests.md CR18; loader error for a missing target | M3a |
| CR19 | Write the params the built-ins take: `moving_platform` kind instead of `platform[moving]`; `save` `min_difficulty_removed` (Medium 0, Hard 1, in A3 games 2, VHard 2) instead of `difficulty`; bare `slope` instead of `dir`; `gravity_flip` `direction = "up"` or `"down"` instead of `up`. Sounds and music share one id space: a sound and a music track with the same name become `death` and `death_2` | loader warning `unknown param` (research#57); `docs/format/builtins/{moving_platform,platform,save,slope,gravity_flip}.md`; research/specs/converter-requests.md CR19 | M3a (`pkg check --trace-mode` on the 20 packages shows no `unknown param` and no duplicate sound or music id; #142) |

## 9. Trace comparison and replay (`crates/tools`, WP8a, WP8b)

Spec: CLONE_SPEC traces and Verification sections (recorder sample point, `--room`, `--from`, `--gml`, `--stub-dlls`); architecture "Comparing traces with the original"; existing `sim::trace`.

### 9.1 What is recorded, and how each kind is used

About 250 CSVs exist (2026-10-24), more arriving: kaenbyou 56, deride-kotaro-from-aslant 32, little-runner 29, machinary 27, animus 20, rely-the-needle-4-2 17, little-trap 11, go-the-spook-house 10, eat-the-lemon 9, help-the-fairy-tale 9, go-the-jun-g-world 8, poronkusema 7, clear-the-easy-miku-2 6, eat-the-rori-girl 6, eat-the-new-year 5, get-the-rainbow-feather 3, showdown 1, keep-celebrating-for-xia 0. Kaenbyou traces carry `.dllcalls.txt` and use `--stub-dlls`.

Three kinds, sorted by `iwbtg trace import` from the header command line:

1. **Diff targets.** `--room`, `--from`, `--x/--y`, and `--gml` that only assigns variables (`--gml 1:global.difficulty=1`, before room entry, or at a frame). Columns are GM variables, `instance_number(...)`, `keyboard_check*(...)`. These become case files and must diff clean.
2. **Probe traces that settle spec items.** `--gml` with `object_event_add` or log strings (`t-ord-1`, `t-evo-1`, `t-room-1`, `tr-room-01-a`, `zz-*`). Our sim cannot run injected GML. Research reads these and updates the spec; WP11 then writes a golden test with the `Probe` built-in that reproduces the spec's stated result.
3. **Probe-read traces.** `--gml` that reads and decodes files into globals (`famb-save-restart-*` decrypts the save into `global.tr_*`). These map to probe columns on `SaveSlot` fields, not to setup.

### 9.2 Case files

Case files live in the client repo at `crates/tools/cases/<package-id>/<case>.toml` (they hold package ids and research trace names, no creator content), written by `iwbtg trace import` and finished by hand.

```toml
source = "kaenbyou/t-alarm-1"          # research/specs/engines/gm8/traces/<source>.{csv,inputs}
start = { room = "template", difficulty = 0 }   # StartPoint::Room, mapped from rTemplate by trace-map
first_frame = 2                         # recorder --from
object = "kid_bullet"                   # our kind for the traced original object
dll_mode = "stub"                       # recorder --stub-dlls
setup = [
  { frame = 0, global = "difficulty", value = 1.0 },   # frame <= first_frame: applied before room entry
  { frame = 3, target = "kid", set = { x = 96.0, y = 567.4, xprevious = 96.0, yprevious = 567.4 } },
]
vars = { "alarm[0]" = "alarm0", "player.x" = "kid.x", "player.y" = "kid.y" }
ignore = ["id", "sprite_index"]
```

- `setup` is a fixed vocabulary (set a field of the kid, of the first instance of a kind (a kind names its loader-derived variants too, `save[min_difficulty_removed=2]` is a `save`, as in the probe's `kind.var` reads), or of a named instance; set a global; set a `SaveSlot` field). It replaces `--gml` assignments, `--x` and `--y`. Entries with `frame <= first_frame` run before `start_game` enters the room.
- Frame mapping: the row at `first_frame` is the state after the first tick in the room (kaenbyou t-speed-1). Our run treats tick k as frame `first_frame + k`. Inputs come from the `.inputs` file by frame number through the engine key map (`KeyMap`).

### 9.3 Probe and field registry

```rust
// crates/sim/src/probe.rs (WP8a): read-only sampling, compiled into every build
pub struct Probe { pub object: Target, pub columns: Vec<Column> }
pub enum Column { Fixed(FixedColumn), Field { target: ProbeTarget, field: FieldRef }, Global(VarId),
                  Count(Target), Key(KeyRead), Save(SaveField), Engine(EngineVar) }
pub enum KeyRead { Held(Action), Pressed(Action), Released(Action), Direct(Action) }
pub fn sample(sim: &mut Sim, probe: &Probe, frame: i64) -> Vec<Vec<trace::Value>>;
// crates/sim/src/probe/registry.rs (WP8a): GM name -> field
pub fn resolve(name: &str, def: &GameDef) -> Option<FieldRef>;   // "djump", "curJumps", "jump[1]", "alarm[0]", "image_index", ...
```

- `Engine(EngineVar)` reads engine state the package does not declare as a variable: `global.difficulty`, `death`, `time`, `savenum`, `clear`, `frozen`, `frozen2`, `reverse`, `grav`, `paused` and `room_speed`. These names win over a package variable of the same name (a save point's `difficulty` parameter interns one). Audio settings (`global.audio_*`, `stream_music`) are not sim state; cases list them in `[unprobed]`. `grav` on a family B kid reads the profile gravity (kid/B.md: declared, never read); `saveTimer` on a `save` instance reads its cooldown timer. `save.<name>` is a save slot field when it is one, else a field of the first `save` instance.
- Kid fields resolve through `kid::api::read_field` (1.6), so WP8a never depends on `KidState` layout; save fields through `SaveField`.
- `KeyRead::Direct` **consumes** the latch, because the recorder's own `keyboard_check_direct` column read it at the sample point and so changed the original run the same way. `sample` therefore takes `&mut Sim`. Whether the recorder's read really consumed it is checked against `t4-input-direct-tap-*` and `t5-input-restart-latch-*` (I1).
- Sample point: after phase 13, before the room change (2.1). One row per alive instance of the traced kind, in dispatch order. Rows only for frames the original has.
- Real columns compare bit for bit (`trace::diff`). `--stages` records the probe after every phase.

### 9.4 Commands

```text
iwbtg trace run    <pkg> <case> [-o ours.csv] [--stages]          (WP8a)
iwbtg trace diff   <original.csv> <ours.csv> [--map <pkg>/tests/trace-map.toml] [--context 5]
iwbtg trace check  <pkg> [--research <dir>]   # every case; non-zero exit on any diff
iwbtg trace import <research-trace-path> <pkg> # case skeleton; reports its kind (9.1)
iwbtg pkg check    <pkg> [--trace-mode]
iwbtg replay run   <pkg> <replay|.inputs> [--start new-game --difficulty N]   (WP8b)
```

Exit codes: 0 success, 1 a check failed, 2 a tool error, 3 the sim reached a WP0 stub of a work package not merged yet.

`replay run` prints the tick count, splits from the `[timing]` triggers, and the final state hash (roadmap Phase 3). `crates/sim/src/timing.rs` (WP8b) evaluates triggers at phase 16.

```rust
// crates/sim/src/setup.rs (WP8a), behind cargo feature "trace-setup"
#[cfg(feature = "trace-setup")]
impl Sim { pub fn apply_setup(&mut self, s: &SetupAction) -> Result<(), SimError>; }  // sets World::setup_applied = true (hashed)
// crates/tools/src/trace_cmd.rs
pub fn run_case(pkg: &PackageFiles, case: &TraceCase, research: &Path) -> Result<TraceTable, ToolError>;
```

## 10. Determinism and purity

### 10.1 State hash

- `hash::state_hash(&World) -> [u8; 32]` is BLAKE3 over the **postcard** encoding of a canonical `HashView` struct borrowed from `World` (every hashed field in a fixed order; postcard writes lengths as varints, so wasm32 and x86_64 give the same bytes). `std::hash::Hash` and its hashers are never used for state hashing: slices write their length with `write_usize` (4 bytes on wasm32, 8 on x86_64, native byte order), and `DefaultHasher`/`RandomState` are unspecified across Rust versions. `num/mod.rs` (WP2) drops the "may be built from either" note; `Real`'s `Hash` impl stays only for test maps.
- `HashView` includes `setup_applied` (10.4) and excludes `output` and the grid.
- Checkpoints in replays: the first 16 bytes (architecture "Replays").

### 10.2 NaN and infinity

`Real`'s `Eq` compares raw bits, and NaN payloads and signs differ between x86_64, wasmtime and browser engines. Built-ins can make non-finite values (`sqrt` of a negative, `x/0` such as machinary's `range / Hand_speed` alarms, `atan2` of infinities). Rule (Design, N15): every `Motion`, `Image`, alarm, view and variable setter, called from a built-in or a script, rejects a non-finite value with `SimError::NonFinite`, which halts the sim (2.2). Intermediate non-finite values inside one `num` function are allowed only if the function's result is finite. A golden test reaches this path on purpose (10.6).

### 10.3 Integer overflow

- `rng.rs` uses `wrapping_mul`/`wrapping_add` (the LCG wraps by definition).
- Counters (`tick`, `room_tick`, `next_id`, deaths, time, grid math) use `checked_*` and return `SimError::Overflow`.
- `[profile.release] overflow-checks = true` in the root `Cargo.toml`, so debug tests and release servers behave the same.

### 10.4 Features

Cargo resolver 3 still unifies features across members built in one command, so a server built next to tools would get `trace-setup`. Rules: the server binary refuses to start when `cfg!(feature = "trace-setup")`; CI builds the server with `-p`; `HashView` carries `setup_applied`, so a run that used setup can never match a clean verification.

### 10.5 Lints (`crates/sim/clippy.toml`, WP0)

Added to the existing bans:

- `disallowed-methods`: `f64::min`, `f64::max` (signed-zero result unspecified, lowered differently on x86 and WASM; use `num::gm_min/gm_max` or `num::min_bits`), `f64::clamp` (panics on NaN or lo > hi), `f64::to_degrees`, `f64::to_radians` (use `num::radtodeg/degtorad`), `std::io::stdin`, `std::fs::read`, `std::fs::write`, `std::fs::read_to_string`, `std::fs::File::open`, `std::fs::File::create`, `std::process::exit`.
- `disallowed-types`: `std::fs::File`, `std::net::TcpStream`, `std::net::TcpListener`, `std::net::UdpSocket`, `std::process::Command`, `std::hash::DefaultHasher`, `std::hash::RandomState`.
- `lib.rs`: `#![deny(clippy::print_stdout, clippy::print_stderr, clippy::float_cmp, clippy::cast_possible_truncation)]`; WP0 fixes any existing hits in `num`, `input`, `replay` and `trace`. Existing `#[cfg(test)]` uses of `std::fs` in `trace.rs` get a scoped `#[allow]` with a reason.

### 10.6 Cross-target checks

Golden engine rooms (`crates/sim/tests/golden/*.rs`, built with `def::testing::GameDefBuilder`) store a hash list per tick. They run on Windows x86_64, Linux x86_64, wasm32-wasip1 under wasmtime, **and** wasm32-unknown-unknown under node via `wasm-bindgen-test` (the shipped web target: different NaN bits, panics abort). One golden case reaches the 10.2 non-finite path and expects `Halted(NonFinite)` on all four. CI owner: WP11.

## 11. Scripts (WP9)

Spec for the language: [0001](../decisions/0001-script-language.md). GML rules the ported scripts must keep come from CLONE_SPEC numbers.

- Rhai setup per 0001: features `std`, `only_i64`, `no_time`, `no_module`, `no_custom_syntax`, `no_closure`, `compact_str`, `internals` on `=1.27.0` (pinned by #90 on or after 2026-11-04, when 1.27.0 was 14 days old). WP0 changes the root `Cargo.toml` line; WP9 owns it afterwards.
- `Engine::new_raw()` with only our packages; `eval`, `**`, `**=` disabled. Done-criteria check both.
- **Both operands always evaluated** (Confirmed for `&&`, `||`, `^^`). Rhai short-circuits `&&`/`||`. The IWS validator rejects any call that can draw from the RNG or change state inside an operand of `&&` or `||` and suggests Rhai's non-short-circuit `&`/`|` on bools, or evaluating first into a variable.
- `if` and `repeat` on reals: ported code calls `gm_bool(x)` and `gm_repeat_count(x)` (rounded, Confirmed), which read `NumRules`.
- Single `=` inside a GML expression is a comparison (Inferred); in Rhai it is a parse error, so porters write `gm_eq`.
- Comparisons of reals go through `gm_eq`..`gm_ge` (NumRules), as 0001 already requires.

## 12. Testing strategy

| Module | Unit tests | Golden / trace tests |
|---|---|---|
| `num` (WP2) | existing suite, rewritten for `NumRules`; every `NumRules` field both ways; `gm_div` truncation table (`-7 div 2 = -3`, `-7 mod 2 = -1`); slip sequence (3.0000000000000004 peak, then ±5.55e-17 creep under exact compare and sign, stop under epsilon); `alarm_from_real`, `byte_from_real`, `gm_string_real` tables; `dcos(270) * 0.4 == -7.347880794884119e-17` | num-* traces when the spec records them |
| `rng` (WP2) | sequence from a known seed; wrapping at 2^32; one draw per call; `choose` bounds | num-rng (not recorded) |
| `motion` (WP2) | coupling bits (direction 180 and -0.0 cases); friction sign cases; gravity vector at 0/90/180/270; fall 0.4k table; non-finite setters return errors | T-MOT-* |
| `collide` (WP3) | kid bbox at y = 567.0, 567.4, 567.5, 566.5; spike triangle pixel counts; overlap symmetry; `move_contact_solid` steps; `distance_to_object` flush case = 1 under the chosen C13 setting; grid vs brute force on 10,000 random placements, also on wasm32 | T-COL-* |
| `world`, `events`, `tick` (WP1) | ids monotonic, sweep keeps order, every `DispatchOrder`, snapshot excludes new instances, `AbortTick`, alarm table, both `anim_slot`s, draw phase depth order and visible filter, latch semantics (`direct` consumes), pair order both settings, recheck switches, hash identical across two runs and with/without grid | t-alarm-1, t-anim-1 |
| `room` (WP4a), `save`, `view` (WP4b) | 7-step order by probe built-ins, persistent restore, deactivated persistent left behind, nested request both settings and the loop bound, each restart model's step list and trigger phase, `quantize` per model, view follow | TR-ROOM-* when the spec records them |
| `kid` (WP5a/b/c) | each action alone on a hand-built world: release cut, fall caps, platform snap and .0/.4 cycle, vine distances, B curJumps, flip swap, first-play save | kid trace cases via WP11 |
| built-ins (WP6a/b/c) | one test per built-in event; blood draw interleaving; blood emitter Step count under both R9 settings | per-mechanic traces |
| sound (WP10) | length expiry, MULTIMEDIA single copy, stop_all, Stub mode returns | audio-* traces (as diff targets of sound columns) |
| `format` (WP7a/b) | fixtures, mask building, `run` expansion order, rank selectors, spawnable-kind ranks, sorted var ids, every converter package loads in `Trace` mode (gated by `IWBTG_RESEARCH`) | — |
| tools (WP8a/b) | case parsing, kind sorting of imports, frame mapping, pre-entry setup, column translation, direct-read probe consumes, CSV round trip, replay run on a golden package | `iwbtg trace check` (WP11) |

Each WP's tests live in its own files (`crates/sim/tests/wpN_*.rs` or module `tests.rs`) so worktrees do not collide.

## 13. Work packages

Every WP ends with the user check the client `CLAUDE.md` requires. Commands assume the `client/` folder; `<research>` is the research repo path.

### WP0: Contracts, scaffold, purity config

- **Owns (creates or edits):** `crates/sim/src/lib.rs` (all `pub mod` lines and crate lints), root `Cargo.toml` (`[profile.release] overflow-checks = true`, Rhai feature line per section 11, `smallvec` pin), `crates/sim/Cargo.toml` (`smallvec`; features `trace-setup`), `crates/sim/clippy.toml` (10.5), `def/{mod,ids,class,game,kind,room,sprite,builtin,physics,sound,timeline,path,testing}.rs` (types only), `rules/{mod,tick,motion,collide,room,num,sound}.rs` (every register field with its default), `world/{mod,instance,store,api}.rs` (structs plus a working minimal `Vec` store: insert, get, iter, sweep), `events.rs` (enums, `EventSet`, `ProbeState`), `output.rs`, `error.rs` (`SimError` with `NonFinite`, `Overflow`, `Unsupported { what }`, `Script`), stub files with signatures and `unimplemented!("WPn: name")` bodies for every function in sections 1-11, the `Action` additions in `input.rs` (Pause, Mute, Reset) and their round-trip in `replay.rs`, `docs/architecture.md` (tick table to 2.1, RNG consumers in sim, sound model in sim, hash encoding 10.1, NaN rule 10.2).
- **Public API:** the types and signatures in sections 1-11; `Sim::{new(def, SimOptions), step, world, def, state_hash}`; `SimOptions { start: StartPoint, seed: u32, dll_mode: DllMode }`; `RunnerRules::for_runner(Runner, engine_rev: u16)`; `GameDefBuilder` (sprite with rect or bits mask, kind, room with instances, physics, engine kinds).
- **Depends on:** nothing. Merged alone, first. Keep it to declarations: no behavior beyond the minimal store.
- **Tests:** `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, a doc test that builds a one-room `GameDef`, a test that a banned call (`f64::max`) is rejected by clippy (documented, run by hand once).
- **Done when:** the workspace builds on Windows, Linux and wasm32; clippy is clean; every later WP can write its code without editing a file it does not own. Ownership then passes as listed in each WP.
- **User check:** `cargo build --workspace` then `cargo test --workspace`. Expected: both finish, last line `test result: ok`. Broken: any red `error[...]` line.

### WP1: World, events, tick pipeline, state hash

- **Owns:** `world/` (after WP0), `events.rs`, `tick.rs`, `alarm.rs`, `anim.rs`, `timeline.rs`, `hash.rs`, `input_state.rs` (`InputState`, latch, `direct`), `sim.rs` (bodies of `Sim` methods), `rules/tick.rs`, `tests/wp1_*.rs`.
- **Public API:** `Ctx::{spawn, spawn_engine, destroy, change_kind, request_room, emit}`; `World::{get, get_mut, exists, iter_alive, first_of, count_of, kid, sweep}`; `events::{dispatch_order, dispatch, run_phase, run_draw_phase, collision_phase, event_set}`; `alarm::{tick, set}`; `anim::advance`; `timeline::advance`; `InputState::{latch, held, pressed, released, direct, clear}`; `hash::state_hash`.
- **Depends on:** WP0. Calls WP2-WP10 through WP0 stubs; tests use the `Probe` built-in (logs every event; can move, spawn, destroy, request a room).
- **Spec:** CLONE_SPEC tick loop (all), rooms "Instance creation and destruction", sprites "Per-step animation update" and placement, input Inferred (latch), collision Inferred (order of collision events, re-check).
- **Tests:** section 12 row.
- **Done when:** every phase and rule in section 2 is implemented or switchable; a probe world runs 10,000 ticks with identical hashes on the four targets of 10.6.
- **User check:** `cargo test -p iwbtg-sim wp1`. Expected `test result: ok`. Broken: any `FAILED`.

### WP2: Numbers, RNG, motion, paths

- **Owns:** `num/` (all), `rng.rs`, `motion.rs`, `path.rs`, `rules/{num,motion}.rs`, `def/{timeline,path}.rs`, `tests/wp2_*.rs`.
- **Public API:** `NumRules` functions (3.2), `Motion`, `apply_auto_motion`, `path::{start, advance, end}`, `Rng`.
- **Depends on:** WP0.
- **Spec:** CLONE_SPEC numbers (all), motion (Confirmed, Inferred, U1-U9), INVESTIGATION last section.
- **Work beyond new code:** move compare, truth, sign, min/max ties, round −0 and floor/ceil zero rules from consts into `NumRules`; change `gm_div` to truncate; replace every OpenGMK or UCRT doc comment in `num/` with its register id (N1, N2, N4, N11-N13, M6); `wrapping_*` in the LCG.
- **Done when:** section 12 rows pass; `grep -ri "opengmk\|ucrt" crates/sim/src/num` finds only register references.
- **User check:** `cargo test -p iwbtg-sim wp2` and `cargo test -p iwbtg-sim num`. Expected `test result: ok` twice.

### WP3: Collision

- **Owns:** `collide/{mod,bbox,xform,overlap,query,contact,grid,resolve}.rs`, `rules/collide.rs`, `tests/wp3_*.rs`.
- **Public API:** section 3.4.
- **Depends on:** WP0.
- **Spec:** CLONE_SPEC collision (all), sprites (mask transforms), numbers (rounding).
- **Done when:** every query works on rect, precise, disk and diamond masks at unit scale, mirror and right angles; grid equals brute force on all targets; C13 switches exist with the flush-kid test.
- **User check:** `cargo test -p iwbtg-sim wp3`. Expected `test result: ok`.

### WP4a: Room entry and room change sequence

- **Owns:** `room.rs` (sequence, `start_game`, `perform_change`, nested requests), `rules/room.rs`, `tests/wp4a_*.rs`.
- **Public API:** `RoomRequest`, `StartPoint`, `start_game`, `perform_change`.
- **Depends on:** WP0; calls `events::run_phase` (stub until WP1).
- **Spec:** CLONE_SPEC rooms (room change requests, room start sequence, persistence, Inferred items, UNKNOWN on creation code), tick loop U8, U9.
- **Done when:** the 7 steps, boot instances, deactivated-persistent rule, nested-request switch with loop bound, `DestroySelf` as a no-op (R24); room changes only at phase 15.
- **User check:** `cargo test -p iwbtg-sim wp4a`.

### WP4b: Restart models, saves, views

- **Owns:** `restart.rs`, `save.rs`, `view.rs`, `run_globals.rs` (`RunGlobals`, declared empty by WP0), `tests/wp4b_*.rs`.
- **Public API:** `RestartModel` behavior per 4.3, `ResetModel`, `SaveSlot`, `SavedPosition`, `save_now`, `save_counters`, `quantize`, `load_into`, `View`, `view::follow_update`.
- **Depends on:** WP4a; WP2 (`NumRules` conversions); WP3 (`place_free` for the xia nudge).
- **Spec:** rooms (game_restart, save files, views), kid/A3.md showdown Restart, kid/B.md R handler, mechanics `camera`.
- **Done when:** four restart kinds with their step lists and three triggers; six quantize models; view follow; the showdown conflict noted in code (R26).
- **User check:** `cargo test -p iwbtg-sim wp4b`.

### WP5a: Kid core and family A1

- **Owns:** `kid/{mod,state,api,presets,death}.rs`, `kid/actions/{common,a1}.rs`, `kid/collisions/{block,platform}.rs`, `def/physics.rs`, `docs/format/physics.md`, `docs/format/schema/v0/physics.schema.json`, `docs/format/builtins/kid.md`, `tests/wp5a_*.rs`.
- **Public API:** `kid::{on_event, event_set, presets::apply}`, `kid::api` (1.6), `PhysicsProfile` serde, `KidState`, `KidGlobals`, `KidAction`/`KidCollisionRun` enums (every name in 5.2 and 5.3, unimplemented ones return `Unsupported` naming the row).
- **Depends on:** WP2, WP3; WP4b `save_now` for `first_play_save` (stub until merged).
- **Spec:** kid/A1.md (all), mechanics "Physics profile fields", tricks.
- **Done when:** every 5.3 row marked 5a works on hand-built worlds; rely's profile in the 5.2 form loads; CR8 note written for the converter.
- **User check:** `cargo test -p iwbtg-sim wp5a`.

### WP5b: Kid family B

- **Owns:** `kid/actions/b.rs`, `kid/collisions/b.rs`, `kid/state_b.rs`, `tests/wp5b_*.rs`.
- **Depends on:** WP5a.
- **Spec:** kid/B.md (all).
- **Done when:** every 5.3 row marked 5b works; kaenbyou and eat-the-lemon profiles load; multi-kid rules.
- **User check:** `cargo test -p iwbtg-sim wp5b`.

### WP5c: Kid families A2-A5 and C

- **Owns:** `kid/actions/{a2,a3,a4,a5,c,slope,ladder,ice,flip}.rs`, `kid/collisions/{slope,moving_block}.rs`, `kid/state_ext.rs` (`KidStateExt`, `KidGlobalsExt`), `def/physics_ext.rs` (`[ice]`, `[slope]`, `[ladder]`, `[flip]`), `tests/wp5c_*.rs`.
- **Depends on:** WP5a.
- **Spec:** kid/A2-A5.md, C.md.
- **Done when:** every 5.3 row marked 5c works or returns `Unsupported` naming its row (tier 3 only); all 20 profiles load.
- **User check:** `cargo test -p iwbtg-sim wp5c`.

### WP6a: Built-ins tier 1

- **Owns:** `builtins/{mod,state,world,kid_start,block,spike,killer,kid_bullet,blood,blood_emitter,game_over,bow,save,warp,difficulty_warp,camera,platform}.rs`, `def/builtin.rs` (after WP0), `docs/format/builtins/*.md` for these, `tests/wp6a_*.rs`.
- **Public API:** `builtins::{on_event, event_set, param_specs, init_state}`, `BuiltinState`, `MechGlobals`, `BuiltinOutput`.
- **Depends on:** WP2, WP3, WP4b (restart and save calls), WP5a `kid::api` (stub until merged), WP10 for world mute and music calls (stub).
- **Spec:** section 6 rows, rooms, numbers hidden consumers, sprites (bow, blood count).
- **Done when:** the rely slice (6.2) first, then the rest of tier 1, each with a hand-built-world test and a doc page.
- **User check:** `cargo test -p iwbtg-sim wp6a`.

### WP6b, WP6c: Built-ins tier 2 and tier 3

- **Owns:** one `builtins/<kind>.rs` and doc page per built-in of the tier (its four entry points, section 18), one variant each in `builtins/state.rs`, `tests/wp6b_*.rs` / `wp6c_*.rs`. Dispatch in `builtins/mod.rs` is already routed by WP0.
- **Depends on:** WP6a (WP6c also WP9 for `scripted`).
- **Done when:** tier complete for its games, or `Unsupported` naming the built-in.
- **User check:** `cargo test -p iwbtg-sim wp6b` (and `wp6c`).

### WP7a: Loader: sprites, masks, rooms, objects, order

- **Owns:** `crates/format/src/{load,sprites,masks,rooms,objects,order,vars,tracemap}.rs`, the `[engine]` and `[timing]` parts of `crates/format/src/manifest.rs` (step 1), `crates/format/tests/wp7a_*.rs` and fixtures, `docs/format/v0.md`, schemas under `docs/format/schema/v0/` except physics.
- **Public API:** 8.1 (WP7a part), `LoadMode`.
- **Depends on:** WP0 (`def` types).
- **Done when:** steps 1-6 and 8-9 of 8.2; every converter package loads in `Trace` mode with only warnings; spawnable-kind rank check; v0.md documents every 8.3 addition and the 8.4 requests are filed with research.
- **User check:** `cargo run -p iwbtg-tools -- pkg check <research>/build/packages/rely_the_needle --trace-mode`. Expected: `OK` plus warnings that name CR1-CR5. Broken: an error or a panic.

### WP7b: Loader: physics, params, timelines, paths, sounds

- **Owns:** `crates/format/src/{physics,params,timelines,paths,sounds}.rs`, `crates/format/tests/wp7b_*.rs`.
- **Depends on:** WP5a (`PhysicsProfileDraft`, `presets::apply`), WP6a (`param_specs`), WP10 (`SoundDef`).
- **Done when:** step 7 of 8.2; legacy profiles map with warnings; N14 constant check against traced bits for the values a trace shows; `required` trick validator.
- **User check:** `cargo test -p iwbtg-format wp7b`, then the WP7a command, now without physics warnings for rely.

### WP8a: Trace runner, probe, setup, CLI

- **Owns:** `crates/tools/src/{main,trace_cmd,case,import,pkg_cmd}.rs`, `crates/tools/tests/*`, `crates/tools/cases/` (format and the importer; case contents are WP11's), `crates/sim/src/{trace.rs,probe.rs,probe/registry.rs,setup.rs}`, `crates/sim/src/input.rs` (parser and `KeyMap`, after WP0).
- **Public API:** 9.2-9.4 (trace part), `TraceCase`, `Probe`, `sample`, `Sim::apply_setup`.
- **Depends on:** WP0; WP7a for real packages (tests use `GameDefBuilder` and a fake trace map until then).
- **Done when:** `trace run/diff/check/import` work; import sorts every recorded rely and kaenbyou trace into the three kinds of 9.1; setup before room entry; `Direct` probe columns consume.
- **User check:** `cargo run -p iwbtg-tools -- trace import <research>/specs/engines/gm8/traces/kaenbyou/t-alarm-1 kaenbyou`. Expected: a case file path and the line `kind: diff target`. Broken: a panic or "unsupported header".

### WP8b: Replay runner and timing triggers

- **Owns:** `crates/sim/src/timing.rs`, `crates/tools/src/replay_cmd.rs`, `tests/wp8b_*.rs`.
- **Depends on:** WP8a, WP4b.
- **Done when:** `iwbtg replay run` prints ticks, splits and the final hash for a golden package and for `.inputs` files; trigger vocabulary documented in v0.md through WP7a.
- **User check:** `cargo run -p iwbtg-tools -- replay run crates/sim/tests/golden/one_room.pkg crates/sim/tests/golden/one_room.inputs`. Expected three lines: `ticks: N`, `splits: ...`, `hash: <hex>`; the same hash on a second run.

### WP9: Script host (minimal, IWS profile)

- **Owns:** `crates/sim/src/script/{mod,host,api,budget,validate}.rs` (`ScriptRuntime` in `host.rs`), `def/script.rs`, the root `Cargo.toml` Rhai line after WP0, `tests/wp9_*.rs`.
- **Public API:** `trait ScriptHost { fn on_event(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, ScriptError>; }`, `script::on_event`, `ScriptState`, the validator.
- **Depends on:** WP1.
- **Spec:** section 11; 0001.
- **Done when:** `Engine::new_raw()`, `eval`/`**`/`**=` disabled, no `no_time` gaps (a test that `timestamp()` does not exist); a scripted object with `create`, `step`, `alarm`, `collide` runs in a golden room; the `&&`/`||` RNG lint rejects a sample script.
- **User check:** `cargo test -p iwbtg-sim wp9`.

### WP10: Sound model and DLL adapters

- **Owns:** `crates/sim/src/sound.rs`, `sound/{supersound,gmfmod,cleanmem}.rs`, `def/sound.rs`, `rules/sound.rs`, `tests/wp10_*.rs`.
- **Public API:** section 7.
- **Depends on:** WP1.
- **Spec:** CLONE_SPEC sound (Inferred contract, UNKNOWN list).
- **Done when:** model, both adapters in `Model` and `Stub` mode, S-rows switchable, hashed state.
- **User check:** `cargo test -p iwbtg-sim wp10`.

### WP11: Integration and milestones

- **Owns:** `crates/tools/cases/<pkg>/*.toml` contents, `crates/sim/tests/golden/`, `crates/sim/tests/milestones/`, `rules/mod.rs` and `def/class.rs` after WP0 (reserved class bits), CI target matrix (10.6), the register defaults when the spec records a trace result (one PR per settled item, new `engine_rev` if any package was verified).
- **Depends on:** whichever WPs a milestone needs (section 0).
- **Done when (per milestone):** M1, M1b, M2, M3a as listed; probe traces of 9.1 kind 2 have a golden test once the spec records their result.
- **User check (M1):** `cargo run -p iwbtg-tools -- trace check <research>/build/packages/rely_the_needle --research <research>`. Expected: `stand OK`, `djump OK`. Broken: a diff block naming a frame and column, or "no cases found".

### File overlap check

- Only WP0 edits `lib.rs`, `crates/sim/Cargo.toml` and `clippy.toml`. The root `Cargo.toml` is WP0's, then its Rhai line is WP9's.
- Shared enums and structs are split by owner (1.5, 1.6): no WP edits another's state, output or globals type.
- `def/`: WP0 creates; then `physics.rs` WP5a, `physics_ext.rs` WP5c, `script.rs` WP9, `builtin.rs` WP6a, `sound.rs` WP10, `timeline.rs`/`path.rs` WP2, `class.rs` WP11, the rest WP7a.
- `crates/format/clippy.toml`: WP0, then WP11 (keep its float list in step with `crates/sim/clippy.toml`).
- `rules/`: `tick` WP1, `num`/`motion` WP2, `collide` WP3, `room` WP4a, `sound` WP10, `mod.rs` WP11.
- `input.rs`: WP0 for the Action additions, then WP8a. `InputState` is WP1's `input_state.rs`.
- Docs: `architecture.md` WP0 (all known updates), later changes by WP11; `physics.md` and its schema WP5a; `builtins/kid.md` WP5a; other `builtins/*.md` WP6a/b/c; `v0.md` and other schemas WP7a.
- A WP that needs a field in another WP's file asks that WP or WP11.

## 14. Dependencies in one picture

```text
WP0 ─┬─ WP1 ─┬─ WP9 ──────────────── WP6c
     │       └─ WP10 ─┐
     ├─ WP2 ─┬────────┼─ WP5a ─┬─ WP5b
     ├─ WP3 ─┤        │        └─ WP5c
     ├─ WP4a ─ WP4b ──┼─ WP6a ── WP6b ── WP6c
     ├─ WP7a ─────────┴─ WP7b
     └─ WP8a ─ WP8b
WP11: M1 = WP1-WP3, WP4a, WP4b, WP5a, WP6a (rely slice), WP7a, WP7b, WP8a + CR1, CR3, CR4
      M1b = M1 + CR2 or Trace load mode;  M2 = M1 + WP5b + WP10;  M3a = M1b + CR5
```

Stub rule: a function from another WP that is not merged yet is called through its WP0 signature. Its WP0 body is `unimplemented!("WPn: <function>")`, so a test that reaches it fails loudly with the WP number.

## 15. UNKNOWN and Inferred register

Every rule the sim needs that the spec marks UNKNOWN, Inferred or Conflict, or that the code took from OpenGMK, with the one place the code isolates it. "Default" is what engine_rev 0 does until the spec records the trace in the last column. Fields live in `RunnerRules` (`rules/`), selected by `RunnerRules::for_runner(runner, engine_rev)`; a changed default after a package was verified becomes a new `engine_rev` branch (architecture "Versions"). "(rec)" marks a trace recorded with the OpenGMK recorder; its result is in CLONE_SPEC. "Confirmed (recorder)" is a recorder result not yet checked against the real runner (research#8). A row whose status is still UNKNOWN or Inferred names what is missing; its default stays the engine_rev 0 choice until a trace settles it (#143).

### Tick and events

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| R1 | When xprevious/yprevious are captured | Confirmed (recorder) U1: at the start of the tick, before Begin Step | `TickRules::prev_capture` | `TickStart` | wanna t-prev-1 (rec), machinary t-mot-02-a rows 85-87 (rec), kaenbyou tr-room-08 rows 5-6 (rec) |
| R2 | Instance order within a phase; whether depth matters | Confirmed (recorder) U2: object index, then creation; depth ignored | `events::dispatch_order`, `TickRules::dispatch` | `RankThenCreation`; alternatives `Creation`, `DepthThenRankThenCreation` | t-ord-1 (rec) |
| R3 | Alarm decrement/fire rule; set to 0; set in own event | Confirmed (recorder) U3: only alarms the object handles count down; a value of 0 becomes -1 without firing; re-armed in its own event: no extra decrement; set by an earlier alarm of the same instance this tick: counted down that tick; set in Step: not counted that tick. An alarm assigned exactly 0 is not traced | `alarm::tick`, `alarm::set` | as the recorder | kaenbyou num-alarm-half-b (rec), jun-g num-eps-alpha rows 45-46 (rec), t-alarm-1, t-alarm-2 (rec) |
| R3b | Fractional alarm values | Confirmed (recorder): round half even | `NumRules::alarm_from_real` | round half even | num-alarm-half (rec), num-eps-alpha (rec) |
| R4 | Key event order | UNKNOWN U11 | `events::key_event_order` | held, press, release; action bit order | none planned |
| R5 | Outside Room / Intersect Boundary slot | Confirmed (recorder) U4: after motion and End of Path, Outside Room first | `TickRules::boundary_slot` | `AfterMotion` | t-evo-1 (rec), t-evo-2 (rec), lr-boundary (rec) |
| R6 | Timeline slot; moments passed when speed > 1 | Slot Confirmed (recorder) U6: after Begin Step, before alarms; moments for speed > 1 UNKNOWN (not traced) | `TickRules::timeline_slot`, `timeline::advance` | after Begin Step | t-evo-1 (rec) |
| R6b | Moment N offset after Set Time Line at 0 (N or N+1 steps) | UNKNOWN (sound) | `timeline::start` | moment N runs N steps after the setting tick | audio-rori-sync (rec), audio-miku-sync-mute (rec) |
| R7 | Animation slot; wrap compare; wrap count; negative speed | Confirmed (recorder): after Draw, `>=`, subtract once; negative speed below 0 not traced (sprites Inferred step 3) | `TickRules::anim_slot = AfterDraw | AfterMotion`, `anim::advance` | `AfterDraw`; `>=`, subtract once | t-anim-1, anim-save-end (rec), anim-switch-g (rec) |
| R7b | Sprite-less instance animates | Confirmed (recorder): advances image_index and never wraps; what Draw shows at those indices is open | `anim::advance` (frames = 0), `TickRules::anim_nosprite` | `NoSpriteAnim::Advance` | little-trap anim-nosprite rows 2-61 (rec) |
| R7c | Drawn and collision subimage after a sprite switch | Inferred | `Image::subimage()` | `floor(index) mod frames` | ANIM-SWITCH |
| R7d | Default image state | Inferred (image_speed 1, scales 1, alpha 1) | `Image::default` | as Inferred | — |
| R8 | What still runs after a room request in the tick | Confirmed (recorder) U8 for room_goto and game_restart: the rest of the tick is skipped; the old room is drawn once only when a transition is set | `TickRules::room_request` | `AbortTick` after the handler | t-room-1 (rec), tr-room-02-c (rec) |
| R9 | New instance runs the current phase | Confirmed (recorder) U10: an instance of the creator's own object created during its turn gets no Step (t-evo-1 row 3) | `TickRules::new_instances_join_phase: JoinPhase` (`Never`, `LaterInOrder`, `Always`) | `LaterInOrder` | tr-room-03-b-log, a3-t17-b, t-evo-1 (rec), BLOOD-COUNT |
| R10 | Instances created during Room Start get Room Start | Confirmed (recorder) U9: no | same snapshot rule | no | t-room-1 (rec), tr-room-14 (rec) |
| R11 | GM 8.1 differences | UNKNOWN U12 | `RunnerRules::for_runner(Gm81)` | same as 8.0 | T-EVO-2 |
| R12 | Order of one instance's collision pairs | Inferred (by the other's object order, then creation) / UNKNOWN: the platform part of T-COL-06 and T-MOT-08 are not run | `CollideRules::pair_order` | `OtherRankThenId`; alternative `HandlerListOrder` | T-COL-06 platform part, T-MOT-08 (research#7), k-platform (rec) |
| R12b | One other matching two handler classes (platform as BLOCK and PLATFORM) | UNKNOWN (C8) | `events::collision_phase` | both fire, handler-list order | T-COL-06 platform part (research#7) |
| R13 | One pending room request, last wins | Inferred / UNKNOWN; only game_restart after room_goto traced | `World::pending` setter | last wins | tr-room-01-a (rec); other pairs not traced |
| R14 | room_speed equals the file speed until world Room Start | Confirmed (recorder): first interval 30, then 50 | `room::enter` | file speed | t-alarm-2 (rec), TR-ROOM-16 |
| R15 | Draw-slot gameplay handlers run every tick | Confirmed need | `Phase::Late` | runs headless | — |
| R16 | Globals survive game_restart; events on restart removal | Globals: Confirmed (recorder), cleared; events on removal UNKNOWN (not traced) | `RoomRules::globals_survive_restart`, `restart_events` | cleared; no events | little-runner tr-room-04 rows 33-34 (rec); TR-MECH-08 |
| R17 | Room change spends a tick; boot tick on retry | Confirmed (recorder): R tick, 1 rInit tick, then the saved room; room changes spend no tick | `RestartModel::boot_ticks` | change between ticks; 1 boot tick | tr-room-01-a rows 42-44 (rec) |
| R17b | Transitions spend no ticks | Confirmed (recorder) | output only | no ticks | tr-room-10-a (rec) |
| R18 | Persistent room revisit: no Create, Room Start yes | Inferred | `room::perform_change` | as stated | TR-ROOM-14 |
| R19 | View follow algorithm and phase | Confirmed (recorder) horizontally: border `min(hborder, view/2)`, follows the same tick's post-move position; camera-object games lag 1 tick; vertical follow UNKNOWN | `view::follow_update` | as the recorder, before Draw slot | animus, eat-the-lemon tr-room-09 (rec), little-runner tr-room-11 rows 258-260 (rec) |
| R20 | Inactive instances and pause (xia) | Confirmed manual; xia io_clear on unpause | `Instance::active`, `InputState::clear` | — | T9 |
| R21 | Which instance is "first" (queries, the kid, view follow, saveGame) | Confirmed (recorder): the save reads the lowest-id kid; `object.variable` and saves read the first in instance order. For one object, lowest id and dispatch order agree | `World::first_of`, `TickRules::first_rule` | dispatch order; alternative lowest id | famB-multikid-restart-b rows 25-26 (rec) |
| R22 | Destroyed-this-step instance seen by collisions and queries | Partly answered: `instance_exists` and `object.variable` skip it (Confirmed (recorder), a3-t18-b row 42); collisions and queries UNKNOWN | `TickRules::dead_in_queries` | false | T-DEAD-1 (research#7) |
| R23 | room_goto during the room change (room creation code) | Confirmed (recorder): 0 ticks in rLoad; Room Start and draw in rLoad not visible | `RoomRules::nested_request` | `ChainAfterSequence`, max 8 | kaenbyou tr-room-12 (rec) |
| R24 | instance_destroy() in room creation code | Confirmed (recorder): destroys nothing, the rest of the code runs | `RoomAction::DestroySelf` | no-op | showdown a3-t18-a rows 42-43 (rec), a3-t20, tr-room-14 |
| R25 | Deferral grain: code action or event action list | Confirmed (recorder): the code action; D&D Go to Room untraced | handler boundary | end of handler (Design) | tr-room-02-c row 13 (rec) |
| R26 | Showdown restart path | Confirmed (recorder): R has no game_restart; the R tick makes a new kid in the old room, the next tick is the reloaded room with the same kid | `GameDef::restart.kind` | `ShowdownLoad` | traces/showdown/a3-t18-a.csv f41-43 (rec) |
| R27 | Restart trigger phase per engine | Confirmed per game (rooms, A3 I7) | `RestartModel::trigger` | per 4.3 table | tr-room-01-a (rec), sh-save-restart (rec) |
| R28 | Runner real-time reads (`fps`, clocks) | Design | world built-in, script API | `fps = tick_rate`; clocks from ticks | — |

### Motion

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| M1 | Friction before gravity; path position | Friction slot Confirmed (recorder): before the move, also on the creation tick; friction vs gravity order UNKNOWN (not traced) | `motion::apply_auto_motion` | friction, gravity, path, move | kaenbyou t-mot-04 (rec), t-evo-1 (rec) |
| M2 | Friction edge cases | Confirmed (recorder) U3: stops at exactly 0; negative friction grows | `motion::apply_friction` | reduce toward 0, clamp at 0; negative grows | kaenbyou t-mot-04 (rec) |
| M3 | Negative speed representation | Confirmed (recorder) U4: speed kept as written, negative included | `Motion::recompute_cartesian` | store speed as written | eat-the-rori-girl t-mot-05 (rec) |
| M4 | Direction when h and v are both 0 | Confirmed (recorder) U5: direction -0.0; a later speed write moves right | `Motion::recompute_polar` | `atan2(-0.0, 0.0)` result | kaenbyou t-mot-06-b (rec) |
| M5 | Polar recompute formula and zero signs | Inferred, Trace-consistent | `Motion::recompute_polar` | `radtodeg(atan2(-v, h))`, +360 if `< 0` | stand, t-alarm-1 |
| M6 | Trig bits for non-table angles: x87 vs libm | UNKNOWN numbers | `num::trig` | libm, table for 0/90/180/270; `atan2` correctly rounded (double-double refinement of libm), snapped to the double below pi/2 when `\|x\| / \|y\| < 1e-13` (cutoff OPEN, between 1.9e-14 and 6.8e-5, traces rely num-rng-b and kaenbyou t-mot-04) | num-trig, kaenbyou land-moving, vines |
| M7 | Solid re-move uses current speeds; "kept at previous place" | Confirmed (recorder) motion U7: "kept at previous place" restores xprevious/yprevious from tick start, undoing the event's and Step's moves; the re-move starts where the event left the instance. C9 not separated | `collide::solid_resolve`, `CollideRules::solid_recheck`, `CollideRules::solid_keep` | add speeds after event; recheck vs `b`; `SolidKeep::Previous` | showdown t-mot-07-e rows 2-8 (rec), machinary kid-a4-long-fall row 96 (rec), t-col-04 (rec) |
| M8 | Path following | UNKNOWN U9, U-MECH-4 | `path::advance` | linear segments by `speed / length`, closed loops wrap | TR-MECH-02 |

### Collision

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| C1 | bbox formula at unit scale | Confirmed (recorder) | `collide::bbox::instance_box` | `round(x) + (left - ox)` | t-col-01..03 (rec) |
| C2 | Rounding mode; tested coordinate vs round-then-add | Inferred / UNKNOWN at an exact .5 in x: the collision box is not the reported bbox | `place_origin`, `CollideRules::place_round`, `box_round` | banker's; round the tested coordinate | t-col-08 (rec), trick-wall-align run d (rec) |
| C3 | bbox under scale, negative scale, angle | Reported box Confirmed (recorder): `bbox::reported_edges`; `prec = false` point, rectangle and line queries test the reported box, mirrored extra pixel included (t-col-09-e rows 4-8), which `collide/query.rs` already does. The collision mask under scale, mirror and rotation is UNKNOWN (precise and pixel collision not logged) | `collide::xform::map_box`, `bbox::reported_box` | exact for ±1 and right angles; scale floors; other angles `Unsupported` | poronkusema t-col-09-a..g (rec), MASK-ROTATE |
| C4 | Mirror rule `d -> -d` vs `d -> -d-1` | Confirmed (recorder) for collisions: `d -> -d`, kid box x-5..x+5 in both facings; the reported bbox_left of a mirrored instance reads x-7 (`DistBoxes::Reported`) | `collide::xform::map_box` | `d -> -d` | MASK-MIRROR, t-col-01 (rec), k-wall-align-left (rec) |
| C5 | Precise alpha `>` vs `>=` | UNKNOWN: t-col-05 cannot tell them apart | `format::masks::ALPHA_IN_MASK` | `>` | needs a new trace |
| C6 | Subimage used for separate masks mid-tick | Inferred; depends on R7 | `Image::subimage()` | current `floor(index)` | cherry traces |
| C7 | Per-pair re-check for solid pairs | Confirmed (recorder) | `events::collision_phase` | re-check | machinary t-col-04 (rec), a1-land-two-blocks |
| C7b | Per-pair re-check for non-solid pairs (killer before or after block event) | Confirmed (recorder): the killer sees the post-block position | `CollideRules::recheck_nonsolid` | true | help-the-fairy-tale t-col-06 (rec), lr-killorder (rec) |
| C8 | Inherited and direct events for the same pair | UNKNOWN | `EventSet::collisions` | both run (R12b) | T-COL-06 platform part (research#7) |
| C9 | Final "still colliding" check: vs `b` or any solid | UNKNOWN motion U7; t-mot-07-e does not separate the two | `CollideRules::solid_recheck` | vs `b` | T-MOT-07 |
| C10 | move_contact_solid from an overlapping start | Confirmed (recorder): no move from an embedded start | `CollideRules::mcs_start` | manual: no move | little-runner t-col-07-d (rec) |
| C11 | move_contact_solid step count, maxdist 0, non-axis | UNKNOWN | `collide::contact::mcs_step_limit` | `floor(maxdist)` steps, 0 = no move | a1-idle-settle, kaenbyou slope room |
| C12 | collision_rectangle/line corners | UNKNOWN | `query::rect_cells`, `line_cells` | inclusive, rounded with C2 | wanna oSpike14a, spook platforms |
| C13 | distance_to_object measure | Confirmed (recorder): sqrt of the squared edge gaps of the rounded reported boxes, touching = 1 | `CollideRules::{dist_boxes, dist_offset, dist_combine}` | `Reported`, `PlusOne`, `Euclid` | trick-vine-kick-and-djump-d rows 108-110 (rec), a3-t16-c row 13 (rec), T-COL-10, bug-right-vine-miss (rec), a-vine-right (rec), famb-vines (rec) |
| C14 | move_bounce_solid and Bounce | UNKNOWN U-MECH-5 | `collide::move_bounce_solid` | `Unsupported` | TR-MECH-04 |
| C15 | Empty manual bbox (left > right) | Inferred | `format::masks` | `MaskShape::Empty` | — |

### Numbers

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| N1 | `==`, `<` exact or epsilon | Confirmed (recorder): tolerance t with 9e-14 < t <= 1e-13; real runner open (research#8); code value from OpenGMK | `NumRules::compare` | `Epsilon(1e-13)` | num-eps-slip-b/-c (rec), num-eps-alpha (rec) |
| N2 | Truthiness `> 0.5` vs `>= 0.5` | Spec infers `> 0.5`; code value from OpenGMK | `NumRules::truth` | `GeHalf` | num-eps-* |
| N3 | FPU precision (53, 64 or 24 bit) | UNKNOWN | `NumRules::float_model` from `[engine]` | f64 | num-fpu-jump |
| N4 | round tie rule; round(−x) to −0 or +0 | Inferred ties-even; −0 rule from OpenGMK | `NumRules::round_neg_zero` | ties even, `+0` | num traces |
| N5 | file_bin_write_byte of a fraction or out-of-range value | Fractions Confirmed (recorder): round to nearest, ties to even (48.5 to 48); out-of-range UNKNOWN (not traced) | `NumRules::byte_from_real` | `ByteConv::RoundWrap`: round half even, wrap mod 256 | num-byte-save (rec), tr-room-05-e (rec) |
| N6 | `string()` two-decimal rounding | UNKNOWN: exact halves go to even, but half even on the decimal value does not fit every value (183.005 gives 183.00, 0.015 gives 0.01) | `NumRules::string_decimals` | round half even on the decimal value (kept until research#100 finds the rule) | num-string-save-b (rec), machinary tr-room-07-b/-c (rec), research#100 |
| N7 | ds_list_write keeps doubles exactly | Confirmed (recorder) | `save::quantize(XyPrevious)` | exact | kaenbyou tr-room-08 rows 51-52 (rec) |
| N8 | div truncates (Confirmed, code fix); div/mod on fractional operands | Confirmed; fractional positive operands Confirmed (recorder); negative operands not traced | `num::gm_div`, `num::gm_mod` | `trunc(a/b)`; `a - (a div b)*b` on reals | little-runner num-divmod-save (rec) |
| N9 | RNG algorithm, scaling sign, start seed, draws per call | Inferred / UNKNOWN | `rng.rs` | Delphi LCG, unsigned, seed from replay, one draw | num-rng |
| N10 | Who draws from the RNG (blood, playerface, particles) | Confirmed consumers; particles partly answered (recorder): room emitters draw (2 draws on most rely room1 ticks, more on spawns); the draw count per emitter is not specified | each built-in; `NumRules::particles_draw_rng` | draws as specs list; particles `false` (placeholder until #556, research#102) | num-rng-a (rec), BLOOD-COUNT |
| N11 | `sign` exact or with epsilon | OpenGMK provenance; slip creep depends on it | `NumRules::sign` | `Epsilon` (follows N1) | num-eps-slip (rec) |
| N12 | `min`/`max` tie and signed-zero rules | OpenGMK provenance | `NumRules::min_max_ties` | current code | num traces |
| N13 | `floor`/`ceil` keep signed zero | OpenGMK provenance (round.rs comments) | `NumRules::floor_ceil_zero` | keep sign | num traces |
| N14 | GML literal parse and converter-computed constants equal GM's values | Confirmed (recorder): digits as one integer times the double nearest 10^-k (research#104); 0.4, 0.45, 0.017, 1/15 are nearest doubles, 31 literals (0.3, 0.95, 0.7, ...) one ulp above; real runner open (research#8) | converter writes the recorder double in shortest round-trip form; built-in constants; `num::gml_literal`; WP7b check | recorder model in the converter; hand-written decimals are the nearest double; computed values computed at run time | 172 release cuts in 14 games bit-exact; showdown a3-t19 tempHspeed 3.0 x 0.95 = 2.85 every step |
| N15 | Non-finite values | Design (spec: division by zero out of scope) | every setter | `SimError::NonFinite` | — |

### Sound

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| S1 | sound_isplaying latency | UNKNOWN | `SoundRules::isplaying_latency` | 0 ticks | audio-isplaying-latency |
| S2 | Second MULTIMEDIA start while one plays | UNKNOWN | `SoundRules::multimedia_second` | stop the first | audio-multimedia-overlap |
| S3 | sound_loop on an already playing MULTIMEDIA sound | UNKNOWN | `SoundRules::loop_playing` | ignore | room creation code loops |
| S4 | game_restart keeps sounds | Inferred | `sound` on `GameRestart` | keeps | audio-restart-carryover |
| S5 | DLL getters return the last value set exactly | UNKNOWN | adapters | exact | audio-supersound-deathfade (rec), audio-fmod-pitchfade (rec) |

### Input

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| I1 | keyboard_check_direct latch (pressed since last direct read, shared per key); recorder's probe reads consume it | Inferred; the recorder has no direct-key latch, so its traces cannot settle it | `InputState::direct`, probe `KeyRead::Direct` | latch on; probe consumes | real runner (research#8); t4-input-direct-tap, t5-input-restart-latch, trick-restart-glitch-b (rec, cannot settle) |
| I2 | Held value at the end of a tick with press and release | UNKNOWN | `InputFrame::held` | as built | T3 |
| I3 | Second Shift press while one is held | UNKNOWN | `sim::input` per-key edges | press counts | T1, T2 |
| I4 | Key repeat sets pressed | Inferred no | client collector | no | T8 |
| I5 | Edges after a blocking pause | UNKNOWN | client pause, `InputState::clear` | drop | T9 |
| I6 | Game-side held flags go stale across death | Inferred | `KidGlobals` key flags | stale kept | T10 |

### Kid and mechanics

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| K1 | Resting frac(y) .4 (depends on C11) | Inferred; consistent with t-col-03 and t-col-08 | emergent | — | famb-spawn-settle (567.0, 567.4, 567.4), k-spawn-idle (rec) |
| K2 | RORI/FT boundary roomTo lookup with no instance | UNKNOWN | kid action `edge_lookup_point_first` | treat as no changer, kill | rori/fairy-tale edge trace |
| K3 | niseblock second count test | UNKNOWN | `fake_block` param `second_test` | passes | — |
| K4 | Bullet runs its own Step on its creation tick | follows R9 (Confirmed (recorder)) | R9 | per R9 `LaterInOrder`: yes when the bullet's object comes later in the order than the kid's | t-evo-1 (rec), k-bullets (rec) |
| K5 | Water reads pressed/released during collisions | Inferred | input frame fixed for the tick | yes | a-water (rec) |
| K6 | Shared-last trigger semantics | Inferred / UNKNOWN U-MECH-2 | `trigger` model `shared_last` | all instances written, first read | TR-MECH-01 |
| K7 | Flag-array triggers across R | UNKNOWN U-MECH-3 | R16 | cleared on game_restart | TR-MECH-08 |
| K8 | Stop-chain timing | UNKNOWN U-MECH-8 | `trap::stop_chain` | alarms from launch tick | TR-MECH-03 |
| K9 | Orbit trig | UNKNOWN U-MECH-6 | `orbit` | as original | TR-MECH-05 |
| K10 | Room variable assignment (spook oRoomSwitch) | Inferred / UNKNOWN U-MECH-9 | `edge_warp` mode `room_assign` | acts like room_goto | sh-roomswitch (rec) |
| K11 | 5 fps rMenu first tick | UNKNOWN | R14 | 50 after Room Start | TR-MECH-12 |
| K12 | Showdown room creation code vs playerStart Room Start | UNKNOWN A3 U11 | 7-step order | creation actions first | A3-T20 |

## 16. Things to tell the user and research

To the user (WP0 updates the documents):

- The architecture's tick table (xprevious at tick start, timelines before alarms) is replaced by section 2.1, which follows the spec.
- Blood and playerface randomness come from the gameplay RNG in the originals, so the sim makes those draws (3.3).
- `sound_isplaying` changes game state in 15 games, so the sim keeps a sound model; audio output stays in the client (section 7). The spec's "music sync" proposal (seek songs back to the tick clock) needs the user's yes or no.
- Three new input actions: Pause, Mute, Reset (2.4).
- Title, menu and select rooms are client UI for now; a run starts at the game's first play room (4.2).
- `physics.toml` becomes the action-list form of 5.2 (CR8).

To research (WP7a files these with the converter requests of 8.4):

- **Spec items answered** (research#3, PR research#99): R24, R25, N14, C13 are Confirmed (recorder); R22 is partly answered, collisions and queries wait for T-DEAD-1 (research#7). R26 (showdown restart) is settled by research PR #98 (recorder).
- **Recorded traces analyzed** (research#4): every trace once listed here has its result in CLONE_SPEC, and section 15 follows them (#143). Still open there: R12/R12b/C8 and R22 (research#7), R13 (other request pairs), C2 at an exact .5 in x, C5, N6 (research#100), N10's draw count (research#102), and every recorder result against the real runner (research#8).
- The `INVESTIGATION.md` provenance note lists the epsilon and truthiness; the plan also tracks sign, min/max, signed zeros and the UCRT trig claim as OpenGMK-derived (N11-N13, M6).

## 17. Changes from revision 1

Each critique was re-checked against the spec before it went in. All were confirmed except where noted.

- Clean room: OpenGMK-derived rules widened to sign, min/max, signed zeros (N4, N11-N13); `gm_div` fixed to truncate (spec Confirmed); UCRT wording removed (M6); depth added to R2; per-pair re-check split into C7 and C7b; collision pair order defaults to the other's rank and id (R12, R12b); restart models get triggers and the showdown conflict (4.3, R26, R27); animation slot conflict and `AfterMotion` (R7), R7b default follows the spec's "has a sprite"; blood emitter as alarm plus Step, particle RNG switch without a GM claim (N10); "first" instance (R21); destroyed instances in queries (R22); nested room requests and deferral grain (R23-R25); numeric literals (N14); distance_to_object switches without "Euclidean" as a spec claim (C13); file order is Inferred, not Confirmed; Pause action added.
- Determinism: postcard + BLAKE3 hash (10.1); non-finite setters (10.2); overflow (10.3); i64 grid math (3.4); clippy additions (10.5); Rhai features and short-circuit lint (11); `NumRules` instead of consts; `trace-setup` feature unification (10.4); sorted VarIds and a non-serialized grid (1.1, 1.4); node/wasm-bindgen target (10.6).
- Coverage: rely blockers became converter requests and gates (8.4); M1 is rely only, kaenbyou moved to M2; `iwbtg replay run` and timing triggers (WP8b); M3 split, M3b waits for Phase 4; sound model (section 7, WP10); Draw slot order and visibility (2.1 row 12); deactivated persistent instances (4.2); kid event schema (5.2); room roles (4.1); direct-read contract (2.2); playerface and blood draw order (3.3, 6.1); real-time reads (R28); timeline offset (R6b); flipped +4 is 10 games; trace inventory and kinds (9.1).
- WP split: WP0 freezes per-owner sub-types (1.5, 1.6) so WPs never edit shared enums; WP4, WP5, WP6, WP7, WP8 split; milestone checks moved to WP11, which removes the WP5/WP6/WP7 cycles.
## 18. WP0 contract notes

The WP0 review froze these interfaces in addition to sections 1-11. Where they differ from the text above, the code and this section win.

- **Halting (2.2).** Handler entry points return `Result<Flow, SimError>`: `kid::on_event`, `builtins::on_event` and each built-in's `on_event`, `script::on_event`, `kid::run_action`, `kid::run_collision`, `kid::api::kill`, `events::probe_on_event`. `events::dispatch` turns an `Err` into `Ctx::halt(e)`, which sets `SimStatus::Halted` (first error wins) and returns the new `Flow::Halt`. `dispatch`, `run_phase`, `Ctx::destroy` and `Ctx::change_kind` return `Flow` and pass `Halt` up; a handler that gets `Halt` returns it at once; `tick::run_tick` stops on `Halt` without the sweep, the room change or `tick += 1`. `SimError::Invariant` covers ids that name nothing. `kid::api::apply_param_zone` returns `Result<(), SimError>`.
- **Ctx (1.4).** `Ctx { def, rules, w, scripts: &ScriptRuntime, dll_mode: DllMode }`. `Sim` owns the `ScriptRuntime` (built by `ScriptRuntime::new(&GameDef)` from `GameDef::scripts: Vec<ScriptDef>`, the table `ScriptId` indexes) and copies `SimOptions::dll_mode` into every `Ctx`. `ScriptHost::on_event` takes `&self`, because a script that spawns re-enters the host through Create.
- **Instances and spawning (1.1, 1.4).** `Instance` has `name: Option<NameId>` and `group: Option<GroupId>`. `SpawnInit` has `rank: Option<u16>` (room-file rank; `None` asks `rank_for`), `xscale`, `yscale`, `angle`, `name`, `group`; its `Default` runs Create.
- **Broadphase (3.4).** `World::instances` is private to `world` (read with `World::instances()`). `World::get_mut` and `World::insert` mark the instance in `Grid::moved`; every query also tests the moved instances; `World::flush_grid` re-buckets them (WP3 body, WP1 decides when). `Grid`'s `PartialEq` is always true.
- **Globals.** `RunGlobals::difficulty: u8` exists for `room::start_game` (WP4a). `KidGlobals { b: KidGlobalsB, ext: KidGlobalsExt }` like `KidState`. `World::timing: TimingState` (hashed); `timing::evaluate(&mut World, &GameDef)`; `timing::raise_run_end(&mut World)` is how `game_clear` raises `run_end`.
- **Numbers (3.2, 11).** `round`, `floor`, `ceil`, `sign`, `gm_min`, `gm_max`, `gm_eq`..`gm_ge`, `gm_bool` and `gm_repeat_count` take `&NumRules` and honor every `NumRules` variant; `num::min`/`max` and the `EPSILON`/`TRUTH_THRESHOLD` consts are gone. WP2 still owns the bodies, the `gm_div` fix and the provenance audit.
- **Built-ins (6).** Every `builtins/<kind>.rs` has `param_specs() -> &'static [ParamSpec]`, `event_set(&KindDef) -> EventSet`, `init_state(&Params) -> Result<BuiltinState, ParamError>` and `on_event(ctx, me, ev) -> Result<Flow, SimError>`; `builtins/mod.rs` routes all four for every `Builtin`.
- **Kid actions (5).** `KidAction` and `KidCollisionRun` are untagged enums of per-file enums (`actions::b::BAction`, `collisions::slope::SlopeRun`, ...), each file with `run(ctx, me, &Action)` (collisions also take `other`). Owners add variants in their own files; `kid::run_action` and `kid::run_collision` already route every part. `[ice]`, `[slope]`, `[ladder]`, `[flip]` live in `def/physics_ext.rs`; `FlipModel::model` lists `none`, `swap_object`, `yflag`, `grav_sign`.
- **Reset (4.3).** `ResetModel { trigger, kind, excluded_roles }` mirrors `RestartModel`; `RestartTrigger` serves both.
- **Loader (8).** `physics::load`, `sounds::load`, `paths::load`, `timelines::load` take `(&PackageFiles, &mut Vec<Diagnostic>)`; the table index is the id. `params::check(&GameDef, &mut Vec<Diagnostic>)` runs on the built definition. `BitMask::new` returns `Result<_, MaskTooLarge>` (limit `MAX_MASK_WORDS`).
- **Tools (9.4).** `replay_cmd::{ReplayRunArgs, main, run, ReplaySummary}`; WP8a's `main.rs` adds the `replay run` subcommand that calls `replay_cmd::main`.
- **Input (2.4).** The default trace key map binds P, M and F2 to `pause`, `mute`, `reset` and ignores Ctrl, so `press Ctrl M` is one `mute` press. The CLONE_SPEC input section says M mutes, with Ctrl held only in wanna and xia.

