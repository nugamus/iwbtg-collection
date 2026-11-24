//! E1 of #1153: the stable save-object key the checkpoint persists (`Checkpoint.point`).

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindId, NameId, RoomDef, RoomId};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::{Checkpoint, CheckpointError, SaveSlot, set_point};
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, SpawnInit, World};

/// One room with one placed `save` object, optionally named.
fn fixture(name: Option<&str>) -> (GameDef, RoomId, KindId, SpawnInit) {
    let mut b = GameDefBuilder::new("e1");
    let save = b.kind(iwbtg_sim::def::KindDef::new("save", Builtin::Save));
    let room = b.room(RoomDef::new("room_0", 800, 608));
    b.place(room, save, 100.0, 200.0).unwrap();
    let mut def = b.build().unwrap();
    if let Some(name) = name {
        def.names = vec![name.to_owned()];
    }
    let placed = &mut def.rooms[room.index().unwrap()].instances[0];
    if name.is_some() {
        placed.name = Some(NameId(0));
    }
    let init = SpawnInit {
        rank: Some(placed.rank),
        name: placed.name,
        ..Default::default()
    };
    (def, room, save, init)
}

fn with_ctx<T>(def: &GameDef, w: &mut World, f: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def,
        rules: &rules,
        w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

fn slot(room: RoomId, point: Option<String>) -> SaveSlot {
    SaveSlot {
        room,
        x: Real::new(100.0),
        y: Real::new(200.0),
        precision: Default::default(),
        point,
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

/// A declared room-file name is the key, and a rename refuses the load.
#[test]
fn a_named_save_object_keys_on_its_name() {
    let (def, room, save, init) = fixture(Some("checkpoint_a"));
    let mut w = World::new(room, 1);
    let id = with_ctx(&def, &mut w, |c| {
        c.spawn(save, Real::new(100.0), Real::new(200.0), init).unwrap()
    });
    assert_eq!(w.get(id).unwrap().point.as_deref(), Some("name:checkpoint_a"));

    w.save = Some(slot(room, None));
    with_ctx(&def, &mut w, |c| set_point(c, id));
    assert_eq!(
        w.save.as_ref().unwrap().point.as_deref(),
        Some("name:checkpoint_a")
    );

    let checkpoint = Checkpoint::from_slot(&def, w.save.as_ref().unwrap()).unwrap();
    assert_eq!(checkpoint.point.as_deref(), Some("name:checkpoint_a"));
    let bytes = checkpoint.encode().unwrap();
    assert_eq!(Checkpoint::decode(&bytes).unwrap(), checkpoint);
    assert_eq!(checkpoint.to_slot(&def).unwrap().point, checkpoint.point.clone());

    let mut renamed = def.clone();
    renamed.names = vec!["something_else".into()];
    assert_eq!(
        checkpoint.to_slot(&renamed),
        Err(CheckpointError::NoPoint("name:checkpoint_a".into()))
    );
}

/// An unnamed save object keys on its exact spawn position; a move refuses, a room reorder does
/// not affect it.
#[test]
fn an_unnamed_save_object_keys_on_its_exact_spawn_position() {
    let (def, room, save, init) = fixture(None);
    let mut w = World::new(room, 1);
    let id = with_ctx(&def, &mut w, |c| {
        c.spawn(save, Real::new(100.0), Real::new(200.0), init).unwrap()
    });
    let key = format!(
        "at:room_0/save@{:016x},{:016x}",
        100.0f64.to_bits(),
        200.0f64.to_bits()
    );
    assert_eq!(w.get(id).unwrap().point.as_deref(), Some(key.as_str()));

    let checkpoint = Checkpoint::from_slot(&def, &slot(room, Some(key.clone()))).unwrap();
    assert_eq!(checkpoint.point.as_deref(), Some(key.as_str()));
    assert!(checkpoint.to_slot(&def).is_ok());

    let mut moved = def.clone();
    moved.rooms[room.index().unwrap()].instances[0].x = Real::new(101.0);
    assert_eq!(
        checkpoint.to_slot(&moved),
        Err(CheckpointError::NoPoint(key.clone()))
    );

    let mut reordered = def.clone();
    reordered.rooms.insert(0, RoomDef::new("before", 800, 608));
    assert!(checkpoint.to_slot(&reordered).is_ok());

    let mut removed = def.clone();
    removed.rooms[room.index().unwrap()].instances.clear();
    assert_eq!(checkpoint.to_slot(&removed), Err(CheckpointError::NoPoint(key)));
}

/// A malformed or unknown key is a refusal, never a fallback; a script-spawned save object has
/// no stable placed identity and records no point.
#[test]
fn no_point_is_never_a_fallback() {
    let (def, room, save, _) = fixture(None);
    let mut w = World::new(room, 1);
    let id = with_ctx(&def, &mut w, |c| {
        c.spawn(save, Real::new(50.0), Real::new(60.0), SpawnInit::default())
            .unwrap()
    });
    assert!(w.get(id).unwrap().point.is_none());
    w.save = Some(slot(room, None));
    with_ctx(&def, &mut w, |c| set_point(c, id));
    assert!(w.save.as_ref().unwrap().point.is_none());
    let checkpoint = Checkpoint::from_slot(&def, w.save.as_ref().unwrap()).unwrap();
    assert!(checkpoint.point.is_none());
    assert!(
        checkpoint.to_slot(&def).is_ok(),
        "no point means no identity check"
    );

    for bad in ["bogus", "name:gone", "at:nowhere/save@0,0", "at:room_0/save@zz,0"] {
        let checkpoint = Checkpoint::from_slot(&def, &slot(room, Some(bad.into()))).unwrap();
        assert_eq!(
            checkpoint.to_slot(&def),
            Err(CheckpointError::NoPoint(bad.into())),
            "{bad}"
        );
    }
}

/// #1153 E3: the draw layer is the declared `depth`, not an instance's numeric id. A nearer
/// (lower) depth draws in front even when it was created later, and equal depths tie-break by
/// creation order.
#[test]
fn a_draw_layer_is_the_declared_depth_not_the_instance_id() {
    use iwbtg_sim::def::KindDef;
    use iwbtg_sim::events::draw_order;

    let mut b = GameDefBuilder::new("e3");
    let mut near = KindDef::new("near", Builtin::Inert);
    near.depth = Real::new(-10.0);
    let near = b.kind(near);
    let mut far = KindDef::new("far", Builtin::Inert);
    far.depth = Real::new(10.0);
    let far = b.kind(far);
    b.room(RoomDef::new("room_0", 800, 608));
    let def = b.build().unwrap();

    let mut w = World::new(RoomId(0), 1);
    let spawn = |w: &mut World, kind| {
        with_ctx(&def, w, |c| {
            c.spawn(
                kind,
                Real::ZERO,
                Real::ZERO,
                SpawnInit {
                    rank: Some(0),
                    ..Default::default()
                },
            )
            .unwrap()
        })
    };
    // near_a has the lowest id and far the middle one, so only depth can put far first.
    let near_a = spawn(&mut w, near);
    let far_id = spawn(&mut w, far);
    let near_b = spawn(&mut w, near);
    assert!(near_a < far_id && far_id < near_b);
    assert_eq!(
        draw_order(&w, |_| true),
        vec![far_id, near_a, near_b],
        "depth 10 draws behind depth -10; equal depths keep creation order"
    );
}
