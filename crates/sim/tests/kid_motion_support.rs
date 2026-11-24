//! Shared fixture for the kid slip, temp-hspeed and ladder tests: a hand-built game with
//! a kid, a slip block and a ladder kind.

use iwbtg_sim::def::ParamValue;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) struct Kinds {
    pub(crate) kid: KindId,
    pub(crate) slip: KindId,
    pub(crate) ladder: KindId,
}

pub(crate) fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, SpriteId, SpriteId) {
    let mut b = GameDefBuilder::new("wp5c-ice");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let back = b.sprite_rect((11, 21), (5, 12));
    let updown = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut sl = KindDef::new("slipblock", Builtin::Block);
    sl.sprite = Some(tile);
    sl.classes = ClassMask::BLOCK | ClassMask::SLIP;
    sl.solid = true;
    let slip = b.kind(sl);
    let mut ld = KindDef::new("ladder", Builtin::Inert);
    ld.sprite = Some(tile);
    ld.classes = ClassMask::LADDER;
    let ladder = b.kind(ld);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = iwbtg_sim::kid::presets::defaults(iwbtg_sim::def::KidFamily::A1);
    p.sprites.back = Some(back);
    p.sprites.ladder_up_down = Some(updown);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds { kid, slip, ladder },
        back,
        updown,
    )
}

pub(crate) fn with_kid<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, SpriteId, SpriteId) -> T,
) -> T {
    let (def, k, back, updown) = game(tweak);
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
    let kid = ctx
        .spawn(k.kid, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    kid::run_action(&mut ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    f(&mut ctx, kid, &k, back, updown)
}

pub(crate) fn input(ctx: &mut Ctx, held: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: 0,
        released: 0,
    });
}

pub(crate) fn walk(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Walk)).expect("walk");
}

pub(crate) fn hs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.hspeed().get()
}

pub(crate) fn set_hs(ctx: &mut Ctx, id: InstId, v: f64) {
    ctx.w.get_mut(id).unwrap().m.set_hspeed(r(v)).unwrap();
}

pub(crate) fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

/// A slip block with `slip` under the kid (its top row at y 109, the kid's feet at 108).
pub(crate) fn slip_block(ctx: &mut Ctx, k: &Kinds, slip: f64) -> InstId {
    let mut init = SpawnInit::default();
    init.params.0.insert("slip".into(), ParamValue::Real(r(slip)));
    ctx.spawn(k.slip, r(80.0), r(109.0), init).expect("slip block")
}
