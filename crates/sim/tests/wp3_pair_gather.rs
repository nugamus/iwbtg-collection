//! #781: the collision phase gathers its pairs through the broadphase grid. A differential test
//! runs generated worlds twice, with the grid gather (`Hooks::ENGINE`) and with the full scan
//! (`broadphase: false`), and requires the same probe log (every handler call, in order) and the
//! same state hash after every tick. The handlers move, spawn, destroy, change kind and turn
//! solid, and in half the worlds every solid put-back drops a third instance onto the outer
//! one, so the gather must follow a world that changes inside a turn.
//!
//! #971: the generated kinds cover both of the grid's other buckets. One has a precise (bits)
//! mask whose bounding box is far larger than its solid pixels, so the grid buckets it by the
//! box while the overlap test is per pixel; one has no mask at all, so the grid never returns it
//! (`Slot::Nowhere`) while the scan still tests it. Two more generated fixtures put one
//! instance in the grid's overflow set: at an unsupported angle (its box cannot be computed, so
//! the first overlap test that reaches it raises `Unsupported` on both sides) and with a sprite
//! larger than [`grid::MAX_CELLS`] (its box computes, but every gather still has to test it).
//!
//! `pair_gather_timing` (ignored) prints ms per tick for both at 500, 2,000 and 5,000 instances:
//! `cargo test --release -p iwbtg-sim --test it wp3_pair_gather:: -- --ignored --nocapture`.

use iwbtg_sim::collide::grid;
use iwbtg_sim::def::{Builtin, ClassMask, KindDef, KindId};
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::{ProbeAct, ProbeLog, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::collide::PairOrder;
use iwbtg_sim::tick::{self, EventRun, Hooks};
use iwbtg_sim::world::{Ctx, Flow, InstId, SimStatus};

use crate::wp1_support::{Fx, builder, p, tokens_of};

/// The full scan: the reference.
const SCAN: Hooks = Hooks {
    broadphase: false,
    ..Hooks::ENGINE
};

/// The engine's solid put-back, after which a third instance jumps onto `me`, as a script that
/// moves other instances would make it: an instance the grid has not re-bucketed now overlaps
/// the outer instance of the turn.
fn shove(ctx: &mut Ctx, me: InstId, other: InstId, run: EventRun) -> Result<Flow, SimError> {
    let flow = (Hooks::ENGINE.solid_resolve)(ctx, me, other, run)?;
    let all = ctx.w.instances();
    let pick = usize::try_from((me.0 + other.0) % u64::try_from(all.len()).unwrap()).unwrap();
    let third = all[pick].id;
    if let Some((x, y)) = ctx.w.get(me).map(|i| (i.m.x(), i.m.y()))
        && let Some(inst) = ctx.w.get_mut(third)
    {
        inst.m.set_x(x)?;
        inst.m.set_y(y)?;
    }
    Ok(flow)
}

/// [`SCAN`] and [`Hooks::ENGINE`] with [`shove`].
const SHOVE_SCAN: Hooks = Hooks {
    solid_resolve: shove,
    ..SCAN
};
const SHOVE_GRID: Hooks = Hooks {
    solid_resolve: shove,
    ..Hooks::ENGINE
};

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

    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + i64::try_from(self.below(u64::try_from(hi - lo).unwrap())).unwrap()
    }

    /// A coordinate in `lo..hi` with a fraction, .5 ties included.
    fn coord(&mut self, lo: i64, hi: i64) -> f64 {
        let frac = [0.0, 0.25, 0.5, 0.75][usize::try_from(self.below(4)).unwrap()];
        #[allow(clippy::cast_precision_loss, reason = "small test coordinates")]
        let w = self.int(lo, hi) as f64;
        w + frac
    }

    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[usize::try_from(self.below(u64::try_from(v.len()).unwrap())).unwrap()]
    }
}

fn r(v: f64) -> Real {
    Real::new(v)
}

/// The classes the generated kinds draw from.
const CLASSES: [ClassMask; 5] = [
    ClassMask::KID,
    ClassMask::BLOCK,
    ClassMask::KILLER,
    ClassMask::SAVE,
    ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0),
];

