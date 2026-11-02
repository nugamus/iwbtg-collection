//! WP3: queries, distance_to_object, contact moves and the solid put-back
//! (docs/plans/sim-core.md 3.4, 12; CLONE_SPEC collision).

mod wp3_support;

mod wp3 {
    use iwbtg_sim::collide::contact::{contact_steps, mcs_step_limit, outside_steps};
    use iwbtg_sim::collide::{
        Target, collision_line, collision_point, collision_rectangle, distance_to_object, instance_place,
        instance_position, move_bounce_solid, move_contact_solid, move_outside_solid, place_empty, place_free,
        place_meeting, solid_resolve,
    };
    use iwbtg_sim::def::ClassMask;
    use iwbtg_sim::error::SimError;
    use iwbtg_sim::rules::collide::{DistBoxes, DistCombine, DistOffset, McsStart, SolidKeep};
    use iwbtg_sim::world::Flow;

    use super::wp3_support::{Img, game, motion, r, unless_stubbed};

    /// Plan 12 and C13: the flush-kid test picks the offset. A kid flush against a vine (his
    /// bbox_right 767, the vine's bbox_left 768) is at distance 1 (kid A1 Inferred; Confirmed
    /// (recorder) animus a-vine-right-a row 3), 2 with one free pixel (a-vine-right-b), 0 when
    /// overlapping (a-vine-right-c). Only `DistOffset::PlusOne` gives 1, so it is the default.
    #[test]
    fn flush_kid_is_at_distance_one() {
        let mut g = game(800, 608);
        let vs = g.block_sprite;
        g.add(g.vine, 768.0, 352.0, Img::sprite(vs));
        let k = g.add_kid(762.0, 382.4, 1.0);
        let t = Target::Class(ClassMask::VINE_R);
        assert_eq!(g.rules.collide.dist_offset, DistOffset::PlusOne);
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(1.0)), "flush");
        g.rules.collide.dist_offset = DistOffset::None;
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(0.0)), "None makes flush 0");
        g.rules.collide.dist_offset = DistOffset::PlusOne;
        g.put(k, 761.0, 382.4);
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(2.0)), "one free pixel");
        g.put(k, 763.0, 382.4);
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(0.0)), "overlap");
    }

    /// The facing-left kid measures from his reported bbox_left x-7 (showdown a3-t16-a.csv
    /// row 5: x 613, bbox_left 606, distance 1 to the vine behind).
    #[test]
    fn mirrored_kid_measures_from_reported_box() {
        let mut g = game(800, 608);
        let vs = g.block_sprite;
        g.add(g.vine, 574.0, 320.0, Img::sprite(vs)); // 574..605
        let k = g.add_kid(613.0, 340.4, -1.0);
        let t = Target::Class(ClassMask::VINE_R);
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(1.0)));
        g.rules.collide.dist_boxes = DistBoxes::RoundedInclusive;
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(3.0)), "collision box 608..618");
    }

    #[test]
    fn distance_combines_and_handles_no_target() {
        let mut g = game(800, 608);
        let k = g.add_kid(100.0, 100.0, 1.0); // 95..105 x 88..108
        let t = Target::Class(ClassMask::VINE_R);
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), None, "no instance (A3 U4)");
        let vs = g.block_sprite;
        g.add(g.vine, 108.0, 112.0, Img::sprite(vs)); // dx = 3, dy = 4
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(5.0)));
        g.rules.collide.dist_combine = DistCombine::Max;
        assert_eq!(distance_to_object(&g.ctx(), k, t).unwrap(), Some(r(4.0)));
        g.rules.collide.dist_boxes = DistBoxes::Unrounded;
        g.rules.collide.dist_combine = DistCombine::Euclid;
        g.put(k, 100.5, 100.0); // reported right edge 105.5: dx 2.5, dy 4
        let d = distance_to_object(&g.ctx(), k, t).unwrap().unwrap();
        assert_eq!(d, iwbtg_sim::num::sqrt(r(2.5 * 2.5 + 16.0)));
    }

    #[test]
    fn place_queries_filter_targets() {
        let mut g = game(800, 608);
        let b1 = g.add_block(96.0, 576.0);
        let ps = g.platform_sprite;
        let plat = g.add(g.platform, 128.0, 576.0, Img::sprite(ps));
        let k = g.add_kid(112.0, 567.4, 1.0);
        let block_kind = Target::Kind(g.block);
        let ctx = g.ctx();
        assert!(place_free(&ctx, k, r(112.0), r(567.4)).unwrap());
        assert!(!place_free(&ctx, k, r(112.0), r(568.4)).unwrap());
        assert!(place_free(&ctx, k, r(140.0), r(568.4)).unwrap(), "place_free ignores the non-solid platform");
        assert!(!place_empty(&ctx, k, r(140.0), r(568.4)).unwrap());
        let block = Target::Class(ClassMask::BLOCK);
        assert!(place_meeting(&ctx, k, r(140.0), r(568.4), block).unwrap(), "platform counts as BLOCK");
        assert!(place_meeting(&ctx, k, r(112.0), r(568.4), block_kind).unwrap());
        assert!(!place_meeting(&ctx, k, r(140.0), r(568.4), block_kind).unwrap());
        assert!(place_meeting(&ctx, k, r(140.0), r(568.4), Target::Inst(plat)).unwrap());
        assert!(!place_meeting(&ctx, k, r(112.0), r(567.4), Target::Any).unwrap(), "me is excluded");
        assert_eq!(instance_place(&ctx, k, r(112.0), r(568.4), Target::Any).unwrap(), Some(b1));
    }

    /// R21: instance_place returns the first by rank, then creation; R22 and inactive
    /// instances.
    #[test]
    fn instance_place_order_dead_and_inactive() {
        let mut g = game(800, 608);
        let ps = g.platform_sprite;
        // Created first, but the platform kind has a higher rank than block.
        let plat = g.add(g.platform, 96.0, 576.0, Img::sprite(ps));
        let b2 = g.add_block(96.0, 576.0);
        let b3 = g.add_block(96.0, 576.0);
        let k = g.add_kid(112.0, 567.4, 1.0);
        let at = (r(112.0), r(568.4));
        assert_eq!(instance_place(&g.ctx(), k, at.0, at.1, Target::Any).unwrap(), Some(b2));
        g.rules.tick.first_rule = iwbtg_sim::rules::tick::FirstRule::LowestId;
        assert_eq!(instance_place(&g.ctx(), k, at.0, at.1, Target::Any).unwrap(), Some(plat));
        g.rules.tick.first_rule = iwbtg_sim::rules::tick::FirstRule::DispatchOrder;
        g.w.get_mut(b2).unwrap().alive = false;
        assert_eq!(instance_place(&g.ctx(), k, at.0, at.1, Target::Any).unwrap(), Some(b3), "dead skipped (R22)");
        g.rules.tick.dead_in_queries = true;
        assert_eq!(instance_place(&g.ctx(), k, at.0, at.1, Target::Any).unwrap(), Some(b2));
        g.rules.tick.dead_in_queries = false;
        g.w.get_mut(b3).unwrap().active = false;
        assert_eq!(instance_place(&g.ctx(), k, at.0, at.1, Target::Any).unwrap(), Some(plat), "inactive skipped");
    }

    /// Point, rectangle and line queries: corners inclusive and rounded (C12); non-precise
    /// tests use the reported box, so the mirrored extra column hits (t-col-09-e).
    #[test]
    fn point_rect_line_queries() {
        let mut g = game(800, 608);
        let b = g.add_block(96.0, 576.0); // 96..127 x 576..607
        let ss = g.spike_sprite;
        let sp = g.add(g.spike, 320.0, 288.0, Img::sprite(ss).xscale(-1.0)); // collision 289..320
        let ctx = g.ctx();
        let any = Target::Any;
        assert_eq!(collision_point(&ctx, r(96.0), r(576.0), any, false, None).unwrap(), Some(b));
        assert_eq!(collision_point(&ctx, r(95.5), r(576.0), any, false, None).unwrap(), Some(b), "95.5 rounds to 96");
        assert_eq!(collision_point(&ctx, r(94.5), r(576.0), any, false, None).unwrap(), None, "94.5 rounds to 94");
        assert_eq!(collision_point(&ctx, r(100.0), r(580.0), any, false, Some(b)).unwrap(), None, "notme");
        assert_eq!(instance_position(&ctx, r(127.0), r(607.0), any).unwrap(), Some(b));
        // Mirrored spike: reported box 287..320 (L = 0, R = -33, 34 px), collision box 289..320.
        assert_eq!(collision_point(&ctx, r(287.0), r(319.0), any, false, None).unwrap(), Some(sp));
        assert_eq!(collision_point(&ctx, r(286.0), r(319.0), any, false, None).unwrap(), None);
        assert_eq!(collision_point(&ctx, r(288.0), r(319.0), any, false, None).unwrap(), Some(sp));
        assert_eq!(collision_point(&ctx, r(288.0), r(319.0), any, true, None).unwrap(), None);
        assert_eq!(collision_point(&ctx, r(289.0), r(319.0), any, true, None).unwrap(), Some(sp));
        // The tip (rows 0-1, columns 15-16 mirrored to 304..305) is precise.
        assert_eq!(collision_point(&ctx, r(300.0), r(288.0), any, true, None).unwrap(), None);
        assert_eq!(collision_point(&ctx, r(300.0), r(288.0), any, false, None).unwrap(), Some(sp));
        // Rectangles: inclusive corners, any order.
        assert_eq!(collision_rectangle(&ctx, r(60.0), r(500.0), r(96.0), r(576.0), any, false, None).unwrap(), Some(b));
        assert_eq!(collision_rectangle(&ctx, r(96.0), r(576.0), r(60.0), r(500.0), any, false, None).unwrap(), Some(b));
        assert_eq!(collision_rectangle(&ctx, r(60.0), r(500.0), r(95.0), r(576.0), any, false, None).unwrap(), None);
        assert_eq!(collision_rectangle(&ctx, r(290.0), r(288.0), r(300.0), r(290.0), any, true, None).unwrap(), None);
        assert_eq!(collision_rectangle(&ctx, r(290.0), r(288.0), r(304.0), r(290.0), any, true, None).unwrap(), Some(sp));
        // Lines.
        assert_eq!(collision_line(&ctx, r(0.0), r(590.0), r(200.0), r(590.0), any, false, None).unwrap(), Some(b));
        assert_eq!(collision_line(&ctx, r(0.0), r(500.0), r(95.0), r(575.0), any, false, None).unwrap(), None);
        assert_eq!(collision_line(&ctx, r(0.0), r(500.0), r(96.0), r(576.0), any, false, None).unwrap(), Some(b));
    }

    #[test]
    fn mcs_step_limit_table() {
        assert_eq!(mcs_step_limit(r(3.0)).unwrap(), Some(3));
        assert_eq!(mcs_step_limit(r(2.9)).unwrap(), Some(2));
        assert_eq!(mcs_step_limit(r(0.0)).unwrap(), Some(0));
        assert_eq!(mcs_step_limit(r(-1.0)).unwrap(), None);
        assert!(matches!(mcs_step_limit(r(f64::NAN)), Err(SimError::NonFinite { .. })));
        assert!(matches!(mcs_step_limit(r(1e30)), Err(SimError::Overflow { .. })));
    }

    /// Contact steps (Inferred algorithm; Confirmed (recorder) results): whole-pixel steps keep
    /// the sub-pixel, no partial last step, embedded start per C10, bounded negative maxdist.
    #[test]
    fn contact_steps_follow_the_spec() {
        let mut g = game(800, 608);
        g.add_block(64.0, 544.0); // 64..95
        g.add_block(0.0, 576.0); // a floor 0..31
        let k = g.add_kid(55.5, 567.4, 1.0);
        let ctx = g.ctx();
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let rest = contact_steps((r(55.5), r(567.4)), r(0.0), Some(10), McsStart::ManualNoMove, hits, || Ok(1000));
        assert_eq!(rest.unwrap(), (r(58.5), r(567.4)), "x 58.5 rests flush against 64 (trick-wall-align d)");
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let two = contact_steps((r(39.0), r(567.4)), r(0.0), Some(2), McsStart::ManualNoMove, hits, || Ok(1000));
        assert_eq!(two.unwrap(), (r(41.0), r(567.4)), "maxdist bounds the steps");
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let zero = contact_steps((r(39.0), r(567.4)), r(0.0), Some(0), McsStart::ManualNoMove, hits, || Ok(1000));
        assert_eq!(zero.unwrap(), (r(39.0), r(567.4)), "maxdist 0 does not move (C11)");
        // Landing: from y 560.0 down onto the floor top 576: rests at 567.0.
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let land = contact_steps((r(16.0), r(560.0)), r(270.0), None, McsStart::ManualNoMove, hits, || Ok(1000));
        assert_eq!(land.unwrap(), (r(16.0), r(567.0)), "lands at T-9");
        // Embedded start: no move under the manual, steps under NotChecked.
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let stuck = contact_steps((r(59.0), r(567.4)), r(180.0), Some(3), McsStart::ManualNoMove, hits, || Ok(1000));
        assert_eq!(stuck.unwrap(), (r(59.0), r(567.4)), "1 px inside: no move (C10)");
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let free = contact_steps((r(59.0), r(567.4)), r(180.0), Some(3), McsStart::NotChecked, hits, || Ok(1000));
        assert_eq!(free.unwrap(), (r(56.0), r(567.4)), "start not checked: steps out");
        // Negative maxdist with nothing in the way: bounded, then Unsupported.
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let lost = contact_steps((r(30.0), r(100.0)), r(90.0), None, McsStart::ManualNoMove, hits, || Ok(50));
        assert!(matches!(lost, Err(SimError::Unsupported { .. })));
        // A long positive maxdist past every solid finishes without testing each step.
        let mut tests = 0;
        let counted = |_x, _y| {
            tests += 1;
            Ok(false)
        };
        let far = contact_steps((r(0.0), r(0.0)), r(0.0), Some(10_000), McsStart::ManualNoMove, counted, || Ok(100));
        assert_eq!(far.unwrap(), (r(10_000.0), r(0.0)));
        assert!(tests <= 102, "{tests} tests");
    }

    #[test]
    fn outside_steps_follow_the_spec() {
        let mut g = game(800, 608);
        g.add_block(64.0, 544.0);
        let k = g.add_kid(70.0, 567.4, 1.0);
        let ctx = g.ctx();
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let out = outside_steps((r(70.0), r(567.4)), r(180.0), None, hits, || Ok(1000));
        assert_eq!(out.unwrap(), (r(58.0), r(567.4)), "first free x moving left");
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let short = outside_steps((r(70.0), r(567.4)), r(180.0), Some(3), hits, || Ok(1000));
        assert_eq!(short.unwrap(), (r(67.0), r(567.4)));
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let none = outside_steps((r(30.0), r(567.4)), r(180.0), None, hits, || Ok(1000));
        assert_eq!(none.unwrap(), (r(30.0), r(567.4)), "no overlap: no move");
    }

    #[test]
    fn move_bounce_solid_is_unsupported() {
        let mut g = game(800, 608);
        let k = g.add_kid(100.0, 100.0, 1.0);
        assert!(matches!(
            move_bounce_solid(&mut g.ctx(), k, true),
            Err(SimError::Unsupported { .. })
        ));
    }

    /// The `&mut Ctx` wrappers. They write `x`/`y` through `Motion::set_x`/`set_y` (WP2), so
    /// this test skips until WP2 merges.
    #[test]
    fn contact_wrappers_move_the_instance() {
        let done = unless_stubbed("move_contact_solid wrapper", || {
            let mut g = game(800, 608);
            g.add_block(64.0, 544.0);
            let k = g.add_kid(55.5, 567.4, 1.0);
            move_contact_solid(&mut g.ctx(), k, r(0.0), r(10.0)).unwrap();
            assert_eq!(g.w.get(k).unwrap().m.x(), r(58.5));
            g.put(k, 70.0, 567.4);
            move_outside_solid(&mut g.ctx(), k, r(180.0), r(-1.0)).unwrap();
            assert_eq!(g.w.get(k).unwrap().m.x(), r(58.0));
        });
        let _ = done;
    }

    /// The solid put-back (manual 111_00; Inferred "previous place", corrected by showdown
    /// t-mot-07-e): the event runs at (xprevious, yprevious), then the speeds move the instance;
    /// still colliding keeps it at (xprevious, yprevious) by default. Skips until WP2 merges
    /// (`Motion::set_x`).
    #[test]
    fn solid_resolve_previous_place() {
        let done = unless_stubbed("solid_resolve", || {
            let mut g = game(800, 608);
            let b = g.add_block(96.0, 576.0);
            let k = g.add_kid(112.0, 570.0, 1.0);
            // Landing: previous y 566.0, moved to 570.0 (inside), vspeed 4.
            g.w.get_mut(k).unwrap().m = motion((112.0, 570.0), (112.0, 566.0), (0.0, 4.0));
            let mut seen = None;
            let flow = solid_resolve(&mut g.ctx(), k, b, |ctx| {
                let m = &ctx.w.get(k).unwrap().m;
                seen = Some((m.x(), m.y()));
                Flow::Continue
            })
            .unwrap();
            assert_eq!(flow, Flow::Continue);
            assert_eq!(seen, Some((r(112.0), r(566.0))), "event runs at the previous place");
            assert_eq!(g.w.get(k).unwrap().m.y(), r(566.0), "still colliding: kept at yprevious");
            // EventPosition keeps where the event left it.
            g.rules.collide.solid_keep = SolidKeep::EventPosition;
            g.w.get_mut(k).unwrap().m = motion((112.0, 570.0), (112.0, 566.0), (0.0, 4.0));
            solid_resolve(&mut g.ctx(), k, b, |ctx| {
                ctx.w.get_mut(k).unwrap().m.set_y(r(565.0)).unwrap();
                Flow::Continue
            })
            .unwrap();
            assert_eq!(g.w.get(k).unwrap().m.y(), r(565.0));
        });
        let _ = done;
    }
}
