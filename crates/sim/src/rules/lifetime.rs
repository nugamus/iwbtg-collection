//! Instance lifetime rules (docs/architecture/sim/world.md, "Creation and destruction"; A1 of
//! #1174).
//!
//! `gm8` leaves a destroyed instance in the store until the end-of-tick sweep. Queries, events,
//! `with`/`instances`, collision and draw already skip it, but [`World::get`](crate::world::World::get)
//! still returns the dead instance for the rest of the tick (the store's "dead this tick"
//! window). `standard` makes destroy immediate and final: the store drops the instance right
//! after its Destroy event, so every lookup, query, `with` snapshot and collision pair agrees.
//!
//! A package selects the ruleset (#1160); until then every package runs [`LifetimeRules::gm8`]
//! and no outcome or hash moves. [`World::visible`](crate::world::World::visible) is the one
//! predicate the world-side readers ask; the collide, event, script, draw, view and timeline
//! readers adopt it in A1b of #1174.

/// Rules for an instance's lifetime.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LifetimeRules {
    /// What a destroyed instance is between its Destroy event and the end of the tick.
    pub destroyed: DestroyedVisibility,
}

impl LifetimeRules {
    /// The `gm8` ruleset: the dead instance stays stored until the sweep. This is also
    /// [`Default`], so an absent ruleset keeps every existing outcome and hash.
    pub const fn gm8() -> LifetimeRules {
        LifetimeRules {
            destroyed: DestroyedVisibility::HalfDead,
        }
    }

    /// The `standard` ruleset: destroy removes the instance from the store at once.
    pub const fn standard() -> LifetimeRules {
        LifetimeRules {
            destroyed: DestroyedVisibility::Immediate,
        }
    }

    /// True when a destroyed instance leaves the store at its Destroy event.
    pub const fn is_immediate(self) -> bool {
        matches!(self.destroyed, DestroyedVisibility::Immediate)
    }
}

impl Default for LifetimeRules {
    /// The absent ruleset is `gm8` (decision 0005, migration).
    fn default() -> LifetimeRules {
        LifetimeRules::gm8()
    }
}

/// What a destroyed instance is between its Destroy event and the end of the tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DestroyedVisibility {
    /// `gm8`: `alive = false`, stored until the end-of-tick sweep; `World::get` still finds it
    /// for the rest of the tick, while every query and walk skips it.
    HalfDead,
    /// `standard`: removed from the store at Destroy; no lookup finds it.
    Immediate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_absent_ruleset_is_gm8() {
        assert_eq!(LifetimeRules::default(), LifetimeRules::gm8());
        assert!(!LifetimeRules::default().is_immediate());
        assert!(LifetimeRules::standard().is_immediate());
    }
}