/// An 8 x 5 bits mask whose ten solid pixels sit in the corners and the middle: its bounding
/// box, the box the grid buckets by, is far larger than the pixels the overlap test accepts
/// (#971).
const PRECISE_ROWS: [&str; 5] = ["##....##", "#......#", "#..##..#", "#......#", "##....##"];

/// What [`generated_with`] adds to the world besides the random instances: one instance whose
/// box lands in the grid's overflow set, or none.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Overflow {
    /// Every random instance is bucketed by the grid; no overflow instance.
    None,
    /// One instance at `image_angle` 45, which the engine cannot rotate (C3): its collision box
    /// cannot be computed, so the grid keeps it in the overflow set, which every gather returns,
    /// and the first overlap test that reaches it raises `Unsupported`, as the full scan does.
    Angle,
    /// One instance whose sprite is larger than [`grid::MAX_CELLS`] cells: its box computes, but
    /// the grid cannot bucket it, so it joins the overflow set and every gather tests it.
    TooBig,
}

/// A generated game and world: `kinds` probe kinds with random sprites (some none), classes and
/// handlers, `n` instances in a 400 x 300 area (dense, so many pairs overlap), with random
/// reactions to collisions and steps. The same seed gives the same fixture.
///
/// Beyond the random kinds, two are fixed (#971): `precise` has a bits mask the grid buckets by
/// its bounding box while overlap is per pixel, and `bare` has no mask at all, so the grid never
/// returns it (`Slot::Nowhere`) while the scan still tests it.
fn generated(seed: u64, n: usize) -> Fx {
    generated_with(seed, n, Overflow::None).0
}

