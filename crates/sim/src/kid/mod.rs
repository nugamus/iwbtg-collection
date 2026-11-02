//! The kid built-in (docs/plans/sim-core.md 5).
//!
//! The kid's events run ordered lists of named actions read from `physics.toml`; every spec rule
//! is a field value or a named action in `actions/`. Nothing family-specific hides in an
//! `if family == B`. WP0 declares the module tree, the cross-WP signatures and the action
//! routing ([`KidAction`], [`run_action`], [`KidCollisionRun`], [`run_collision`]); WP5a owns
//! this file. WP5b and WP5c own their files under `actions/`, `collisions/` and `state_*.rs` and
//! add their actions as variants of the enums there, so they never edit this file.

pub mod api;
pub mod death;
pub mod presets;
pub mod state;
pub mod state_b;
pub mod state_ext;

/// Named kid actions, one file per family or topic.
pub mod actions {
    pub mod a1;
    pub mod a2;
    pub mod a3;
    pub mod a4;
    pub mod a5;
    pub mod b;
    pub mod c;
    pub mod common;
    pub mod flip;
    pub mod ice;
    pub mod ladder;
    pub mod slope;
}

/// Named collision runs (`events.collisions` entries).
pub mod collisions {
    pub mod b;
    pub mod block;
    pub mod moving_block;
    pub mod platform;
    pub mod slope;
}

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub use state::{KidGlobals, KidState};

use crate::def::PhysicsProfile;
use crate::def::physics::KeyOn;
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// Kid effects for the client (death, blood). WP5a adds the variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidOutput {}

/// One named action in a `physics.toml` event list (`step = ["walk", "jump", ...]`, plan 5.2).
///
/// Split by owner like `KindState`: each part is the enum of one file under `actions/`, and the
/// name in the file picks the part whose enum has that name (`untagged`). Names must be unique
/// across parts.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(untagged)]
pub enum KidAction {
    /// Every family (WP5a).
    Common(actions::common::CommonAction),
    /// Family A1 (WP5a).
    A1(actions::a1::A1Action),
    /// Family A2 (WP5c).
    A2(actions::a2::A2Action),
    /// Family A3 (WP5c).
    A3(actions::a3::A3Action),
    /// Family A4 (WP5c).
    A4(actions::a4::A4Action),
    /// Family A5 (WP5c).
    A5(actions::a5::A5Action),
    /// Family B (WP5b).
    B(actions::b::BAction),
    /// Family C (WP5c).
    C(actions::c::CAction),
    /// Gravity flips (WP5c).
    Flip(actions::flip::FlipAction),
    /// Slip blocks and ice (WP5c).
    Ice(actions::ice::IceAction),
    /// Ladders (WP5c).
    Ladder(actions::ladder::LadderAction),
    /// Slopes (WP5c).
    Slope(actions::slope::SlopeAction),
}

/// Runs one named action on the kid `me`, in the file that owns it.
pub fn run_action(ctx: &mut Ctx, me: InstId, action: &KidAction) -> Result<Flow, SimError> {
    match action {
        KidAction::Common(a) => actions::common::run(ctx, me, a),
        KidAction::A1(a) => actions::a1::run(ctx, me, a),
        KidAction::A2(a) => actions::a2::run(ctx, me, a),
        KidAction::A3(a) => actions::a3::run(ctx, me, a),
        KidAction::A4(a) => actions::a4::run(ctx, me, a),
        KidAction::A5(a) => actions::a5::run(ctx, me, a),
        KidAction::B(a) => actions::b::run(ctx, me, a),
        KidAction::C(a) => actions::c::run(ctx, me, a),
        KidAction::Flip(a) => actions::flip::run(ctx, me, a),
        KidAction::Ice(a) => actions::ice::run(ctx, me, a),
        KidAction::Ladder(a) => actions::ladder::run(ctx, me, a),
        KidAction::Slope(a) => actions::slope::run(ctx, me, a),
    }
}

/// One named collision run in `events.collisions` (`{ with = "block", run = "block_standard" }`,
/// plan 5.2). Split by owner like [`KidAction`].
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(untagged)]
pub enum KidCollisionRun {
    /// `block_standard` and friends (WP5a).
    Block(collisions::block::BlockRun),
    /// `platform_standard` (WP5a).
    Platform(collisions::platform::PlatformRun),
    /// Family B runs (WP5b).
    B(collisions::b::BRun),
    /// `kill` (WP5a).
    Death(death::DeathRun),
    /// Slope runs (WP5c).
    Slope(collisions::slope::SlopeRun),
    /// poronkusema `moving_block_por` (WP5c).
    MovingBlock(collisions::moving_block::MovingBlockRun),
}

