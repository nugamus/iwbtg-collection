//! Random-number stream rules (decision 0005; register N10; docs/architecture/sim/rng.md).
//!
//! `gm8` has one generator and the recorder's exact consumer order, so a blood or particle draw
//! can shift a later enemy decision (F1). `standard` names four streams (`gameplay`, `effects`,
//! `draw`, `script`) so a visual effect cannot perturb gameplay. The layout is a ruleset value;
//! the stream state lives in the hashed [`crate::rng::RngStreams`]. A package selects the ruleset
//! (#1160); until then every package runs [`RngRules::gm8`] and the draw order does not move.

/// Rules for the random-number streams.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RngRules {
    /// One shared generator (`gm8`) or one per named stream (`standard`).
    pub layout: StreamLayout,
}

impl RngRules {
    /// The `gm8` ruleset: one generator, the recorder's consumer order (Confirmed).
    pub const fn gm8() -> RngRules {
        RngRules {
            layout: StreamLayout::Shared,
        }
    }

    /// The `standard` ruleset: separate `gameplay`, `effects`, `draw` and `script` streams.
    pub const fn standard() -> RngRules {
        RngRules {
            layout: StreamLayout::Split,
        }
    }

    /// True when effects draw from their own stream.
    pub const fn is_split(self) -> bool {
        matches!(self.layout, StreamLayout::Split)
    }
}

impl Default for RngRules {
    /// The absent ruleset is `gm8` (decision 0005, migration).
    fn default() -> RngRules {
        RngRules::gm8()
    }
}

/// Whether every consumer draws from one generator or from named streams.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum StreamLayout {
    /// One generator for every consumer (the `gm8` recorder behavior).
    #[default]
    Shared,
    /// One generator per named stream (`standard`).
    Split,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_absent_ruleset_is_the_shared_gm8_stream() {
        assert_eq!(RngRules::default(), RngRules::gm8());
        assert!(!RngRules::default().is_split());
        assert!(RngRules::standard().is_split());
    }
}
