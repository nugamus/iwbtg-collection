//! ladder and decor: no events, no params and no state.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, KindDef, Params};

#[test]
fn ladder_and_decor_have_no_events_no_params_and_no_state() {
    for base in [Builtin::Ladder, Builtin::Decor] {
        assert!(builtins::param_specs(base).is_empty());
        let k = KindDef::new("k", base);
        assert_eq!(
            builtins::event_set(base, &k),
            iwbtg_sim::events::EventSet::default()
        );
        assert!(builtins::init_state(base, &Params::default()).is_ok());
    }
}
