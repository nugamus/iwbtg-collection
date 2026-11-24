//! music: the track param and the events picked from the mode (the sound model is in
//! builtins_music.rs).

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ParamValue, Params, SoundId};

use crate::builtins_boss_item_support::*;

#[test]
fn music_needs_a_track_and_picks_its_events_from_the_mode() {
    assert!(builtins::init_state(Builtin::Music, &Params::default()).is_err());
    let track = ("track", ParamValue::Sound(SoundId(0)));
    let once = kind(
        Builtin::Music,
        "m1",
        params(&[("track", ParamValue::Sound(SoundId(0)))]),
    );
    assert!(!builtins::event_set(Builtin::Music, &once).step);
    let each = kind(
        Builtin::Music,
        "m2",
        params(&[track, ("mode", word("every_step_if_not_playing"))]),
    );
    assert!(builtins::event_set(Builtin::Music, &each).step);
}
