//! Built-in `save` (mechanics `save`, kid specs "Saving"; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/save.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! A save point saves when the kid touches it with the right input (`on_touch`), when a bullet
//! hits it (`on_shot`), or both. The save itself is `save::save_now` with the game's position
//! model. Every trigger runs in the Collision (or Key held) event, after the kid's own block
//! correction (Confirmed (recorder) rely `num-byte-save-d.csv`); the games that save in Step or
//! End Step (WAN, LEM, SPK) use the same code here, OPEN until a trace shows a difference.
//!
//! Cooldown: `saveTimer` is set to `cooldown` on a save and counts down every Step; a save needs
//! it below 0. With `on_touch = "skip_held"` (kaenbyou) it counts down only on ticks S is held and
//! only while above 0, and a save needs it at most 0. rely has no working cooldown (it is set but never checked), so `cooldown = 0`
//! never gates. The blink (`blink`, default on; kid/A1.md "Saving"): a save shows frame 1 with
//! `image_speed` 0.017, Animation End goes back to frame 0 and stops.
//!
//! OPEN, not done: the save sound and the particles (RBF) belong to the client and the sound
//! model; `self_destroy_alarm` variants beyond the plain alarm.
//!
//! `alarm_save` (the autosave object of eat-the-lemon and xia; kid/B.md eat-the-lemon item 7,
//! kid/C.md objautosave): Create arms Alarm 0 for that many ticks and Alarm 0 saves from the first
//! kid in instance order, if one exists. The point stays (traces/eat-the-lemon/famb-multikid-restart.md:
//! `autosave.alarm[0]` reads 0 on the save tick and -1 after).

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_mut, state_of,
};
use crate::alarm;
use crate::collide::Target;
use crate::collide::query::place_meeting;
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::num::{self, Real};
use crate::save::{self, SavedPosition};
use crate::world::{Ctx, Flow, InstId};

const TOUCH: &[&str] = &["never", "press", "shoot_press", "skip_held", "always"];
const GRAVITY: &[&str] = &["any", "normal", "flipped"];
pub(crate) const POSITION: &[&str] = &[
    "xy_base100",
    "xy_base256",
    "xy_text2dec",
    "xy_previous",
    "fixed_point",
    "floor_nudge",
];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "on_touch",
        ty: ParamType::Enum(TOUCH),
        required: false,
    },
    ParamSpec {
        name: "on_shot",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "once",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "cooldown",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "min_difficulty_removed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "requires_gravity",
        ty: ParamType::Enum(GRAVITY),
        required: false,
    },
    ParamSpec {
        name: "requires_kid_depth0",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "saved_position",
        ty: ParamType::Enum(POSITION),
        required: false,
    },
    ParamSpec {
        name: "fixed_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "fixed_y",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "self_destroy_alarm",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "alarm_save",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "blink",
        ty: ParamType::Bool,
        required: false,
    },
];

/// What the kid has to do at the save point.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Touch {
    /// Touching does nothing.
    Never,
    /// Touch while S or Z was pressed this tick (rely).
    Press,
    /// Touch while Z was pressed this tick (little-trap).
    ShootPress,
    /// Overlap while S is held (kaenbyou, eat-the-lemon).
    SkipHeld,
    /// Touching is enough (poronkusema, LR object738).
    Always,
}

/// Which kid the point accepts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Gravity {
    /// Either.
    Any,
    /// Only the normal kid (`global.grav` 0).
    Normal,
    /// Only the flipped kid.
    Flipped,
}

/// A save point's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SaveState {
    /// Touch trigger.
    pub on_touch: Touch,
    /// A bullet hit saves.
    pub on_shot: bool,
    /// Destroy after the first save (autosave).
    pub once: bool,
    /// Ticks between saves; 0 never gates.
    pub cooldown: Real,
    /// `saveTimer`.
    pub timer: Real,
    /// Destroyed in Create when the difficulty is above this.
    pub min_difficulty_removed: Option<u8>,
    /// Kid gravity filter.
    pub requires_gravity: Gravity,
    /// MAC: the kid's depth must be 0.
    pub requires_kid_depth0: bool,
    /// Position model (`SavedPosition` by name).
    pub position: String,
    /// The fixed point, for `fixed_point`.
    pub fixed: Option<(Real, Real)>,
    /// Alarm 0 at Create destroys the point after this many ticks.
    pub self_destroy: Option<Real>,
    /// Alarm 0 at Create saves from the first kid after this many ticks (autosave).
    pub alarm_save: Option<Real>,
    /// Show the save blink.
    pub blink: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

fn touch_of(name: Option<&str>) -> Touch {
    match name {
        Some("never") => Touch::Never,
        Some("shoot_press") => Touch::ShootPress,
        Some("skip_held") => Touch::SkipHeld,
        Some("always") => Touch::Always,
        _ => Touch::Press,
    }
}

