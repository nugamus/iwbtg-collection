//! E2 of #1153: the explicit persistence scopes (room, run, save).

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, KindId, RoomDef, RoomId};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, Persistence, SpawnInit, World};

struct Fixture {
    def: iwbtg_sim::def::GameDef,
    room: RoomId,
    plain: KindId,
    runner: KindId,
    save: KindId,
    save_persistent: KindId,
}

fn fixture(room_persistent: bool) -> Fixture {
    let mut b = GameDefBuilder::new("e2");
    let plain = b.kind(KindDef::new("plain", Builtin::Inert));
    let mut runner = KindDef::new("runner", Builtin::Inert);
    runner.persistent = true;
    let runner = b.kind(runner);
    let save = b.kind(KindDef::new("save", Builtin::Save));
    let mut save_persistent = KindDef::new("save_persistent", Builtin::Save);
    save_persistent.persistent = true;
    let save_persistent = b.kind(save_persistent);
    let mut room = RoomDef::new("room_0", 800, 608);
    room.persistent = room_persistent;
    let room = b.room(room);
    Fixture {
        def: b.build().unwrap(),
        room,
        plain,
        runner,
        save,
        save_persistent,
    }
}

fn with_ctx<T>(f: &Fixture, w: &mut World, f2: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &f.def,
        rules: &rules,
        w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f2(&mut ctx)
}

fn spawn(f: &Fixture, w: &mut World, kind: KindId) -> InstId {
    with_ctx(f, w, |c| {
        c.spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
            .unwrap()
    })
}

/// The scope is derived from the kind: plain -> Room, persistent -> Run, save -> Save, and a
/// save that is also marked persistent is still Save (the checkpoint writer wins).
#[test]
fn the_scope_comes_from_the_kind() {
    let f = fixture(false);
    let mut w = World::new(f.room, 1);
    for (kind, want) in [
        (f.plain, Persistence::Room),
        (f.runner, Persistence::Run),
        (f.save, Persistence::Save),
        (f.save_persistent, Persistence::Save),
    ] {
        let id = spawn(&f, &mut w, kind);
        let inst = w.get(id).unwrap();
        assert_eq!(inst.persistence, want, "{:?}", inst.kind);
        assert_eq!(
            inst.persistent,
            matches!(want, Persistence::Run) || kind == f.save_persistent
        );
    }
}

/// The room's persistent flag is not an instance scope: a persistent room still has Room-scope
/// instances, and a Run instance lives in a non-persistent room just the same.
#[test]
fn the_room_flag_is_not_an_instance_scope() {
    let f = fixture(true);
    let mut w = World::new(f.room, 1);
    let plain = spawn(&f, &mut w, f.plain);
    let runner = spawn(&f, &mut w, f.runner);
    assert!(f.def.rooms[f.room.index().unwrap()].persistent);
    assert_eq!(w.get(plain).unwrap().persistence, Persistence::Room);
    assert_eq!(w.get(runner).unwrap().persistence, Persistence::Run);
}

/// `change_kind` re-derives the scope from the new kind and the effective persistent flag.
#[test]
fn change_kind_re_derives_the_scope() {
    let f = fixture(false);
    let mut w = World::new(f.room, 1);
    let id = spawn(&f, &mut w, f.plain);
    with_ctx(&f, &mut w, |c| {
        let _ = c.change_kind(id, f.runner, false);
    });
    assert_eq!(w.get(id).unwrap().persistence, Persistence::Run);
    with_ctx(&f, &mut w, |c| {
        let _ = c.change_kind(id, f.save, false);
    });
    assert_eq!(w.get(id).unwrap().persistence, Persistence::Save);
}

/// The scope is runtime provenance: it is not serialized (and so not hashed). Two instances
/// that differ only in it encode identically.
#[test]
fn the_scope_is_not_serialized() {
    let f = fixture(false);
    let mut w = World::new(f.room, 1);
    let id = spawn(&f, &mut w, f.plain);
    let before = postcard::to_allocvec(w.get(id).unwrap()).unwrap();
    w.get_mut(id).unwrap().persistence = Persistence::Save;
    assert_eq!(before, postcard::to_allocvec(w.get(id).unwrap()).unwrap());
}
