//! Draw-slot rules (docs/architecture/sim/tick.md row 12, register R15).
//!
//! Decision 0005 splits presentation from gameplay: under `standard` the Draw slot is a pure
//! projection of sim state, so a Draw handler that writes hashed state stops the run instead of
//! silently changing a later tick. `gm8` keeps the recorder's behavior, where Draw may mutate.
//! A package selects the ruleset (#1160); until then every package runs [`DrawRules::gm8`] and
//! no existing outcome or hash moves.

/// Rules for the Draw slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DrawRules {
    /// Whether the Draw slot may mutate sim state.
    pub purity: DrawPurity,
}

impl DrawRules {
    /// The `gm8` ruleset: Draw may mutate (Confirmed (recorder): the slot is not a projection).
    /// This is also [`Default`], so an absent ruleset keeps every existing outcome and hash.
    pub const fn gm8() -> DrawRules {
        DrawRules {
            purity: DrawPurity::Ambient,
        }
    }

    /// The `standard` ruleset: Draw is a pure projection; a mutation is an error.
    pub const fn standard() -> DrawRules {
        DrawRules {
            purity: DrawPurity::Pure,
        }
    }

    /// True when the slot must not mutate sim state.
    pub const fn is_pure(self) -> bool {
        matches!(self.purity, DrawPurity::Pure)
    }
}

impl Default for DrawRules {
    /// The absent ruleset is `gm8` (decision 0005, migration).
    fn default() -> DrawRules {
        DrawRules::gm8()
    }
}

/// Whether the Draw slot may mutate sim state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawPurity {
    /// Draw may mutate (the `gm8` recorder behavior).
    Ambient,
    /// Draw cannot mutate hashed sim state (decision 0005, `standard`).
    Pure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_absent_ruleset_is_gm8() {
        assert_eq!(DrawRules::default(), DrawRules::gm8());
        assert!(!DrawRules::default().is_pure());
        assert!(DrawRules::standard().is_pure());
    }
}
