//! Showdown's temp hspeed: remembering the speed while held and pushing it back when the keys
//! let go (kid/A3.md Step action 3).

use iwbtg_sim::def::physics::IceRules;
use iwbtg_sim::kid::actions::ice::IceAction;
use iwbtg_sim::kid::{self, KidAction};
use iwbtg_sim::num::{self};
use iwbtg_sim::world::{Ctx, InstId};

use crate::kid_motion_support::*;

// ---- Showdown's temp hspeed ----

fn temp(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Ice(IceAction::IceTempHspeed)).expect("ice");
}

#[test]
fn temp_hspeed_remembers_the_speed_and_pushes_it_back_when_the_keys_let_go() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            set_hs(ctx, kid, 3.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 3.0, "stored");
            temp(ctx, kid);
            assert_eq!(
                kstate(ctx, kid).temp_hspeed.get(),
                3.0,
                "same sign: kept up to date"
            );
            // The key is let go: hspeed 0, so it gains tempHspeed, which decays.
            set_hs(ctx, kid, 0.0);
            temp(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
            // Bit for bit: the GML literal 0.95 under the recorder model (N14) gives 2.85; the
            // nearest double to 0.95 would give 2.8499999999999996.
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 2.85);
            // Opposite sign: also pushed.
            set_hs(ctx, kid, -3.0);
            temp(ctx, kid);
            assert_eq!(hs(ctx, kid), -0.149_999_999_999_999_9);
        },
    );
}

/// The slip-ice glide of `traces/showdown/a3-t19-a.csv` (frames 32-71, tempHspeed column):
/// tempHspeed 3.0 decays by the GML literal `0.95` each tick. Every step matches the recorder's
/// reading of the literal, 0.9500000000000001 (N14, research#104), bit for bit.
#[test]
fn temp_hspeed_glide_matches_the_showdown_trace() {
    const TRACE: [f64; 8] = [
        2.85,
        2.707_500_000_000_000_5,
        2.572_125_000_000_000_7,
        2.443_518_750_000_001,
        2.321_342_812_500_001,
        2.205_275_671_875_001_3,
        2.095_011_888_281_251,
        1.990_261_293_867_188_9,
    ];
    assert_eq!(
        num::gml_literal("0.95").expect("literal").get(),
        IceRules::default().temp_decay.get(),
        "the profile default is the literal's recorder value"
    );
    assert_ne!(
        3.0 * 0.95,
        2.85,
        "the nearest double to 0.95 would not match the trace"
    );
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            set_hs(ctx, kid, 3.0);
            temp(ctx, kid);
            for want in TRACE {
                set_hs(ctx, kid, 0.0);
                temp(ctx, kid);
                assert_eq!(kstate(ctx, kid).temp_hspeed.get(), want);
            }
        },
    );
}

#[test]
fn temp_hspeed_zeroes_below_the_limit_and_off_ice() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            kstate(ctx, kid).temp_hspeed = r(0.2);
            set_hs(ctx, kid, 0.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 0.0, "0.2 * 0.95 is below 0.2");
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            kstate(ctx, kid).temp_hspeed = r(2.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 0.0, "off ice");
        },
    );
}
