//! Save files: position quantization, save_now, save_counters and load_into.

use iwbtg_sim::def::{ClassMask, EngineKind};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::NumRules;
use iwbtg_sim::save::{
    Checkpoint, CheckpointError, RoomKey, SavePrecision, SavedPosition, load_into, quantize, save_counters,
    save_now,
};
use iwbtg_sim::world::World;

use crate::state_support::*;

// ---- quantize -------------------------------------------------------------------------

fn q(model: SavedPosition, v: f64) -> f64 {
    quantize(r(v), r(v), model, &NumRules::default()).0.get()
}

#[test]
fn base100_rounds_the_last_byte_half_to_even() {
    // Confirmed (recorder): 567.4 gives 567, 538.93 gives 539, 48.5 gives 48, 51.6 gives 52.
    for (v, want) in [
        (567.4, 567.0),
        (538.93, 539.0),
        (48.5, 48.0),
        (51.59999999999991, 52.0),
    ] {
        assert!((q(SavedPosition::XyBase100Bytes, v) - want).abs() < 1e-9, "{v}");
    }
    // Digits above 99 keep their place values: 12345.6 is 1, 23, 45.6 -> 46.
    assert!((q(SavedPosition::XyBase100Bytes, 12345.6) - 12346.0).abs() < 1e-9);
}

#[test]
fn base256_truncates() {
    // Confirmed (recorder): 936.6 gives 936, 263.70000000000005 gives 263.
    assert!((q(SavedPosition::XyBase256Bytes, 936.6) - 936.0).abs() < 1e-9);
    assert!((q(SavedPosition::XyBase256Bytes, 263.700_000_000_000_05) - 263.0).abs() < 1e-9);
}

#[test]
fn text_keeps_two_decimals() {
    // Confirmed (recorder): 551.70 and 542.55 survive; an integer stays whole.
    assert!((q(SavedPosition::XyText2Dec, 551.7) - 551.7).abs() < 1e-12);
    assert!((q(SavedPosition::XyText2Dec, 183.123_456) - 183.12).abs() < 1e-12);
    assert!((q(SavedPosition::XyText2Dec, 48.0) - 48.0).abs() < 1e-12);
}

#[test]
fn previous_fixed_and_floor() {
    assert!((q(SavedPosition::XyPrevious, 12.345_678) - 12.345_678).abs() < 1e-12);
    assert!((q(SavedPosition::FixedPoint, 7.25) - 7.25).abs() < 1e-12);
    assert!((q(SavedPosition::FloorNudge, 7.9) - 7.0).abs() < 1e-12);
}

// ---- save_now, save_counters, load_into --------------------------------------------------

#[test]
fn save_now_stores_room_position_counters_and_flags() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
    with(&g, &mut w, |c| {
        c.w.engine.run.difficulty = 1;
        c.w.engine.run.deaths = 7;
        c.w.engine.run.time = 11;
        c.w.engine.run.clear = true;
        c.w.engine.mech.bosses = 5;
        c.w.engine.mech.items = 6;
        c.w.get_mut(kid).unwrap().m.set_x(r(567.4)).unwrap();
        c.w.get_mut(kid).unwrap().img.set_xscale(r(-1.0)).unwrap();
        save_now(c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
    });
    let s = w.save.clone().unwrap();
    assert_eq!((s.room, s.x, s.y), (g.play, r(567.0), r(200.0)));
    assert_eq!((s.facing, s.flipped, s.difficulty), (-1, false, 1));
    assert_eq!(
        (s.deaths, s.time, s.bosses, s.items, s.clear),
        (7, 11, 5, 6, true)
    );
}

#[test]
fn save_now_previous_fixed_and_flipped_class() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    let kid = spawn(&g, &mut w, g.kid, 10.0, 20.0);
    with(&g, &mut w, |c| {
        let m = &mut c.w.get_mut(kid).unwrap().m;
        m.set_xprevious(r(1.5)).unwrap();
        m.set_yprevious(r(2.5)).unwrap();
        save_now(c, kid, SavedPosition::XyPrevious, None).unwrap();
        assert_eq!(
            (c.w.save.as_ref().unwrap().x, c.w.save.as_ref().unwrap().y),
            (r(1.5), r(2.5))
        );
        assert!(save_now(c, kid, SavedPosition::FixedPoint, None).is_err());
        save_now(c, kid, SavedPosition::FixedPoint, Some((r(3.0), r(4.0)))).unwrap();
        assert_eq!(
            (c.w.save.as_ref().unwrap().x, c.w.save.as_ref().unwrap().y),
            (r(3.0), r(4.0))
        );
    });
    let kid2 = spawn(
        &g,
        &mut w,
        g.def.engine_kind(EngineKind::KidFlipped).unwrap(),
        0.0,
        0.0,
    );
    with(&g, &mut w, |c| {
        save_now(c, kid2, SavedPosition::XyBase100Bytes, None).unwrap();
        assert!(c.w.save.as_ref().unwrap().flipped);
    });
}

