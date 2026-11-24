//! Views and runner view follow (docs/architecture/sim/rooms.md, R19).
//!
//! Views affect gameplay through `view_xview` reads, so they are hashed sim state. WP0
//! declares; WP4b owns.
//!
//! The follow is an explicit [`Camera`] preset (target, dead zone, room clamp, smoothing) built
//! from a view each tick; it is derived state and adds nothing to `World`, so behavior and
//! hashes stay identical until the standard ruleset can declare a preset (#1189).

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::error::SimError;
use crate::num::{self, Real};
use crate::rules::NumRules;
use crate::world::{Ctx, InstId, World};

/// One view's run-time state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct View {
    /// `view_enabled` for the room.
    pub enabled: bool,
    /// `view_visible`.
    pub visible: bool,
    /// `view_xview`.
    pub x: Real,
    /// `view_yview`.
    pub y: Real,
    /// `view_wview`.
    pub w: Real,
    /// `view_hview`.
    pub h: Real,
    /// Port on the screen.
    pub port: ViewPort,
    /// What the runner follows.
    pub follow: Option<Target>,
    /// `view_hborder`.
    pub hborder: Real,
    /// `view_vborder`.
    pub vborder: Real,
    /// `view_hspeed`.
    pub hspeed: Real,
    /// `view_vspeed`.
    pub vspeed: Real,
    /// `view_angle`.
    pub angle: Real,
}

/// A view's port on the screen.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ViewPort {
    /// `view_xport`.
    pub x: Real,
    /// `view_yport`.
    pub y: Real,
    /// `view_wport`.
    pub w: Real,
    /// `view_hport`.
    pub h: Real,
}

/// A view's runner follow as an explicit camera preset: what it follows, its dead zone, the
/// room clamp, its smoothing and its size. [`Camera::of`] builds it from the room's [`View`]
/// each tick; the view remains the hashed state, so this adds no field to `World`.
///
/// This is the one place the follow arithmetic lives. The `standard` ruleset will let a room or
/// profile declare the preset directly (E2b, blocked by #1160); until then every camera is
/// derived from the room's view data, and `gm8` behavior, hashes, replays and pins are unchanged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Camera {
    /// What it follows (the first match by R21, [`follow_target`]).
    pub target: Target,
    /// `view_hborder`, `view_vborder`: the dead-zone half-size on each axis. Capped at half the
    /// view in [`Camera::step`], so a larger border still has a reachable target.
    pub dead_zone: (Real, Real),
    /// The largest `view_xview`/`view_yview`: the room size less the view size. Negative when
    /// the view is larger than the room, in which case the view sits at 0.
    pub clamp: (Real, Real),
    /// `view_hspeed`, `view_vspeed`: the most the view moves per tick on each axis. A negative
    /// speed snaps to the target; 0 freezes the axis (partial evidence only).
    pub smoothing: (Real, Real),
    /// `view_wview`, `view_hview`.
    pub size: (Real, Real),
    /// `view_xview`, `view_yview`.
    pub pos: (Real, Real),
}

impl Camera {
    /// The camera a view describes in a room of `room` pixels, or `None` when the view is not
    /// enabled and visible or follows nothing.
    pub fn of(v: &View, room: (Real, Real)) -> Option<Camera> {
        if !v.enabled || !v.visible {
            return None;
        }
        Some(Camera {
            target: v.follow?,
            dead_zone: (v.hborder, v.vborder),
            clamp: (room.0 - v.w, room.1 - v.h),
            smoothing: (v.hspeed, v.vspeed),
            size: (v.w, v.h),
            pos: (v.x, v.y),
        })
    }

    /// The camera's next position, following the point `at` (the followed instance's origin)
    /// this tick. Pure: it reads no world state, so it is the same computation whatever asks.
    ///
    /// Per axis (CLONE_SPEC rooms, "Runner view follow"): the border is
    /// `min(border, view size / 2)` (Confirmed (recorder) horizontally: animus's 600 border
    /// acts as 400 on an 800 view); if the origin minus the border is left of the view, the
    /// target edge is that, else if the origin plus the border is right of the view's far edge,
    /// the target is that minus the view size, else the view stays; clamp the target inside the
    /// room; then move there, at most the speed when it is not negative. OPEN: the origin versus
    /// the bounding box (the kid's box is symmetric) and the check order against other
    /// instances' events (R19).
    pub fn step(&self, at: (Real, Real), r: &NumRules) -> (Real, Real) {
        (
            follow_axis(
                at.0,
                self.pos.0,
                self.size.0,
                self.dead_zone.0,
                self.smoothing.0,
                self.clamp.0,
                r,
            ),
            follow_axis(
                at.1,
                self.pos.1,
                self.size.1,
                self.dead_zone.1,
                self.smoothing.1,
                self.clamp.1,
                r,
            ),
        )
    }
}

/// One axis of the follow: the new view origin, inside `[0, clamp]`.
fn follow_axis(
    at: Real,
    view: Real,
    size: Real,
    border: Real,
    speed: Real,
    clamp: Real,
    r: &NumRules,
) -> Real {
    let border = num::gm_min(border, size / Real::new(2.0), r);
    let mut target = view;
    if num::gm_lt(at - border, view, r) {
        target = at - border;
    } else if num::gm_gt(at + border, view + size, r) {
        target = at + border - size;
    }
    // Inside the room: the far edge first, then 0, so a view larger than the room sits at 0.
    target = num::gm_max(num::gm_min(target, clamp, r), Real::ZERO, r);
    if speed.get() < 0.0 {
        target
    } else if num::gm_gt(target, view, r) {
        num::gm_min(view + speed, target, r)
    } else {
        num::gm_max(view - speed, target, r)
    }
}

