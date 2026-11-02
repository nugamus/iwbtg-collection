//! WP3: the broadphase grid gives the same answers as brute force (docs/plans/sim-core.md
//! 3.4, 10.6). Runs on every target, wasm32 included.

mod wp3_support;

mod wp3 {
    use iwbtg_sim::collide::grid::Grid;
    use iwbtg_sim::collide::{
        Target, collision_line, collision_point, collision_rectangle, instance_place, place_free, place_meeting,
    };
    use iwbtg_sim::def::ClassMask;
    use iwbtg_sim::error::SimError;
    use iwbtg_sim::world::InstId;
    use iwbtg_sim::{Ctx, World};

    use super::wp3_support::{Game, Img, game, r};

    /// xorshift64*: a fixed, platform-independent sequence.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// 0..n
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        /// A coordinate in `lo..hi` with a fraction from a set that includes exact .5 ties.
        fn coord(&mut self, lo: i64, hi: i64) -> f64 {
            let span = u64::try_from(hi - lo).unwrap();
            let whole = lo + i64::try_from(self.below(span)).unwrap();
            let frac = [0.0, 0.25, 0.4, 0.5, 0.75, 0.4999999999999][usize::try_from(self.below(6)).unwrap()];
            #[allow(clippy::cast_precision_loss, reason = "small test coordinates")]
            let w = whole as f64;
            w + frac
        }

