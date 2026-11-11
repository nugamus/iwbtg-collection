# Sim core plan: 15. register, Motion

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

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
