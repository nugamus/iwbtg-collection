//! WP3: placement, bounding boxes, standing heights, mirror and spike masks
//! (docs/plans/sim-core.md 3.4, 12; CLONE_SPEC collision).

use crate::wp3_support;

mod wp3 {
    use iwbtg_sim::collide::bbox::reported_box;
    use iwbtg_sim::collide::contact::contact_steps;
    use iwbtg_sim::collide::{Target, collision_point, instance_box, place_at, place_free, place_meeting};
    use iwbtg_sim::def::IRect;
    use iwbtg_sim::rules::collide::{BoxRound, McsStart, PlaceRound};
    use iwbtg_sim::trace::TraceTable;

    use super::wp3_support::{Game, Img, game, r};

    fn rect(l: i64, t: i64, rr: i64, b: i64) -> IRect {
        IRect {
            left: l,
            top: t,
            right: rr,
            bottom: b,
        }
    }

    /// Collision box of `id` at its own position.
    fn cbox(g: &Game, id: iwbtg_sim::world::InstId) -> IRect {
        let inst = g.w.get(id).unwrap();
        instance_box(&g.def, inst, (inst.m.x(), inst.m.y()), &g.rules.collide)
            .unwrap()
            .unwrap()
            .bbox
    }

    /// Reported box of `id` at its own position.
    fn rbox(g: &Game, id: iwbtg_sim::world::InstId) -> IRect {
        let inst = g.w.get(id).unwrap();
        reported_box(&g.def, inst, (inst.m.x(), inst.m.y()), &g.rules.collide)
            .unwrap()
            .unwrap()
    }

    /// Plan 12: the kid box at y = 567.0, 567.4, 567.5, 566.5 (C1, C2; machinary t-col-03 and
    /// t-col-08-a).
    #[test]
    fn kid_bbox_at_fractional_y() {
        let mut g = game(800, 608);
        for (y, top, bottom) in [
            (567.0, 555, 575),
            (567.4, 555, 575),
            (567.5, 556, 576),
            (566.5, 554, 574),
        ] {
            let k = g.add_kid(100.0, y, 1.0);
            assert_eq!(cbox(&g, k), rect(95, top, 105, bottom), "collision box at y {y}");
            assert_eq!(rbox(&g, k), rect(95, top, 105, bottom), "reported box at y {y}");
        }
    }

    /// x .5: the collision box rounds x first by default (trick-wall-align run d), the reported
    /// box per edge (t-col-08-a: x 200.5 reads 196..206).
    #[test]
    fn half_pixel_x_rounding_switch() {
        let mut g = game(800, 608);
        let k = g.add_kid(200.5, 566.5, 1.0);
        assert_eq!(cbox(&g, k), rect(195, 554, 205, 574));
        assert_eq!(rbox(&g, k), rect(196, 554, 206, 574));
        let k2 = g.add_kid(58.5, 567.4, 1.0);
        assert_eq!(cbox(&g, k2).right, 63, "58.5 rests flush against a block at 64");
        assert_eq!(rbox(&g, k2).right, 64);
        g.rules.collide.box_round = BoxRound::AfterOffset;
        assert_eq!(cbox(&g, k), rect(196, 554, 206, 574));
    }

    /// Plan 12: the standing-height table (CLONE_SPEC collision Inferred "Standing heights",
    /// Confirmed (recorder) t-col-03, t-col-08): block top T = 576; free for round(y) <= T-9,
    /// grounded (y+1 collides) for T-9.5 <= y < T-8.5.
    #[test]
    fn standing_height_table() {
        let mut g = game(800, 608);
        g.add_block(96.0, 576.0);
        let k = g.add_kid(112.0, 0.0, 1.0);
        // (y, free at y, grounded: y + 1 collides)
        let table = [
            (566.0, true, false),
            (566.4, true, false),
            (566.5, true, true),
            (567.0, true, true),
            (567.15, true, true),
            (567.4, true, true),
            (567.4999999999999, true, true),
            (567.5, false, true),
            (568.0, false, true),
        ];
        for (y, free, grounded) in table {
            let ctx = g.ctx();
            let y1 = place_at(r(y), r(1.0), &ctx.rules.collide).unwrap();
            assert_eq!(place_free(&ctx, k, r(112.0), r(y)).unwrap(), free, "free at {y}");
            assert_eq!(
                !place_free(&ctx, k, r(112.0), y1).unwrap(),
                grounded,
                "grounded at {y}"
            );
        }
        // C2 alternative: round(y) + 1 makes 566.5 airborne.
        g.rules.collide.place_round = PlaceRound::RoundThenAdd;
        let ctx = g.ctx();
        let y1 = place_at(r(566.5), r(1.0), &ctx.rules.collide).unwrap();
        assert!(place_free(&ctx, k, r(112.0), y1).unwrap());
    }