/// The runner's view follow, once per tick before the Draw slot (Inferred, R19): each enabled,
/// visible view with a followed target builds its [`Camera`] and steps it to the first match of
/// the target by R21 (active and alive).
pub fn follow_update(ctx: &mut Ctx) -> Result<(), SimError> {
    let Some(room) = ctx.def.room(ctx.w.room) else {
        return Ok(());
    };
    let room = (
        Real::new(f64::from(room.width)),
        Real::new(f64::from(room.height)),
    );
    for i in 0..ctx.w.views.len() {
        let Some(v) = ctx.w.views.get(i) else { continue };
        let Some(camera) = Camera::of(v, room) else {
            continue;
        };
        let Some(id) = follow_target(ctx, camera.target) else {
            continue;
        };
        let Some(inst) = ctx.w.get(id) else { continue };
        let (x, y) = camera.step((inst.m.x(), inst.m.y()), &ctx.rules.num);
        if let Some(v) = ctx.w.views.get_mut(i) {
            v.x = x;
            v.y = y;
        }
    }
    Ok(())
}

/// The instance a view follows: the first match by R21.
fn follow_target(ctx: &Ctx, t: Target) -> Option<InstId> {
    let ids = ctx
        .w
        .instances()
        .iter()
        .filter(|i| World::visible_instance(i, false) && t.matches(ctx.def, i))
        .map(|i| i.id);
    ctx.w.first_among(ids, &ctx.rules.tick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::KindId;

    /// A camera at `pos` on both axes, an 800x608 view in a 2400x608 room, with `border` and
    /// `speed` on both axes.
    fn camera(pos: f64, border: f64, speed: f64) -> Camera {
        Camera {
            target: Target::Kind(KindId(0)),
            dead_zone: (Real::new(border), Real::new(border)),
            clamp: (Real::new(2400.0 - 800.0), Real::ZERO),
            smoothing: (Real::new(speed), Real::new(speed)),
            size: (Real::new(800.0), Real::new(608.0)),
            pos: (Real::new(pos), Real::new(pos)),
        }
    }

    fn step_x(c: &Camera, at: f64) -> f64 {
        c.step((Real::new(at), Real::new(0.0)), &NumRules::default())
            .0
            .get()
    }

    #[test]
    fn a_target_inside_the_dead_zone_leaves_the_view_alone() {
        // View 0..800 with border 100: the dead zone is 100..700 (inclusive).
        let c = camera(0.0, 100.0, -1.0);
        for at in [100.0, 400.0, 700.0] {
            assert_eq!(step_x(&c, at), 0.0, "target {at} is inside the dead zone");
        }
    }

    #[test]
    fn a_target_past_the_dead_zone_moves_the_view_to_its_edge() {
        let c = camera(0.0, 100.0, -1.0);
        assert_eq!(step_x(&c, 750.0), 50.0, "750 + 100 - 800");
        assert_eq!(step_x(&c, 50.0), 0.0, "-50 clamps to the room edge");
    }

    #[test]
    fn the_view_stays_inside_the_room() {
        let c = camera(500.0, 100.0, -1.0);
        assert_eq!(step_x(&c, 2399.0), 1600.0, "the far edge is the clamp");
        assert_eq!(step_x(&c, 0.0), 0.0, "the near edge is 0");
    }

    #[test]
    fn a_view_larger_than_the_room_sits_at_zero() {
        let mut c = camera(0.0, 100.0, -1.0);
        c.clamp = (Real::new(-300.0), Real::new(-100.0));
        assert_eq!(step_x(&c, 0.0), 0.0);
        assert_eq!(step_x(&c, 1000.0), 0.0);
    }

    #[test]
    fn smoothing_caps_the_move_freezes_and_snaps() {
        // From 0 the desired edge is 50; at most 10 per tick.
        assert_eq!(step_x(&camera(0.0, 100.0, 10.0), 750.0), 10.0);
        // 0 freezes the axis; a negative speed snaps all the way.
        assert_eq!(step_x(&camera(0.0, 100.0, 0.0), 750.0), 0.0);
        assert_eq!(step_x(&camera(0.0, 100.0, -1.0), 750.0), 50.0);
    }

    #[test]
    fn the_border_is_capped_at_half_the_view() {
        // A border of 1000 on an 800 view acts as 400: the dead zone is the centre point 400.
        let c = camera(0.0, 1000.0, -1.0);
        assert_eq!(step_x(&c, 400.0), 0.0);
        assert_eq!(step_x(&c, 500.0), 100.0, "500 + 400 - 800");
    }

    #[test]
    fn a_camera_needs_an_enabled_visible_view_with_a_follow_target() {
        let room = (Real::new(2400.0), Real::new(608.0));
        let mut v = View {
            enabled: true,
            visible: true,
            follow: Some(Target::Kind(KindId(0))),
            ..View::default()
        };
        assert!(Camera::of(&v, room).is_some());
        v.visible = false;
        assert!(Camera::of(&v, room).is_none());
        v.visible = true;
        v.enabled = false;
        assert!(Camera::of(&v, room).is_none());
        v.enabled = true;
        v.follow = None;
        assert!(Camera::of(&v, room).is_none());
    }
}
