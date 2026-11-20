//! Kid family A5: little-runner's End Step gravity, the solid recovery, spook house's frozen Step
//! and idle sprite rule (kid/A5.md).

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::a5::A5Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;

use crate::kid_a4_a5_support::*;

#[test]
fn the_end_step_sets_gravity_every_tick_even_when_frozen() {
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.get_mut(kid).unwrap().m.set_gravity(Real::ZERO).unwrap();
            if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
                s.frozen = true;
            }
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
        },
    );
}

#[test]
fn a_kid_stuck_in_a_solid_goes_back_and_moves_to_contact() {
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 110.0);
            // Came from y 100, ended inside the block at 106.
            let m = &mut ctx.w.get_mut(kid).unwrap().m;
            m.set_y(r(100.0)).unwrap();
            m.set_yprevious(r(100.0)).unwrap();
            m.set_y(r(106.0)).unwrap();
            m.set_vspeed(r(6.0)).unwrap();
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(pos(ctx, kid).1, 101.0, "flush above the block");
            assert_eq!(vs(ctx, kid), 6.0, "vspeed is kept");
        },
    );
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 120.0, 0.0, 2.0);
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(pos(ctx, kid).1, 120.0, "free: nothing moves");
        },
    );
}

#[test]
fn stop_if_frozen_skips_the_rest_of_the_step_list() {
    let step = vec![
        KidAction::Common(CommonAction::StopIfFrozen),
        KidAction::Common(CommonAction::Walk),
    ];
    with_kid(
        KidFamily::A5,
        |p| p.events.step = step.clone(),
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 100.0, 2.0, 0.0);
            ctx.w.engine.kid.frozen = true;
            input(ctx, &[Action::Right], &[], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(
                ctx.w.get(kid).unwrap().m.hspeed(),
                r(2.0),
                "nothing ran, hspeed not rewritten"
            );
            ctx.w.engine.kid.frozen = false;
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0));
        },
    );
}

#[test]
fn the_idle_sprite_needs_a_block_below_when_the_game_says_so() {
    with_kid(
        KidFamily::A5,
        |p| p.misc.idle_needs_block = true,
        |ctx, kid, k, idle| {
            let before = ctx.w.get(kid).unwrap().img.sprite();
            input(ctx, &[], &[], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(
                ctx.w.get(kid).unwrap().img.sprite(),
                before,
                "in the air the sprite is left alone"
            );
            spawn(ctx, k.block, 80.0, 109.0);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(idle));
        },
    );
}

#[test]
fn only_a5_sets_gravity_in_the_end_step() {
    assert!(presets::defaults(KidFamily::A4).events.end_step.is_empty());
    assert_eq!(presets::defaults(KidFamily::A5).events.end_step.len(), 1);
}