/// [`generated`], plus the one instance whose box lands in the grid's overflow set when
/// `overflow` asks for one: the fixture and that instance's id.
fn generated_with(seed: u64, n: usize, overflow: Overflow) -> (Fx, Option<InstId>) {
    let mut rng = Rng(seed);
    let mut b = builder();
    let mut kinds: Vec<KindId> = Vec::new();
    for k in 0..6 {
        let mut kd = KindDef::new(format!("k{k}"), Builtin::Probe);
        kd.events = probe_event_set();
        kd.classes = rng.pick(&CLASSES);
        if rng.below(6) != 0 {
            let size = (
                u32::try_from(rng.int(1, 48)).unwrap(),
                u32::try_from(rng.int(1, 48)).unwrap(),
            );
            let origin = (
                i32::try_from(rng.int(0, 16)).unwrap(),
                i32::try_from(rng.int(0, 16)).unwrap(),
            );
            kd.sprite = Some(b.sprite_rect(size, origin));
        }
        for _ in 0..rng.below(3) {
            kd.events.collisions.push(rng.pick(&CLASSES));
        }
        kinds.push(b.kind(kd));
    }
    // #971: the precise-mask kind: the grid buckets by its 8 x 5 box, the overlap test is per
    // pixel, so a pair the grid returns may still not overlap.
    let mut precise = KindDef::new("precise", Builtin::Probe);
    precise.events = probe_event_set();
    precise.classes = ClassMask::KILLER;
    precise.sprite = Some(b.sprite_bits(&PRECISE_ROWS, (4, 2)));
    precise.events.collisions.push(ClassMask::KILLER);
    kinds.push(b.kind(precise));
    // #971: the maskless kind: no sprite and no mask, so no collision box and no reported box.
    // It takes turns (it has a handler) and the scan tests it against everyone, while the grid
    // never returns it.
    let mut bare = KindDef::new("bare", Builtin::Probe);
    bare.events = probe_event_set();
    bare.classes = ClassMask::BLOCK;
    bare.events.collisions.push(ClassMask::BLOCK);
    kinds.push(b.kind(bare));
    // The overflow kind is never drawn for the random instances: its box is the point.
    let overflow_kind = match overflow {
        Overflow::None => None,
        Overflow::Angle => {
            let mut kd = KindDef::new("tilted", Builtin::Probe);
            kd.events = probe_event_set();
            kd.classes = ClassMask::KILLER;
            kd.sprite = Some(b.sprite_rect((16, 16), (8, 8)));
            kd.events.collisions.extend(CLASSES);
            Some(b.kind(kd))
        }
        Overflow::TooBig => {
            // (MAX_CELLS + 2) cells wide on each axis: 64 px cells plus the 2 px pad, so the
            // bucket range alone is already over MAX_CELLS.
            let side = u32::try_from(grid::CELL * (grid::MAX_CELLS + 2)).unwrap();
            let mut kd = KindDef::new("huge", Builtin::Probe);
            kd.events = probe_event_set();
            kd.classes = ClassMask::KILLER;
            kd.sprite = Some(b.sprite_rect((side, side), (0, 0)));
            kd.events.collisions.extend(CLASSES);
            Some(b.kind(kd))
        }
    };
    let mut order = kinds.clone();
    for i in (1..order.len()).rev() {
        let j = usize::try_from(rng.below(u64::try_from(i + 1).unwrap())).unwrap();
        order.swap(i, j);
    }
    if let Some(kind) = overflow_kind {
        match overflow {
            // Last rank: every other instance meets it as an inner instance before its own
            // turn, so the error must come out of the overflow set a gather returns.
            Overflow::Angle => order.push(kind),
            // First rank: its own turn reaches every other instance, so its box over MAX_CELLS
            // must be gathered by every other instance's turn too.
            Overflow::TooBig => order.insert(0, kind),
            Overflow::None => unreachable!("no overflow kind"),
        }
    }
    b.order(&order);
    let mut fx = Fx::new(b.build().unwrap());
    for i in 0..n {
        let kind = rng.pick(&kinds);
        let mut state = p(&format!("_{i}"));
        if rng.below(3) == 0 {
            let (dx, dy) = (rng.coord(-6, 6), rng.coord(-6, 6));
            state = state.with(ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(dx), r(dy))));
        }
        for _ in 0..rng.below(3) {
            let act = match rng.below(7) {
                0 | 1 => ProbeAct::Move(r(rng.coord(-160, 160)), r(rng.coord(-160, 160))),
                2 => ProbeAct::DestroyOther,
                3 => ProbeAct::Spawn {
                    kind: rng.pick(&kinds),
                    x: r(rng.coord(0, 400)),
                    y: r(rng.coord(0, 300)),
                    child: Some(Box::new(p("+"))),
                },
                4 => ProbeAct::ChangeKind(rng.pick(&kinds), rng.below(2) == 0),
                5 => ProbeAct::SetSolid(rng.below(2) == 0),
                _ => ProbeAct::DestroySelf,
            };
            let mut step = ProbeStep::on(ProbeOn::Collision, act);
            if rng.below(2) == 0 {
                step = step.once();
            }
            state = state.with(step);
        }
        let (x, y) = (rng.coord(0, 400), rng.coord(0, 300));
        let id = fx.probe(kind, x, y, state);
        let inst = fx.w.get_mut(id).unwrap();
        inst.solid = rng.below(4) == 0;
        inst.active = rng.below(20) != 0;
    }
    let overflow = overflow_kind.map(|kind| {
        let id = match overflow {
            Overflow::Angle => {
                let id = fx.probe(kind, 200.0, 150.0, p("ovf"));
                // 45 degrees: no exact pixel mapping in engine_rev 0 (C3), so the collision box
                // cannot be computed and the grid's rebuild can only put it in the overflow set.
                fx.w.get_mut(id).unwrap().img.set_angle(Real::new(45.0)).unwrap();
                id
            }
            Overflow::TooBig => {
                // At the room's corner the huge box covers every generated instance, so its
                // pairs must run.
                fx.probe(kind, 0.0, 0.0, p("ovf"))
            }
            Overflow::None => unreachable!("no overflow kind"),
        };
        fx.w.get_mut(id).unwrap().solid = false;
        id
    });
    (fx, overflow)
}

/// How many instances of the generated kind named `name` the fixture holds.
fn count_kind(fx: &Fx, name: &str) -> usize {
    let kind = fx
        .def
        .kinds
        .iter()
        .position(|k| k.name == name)
        .map(|i| KindId(u32::try_from(i).unwrap()));
    fx.w.instances()
        .iter()
        .filter(|inst| Some(inst.kind) == kind)
        .count()
}

