//! Automatic motion switches (docs/plans/sim-core.md 3.1, register M1, M2).
//!
//! M3-M5 live in `Motion::recompute_polar` and `recompute_cartesian`, M7 in
//! [`CollideRules`](super::CollideRules), M8 in `path::advance`. WP0 declares; WP2 owns.

/// Rules for `motion::apply_auto_motion`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MotionRules {
    /// M1 (Inferred / UNKNOWN): order of the steps of automatic motion.
    pub order: MotionOrder,
    /// M2 (UNKNOWN U3): friction edge cases.
    pub friction: FrictionRule,
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
