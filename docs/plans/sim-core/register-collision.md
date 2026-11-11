# Sim core plan: 15. register, Collision

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

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
