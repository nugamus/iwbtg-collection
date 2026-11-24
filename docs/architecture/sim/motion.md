# The simulation: Motion

Part of [the simulation](../sim.md).

## Motion

GM8's motion variables and the automatic motion step (row 8 of the [tick pipeline](tick.md)). Spec: `research/specs/engines/gm8/runner/motion.md` (claims `MOT-NNN`; index `research/specs/INDEX.md`). This is the live text of the [sim core plan](../../plans/sim-core.md) section 3.1, which stays only as history.

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

  The `standard` representation (B5) has its own value type and no path argument yet:

  ```rust
  impl Body {
      pub fn new_at(x: Real, y: Real) -> Body;
      pub fn speed(&self) -> Result<Real, SimError>;       // view of the velocity
      pub fn direction(&self) -> Result<Real, SimError>;   // view, unsnapped
      pub fn acceleration(&self) -> (Real, Real);          // gravity as a vector
      pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError>;  // also vspeed
      pub fn set_speed(&mut self, v: Real) -> Result<(), SimError>;
      pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError>;
      pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
      pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError>;
      pub fn apply_auto_motion(&mut self, rules: &MotionRules) -> Result<MotionEvents, SimError>;
  }
  ```

- **Representations (B5).** `MotionRules::model` selects one of two state representations; it is a ruleset value, not a package value.
  - `MotionModel::Gm8Coupled` is the default and the format-0 value. It keeps the recorded semantics named by `[compat] motion = "gm8_coupled"` when that profile field lands: `Motion` stores every GM variable in both forms, writes couple them, and the polar-to-cartesian recompute snaps to integers ([numbers](numbers.md), Motion coupling). An absent ruleset keeps this value, so every existing package is unchanged.
  - `MotionModel::VelocityAcceleration` is the `standard` representation (`Body`): position, velocity and gravity-as-acceleration are the state; `speed()` and `direction()` are views computed from the velocity; writes, friction and the direction view keep the raw values and do not snap. A zero velocity has no direction (the view reads `+0.0`), so writing a direction at rest leaves the body at rest; use `motion_set` to set both. Negative speed is not a state: writing one moves backwards along the direction view and the speed view reads the magnitude.
  - The standard representation is unreachable until #1160 selects rulesets, and no caller stores a `Body` yet. `apply_auto_motion` rejects `VelocityAcceleration` and `Body::apply_auto_motion` rejects `Gm8Coupled`, so a wrong pairing is an error rather than a silent re-coupling. Paths stay on `Motion` (`path::advance` takes it) until the adoption slice gives paths a shared motion interface.
  - Evidence: `tests/motion.rs` runs both representations on the same writes and ticks. The velocity and position agree wherever gm8 does not snap; the declared differences are the absent snap and the direction view at rest. `determinism` and `golden_corpus` do not move because no caller selects the standard model.
- **Coupling** (Inferred formulas, trace-backed bits; the `Gm8Coupled` representation). Setting `hspeed` or `vspeed` recomputes `speed = sqrt(h*h + v*v)` and `direction = radtodeg(atan2(-v, h))`, plus 360 when negative. The trace checks: hspeed `-7.347880794884119e-17`, vspeed `0` gives direction `180.0` and speed `7.347880794884119e-17` (`kaenbyou/stand.csv` rows 3-9); hspeed 16, vspeed 0 gives direction `-0.0` (`kaenbyou/t-alarm-1.csv`). The negative-zero direction is kept and only values `< 0` wrap. Setting `speed` or `direction` recomputes `h = speed * dcos(direction)`, `v = -speed * dsin(direction)`. Speed and direction are stored as written, so a negative speed survives a later direction change (U4, M3) and the direction survives speed 0 (U5, M4). `Motion::recompute_polar` and `recompute_cartesian` are the only places to change after T-MOT-05/06 (`eat-the-rori-girl/t-mot-05-*`, `kaenbyou/t-mot-06*` recorded). The integer snap on write and the trig table are in [numbers](numbers.md), Motion coupling.
- **Auto motion** (one function, Inferred order, M1): friction, then gravity as a vector (`h += g*dcos(gd)`, `v -= g*dsin(gd)`), then path, then `x += h`, `y += v`, no rounding. The gravity residue `-7.347880794884119e-17` comes out of `dcos(270)` (Trace). Gravity written in Step is used the same tick (Inferred).
- **Paths** (`path.rs`): `PathDef { points: Vec<(Real, Real, Real)>, closed, smooth, precision }`, `PathRun { path, position, speed, end_action, absolute, scale, xstart, ystart }`. `path::advance` is the only path code; its rule is UNKNOWN (U9, U-MECH-4). Tier 2.
