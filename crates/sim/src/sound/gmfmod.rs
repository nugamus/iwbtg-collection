//! GMFMODSimple DLL adapter (go-the-spook-house, keep-celebrating-for-xia; docs/plans/sim-core.md
//! 7, S5).
//!
//! Sound ids and instance ids are numbers; 0 means error. Groups are 1 to 4, 0 is master. A
//! one-shot instance becomes invalid when it ends; a looped one stays until stopped. A new
//! instance of a **streamed** sound stops every other instance of that sound first (the library
//! note). Group volume and pitch are linear factors, default 1. **Getters return exactly the
//! last value set** (S5, UNKNOWN: the real DLL may round to a float). The fades are game-side
//! GML (fader objects), not here. `FMODUpdate` is a no-op.
//!
//! In [`DllMode::Stub`] every call changes nothing and returns 0, like `--stub-dlls`.
//!
//! The DLL does not know a sound length, so the script host passes `length_ticks` at
//! `sound_add` (0 = unknown: a one-shot plays until stopped).

use serde::{Deserialize, Serialize};

use super::{DllMode, SoundOutput, SoundTarget, active, emit};
use crate::num::{Real, round::to_int_round};
use crate::world::Ctx;

/// One loaded sound.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FmSound {
    /// File as passed to `FMODSoundAdd`.
    pub file: String,
    /// Streamed.
    pub stream: bool,
    /// 3D flag as passed.
    pub d3: bool,
    /// Length in ticks (0 = unknown).
    pub length_ticks: u32,
    /// Group the sound plays in (0 = master only).
    pub group: u8,
}

/// One playing instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FmInstance {
    /// Instance id.
    pub id: u32,
    /// Sound id.
    pub sound: u32,
    /// Looped.
    pub looping: bool,
    /// Tick of the call, moved forward by paused time.
    pub start_tick: u64,
    /// Tick a pause began, while paused.
    pub paused_at: Option<u64>,
    /// Last instance volume set (1 on creation).
    pub volume: Real,
}

/// GMFMODSimple state. Hashed.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GmfmodState {
    /// `FMODinit` ran.
    pub inited: bool,
    /// Instance limit given to `FMODinit`.
    pub max_instances: u32,
    /// Loaded sounds; sound id n is `sounds[n - 1]`.
    pub sounds: Vec<FmSound>,
    /// Live instances, oldest first.
    pub instances: Vec<FmInstance>,
    /// The next instance id.
    pub next_instance: u32,
    /// Group volumes, index 0 master.
    pub group_volume: [Real; 5],
    /// Group pitches, index 0 master.
    pub group_pitch: [Real; 5],
}

impl Default for GmfmodState {
    fn default() -> GmfmodState {
        GmfmodState {
            inited: false,
            max_instances: 0,
            sounds: Vec::new(),
            instances: Vec::new(),
            next_instance: 1,
            group_volume: [Real::ONE; 5],
            group_pitch: [Real::ONE; 5],
        }
    }
}

fn stub(ctx: &Ctx) -> bool {
    ctx.dll_mode == DllMode::Stub
}

fn id(v: Real) -> Option<u32> {
    to_int_round(v).ok().and_then(|n| u32::try_from(n).ok())
}

fn group(v: Real) -> Option<usize> {
    id(v).and_then(|g| usize::try_from(g).ok()).filter(|g| *g <= 4)
}

fn flag(ok: bool) -> Real {
    if ok { Real::ONE } else { Real::ZERO }
}

fn sound_of(st: &GmfmodState, sound: Real) -> Option<(u32, &FmSound)> {
    let n = id(sound)?;
    let i = usize::try_from(n.checked_sub(1)?).ok()?;
    Some((n, st.sounds.get(i)?))
}

fn instance_of(st: &mut GmfmodState, inst: Real) -> Option<&mut FmInstance> {
    let n = id(inst)?;
    st.instances.iter_mut().find(|i| i.id == n)
}

/// `FMODinit(max instances, web flag)`: 1.
pub fn init(ctx: &mut Ctx, max_instances: Real, _web: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let st = &mut ctx.w.sound.gmfmod;
    st.inited = true;
    st.max_instances = id(max_instances).unwrap_or(0);
    Real::ONE
}

/// `FMODSoundAdd(file, 3d, stream)`: the new sound id, or 0 before init. `length_ticks` is the
/// sim length (0 = unknown).
pub fn sound_add(ctx: &mut Ctx, file: &str, d3: bool, stream: bool, length_ticks: u32) -> Real {
    if stub(ctx) || !ctx.w.sound.gmfmod.inited {
        return Real::ZERO;
    }
    let st = &mut ctx.w.sound.gmfmod;
    let Ok(n) = u32::try_from(st.sounds.len().saturating_add(1)) else {
        return Real::ZERO;
    };
    st.sounds.push(FmSound {
        file: file.to_owned(),
        stream,
        d3,
        length_ticks,
        group: 0,
    });
    emit(
        ctx,
        SoundOutput::Load {
            target: SoundTarget::FmodSound(n),
            file: file.to_owned(),
            stream,
        },
    );
    Real::new(f64::from(n))
}

