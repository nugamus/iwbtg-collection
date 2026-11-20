# The simulation: Built-ins

Part of [the simulation](../sim.md).

## Built-ins

Every built-in is one file under `crates/sim/src/builtins/` and one `Builtin` variant in `def/builtin.rs`. Each kind's parameters, defaults, events and behavior are documented once in `docs/format/builtins/<kind>.md`; the original objects' differences become parameters, so engine families never get separate kinds. The cherry is `killer`, and the original's player_start is `kid_start`.

This is the live text of sections 6.1 and 6.2 of [the sim core plan](../../plans/sim-core.md), which stays only as history; this page is the authority.

```rust
// builtins/mod.rs
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;   // match on Builtin
pub fn event_set(b: Builtin, kind: &KindDef) -> EventSet;
pub fn param_specs(b: Builtin) -> &'static [ParamSpec];               // read by crates/format to validate
pub fn init_state(b: Builtin, params: &Params) -> Result<BuiltinState, ParamError>;
```

`def/builtin.rs` holds the `Builtin` enum. Every variant named in the table below exists so the loader can parse kinds.

### Tiers and the table

Tiers: tier 1 is needed by M1, M1b, M2 and game 1; tier 2 by games 2-11; tier 3 by the rest. Instance counts are placed instances from the mechanics.md coverage table.