fn kind_str<'a>(kind: &'a KindDef, name: &str) -> Option<&'a str> {
    match kind.params.0.get(name) {
        Some(ParamValue::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// What a kind with this base handles. Declared from the kind's params (handlers are per kind).
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        alarms: 1,
        animation_end: !matches!(kind.params.0.get("blink"), Some(ParamValue::Bool(false))),
        ..EventSet::default()
    };
    if matches!(kind.params.0.get("on_shot"), Some(ParamValue::Bool(true))) {
        set.collisions.push(ClassMask::KID_BULLET);
    }
    match touch_of(kind_str(kind, "on_touch")) {
        Touch::Never => {}
        Touch::SkipHeld => set.key_held |= Action::Skip.bit(),
        _ => set.collisions.push(KIDS),
    }
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let position = param_enum(params, "saved_position", POSITION)?.unwrap_or("xy_base100");
    let fx = param_real(params, "fixed_x")?;
    let fy = param_real(params, "fixed_y")?;
    let fixed = match (fx, fy) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return Err(super::param_error("fixed_x", "give fixed_x and fixed_y together")),
    };
    if position == "fixed_point" && fixed.is_none() {
        return Err(super::param_error(
            "fixed_x",
            "fixed_point needs fixed_x and fixed_y",
        ));
    }
    let min_difficulty_removed = match param_real(params, "min_difficulty_removed")? {
        Some(v) => Some(super::warp::whole(v, "min_difficulty_removed").and_then(|n| {
            u8::try_from(n).map_err(|_| super::param_error("min_difficulty_removed", "must be 0 to 255"))
        })?),
        None => None,
    };
    let self_destroy = param_real(params, "self_destroy_alarm")?;
    let alarm_save = param_real(params, "alarm_save")?;
    if self_destroy.is_some() && alarm_save.is_some() {
        return Err(super::param_error(
            "alarm_save",
            "alarm_save and self_destroy_alarm both use Alarm 0; give one",
        ));
    }
    let requires_gravity = match param_enum(params, "requires_gravity", GRAVITY)? {
        Some("normal") => Gravity::Normal,
        Some("flipped") => Gravity::Flipped,
        _ => Gravity::Any,
    };
    Ok(BuiltinState::Save(SaveState {
        on_touch: touch_of(param_enum(params, "on_touch", TOUCH)?),
        on_shot: param_bool(params, "on_shot")?.unwrap_or(false),
        once: param_bool(params, "once")?.unwrap_or(false),
        cooldown: param_real(params, "cooldown")?.unwrap_or(Real::ZERO),
        timer: Real::new(-1.0),
        min_difficulty_removed,
        requires_gravity,
        requires_kid_depth0: param_bool(params, "requires_kid_depth0")?.unwrap_or(false),
        position: position.to_owned(),
        fixed,
        self_destroy,
        alarm_save,
        blink: param_bool(params, "blink")?.unwrap_or(true),
    }))
}

pub(crate) fn model_of(name: &str) -> SavedPosition {
    match name {
        "xy_base256" => SavedPosition::XyBase256Bytes,
        "xy_text2dec" => SavedPosition::XyText2Dec,
        "xy_previous" => SavedPosition::XyPrevious,
        "fixed_point" => SavedPosition::FixedPoint,
        "floor_nudge" => SavedPosition::FloorNudge,
        _ => SavedPosition::XyBase100Bytes,
    }
}

