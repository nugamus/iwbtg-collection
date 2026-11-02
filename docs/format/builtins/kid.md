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
| `frozen`, `frozen2` | Input blocked; movement action skipped. Also `global.frozen`, `global.frozen2` (`KidGlobals`). |
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
| `read_field(world, kid, field)` | A kid field by GM name for the trace probe (`djump`, `curJumps`, `onPlatform`, `jump[1]`, ...); none for an unknown name. |

## Events

`kid::on_event` runs: Create, Room Start, Step and End Step lists; Alarm rows; Key rows; Collision rows (every row of the matching class, in order). The kid's `EventSet` follows the lists, so an empty list costs nothing.

Boundary events follow `events.boundary` (A1: Intersect Boundary kills or warps, then wraps; see [physics.md](../physics.md)).

OPEN: sounds (#231); the flipped kid and the other families' actions (WP5b, WP5c).
