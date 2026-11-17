//! WP3: collision queries against empty masks (a no-frame sprite, maskNothing, the inverted
//! rectangle sprite520), CLONE_SPEC collision, "Collisions with these boxes" (research#230,
//! iwbtg-collection#926).

use crate::wp3_support;

mod wp3 {
    use iwbtg_sim::collide::{
        Target, collision_point, collision_rectangle, distance_to_object, instance_place, instance_position,
        place_free, place_meeting, reported_box,
    };
    use iwbtg_sim::def::{IRect, Mask, MaskId, MaskSet, MaskShape, SpriteDef, SpriteId};
    use iwbtg_sim::world::InstId;

    use super::wp3_support::{Game, Img, game, r};

    /// The sprites of help-the-fairy-tale the trace uses (empty-mask-collide.md).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Spr {
        /// 32x32 rectangle, bounds 0,0..31,31 (the block's own sprite; control).
        MaskBlock,
        /// No frames (sprite543 and sprite642): the converter's empty mask.
        NoFrames,
        /// 2x2, fully transparent, manual rectangle left 1, right 0, top 1, bottom 0.
        MaskNothing,
        /// 32x32, fully transparent, manual rectangle left 31, right 0, top 31, bottom 0.
        Sprite520,
    }

    /// Adds an empty-mask sprite to the test game.
    fn add_empty(g: &mut Game, frames: u16, size: (u32, u32), bbox: IRect) -> SpriteId {
        let mask = MaskId(u32::try_from(g.def.masks.len()).expect("few masks"));
        g.def.masks.push(Mask {
            bbox,
            shape: MaskShape::Empty,
        });
        let id = SpriteId(u32::try_from(g.def.sprites.len()).expect("few sprites"));
        g.def.sprites.push(SpriteDef {
            frames,
            size,
            origin: (0, 0),
            masks: MaskSet::Shared(mask),
        });
        id
    }

    fn rect(left: i64, right: i64, top: i64, bottom: i64) -> IRect {
        IRect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// One probe of empty-mask-collide.csv, frame 3 (frames 4 and 5 are identical).
    struct Row {
        k: i64,
        probe: Spr,
        block: Spr,
        at: (f64, f64),
        /// pm, pmb, ipb, pf.
        place: [bool; 4],
        dto: f64,
        /// cpS0, cpS1, crS0, crS1, posS, cpB0, cpB1.
        shape: [bool; 7],
        /// The probe's and the block's bbox_left, bbox_right, bbox_top, bbox_bottom.
        probe_box: IRect,
        block_box: IRect,
    }

    #[allow(clippy::too_many_lines, reason = "the trace's 16 rows, one per line group")]
    fn rows() -> Vec<Row> {
        use Spr::{MaskBlock as B, MaskNothing as N, NoFrames as F, Sprite520 as S};
        let (t, f) = (true, false);
        let row = |k, probe, block, at, place, dto, shape, probe_box, block_box| Row {
            k,
            probe,
            block,
            at,
            place,
            dto,
            shape,
            probe_box,
            block_box,
        };
        vec![
            row(
                0,
                B,
                B,
                (-2990.0, -990.0),
                [t, t, t, f],
                0.0,
                [t, t, t, t, t, t, t],
                rect(-2990, -2959, -990, -959),
                rect(-3000, -2969, -1000, -969),
            ),
            row(
                1,
                F,
                B,
                (-2790.0, -990.0),
                [f, f, f, t],
                0.0,
                [t, f, t, f, f, t, t],
                rect(-2790, -2790, -990, -990),
                rect(-2800, -2769, -1000, -969),
            ),
            row(
                2,
                F,
                B,
                (-2569.0, -969.0),
                [f, f, f, t],
                0.0,
                [t, f, t, f, f, t, t],
                rect(-2569, -2569, -969, -969),
                rect(-2600, -2569, -1000, -969),
            ),
            row(
                3,
                F,
                B,
                (-2368.0, -990.0),
                [f, f, f, t],
                1.0,
                [t, f, t, f, f, t, t],
                rect(-2368, -2368, -990, -990),
                rect(-2400, -2369, -1000, -969),
            ),
            row(
                4,
                F,
                B,
                (-2160.0, -990.0),
                [f, f, f, t],
                9.0,
                [t, f, t, f, f, t, t],
                rect(-2160, -2160, -990, -990),
                rect(-2200, -2169, -1000, -969),
            ),
            row(
                5,
                N,
                B,
                (-1990.0, -990.0),
                [f, f, f, t],
                0.0,
                [t, f, t, f, f, t, t],
                rect(-1990, -1989, -990, -989),
                rect(-2000, -1969, -1000, -969),
            ),
            row(
                6,
                N,
                B,
                (-1801.0, -1001.0),
                [f, f, f, t],
                0.0,
                [t, f, t, f, f, t, t],
                rect(-1801, -1800, -1001, -1000),
                rect(-1800, -1769, -1000, -969),
            ),
            row(
                7,
                N,
                B,
                (-1602.0, -1002.0),
                [f, f, f, t],
                std::f64::consts::SQRT_2,
                [t, f, t, f, f, t, t],
                rect(-1602, -1601, -1002, -1001),
                rect(-1600, -1569, -1000, -969),
            ),
            row(
                8,
                N,
                B,
                (-1360.0, -990.0),
                [f, f, f, t],
                9.0,
                [t, f, t, f, f, t, t],
                rect(-1360, -1359, -990, -989),
                rect(-1400, -1369, -1000, -969),
            ),
            row(
                9,
                S,
                B,
                (-1190.0, -990.0),
                [f, f, f, t],
                14.142_135_623_730_951,
                [f, f, f, f, f, t, t],
                rect(-1159, -1190, -959, -990),
                rect(-1200, -1169, -1000, -969),
            ),
            row(
                10,
                S,
                B,
                (-1020.0, -990.0),
                [f, f, f, t],
                22.360_679_774_997_898,
                [f, f, f, f, f, t, t],
                rect(-989, -1020, -959, -990),
                rect(-1000, -969, -1000, -969),
            ),
            row(
                11,
                F,
                B,
                (-790.0, -990.0),
                [f, f, f, t],
                0.0,
                [t, f, t, f, f, t, t],
                rect(-790, -790, -990, -990),
                rect(-800, -769, -1000, -969),
            ),
            row(
                12,
                B,
                F,
                (-610.0, -1010.0),
                [f, f, f, t],
                0.0,
                [t, t, t, t, t, t, f],
                rect(-610, -579, -1010, -979),
                rect(-600, -600, -1000, -1000),
            ),
            row(
                13,
                B,
                N,
                (-410.0, -1010.0),
                [f, f, f, t],
                0.0,
                [t, t, t, t, t, t, f],
                rect(-410, -379, -1010, -979),
                rect(-400, -399, -1000, -999),
            ),
            row(
                14,
                B,
                F,
                (-199.0, -999.0),
                [f, f, f, t],
                std::f64::consts::SQRT_2,
                [t, t, t, t, t, t, f],
                rect(-199, -168, -999, -968),
                rect(-200, -200, -1000, -1000),
            ),
            row(
                15,
                B,
                B,
                (-10.0, -1010.0),
                [t, t, t, f],
                0.0,
                [t, t, t, t, t, t, t],
                rect(-10, 21, -1010, -979),
                rect(0, 31, -1000, -969),
            ),
        ]
    }

    /// Every collision column of empty-mask-collide.csv (k0-k15, frame 3): place tests never
    /// see an empty mask from either side, non-precise point and rectangle queries hit its
    /// reported box (never sprite520's inverted one), prec 1 and instance_position miss, and
    /// distance_to_object measures the reported box, inverted edges included.
    #[test]
    fn empty_masks_match_empty_mask_collide_trace() {
        let mut g = game(800, 608);
        let no_frames = add_empty(&mut g, 0, (1, 1), rect(0, -1, 0, -1));
        let nothing = add_empty(&mut g, 1, (2, 2), rect(1, 0, 1, 0));
        let s520 = add_empty(&mut g, 1, (32, 32), rect(31, 0, 31, 0));
        let block_sprite = g.block_sprite;
        let sprite = |s: Spr| match s {
            Spr::MaskBlock => block_sprite,
            Spr::NoFrames => no_frames,
            Spr::MaskNothing => nothing,
            Spr::Sprite520 => s520,
        };
        let rows = rows();
        let mut placed: Vec<(InstId, InstId)> = Vec::new();
        for row in &rows {
            // Each case has its own block, 200 px apart (bx = -3000 + 200k, by = -1000).
            let bx = -3000.0 + 200.0 * f64::from(u8::try_from(row.k).expect("k"));
            let b = g.add(g.block, bx, -1000.0, Img::sprite(sprite(row.block)));
            let p = g.add(g.vine, row.at.0, row.at.1, Img::sprite(sprite(row.probe)));
            placed.push((p, b));
        }
        let block = Target::Kind(g.block);
        let ctx = g.ctx();
        for (row, &(p, b)) in rows.iter().zip(&placed) {
            let k = row.k;
            let (x, y) = (r(row.at.0), r(row.at.1));
            let pi = ctx.w.get(p).expect("probe");
            let bi = ctx.w.get(b).expect("block");
            let rules = &ctx.rules.collide;
            assert_eq!(
                reported_box(ctx.def, pi, (pi.m.x(), pi.m.y()), rules).unwrap(),
                Some(row.probe_box),
                "k{k} probe bbox"
            );
            assert_eq!(
                reported_box(ctx.def, bi, (bi.m.x(), bi.m.y()), rules).unwrap(),
                Some(row.block_box),
                "k{k} block bbox"
            );
            let place = [
                place_meeting(&ctx, p, x, y, block).unwrap(),
                place_meeting(&ctx, p, x, y, Target::Inst(b)).unwrap(),
                instance_place(&ctx, p, x, y, block).unwrap() == Some(b),
                place_free(&ctx, p, x, y).unwrap(),
            ];
            assert_eq!(place, row.place, "k{k} pm pmb ipb pf");
            assert_eq!(
                distance_to_object(&ctx, p, Target::Inst(b)).unwrap(),
                Some(r(row.dto)),
                "k{k} dto"
            );
            let me = Target::Inst(p);
            let one = r(1.0);
            let (bx, by) = (bi.m.x(), bi.m.y());
            let shape = [
                collision_point(&ctx, x, y, me, false, None).unwrap() == Some(p),
                collision_point(&ctx, x, y, me, true, None).unwrap() == Some(p),
                collision_rectangle(&ctx, x - one, y - one, x + one, y + one, me, false, None).unwrap()
                    == Some(p),
                collision_rectangle(&ctx, x - one, y - one, x + one, y + one, me, true, None).unwrap()
                    == Some(p),
                instance_position(&ctx, x, y, me).unwrap() == Some(p),
                collision_point(&ctx, bx, by, Target::Inst(b), false, None).unwrap() == Some(b),
                collision_point(&ctx, bx, by, Target::Inst(b), true, None).unwrap() == Some(b),
            ];
            assert_eq!(shape, row.shape, "k{k} cpS0 cpS1 crS0 crS1 posS cpB0 cpB1");
        }
    }
}