/// Runs one event on a save point.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Save(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            // The blink art rests on frame 0: Create sets `image_speed` 0 before anything else
            // (kid/B.md Saving; traces/kaenbyou/famb-save-restart-b.csv, image_speed 0.0 on frame 2).
            if s.blink
                && let Some(inst) = ctx.w.get_mut(me)
            {
                inst.img.set_speed(Real::ZERO)?;
            }
            if let Some(max) = s.min_difficulty_removed
                && ctx.w.engine.run.difficulty > max
            {
                return Ok(ctx.destroy(me));
            }
            if let Some(n) = s.self_destroy.or(s.alarm_save) {
                let rules = &ctx.rules.num;
                if let Some(inst) = ctx.w.get_mut(me) {
                    alarm::set(inst, 0, n, rules)?;
                }
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) if s.self_destroy.is_some() => Ok(ctx.destroy(me)),
        // The autosave: the first kid in instance order, the lowest id (famb-multikid-restart-b.csv
        // f25: the file holds kid 104180's marked x). No kid, no save (kid/C.md objautosave).
        EventKind::Alarm(0) if s.alarm_save.is_some() => {
            let rules = ctx.rules;
            match ctx.w.kid(&rules.tick) {
                Some(kid) => try_save(ctx, me, &s, kid),
                None => Ok(Flow::Continue),
            }
        }
        // `skip_held` (kaenbyou "Key S", kid/B.md Saving): the timer counts only in the key event.
        EventKind::Step if s.on_touch != Touch::SkipHeld => {
            if let Some(BuiltinState::Save(st)) = state_mut(ctx, me) {
                st.timer -= Real::ONE;
            }
            Ok(Flow::Continue)
        }
        EventKind::AnimationEnd if s.blink => {
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.img.set_index(Real::ZERO)?;
                inst.img.set_speed(Real::ZERO)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Collision { with, other } => {
            let by_kid = with.intersects(KIDS);
            let by_bullet = with.intersects(ClassMask::KID_BULLET);
            if by_kid && touch_fires(ctx, &s) {
                try_save(ctx, me, &s, *other)
            } else if by_bullet && s.on_shot {
                let rules = ctx.rules;
                match ctx.w.kid(&rules.tick) {
                    Some(kid) => try_save(ctx, me, &s, kid),
                    None => Ok(Flow::Continue),
                }
            } else {
                Ok(Flow::Continue)
            }
        }
        EventKind::KeyHeld(Action::Skip) if s.on_touch == Touch::SkipHeld => {
            // Every tick S is held: subtract 1 while above 0, then save when it is at most 0 and
            // the mask touches the kid (the gate in `try_save` reads the updated timer).
            let nr = &ctx.rules.num;
            let mut s = s;
            if num::gm_gt(s.timer, Real::ZERO, nr) {
                s.timer -= Real::ONE;
                if let Some(BuiltinState::Save(st)) = state_mut(ctx, me) {
                    st.timer = s.timer;
                }
            }
            let hit = place_meeting(ctx, me, own_x(ctx, me)?, own_y(ctx, me)?, Target::Class(KIDS))?;
            if !hit {
                return Ok(Flow::Continue);
            }
            let rules = ctx.rules;
            match ctx.w.kid(&rules.tick) {
                Some(kid) => try_save(ctx, me, &s, kid),
                None => Ok(Flow::Continue),
            }
        }
        _ => Ok(Flow::Continue),
    }
}

fn own_x(ctx: &Ctx, me: InstId) -> Result<Real, SimError> {
    ctx.w
        .get(me)
        .map(|i| i.m.x())
        .ok_or_else(|| SimError::invariant("save point vanished"))
}

fn own_y(ctx: &Ctx, me: InstId) -> Result<Real, SimError> {
    ctx.w
        .get(me)
        .map(|i| i.m.y())
        .ok_or_else(|| SimError::invariant("save point vanished"))
}

/// The touch trigger's input condition this tick.
fn touch_fires(ctx: &Ctx, s: &SaveState) -> bool {
    let input = &ctx.w.input;
    match s.on_touch {
        Touch::Never | Touch::SkipHeld => false,
        Touch::Press => input.pressed(Action::Shoot) || input.pressed(Action::Skip),
        Touch::ShootPress => input.pressed(Action::Shoot),
        Touch::Always => true,
    }
}

/// Saves from `kid` when the cooldown, gravity and depth rules allow it.
fn try_save(ctx: &mut Ctx, me: InstId, s: &SaveState, kid: InstId) -> Result<Flow, SimError> {
    let nr = &ctx.rules.num;
    let gated = if s.on_touch == Touch::SkipHeld {
        num::gm_gt(s.timer, Real::ZERO, nr)
    } else {
        num::gm_gt(s.cooldown, Real::ZERO, nr) && num::gm_ge(s.timer, Real::ZERO, nr)
    };
    if gated {
        return Ok(Flow::Continue);
    }
    let Some(k) = ctx.w.get(kid) else {
        return Ok(Flow::Continue);
    };
    let flipped = ctx.w.engine.kid.grav || k.classes.intersects(ClassMask::KID_FLIPPED);
    let gravity_ok = match s.requires_gravity {
        Gravity::Any => true,
        Gravity::Normal => !flipped,
        Gravity::Flipped => flipped,
    };
    if !gravity_ok || (s.requires_kid_depth0 && num::gm_ne(k.depth, Real::ZERO, nr)) {
        return Ok(Flow::Continue);
    }
    save::save_now(ctx, kid, model_of(&s.position), s.fixed)?;
    if let Some(BuiltinState::Save(st)) = state_mut(ctx, me) {
        st.timer = st.cooldown;
    }
    if s.blink
        && let Some(inst) = ctx.w.get_mut(me)
    {
        inst.img.set_index(Real::ONE)?;
        inst.img.set_speed(Real::new(0.017))?;
    }
    if s.once {
        return Ok(ctx.destroy(me));
    }
    Ok(Flow::Continue)
}
