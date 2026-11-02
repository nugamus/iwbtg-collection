//! #267: what a save stores as "flipped" (docs/format/builtins/save.md, Saves).
//!
//! A game that flips by the `yflag` model (family B) saves `global.reverse` in `SaveSlot::flipped`
//! and loads it back into `global.reverse`; it never takes the flipped-kind and `+dy` path of the
//! games that flip by kind. The other games keep that path. Stand-in kinds: inert kids.

use iwbtg_sim::def::physics::FlipKind;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, RoomDef};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::{SavedPosition, load_into, save_now};
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct G {
    def: GameDef,
    kid: KindId,
    flipped_kid: KindId,
}

fn game(model: FlipKind) -> G {
    let mut b = GameDefBuilder::new("save-reverse");
    let sprite = b.sprite_rect((8, 8), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    k.persistent = true;
    k.sprite = Some(sprite);
    let kid = b.kind(k);
    let mut f = KindDef::new("kid_flipped", Builtin::Inert);
    f.classes = ClassMask::KID_FLIPPED;
    f.persistent = true;
    f.sprite = Some(sprite);
    let flipped_kid = b.kind(f);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidFlipped, flipped_kid);
    b.room(RoomDef::new("r", 320, 240));
    let mut def = b.build().unwrap();
    def.physics.flip.model = model;
    def.restart.flipped_load_dy = r(4.0);
    G {
        def,
        kid,
        flipped_kid,
    }
}

struct Run {
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
}

fn run(g: &G) -> Run {
    Run {
        rules: RunnerRules::for_runner(g.def.runner, g.def.engine_rev).unwrap(),
        scripts: ScriptRuntime::default(),
        w: World::new(g.def.start_room, 1),
    }
}

fn ctx<'a>(g: &'a G, s: &'a mut Run) -> Ctx<'a> {
    Ctx {
        def: &g.def,
        rules: &s.rules,
        w: &mut s.w,
        scripts: &s.scripts,
        dll_mode: DllMode::Stub,
    }
}

fn spawn_kid(c: &mut Ctx, kind: KindId) -> InstId {
    c.spawn(kind, r(70.0), r(120.0), SpawnInit::default()).unwrap()
}

#[test]
fn a_family_b_save_stores_reverse_and_loads_it_back() {
    let g = game(FlipKind::Yflag);
    let mut s = run(&g);
    let mut c = ctx(&g, &mut s);
    let kid = spawn_kid(&mut c, g.kid);
    c.w.engine.kid.b.reverse = true;
    save_now(&mut c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
    let slot = c.w.save.clone().expect("saved");
    assert!(slot.flipped, "reverse = 1 is stored in `flipped`");

    c.w.engine.kid.b.reverse = false;
    let loaded = load_into(&mut c, &slot).unwrap();
    assert!(
        c.w.engine.kid.b.reverse,
        "the load sets global.reverse from the save"
    );
    let inst = c.w.get(loaded).unwrap();
    assert_eq!(inst.kind, g.kid, "B has one kid kind, even when reversed");
    assert_eq!(inst.m.y(), slot.y, "and no `flipped_load_dy` offset");
}

#[test]
fn a_family_b_save_without_reverse_stores_false() {
    let g = game(FlipKind::Yflag);
    let mut s = run(&g);
    let mut c = ctx(&g, &mut s);
    let kid = spawn_kid(&mut c, g.kid);
    save_now(&mut c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
    let slot = c.w.save.clone().unwrap();
    assert!(!slot.flipped);
    c.w.engine.kid.b.reverse = true;
    load_into(&mut c, &slot).unwrap();
    assert!(
        !c.w.engine.kid.b.reverse,
        "the saved reverse (0) replaces the current one"
    );
}

#[test]
fn a_family_b_save_ignores_a_flipped_kid_class() {
    // Only `global.reverse` counts for B: a kid of the flipped class does not make the save flipped.
    let g = game(FlipKind::Yflag);
    let mut s = run(&g);
    let mut c = ctx(&g, &mut s);
    let kid = spawn_kid(&mut c, g.flipped_kid);
    save_now(&mut c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
    assert!(!c.w.save.as_ref().unwrap().flipped);
}

#[test]
fn a_game_that_flips_by_kind_still_saves_the_kind_and_loads_the_offset() {
    let g = game(FlipKind::SwapObject);
    let mut s = run(&g);
    let mut c = ctx(&g, &mut s);
    let kid = spawn_kid(&mut c, g.flipped_kid);
    c.w.engine.kid.b.reverse = true; // not this game's business: reverse is B's
    save_now(&mut c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
    let slot = c.w.save.clone().unwrap();
    assert!(slot.flipped, "the flipped kind is saved as flipped");
    let loaded = load_into(&mut c, &slot).unwrap();
    let inst = c.w.get(loaded).unwrap();
    assert_eq!(inst.kind, g.flipped_kid);
    assert_eq!(inst.m.y(), slot.y + r(4.0), "flipped_load_dy applies");
}