| Built-in | Tier | Games (placed) | Behavior source | Notes |
|---|---|---|---|---|
| `world` (engine controller) | 1 | all 20 | rooms (room_speed 50 at Room Start), input (R, F2, P, Ctrl+M, latch clearing), kid specs (timers), restart table | Not a GM port: the engine services the infra objects did, at the rank of the original world object (`@world` in object_order). Runs `RestartModel` at its trigger ([rooms](rooms.md)), `ResetModel`, the world-Step dummy `direct` reads (`latch_cleared_by_world`), B timer, pause (xia deactivation; I5 io_clear), mute flag. Fixed real-time reads (R28). |
| `kid_start` | 1 | all, 370 | mechanics `player_start` | Create moves itself by the spawn offset; Room Start creates the kid if none and gravity normal; flipped variants (animus). |
| `block` | 1 | all, 63,678 | mechanics `block` | Solid flag, mask variants, `visual_spawns_collider` (rely objBLOCK, CR1), slip value, debug visibility. |
| `spike` | 1 | 18, 14,962 | mechanics `spike`; collision spikes Confirmed | Precise mask from its sprite; `dir` picks the sprite; B auto rotation (90/180/270 + offsets 16/15). Trap hooks in tier 2. |
| `kid_bullet` | 1 | all | kid specs "Shooting" | Create arms alarm 0 = life; hspeed set after Create by the shooter; block check before moving (rely) or collision destroy; stop-and-die (poronkusema). |
| `blood_emitter` | 1 | all | kid specs "Death"; sprites Inferred (BLOOD-COUNT); numbers hidden consumers | **Alarm plus Step**, not a hard-coded count: Create sets alarm 0 = `life` (20) and Step spawns `per_step` blood (40; showdown 5 per hit for 5 ticks; A4 probability form), alarm 0 destroys. The number of Steps (19 or 20) then follows from R9 and the creation phase (sprites Inferred). B (kaenbyou) uses `mode = "particles"` (kid/B.md Death), RNG use per N10. |
| `blood` | 1 | all | numbers hidden consumers | Create draws `choose(0,1,2)` then gravity `random` ([rng](rng.md) order); output `KidOutput::Blood`. Sticks to solids per mechanics. |
| `game_over` | 1 | all | kid specs "Death" | Delay, view-relative position; `Late` handler for fades. |
| `bow` | 1 | most | sprites "Bow" | Step copies the kid's snapshot (`kid::api::bow_snapshot`, 1-tick lag Inferred); destroyed unless difficulty 0; Begin Step copy in wanna. `go-the-jun-g-world/draw-bow-lag-*` recorded. |
| `save` | 1 | 18, 768 | mechanics `save`, kid specs "Saving" | Trigger enum (shot, touch+press S/Z, held S keyboard event (kaenbyou), touch, auto, step overlap), cooldown, difficulty filter, saved position model (`save::save_now`), blink via image_speed 0.017 and Animation End. |
| `warp` | 1 | 18, 486 | mechanics `warp` | `room_to`, `warp_xy` (0,0 = destroy kid and bow), transition. Calls `request_room`, then sets kid position in the same handler (Confirmed deferral). |
| `difficulty_warp` | 1 | rely and others (warpStart) | rooms (warpStart), mechanics `warp` | Sets difficulty from param `dif`, deletes the save, goes to the new-game room; `StartPoint::NewGame` does the same (CR2). |
| `camera` | 1 | 15, 193 | mechanics `camera`; rooms Views | `mode = screen_snap | snap_offset | smooth_follow | end_step_clamp`. Not placed in rely's play rooms. |
| `killer` | 1 | most | mechanics `spike`, `cherry` | KILLER class with its own sprite; `cherry` adds image_speed 1/15, outside-room destroy, `bounce` (#835). |
| `platform` | 1 | 6, 109 | mechanics `platform` | No events; classes `PLATFORM | BLOCK` or `| SOLIDBLOCK`. Kid does the work. |
| `playerface` | 2 | 11 | numbers hidden consumers | Create: `image_index = irandom(3)` (RNG draw even when never shown). |
| `moving_platform` | 2 | 12, 341 | mechanics `moving_platform` | Yuuutu step algorithm; `carry_rule` variants. |
| `water` | 2 | 9, 2,510 | mechanics `water`, kid specs | Collision with kid: cap, refresh via `kid::api::refresh_air_jump`. |
| `vine` | 2 | 8, 499 | mechanics `vine` | No events; classes VINE_L/R. |
| `fake_block` | 2 | 11, 496 | mechanics `fake_block` | Solid when no kid, vanish on touch/approach, reverses moving platforms. |
| `hidden_block` | 2 | 8, 445 | mechanics `hidden_block` | Reveal at distance < 2 (C13). |
| `trigger` | 2 | 13, 666 | mechanics `trigger` | Five models; state in `MechGlobals`. |
| `trap` | 2 | 13, 1,362 | mechanics `trap` | Launch forms velocity / angle / path / move_to / step_toward; stop chains; depth reveal. |
| `drop_platform`, `gravity_flip`, `edge_warp`, `bullet_blocker`, `slope` | 2 | 4-6 each | mechanics | As mechanics.md. |
| `music` | 2 | 14, 261 | mechanics `music`; sound Inferred | Calls the sound model ([sound](sound.md)); `every_step_if_not_playing` reads `sound_isplaying` from it. |
| `boss_gate`, `boss_item`, `secret_item`, `item_viewer`, `game_clear` | 2 | 4-9 each | mechanics | Flags in `MechGlobals`; `game_clear` raises the `run_end` timing trigger. |
| `jump_refresher`, `spring`, `conveyor`, `kid_param_zone` | 3 | showdown, rainbow, little-trap | mechanics | |
| `breakable_block`, `trap_stopper`, `orbit`, `path_platform`, `time_limit`, `sign`, `ladder`, `decor` | 3 | 1-5 each | mechanics | `sign` raises a client text box output; `decor` has no behavior. |
| `scripted` | 3 | 1,712 placed | mechanics "Scripts that stay hand-written" | Through the [script host](scripts.md). |

### Rely slice (first playable path)

The smallest set that makes rely-the-needle-4-2's play rooms run: `world`, `kid_start`, `block` (with `visual_spawns_collider`), `spike`, `kid_bullet`, `save` (touch + press S/Z), `warp`, `blood_emitter`, `blood`, `game_over`, `bow`, `difficulty_warp`. `camera`, `platform` and `killer` are not placed in rely's play rooms (A1 "Placed in reference rooms"), so they come after the slice. Kaenbyou's tier-1 needs for M2 are also in the slice: held-S save, B spike rotation and B particle blood mode.