/// Runs one named collision run for the kid `me` against `other`, in the file that owns it.
pub fn run_collision(
    ctx: &mut Ctx,
    me: InstId,
    other: InstId,
    run: &KidCollisionRun,
) -> Result<Flow, SimError> {
    match run {
        KidCollisionRun::Block(r) => collisions::block::run(ctx, me, other, r),
        KidCollisionRun::Platform(r) => collisions::platform::run(ctx, me, other, r),
        KidCollisionRun::B(r) => collisions::b::run(ctx, me, other, r),
        KidCollisionRun::Death(r) => death::run(ctx, me, other, r),
        KidCollisionRun::Slope(r) => collisions::slope::run(ctx, me, other, r),
        KidCollisionRun::MovingBlock(r) => collisions::moving_block::run(ctx, me, other, r),
    }
}

/// Runs `list` on the kid in order; stops at the first flow that is not `Continue`.
fn run_list(ctx: &mut Ctx, me: InstId, list: &[KidAction]) -> Result<Flow, SimError> {
    for a in list {
        let flow = run_action(ctx, me, a)?;
        if flow != Flow::Continue {
            return Ok(flow);
        }
    }
    Ok(Flow::Continue)
}

/// Runs one event on a kid instance: the profile's action list for that event, through
/// [`run_action`]. Create, Room Start, Step and End Step are wired; keys, alarms, collisions and
/// the boundary arrive with WP5a.3 and WP5a.4 and handle nothing until then.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let def = ctx.def;
    let e = &def.physics.events;
    if let EventKind::Collision { with, other } = ev {
        // Every row whose handler class matched, in handler order (C8, R12b).
        for row in e.collisions.iter().filter(|c| c.with == *with) {
            let flow = run_collision(ctx, me, *other, &row.run)?;
            if flow != Flow::Continue {
                return Ok(flow);
            }
        }
        return Ok(Flow::Continue);
    }
    let list = match ev {
        EventKind::Alarm(n) => {
            for row in e.alarms.iter().filter(|a| a.n == *n) {
                let flow = run_list(ctx, me, &row.run)?;
                if flow != Flow::Continue {
                    return Ok(flow);
                }
            }
            return Ok(Flow::Continue);
        }
        EventKind::KeyHeld(a) | EventKind::KeyPress(a) | EventKind::KeyRelease(a) => {
            let on = match ev {
                EventKind::KeyHeld(_) => KeyOn::Held,
                EventKind::KeyPress(_) => KeyOn::Press,
                _ => KeyOn::Release,
            };
            for row in e.keys.iter().filter(|k| k.action == *a && k.on == on) {
                let flow = run_list(ctx, me, &row.run)?;
                if flow != Flow::Continue {
                    return Ok(flow);
                }
            }
            return Ok(Flow::Continue);
        }
        EventKind::Create => &e.create,
        EventKind::RoomStart => &e.room_start,
        EventKind::Step => &e.step,
        EventKind::EndStep => &e.end_step,
        _ => return Ok(Flow::Continue),
    };
    run_list(ctx, me, list)
}

/// What a kid kind with this profile handles.
pub fn event_set(p: &PhysicsProfile) -> EventSet {
    EventSet {
        step: !p.events.step.is_empty(),
        end_step: !p.events.end_step.is_empty(),
        room_start: !p.events.room_start.is_empty(),
        alarms: p.events.alarms.iter().fold(0, |m, a| m | (1u16 << a.n.min(11))),
        key_held: key_bits(p, KeyOn::Held),
        key_press: key_bits(p, KeyOn::Press),
        key_release: key_bits(p, KeyOn::Release),
        collisions: p
            .events
            .collisions
            .iter()
            .map(|c| c.with)
            .fold(SmallVec::new(), |mut v, c| {
                if !v.contains(&c) {
                    v.push(c);
                }
                v
            }),
        ..EventSet::default()
    }
}

/// The bits of the actions with a key row on `on`.
fn key_bits(p: &PhysicsProfile, on: KeyOn) -> u32 {
    p.events
        .keys
        .iter()
        .filter(|k| k.on == on)
        .fold(0, |m, k| m | k.action.bit())
}
