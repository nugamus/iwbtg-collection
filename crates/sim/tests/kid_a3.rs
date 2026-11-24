//! Kid family A3: the held suicide key, the End Step killer test, showdown's triple jump and
//! the A3 preset (kid/A3.md).

use iwbtg_sim::def::physics::AirJump;
use iwbtg_sim::def::{KidFamily, PhysicsProfile, RoomId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::a3::A3Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::collisions::block::BlockRun;
use iwbtg_sim::kid::{self, KidCollisionRun};
use iwbtg_sim::world::{Ctx, InstId};

use crate::kid_a2_a3_support::*;

#[test]
fn a3_walking_ignores_vines() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            // A right vine flush against the kid: A1 would not let it walk left.
            spawn(ctx, k.vine_r, 106.0, 96.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, -3.0);
            assert_eq!(kstate(ctx, kid).facing, -1);
        },
    );
    with_kid(
        KidFamily::A1,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_r, 106.0, 96.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 0.0, "A1 keeps hspeed near its right vine");
        },
    );
}

#[test]
fn the_held_suicide_key_kills_outside_menu_rooms() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(ctx, &[], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(ctx.w.get(kid).unwrap().alive, "not held");
            input(ctx, &[Action::Suicide], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(!ctx.w.get(kid).unwrap().alive, "held, no press needed");
        },
    );
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            ctx.w.room = RoomId(1); // the Menu room
            input(ctx, &[Action::Suicide], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

fn end_step(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
}

#[test]
fn the_end_step_killer_test_kills_after_movement() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 300.0, 300.0);
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            spawn(ctx, k.killer, 90.0, 90.0);
            end_step(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn the_killer_test_skips_menu_rooms_and_resets_in_select_rooms() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 90.0, 90.0);
            ctx.w.room = RoomId(1);
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            ctx.w.room = RoomId(2);
            let m = &mut ctx.w.get_mut(kid).unwrap().m;
            m.set_xstart(r(10.0)).unwrap();
            m.set_ystart(r(20.0)).unwrap();
            end_step(ctx, kid);
            let m = &ctx.w.get(kid).unwrap().m;
            assert_eq!((m.x().get(), m.y().get()), (10.0, 20.0));
            assert!(ctx.w.get(kid).unwrap().alive, "moved to its start, not killed");
        },
    );
}

fn showdown(p: &mut PhysicsProfile) {
    p.jump.air = AirJump::DjumpThenTjump;
    p.jump.release_twice_while_tjump = true;
    p.numbers.jumps = vec![r(8.5), r(7.0), r(6.0)];
}

#[test]
fn create_grants_the_third_jump() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, _| {
        assert!(kstate(ctx, kid).tjump, "Create sets tjump (I8)");
    });
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            assert!(!kstate(ctx, kid).tjump);
        },
    );
}

#[test]
fn jump_then_double_then_triple_then_nothing() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, k| {
        let floor = spawn(ctx, k.block, 80.0, 109.0);
        input(ctx, &[Action::Jump], &[Action::Jump], &[]);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -8.5);
        ctx.w.get_mut(floor).unwrap().alive = false;
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -7.0);
        assert!(!kstate(ctx, kid).djump && kstate(ctx, kid).tjump);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -6.0, "the third jump");
        assert!(!kstate(ctx, kid).tjump);
        set_v(ctx, kid, 0.0, 1.0);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, 1.0, "a fourth press does nothing");
    });
}

#[test]
fn the_double_jump_grants_the_third_jump_only_in_a_flagged_room() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, _| {
        kstate(ctx, kid).tjump = false;
        input(ctx, &[Action::Jump], &[Action::Jump], &[]);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -7.0);
        assert!(!kstate(ctx, kid).tjump, "an unflagged room leaves tjump alone");
        ctx.w.room = RoomId(3);
        kstate(ctx, kid).djump = true;
        common(ctx, kid, CommonAction::Jump);
        assert!(
            !kstate(ctx, kid).djump && kstate(ctx, kid).tjump,
            "room028 (a3-t09 f47)"
        );
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -6.0, "so the third jump follows");
    });
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            ctx.w.room = RoomId(3);
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            common(ctx, kid, CommonAction::Jump);
            assert!(!kstate(ctx, kid).tjump, "only with air = djump_then_tjump");
        },
    );
}

#[test]
fn a_release_cuts_twice_while_the_third_jump_is_pending() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, _| {
        set_v(ctx, kid, 0.0, -8.0);
        input(ctx, &[], &[], &[Action::Jump]);
        common(ctx, kid, CommonAction::Release);
        let once = (r(-8.0) * r(0.45)).get();
        assert_eq!(
            hv(ctx, kid).1,
            (r(once) * r(0.45)).get(),
            "x 0.45 x 0.45 (0.2025)"
        );
        kstate(ctx, kid).tjump = false;
        set_v(ctx, kid, 0.0, -8.0);
        common(ctx, kid, CommonAction::Release);
        assert_eq!(hv(ctx, kid).1, once);
    });
}

#[test]
fn landing_on_a_block_spends_the_third_jump() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, k| {
        let b = spawn(ctx, k.block, 80.0, 110.0);
        set_v(ctx, kid, 0.0, 4.0);
        let run = KidCollisionRun::Block(BlockRun::BlockStandard);
        kid::run_collision(ctx, kid, b, &run).unwrap();
        assert!(!kstate(ctx, kid).tjump);
        assert!(kstate(ctx, kid).djump);
    });
}

#[test]
fn the_a3_lists_run_end_to_end() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).expect("step");
            assert_eq!(hv(ctx, kid).0, 3.0);
            assert_eq!(
                ctx.w.get(kid).unwrap().m.gravity(),
                r(0.4),
                "gravity reasserted after the jump code"
            );
            assert!(ctx.w.engine.kid.ext.key_right);
            kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
        },
    );
}
