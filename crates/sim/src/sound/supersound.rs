//! SuperSound DLL adapter (kaenbyou, eat-the-lemon; docs/plans/sim-core.md 7, S5).
//!
//! Handles are strings (the handle of the n-th load is `"n"`; `"0"` is a failed load). Every
//! call returns a string, as the DLL does. Volume is DirectSound attenuation in hundredths of
//! a dB (-10000 silent to 0 full); the GML wrappers add or subtract 10000. **Getters return
//! exactly the last value set** (S5, UNKNOWN for the real DLL; it decides the death fade start
//! value and so how many fader steps run). A Real is returned through `string()` rules.
//!
//! In [`DllMode::Stub`] every call changes nothing and returns `""`, like the recorder
//! `--stub-dlls` (the kaenbyou traces read a start volume of 0).
//!
//! Inferred, OPEN: a paused sound reports playing `"0"` and paused `"1"`; a failed call returns
//! `"0"`; the DLL does not know a sound length, so the script host passes `length_ticks` at
//! load (0 = unknown, plays until stopped).

use serde::{Deserialize, Serialize};

use super::{DllMode, SoundOutput, SoundTarget, active, emit};
use crate::num::{Real, gm_string_real};
use crate::world::Ctx;

/// One loaded SuperSound sound.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SsSound {
    /// File as passed to `SS_LoadSound`.
    pub file: String,
    /// Stream flag.
    pub stream: bool,
    /// Length in ticks; 0 is unknown (plays until stopped).
    pub length_ticks: u32,
    /// Freed by `SS_FreeSound`.
    pub freed: bool,
    /// Started (playing or paused).
    pub started: bool,
    /// Looping.
    pub looping: bool,
    /// Tick the play call ran, moved forward by the time spent paused.
    pub start_tick: u64,
    /// The tick a pause began, while paused.
    pub paused_at: Option<u64>,
    /// Last volume set, DLL units.
    pub volume: Real,
    /// Last pan set (units assumed -10000 to 10000, UNKNOWN).
    pub pan: Real,
}

/// SuperSound state. Hashed.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct SuperSoundState {
    /// `SS_Init` ran.
    pub inited: bool,
    /// Loaded sounds; handle n is `sounds[n - 1]`.
    pub sounds: Vec<SsSound>,
}

fn stub(ctx: &Ctx) -> bool {
    ctx.dll_mode == DllMode::Stub
}

fn ok(b: bool) -> String {
    if b { "1" } else { "0" }.to_owned()
}

fn slot<'a>(st: &'a mut SuperSoundState, h: &str) -> Option<(u32, &'a mut SsSound)> {
    let n: u32 = h.trim().parse().ok()?;
    let i = usize::try_from(n.checked_sub(1)?).ok()?;
    let s = st.sounds.get_mut(i)?;
    if s.freed { None } else { Some((n, s)) }
}

fn playing(s: &SsSound, now: u64) -> bool {
    s.started
        && s.paused_at.is_none()
        && active(
            now,
            s.start_tick,
            s.length_ticks,
            s.looping || s.length_ticks == 0,
        )
}

/// `SS_Init`: returns `"Yes"` (anything but `"No"`).
pub fn init(ctx: &mut Ctx) -> String {
    if stub(ctx) {
        return String::new();
    }
    ctx.w.sound.supersound.inited = true;
    "Yes".to_owned()
}

/// `SS_LoadSound(file, stream)`: the new handle, or `"0"` before `SS_Init`. `length_ticks` is the
/// sound length for the sim (0 = unknown).
pub fn load(ctx: &mut Ctx, file: &str, stream: bool, length_ticks: u32) -> String {
    if stub(ctx) {
        return String::new();
    }
    let st = &mut ctx.w.sound.supersound;
    let Ok(n) = u32::try_from(st.sounds.len().saturating_add(1)) else {
        return "0".to_owned();
    };
    if !st.inited {
        return "0".to_owned();
    }
    st.sounds.push(SsSound {
        file: file.to_owned(),
        stream,
        length_ticks,
        freed: false,
        started: false,
        looping: false,
        start_tick: 0,
        paused_at: None,
        volume: Real::ZERO,
        pan: Real::ZERO,
    });
    emit(
        ctx,
        SoundOutput::Load {
            target: SoundTarget::Super(n),
            file: file.to_owned(),
            stream,
        },
    );
    n.to_string()
}

fn start(ctx: &mut Ctx, h: &str, looping: bool) -> String {
    if stub(ctx) {
        return String::new();
    }
    let now = ctx.w.tick;
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    s.started = true;
    s.looping = looping;
    s.start_tick = now;
    s.paused_at = None;
    emit(
        ctx,
        SoundOutput::Play {
            target: SoundTarget::Super(n),
            looping,
        },
    );
    "1".to_owned()
}

