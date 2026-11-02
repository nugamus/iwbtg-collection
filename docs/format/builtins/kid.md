# `kid`

The player character. It is a built-in, not a script: its events run the ordered action lists of the game's [`physics.toml`](../physics.md), so every game's differences are values and named actions in that file. Scripts can read it and use the kid api; they never replace its physics. Source: `crates/sim/src/kid/`.

It has no params. Place it through `kid_start` (the room's start marker) or let a save load create it; a game may have several kid kinds (the flipped kid is its own kind, WP5c).

## State

Fields other built-ins read go through `kid::api`, never `KidState` directly (plan 1.6).

| State | Meaning |
|---|---|
| `djump` | A double jump is available. Create, landing, refreshers and platforms set it. |
| `cur_jumps`, `tjump` | Family B counter and showdown's third jump. |
| `on_platform` | Standing on a platform. Not set at Create (reads 0). |
| `ladder`, `ladder_ud`, `ladder_jump` | Ladder flags (WP5c). |
| `frozen`, `frozen2` | Input blocked; movement action skipped: with `frozen2` (the kid's own or the global) the A1 actions `walk`, `air_sprite`, `fall_cap`, `shoot`, `jump` and `release` do nothing. Also `global.frozen`, `global.frozen2` (`KidGlobals`). |
| `yflag`, `flipped` | Gravity side. |
| `facing` | -1 or 1. |
| `run_speed`, `jump_speeds`, `max_vspeed`, `max_fall_speed` | Copied from `[numbers]` at Create; parameter zones change them at run time. |
| `invincible` | `muteki`: a kill is ignored while set. |

`KidGlobals` outlive the kid: `grav` (the next kid is the flipped one), `image_xscale` (the facing handed to the next kid: -1 or 0), `frozen`, `frozen2`.

## `kid::api`

| Function | Does |
|---|---|
| `facing(world, kid)` | -1 or 1 (1 for a non-kid). |
| `refresh_air_jump(ctx, kid)` | `djump` true and the family B counter 1 (water, refreshers, landing, platforms). |
| `set_on_platform(ctx, kid, on)` | The platform flag. |
| `apply_param_zone(ctx, kid, zone)` | Writes `run_speed`, `max_vspeed`, `max_fall_speed`, `jump`, `jump2`, `jump3` (GM names `maxSpeed`, `maxVspeed`, `maxFallSpeed` too). Errors, applying nothing, on a non-real, non-finite or unknown value. The values stay until another zone or a new kid's Create. |
| `bow_snapshot(world, kid)` | Position, scales and alpha, for the bow. |
| `kill(ctx, kid, cause)` | The death sequence of `[death]`; `AbortTick` when a Select room reset asked for a room. |
| `read_field(world, kid, field)` | A kid field by GM name for the trace probe (`djump`, `curJumps`, `onPlatform`, `jump[1]` (1-based, the ground jump), `maxJumps`, `infJump`, ...); none for an unknown name. |

## Events

`kid::on_event` runs: Create, Room Start, Step and End Step lists; Alarm rows; Key rows; Collision rows (every row of the matching class, in order). The kid's `EventSet` follows the lists, so an empty list costs nothing.

Boundary events follow `events.boundary` (A1: Intersect Boundary kills or warps, then wraps; see [physics.md](../physics.md)).

OPEN: sounds (#231, including family B's death sound object).

Family C (xia, client#295): key C (`teleport_to_start`, `extra1`) is built. The restart through `scrLoadGame` is the `recreate_in_place` restart model (`[engine.restart]`, the saved grav picks the kid object); its death music, fade and pause steps belong to the `world` built-in. The jump, vine and flip rows for `slide_block`, `slope_c` and slip blocks are tier 3 and not built, because no xia room places them (a profile listing them does not load). The `objMovingPlatform` yspeed carry check against the flipped platform rule needs the T-platform trace (kid/C.md, not recorded yet).

Known gaps of families A2 to A5 (client#286):

- `frozen` is a flag. Wanna reads it as a countdown (`frozen > 0` forces `h` to 0 and subtracts 1), but nothing in wanna sets it above 0 (kid/A3.md), so the difference is dormant. A game that sets it above 0 needs a countdown first.
- A3 Step action 1 (the `frames` and `seconds` counters) is not modelled: the client counts ticks and the saved time is our own format.
- showdown's room-specific triple jump and bullet limit (1 in room046 and room049), `ad_nudge` while frozen, `key_flags` with a press and a release in one tick, machinary's depth-layer scale, spook house's wings and powerup gravity, and little-runner's boss death sound wait for spec answers: research#43.
- The roomout edge targets of eat-the-new-year are an `edge_warp` with a per-edge table ([edge_warp](edge_warp.md)).