/// The instance id a collision log token names, `k<id><tag>`, if the token is one.
fn token_other(token: &str) -> Option<u64> {
    let rest = token.strip_prefix('k')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Builds `fx`'s broadphase grid now, as the first collision phase would, and returns the ids in
/// its overflow set.
fn overflow_ids(fx: &mut Fx) -> Vec<InstId> {
    fx.w.ensure_grid(&fx.def, &fx.rules.collide)
        .expect("build the grid");
    fx.w.grid.overflow().iter().copied().collect()
}

/// Runs `ticks` ticks with `hooks`: the log and state hash after each, then the status.
fn run(fx: &mut Fx, hooks: &Hooks, ticks: u32) -> (Vec<(String, [u8; 32])>, SimStatus) {
    let mut out = Vec::new();
    for _ in 0..ticks {
        if fx.w.status != SimStatus::Running {
            break;
        }
        let mut c = fx.ctx();
        // A halt is part of the outcome: the status below compares it.
        let _ = tick::run_tick_with(&mut c, InputFrame::EMPTY, hooks);
        out.push((fx.take_log(), state_hash(&fx.w)));
    }
    (out, fx.w.status.clone())
}

#[test]
fn pair_gather_matches_the_full_scan_on_generated_worlds() {
    let mut collisions = 0;
    for seed in 1..=40_u64 {
        let n = 20 + usize::try_from(seed * 7 % 120).unwrap();
        let (scan_hooks, grid_hooks) = if seed % 2 == 0 {
            (&SHOVE_SCAN, &SHOVE_GRID)
        } else {
            (&SCAN, &Hooks::ENGINE)
        };
        let mut scan = generated(seed, n);
        let mut grid = generated(seed, n);
        // #971: the precise-mask and maskless kinds are in the world, not only in the def.
        assert!(
            count_kind(&scan, "precise") > 0,
            "seed {seed}: a precise-mask instance"
        );
        assert!(count_kind(&scan, "bare") > 0, "seed {seed}: a maskless instance");
        let (want, want_status) = run(&mut scan, scan_hooks, 12);
        let (got, got_status) = run(&mut grid, grid_hooks, 12);
        for (t, (w, g)) in want.iter().zip(&got).enumerate() {
            assert_eq!(w.0, g.0, "seed {seed}, tick {t}: handler sequence");
            assert_eq!(w.1, g.1, "seed {seed}, tick {t}: state hash");
        }
        assert_eq!(want.len(), got.len(), "seed {seed}: ticks run");
        assert_eq!(want_status, got_status, "seed {seed}: status");
        collisions += want
            .iter()
            .map(|(log, _)| log.matches(" k").count())
            .sum::<usize>();
    }
    assert!(
        collisions > 1000,
        "the worlds collide enough to test: {collisions}"
    );
}

/// #944: the non-default pair orders gather through the grid too (or scan with a stand-in
/// overlap test). Their pair list is fixed before any handler of the turn runs, so the gather
/// must give the same pairs in the same order as the scan: same handler log and state hash.
#[test]
fn pair_gather_matches_the_full_scan_for_the_other_pair_orders() {
    let mut collisions = 0;
    for seed in 1..=24_u64 {
        let n = 20 + usize::try_from(seed * 7 % 120).unwrap();
        let (scan_hooks, grid_hooks) = if seed % 2 == 0 {
            (&SHOVE_SCAN, &SHOVE_GRID)
        } else {
            (&SCAN, &Hooks::ENGINE)
        };
        for order in [PairOrder::OtherRankThenId, PairOrder::HandlerListOrder] {
            let mut scan = generated(seed, n);
            let mut grid = generated(seed, n);
            for fx in [&mut scan, &mut grid] {
                fx.rules.collide.pair_order = order;
                // C7b both ways: with it off the gather's overlap test is the only one.
                fx.rules.collide.recheck_nonsolid = seed % 3 != 0;
            }
            let (want, want_status) = run(&mut scan, scan_hooks, 12);
            let (got, got_status) = run(&mut grid, grid_hooks, 12);
            for (t, (w, g)) in want.iter().zip(&got).enumerate() {
                assert_eq!(
                    w.0, g.0,
                    "seed {seed}, order {order:?}, tick {t}: handler sequence"
                );
                assert_eq!(w.1, g.1, "seed {seed}, order {order:?}, tick {t}: state hash");
            }
            assert_eq!(want.len(), got.len(), "seed {seed}, order {order:?}: ticks run");
            assert_eq!(want_status, got_status, "seed {seed}, order {order:?}: status");
            collisions += want
                .iter()
                .map(|(log, _)| log.matches(" k").count())
                .sum::<usize>();
        }
    }
    assert!(
        collisions > 1000,
        "the worlds collide enough to test: {collisions}"
    );
}

/// #971: an instance at an angle the engine cannot rotate has no computable collision box, so
/// the grid keeps it in the overflow set, which every gather returns. It has the highest rank,
/// so the error comes from another instance's turn, where only the overflow set puts it in the
/// gathered candidates. Both runs must raise the same `Unsupported` on the same tick with the
/// same log and state hash: the error surfaces where the full scan raises it, not when the grid
/// rebuild met it.
#[test]
fn pair_gather_matches_the_full_scan_for_an_unsupported_angle() {
    for seed in 1..=8_u64 {
        let n = 20 + usize::try_from(seed * 5).unwrap();
        let (mut scan, _) = generated_with(seed, n, Overflow::Angle);
        let (mut grid, maybe_ovf) = generated_with(seed, n, Overflow::Angle);
        let ovf = maybe_ovf.expect("the fixture has its overflow instance");

        // The overflow instance is what the grid's rebuild cannot bucket, and it is the only
        // one: its angle makes the box fail, not a size that could hit another instance.
        assert_eq!(
            overflow_ids(&mut grid),
            vec![ovf],
            "seed {seed}: the tilted instance is the grid's overflow set"
        );

        let (want, want_status) = run(&mut scan, &SCAN, 12);
        let (got, got_status) = run(&mut grid, &Hooks::ENGINE, 12);

        // The error actually ran: both sides halted with the unsupported angle, rather than
        // both logging nothing and staying `Running` because neither reached the instance.
        assert!(
            matches!(want_status, SimStatus::Halted(SimError::Unsupported { .. })),
            "seed {seed}: the scan halts with Unsupported, not {want_status:?}"
        );
        assert_eq!(want_status, got_status, "seed {seed}: status");
        assert!(!want.is_empty(), "seed {seed}: the halt tick ran");
        for (t, (w, g)) in want.iter().zip(&got).enumerate() {
            assert_eq!(w.0, g.0, "seed {seed}, tick {t}: handler sequence");
            assert_eq!(w.1, g.1, "seed {seed}, tick {t}: state hash");
        }
        assert_eq!(want.len(), got.len(), "seed {seed}: ticks run");
    }
}

/// #971: a box larger than `grid::MAX_CELLS` cannot be bucketed, so the grid keeps the instance
/// in its overflow set, which every gather returns and tests against the outer instance. The
/// world keeps running: the same pairs must fire, in the same order, on both sides, under every
/// pair order.
#[test]
fn pair_gather_gathers_an_instance_over_max_cells() {
    let mut collisions = 0;
    for seed in 1..=4_u64 {
        let n = 20 + usize::try_from(seed * 4).unwrap();
        for order in [
            PairOrder::LowerRankFirst,
            PairOrder::OtherRankThenId,
            PairOrder::HandlerListOrder,
        ] {
            let (mut scan, _) = generated_with(seed, n, Overflow::TooBig);
            let (mut grid, maybe_ovf) = generated_with(seed, n, Overflow::TooBig);
            let ovf = maybe_ovf.expect("the fixture has its overflow instance");
            for fx in [&mut scan, &mut grid] {
                fx.rules.collide.pair_order = order;
            }
            assert_eq!(
                overflow_ids(&mut grid),
                vec![ovf],
                "seed {seed}, order {order:?}: the huge instance is the grid's overflow set"
            );

            let (want, want_status) = run(&mut scan, &SCAN, 12);
            let (got, got_status) = run(&mut grid, &Hooks::ENGINE, 12);
            for (t, (w, g)) in want.iter().zip(&got).enumerate() {
                assert_eq!(
                    w.0, g.0,
                    "seed {seed}, order {order:?}, tick {t}: handler sequence"
                );
                assert_eq!(w.1, g.1, "seed {seed}, order {order:?}, tick {t}: state hash");
            }
            assert_eq!(want.len(), got.len(), "seed {seed}, order {order:?}: ticks run");
            assert_eq!(want_status, got_status, "seed {seed}, order {order:?}: status");
            assert_eq!(
                want_status,
                SimStatus::Running,
                "seed {seed}, order {order:?}: a box over MAX_CELLS is not an error"
            );

            // Its pairs actually ran: the overflow instance's own collision log entries.
            let hits = want
                .iter()
                .flat_map(|(log, _)| tokens_of(log, "ovf"))
                .filter(|token| token.starts_with('k'))
                .count();
            assert!(
                hits > 0,
                "seed {seed}, order {order:?}: the overflow instance took part in a collision"
            );
            collisions += hits;
            // The per-instance orders pair every instance with it, and its box is in no cell
            // bucket, so an entry that names it as the other instance proves the gather
            // returned it from the overflow set. (`LowerRankFirst` runs those pairs in its own
            // turn, at rank 0, so it cannot show this direction.)
            if order != PairOrder::LowerRankFirst {
                let as_other = want
                    .iter()
                    .flat_map(|(log, _)| log.split(' '))
                    .filter(|token| token_other(token) == Some(ovf.0))
                    .count();
                assert!(
                    as_other > 0,
                    "seed {seed}, order {order:?}: the gather returned the overflow instance"
                );
            }
        }
    }
    assert!(
        collisions > 20,
        "the overflow instance collides enough to test: {collisions}"
    );
}

/// One kid that handles killers (so every bullet is an outer instance, #781) and `n` killer
/// bullets spread over 3200 x 2400 px, each moving every tick. Logs nothing.
fn bullets(n: usize) -> Fx {
    let mut rng = Rng(0x781);
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut kid = KindDef::new("kid", Builtin::Probe);
    kid.events = probe_event_set();
    kid.classes = ClassMask::KID;
    kid.sprite = Some(sprite);
    kid.events.collisions.push(ClassMask::KILLER);
    let kid = b.kind(kid);
    let mut bullet = KindDef::new("bullet", Builtin::Probe);
    bullet.events = probe_event_set();
    bullet.classes = ClassMask::KILLER;
    bullet.sprite = Some(sprite);
    let bullet = b.kind(bullet);
    let mut fx = Fx::new(b.build().unwrap());
    let silent = || ProbeState::new("", None).style(ProbeLog::Silent);
    fx.probe(kid, 400.0, 304.0, silent());
    for _ in 1..n {
        let (dx, dy) = (rng.coord(-4, 4), rng.coord(-4, 4));
        let state = silent().with(ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(dx), r(dy))));
        fx.probe(bullet, rng.coord(0, 3200), rng.coord(0, 2400), state);
    }
    fx
}

