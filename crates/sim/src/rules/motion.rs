//! Automatic motion switches (docs/architecture/sim/motion.md, register M1, M2).
//!
//! M3-M5 live in `Motion::recompute_polar` and `recompute_cartesian`, M7 in
//! [`CollideRules`](super::CollideRules), M8 in `path::advance`. B5 selects the motion
//! representation (gm8 coupled vs standard velocity/acceleration). WP0 declares; WP2 owns.

/// Rules for `motion::apply_auto_motion`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MotionRules {
    /// M1: order of the steps of automatic motion. Friction before the move, also on the
    /// creation tick: Confirmed (recorder), kaenbyou t-mot-04; friction vs gravity: UNKNOWN.
    pub order: MotionOrder,
    /// M2 (U3, answered by the recorder, kaenbyou t-mot-04): friction edge cases.
    pub friction: FrictionRule,
    /// B5: the state representation. `Gm8Coupled` is the default and the format-0 value; the
    /// standard value is not reachable from a package until the ruleset selection (#1160).
    pub model: MotionModel,
}

/// B5: the motion representation a ruleset selects (docs/architecture/sim/motion.md).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MotionModel {
    /// `gm8`: every GM motion variable is stored, writes couple the cartesian and polar forms, and
    /// the polar-to-cartesian recompute snaps to integers (M3-M5). This is also the format-0
    /// semantics named by `[compat] motion = "gm8_coupled"`; an absent ruleset keeps it.
    #[default]
    Gm8Coupled,
    /// `standard`: position, velocity and gravity-as-acceleration are the state; `speed` and
    /// `direction` are views; no integer snap. Adopted port by port with a feel contract; a
    /// package cannot select it (decision 0005).
    VelocityAcceleration,
}

/// M1: order of the steps of automatic motion.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MotionOrder {
    /// Friction, gravity vector, path, then `x += hspeed`, `y += vspeed`. Default.
    #[default]
    FrictionGravityPathMove,
}

/// M2: friction edge cases.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FrictionRule {
    /// Reduce speed toward 0 and stop at 0; negative friction grows speed. Default.
    #[default]
    TowardZeroClamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A format-0 package (and `RunnerRules::for_runner`) keeps the gm8 representation, so no
    /// existing run moves. The standard value exists but is unreachable until #1160.
    #[test]
    fn gm8_coupled_is_the_default_model() {
        assert_eq!(MotionRules::default().model, MotionModel::Gm8Coupled);
        assert_eq!(MotionModel::default(), MotionModel::Gm8Coupled);
        let standard = MotionRules {
            model: MotionModel::VelocityAcceleration,
            ..MotionRules::default()
        };
        assert_eq!(standard.model, MotionModel::VelocityAcceleration);
        // The other fields do not change with the representation.
        assert_eq!(standard.order, MotionOrder::default());
        assert_eq!(standard.friction, FrictionRule::default());
    }
}
