//! The sound model (docs/plans/sim-core.md 7, register S1-S5; docs/format/builtins/sound.md).
//!
//! Audio output is the client's job; whether a sound **is playing** is sim state, because
//! `sound_isplaying` changes game state in 15 games. This module answers from a deterministic
//! model driven by ticks, never from a device.
//!
//! **Built-in sounds.** `sound_play` / `sound_loop` start a copy at the call's tick. A copy
//! plays for the ticks `start .. start + length_ticks` (a looping copy until it is stopped).
//! [`tick`] (tick row 16) only drops the copies that have ended; [`is_playing`] does not wait
//! for it, so a custom hook set without `tick` still answers correctly. At most one MULTIMEDIA
//! copy exists (S2, S3). The built-in model always runs, in every [`DllMode`]: the recorder
//! `--stub-dlls` option stubs only the DLLs.
//!
//! **DLL adapters** ([`supersound`], [`gmfmod`], [`cleanmem`]) are called by the script host
//! (WP9) with the arguments a GML `external_call` passes. In [`DllMode::Stub`] every call does
//! nothing and returns 0 or "".
//!
//! Spec: CLONE_SPEC sound (Inferred contract; UNKNOWN S1-S5).

pub mod cleanmem;
pub mod gmfmod;
pub mod supersound;

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, SoundDef, SoundId, SoundKind};
use crate::error::SimError;
use crate::num::Real;
use crate::output::OutputEvent;
use crate::rules::SoundRules;
use crate::rules::sound::{LoopPlaying, MultimediaSecond};
use crate::world::{Ctx, World};

/// A built-in sound copy that is playing. Hashed.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Copy {
    /// The sound resource.
    pub sound: SoundId,
    /// The tick of the play call.
    pub start_tick: u64,
    /// Started by `sound_loop`.
    pub looping: bool,
    /// The sound is MULTIMEDIA.
    pub multimedia: bool,
    /// The sound's length in ticks, copied at the call.
    pub length_ticks: u32,
}

/// Playing copies and DLL adapter state. Hashed.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct SoundState {
    /// Built-in copies, oldest first.
    pub copies: Vec<Copy>,
    /// SuperSound handles (kaenbyou, eat-the-lemon).
    pub supersound: supersound::SuperSoundState,
    /// GMFMODSimple sounds, instances and groups (go-the-spook-house, xia).
    pub gmfmod: gmfmod::GmfmodState,
}

/// What a sound event is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SoundTarget {
    /// A built-in sound resource.
    Builtin(SoundId),
    /// A SuperSound handle (its number; the DLL handle string is this number).
    Super(u32),
    /// A GMFMODSimple sound id.
    FmodSound(u32),
    /// A GMFMODSimple instance of a sound.
    FmodInstance {
        /// The instance id the game holds.
        instance: u32,
        /// The sound it plays.
        sound: u32,
    },
    /// A GMFMODSimple group (0 is master, 1 to 4).
    FmodGroup(u8),
}

/// Sound events for the client. Not hashed; the client plays, stops and mixes.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum SoundOutput {
    /// A DLL adapter loaded a file (`file` as the game passed it) as `target`.
    Load {
        /// The new sound or handle.
        target: SoundTarget,
        /// The file path the game gave.
        file: String,
        /// Streamed rather than fully loaded.
        stream: bool,
    },
    /// Start a copy or instance at this tick.
    Play {
        /// What starts.
        target: SoundTarget,
        /// Looping.
        looping: bool,
    },
    /// Stop every copy of the target (or the instance, or the group).
    Stop {
        /// What stops.
        target: SoundTarget,
    },
    /// Stop everything the built-in runner plays (`sound_stop_all`), or everything an adapter
    /// plays (`FMODAllStop`) when `adapter` is set.
    StopAll {
        /// True for the DLL adapter all-stop.
        adapter: bool,
    },
    /// Pause or resume a DLL sound.
    Pause {
        /// What.
        target: SoundTarget,
        /// True to pause.
        paused: bool,
    },
    /// Volume changed. Built-in: 0 to 1 (`sound_volume`). SuperSound: hundredths of a dB (-10000
    /// to 0). FMOD: linear factor.
    Volume {
        /// What.
        target: SoundTarget,
        /// The value last set.
        value: Real,
    },
    /// Pitch changed (FMOD groups): linear factor, 1 is normal.
    Pitch {
        /// What.
        target: SoundTarget,
        /// The value last set.
        value: Real,
    },
}

/// How DLL sound calls behave.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum DllMode {
    /// Modeled adapters, for play and replays.
    #[default]
    Model,
    /// Every call returns 0 or "", like the recorder `--stub-dlls`.
    Stub,
}