/// `FMODSoundSetGroup(sound, group)`: 1, or 0 for an unknown sound or group.
pub fn sound_set_group(ctx: &mut Ctx, sound: Real, grp: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let (Some(g), Some(n)) = (group(grp), id(sound)) else {
        return Real::ZERO;
    };
    let st = &mut ctx.w.sound.gmfmod;
    let slot = usize::try_from(n.saturating_sub(1))
        .ok()
        .and_then(|i| st.sounds.get_mut(i));
    match (n, slot) {
        (1.., Some(s)) => {
            s.group = u8::try_from(g).unwrap_or(0);
            Real::ONE
        }
        _ => Real::ZERO,
    }
}

fn start(ctx: &mut Ctx, sound: Real, paused: bool, looping: bool) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let now = ctx.w.tick;
    let st = &mut ctx.w.sound.gmfmod;
    let Some((sid, snd)) = sound_of(st, sound) else {
        return Real::ZERO;
    };
    let streamed = snd.stream;
    let mut stopped = Vec::new();
    if streamed {
        st.instances.retain(|i| {
            let same = i.sound == sid;
            if same {
                stopped.push(i.id);
            }
            !same
        });
    }
    let limit = usize::try_from(st.max_instances).unwrap_or(usize::MAX);
    if st.instances.len() >= limit {
        return Real::ZERO;
    }
    let iid = st.next_instance;
    st.next_instance = st.next_instance.saturating_add(1);
    st.instances.push(FmInstance {
        id: iid,
        sound: sid,
        looping,
        start_tick: now,
        paused_at: paused.then_some(now),
        volume: Real::ONE,
    });
    for old in stopped {
        emit(
            ctx,
            SoundOutput::Stop {
                target: SoundTarget::FmodInstance {
                    instance: old,
                    sound: sid,
                },
            },
        );
    }
    let target = SoundTarget::FmodInstance {
        instance: iid,
        sound: sid,
    };
    emit(ctx, SoundOutput::Play { target, looping });
    if paused {
        emit(ctx, SoundOutput::Pause { target, paused: true });
    }
    Real::new(f64::from(iid))
}

/// `FMODSoundPlay(sound, start paused)`: the instance id, or 0 for an error (unknown sound,
/// sound id 0, instance limit reached).
pub fn sound_play(ctx: &mut Ctx, sound: Real, start_paused: bool) -> Real {
    start(ctx, sound, start_paused, false)
}

/// `FMODSoundLoop(sound, start paused)`.
pub fn sound_loop(ctx: &mut Ctx, sound: Real, start_paused: bool) -> Real {
    start(ctx, sound, start_paused, true)
}

/// `FMODInstanceStop(instance)`: 1 if it was live.
pub fn instance_stop(ctx: &mut Ctx, inst: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let Some(n) = id(inst) else {
        return Real::ZERO;
    };
    let st = &mut ctx.w.sound.gmfmod;
    let Some(pos) = st.instances.iter().position(|i| i.id == n) else {
        return Real::ZERO;
    };
    let sound = st.instances.remove(pos).sound;
    emit(
        ctx,
        SoundOutput::Stop {
            target: SoundTarget::FmodInstance { instance: n, sound },
        },
    );
    Real::ONE
}

/// `FMODInstanceSetPaused(instance, paused)`.
pub fn instance_set_paused(ctx: &mut Ctx, inst: Real, paused: bool) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let now = ctx.w.tick;
    let Some(i) = instance_of(&mut ctx.w.sound.gmfmod, inst) else {
        return Real::ZERO;
    };
    let target = SoundTarget::FmodInstance {
        instance: i.id,
        sound: i.sound,
    };
    match (paused, i.paused_at) {
        (true, None) => i.paused_at = Some(now),
        (false, Some(at)) => {
            i.start_tick = i.start_tick.saturating_add(now.saturating_sub(at));
            i.paused_at = None;
        }
        _ => return Real::ONE,
    }
    emit(ctx, SoundOutput::Pause { target, paused });
    Real::ONE
}

/// `FMODInstanceSetVolume(instance, volume)`: stores the value exactly.
pub fn instance_set_volume(ctx: &mut Ctx, inst: Real, value: Real) -> Real {
    if stub(ctx) || !value.is_finite() {
        return Real::ZERO;
    }
    let Some(i) = instance_of(&mut ctx.w.sound.gmfmod, inst) else {
        return Real::ZERO;
    };
    i.volume = value;
    let target = SoundTarget::FmodInstance {
        instance: i.id,
        sound: i.sound,
    };
    emit(ctx, SoundOutput::Volume { target, value });
    Real::ONE
}

