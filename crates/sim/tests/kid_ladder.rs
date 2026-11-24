//! Ladders: grab, climb, the entry rules, leaving the area and the jump reset (kid/A1.md Step
//! action 3, End Step).

use iwbtg_sim::def::physics::LadderEntry;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::ladder::LadderAction;
use iwbtg_sim::kid::{self, KidAction};
use iwbtg_sim::world::{Ctx, InstId, SpawnInit};

use crate::kid_motion_support::*;

// ---- Ladders ----

fn ladder(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::Ladder)).expect("ladder");
}

#[test]
fn up_on_a_ladder_grabs_it_climbs_and_stops_gravity() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            ctx.w.get_mut(kid).unwrap().m.set_vspeed(r(5.0)).unwrap();
            input(ctx, &[Action::Up]);
            ladder(ctx, kid);
            assert!(kstate(ctx, kid).ladder && kstate(ctx, kid).ladder_ud);
            let m = &ctx.w.get(kid).unwrap().m;
            assert_eq!(
                (m.y().get(), m.vspeed().get(), m.gravity().get()),
                (97.0, 0.0, 0.0)
            );
            ladder(ctx, kid);
            assert_eq!(ctx.w.get(kid).unwrap().m.y().get(), 94.0);
        },
    );
}

#[test]
fn down_enters_too_unless_the_entry_is_up_only() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Down]);
            ladder(ctx, kid);
            assert!(kstate(ctx, kid).ladder);
            assert_eq!(ctx.w.get(kid).unwrap().m.y().get(), 103.0);
        },
    );
    with_kid(
        |p| p.ladder.entry = LadderEntry::UpOnly,
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Down]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder);
            assert_eq!(
                ctx.w.get(kid).unwrap().m.gravity().get(),
                0.4,
                "gravity reset when off the ladder"
            );
        },
    );
}

#[test]
fn leaving_the_ladder_area_or_jumping_lets_go() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Up, Action::Jump]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder, "Jump held leaves");
            assert!(kstate(ctx, kid).ladder_jump);
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            kstate(ctx, kid).ladder = true;
            input(ctx, &[]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder, "no ladder overlaps any more");
        },
    );
}

#[test]
fn ladder_sprites_and_the_jump_reset() {
    with_kid(
        |_| {},
        |ctx, kid, _, back, updown| {
            kstate(ctx, kid).ladder = true;
            kstate(ctx, kid).ladder_ud = true;
            kstate(ctx, kid).ladder_jump = true;
            kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::LadderSprites)).unwrap();
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(i.img.sprite(), Some(updown), "moving wins over the back sprite");
            assert_eq!(i.img.speed(), r(0.2));
            let _ = back;
            kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::LadderJumpReset)).unwrap();
            assert!(!kstate(ctx, kid).ladder_jump);
        },
    );
}
