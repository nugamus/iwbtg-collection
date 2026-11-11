//! Built-in `death_sound` (family B `objDeathSound`; kid/B.md "Death (killPlayer)", traces
//! kaenbyou famb-death-timing-*, audio-supersound-deathfade-*; docs/format/builtins/
//! death_sound.md). Tier 2.
//!
//! The kid's death step `death_sound` creates it. It fades the music and plays the death sound
//! one tick after its Create:
//!
//! - Create: without a `music` param the music playing now is the one to fade (`sound::current_music`).
//! - Create: `now_vol` = `start_volume` (10000 = full, the original's SuperSound scale, hundredths
//!   of a dB); an alarm of 1 tick for the sound. Under [`DllMode::Stub`] the start is 0 instead:
//!   the stubbed SuperSound handle check fails and the death code uses 0 (kaenbyou
//!   audio-supersound-deathfade-a: `start_vol` = `now_vol` = 0 throughout), so the fade never moves.
//! - Step, while music is not muted (`MechGlobals::music_muted`): the music volume is set from
//!   `now_vol` (sent before the decrement: the first Step sends full volume), then `now_vol`
//!   drops by 100 while it is at least 2000. Below that it stays (the original would pause a
//!   non-streamed music there; `stream_music` games, kaenbyou, never do: not built).
//! - Alarm 0: plays the death sound unless it is already playing.
//! - `release` (the retry): stops the death sound, restores the music volume and destroys it.
//!
//! The volume is output only: `now_vol / start_volume` goes to `sound::volume` on the `music`
//! sound (a linear stand-in for the original's dB scale, which the client maps).
//!
//! The probe reads `start_vol` and `now_vol` (the original's names) through [`probe_field`].

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_real, state_mut, state_of};
use crate::alarm;
use crate::def::{KindDef, ParamValue, Params, SoundId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::sound::{self, DllMode};
use crate::world::{Ctx, Flow, InstId, KindState};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
    ParamSpec {
        name: "music",
        ty: ParamType::Sound,
        required: false,
    },
    ParamSpec {
        name: "start_volume",
        ty: ParamType::Real,
        required: false,
    },
];

/// The fade floor: `now_vol` stops dropping once it is below this.
const FLOOR: f64 = 2000.0;
/// The drop per Step.
const STEP: f64 = 100.0;

/// A death sound's settings and fade state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DeathSoundState {
    /// The death sound effect (none: only the fade).
    pub sound: Option<SoundId>,
    /// The music to fade: the `music` param, else the music playing at Create (none: only the sound).
    pub music: Option<SoundId>,
    /// The volume the fade starts from (10000).
    pub start: Real,
    /// The current fade volume.
    pub now: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Create, Step and Alarm 0.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        step: true,
        alarms: 1,
        ..EventSet::default()
    }
}

fn sound_param(params: &Params, name: &str) -> Option<SoundId> {
    match params.0.get(name) {
        Some(ParamValue::Sound(s)) => Some(*s),
        _ => None,
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let start = param_real(params, "start_volume")?.unwrap_or(Real::new(10000.0));
    Ok(BuiltinState::DeathSound(DeathSoundState {
        sound: sound_param(params, "sound"),
        music: sound_param(params, "music"),
        start,
        now: start,
    }))
}

/// A probe column of a death sound by the original's variable name: `start_vol` and `now_vol`.
pub fn probe_field(s: &DeathSoundState, name: &str) -> Option<Real> {
    match name {
        "start_vol" => Some(s.start),
        "now_vol" => Some(s.now),
        _ => None,
    }
}

/// The music volume as the client gets it: 0 to 1.
fn fraction(s: &DeathSoundState) -> Real {
    // gm-compare: ok (guards the division below against a zero or negative param; not a GML comparison)
    if s.start.get() > 0.0 {
        num::clamp(Real::new(s.now.get() / s.start.get()), Real::ZERO, Real::ONE)
    } else {
        Real::ZERO
    }
}

/// Runs one event on a death sound.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::DeathSound(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            // No `music` param: fade the music playing now (the original's `curMusic`).
            let current = if s.music.is_none() {
                sound::current_music(ctx.w)
            } else {
                s.music
            };
            let stub = ctx.dll_mode == DllMode::Stub;
            if let Some(BuiltinState::DeathSound(st)) = state_mut(ctx, me) {
                st.music = current;
                // The stubbed DLL answers 0, so the start volume is 0 (deathfade-a).
                if stub {
                    st.start = Real::ZERO;
                    st.now = Real::ZERO;
                }
            }
            let rules = &ctx.rules.num;
            if let Some(inst) = ctx.w.get_mut(me) {
                alarm::set(inst, 0, Real::ONE, rules)?;
            }
        }
        EventKind::Alarm(0) => {
            if let Some(sound) = s.sound
                && !sound::is_playing(ctx.w, ctx.def, sound, &ctx.rules.sound)
            {
                sound::play(ctx, sound, false)?;
            }
        }
        EventKind::Step => {
            if ctx.w.engine.mech.music_muted {
                return Ok(Flow::Continue);
            }
            if let Some(music) = s.music {
                sound::volume(ctx, music, fraction(&s))?;
            }
            let n = &ctx.rules.num;
            if num::gm_ge(s.now, Real::new(FLOOR), n)
                && let Some(BuiltinState::DeathSound(st)) = state_mut(ctx, me)
            {
                st.now -= Real::new(STEP);
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}

/// The retry's step (kid/B.md Restart 2): every death sound stops its sound, restores the music
/// volume and is destroyed.
pub fn release(ctx: &mut Ctx) -> Result<(), SimError> {
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| matches!(&i.state, KindState::Builtin(BuiltinState::DeathSound(_))))
        .map(|i| i.id)
        .collect();
    for id in ids {
        let Some(BuiltinState::DeathSound(s)) = state_of(ctx, id) else {
            continue;
        };
        let s = s.clone();
        if let Some(sd) = s.sound {
            sound::stop(ctx, sd);
        }
        if let Some(music) = s.music {
            sound::volume(ctx, music, Real::ONE)?;
        }
        let _ = ctx.destroy(id);
    }
    Ok(())
}
