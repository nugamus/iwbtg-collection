# Sim register: Motion

Part of the [sim register](../sim.md#register).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../../plans/fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks M2; T2 and T3 move to the port (rule on the tier page).

### Motion

| Id | Item | Impact | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|---|
| M1 | Friction before gravity; path position | T1 | Friction slot Confirmed (recorder): before the move, also on the creation tick; friction vs gravity order UNKNOWN (not traced) | `motion::apply_auto_motion` | friction, gravity, path, move | kaenbyou t-mot-04 (rec), t-evo-1 (rec) |
| M2 | Friction edge cases | T1 | Confirmed (recorder) U3: stops at exactly 0; negative friction grows | `motion::apply_friction` | reduce toward 0, clamp at 0; negative grows | kaenbyou t-mot-04 (rec) |
| M3 | Negative speed representation; integer snap of the cartesian recompute | T1 | Confirmed (recorder) U4: speed kept as written, negative included. Confirmed (recorder) research#193, research#199: speed and direction writes, motion_set and friction snap hspeed and vspeed to an integer within 1e-4 (strict); motion_add, hspeed/vspeed writes and gravity do not | `Motion::recompute_cartesian`, `num::snap_to_int` | store speed as written; snap every polar-to-cartesian recompute | eat-the-rori-girl t-mot-05 (rec), animus bounce-snap, motion-snap-set, motion-snap-fric (rec) |
| M4 | Direction when h and v are both 0 | T1 | Confirmed (recorder) U5: direction -0.0; a later speed write moves right | `Motion::recompute_polar` | `atan2(-0.0, 0.0)` result | kaenbyou t-mot-06-b (rec) |
| M5 | Polar recompute formula and zero signs | T1 | Confirmed (recorder) research#199: speed `sqrt(h*h + v*v)` unsnapped, direction snapped after the +360 (can read 360.0); a motion_add that leaves (h, v) bit-identical keeps speed and direction | `Motion::recompute_polar`, `Motion::motion_add` | `snap(radtodeg(atan2(-v, h)) + 360 if < 0)` | stand, t-alarm-1, animus motion-snap-hv, motion-snap-add (rec) |
| M6 | Trig bits for non-table angles: x87 vs libm | T1 | UNKNOWN numbers; answered (real runner, research#8) for cos/sin: x87 FCOS/FSIN, kid hspeed residue -7.347638122934264e-17 instead of libm's -7.347880794884119e-17; decision: keep the recorder's libm bits (about 2e-21 apart); `atan2` and non-axis angles on the real runner still OPEN (research#154) | `num::trig` | libm, table for 0/90/180/270; `atan2` correctly rounded (double-double refinement of libm), snapped to the double below pi/2 when `\|x\| / \|y\| < 1e-13` (cutoff OPEN, between 1.9e-14 and 6.8e-5, traces rely num-rng-b and kaenbyou t-mot-04) | num-trig, kaenbyou land-moving, vines |
| M7 | Solid re-move uses current speeds; "kept at previous place" | T1 | Confirmed (recorder) motion U7: "kept at previous place" restores xprevious/yprevious from tick start, undoing the event's and Step's moves; the re-move starts where the event left the instance. C9 not separated | `collide::solid_resolve`, `CollideRules::solid_recheck`, `CollideRules::solid_keep` | add speeds after event; recheck vs `b`; `SolidKeep::Previous` | showdown t-mot-07-e rows 2-8 (rec), machinary kid-a4-long-fall row 96 (rec), t-col-04 (rec) |
| M8 | Path following | T1 | UNKNOWN U9, U-MECH-4 | `path::advance` | linear segments by `speed / length`, closed loops wrap | TR-MECH-02 |
