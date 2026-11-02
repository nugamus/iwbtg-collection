//! #231: the kid's `[sounds]` play through `sound::play` from jump, double jump, shoot, vine kick
//! and the death `sound` step (kid/A1.md playerJump, playerShoot, Step action 4, killPlayer).

use iwbtg_sim::def::physics::DeathStep;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef, SoundDef,
    SoundId, SoundKind,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::api::{self, KillCause};
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::{self, DllMode};
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

const JUMP: SoundId = SoundId(0);
const DJUMP: SoundId = SoundId(1);
const SHOOT: SoundId = SoundId(2);
const VINE: SoundId = SoundId(3);
const DEATH: SoundId = SoundId(4);

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    block: KindId,
    vine_l: KindId,
}

fn game(with_sounds: bool) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("kid-sounds");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(mask);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut bl = KindDef::new("block", Builtin::Inert);
    bl.sprite = Some(tile);
    bl.classes = ClassMask::BLOCK;
    bl.solid = true;
    let block = b.kind(bl);
    let mut v = KindDef::new("vine_l", Builtin::Inert);
    v.sprite = Some(tile);
    v.classes = ClassMask::VINE_L;
    let vine_l = b.kind(v);
    let mut bu = KindDef::new("bullet", Builtin::Inert);
    bu.classes = ClassMask::KID_BULLET;
    let bullet = b.kind(bu);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidBullet, bullet);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p: PhysicsProfile = presets::defaults(KidFamily::A1);
    if with_sounds {
        p.sounds.jump = Some(JUMP);
        p.sounds.double_jump = Some(DJUMP);
        p.sounds.shoot = Some(SHOOT);
        p.sounds.vine_kick = Some(VINE);
        p.sounds.death = Some(DEATH);
    }
    p.death.order = vec![DeathStep::Sound, DeathStep::DestroyKid];
    b.physics(p);
    let mut def = b.build().expect("game builds");
    let sd = |name: &str| SoundDef {
        name: name.to_owned(),
        kind: SoundKind::Normal,
        length_ticks: 100,
        volume: Real::ONE,
        pan: Real::ZERO,
    };
    def.sounds = vec![sd("jump"), sd("djump"), sd("shoot"), sd("vine"), sd("death")];
    (def, Kinds { kid, block, vine_l })
}

fn with_kid<T>(with_sounds: bool, f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T) -> T {
    let (def, k) = game(with_sounds);
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
    f(&mut ctx, kid, &k)
}

fn press(ctx: &mut Ctx, held: &[Action], pressed: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: 0,
    });
}

fn run(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
}

fn playing(ctx: &Ctx, s: SoundId) -> bool {
    sound::is_playing(ctx.w, ctx.def, s, &ctx.rules.sound)
}

#[test]
fn a_ground_jump_plays_the_jump_sound_and_an_air_jump_the_double_jump_sound() {
    with_kid(true, |ctx, kid, k| {
        ctx.spawn(k.block, r(90.0), r(109.0), SpawnInit::default())
            .unwrap();
        press(ctx, &[Action::Jump], &[Action::Jump]);
        run(ctx, kid, CommonAction::Jump);
        assert!(playing(ctx, JUMP));
        assert!(!playing(ctx, DJUMP));
    });
    with_kid(true, |ctx, kid, _| {
        api::refresh_air_jump(ctx, kid);
        press(ctx, &[Action::Jump], &[Action::Jump]);
        run(ctx, kid, CommonAction::Jump);
        assert!(playing(ctx, DJUMP));
        assert!(!playing(ctx, JUMP));
    });
}

#[test]
fn no_jump_press_or_no_air_jump_left_plays_nothing() {
    with_kid(true, |ctx, kid, _| {
        api::set_air_jump(ctx, kid, false);
        press(ctx, &[Action::Jump], &[Action::Jump]);
        run(ctx, kid, CommonAction::Jump);
        assert!(
            !playing(ctx, JUMP) && !playing(ctx, DJUMP),
            "djump is spent in the air"
        );
    });
}

#[test]
fn shooting_plays_the_shoot_sound_only_when_a_bullet_appears() {
    with_kid(true, |ctx, kid, _| {
        press(ctx, &[], &[]);
        run(ctx, kid, CommonAction::Shoot);
        assert!(!playing(ctx, SHOOT), "no press");
        press(ctx, &[Action::Shoot], &[Action::Shoot]);
        run(ctx, kid, CommonAction::Shoot);
        assert!(playing(ctx, SHOOT));
    });
}

#[test]
fn a_vine_kick_plays_the_vine_sound_but_a_drop_does_not() {
    with_kid(true, |ctx, kid, k| {
        ctx.spawn(k.vine_l, r(64.0), r(96.0), SpawnInit::default())
            .unwrap();
        press(ctx, &[Action::Right], &[Action::Right]);
        run(ctx, kid, CommonAction::Vines);
        assert!(!playing(ctx, VINE), "Right without Jump drops");
        press(ctx, &[Action::Jump, Action::Right], &[Action::Right]);
        run(ctx, kid, CommonAction::Vines);
        assert!(playing(ctx, VINE));
    });
}

#[test]
fn the_death_sound_step_plays_the_death_sound() {
    with_kid(true, |ctx, kid, _| {
        api::kill(ctx, kid, KillCause::Boundary).expect("kill");
        assert!(playing(ctx, DEATH));
    });
}

#[test]
fn a_profile_without_sounds_plays_nothing_and_does_not_fail() {
    with_kid(false, |ctx, kid, k| {
        ctx.spawn(k.block, r(90.0), r(109.0), SpawnInit::default())
            .unwrap();
        press(
            ctx,
            &[Action::Jump, Action::Shoot],
            &[Action::Jump, Action::Shoot],
        );
        run(ctx, kid, CommonAction::Jump);
        run(ctx, kid, CommonAction::Shoot);
        api::kill(ctx, kid, KillCause::Boundary).expect("kill");
        for s in [JUMP, DJUMP, SHOOT, VINE, DEATH] {
            assert!(!playing(ctx, s));
        }
    });
}