/// ms per tick over `ticks` ticks.
#[allow(clippy::disallowed_types, reason = "a timing test measures wall time")]
fn ms_per_tick(fx: &mut Fx, hooks: &Hooks, ticks: u32) -> f64 {
    let start = std::time::Instant::now();
    for _ in 0..ticks {
        let mut c = fx.ctx();
        tick::run_tick_with(&mut c, InputFrame::EMPTY, hooks).expect("tick");
    }
    start.elapsed().as_secs_f64() * 1000.0 / f64::from(ticks)
}

#[test]
#[ignore = "timing; run with --release --ignored --nocapture"]
fn pair_gather_timing() {
    for n in [500, 2_000, 5_000] {
        let ticks = if n > 2_000 { 5 } else { 20 };
        let scan = ms_per_tick(&mut bullets(n), &SCAN, ticks);
        let grid = ms_per_tick(&mut bullets(n), &Hooks::ENGINE, ticks);
        println!("{n:>5} instances: full scan {scan:>9.2} ms/tick, grid gather {grid:>7.2} ms/tick");
    }
    let mut a = bullets(500);
    let mut b = bullets(500);
    for _ in 0..5 {
        ms_per_tick(&mut a, &SCAN, 1);
        ms_per_tick(&mut b, &Hooks::ENGINE, 1);
        assert_eq!(state_hash(&a.w), state_hash(&b.w));
    }
}
