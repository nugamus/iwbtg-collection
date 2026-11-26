//! slope: its parameter checks.

use iwbtg_sim::builtins::{self};
use iwbtg_sim::def::{Builtin, ParamValue};

use crate::builtins_drop_flip_edge_support::*;

#[test]
fn slope_checks_its_params() {
    let ok = params(&[
        ("shape", word("rise_left")),
        ("scale_x", ParamValue::Real(r(-1.0))),
    ]);
    assert!(builtins::init_state(Builtin::Slope, &ok).is_ok());
    assert!(builtins::init_state(Builtin::Slope, &params(&[("shape", word("wavy"))])).is_err());
}