/// `FMODInstanceGetVolume(instance)`: the last value set, 0 for a dead instance.
pub fn instance_get_volume(ctx: &mut Ctx, inst: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    instance_of(&mut ctx.w.sound.gmfmod, inst).map_or(Real::ZERO, |i| i.volume)
}

/// `FMODInstanceIsPlaying`-style check for the sim and tests: live and not paused.
pub fn instance_is_playing(ctx: &mut Ctx, inst: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    flag(instance_of(&mut ctx.w.sound.gmfmod, inst).is_some_and(|i| i.paused_at.is_none()))
}

/// `FMODGroupStop(group)`: stops the instances of sounds in that group (0 stops all).
pub fn group_stop(ctx: &mut Ctx, grp: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let Some(g) = group(grp) else {
        return Real::ZERO;
    };
    let st = &mut ctx.w.sound.gmfmod;
    let sounds = st.sounds.clone();
    st.instances.retain(|i| {
        let sg = usize::try_from(i.sound.saturating_sub(1))
            .ok()
            .and_then(|k| sounds.get(k))
            .map_or(0, |s| usize::from(s.group));
        !(g == 0 || sg == g)
    });
    emit(
        ctx,
        SoundOutput::Stop {
            target: SoundTarget::FmodGroup(u8::try_from(g).unwrap_or(0)),
        },
    );
    Real::ONE
}

/// `FMODGroupSetVolume(group, volume)`: stores the value exactly.
pub fn group_set_volume(ctx: &mut Ctx, grp: Real, value: Real) -> Real {
    if stub(ctx) || !value.is_finite() {
        return Real::ZERO;
    }
    let Some(g) = group(grp) else {
        return Real::ZERO;
    };
    ctx.w.sound.gmfmod.group_volume[g] = value;
    emit(
        ctx,
        SoundOutput::Volume {
            target: SoundTarget::FmodGroup(u8::try_from(g).unwrap_or(0)),
            value,
        },
    );
    Real::ONE
}

/// `FMODGroupGetVolume(group)`: the last value set (1 at start), 0 for a bad group.
pub fn group_get_volume(ctx: &Ctx, grp: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    group(grp).map_or(Real::ZERO, |g| ctx.w.sound.gmfmod.group_volume[g])
}

/// `FMODGroupSetPitch(group, pitch)`: stores the value exactly.
pub fn group_set_pitch(ctx: &mut Ctx, grp: Real, value: Real) -> Real {
    if stub(ctx) || !value.is_finite() {
        return Real::ZERO;
    }
    let Some(g) = group(grp) else {
        return Real::ZERO;
    };
    ctx.w.sound.gmfmod.group_pitch[g] = value;
    emit(
        ctx,
        SoundOutput::Pitch {
            target: SoundTarget::FmodGroup(u8::try_from(g).unwrap_or(0)),
            value,
        },
    );
    Real::ONE
}

/// `FMODGroupGetPitch(group)`: the last value set (1 at start), 0 for a bad group.
pub fn group_get_pitch(ctx: &Ctx, grp: Real) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    group(grp).map_or(Real::ZERO, |g| ctx.w.sound.gmfmod.group_pitch[g])
}

/// `FMODAllStop`: stops and frees every instance.
pub fn all_stop(ctx: &mut Ctx) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    ctx.w.sound.gmfmod.instances.clear();
    emit(ctx, SoundOutput::StopAll { adapter: true });
    Real::ONE
}

/// `FMODUpdate`: no-op (the sim ends instances itself at tick row 16).
pub fn update(_ctx: &mut Ctx) -> Real {
    Real::ZERO
}

/// `FMODfree`: shuts the system down; sounds, instances and group settings reset.
pub fn free(ctx: &mut Ctx) -> Real {
    if stub(ctx) {
        return Real::ZERO;
    }
    let next = ctx.w.sound.gmfmod.next_instance;
    ctx.w.sound.gmfmod = GmfmodState {
        next_instance: next,
        ..GmfmodState::default()
    };
    emit(ctx, SoundOutput::StopAll { adapter: true });
    Real::ONE
}

/// Tick row 16: a one-shot instance with a known length that has played out becomes invalid.
pub(crate) fn expire(st: &mut GmfmodState, now: u64) {
    let sounds = &st.sounds;
    st.instances.retain(|i| {
        let len = usize::try_from(i.sound.saturating_sub(1))
            .ok()
            .and_then(|k| sounds.get(k))
            .map_or(0, |s| s.length_ticks);
        i.looping || i.paused_at.is_some() || len == 0 || active(now, i.start_tick, len, false)
    });
}
