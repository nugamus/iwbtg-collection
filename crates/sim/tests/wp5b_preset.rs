//! WP5b.6 (#264): the family B preset's event lists run end to end on a hand-built world
//! (kid/B.md Create, Room Start, Step, End Step).

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KidFamily, KindDef, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

#[test]
fn the_b_lists_run_in_order_without_unsupported_actions() {
    let mut b = GameDefBuilder::new("wp5b-preset");
    let sprite = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(sprite);
    k.classes = ClassMask::KID;
    let kid_kind = b.kind(k);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid_kind);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    b.physics(presets::defaults(KidFamily::B));
    let def = b.build().expect("game builds");
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
    // Already saved: first_play_save has nothing to do.
    w.save = Some(iwbtg_sim::save::SaveSlot {
        room,
        x: r(100.0),
        y: r(100.0),
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    });
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let kid = ctx
        .spawn(kid_kind, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    for ev in [EventKind::Create, EventKind::RoomStart] {
        kid::on_event(&mut ctx, kid, &ev).unwrap_or_else(|e| panic!("{ev:?}: {e}"));
    }
    assert_eq!(
        ctx.w.get(kid).unwrap().m.gravity(),
        Real::ZERO,
        "Create leaves gravity at 0"
    );
    assert_eq!(
        ctx.w.count_of(ClassMask::BOW),
        1,
        "Room Start made the bow, Step does not add another"
    );
    ctx.w.input.latch(InputFrame {
        held: Action::Right.bit(),
        pressed: 0,
        released: 0,
    });
    kid::on_event(&mut ctx, kid, &EventKind::Step).expect("step");
    assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0));
    assert_eq!(ctx.w.count_of(ClassMask::BOW), 1);
    kid::on_event(&mut ctx, kid, &EventKind::EndStep).expect("end step");
    assert_eq!(
        ctx.w.get(kid).unwrap().m.gravity(),
        r(0.4),
        "the End Step turns gravity on"
    );
    let KindState::Kid(s) = &ctx.w.get(kid).unwrap().state else {
        unreachable!()
    };
    assert_eq!((s.yflag, s.cur_jumps), (1, 1));
}
