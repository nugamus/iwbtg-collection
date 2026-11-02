//! WP5c.4b (#299): group A's flip by object swap (kid/A1.md Step action 5, kid/A2.md player2,
//! kid/A3.md) on hand-built worlds.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef,
};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::actions::flip::FlipAction;
use iwbtg_sim::kid::{self, KidAction, KidState, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    flipped: Option<KindId>,
}

fn game(with_flipped: bool) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5c-flip");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(mask);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    b.engine_kind(EngineKind::Kid, kid);
    let flipped = with_flipped.then(|| {
        let mut kf = KindDef::new("kid2", Builtin::Kid);
        kf.sprite = Some(mask);
        kf.classes = ClassMask::KID_FLIPPED;
        let id = b.kind(kf);
        b.engine_kind(EngineKind::KidFlipped, id);
        id
    });
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let p: PhysicsProfile = presets::defaults(KidFamily::A1);
    b.physics(p);
    (b.build().expect("game builds"), Kinds { kid, flipped })
}

fn with_world<T>(with_flipped: bool, f: impl FnOnce(&mut Ctx, &Kinds) -> T) -> T {
    let (def, k) = game(with_flipped);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx, &k)
}

fn make(ctx: &mut Ctx, kind: KindId) -> InstId {
    let id = ctx
        .spawn(kind, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    kid::run_action(ctx, id, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    id
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn swap(ctx: &mut Ctx, id: InstId) -> Result<(), iwbtg_sim::error::SimError> {
    kid::run_action(ctx, id, &KidAction::Flip(FlipAction::FlipSwap)).map(|_| ())
}

fn of_kind(ctx: &Ctx, kind: KindId) -> Vec<InstId> {
    ctx.w
        .iter_alive()
        .filter(|i| i.kind == kind)
        .map(|i| i.id)
        .collect()
}

#[test]
fn create_of_the_flipped_object_signs_gravity_and_scale() {
    with_world(true, |ctx, k| {
        let n = make(ctx, k.kid);
        assert_eq!(ctx.w.get(n).unwrap().m.gravity(), r(0.4));
        assert!(!kstate(ctx, n).flipped);
        let f = make(ctx, k.flipped.unwrap());
        assert_eq!(ctx.w.get(f).unwrap().m.gravity(), r(-0.4));
        assert_eq!(ctx.w.get(f).unwrap().img.yscale(), r(-1.0));
        assert!(kstate(ctx, f).flipped);
    });
}

#[test]
fn flip_swap_hands_over_to_the_flipped_object_four_up_and_back_four_down() {
    with_world(true, |ctx, k| {
        let n = make(ctx, k.kid);
        kstate(ctx, n).facing = -1;
        swap(ctx, n).expect("no-op with grav 0");
        assert!(of_kind(ctx, k.flipped.unwrap()).is_empty());
        ctx.w.engine.kid.grav = true;
        swap(ctx, n).expect("swap");
        assert!(ctx.w.get(n).is_none_or(|i| !i.alive), "the old instance is gone");
        let f = of_kind(ctx, k.flipped.unwrap());
        assert_eq!(f.len(), 1);
        assert_eq!(ctx.w.get(f[0]).unwrap().m.y().get(), 96.0);
        assert_eq!(ctx.w.engine.kid.image_xscale, -1, "the facing is handed over");
        // A kid that is already flipped matches grav: nothing happens.
        let f0 = make(ctx, k.flipped.unwrap());
        swap(ctx, f0).expect("no-op");
        assert!(ctx.w.get(f0).unwrap().alive);
        // And back.
        ctx.w.engine.kid.grav = false;
        swap(ctx, f0).expect("swap back");
        let back = of_kind(ctx, k.kid);
        assert_eq!(back.len(), 1);
        assert_eq!(ctx.w.get(back[0]).unwrap().m.y().get(), 104.0);
    });
}

#[test]
fn a_game_without_a_flipped_object_cannot_swap() {
    with_world(false, |ctx, k| {
        let n = make(ctx, k.kid);
        ctx.w.engine.kid.grav = true;
        assert!(swap(ctx, n).is_err());
    });
}

#[test]
fn gravity_keeps_its_sign_for_the_flipped_kid() {
    with_world(true, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        ctx.w.get_mut(f).unwrap().m.set_gravity(Real::ZERO).unwrap();
        kid::run_action(ctx, f, &KidAction::Common(CommonAction::Gravity)).unwrap();
        assert_eq!(ctx.w.get(f).unwrap().m.gravity(), r(-0.4));
        let n = make(ctx, k.kid);
        ctx.w.get_mut(n).unwrap().m.set_gravity(Real::ZERO).unwrap();
        kid::run_action(ctx, n, &KidAction::Common(CommonAction::Gravity)).unwrap();
        assert_eq!(ctx.w.get(n).unwrap().m.gravity(), r(0.4));
    });
}

#[test]
fn the_group_a_presets_list_the_swap() {
    for family in [KidFamily::A1, KidFamily::A2, KidFamily::A3] {
        let p = presets::defaults(family);
        assert!(
            p.events.step.contains(&KidAction::Flip(FlipAction::FlipSwap)),
            "{family:?}"
        );
    }
    for family in [KidFamily::A4, KidFamily::A5] {
        let p = presets::defaults(family);
        assert!(
            !p.events.step.contains(&KidAction::Flip(FlipAction::FlipSwap)),
            "{family:?}"
        );
    }
}