        fn pick<T: Copy>(&mut self, v: &[T]) -> T {
            v[usize::try_from(self.below(u64::try_from(v.len()).unwrap())).unwrap()]
        }
    }

    const W: i64 = 2048;
    const H: i64 = 1024;

    /// A world of `n` random instances: blocks, platforms, kids (both facings), spikes at right
    /// angles, mirrored and scaled, and a few at 45 degrees (unsupported for collision masks:
    /// both sides must give the same error). Some are not solid, inactive or dead.
    fn random_world(rng: &mut Rng, n: usize) -> (Game, Vec<InstId>) {
        let mut g = game(2048, 1024);
        let mut kids = Vec::new();
        for _ in 0..n {
            let x = rng.coord(-100, W + 100);
            let y = rng.coord(-100, H + 100);
            let id = match rng.below(10) {
                0..=3 => g.add_block(x, y),
                4 => {
                    let s = g.platform_sprite;
                    g.add(g.platform, x, y, Img::sprite(s))
                }
                5 | 6 => {
                    let k = g.add_kid(x, y, rng.pick(&[1.0, -1.0]));
                    kids.push(k);
                    k
                }
                _ => {
                    let s = rng.pick(&[g.spike_sprite, g.spike_centre_sprite]);
                    let angle = if rng.below(40) == 0 { 45.0 } else { rng.pick(&[0.0, 90.0, 180.0, 270.0, -90.0]) };
                    let img = Img::sprite(s)
                        .angle(angle)
                        .xscale(rng.pick(&[1.0, -1.0, 0.5, 2.0, -0.5]))
                        .yscale(rng.pick(&[1.0, -1.0, 1.5]));
                    g.add(g.spike, x, y, img)
                }
            };
            let roll = rng.below(20);
            let inst = g.w.get_mut(id).unwrap();
            match roll {
                0 => inst.solid = !inst.solid,
                1 => inst.active = false,
                2 => inst.alive = false,
                _ => {}
            }
        }
        (g, kids)
    }

    /// The answer of one query, for comparing.
    #[derive(Debug, PartialEq)]
    enum Answer {
        Bool(Result<bool, SimError>),
        Id(Result<Option<InstId>, SimError>),
    }

    /// One random query.
    #[derive(Clone, Copy, Debug)]
    enum Query {
        Free(InstId, f64, f64),
        Meeting(InstId, f64, f64, Target),
        Place(InstId, f64, f64, Target),
        Point(f64, f64, Target, bool),
        Rect(f64, f64, f64, f64, Target, bool),
        Line(f64, f64, f64, f64, Target, bool),
    }

    fn random_query(rng: &mut Rng, g: &Game, kids: &[InstId]) -> Query {
        let targets = [
            Target::Any,
            Target::Class(ClassMask::BLOCK),
            Target::Class(ClassMask::KILLER),
            Target::Kind(g.spike),
            Target::Kind(g.kid),
        ];
        let t = rng.pick(&targets);
        let me = rng.pick(kids);
        let x = rng.coord(-150, W + 150);
        let y = rng.coord(-150, H + 150);
        let prec = rng.below(2) == 0;
        let dx = rng.coord(-80, 80);
        let dy = rng.coord(-80, 80);
        match rng.below(6) {
            0 => Query::Free(me, x, y),
            1 => Query::Meeting(me, x, y, t),
            2 => Query::Place(me, x, y, t),
            3 => Query::Point(x, y, t, prec),
            4 => Query::Rect(x, y, x + dx, y + dy, t, prec),
            _ => Query::Line(x, y, x + dx, y + dy, t, prec),
        }
    }

    fn answer(ctx: &Ctx, q: Query) -> Answer {
        match q {
            Query::Free(me, x, y) => Answer::Bool(place_free(ctx, me, r(x), r(y))),
            Query::Meeting(me, x, y, t) => Answer::Bool(place_meeting(ctx, me, r(x), r(y), t)),
            Query::Place(me, x, y, t) => Answer::Id(instance_place(ctx, me, r(x), r(y), t)),
            Query::Point(x, y, t, p) => Answer::Id(collision_point(ctx, r(x), r(y), t, p, None)),
            Query::Rect(x1, y1, x2, y2, t, p) => {
                Answer::Id(collision_rectangle(ctx, r(x1), r(y1), r(x2), r(y2), t, p, None))
            }
            Query::Line(x1, y1, x2, y2, t, p) => Answer::Id(collision_line(ctx, r(x1), r(y1), r(x2), r(y2), t, p, None)),
        }
    }

    /// Runs `n` random queries against the grid world and the brute-force world; returns how
    /// many found something (so the test knows it exercised hits) and how many errored.
    fn compare(rng: &mut Rng, g: &mut Game, brute: &mut World, kids: &[InstId], n: usize) -> (usize, usize) {
        let mut hits = 0;
        let mut errors = 0;
        for i in 0..n {
            let q = random_query(rng, g, kids);
            let a = answer(&g.ctx(), q);
            let b = {
                let ctx = Ctx {
                    def: &g.def,
                    rules: &g.rules,
                    w: brute,
                    scripts: &g.scripts,
                    dll_mode: iwbtg_sim::sound::DllMode::Stub,
                };
                answer(&ctx, q)
            };
            assert_eq!(a, b, "query {i}: {q:?}");
            match a {
                Answer::Bool(Ok(false)) | Answer::Id(Ok(Some(_))) => hits += 1,
                Answer::Bool(Err(_)) | Answer::Id(Err(_)) => errors += 1,
                _ => {}
            }
        }
        (hits, errors)
    }

    /// Plan 3.4 and 12: grid equals brute force on 10,000 random placements, then again after
    /// moves without a flush (moved instances are candidates), after a flush, and after a sweep.
    #[test]
    fn grid_equals_brute_force() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let (mut g, kids) = random_world(&mut rng, 600);
        g.w.grid = Grid::rebuild(&g.w, &g.def, &g.rules.collide).unwrap();
        assert!(g.w.grid.is_built());
        let mut brute = g.w.clone();
        brute.grid = Grid::default();
        assert!(!brute.grid.is_built());

        let (hits, errors) = compare(&mut rng, &mut g, &mut brute, &kids, 10_000);
        assert!(hits > 500, "only {hits} of 10000 queries hit something");
        assert!(errors > 0, "no query reached a 45-degree mask");
        assert!(errors < 2_000, "{errors} errors: too many to test anything else");

        // Move instances without flushing: the grid answers from `moved`.
        let ids: Vec<InstId> = g.w.instances().iter().map(|i| i.id).collect();
        for _ in 0..150 {
            let id = rng.pick(&ids);
            let (x, y) = (rng.coord(-100, W + 100), rng.coord(-100, H + 100));
            g.put(id, x, y);
            brute.get_mut(id).unwrap().m = iwbtg_sim::motion::Motion::new_at(r(x), r(y));
        }
        assert!(!g.w.grid.moved().is_empty());
        compare(&mut rng, &mut g, &mut brute, &kids, 2_000);
        g.w.flush_grid(&g.def, &g.rules.collide).unwrap();
        assert!(g.w.grid.moved().is_empty());
        compare(&mut rng, &mut g, &mut brute, &kids, 2_000);

        // Destroy and sweep some (never the kids the queries use), then flush.
        for _ in 0..100 {
            let id = rng.pick(&ids);
            if kids.contains(&id) {
                continue;
            }
            g.w.get_mut(id).unwrap().alive = false;
            brute.get_mut(id).unwrap().alive = false;
        }
        g.rules.tick.dead_in_queries = true;
        compare(&mut rng, &mut g, &mut brute, &kids, 1_000);
        g.rules.tick.dead_in_queries = false;
        g.w.sweep();
        brute.sweep();
        g.w.flush_grid(&g.def, &g.rules.collide).unwrap();
        compare(&mut rng, &mut g, &mut brute, &kids, 2_000);
    }

    /// A grid built for a room keeps working for instances far outside it and for huge query
    /// rectangles (sparse cells, checked i64 cell math).
    #[test]
    fn grid_handles_far_instances_and_huge_queries() {
        let mut g = game(800, 608);
        let far = g.add_block(1.0e12, -3.0e11);
        let near = g.add_block(64.0, 64.0);
        g.w.grid = Grid::rebuild(&g.w, &g.def, &g.rules.collide).unwrap();
        let ctx = g.ctx();
        assert_eq!(collision_point(&ctx, r(1.0e12 + 5.0), r(-3.0e11 + 5.0), Target::Any, false, None).unwrap(), Some(far));
        let all = collision_rectangle(&ctx, r(-1.0e15), r(-1.0e15), r(1.0e15), r(1.0e15), Target::Any, false, None);
        assert_eq!(all.unwrap(), Some(far), "same rank: the lower id comes first");
        let cands = ctx.w.grid.candidates(i64::MIN, i64::MIN, i64::MAX, i64::MAX).unwrap();
        assert_eq!(cands, vec![far, near]);
    }
}
