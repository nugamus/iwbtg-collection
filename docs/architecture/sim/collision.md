# The simulation: Collision

Part of [the simulation](../sim.md).

## Collision

- **Masks.** Each sprite frame has a `Mask`: a bounding box in sprite space and an optional bitset (rows as `u64` words) for precise masks. `crates/format` builds masks from PNG alpha at load, using the sprite's `collision` mode (`precise`, `rect`, `none`) and `alpha_tolerance`. The sim receives finished bitmasks; image data never reaches it.
- **Bounding box in the room.** Computed exactly as GM8 does: position, origin, scale and bbox offsets, then banker's rounding. The kid's box in all 20 games is the rectangle [12, 11, 22, 31] with origin (17, 23) (`research/shortlist.md`).
- **Precise test.** Integer pixels in the intersection of two boxes, mapped back through each instance's scale. Rotated precise masks are rejected by the validator in format v0 unless a CLONE_SPEC defines the rule (**OPEN**).
- **Broadphase.** One uniform grid per room, 64 px cells, each cell holding instance ids, built at room entry. Anything outside the room bounds goes in a separate list. Position and image setters live on `Motion` and `Image`, which cannot reach the grid, so `World::get_mut` (the only way to change an instance) marks the instance as moved; every query also tests all moved instances, and `World::flush_grid` re-buckets them between handlers or phases. A stale bucket costs a test, never a missed instance (`collide/grid.rs`).
- **Order never depends on the grid.** Query results are sorted by (rank, id) before use.
- **Queries.** `place_meeting`, `place_free`, `collision_point`, `collision_rect`, `collision_line`, `instance_place` and `move_contact_solid` follow GM8 semantics (1 px steps, step limits) per the CLONE_SPEC.
- **Bounce** (`collide/bounce.rs`, C14, #835). `move_bounce_solid` and `move_bounce_all` (the Bounce action; `place_free` or `place_empty` as "blocked") change only `hspeed` and `vspeed`: the tick's motion then moves by the new speed, so a bounce never moves to contact. The two rules are pure functions over a "blocked here" closure (`plain_bounce`, `precise_direction`), which `tests/bounce.rs` checks row by row against the recorder tables. A precise bounce sets `direction` and computes `hspeed` and `vspeed` itself (`precise_speeds`): libm `speed * cos` and `-speed * sin` of the direction in radians, each snapped to the nearest integer when within 1e-4 (research#193). The general `speed`/`direction` setters do not snap yet (#855).
- **Scale.** 1,288 water tiles in one room (Fairy tale) and 28,900 instances over 116 rooms (qoqoqo) are well within this design. A benchmark covers both.
- **OPEN:** spike masks (precise triangle or bounding box) per engine.
