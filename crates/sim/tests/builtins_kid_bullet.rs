//! kid_bullet: the alarm, the before-move die, the block collision mode and stop-and-die.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, KindDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_rely_a_support::*;

#[test]
fn bullet_create_arms_alarm_zero_and_alarm_destroys() {
    let g = bullet_game(None);
    let mut w = World::new(g.room, 1);
    let b = shoot(&g, &mut w, 100.0, 100.0, 16.0);
    assert_eq!(w.get(b).unwrap().alarms[0], r(40.0));
    fire(&g, &mut w, b, &EventKind::Alarm(0));
    assert!(!w.get(b).unwrap().alive);
    // `life` is a param.
    let g = game(
        |_, _| {
            let mut k = KindDef::new("bullet", Builtin::KidBullet);
            k.params = params(&[("life", real(25.0))]);
            vec![k]
        },
        |_| {},
    );
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "bullet"), 0.0, 0.0);
    assert_eq!(w.get(b).unwrap().alarms[0], r(25.0));
}

#[test]
fn bullet_before_move_dies_at_a_block_one_move_ahead() {
    let g = bullet_game(None);
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.block, 64.0, 0.0);
    // The bullet is 4x4 at x 40: x + 16 = 56 reaches 59, the block starts at 64: free.
    let free = shoot(&g, &mut w, 40.0, 10.0, 16.0);
    fire(&g, &mut w, free, &EventKind::Step);
    assert!(w.get(free).unwrap().alive);
    // At x 50 the next move reaches 66: blocked, destroyed before it moves.
    let hit = shoot(&g, &mut w, 50.0, 10.0, 16.0);
    fire(&g, &mut w, hit, &EventKind::Step);
    assert!(!w.get(hit).unwrap().alive);
    assert_eq!(pos(&w, hit).0, 50.0, "it never moved");
}

#[test]
fn bullet_before_move_passes_invisible_platforms_but_not_visible_ones() {
    let g = bullet_game(None);
    let mut w = World::new(g.room, 1);
    let plat = spawn(&g, &mut w, kind_id(&g, "plat"), 64.0, 0.0);
    let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(
        w.get(b).unwrap().alive,
        "an invisible platform does not stop bullets"
    );
    w.get_mut(plat).unwrap().visible = true;
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!w.get(b).unwrap().alive, "a visible one does");
}

#[test]
fn bullet_collision_destroy_declares_the_block_collision_only() {
    let g = bullet_game(Some("collision_destroy"));
    let k = builtins::event_set(Builtin::KidBullet, kind_of(&g, "bullet"));
    assert_eq!(k.collisions.as_slice(), &[ClassMask::BLOCK]);
    assert!(!k.step, "no Step check in this mode");
    let mut w = World::new(g.room, 1);
    let blk = spawn(&g, &mut w, g.block, 64.0, 0.0);
    let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(w.get(b).unwrap().alive, "Step does not look ahead");
    fire(
        &g,
        &mut w,
        b,
        &EventKind::Collision {
            with: ClassMask::BLOCK,
            other: blk,
        },
    );
    assert!(!w.get(b).unwrap().alive);
    // The default mode declares a Step and no collision.
    let g = bullet_game(None);
    let k = builtins::event_set(Builtin::KidBullet, kind_of(&g, "bullet"));
    assert!(k.step && k.collisions.is_empty());
    assert_eq!(k.alarms, 1);
}

#[test]
fn bullet_stop_and_die_moves_to_contact_then_dies_next_step() {
    let g = bullet_game(Some("stop_and_die"));
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.block, 64.0, 0.0);
    let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(w.get(b).unwrap().alive, "it stops, it does not die yet");
    let i = w.get(b).unwrap();
    assert_eq!(i.m.hspeed(), r(0.0));
    assert_eq!(
        i.m.x(),
        r(60.0),
        "a 4 px bullet at x 60 covers 60..63, next to the block at 64"
    );
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!w.get(b).unwrap().alive);
}
