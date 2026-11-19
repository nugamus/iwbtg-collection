//! #781: the collision phase gathers its pairs through the broadphase grid. A differential test
//! runs generated worlds twice, with the grid gather (`Hooks::ENGINE`) and with the full scan
//! (`broadphase: false`), and requires the same probe log (every handler call, in order) and the
//! same state hash after every tick. The handlers move, spawn, destroy, change kind and turn
//! solid, and in half the worlds every solid put-back drops a third instance onto the outer
//! one, so the gather must follow a world that changes inside a turn.
//!
//! `pair_gather_timing` (ignored) prints ms per tick for both at 500, 2,000 and 5,000 instances:
//! `cargo test --release -p iwbtg-sim --test it wp3_pair_gather:: -- --ignored --nocapture`.

use iwbtg_sim::def::{Builtin, ClassMask, KindDef, KindId};
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::{ProbeAct, ProbeLog, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::collide::PairOrder;
use iwbtg_sim::tick::{self, EventRun, Hooks};
use iwbtg_sim::world::{Ctx, Flow, InstId, SimStatus};

use crate::wp1_support::{Fx, builder, p};

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

/// A generated game and world: `kinds` probe kinds with random sprites (some none), classes and
/// handlers, `n` instances in a 400 x 300 area (dense, so many pairs overlap), with random
/// reactions to collisions and steps. The same seed gives the same fixture.
fn generated(seed: u64, n: usize) -> Fx {
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
    let mut order = kinds.clone();
    for i in (1..order.len()).rev() {
        let j = usize::try_from(rng.below(u64::try_from(i + 1).unwrap())).unwrap();
        order.swap(i, j);
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
    fx
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
