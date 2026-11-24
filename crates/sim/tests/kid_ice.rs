//! Slip blocks: walking speed on ice, the three slip models and the run-speed clamp
//! (kid/A1.md Step action 2).

use iwbtg_sim::def::ParamValue;
use iwbtg_sim::def::physics::IceModel;
use iwbtg_sim::input::Action;
use iwbtg_sim::world::SpawnInit;

use crate::kid_motion_support::*;

// ---- Slip ----

#[test]
fn off_ice_walking_sets_the_speed_outright() {
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn on_ice_walking_builds_up_by_the_slip_and_stops_at_the_run_speed() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.2);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 0.2);
            for _ in 0..30 {
                walk(ctx, kid);
            }
            assert!(
                (hs(ctx, kid) - 3.0).abs() < 0.21,
                "builds up to about 3 and no further"
            );
            let top = hs(ctx, kid);
            walk(ctx, kid);
            assert!(hs(ctx, kid) >= top - 1e-9 && hs(ctx, kid) <= top + 0.2 + 1e-9);
        },
    );
}

#[test]
fn on_ice_without_input_the_speed_decays_by_the_slip_and_may_overshoot_zero() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.4);
            set_hs(ctx, kid, 1.0);
            input(ctx, &[]);
            walk(ctx, kid);
            assert!((hs(ctx, kid) - 0.6).abs() < 1e-9);
            walk(ctx, kid);
            walk(ctx, kid);
            assert!(
                (hs(ctx, kid) + 0.2).abs() < 1e-9,
                "1.0 - 3 * 0.4: past zero, no clamp"
            );
        },
    );
}

#[test]
fn a_block_a_little_below_is_not_ice() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            // The probe is 4 px: a slip block 6 px below does not count.
            let mut init = SpawnInit::default();
            init.params.0.insert("slip".into(), ParamValue::Real(r(0.2)));
            ctx.spawn(k.slip, r(80.0), r(114.0), init).unwrap();
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn model_none_ignores_slip_blocks() {
    with_kid(
        |p| p.ice.model = IceModel::None,
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.2);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn c_clamps_the_slip_to_the_run_speed() {
    with_kid(
        |p| p.ice.clamp = true,
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 2.0);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 2.0);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0, "2 + 2 clamps to 3");
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 2.0);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 4.0, "A1 does not clamp");
        },
    );
}