    /// Plan 12: mirror at x 805/806 (C4, d -> -d; help-the-fairy-tale t-col-01-a row 46): facing
    /// left against a wall ending at 799 the kid stops at 805, while his reported bbox reads
    /// 798..810.
    #[test]
    fn mirrored_kid_stops_at_805() {
        let mut g = game(1600, 1200);
        g.add_block(768.0, 1088.0);
        let k = g.add_kid(806.0, 1100.4, -1.0);
        assert_eq!(cbox(&g, k), rect(801, 1088, 811, 1108));
        g.put(k, 805.0, 1100.4);
        assert_eq!(rbox(&g, k), rect(798, 1088, 810, 1108), "t-col-01-a row 46");
        let ctx = g.ctx();
        assert!(place_free(&ctx, k, r(805.0), r(1100.4)).unwrap());
        assert!(
            !place_free(&ctx, k, r(804.0), r(1100.4)).unwrap(),
            "d -> -d-1 would make 805 collide"
        );
        // move_contact_solid(180, 3) from 806 stops at 805 (t-col-07-d), from 804 does not move.
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let end = contact_steps(
            (r(806.0), r(1100.4)),
            r(180.0),
            Some(3),
            McsStart::ManualNoMove,
            hits,
            || Ok(1000),
        );
        assert_eq!(end.unwrap(), (r(805.0), r(1100.4)));
        let hits = |x, y| place_free(&ctx, k, x, y).map(|f| !f);
        let end = contact_steps(
            (r(804.0), r(1100.4)),
            r(180.0),
            Some(3),
            McsStart::ManualNoMove,
            hits,
            || Ok(1000),
        );
        assert_eq!(
            end.unwrap(),
            (r(804.0), r(1100.4)),
            "embedded start does not move (C10)"
        );
    }

    /// Plan 12: spike pixel counts. The up triangle has 544 pixels (CLONE_SPEC collision,
    /// Confirmed). Exact right-angle rotation (C3 default) keeps 544 at every angle; OpenGMK's
    /// rotated sprSpikeM has 539 and 538 at 180 and 270 (kaenbyou famb-spike-mask-a), which
    /// this default does not reproduce (UNKNOWN, kept for MASK-ROTATE).
    #[test]
    fn spike_pixel_counts() {
        let mut g = game(800, 608);
        let cases = [
            (Img::sprite(g.spike_sprite), 544),
            (Img::sprite(g.spike_sprite).xscale(-1.0), 544),
            (Img::sprite(g.spike_centre_sprite).angle(90.0), 544),
            (Img::sprite(g.spike_centre_sprite).angle(180.0), 544),
            (Img::sprite(g.spike_centre_sprite).angle(270.0), 544),
            (Img::sprite(g.spike_sprite).xscale(0.5).yscale(0.5), 136),
        ];
        let mut ids = Vec::new();
        for (i, (img, want)) in cases.iter().enumerate() {
            let x = 64.0 + 96.0 * f64::from(u8::try_from(i).unwrap());
            ids.push((g.add(g.spike, x, 288.0, *img), *want));
        }
        let boxes: Vec<IRect> = ids.iter().map(|(id, _)| cbox(&g, *id)).collect();
        let ctx = g.ctx();
        for ((id, want), b) in ids.iter().zip(&boxes) {
            let mut n = 0;
            for y in b.top - 1..=b.bottom + 1 {
                for x in b.left - 1..=b.right + 1 {
                    if collision_point(&ctx, r(x as f64), r(y as f64), Target::Inst(*id), true, None)
                        .unwrap()
                        .is_some()
                    {
                        n += 1;
                    }
                }
            }
            assert_eq!(n, *want, "spike {id:?}");
        }
    }

