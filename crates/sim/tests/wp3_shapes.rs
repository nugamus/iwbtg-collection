//! WP3 done-when: every query works on rect, precise, disk and diamond masks at unit scale,
//! mirrored and at right angles (docs/plans/sim-core.md 3.4, WP3).
//!
//! Each shape is centred on its origin, and disk and diamond are symmetric under every mirror
//! and right-angle turn, so the transformed solid set must be the unit set shifted by at most one
//! pixel per axis (where the mirror and rotation conventions place the origin). A plain rect mask
//! answers point queries from its reported box, which grows by the bbox rule's extra columns when
//! mirrored or turned (CLONE_SPEC collision, Confirmed (recorder), poronkusema t-col-09-e), so for
//! the rect the set must be a full rectangle at least 15 x 15.
//! The precise spike is not symmetric: for it the pixel count must hold. Then the point,
//! rectangle, line, position and place queries must all agree on one solid pixel and one empty
//! box pixel.

mod wp3_support;

mod wp3 {
    use std::collections::BTreeSet;

    use iwbtg_sim::collide::{
        Target, collision_line, collision_point, collision_rectangle, instance_place, instance_position,
        place_meeting,
    };
    use iwbtg_sim::def::SpriteId;
    use iwbtg_sim::world::InstId;

    use super::wp3_support::{Game, Img, diamond_rows, disk_rows, game, r, spike_rows};

    const X: i64 = 200;
    const Y: i64 = 200;

    fn pixels(rows: &[String]) -> usize {
        rows.iter()
            .map(|row| row.chars().filter(|c| *c == '#').count())
            .sum()
    }

    /// The world pixels where a precise `collision_point` finds `id`, scanned around (X, Y).
    fn solid_set(g: &mut Game, id: InstId) -> BTreeSet<(i64, i64)> {
        let ctx = g.ctx();
        let mut out = BTreeSet::new();
        for py in Y - 40..=Y + 40 {
            for px in X - 40..=X + 40 {
                let hit =
                    collision_point(&ctx, r(px as f64), r(py as f64), Target::Inst(id), true, None).unwrap();
                if hit == Some(id) {
                    out.insert((px, py));
                }
            }
        }
        out
    }

    fn shifted(set: &BTreeSet<(i64, i64)>, dx: i64, dy: i64) -> BTreeSet<(i64, i64)> {
        set.iter().map(|(x, y)| (x + dx, y + dy)).collect()
    }

    fn transforms() -> Vec<(&'static str, f64, f64, f64)> {
        vec![
            ("unit", 1.0, 1.0, 0.0),
            ("mirror x", -1.0, 1.0, 0.0),
            ("mirror y", 1.0, -1.0, 0.0),
            ("angle 90", 1.0, 1.0, 90.0),
            ("angle 180", 1.0, 1.0, 180.0),
            ("angle 270", 1.0, 1.0, 270.0),
        ]
    }

    /// Every query agrees on one solid pixel and one empty pixel of the box.
    fn queries_agree(g: &mut Game, id: InstId, solid: (i64, i64), empty: Option<(i64, i64)>, what: &str) {
        let dot_sprite = g.dot_sprite;
        let dot = g.add(g.spike, 0.0, 0.0, Img::sprite(dot_sprite));
        let t = Target::Inst(id);
        for (p, want, label) in [(Some(solid), true, "solid"), (empty, false, "empty")] {
            let Some((px, py)) = p else { continue };
            let (x, y) = (r(px as f64), r(py as f64));
            let ctx = g.ctx();
            let found = |o: Option<InstId>| o == Some(id);
            assert_eq!(
                found(collision_point(&ctx, x, y, t, true, None).unwrap()),
                want,
                "{what} {label}: point"
            );
            assert_eq!(
                found(collision_rectangle(&ctx, x, y, x, y, t, true, None).unwrap()),
                want,
                "{what} {label}: rectangle"
            );
            assert_eq!(
                found(collision_line(&ctx, x, y, x, y, t, true, None).unwrap()),
                want,
                "{what} {label}: line"
            );
            assert_eq!(
                found(instance_position(&ctx, x, y, t).unwrap()),
                want,
                "{what} {label}: instance_position"
            );
            assert_eq!(
                place_meeting(&ctx, dot, x, y, t).unwrap(),
                want,
                "{what} {label}: place_meeting"
            );
            assert_eq!(
                found(instance_place(&ctx, dot, x, y, t).unwrap()),
                want,
                "{what} {label}: instance_place"
            );
            // Not precise: the box counts, so the empty box pixel hits too.
            assert!(
                found(collision_rectangle(&ctx, x, y, x, y, t, false, None).unwrap()),
                "{what} {label}: box"
            );
        }
        g.w.get_mut(dot).unwrap().alive = false;
        g.w.sweep();
    }

