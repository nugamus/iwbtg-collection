# The simulation: Tick pipeline

Part of [the simulation](../sim.md).

## Tick pipeline

`Sim::step(InputFrame) -> StepResult` runs one tick (`tick::run_tick`). The order follows `research/specs/engines/gm8/CLONE_SPEC.md` ("tick loop", "motion", "sprites and animation", "input"); the [sim core plan](../../plans/sim-core.md) section 2.1 is the detailed version. Phase placement that the spec marks Inferred or UNKNOWN is a field of `TickRules` (register ids in plan section 15), never an `if` scattered in built-ins. If the spec changes, the rule changes in that one field.

| # | Phase | Status | Isolation |
|---|---|---|---|
| 0 | Region step (stripped runs: ranked or frozen tick, dialogue timeout presses added to the frame; #34), then latch input: build held/pressed/released for the tick; `direct_latch |= pressed` per key | Inferred (sample once before Begin Step); region step Design | `stripped::begin_tick`, `InputState::latch` |
| 1 | Capture `xprevious/yprevious` if `prev_capture = TickStart` (default) | Confirmed (recorder) U1 (R1) | `TickRules::prev_capture` |
| 2 | Begin Step | Confirmed order | |
| 3 | Timelines advance, moments run | Slot Confirmed (recorder) U6 (R6); moments for speed > 1 UNKNOWN | `TickRules::timeline_slot = AfterBeginStep` |
| 4 | Alarms: for each alarm the object handles, a value > 0 loses 1 and fires when that leaves it at or below 0; a value of exactly 0 becomes -1 without firing | Confirmed order; rule Confirmed (recorder) (R3) | `alarm::tick` |
| 5 | Keyboard events: held, then press, then release (keys in action-bit order) | Confirmed slot; inner order UNKNOWN U11 (R4) | `events::key_event_order()` |
| 6 | Mouse events | Confirmed slot; no gameplay use in the 20 games | no-op phase kept for order |
| 7 | Step | Confirmed | |
| 8 | Motion for every instance: (capture prev if `BeforeMotion`), friction, gravity vector, path, `x += hspeed`, `y += vspeed`, polar recompute | Slot Confirmed; inner order Inferred | `motion::apply_auto_motion` |
| 8b | Animation if `anim_slot = AfterMotion` | Alternative; the recorder puts it at row 13 (R7) | `anim::advance` |
| 9 | Outside Room, Intersect Boundary | Confirmed (recorder) slot (U4, R5) | `TickRules::boundary_slot` |
| 10 | Collision events with the solid put-back rule | Confirmed (manual); pairs of instances, lower kind first, both directions per pair Confirmed (recorder) (R12); re-checks Inferred/UNKNOWN (C7, C7b, C9); one handler per pair Confirmed (C8) | `events::collision_phase`, `collide::solid_resolve` |
| 11 | End Step | Confirmed | |
| 11b | Particle update: `World::particles` draws from the RNG (systems in creation order, live particles' step and death particles, then emitters), only with `NumRules::particles_draw_rng` | Confirmed (recorder) slot (CLONE_SPEC numbers "Particle draw rules" 1, N10); a tick a room request ended earlier skips it with End Step and Draw: OPEN (research#170) | `tick::particle_update`, `Particles::update` |
| 12 | Draw slot (`Late` handlers): instances in draw order (depth descending, then creation), visible instances only | Confirmed; Draw changes state, so it runs headless too (R15) | `events::run_draw_phase` |
| 13 | Animation if `anim_slot = AfterDraw` (default): `image_index += image_speed`, wrap, Animation End | Confirmed (recorder) (R7): slot, wrap `>=` and subtract once | `anim::advance` |
| 14 | Destroy sweep | Design (manual: removed at end of step) | `World::sweep` |
| 15 | Room change if requested | Confirmed sequence; abort rule Confirmed (recorder) (R8) | `room::perform_change` |
| 16 | `tick += 1` (checked), timing triggers, checkpoint hash every 500 ticks | Design | `tick.rs`, `timing.rs`, `hash.rs` |

The trace recorder samples after row 13 and before row 15. The previous version of this table (xprevious at tick start, timelines before alarms, no Draw slot) is replaced by this one.

Rules that apply to every step:

- **Dispatch order.** `events::dispatch_order` decides it (R2). Default: `object_order` rank, then creation order (Inferred); alternatives `Creation` and `DepthThenRankThenCreation`. Each phase snapshots the order once at its start. Instances created during a phase take part in the later phases of the tick. Whether they also run the current phase is R9: the spec now records (recorder) that a new instance runs the current phase when its object comes later in the order than the creator's, and not when it is the creator's own object (CLONE_SPEC tick loop, Confirmed (recorder): tr-room-03-b-log, a3-t17-b, t-evo-1). `TickRules::new_instances_join_phase` defaults to `JoinPhase::LaterInOrder`, which follows it; `Never` and `Always` stay as switches. "The first instance" of a query, the kid, view follow and saveGame always comes from `World::first_of` (R21).
- **Room change requests.** `room_goto`, `room_restart`, `game_restart` and `game_end` set one pending request (last wins, R13); the requesting handler finishes, then no further events run this tick (R8, `TickRules::room_request`).
- **Immediate mutation.** Built-ins and scripts change the world immediately, as GML does. Create runs inside `spawn`; Destroy runs inside `destroy`.
- **Room speed.** `room_speed` is sim state: the room file's speed on entry, 50 after the world's Room Start (R14). The client runs steps at that rate with its own accumulator (not Bevy's `Time<Fixed>`, because the rate can change per room). Timing counts ticks. Displayed real time is the sum of `1 / room_speed` over the ticks.
- **GM 8.0 and 8.1.** `RunnerRules::for_runner(runner, engine_rev)` returns the same rules for both until a trace shows a difference (U12, R11); any difference becomes a field, never a branch in built-ins.
- **Halting.** A script error, a non-finite value, an overflow, a broken invariant or an unsupported case sets `SimStatus::Halted(SimError)` and freezes the world where it stopped. Handlers return `Result<Flow, SimError>`; `events::dispatch` turns an error into `Ctx::halt`, which sets the status and returns `Flow::Halt`. A phase that gets `Halt` stops at once, and `tick::run_tick` returns without the sweep, the room change or `tick += 1`. The sim never panics on any input (wasm32-unknown-unknown aborts on panic).