    /// The tip of an up spike only kills where the kid's box reaches an opaque pixel
    /// (CLONE_SPEC collision Inferred "Spikes versus bounding boxes", Confirmed (recorder)).
    #[test]
    fn spike_tip_is_precise() {
        let mut g = game(800, 608);
        let s = g.spike_sprite;
        let spike = g.add(g.spike, 320.0, 288.0, Img::sprite(s));
        let k = g.add_kid(0.0, 0.0, 1.0);
        let ctx = g.ctx();
        let t = Target::Inst(spike);
        // bbox_bottom = 288 (y = 280): rows 0 only; the tip covers columns 335..336.
        assert!(
            place_meeting(&ctx, k, r(336.0), r(280.0), t).unwrap(),
            "box 331..341 covers the tip"
        );
        assert!(
            !place_meeting(&ctx, k, r(329.0), r(280.0), t).unwrap(),
            "box 324..334 misses the tip"
        );
        assert!(
            place_meeting(&ctx, k, r(330.0), r(280.0), t).unwrap(),
            "box 325..335 touches 335"
        );
        // Side touch at floor level: the bottom rows are full width.
        assert!(
            place_meeting(&ctx, k, r(316.0), r(311.0), t).unwrap(),
            "bbox_right 321 at the base"
        );
        assert!(
            !place_meeting(&ctx, k, r(314.0), r(311.0), t).unwrap(),
            "bbox_right 319 left of the spike"
        );
        // Bounding boxes overlap here, pixels do not (upper-left corner of the box).
        assert!(!place_meeting(&ctx, k, r(316.0), r(283.0), t).unwrap());
    }

    /// Reads one research trace, or `None` when `IWBTG_RESEARCH` is unset or the file is
    /// missing.
    #[allow(
        clippy::disallowed_methods,
        reason = "tests may read research files; the sim itself never does IO"
    )]
    fn research_trace(rel: &str) -> Option<TraceTable> {
        let root = std::env::var_os("IWBTG_RESEARCH")?;
        let path = std::path::PathBuf::from(root)
            .join("specs/engines/gm8/traces")
            .join(rel);
        let text = std::fs::read_to_string(&path).ok()?;
        Some(TraceTable::parse(&text).unwrap_or_else(|e| panic!("{rel}: {e}")))
    }

    /// The reported box of the kid's mask (maskPlayer: origin (17, 23), box 12..22 x 11..31)
    /// equals the recorded `bbox_*` on every row of the kid traces that log `image_xscale`
    /// (CLONE_SPEC collision Confirmed (recorder): general bbox rule, the x-7 mirrored box).
    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "trace bbox values are small integers"
    )]
    fn reported_box_matches_kid_traces() {
        let files = [
            "help-the-fairy-tale/t-col-01-a.csv",
            "help-the-fairy-tale/t-col-01-b.csv",
            "machinary/t-col-02.csv",
            "machinary/t-col-03.csv",
            "rely-the-needle-4-2/mask-mirror.csv",
            "showdown/a3-t16-a.csv",
        ];
        #[allow(
            clippy::disallowed_methods,
            reason = "tests may read research files; the sim itself never does IO"
        )]
        let unset = std::env::var_os("IWBTG_RESEARCH").is_none();
        if unset {
            eprintln!("skipped: IWBTG_RESEARCH is not set");
            return;
        }
        let mut rows = 0;
        for f in files {
            let Some(t) = research_trace(f) else {
                eprintln!("skipped {f}: not found");
                continue;
            };
            let col = |n: &str| t.column(n).unwrap_or_else(|| panic!("{f}: no column {n}"));
            let (cx, cy, cs) = (col("x"), col("y"), col("image_xscale"));
            let (cl, ct, cr, cb) = (
                col("bbox_left"),
                col("bbox_top"),
                col("bbox_right"),
                col("bbox_bottom"),
            );
            let mut g = game(4096, 4096);
            for row in &t.rows {
                let v = |c: usize| row[c].as_f64();
                let (Some(x), Some(y), Some(s)) = (v(cx), v(cy), v(cs)) else {
                    continue;
                };
                let k = g.add_kid(x, y, s);
                let b = rbox(&g, k);
                let want = rect(
                    v(cl).unwrap() as i64,
                    v(ct).unwrap() as i64,
                    v(cr).unwrap() as i64,
                    v(cb).unwrap() as i64,
                );
                assert_eq!(
                    b,
                    want,
                    "{f} frame {:?} at ({x}, {y}) xscale {s}",
                    row[0].decimal()
                );
                rows += 1;
            }
        }
        eprintln!("checked {rows} trace rows");
        assert!(rows > 0, "no trace rows checked");
    }
}
