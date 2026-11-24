//! A1 of #1174: one visibility predicate and the two lifetime modes (docs/architecture/sim/world.md).

use iwbtg_sim::collide::{Target, place_meeting};
use iwbtg_sim::def::ClassMask;
use iwbtg_sim::events::dispatch_order;
use iwbtg_sim::rules::{LifetimeRules, RunnerRules, TickRules};
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, World};

use crate::state_support::*;

fn with_lifetime<T>(g: &G, w: &mut World, lifetime: LifetimeRules, f: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules {
        lifetime,
        ..Default::default()
    };
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &g.def,
        rules: &rules,
        w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

/// Every consumer agrees after a destroy in both modes: the instance is stored (gm8) or gone
/// (standard), but the predicate, `iter_visible`, queries, the event walk, collision and
/// `count_of` all say the same thing.
#[test]
fn every_walk_agrees_after_destroy() {
    for lifetime in [LifetimeRules::gm8(), LifetimeRules::standard()] {
        let immediate = lifetime.is_immediate();
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        let gone = spawn(&g, &mut w, g.block, 10.0, 10.0);
        let kept = spawn(&g, &mut w, g.block, 40.0, 40.0);
        with_lifetime(&g, &mut w, lifetime, |c| {
            assert!(c.w.visible(gone, false));
            assert!(c.w.exists(gone));
            assert_eq!(c.w.count_of(ClassMask::BLOCK), 2);

            let _ = c.destroy(gone);

            assert!(!c.w.visible(gone, false));
            assert!(!c.w.exists(gone));
            assert_eq!(c.w.get(gone).is_none(), immediate);
            assert!(!c.w.iter_visible().any(|i| i.id == gone));
            assert_eq!(c.w.count_of(ClassMask::BLOCK), 1);
            assert_eq!(c.w.first_of(ClassMask::BLOCK, &c.rules.tick), Some(kept));
            let walk = dispatch_order(c.w, |_| true);
            assert!(!walk.contains(&gone));
            assert!(walk.contains(&kept));
            // A collision at the destroyed instance's spot sees nothing, stored or not.
            assert!(!place_meeting(c, kept, r(10.0), r(10.0), Target::Class(ClassMask::BLOCK)).unwrap());
        });
        w.sweep();
        assert!(w.get(gone).is_none(), "the sweep leaves nothing behind");
    }
}

/// R22 (`TickRules::dead_in_queries`) is the only way a dead instance is queried, and it never
/// makes it visible to an event walk or `instance_exists`.
#[test]
fn the_r22_opt_in_is_the_only_dead_query_path() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    let gone = spawn(&g, &mut w, g.block, 10.0, 10.0);
    let rules = RunnerRules {
        tick: TickRules {
            dead_in_queries: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &g.def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let _ = ctx.destroy(gone);
    assert!(!ctx.w.visible(gone, false));
    assert!(ctx.w.visible(gone, true));
    assert_eq!(ctx.w.first_of(ClassMask::BLOCK, &ctx.rules.tick), Some(gone));
    assert!(!dispatch_order(ctx.w, |_| true).contains(&gone));
    assert!(ctx.w.get(gone).is_some(), "gm8 keeps it until the sweep");
}
