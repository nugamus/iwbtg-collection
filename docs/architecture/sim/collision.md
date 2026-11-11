# The simulation: Collision

Part of [the simulation](../sim.md).

## Collision

- **Masks.** Each sprite frame has a `Mask`: a bounding box in sprite space and an optional bitset (rows as `u64` words) for precise masks. `crates/format` builds masks from PNG alpha at load, using the sprite's `collision` mode (`precise`, `rect`, `none`) and `alpha_tolerance`. The sim receives finished bitmasks; image data never reaches it.
- **Bounding box in the room.** Computed exactly as GM8 does: position, origin, scale and bbox offsets, then banker's rounding. The kid's box in all 20 games is the rectangle [12, 11, 22, 31] with origin (17, 23) (`research/shortlist.md`).
- **Precise test.** Integer pixels in the intersection of two boxes, mapped back through each instance's scale. Rotated precise masks are rejected by the validator in format v0 unless a CLONE_SPEC defines the rule (**OPEN**).
- **Broadphase.** One uniform grid per room, 64 px cells, each cell holding instance ids, built at room entry. Anything outside the room bounds goes in a separate list. Position and image setters live on `Motion` and `Image`, which cannot reach the grid, so `World::get_mut` (the only way to change an instance) marks the instance as moved; every query also tests all moved instances, and `World::flush_grid` re-buckets them between handlers or phases. A stale bucket costs a test, never a missed instance (`collide/grid.rs`).
- **Order never depends on the grid.** Query results are sorted by (rank, id) before use.
- **Queries.** `place_meeting`, `place_free`, `collision_point`, `collision_rect`, `collision_line`, `instance_place` and `move_contact_solid` follow GM8 semantics (1 px steps, step limits) per the CLONE_SPEC.
- **Scale.** 1,288 water tiles in one room (Fairy tale) and 28,900 instances over 116 rooms (qoqoqo) are well within this design. A benchmark covers both.
- **OPEN:** spike masks (precise triangle or bounding box) per engine.
