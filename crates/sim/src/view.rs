//! Views and runner view follow (docs/plans/sim-core.md 4.5, R19).
//!
//! Views affect gameplay through `view_xview` reads, so they are hashed sim state. WP0
//! declares; WP4b owns.

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::error::SimError;
use crate::num::{self, Real};
use crate::rules::NumRules;
use crate::world::{Ctx, InstId};

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

/// The runner's view follow, once per tick before the Draw slot (Inferred, R19).
///
/// For each enabled, visible view with a followed target, per axis (CLONE_SPEC rooms, "Runner
/// view follow"): take the followed instance's origin (the first of the target by R21, active
/// and alive); the border is `min(border, view size / 2)` (Confirmed (recorder) horizontally:
/// animus's 600 border acts as 400 on an 800 view); if the origin minus the border is left of
/// the view, the target edge is that, else if the origin plus the border is right of the view's
/// far edge, the target is that minus the view size, else the view stays; clamp the target
/// inside the room; then move there, at most `speed` pixels when the speed is not -1 (speed 0
/// freezes the axis: partial evidence only). OPEN: the origin versus the bounding box (the
/// kid's box is symmetric) and the check order against other instances' events (R19).
pub fn follow_update(ctx: &mut Ctx) -> Result<(), SimError> {
    let Some(room) = ctx.def.room(ctx.w.room) else {
        return Ok(());
    };
    let (room_w, room_h) = (
        Real::new(f64::from(room.width)),
        Real::new(f64::from(room.height)),
    );
    for i in 0..ctx.w.views.len() {
        let Some(v) = ctx.w.views.get(i) else { continue };
        let Some(target) = v.follow else { continue };
        if !v.enabled || !v.visible {
            continue;
        }
        let Some(id) = follow_target(ctx, target) else {
            continue;
        };
        let Some(inst) = ctx.w.get(id) else { continue };
        let (px, py) = (inst.m.x(), inst.m.y());
        let nx = follow_axis(px, v.x, v.w, v.hborder, v.hspeed, room_w, &ctx.rules.num);
        let ny = follow_axis(py, v.y, v.h, v.vborder, v.vspeed, room_h, &ctx.rules.num);
        if let Some(v) = ctx.w.views.get_mut(i) {
            v.x = nx;
            v.y = ny;
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
        .filter(|i| i.alive && i.active && t.matches(i))
        .map(|i| i.id);
    ctx.w.first_among(ids, &ctx.rules.tick)
}

/// One axis of the follow: the new view origin.
fn follow_axis(
    pos: Real,
    view: Real,
    size: Real,
    border: Real,
    speed: Real,
    room: Real,
    r: &NumRules,
) -> Real {
    let border = num::gm_min(border, size / Real::new(2.0), r);
    let mut target = view;
    if num::gm_lt(pos - border, view, r) {
        target = pos - border;
    } else if num::gm_gt(pos + border, view + size, r) {
        target = pos + border - size;
    }
    // Inside the room: the far edge first, then 0, so a view larger than the room sits at 0.
    target = num::gm_max(num::gm_min(target, room - size, r), Real::ZERO, r);
    if speed.get() < 0.0 {
        target
    } else if num::gm_gt(target, view, r) {
        num::gm_min(view + speed, target, r)
    } else {
        num::gm_max(view - speed, target, r)
    }
}
