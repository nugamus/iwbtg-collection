//! WP5c.4d (#301): poronkusema's `moving_block_por` collision run (kid/A1.md "Collision with
//! movingBlock") on a hand-built world. The block is 64 x 64 at (200, 200) with vspeed 0 unless
//! a test says otherwise.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::collisions::moving_block::MovingBlockRun;
use iwbtg_sim::kid::{self, KidAction, KidCollisionRun, KidState, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

fn game() -> (GameDef, KindId, KindId) {
    let mut b = GameDefBuilder::new("wp5c-mb");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let big = b.sprite_rect((64, 64), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(mask);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut m = KindDef::new("mb", Builtin::Inert);
    m.sprite = Some(big);
    m.classes = ClassMask::MOVING_BLOCK;
    let mb = b.kind(m);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let p: PhysicsProfile = presets::defaults(KidFamily::A1);
    b.physics(p);
    (b.build().expect("game builds"), kid, mb)
}

fn run(prev_y: f64, y: f64, x: f64, kvs: f64, mvs: f64) -> (f64, f64, f64, bool, bool) {
    let (def, kidk, mbk) = game();
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
    let kid: InstId = ctx.spawn(kidk, r(x), r(prev_y), SpawnInit::default()).unwrap();
    kid::run_action(&mut ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).unwrap();
    {
        let m = &mut ctx.w.get_mut(kid).unwrap().m;
        m.set_y(r(y)).unwrap();
        m.set_vspeed(r(kvs)).unwrap();
    }
    if let KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
        let s: &mut KidState = s;
        s.djump = false;
    }
    let mb = ctx.spawn(mbk, r(200.0), r(200.0), SpawnInit::default()).unwrap();
    ctx.w.get_mut(mb).unwrap().m.set_vspeed(r(mvs)).unwrap();
    kid::run_collision(
        &mut ctx,
        kid,
        mb,
        &KidCollisionRun::MovingBlock(MovingBlockRun::MovingBlockPor),
    )
    .unwrap();
    let i = ctx.w.get(kid).unwrap();
    let (x, y, vs) = (i.m.x().get(), i.m.y().get(), i.m.vspeed().get());
    let (on, dj) = match &i.state {
        KindState::Kid(s) => (s.on_platform, s.djump),
        _ => unreachable!(),
    };
    (x, y, vs, on, dj)
}

#[test]
fn ride_from_above_takes_the_block_speed_and_sets_the_flags() {
    // Last step 10 above the block top (y 200), now 2 below the line, falling at 3 against 1.
    let (x, y, vs, on, dj) = run(190.0, 201.0, 230.0, 3.0, 1.0);
    assert_eq!((x, y, vs), (230.0, 191.0, 1.0));
    assert!(on && dj);
}

#[test]
fn bump_from_below_puts_the_kid_under_the_block() {
    // Last step 80 below the top (64 + 10 = 74 needed): y = 200 + 64 + 12; vspeed = block's if >= 0.
    let (_, y, vs, on, _) = run(280.0, 270.0, 230.0, -4.0, 2.0);
    assert_eq!((y, vs), (276.0, 2.0));
    assert!(!on);
    let (_, _, vs, _, _) = run(280.0, 270.0, 230.0, -4.0, -2.0);
    assert_eq!(vs, 0.0, "a rising block gives 0");
}

#[test]
fn otherwise_the_kid_is_pushed_out_sideways() {
    // Beside the block, level with it: left of the middle goes to x - 6, right of it to x + width + 5.
    assert_eq!(run(220.0, 220.0, 210.0, 0.0, 0.0).0, 194.0);
    assert_eq!(run(220.0, 220.0, 250.0, 0.0, 0.0).0, 269.0);
}