/// `SS_PlaySound`: plays once from the start.
pub fn play(ctx: &mut Ctx, h: &str) -> String {
    start(ctx, h, false)
}

/// `SS_LoopSound`: plays looping from the start.
pub fn loop_sound(ctx: &mut Ctx, h: &str) -> String {
    start(ctx, h, true)
}

/// `SS_StopSound`.
pub fn stop(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    s.started = false;
    s.paused_at = None;
    emit(
        ctx,
        SoundOutput::Stop {
            target: SoundTarget::Super(n),
        },
    );
    "1".to_owned()
}

/// `SS_PauseSound`: only a playing sound pauses.
pub fn pause(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let now = ctx.w.tick;
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    let can = playing(s, now);
    if can {
        s.paused_at = Some(now);
        emit(
            ctx,
            SoundOutput::Pause {
                target: SoundTarget::Super(n),
                paused: true,
            },
        );
    }
    ok(can)
}

/// `SS_ResumeSound`: the paused time does not count toward the length.
pub fn resume(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let now = ctx.w.tick;
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    let Some(at) = s.paused_at.take() else {
        return "0".to_owned();
    };
    s.start_tick = s.start_tick.saturating_add(now.saturating_sub(at));
    emit(
        ctx,
        SoundOutput::Pause {
            target: SoundTarget::Super(n),
            paused: false,
        },
    );
    "1".to_owned()
}

/// `SS_FreeSound`: the handle becomes invalid (`"-1"` from `SS_IsSoundPlaying`).
pub fn free(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    s.freed = true;
    s.started = false;
    s.paused_at = None;
    emit(
        ctx,
        SoundOutput::Stop {
            target: SoundTarget::Super(n),
        },
    );
    "1".to_owned()
}

/// `SS_SetSoundVol`: stores `value` exactly. A non-finite value is refused (`"0"`).
pub fn set_volume(ctx: &mut Ctx, h: &str, value: Real) -> String {
    if stub(ctx) {
        return String::new();
    }
    if !value.is_finite() {
        return "0".to_owned();
    }
    let Some((n, s)) = slot(&mut ctx.w.sound.supersound, h) else {
        return "0".to_owned();
    };
    s.volume = value;
    emit(
        ctx,
        SoundOutput::Volume {
            target: SoundTarget::Super(n),
            value,
        },
    );
    "1".to_owned()
}

/// `SS_GetSoundVol`: exactly the last value set (S5), or `"0"` for a new handle (full volume)
/// or an invalid one.
pub fn get_volume(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let rules = ctx.rules;
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => gm_string_real(s.volume, &rules.num),
        None => "0".to_owned(),
    }
}

/// `SS_SetSoundPan`: stores `value` exactly (no output; the game never calls it).
pub fn set_pan(ctx: &mut Ctx, h: &str, value: Real) -> String {
    if stub(ctx) || !value.is_finite() {
        return if stub(ctx) { String::new() } else { "0".to_owned() };
    }
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => {
            s.pan = value;
            "1".to_owned()
        }
        None => "0".to_owned(),
    }
}

/// `SS_GetSoundPan`: the last pan set.
pub fn get_pan(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let rules = ctx.rules;
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => gm_string_real(s.pan, &rules.num),
        None => "0".to_owned(),
    }
}

/// `SS_IsSoundPlaying`: `"1"` playing, `"0"` stopped, ended or paused, `"-1"` invalid handle.
pub fn is_playing(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let now = ctx.w.tick;
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => ok(playing(s, now)),
        None => "-1".to_owned(),
    }
}

/// `SS_IsSoundLooping`.
pub fn is_looping(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    let now = ctx.w.tick;
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => ok(s.looping && s.started && now >= s.start_tick),
        None => "-1".to_owned(),
    }
}

/// `SS_IsSoundPaused`.
pub fn is_paused(ctx: &mut Ctx, h: &str) -> String {
    if stub(ctx) {
        return String::new();
    }
    match slot(&mut ctx.w.sound.supersound, h) {
        Some((_, s)) => ok(s.paused_at.is_some()),
        None => "-1".to_owned(),
    }
}

/// Tick row 16: a sound with a known length that has played it out stops.
pub(crate) fn expire(st: &mut SuperSoundState, now: u64) {
    for s in &mut st.sounds {
        if s.started && s.paused_at.is_none() && !playing(s, now) {
            s.started = false;
        }
    }
}
