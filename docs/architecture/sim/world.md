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
    vars: BTreeMap<VarName, Value>, // custom variables; plain values only
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

The sketch above is the original idea. The declared types are in `crates/sim/src/world/instance.rs` and `def/builtin.rs` ([plan](../../plans/sim-core.md) 1.1, 1.5, 6.1): per-owner `KindState` parts (kid, built-in, script, probe), `ClassMask` collision classes instead of parent objects, alarms as reals, and the built-in list from `research/specs/mechanics.md`.

- **Storage.** Instances live in a `Vec<Instance>` sorted by id. Lookup by id is a binary search. Runtime ids come from `World::next_id`, which starts at `World::first_runtime_id` (`[engine].first_runtime_id`, else one above the highest placed `gm_id`, else 100001) and only grows, so a runtime instance is appended. A room-placed instance with a GM8 id (`gm_id`, #669) gets that id instead; it is below every runtime id, so it goes in at its sorted place and id order is not creation order for it (CLONE_SPEC, Instances, "Placed instance ids"). Room entry skips a placed instance whose id is already alive (a carried persistent instance), register R29. The list is private to `world`: other code reads it through `World::instances` and changes an instance only through `World::get_mut`, which keeps the broadphase grid in step (see Collision). Event dispatch does not keep per-rank lists: each phase snapshots `events::dispatch_order` once (plan 1.3).
- **Creation and destruction.** New instances are appended. Destroy runs the Destroy event, then sets `alive = false`; the rest of the destroying event still runs, and the instance gets no later events (CLONE_SPEC tick loop, Confirmed (recorder)). Destroyed instances stay in the list until the end of the step (Confirmed, manual 403_05), so the sweep at the end of the tick removes them with `retain`, which keeps order. Nothing sees a destroyed-this-step instance afterwards: queries (`place_meeting`, `instance_place`, `collision_*`, `instance_nearest`, `distance_to_object`, `instance_number`, `instance_exists`, `with`) miss it and the collision phase skips every remaining pair that has it as self or as other (R22, Confirmed (recorder), research T-DEAD-1). `TickRules::dead_in_queries` stays at false.
- **Roles replace parent objects.** Collision queries ask for roles (`solid`, `killer`, `platform`, `water`, `vine`, `stops_bullets`, ...) instead of GM parent objects. Each kind declares its roles.
- **Package objects.** A package object (`@boss1`) is `KindState::Scripted` or inherits a built-in with `base = "projectile"`, `base = "trigger"` and so on. A scripted object with a base keeps the base's native behavior and roles; the script adds events. Collision with the kid works through roles, never through script code.
- **The kid** is a built-in. Scripts read it, can kill it and can set documented knobs (for example gravity flip). They never replace its physics.
- **Globals and save slot.** Game-wide variables persist across rooms, like GM globals. The save slot (room, position, gravity, flags) is part of the sim state. The client (`savefile.rs`, through `storage.rs`: docs/architecture/client/storage.md) writes the slot to `<data folder>/iwbtg-collection/saves/<game id>.save` (Windows: `%LOCALAPPDATA%`; elsewhere `$XDG_DATA_HOME` or `~/.local/share`; on the web a `localStorage` entry) whenever the sim's slot changes, as the postcard bytes of `SaveSlot`, written beside the file and renamed so a crash never leaves half a save. A slot the sim deletes (a new game) deletes the file. At start the client begins from the saved spot (`StartPoint::FromSave`); a missing file is a new game, and a broken file or a saved room the game does not have is a warning (stderr, or the browser console) and a new game, never a crash. `--no-save-file` never reads or writes it. Ranked replays always start from a new game.
- **Sampling the tick of a room change (#450).** `tick::run_tick` is `run_rows` (rows 0-14) then `finish_tick` (rows 15-16, the room change); `Sim::step_rows` and `Sim::finish_step` expose the split. The recorder's row of a room-change tick shows the old room's instances (kid/B.md Restart timing, CLONE_SPEC D/E retry), so the trace runner samples between the two when a room change is pending. `Sim::step` and the world after it are unchanged.

- **Retry, reset and saves (WP4b).** The `world` built-in calls `restart::run_restart` (R) and `run_reset` (F2) at its rank, in the phase `[engine.restart].trigger` names. `game_restart_via_boot` moves (or creates) the kid at the saved spot, counts a death when `death_on_restart = "save_counters"`, requests `game_restart` and sets `RunGlobals::boot_load`, the stand-in for the temp file: it survives the restart, `restart::boot_step` (world Step) counts `boot_ticks` down and on the last boot tick loads the slot and requests the saved room (timeline N, N+1, N+2). `boot_ticks = 0` is `Unsupported` (R17). The other kinds recreate the kid and request the saved room at once; showdown has no `game_restart` (R26, settled by the recorder). Kid-side extras (B's timer and `reverse`, showdown's `frozen`) wait for `kid::api`. `save::quantize` has six position models; `save_now` reads the kid through its instance only. `view::follow_update` runs the runner view follow (border halved when larger than half the view, clamp, speed limit; speed 0 freezes an axis).
- **Output queue.** Audio and visual effects (`play_sound`, `play_music`, screen shake, blood) are events pushed to `output`. The client consumes them. `Sim::step` clears `output` first, except the first step (tick 0), which keeps what `Sim::new` produced (the first room's start: music, sounds); read `output` after each step and never after `Sim::new`. The sim never reads anything back from audio or rendering.
- **Room creation code (#629).** `room::run_action` runs a room's `on_create` rows in step 6 of the room start. `RoomAction::When` wraps a row with its condition (`RoomWhen`: difficulty below, kid exists, kind absent). An indexed `set_global` (`global.stage[global.savenum]`) stores its element as the run-time variable `stage[<n>]` in `World::var_names` (plan 1.1; hashed with the world), so it needs no array type. `save_game` with no kid is `save::save_without_kid` (x 0, y 0).
