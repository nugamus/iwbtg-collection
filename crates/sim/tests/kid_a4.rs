//! Kid family A4: the shifting and down-only fall caps, the ground jump and release, machinary's
//! fixed room edges, and the A4 and A5 preset lists (kid/A4.md).

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::physics::{BoundaryModel, FallCapRule};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::world::{Ctx, InstId};

use crate::kid_a4_a5_support::*;

#[test]
fn the_shifting_fall_cap_moves_y_back_by_the_excess_then_clamps() {
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            // Falling at 12: y shifts up by 12 - 8 = 4, vspeed clamps to 9.
            set(ctx, kid, 100.0, 100.0, 0.0, 12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (96.0, 9.0));
            // Rising at -10: y shifts down by 2, vspeed clamps to -9 (both directions).
            set(ctx, kid, 100.0, 100.0, 0.0, -10.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (102.0, -9.0));
            // At or below 8: nothing.
            set(ctx, kid, 100.0, 100.0, 0.0, 8.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (100.0, 8.0));
            // Between 8 and 9: shifted, not clamped.
            set(ctx, kid, 100.0, 100.0, 0.0, 8.5);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (99.5, 8.5));
        },
    );
}

#[test]
fn the_down_only_cap_leaves_a_fast_rise_alone() {
    with_kid(
        KidFamily::A5,
        |p| p.fall_cap.rule = FallCapRule::ClampDownOnly,
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 100.0, 0.0, 12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!(vs(ctx, kid), 9.0);
            set(ctx, kid, 100.0, 100.0, 0.0, -12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!(vs(ctx, kid), -12.0);
        },
    );
}

#[test]
fn a4_ground_jump_leaves_djump_alone_and_release_halves() {
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 109.0);
            if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
                s.djump = false;
            }
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            common(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
            let iwbtg_sim::world::KindState::Kid(s) = &ctx.w.get(kid).unwrap().state else {
                unreachable!()
            };
            assert!(!s.djump, "a ground jump does not touch djump in A4");
            input(ctx, &[], &[], &[Action::Jump]);
            common(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), -4.25, "multiplier 0.5");
        },
    );
}

/// `muteki`: `killPlayer` does nothing, so the position adjustment that follows it shows.
fn tough(ctx: &mut Ctx, kid: InstId) {
    if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
        s.invincible = true;
    }
}

fn boundary(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::IntersectBoundary).expect("boundary");
}

#[test]
fn machinary_edges_are_800_by_608_whatever_the_room_and_wrap_by_those_numbers() {
    // x < 0 in a 7232 wide room: add 800, undo the vertical move. (No room changer: it dies,
    // but the position is adjusted after the kill all the same.)
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, -3.0, 300.0, 0.0, 4.0);
            boundary(ctx, kid);
            assert!(
                ctx.w.get(kid).unwrap().alive,
                "muteki: killPlayer does nothing, the wrap still runs"
            );
            assert_eq!(pos(ctx, kid), (797.0, 296.0));
            assert_eq!(pos(ctx, kid), (797.0, 296.0));
        },
    );
    // Past 800 inside a 7232 wide room counts as outside too.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 900.0, 300.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid), (100.0, 300.0));
        },
    );
    // The top adds 608; the bottom subtracts 600, not 608.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 50.0, -2.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid).1, 606.0);
        },
    );
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 50.0, 620.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid).1, 20.0, "620 - 600");
        },
    );
    // Inside the 800 by 608 box: nothing.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 800.0, 608.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            assert_eq!(pos(ctx, kid), (800.0, 608.0));
        },
    );
}

#[test]
fn the_fixed_model_wires_the_boundary_event() {
    let p = presets::defaults(KidFamily::A4);
    assert_eq!(p.events.boundary, BoundaryModel::IntersectOriginFixed);
    assert!(kid::event_set(&p).intersect_boundary);
}

#[test]
fn the_a4_and_a5_lists_run_end_to_end() {
    for family in [KidFamily::A4, KidFamily::A5] {
        with_kid(
            family,
            |_| {},
            |ctx, kid, _, _| {
                input(ctx, &[Action::Right], &[Action::Right], &[]);
                kid::on_event(ctx, kid, &EventKind::Step).expect("step");
                assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0), "{family:?}");
                kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
            },
        );
    }
}
