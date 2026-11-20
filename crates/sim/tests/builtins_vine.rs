//! vine: the side parameter check.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};

use crate::builtins_playerface_water_support::*;

#[test]
fn vine_needs_a_valid_side() {
    assert!(builtins::init_state(Builtin::Vine, &Params::default()).is_err());
    assert!(builtins::init_state(Builtin::Vine, &params(&[("side", word("left"))])).is_ok());
    assert!(builtins::init_state(Builtin::Vine, &params(&[("side", word("up"))])).is_err());
}
