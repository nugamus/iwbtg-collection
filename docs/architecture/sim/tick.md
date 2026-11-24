# The simulation: Tick pipeline

Part of [the simulation](../sim.md).

## Tick pipeline

`Sim::step(InputFrame) -> StepResult` runs one tick (`tick::run_tick`). The order follows the engine spec, one file per feature: `research/specs/engines/gm8/runner/tick.md` (`TICK-NNN`), `runner/motion.md` (`MOT-NNN`), `runner/sprites.md` (`SPR-NNN`) and `runner/input.md` (`INP-NNN`), indexed by `research/specs/INDEX.md`; this table is the live text moved from [the sim core plan](../../plans/sim-core.md) section 2.1, which is history. Phase placement that the spec marks Inferred or UNKNOWN is a field of `TickRules` (register ids on [Tick and events](register-tick-and-events.md)), never an `if` scattered in built-ins. If the spec changes, the rule changes in that one field.

| # | Phase | Status | Isolation |
|---|---|---|---|
| 0 | Region step (stripped runs: ranked or frozen tick, dialogue timeout presses added to the frame; #34), then latch the client's `InputFrame`, masked to action bits; `direct_latch |= pressed` per key | Inferred (sample once before Begin Step); region step Design | `stripped::begin_tick`, `InputState::latch` |
| 1 | Capture `xprevious/yprevious` if `prev_capture = TickStart` (default) | Confirmed (recorder) U1 (R1) | `TickRules::prev_capture` |
| 2 | Begin Step | Confirmed order | |
| 3 | Timelines advance, moments run | Slot Confirmed (recorder) U6 (R6); moments for speed > 1 UNKNOWN; the order of one tick's due moments is `TimingRules::timeline_model` | `TimingRules::timeline_slot = AfterBeginStep`, `timeline::due_moments` |
| 4 | Alarms: for each alarm the object handles, a value > 0 loses 1 and fires when that leaves it at or below 0; a value of exactly 0 becomes -1 without firing | Confirmed order; rule Confirmed (recorder) (R3) | `alarm::tick` |
| 5 | Keyboard events: held, then press, then release (keys in action-bit order) | Confirmed slot; inner order UNKNOWN U11 (R4) | `events::key_event_order()` |
| 6 | Mouse events | Confirmed slot; no gameplay use in the 20 games | no-op phase kept for order |
| 7 | Step | Confirmed | |
| 8 | Motion for every instance: (capture prev if `BeforeMotion`), friction, gravity vector, path, `x += hspeed`, `y += vspeed`, polar recompute | Slot Confirmed; inner order Inferred | `motion::apply_auto_motion` |
| 8b | Animation if `TimingRules::anim_slot = AfterMotion` | Alternative; the recorder puts it at row 13 (R7) | `anim::advance` |
| 9 | Outside Room, Intersect Boundary | Confirmed (recorder) slot (U4, R5) | `TickRules::boundary_slot` |
| 10 | Collision events with the solid put-back rule | Confirmed (manual); pairs of instances, lower kind first, both directions per pair Confirmed (recorder) (R12); re-checks Inferred/UNKNOWN (C7, C7b, C9); one handler per pair Confirmed (C8) | `events::collision_phase`, `collide::solid_resolve`; pairs gathered through the broadphase grid ([collision](collision.md), #781) |
| 11 | End Step | Confirmed | |
| 11b | Particle update: `World::particles` draws from the RNG (systems in creation order, live particles' step and death particles, then emitters), only with `NumRules::particles_draw_rng` | Confirmed (recorder) slot (NUM-058 item 1, N10, `research/specs/engines/gm8/runner/numbers.md`); a tick a room request ended earlier skips it with End Step and Draw: Confirmed (recorder), NUM-058 item 9 (live particles do not age that tick either: Confirmed (recorder), research#181) | `tick::particle_update`, `Particles::update` |
| 12 | Draw slot (`Late` handlers): instances in draw order (depth descending, then creation), visible instances only | Confirmed; gm8 Draw changes state, so it runs headless too; standard Draw is a pure projection and a hashed-state mutation halts (R15) | `events::run_draw_phase`, `DrawRules::purity` |
| 13 | Animation if `TimingRules::anim_slot = AfterDraw` (default): `image_index += image_speed`, wrap, Animation End | Confirmed (recorder) (R7): slot, wrap `>=` and subtract once | `anim::advance` |
| 14 | Destroy sweep | Design (manual: removed at end of step) | `World::sweep` |
| 15 | Room change if requested | Confirmed sequence; abort rule Confirmed (recorder) (R8) | `room::perform_change` |
| 16 | `tick += 1` (checked), timing triggers, checkpoint hash every 500 ticks | Design | `tick.rs`, `timing.rs`, `hash.rs` |

The trace recorder samples after row 13 and before row 15. The previous version of this table (xprevious at tick start, timelines before alarms, no Draw slot) is replaced by this one.

Rules that apply to every step:

- **Dispatch order.** `events::dispatch_order` decides it (R2): `object_order` rank, then **instance id**; this is the one order (decision 0005). **OPEN (research#277):** for a room-placed instance with a `gm_id`, id order can differ from room-file creation order, and the current walk keys on id. A trace must settle which one the original uses for event and draw order; do not read "creation order" below as already implemented for `gm_id` instances. Each phase snapshots the order once at its start. Instances created during a phase take part in the later phases of the tick. Whether they also run the current phase is R9: a new instance runs the current phase when its object rank comes later than the creator's, and not when it is the creator's own rank (TICK-112, `research/specs/engines/gm8/runner/tick.md`, Confirmed (recorder): tr-room-03-b-log, a3-t17-b, t-evo-1). "The first instance" of a query, the kid, view follow and saveGame always comes from `World::first_of` (R21).
- **Room change requests.** `room_goto`, `room_restart`, `game_restart` and `game_end` set one pending request (last wins, R13); the requesting handler finishes, then no further events run this tick (R8, `TickRules::room_request`).
- **Immediate mutation.** Built-ins and scripts change the world immediately, as GML does. Create runs inside `spawn`; Destroy runs inside `destroy`.
- **One clock (R30, decision 0005).** Ticks are the truth. Alarms, timeline moments, path position and animation each advance exactly once per tick, and `TickRules::timing` declares the unit of each (`TimeUnit::Ticks` at run time) and the slot each advances in; the pipeline builds its phase plan from that one table (`tick::phase_plan`), with paths inside motion and alarms in their own phase. `TimingRules::timeline_model` declares how a tick's due moments are ordered: `Moments` (the `gm8` default) keeps the recorded declaration order, while `DatedCallbacks` (`standard`) runs them by deadline then declaration index. `ClockPolicy` says what one second means: the default `RoomSpeed` is the legacy `gm8` rule, where `room_speed` is the room file's speed on entry and the tick rate after the world's Room Start (R14), and the play timer and `time_limit` re-arm from it; `Fixed` (`standard`) reads `GameDef::tick_rate` and derives seconds only at the UI and audio boundary. `room_speed` stays sim state and stays hashed either way. The client still runs steps on Bevy's `FixedUpdate` clock (`Time<Fixed>`) and retunes it to `room_speed` after every tick (`view::retune`, [view](../client/view.md)); removing that coupling is #1164. Displayed time is derived from the tick count, never from a wall clock.
- **GM 8.0 and 8.1.** `RunnerRules::for_runner(runner, engine_rev)` returns the same rules for both: research#15 found no simulation-visible difference (U12, R11). `runner` records the build for provenance; only the real 8.1 event-order and input check remains OPEN (research#175). Any difference found later becomes a field, never a branch in built-ins.
- **Halting.** A script error, a non-finite value, an overflow, a broken invariant or an unsupported case sets `SimStatus::Halted(SimError)` and freezes the world where it stopped. Handlers return `Result<Flow, SimError>`; `events::dispatch` turns an error into `Ctx::halt`, which sets the status and returns `Flow::Halt`. A phase that gets `Halt` stops at once, and `tick::run_tick` returns without the sweep, the room change or `tick += 1`. A halt while the sim is built (Game Start: a script error in a start-room Create) is `Sim::new`'s error; `Sim::new_or_halted` returns that sim halted at tick 0 instead, as a halt in play leaves it, and errs only when the sim cannot be set up (a missing map, an unsupported runner, scripts that do not compile). The client uses it so a first-tick script error shows the script error screen (#946). The sim never panics on any input (wasm32-unknown-unknown aborts on panic).

## Pipeline rules reference

The live text of plan sections 2.2-2.4. The [sim core plan](../../plans/sim-core.md) keeps the same text as history; this page is the authority.

- **Deferral grain** (rooms UNKNOWN 1, R25). The manual says the change happens "after the current action". The grain is the code action: the rest of the requesting code action runs and later actions of the same event do not (Confirmed (recorder), `tr-room-02-c.csv` row 13). A D&D Go to Room followed by more actions is untraced. Our handlers are one unit each, so we apply the request at the end of the handler (Design); a ported event whose actions follow a room-change code action must be written so that order does not matter, or flagged.
- **Create runs inside `spawn`** before it returns (Inferred). Room Start for instances created during Room Start: R10, default no (snapshot).
- **Alarm assignment of fractional values** (kaenbyou, jun-g, little-runner): stored through `num::alarm_from_real` (R3b). Round half to even (Confirmed (recorder): `kaenbyou/num-alarm-half-b`, `go-the-jun-g-world/num-eps-alpha`).
- **Alarm only ticks when the object handles that alarm** (Confirmed manual). Each kind declares `EventSet::alarms: u16`.
- **Inactive instances** get no events, are not drawn and are ignored by queries (Confirmed manual 403_06).
- **Direct key reads** (Inferred input: one latch bit per key, shared by every direct read, consumed by each read). Every built-in and kid action that ports a `keyboard_check_direct` calls `InputState::direct(&mut self, Action) -> bool`; nothing else reads or clears the latch. Ladder code that reads Up twice in one Step (little-trap, rainbow, rori, fairy-tale) makes two calls. The world built-in's dummy reads (kaenbyou, eat-the-lemon, xia) are `direct` calls from the world's Step, enabled by its `latch_cleared_by_world` param, not a kid field.
- **Real-time variables** (R28, Design). GML reads of `fps`, `current_time`, `time_span`-style clocks (kaenbyou and eat-the-lemon world Step cheat check, numbers "round, frac and real") return fixed values derived from ticks: `fps = tick_rate` (0 during the trace runner's warm-up, `TickRules::fps_zero_ticks`, #641), clocks = `tick * 1000 / tick_rate` ms. Documented per built-in that reads them.

## Events and dispatch

```rust
// events.rs
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

`dispatch` matches on `Builtin`: `Builtin::Kid => kid::on_event`, other built-ins `=> builtins::on_event`, and any kind with a script `=> script::on_event` after the base handler. Keys are actions: the GM "Key Q pressed" kid event is `KeyPress(Suicide)`, "Key R pressed" on world is `KeyPress(Restart)`, P is `KeyPress(Pause)` (INP-071, `research/specs/engines/gm8/runner/input.md`, Inferred mapping; Pause, Mute and Reset are new actions, below).

**Collision phase** (Confirmed rule; pair order Confirmed (recorder), OpenGMK only; all in `events::collision_phase`):

1. Walk the outer instance `a` by its own rank, then id (R12, the one fixed order; Confirmed (recorder), COL-062 `research/specs/engines/gm8/runner/collision.md` "Order of collision events: pairs of instances, lower object first", traces/rely-the-needle-4-2/t-ord-2.md). The rank is the instance's own kind's, never a parent's (t-ord-2 run C). Instances that handle no collision and match no kind's handler class are left out: they could take part in no event. New instances join per R9.
2. When `a`'s turn comes, take every other alive, active `b` whose rank is greater than or equal to `a`'s, by rank then id; lower ranks are not met again. For each `b` that overlaps `a` there and then: `a`'s handler for `b` runs, then `b`'s handler for `a`, right away (each only if that instance has a handler matching the other, step 3). Two instances of one rank meet twice (`a1`-`a2` in `a1`'s turn, `a2`-`a1` in `a2`'s), and each meeting runs both directions, so a handler for its own class runs twice per pair per tick (t-ord-2-a frame 4). Event registration and creation order play no part beyond the ids (t-ord-2-b). For the kid, rank 0 in these games, his pairs come first, by the other's rank then id, his event before the other's: a save point's Collision with player runs right after his block pair and before his killer pair. Test: `wp1_r12_collision_pairs_run_lower_kind_first_t_ord_2`. This is the only pair order (decision 0005): the alternate test-only walks are deleted.
3. One `b` matching several handler classes of `a` (a platform is `PLATFORM | BLOCK`, and the kid handles both) gives one pair: only the handler most specific for `b` runs (C8, R12b, Confirmed (recorder): showdown t-mot-08-a/b, help-the-fairy-tale t-col-06-e/f). The sim has no parent tree, so `events::pair_handler` takes the lowest `ClassMask::generality` (`block` last, `solidblock` next, other classes first; the nearest-ancestor default), then handler order. With no platform handler, the block (or solidblock) handler runs for the platform. Step 2 changes which pairs run when, not which handler a pair runs.
4. If `b.solid`: before the pair, re-check that `a` at its current position still overlaps `b`; skip it if not (Inferred, "Overlap is re-checked before each pair's event", C7). Then `collide::solid_resolve`: put `a` at `(xprevious, yprevious)`, run the event, then `x += hspeed; y += vspeed` from wherever the event left it; if `a` overlaps `b` again (or any solid, C9 switch), put it back where the event left it (Inferred "What previous place and new position mean"). Here `a` is the instance whose handler runs, so in the second direction of a step-2 pair the roles swap. The first direction runs right after step 2's overlap test, which serves as its re-check.
5. If `b` is not solid: run the event at the current position, no put-back (Confirmed manual). Whether the pair is re-checked first (does a killer see the kid before or after a block event moved him) is UNKNOWN (collision UNKNOWN, 5th bullet): switch `CollideRules::recheck_nonsolid` (C7b), default `true` (Design: one rule for both cases), settled by T-COL-06.
6. Skip a pair whose `a` or `b` died earlier in the phase, including pairs of instances later in the phase (R22, Confirmed (recorder): a kid killed in his block event gets no save point Collision afterwards; test `wp1_r22_a_pair_with_an_instance_killed_earlier_in_the_phase_is_skipped`).

## Input actions

`sim::input::Action` has `Pause` (P: kaenbyou `jump.blocked_by_pause_press`, B and xia pause, showdown keyboard_wait), `Mute` (Ctrl+M: sets the game's music flag, which changes `sound_stop_all` calls and so sim state) and `Reset` (F2: game reset; Groups A, B, C delete temp and game_restart, kaenbyou/lemon gameReset). They take bits 12-14. Escape and Alt+F4 stay client-only (quit), never sim input. `.inputs` and `.iwbr` accept the new bits; `replay.rs` and the `.inputs` parser round-trip them.
