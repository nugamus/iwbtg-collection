//! The sound model (docs/architecture/sim/sound.md, register S1-S5; docs/format/sounds.md).
//!
//! Audio output is the client's job; whether a sound **is playing** is sim state, because
//! `sound_isplaying` changes game state in 15 games. This module answers from a deterministic
//! model driven by ticks, never from a device.
//!
//! **Built-in sounds.** `sound_play` / `sound_loop` start a copy at the call's tick. A copy
//! plays for the ticks `start .. start + length_ticks` (a looping copy, or one with an unknown
//! length 0, until it is stopped).
//! [`tick`] (tick row 16) only drops the copies that have ended; [`is_playing`] does not wait
//! for it, so a custom hook set without `tick` still answers correctly. At most one MULTIMEDIA
//! copy exists (S2, S3). The built-in model always runs, in every [`DllMode`]: the recorder
//! `--stub-dlls` option stubs only the DLLs.
//!
//! **DLL adapters** ([`supersound`], [`gmfmod`], [`cleanmem`]) are called by the script host
//! with the arguments a GML `external_call` passes. Which adapters answer is the run's
//! named [`SoundModel`] (`rules.sound.model`): `event` (standard) models neither, `supersound`
//! and `gmfmod` model one, and `stub` models neither and returns each call's stub value. A
//! recorder `--stub-dlls` run ([`DllMode::Stub`]) forces `stub` ([`Ctx::sound_model`]).
//!
//! **Events.** Every event carries the sim tick of its call; `Play` and `Load` carry the
//! sound's length so the client mixer owns playback without reading the package. Not hashed.
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
use crate::rules::sound::{LoopPlaying, MultimediaSecond, SoundModel};
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
///
/// Every event carries the sim `tick` of the call that produced it, so the client mixer owns
/// latency and output while the sim's tick clock stays the authority. `Play` and `Load` also
/// carry the sound's `length_ticks` (0 = unknown, plays until stopped) so the client does not
/// have to read the package's sound table for a duration. A fade is a schedule of `Volume`
/// events with their ticks: the sim keeps the per-tick schedule for trace/probe exactness, and
/// the client may interpolate between them.
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
        /// The sound's length in ticks (0 = unknown, plays until stopped).
        length_ticks: u32,
        /// The sim tick of the load call.
        tick: u64,
    },
    /// Start a copy or instance at this tick.
    Play {
        /// What starts.
        target: SoundTarget,
        /// Looping.
        looping: bool,
        /// The sound's length in ticks (0 = unknown, plays until stopped).
        length_ticks: u32,
        /// The sim tick of the play call.
        tick: u64,
    },
    /// Stop every copy of the target (or the instance, or the group).
    Stop {
        /// What stops.
        target: SoundTarget,
        /// The sim tick of the stop call.
        tick: u64,
    },
    /// Stop everything the built-in runner plays (`sound_stop_all`), or everything an adapter
    /// plays (`FMODAllStop`) when `adapter` is set.
    StopAll {
        /// True for the DLL adapter all-stop.
        adapter: bool,
        /// The sim tick of the call.
        tick: u64,
    },
    /// Pause or resume a DLL sound.
    Pause {
        /// What.
        target: SoundTarget,
        /// True to pause.
        paused: bool,
        /// The sim tick of the call.
        tick: u64,
    },
    /// Volume changed. Built-in: 0 to 1 (`sound_volume`). SuperSound: hundredths of a dB (-10000
    /// to 0). FMOD: linear factor.
    Volume {
        /// What.
        target: SoundTarget,
        /// The value last set.
        value: Real,
        /// The sim tick of the call.
        tick: u64,
    },
    /// Pitch changed (FMOD groups): linear factor, 1 is normal.
    Pitch {
        /// What.
        target: SoundTarget,
        /// The value last set.
        value: Real,
        /// The sim tick of the call.
        tick: u64,
    },
}

/// How DLL sound calls behave in a build. This is the recorder `--stub-dlls` override, not the
/// package's profile: the run's named adapters come from [`SoundModel`] on [`SoundRules`], and
/// [`Ctx::sound_model`] folds the two together.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum DllMode {
    /// Use the run's named [`SoundModel`] adapters, for play and replays.
    #[default]
    Model,
    /// Every call returns 0 or "", like the recorder `--stub-dlls`.
    Stub,
}

impl Ctx<'_> {
    /// The sound model this run plays under. A recorder `--stub-dlls` run (`DllMode::Stub`)
    /// overrides the profile; otherwise the profile's [`SoundModel`] answers.
    pub fn sound_model(&self) -> SoundModel {
        if self.dll_mode == DllMode::Stub {
            SoundModel::Stub
        } else {
            self.rules.sound.model
        }
    }
}

/// Whether a thing started at `start` still plays at tick `now`: a looping thing, or one whose
/// `length` is 0 ("unknown": the package gave no `length_ticks`, CR9), plays until it is
/// stopped; any other plays the ticks `start .. start + length`. The one rule for built-in
/// copies, SuperSound sounds and FMOD instances (#1173).
pub(crate) fn active(now: u64, start: u64, length: u32, looping: bool) -> bool {
    looping || length == 0 || now.saturating_sub(start) < u64::from(length)
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
                tick: now,
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
            length_ticks,
            tick: now,
        },
    );
    Ok(())
}

/// GML `sound_stop`: ends every copy of `s`.
pub fn stop(ctx: &mut Ctx, s: SoundId) {
    let tick = ctx.w.tick;
    let before = ctx.w.sound.copies.len();
    ctx.w.sound.copies.retain(|c| c.sound != s);
    if ctx.w.sound.copies.len() != before {
        emit(
            ctx,
            SoundOutput::Stop {
                target: SoundTarget::Builtin(s),
                tick,
            },
        );
    }
}

/// GML `sound_stop_all`: ends every built-in copy. DLL sounds are not touched (the kaenbyou
/// stop-all does not affect SuperSound).
pub fn stop_all(ctx: &mut Ctx) {
    let tick = ctx.w.tick;
    ctx.w.sound.copies.clear();
    emit(ctx, SoundOutput::StopAll { adapter: false, tick });
}

/// GML `sound_volume`: output only. It changes no sim state; MULTIMEDIA sounds ignore it. A
/// non-finite value is an error (N15).
pub fn volume(ctx: &mut Ctx, s: SoundId, value: Real) -> Result<(), SimError> {
    if !value.is_finite() {
        return Err(SimError::non_finite("sound_volume"));
    }
    if sound_def(ctx.def, s)?.kind == SoundKind::Normal {
        let tick = ctx.w.tick;
        emit(
            ctx,
            SoundOutput::Volume {
                target: SoundTarget::Builtin(s),
                value,
                tick,
            },
        );
    }
    Ok(())
}

/// The music playing now: the sound of the newest built-in copy that is MULTIMEDIA or looping and
/// has not ended (music is what loops or streams; effects are one-shots). `None` when there is none.
/// A `death_sound` without a `music` param fades this one (kaenbyou `curMusic`, set at run time).
pub fn current_music(w: &World) -> Option<SoundId> {
    w.sound
        .copies
        .iter()
        .rev()
        .find(|c| (c.multimedia || c.looping) && active(w.tick, c.start_tick, c.length_ticks, c.looping))
        .map(|c| c.sound)
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