#[test]
fn floor_nudge_follows_the_three_checks() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    // A solid 10x10 block at (10, 0); the kid is a 4x4 box.
    spawn(&g, &mut w, g.block, 10.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| {
        c.w.get_mut(kid).unwrap().m.set_x(r(7.5)).unwrap();
        c.w.get_mut(kid).unwrap().m.set_y(r(0.0)).unwrap();
        // The 4x4 box at x 7 reaches pixel 10, which the block holds, so every check
        // fails: step 1 gives x 8.5, step 2 gives y 1, step 3 adds 1 to both, then floor.
        save_now(c, kid, SavedPosition::FloorNudge, None).unwrap();
        let s = c.w.save.clone().unwrap();
        assert_eq!((s.x, s.y), (r(9.0), r(2.0)));
    });
}

#[test]
fn counters_only_save_keeps_the_spot() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.play));
    with(&g, &mut w, |c| {
        c.w.engine.run.deaths = 6;
        c.w.engine.run.time = 10;
        save_counters(c).unwrap();
    });
    let s = w.save.unwrap();
    assert_eq!(
        (s.deaths, s.time, s.x, s.y, s.items),
        (6, 10, r(49.0), r(119.0), 4)
    );
}

#[test]
fn load_into_creates_a_persistent_kid_and_restores_globals() {
    let g = game(|d| d.restart.flipped_load_dy = Real::new(4.0));
    let mut w = World::new(g.play, 1);
    let mut s = slot(g.play);
    let id = with(&g, &mut w, |c| load_into(c, &s).unwrap());
    let inst = w.get(id).unwrap();
    assert!(inst.persistent);
    assert_eq!(
        (inst.m.x(), inst.m.y(), inst.m.xprevious()),
        (r(49.0), r(119.0), r(0.0))
    );
    assert_eq!(inst.img.xscale(), r(-1.0));
    let e = &w.engine;
    assert_eq!(
        (e.run.difficulty, e.run.deaths, e.run.time, e.run.clear),
        (2, 5, 9, true)
    );
    assert_eq!((e.mech.bosses, e.mech.items), (3, 4));
    // Flipped: the flipped kind and the +4 load offset.
    s.flipped = true;
    let id = with(&g, &mut w, |c| load_into(c, &s).unwrap());
    let inst = w.get(id).unwrap();
    assert!(inst.classes.intersects(ClassMask::KID_FLIPPED));
    assert_eq!(inst.m.y(), r(123.0));
}

// ---- restart and reset ------------------------------------------------------------------

// ---- checkpoint --------------------------------------------------------------------------

#[test]
fn save_now_records_the_precision() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
    with(&g, &mut w, |c| {
        save_now(c, kid, SavedPosition::XyBase256Bytes, None).unwrap()
    });
    let slot = w.save.clone().unwrap();
    assert_eq!(slot.precision, SavePrecision::XyBase256Bytes);

    // The precision is runtime provenance, not part of the serialized slot (and so not of the
    // state hash): two slots that differ only in it encode the same.
    let mut other = slot;
    other.precision = SavePrecision::XyBase100Bytes;
    assert_eq!(
        postcard::to_allocvec(&w.save).unwrap(),
        postcard::to_allocvec(&Some(other)).unwrap()
    );
}

#[test]
fn a_checkpoint_round_trips_and_names_the_room() {
    let g = game(|_| {});
    let saved = slot(g.play);
    let checkpoint = Checkpoint::from_slot(&g.def, &saved).unwrap();
    assert_eq!(checkpoint.format, Checkpoint::FORMAT);
    assert_eq!(checkpoint.room, RoomKey("play".into()));
    assert_eq!(checkpoint.precision, SavePrecision::XyBase100Bytes);
    assert!(checkpoint.ruleset.0.starts_with("gm8@"));
    assert!(checkpoint.profile.0.starts_with("gm8/"));

    let bytes = checkpoint.encode().unwrap();
    let decoded = Checkpoint::decode(&bytes).unwrap();
    assert_eq!(decoded, checkpoint);
    assert_eq!(decoded.to_slot(&g.def).unwrap(), saved);
}

#[test]
fn a_checkpoint_resolves_by_name_after_reordering() {
    let g = game(|_| {});
    let saved = slot(g.play);
    let checkpoint = Checkpoint::from_slot(&g.def, &saved).unwrap();

    let mut reordered = g.def.clone();
    let play = reordered.rooms.iter().position(|r| r.name == "play").unwrap();
    reordered.rooms.swap(0, play);
    let resolved = checkpoint.to_slot(&reordered).unwrap();
    assert_eq!(reordered.room(resolved.room).unwrap().name, "play");
    assert_ne!(resolved.room, saved.room, "the numeric room index moved");

    let mut removed = g.def.clone();
    removed.rooms.retain(|r| r.name != "play");
    assert_eq!(
        checkpoint.to_slot(&removed),
        Err(CheckpointError::NoRoom("play".into()))
    );

    assert_eq!(
        Checkpoint::from_slot(&g.def, &slot(g.boot)),
        Err(CheckpointError::NotPlay("boot".into()))
    );
}

#[test]
fn a_checkpoint_format_is_checked() {
    let g = game(|_| {});
    let mut checkpoint = Checkpoint::from_slot(&g.def, &slot(g.play)).unwrap();
    checkpoint.format = Checkpoint::FORMAT + 1;
    let bytes = checkpoint.encode().unwrap();
    assert_eq!(
        Checkpoint::decode(&bytes),
        Err(CheckpointError::Format(Checkpoint::FORMAT + 1))
    );
}