/// Whether a thing started at `start` still plays at tick `now`. `length` 0 with `looping`
/// false is an ended built-in copy; DLL adapters treat an unknown length as looping.
pub(crate) fn active(now: u64, start: u64, length: u32, looping: bool) -> bool {
    looping || now.saturating_sub(start) < u64::from(length)
}

pub(crate) fn emit(ctx: &mut Ctx, ev: SoundOutput) {
    ctx.emit(OutputEvent::Sound(ev));
}

fn sound_def(def: &GameDef, s: SoundId) -> Result<&SoundDef, SimError> {
    s.index()
        .and_then(|i| def.sounds.get(i))
        .ok_or_else(|| SimError::invariant(format!("sound id {} names no sound", s.0)))
}

/// GML `sound_play` / `sound_loop`: starts a copy at the current tick.
///
/// MULTIMEDIA: if a MULTIMEDIA copy plays, another sound stops it first or is ignored (S2); the
/// same sound is ignored or restarted (S3). Normal sounds overlap freely, copies of one sound
/// included.
pub fn play(ctx: &mut Ctx, s: SoundId, looping: bool) -> Result<(), SimError> {
    let sd = sound_def(ctx.def, s)?;
    let multimedia = sd.kind == SoundKind::Multimedia;
    let length_ticks = sd.length_ticks;
    let now = ctx.w.tick;
    drop_ended(&mut ctx.w.sound, now);
    if multimedia && let Some(pos) = ctx.w.sound.copies.iter().position(|c| c.multimedia) {
        let first = ctx.w.sound.copies[pos].sound;
        if first == s {
            if ctx.rules.sound.loop_playing == LoopPlaying::Ignore {
                return Ok(());
            }
        } else if ctx.rules.sound.multimedia_second == MultimediaSecond::Ignore {
            return Ok(());
        }
        ctx.w.sound.copies.remove(pos);
        emit(
            ctx,
            SoundOutput::Stop {
                target: SoundTarget::Builtin(first),
            },
        );
    }
    ctx.w.sound.copies.push(Copy {
        sound: s,
        start_tick: now,
        looping,
        multimedia,
        length_ticks,
    });
    emit(
        ctx,
        SoundOutput::Play {
            target: SoundTarget::Builtin(s),
            looping,
        },
    );
    Ok(())
}

/// GML `sound_stop`: ends every copy of `s`.
pub fn stop(ctx: &mut Ctx, s: SoundId) {
    let before = ctx.w.sound.copies.len();
    ctx.w.sound.copies.retain(|c| c.sound != s);
    if ctx.w.sound.copies.len() != before {
        emit(
            ctx,
            SoundOutput::Stop {
                target: SoundTarget::Builtin(s),
            },
        );
    }
}

/// GML `sound_stop_all`: ends every built-in copy. DLL sounds are not touched (the kaenbyou
/// stop-all does not affect SuperSound).
pub fn stop_all(ctx: &mut Ctx) {
    ctx.w.sound.copies.clear();
    emit(ctx, SoundOutput::StopAll { adapter: false });
}

/// GML `sound_volume`: output only. It changes no sim state; MULTIMEDIA sounds ignore it. A
/// non-finite value is an error (N15).
pub fn volume(ctx: &mut Ctx, s: SoundId, value: Real) -> Result<(), SimError> {
    if !value.is_finite() {
        return Err(SimError::non_finite("sound_volume"));
    }
    if sound_def(ctx.def, s)?.kind == SoundKind::Normal {
        emit(
            ctx,
            SoundOutput::Volume {
                target: SoundTarget::Builtin(s),
                value,
            },
        );
    }
    Ok(())
}

/// GML `sound_isplaying`. A copy is visible `rules.isplaying_latency` ticks after its play call
/// (S1, default 0: at once, Confirmed (recorder)); a stop is visible at once. An unknown sound
/// id is not playing.
pub fn is_playing(w: &World, def: &GameDef, s: SoundId, rules: &SoundRules) -> bool {
    let _ = def;
    w.sound.copies.iter().any(|c| {
        c.sound == s
            && w.tick >= c.start_tick.saturating_add(u64::from(rules.isplaying_latency))
            && active(w.tick, c.start_tick, c.length_ticks, c.looping)
    })
}

fn drop_ended(st: &mut SoundState, now: u64) {
    st.copies
        .retain(|c| active(now, c.start_tick, c.length_ticks, c.looping));
}

/// Tick row 16: drops the copies, SuperSound handles and FMOD instances whose length has
/// passed by the end of this tick.
pub fn tick(ctx: &mut Ctx) {
    // `w.tick` increments after row 16, so a copy that is over from the next tick on goes now.
    let next = ctx.w.tick.saturating_add(1);
    let st = &mut ctx.w.sound;
    drop_ended(st, next);
    supersound::expire(&mut st.supersound, next);
    gmfmod::expire(&mut st.gmfmod, next);
}