    fn check_symmetric(sprite: fn(&Game) -> SpriteId, count: usize, name: &str) {
        let rect = name == "rect";
        let mut g = game(800, 608);
        let s = sprite(&g);
        let unit = {
            let id = g.add(g.block, X as f64, Y as f64, Img::sprite(s));
            let set = solid_set(&mut g, id);
            g.w.get_mut(id).unwrap().alive = false;
            g.w.sweep();
            set
        };
        assert_eq!(unit.len(), count, "{name}: unit pixel count");
        for (label, xs, ys, angle) in transforms() {
            let id = g.add(
                g.block,
                X as f64,
                Y as f64,
                Img::sprite(s).xscale(xs).yscale(ys).angle(angle),
            );
            let set = solid_set(&mut g, id);
            let what = format!("{name} {label}");
            let left = set.iter().map(|p| p.0).min().unwrap();
            let top = set.iter().map(|p| p.1).min().unwrap();
            if rect {
                let w = set.iter().map(|p| p.0).max().unwrap() - left + 1;
                let h = set.iter().map(|p| p.1).max().unwrap() - top + 1;
                assert_eq!(set.len() as i64, w * h, "{what}: a full rectangle");
                assert!(w >= 15 && h >= 15 && w <= 17 && h <= 17, "{what}: {w} x {h}");
            } else {
                assert_eq!(set.len(), count, "{what}: pixel count");
                let matches = (-1..=1).any(|dx| (-1..=1).any(|dy| shifted(&unit, dx, dy) == set));
                assert!(matches, "{what}: not the unit shape (shifted by at most 1 px)");
            }
            let centre = (left + 7, top + 7);
            assert!(set.contains(&centre));
            // The box corner is empty for disk and diamond; a rect has no empty pixel.
            let corner = (!rect).then_some((left, top));
            if let Some(c) = corner {
                assert!(!set.contains(&c), "{what}: corner is empty");
            }
            queries_agree(&mut g, id, centre, corner, &what);
            g.w.get_mut(id).unwrap().alive = false;
            g.w.sweep();
        }
    }

    #[test]
    fn rect_mask_under_every_transform() {
        check_symmetric(|g| g.rect_centre_sprite, 225, "rect");
    }

    #[test]
    fn disk_mask_under_every_transform() {
        check_symmetric(|g| g.disk_sprite, pixels(&disk_rows()), "disk");
    }

    #[test]
    fn diamond_mask_under_every_transform() {
        check_symmetric(|g| g.diamond_sprite, pixels(&diamond_rows()), "diamond");
    }

    /// The precise spike is not symmetric: every transform keeps its 544 pixels, and the
    /// queries agree on its tip and on an empty corner of its box.
    #[test]
    fn precise_mask_under_every_transform() {
        let mut g = game(800, 608);
        let s = g.spike_centre_sprite;
        assert_eq!(pixels(&spike_rows()), 544);
        for (label, xs, ys, angle) in transforms() {
            let id = g.add(
                g.spike,
                X as f64,
                Y as f64,
                Img::sprite(s).xscale(xs).yscale(ys).angle(angle),
            );
            let set = solid_set(&mut g, id);
            let what = format!("spike {label}");
            assert_eq!(set.len(), 544, "{what}: pixel count");
            let (left, right) = (
                set.iter().map(|p| p.0).min().unwrap(),
                set.iter().map(|p| p.0).max().unwrap(),
            );
            let (top, bottom) = (
                set.iter().map(|p| p.1).min().unwrap(),
                set.iter().map(|p| p.1).max().unwrap(),
            );
            let solid = *set.iter().next().unwrap();
            let empty = [(left, top), (right, top), (left, bottom), (right, bottom)]
                .into_iter()
                .find(|c| !set.contains(c))
                .expect("a triangle leaves a box corner empty");
            queries_agree(&mut g, id, solid, Some(empty), &what);
            g.w.get_mut(id).unwrap().alive = false;
            g.w.sweep();
        }
    }
}
