//! Built-in `music` (mechanics `music`, sound Inferred; docs/plans/sim-core.md 6.1, 7;
//! docs/format/builtins/music.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! Plays a track through the sound model (`sound::play`, WP10). `mode`:
//!
//! - `on_create` (KOT playMusic, whose creation code plays the track): Create plays the track.
//! - `every_step_if_not_playing` (SHO playMusic001-005): each Step, while music is on and the
//!   track is not playing, stop all sounds and play the track.
//!
//! `loop` (default true) loops it. With `respects_music_off` nothing plays while music is off
//! (`global.musicD`).
//!
//! OPEN, not done: the music-off flag. The world built-in holds it (`music_muted`, #249);
//! until that merges the flag reads as on. `sound::*` is the WP10 model (#20), so these events
//! cannot run in tests yet; the params and the event set are tested.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error};
use crate::def::{KindDef, ParamValue, Params, SoundId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::sound;
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["on_create", "every_step_if_not_playing"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "track",
        ty: ParamType::Sound,
        required: true,
    },
    ParamSpec {
        name: "loop",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    ParamSpec {
        name: "respects_music_off",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A music instance's settings.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct MusicState {
    /// The track.
    pub track: SoundId,
    /// Loop it.
    pub looping: bool,
    /// Play each Step while it is not playing (else once in Create).
    pub every_step: bool,
    /// Silent while music is off.
    pub respects_off: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step in `every_step_if_not_playing` mode.
pub fn event_set(kind: &KindDef) -> EventSet {
    EventSet {
        step: matches!(kind.params.0.get("mode"), Some(ParamValue::Str(w)) if w == "every_step_if_not_playing"),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let track = match params.0.get("track") {
        Some(ParamValue::Sound(s)) => *s,
        _ => return Err(param_error("track", "a music object needs `track` (a sound)")),
    };
    Ok(BuiltinState::Music(MusicState {
        track,
        looping: param_bool(params, "loop")?.unwrap_or(true),
        every_step: param_enum(params, "mode", MODES)? == Some("every_step_if_not_playing"),
        respects_off: param_bool(params, "respects_music_off")?.unwrap_or(false),
    }))
}

/// Whether music is off (`global.musicD`). OPEN: reads as on until the world holds the flag.
fn music_off(_ctx: &Ctx) -> bool {
    false
}

/// Runs one event on a music object.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Music(s)) = super::state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    if s.respects_off && music_off(ctx) {
        return Ok(Flow::Continue);
    }
    match ev {
        EventKind::Create if !s.every_step => sound::play(ctx, s.track, s.looping)?,
        EventKind::Step if s.every_step && !sound::is_playing(ctx.w, ctx.def, s.track, &ctx.rules.sound) => {
            sound::stop_all(ctx);
            sound::play(ctx, s.track, s.looping)?;
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
