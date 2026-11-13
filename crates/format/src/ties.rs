//! Collision handler ties (docs/format/v0.md, Validation step 7; #737): kinds whose classes
//! match two equally specific collision handlers of some kind. The sim runs one handler per
//! (instance, other) pair, picked by [`iwbtg_sim::events::pair_handler`]: the matching handler
//! class with the lowest [`ClassMask::generality`], then the first in handler order (sim register
//! C8). A GM object has one parent chain, so one ancestor is always the most specific; class
//! sets have no such order, and a tie between two classes of the same generality falls back to
//! handler order, which may not be the original's choice. The loader warns about each tie.
//!
//! The check calls the sim's `pair_handler` and `generality`, so it cannot drift from the rule
//! the sim runs.

use std::collections::BTreeSet;

use iwbtg_sim::def::{ClassMask, GameDef, KindId};
use iwbtg_sim::events::pair_handler;

/// One tie: an instance of `handler` touching an instance of `other` runs its `picked`
/// handler, though `tied` matches `other` and is just as specific.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Tie {
    /// The kind whose collision handlers tie.
    pub handler: KindId,
    /// The kind whose classes match both handlers.
    pub other: KindId,
    /// The handler class the sim runs (first in handler order).
    pub picked: ClassMask,
    /// A different handler class of the same generality that also matches.
    pub tied: ClassMask,
}

/// Every tie, by handler kind, then other kind, then tied class. Each pair of distinct handler
/// classes once per (handler, other): a class listed twice is one GM event, not a tie.
pub fn ties(def: &GameDef) -> Vec<Tie> {
    let mut out = BTreeSet::new();
    for (h, hk) in def.kinds.iter().enumerate() {
        let handlers = hk.events.collisions.as_slice();
        if handlers.len() < 2 {
            continue;
        }
        let Ok(h) = u32::try_from(h) else { continue };
        for (o, ok) in def.kinds.iter().enumerate() {
            let Ok(o) = u32::try_from(o) else { continue };
            let Some((_, picked)) = pair_handler(handlers, ok.classes) else {
                continue;
            };
            for &tied in handlers {
                if tied != picked && ok.classes.intersects(tied) && tied.generality() == picked.generality() {
                    out.insert(Tie {
                        handler: KindId(h),
                        other: KindId(o),
                        picked,
                        tied,
                    });
                }
            }
        }
    }
    out.into_iter().collect()
}
