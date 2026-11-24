//! Kid family A2: the A/D nudge, the game's vine-key flags, walking that ignores vines and
//! the A2 preset (kid/A2.md).

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::physics::VineKeys;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::a2::A2Action;
use iwbtg_sim::kid::actions::a3::A3Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::presets;

use crate::kid_a2_a3_support::*;

#[test]
fn ad_nudge_moves_one_pixel_per_press_while_standing_on_a_block() {
    with_kid(
        KidFamily::A2,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            set_v(ctx, kid, 0.0, 0.0);
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 1.0, "D");
            input(ctx, &[], &[Action::Extra1], &[]);
            set_v(ctx, kid, 0.0, 0.0);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, -1.0, "A");
            // Right + D adds to the walk speed (trace: hspeed 4).
            set_v(ctx, kid, 3.0, 0.0);
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 4.0);
            // Both keys cancel.
            set_v(ctx, kid, 0.0, 0.0);
            input(ctx, &[], &[Action::Extra1, Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn ad_nudge_does_nothing_in_the_air_or_when_frozen() {
    with_kid(
        KidFamily::A2,
        |_| {},
        |ctx, kid, k| {
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0, "no block below");
            spawn(ctx, k.block, 80.0, 109.0);
            kstate(ctx, kid).frozen = true;
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn key_flags_stay_set_from_a_press_to_its_release() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(
                ctx,
                &[Action::Right, Action::Jump],
                &[Action::Right, Action::Jump],
                &[],
            );
            a3(ctx, kid, A3Action::KeyFlags);
            let e = ctx.w.engine.kid.ext.clone();
            assert!(e.key_right && e.key_jump && !e.key_left);
            // No edge this tick: the flags stay (they are not the key state).
            input(ctx, &[], &[], &[]);
            a3(ctx, kid, A3Action::KeyFlags);
            assert!(ctx.w.engine.kid.ext.key_right);
            input(ctx, &[], &[], &[Action::Right]);
            a3(ctx, kid, A3Action::KeyFlags);
            let e = &ctx.w.engine.kid.ext;
            assert!(!e.key_right && e.key_jump);
        },
    );
}

#[test]
fn latched_flags_drive_the_vine_kick_and_drop() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            // No key edge this tick, but the flags say Right and Jump.
            ctx.w.engine.kid.ext.key_right = true;
            ctx.w.engine.kid.ext.key_jump = true;
            input(ctx, &[], &[], &[]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (15.0, -9.0));
        },
    );
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            ctx.w.engine.kid.ext.key_right = true;
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (3.0, 2.0), "no jump flag: the drop, slide speed 2");
        },
    );
    // `edges` would not have fired on the same input.
    with_kid(
        KidFamily::A3,
        |p| p.input.vine_keys = VineKeys::Edges,
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            ctx.w.engine.kid.ext.key_right = true;
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn the_a2_preset_is_rely_without_the_fade() {
    let a2 = presets::defaults(KidFamily::A2);
    let a1 = presets::defaults(KidFamily::A1);
    assert_eq!(a2.preset, KidFamily::A2);
    assert!(!a2.misc.fade_in);
    assert_eq!(
        a2.events.step.len(),
        a1.events.step.len() - 2,
        "no platform_gravity_zero or fade_in"
    );
    assert_eq!(a2.numbers, a1.numbers);
}
