# The simulation: Motion

Part of [the simulation](../sim.md).

## Motion

GM8's motion variables and the automatic motion step (row 8 of the [tick pipeline](tick.md)). This is the live text of the [sim core plan](../../plans/sim-core.md) section 3.1, which stays only as history.

- **`Motion`** holds every GM motion variable (`x`, `y`, `xstart`, `ystart`, `xprevious`, `yprevious`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`) with private fields and one accessor per variable. A setter recomputes the coupled variables and rejects a non-finite value with `SimError::NonFinite` ([numbers](numbers.md)).

  ```rust
  impl Motion {
      pub fn new_at(x: Real, y: Real) -> Motion;      // speed 0, direction 0, gravity 0, gravity_direction 270 (Inferred defaults)
      pub fn x(&self) -> Real;  pub fn set_x(&mut self, v: Real) -> Result<(), SimError>;   // also y, xstart...
      pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError>;  // recompute speed and direction
      pub fn set_vspeed(&mut self, v: Real) -> Result<(), SimError>;
      pub fn set_speed(&mut self, v: Real) -> Result<(), SimError>;   // recompute hspeed and vspeed
      pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError>;
      pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
      pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
      pub fn set_gravity(&mut self, g: Real) -> Result<(), SimError>;
      pub fn set_gravity_direction(&mut self, d: Real) -> Result<(), SimError>;
      pub fn set_friction(&mut self, f: Real) -> Result<(), SimError>;
      pub fn capture_previous(&mut self);
  }
  pub fn apply_auto_motion(m: &mut Motion, path: Option<&mut PathRun>, def: &GameDef, rules: &MotionRules) -> Result<MotionEvents, SimError>;
  ```

- **Coupling** (Inferred formulas, trace-backed bits). Setting `hspeed` or `vspeed` recomputes `speed = sqrt(h*h + v*v)` and `direction = radtodeg(atan2(-v, h))`, plus 360 when negative. The trace checks: hspeed `-7.347880794884119e-17`, vspeed `0` gives direction `180.0` and speed `7.347880794884119e-17` (`kaenbyou/stand.csv` rows 3-9); hspeed 16, vspeed 0 gives direction `-0.0` (`kaenbyou/t-alarm-1.csv`). The negative-zero direction is kept and only values `< 0` wrap. Setting `speed` or `direction` recomputes `h = speed * dcos(direction)`, `v = -speed * dsin(direction)`. Speed and direction are stored as written, so a negative speed survives a later direction change (U4, M3) and the direction survives speed 0 (U5, M4). `Motion::recompute_polar` and `recompute_cartesian` are the only places to change after T-MOT-05/06 (`eat-the-rori-girl/t-mot-05-*`, `kaenbyou/t-mot-06*` recorded). The integer snap on write and the trig table are in [numbers](numbers.md), Motion coupling.
- **Auto motion** (one function, Inferred order, M1): friction, then gravity as a vector (`h += g*dcos(gd)`, `v -= g*dsin(gd)`), then path, then `x += h`, `y += v`, no rounding. The gravity residue `-7.347880794884119e-17` comes out of `dcos(270)` (Trace). Gravity written in Step is used the same tick (Inferred).
- **Paths** (`path.rs`): `PathDef { points: Vec<(Real, Real, Real)>, closed, smooth, precision }`, `PathRun { path, position, speed, end_action, absolute, scale, xstart, ystart }`. `path::advance` is the only path code; its rule is UNKNOWN (U9, U-MECH-4). Tier 2.
