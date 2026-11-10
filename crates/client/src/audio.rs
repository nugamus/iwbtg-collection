//! Sound and music: plays the sim's `SoundOutput` events, and the script cues `emit` and `shake`.
//! The sim decides what plays and for how long (its sound model); this only makes the noise.
//! The audio offset delays playback only, never the sim (docs/content.md, Controls and audio).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackMode, PlaybackSettings, Volume,
};
use bevy::prelude::*;
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::def::{GameDef, SoundId};
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::script::ScriptOutput;
use iwbtg_sim::sound::{SoundOutput, SoundTarget};

use crate::view::Game;

/// Longest audio offset: audio can only be held back, so this is how far behind the picture it
/// may run.
pub const MAX_OFFSET_MS: u32 = 500;

/// The audio command-line flags.
#[derive(clap::Args, Debug)]
pub struct AudioArgs {
    /// Hold the sound back by this many milliseconds (0 to 500) to line it up with the picture on
    /// a slow audio device. It moves playback only, never the sim. Saved in the settings file, so
    /// the next start without the flag keeps it. Default: the saved value, else 0.
    #[arg(long, value_name = "MS", value_parser = clap::value_parser!(u32).range(0..=i64::from(MAX_OFFSET_MS)))]
    pub audio_offset: Option<u32>,
}

/// Sound, music and screen shake for one package.
pub struct AudioPlugin {
    offset_ms: u32,
    files: PackageFiles,
}

impl AudioPlugin {
    /// `--audio-offset`, when given, is saved first; the plugin uses the saved offset.
    pub fn new(args: &AudioArgs, files: &PackageFiles) -> AudioPlugin {
        if let Some(ms) = args.audio_offset {
            crate::settings::update(|s| s.audio_offset_ms = ms);
        }
        AudioPlugin {
            offset_ms: crate::settings::get().audio_offset_ms,
            files: files.clone(),
        }
    }
}

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        if self.offset_ms > 0 {
            info!("audio offset {} ms", self.offset_ms);
        }
        app.insert_resource(Queue::new(self.offset_ms))
            .insert_resource(Sounds::new(&self.files))
            .init_resource::<Shake>()
            .add_systems(
                Update,
                (
                    // Shake adds its offset on top of the view centre `follow_view` just set.
                    apply_shake.after(crate::view::follow_view),
                    play,
                    sync_music.after(play),
                ),
            );
    }
}

/// What the client acts on from one tick's output.
#[derive(Clone, PartialEq, Debug)]
pub enum Cue {
    Sound(SoundOutput),
    /// A script `emit(name)`: plays the package sound of that name, if there is one.
    Emit(String),
}

/// The cues in an output list, in order.
pub fn cues(output: &[OutputEvent]) -> Vec<Cue> {
    output
        .iter()
        .filter_map(|e| match e {
            OutputEvent::Sound(s) => Some(Cue::Sound(s.clone())),
            OutputEvent::Script(ScriptOutput::Emit { name }) => Some(Cue::Emit(name.clone())),
            _ => None,
        })
        .collect()
}

/// A thing for the player to do with the speakers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Start {
        target: SoundTarget,
        sound: SoundId,
        looping: bool,
        volume: f32,
        speed: f32,
    },
    Stop(SoundTarget),
    /// Every built-in voice, or every DLL voice.
    StopAll {
        adapter: bool,
    },
    Pause {
        target: SoundTarget,
        paused: bool,
    },
    Volume {
        target: SoundTarget,
        volume: f32,
    },
    Speed {
        target: SoundTarget,
        speed: f32,
    },
}

/// Which sound each DLL handle loaded, and the volume and pitch last set per target. Pure: the
/// Bevy side only carries out the commands.
#[derive(Default)]
pub struct Mixer {
    loaded: Table<SoundId>,
    volume: Table<f32>,
    speed: Table<f32>,
}

/// A few values by target. `SoundTarget` has no `Hash`, and a game holds only a handful.
struct Table<V>(Vec<(SoundTarget, V)>);

impl<V> Default for Table<V> {
    fn default() -> Self {
        Table(Vec::new())
    }
}

impl<V: Copy> Table<V> {
    fn get(&self, t: &SoundTarget) -> Option<V> {
        self.0.iter().find(|(k, _)| k == t).map(|(_, v)| *v)
    }

    fn insert(&mut self, t: SoundTarget, v: V) {
        match self.0.iter_mut().find(|(k, _)| *k == t) {
            Some(slot) => slot.1 = v,
            None => self.0.push((t, v)),
        }
    }
}

/// A sound's key in the loaded table: a SuperSound handle or an FMOD sound; an FMOD instance
/// plays the sound it came from.
fn source_key(target: SoundTarget) -> SoundTarget {
    match target {
        SoundTarget::FmodInstance { sound, .. } => SoundTarget::FmodSound(sound),
        t => t,
    }
}

/// The package sound a DLL file name stands for: its stem names a package sound id, compared
/// without regard to case (the same rule as `GameDef::dll_sound_length`).
fn sound_for_file(def: &GameDef, file: &str) -> Option<SoundId> {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    def.sounds
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(stem))
        .map(|i| SoundId(i as u32))
}

/// SuperSound volumes are hundredths of a dB (-10000 to 0); the others are linear factors.
fn linear(target: SoundTarget, value: f64) -> f32 {
    match target {
        SoundTarget::Super(_) => 10f64.powf(value / 2000.0) as f32,
        _ => value.max(0.0) as f32,
    }
}

impl Mixer {
    pub fn handle(&mut self, def: &GameDef, cue: &Cue) -> Vec<Cmd> {
        match cue {
            Cue::Emit(name) => def
                .sounds
                .iter()
                .position(|s| s.name == *name)
                .map(|i| Cmd::Start {
                    target: SoundTarget::Builtin(SoundId(i as u32)),
                    sound: SoundId(i as u32),
                    looping: false,
                    volume: 1.0,
                    speed: 1.0,
                })
                .into_iter()
                .collect(),
            Cue::Sound(s) => self.sound(def, s),
        }
    }

    fn sound(&mut self, def: &GameDef, s: &SoundOutput) -> Vec<Cmd> {
        match s {
            SoundOutput::Load { target, file, .. } => {
                if let Some(id) = sound_for_file(def, file) {
                    self.loaded.insert(source_key(*target), id);
                }
                vec![]
            }
            SoundOutput::Play { target, looping } => {
                let sound = match target {
                    SoundTarget::Builtin(id) => Some(*id),
                    SoundTarget::FmodGroup(_) => None,
                    t => self.loaded.get(&source_key(*t)),
                };
                let Some(sound) = sound else { return vec![] };
                vec![Cmd::Start {
                    target: *target,
                    sound,
                    looping: *looping,
                    volume: self.volume.get(target).unwrap_or_else(|| {
                        // A built-in sound starts at the volume its package gives it.
                        def.sounds
                            .get(sound.0 as usize)
                            .map_or(1.0, |d| d.volume.get() as f32)
                    }),
                    speed: self.speed.get(target).unwrap_or(1.0),
                }]
            }
            SoundOutput::Stop { target } => vec![Cmd::Stop(*target)],
            SoundOutput::StopAll { adapter } => vec![Cmd::StopAll { adapter: *adapter }],
            SoundOutput::Pause { target, paused } => vec![Cmd::Pause {
                target: *target,
                paused: *paused,
            }],
            SoundOutput::Volume { target, value } => {
                let volume = linear(*target, value.get());
                self.volume.insert(*target, volume);
                vec![Cmd::Volume {
                    target: *target,
                    volume,
                }]
            }
            SoundOutput::Pitch { target, value } => {
                let speed = value.get().max(0.01) as f32;
                self.speed.insert(*target, speed);
                vec![Cmd::Speed {
                    target: *target,
                    speed,
                }]
            }
        }
    }
}

/// Every sound file of the package by sound name: `sounds/<id>.ogg`, `music/<id>.ogg`, and the
/// engine pack's `engine/<pack>/...` as `engine:<pack>/<id>`.
pub fn sound_bytes(files: &PackageFiles) -> HashMap<String, Arc<[u8]>> {
    let mut out = HashMap::new();
    for (path, bytes) in files.iter() {
        let name = ["sounds", "music"].into_iter().find_map(|folder| {
            if let Some((pack, id)) = iwbtg_format::engine_pack::asset_file(path, folder, ".ogg") {
                return Some(format!("{}{pack}/{id}", iwbtg_format::ids::ENGINE_PREFIX));
            }
            path.strip_prefix(folder)?
                .strip_prefix('/')?
                .strip_suffix(".ogg")
                .filter(|id| !id.contains('/'))
                .map(String::from)
        });
        if let Some(name) = name {
            out.insert(name, Arc::from(bytes));
        }
    }
    out
}

/// Cues waiting for their time. The offset holds each back by the same amount.
#[derive(Resource)]
pub struct Queue {
    pending: VecDeque<(Instant, Cue)>,
    offset: Duration,
}

impl Queue {
    pub fn new(offset_ms: u32) -> Queue {
        Queue {
            pending: VecDeque::new(),
            offset: Duration::from_millis(u64::from(offset_ms.min(MAX_OFFSET_MS))),
        }
    }

    pub fn push(&mut self, now: Instant, output: &[OutputEvent]) {
        self.pending
            .extend(cues(output).into_iter().map(|c| (now + self.offset, c)));
    }

    /// Drops every waiting cue (a retry starts the run again).
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// The cues whose time has come.
    pub fn due(&mut self, now: Instant) -> Vec<Cue> {
        let mut out = Vec::new();
        while self.pending.front().is_some_and(|(at, _)| *at <= now) {
            out.extend(self.pending.pop_front().map(|(_, c)| c));
        }
        out
    }
}

/// The sound files and the mixer's memory.
#[derive(Resource)]
pub struct Sounds {
    bytes: HashMap<String, Arc<[u8]>>,
    handles: HashMap<SoundId, Handle<AudioSource>>,
    mixer: Mixer,
}

impl Sounds {
    pub fn new(files: &PackageFiles) -> Sounds {
        Sounds {
            bytes: sound_bytes(files),
            handles: HashMap::new(),
            mixer: Mixer::default(),
        }
    }
}

/// Silences the run for a retry: every voice stops, waiting cues and the mixer's memory go, and
/// a running shake ends. The new run's room start then plays from a clean slate.
pub fn reset(world: &mut World) {
    let voices: Vec<Entity> = world
        .query_filtered::<Entity, With<Voice>>()
        .iter(world)
        .collect();
    for voice in voices {
        world.despawn(voice);
    }
    if let Some(mut queue) = world.get_resource_mut::<Queue>() {
        queue.clear();
    }
    if let Some(mut sounds) = world.get_resource_mut::<Sounds>() {
        sounds.mixer = Mixer::default();
    }
    if let Some(mut shake) = world.get_resource_mut::<Shake>() {
        *shake = Shake::default();
    }
}

/// A playing copy, by what it plays.
#[derive(Component)]
pub struct Voice(SoundTarget);

/// Seeks a looping tune back when it drifts more than this many ticks from the tick clock
/// (docs/product.md, Music sync).
pub const MAX_DRIFT_TICKS: f64 = 2.0;

/// Ticks to leave a tune alone after it starts or is seeked: its position takes a moment to settle.
const SETTLE_TICKS: f64 = 10.0;

/// Where the tick clock says one looping tune should be. The sim's tick count is the authority:
/// the tune is expected at the time it has been playing in ticks (pitch speeds it up, a pause
/// holds it), wrapped to its length. Pure, so it is tested without a sound device.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MusicSync {
    /// The tick (with its fraction) the position below belongs to.
    anchor_tick: f64,
    /// The tune position in seconds at the anchor.
    anchor_pos: f64,
    speed: f64,
    paused: bool,
    /// Length of one loop in seconds.
    length: f64,
    /// No checks before this tick.
    settle_until: f64,
}

impl MusicSync {
    /// A tune that starts now, at `tick` (counting the fraction of the tick the clock is into),
    /// `length` seconds long.
    pub fn new(tick: f64, speed: f64, length: f64) -> MusicSync {
        MusicSync {
            anchor_tick: tick,
            anchor_pos: 0.0,
            speed,
            paused: false,
            length,
            settle_until: tick + SETTLE_TICKS,
        }
    }

    /// Where the tune should be at `tick`, in seconds into the loop.
    pub fn expected(&self, tick: f64, tick_rate: f64) -> f64 {
        let played = if self.paused {
            0.0
        } else {
            (tick - self.anchor_tick).max(0.0) / tick_rate * self.speed
        };
        (self.anchor_pos + played).rem_euclid(self.length)
    }

    fn reanchor(&mut self, tick: f64, tick_rate: f64) {
        self.anchor_pos = self.expected(tick, tick_rate);
        self.anchor_tick = tick;
    }

    /// The pitch changed: later time counts at the new speed.
    pub fn set_speed(&mut self, tick: f64, tick_rate: f64, speed: f64) {
        self.reanchor(tick, tick_rate);
        self.speed = speed;
    }

    /// Pausing holds the expected position; resuming continues from it.
    pub fn set_paused(&mut self, tick: f64, tick_rate: f64, paused: bool) {
        if paused != self.paused {
            self.reanchor(tick, tick_rate);
            self.paused = paused;
            self.settle_until = tick + SETTLE_TICKS;
        }
    }

    /// The position to seek to when the tune is at `actual` seconds and has drifted more than
    /// [`MAX_DRIFT_TICKS`] from the tick clock; `None` when it is close enough (or settling,
    /// or paused). The drift is the short way round the loop.
    pub fn correction(&mut self, tick: f64, tick_rate: f64, actual: f64) -> Option<f64> {
        if self.paused || tick < self.settle_until {
            return None;
        }
        let expected = self.expected(tick, tick_rate);
        let mut drift = (actual.rem_euclid(self.length) - expected).abs();
        drift = drift.min(self.length - drift);
        if drift <= MAX_DRIFT_TICKS / tick_rate {
            return None;
        }
        self.settle_until = tick + SETTLE_TICKS;
        Some(expected)
    }
}

fn is_adapter(t: SoundTarget) -> bool {
    !matches!(t, SoundTarget::Builtin(_))
}

/// Starts, stops and mixes what the cues ask for.
pub fn play(
    mut commands: Commands,
    game: NonSend<Game>,
    mut queue: ResMut<Queue>,
    mut sounds: ResMut<Sounds>,
    mut assets: ResMut<Assets<AudioSource>>,
    mut voices: Query<(Entity, &Voice, Option<&mut AudioSink>, Option<&mut MusicSync>)>,
    fixed: Res<Time<Fixed>>,
) {
    let def = game.sim.def();
    let tick_rate = f64::from(def.tick_rate.max(1));
    // The tick count with the part of the next tick the fixed clock is already into.
    let now_tick = game.sim.world().tick as f64 + f64::from(fixed.overstep_fraction());
    for cue in queue.due(Instant::now()) {
        for cmd in sounds.mixer.handle(def, &cue) {
            match cmd {
                Cmd::Start {
                    target,
                    sound,
                    looping,
                    volume,
                    speed,
                } => {
                    let handle = match sounds.handles.get(&sound) {
                        Some(h) => h.clone(),
                        None => {
                            let Some(bytes) = def
                                .sounds
                                .get(sound.0 as usize)
                                .and_then(|d| sounds.bytes.get(&d.name))
                            else {
                                continue;
                            };
                            let h = assets.add(AudioSource { bytes: bytes.clone() });
                            sounds.handles.insert(sound, h.clone());
                            h
                        }
                    };
                    // A looping tune with a known length follows the tick clock.
                    let length = def
                        .sounds
                        .get(sound.0 as usize)
                        .map_or(0.0, |d| f64::from(d.length_ticks) / tick_rate);
                    let mut voice = commands.spawn((
                        AudioPlayer::new(handle),
                        PlaybackSettings {
                            mode: if looping {
                                PlaybackMode::Loop
                            } else {
                                PlaybackMode::Despawn
                            },
                            volume: Volume::Linear(volume),
                            speed,
                            ..default()
                        },
                        Voice(target),
                    ));
                    if looping && length > 0.0 {
                        voice.insert(MusicSync::new(now_tick, f64::from(speed), length));
                    }
                }
                Cmd::Stop(t) => {
                    for (e, v, _, _) in &voices {
                        if v.0 == t
                            || matches!((t, v.0), (SoundTarget::FmodSound(s), SoundTarget::FmodInstance { sound, .. }) if s == sound)
                        {
                            commands.entity(e).despawn();
                        }
                    }
                }
                Cmd::StopAll { adapter } => {
                    for (e, v, _, _) in &voices {
                        if is_adapter(v.0) == adapter {
                            commands.entity(e).despawn();
                        }
                    }
                }
                Cmd::Pause { target, paused } => {
                    for (_, v, sink, sync) in &mut voices {
                        if v.0 != target {
                            continue;
                        }
                        if let Some(mut sync) = sync {
                            sync.set_paused(now_tick, tick_rate, paused);
                        }
                        if let Some(sink) = sink {
                            if paused { sink.pause() } else { sink.play() }
                        }
                    }
                }
                Cmd::Volume { target, volume } => {
                    for (_, v, sink, _) in &mut voices {
                        if let (true, Some(mut sink)) = (v.0 == target, sink) {
                            sink.set_volume(Volume::Linear(volume));
                        }
                    }
                }
                Cmd::Speed { target, speed } => {
                    for (_, v, sink, sync) in &mut voices {
                        if v.0 != target {
                            continue;
                        }
                        if let Some(mut sync) = sync {
                            sync.set_speed(now_tick, tick_rate, f64::from(speed));
                        }
                        if let Some(sink) = sink {
                            sink.set_speed(speed);
                        }
                    }
                }
            }
        }
    }
}

/// Keeps looping tunes on the tick clock: a tune that has drifted more than two ticks (40 ms at
/// 50 Hz) from where the sim tick count says it should be is seeked back to that position;
/// smaller drift is left alone, so normal play has no audible skips.
pub fn sync_music(
    game: NonSend<Game>,
    fixed: Res<Time<Fixed>>,
    mut tunes: Query<(&AudioSink, &mut MusicSync)>,
) {
    let tick_rate = f64::from(game.sim.def().tick_rate.max(1));
    let now_tick = game.sim.world().tick as f64 + f64::from(fixed.overstep_fraction());
    for (sink, mut sync) in &mut tunes {
        if let Some(to) = sync.correction(now_tick, tick_rate, sink.position().as_secs_f64()) {
            // A source that cannot seek stays where it is; the next check tries again.
            let _ = sink.try_seek(Duration::from_secs_f64(to));
        }
    }
}

/// A screen shake still running: ticks left and its strength in pixels.
#[derive(Resource, Default)]
pub struct Shake {
    ticks: u32,
    strength: f32,
    tick: u32,
}

impl Shake {
    /// Takes a tick's `shake` calls and counts the running one down. A stronger new shake wins.
    pub fn tick(&mut self, output: &[OutputEvent]) {
        self.tick = self.tick.wrapping_add(1);
        self.ticks = self.ticks.saturating_sub(1);
        for e in output {
            if let OutputEvent::Script(ScriptOutput::Shake { ticks, strength }) = e {
                let strength = strength.get() as f32;
                if *ticks > 0 && (self.ticks == 0 || strength >= self.strength) {
                    self.ticks = *ticks;
                    self.strength = strength;
                }
            }
        }
    }

    /// The camera's offset this frame, in pixels. A client effect: it has no say in the sim, so
    /// it does not use the sim's random numbers.
    pub fn offset(&self) -> Vec2 {
        if self.ticks == 0 {
            return Vec2::ZERO;
        }
        // A cheap deterministic wobble, alternating sign so it always moves.
        let n = self.tick;
        let (a, b) = (
            n.wrapping_mul(2_654_435_761) >> 16 & 0xff,
            n.wrapping_mul(40_503) >> 8 & 0xff,
        );
        let unit = |v: u32| (v as f32 / 255.0) * 2.0 - 1.0;
        Vec2::new(unit(a), unit(b)) * self.strength
    }
}

/// Moves the camera by the shake.
pub fn apply_shake(shake: Res<Shake>, mut camera: Single<&mut Transform, With<Camera2d>>) {
    let o = shake.offset();
    // Added to the centre `view::follow_view` sets every frame, so it never builds up.
    camera.translation.x += o.x;
    camera.translation.y += o.y;
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::num::Real;

    fn out(s: SoundOutput) -> Vec<OutputEvent> {
        vec![OutputEvent::Sound(s)]
    }

    #[test]
    fn only_sound_and_emit_events_are_cues() {
        let mut o = out(SoundOutput::StopAll { adapter: false });
        o.push(OutputEvent::Script(ScriptOutput::Emit { name: "ding".into() }));
        o.push(OutputEvent::Script(ScriptOutput::Shake {
            ticks: 3,
            strength: Real::new(2.0),
        }));
        assert_eq!(cues(&o).len(), 2);
    }

    #[test]
    fn the_offset_holds_cues_back_and_keeps_their_order() {
        let mut q = Queue::new(100);
        let t0 = Instant::now();
        q.push(t0, &out(SoundOutput::StopAll { adapter: false }));
        q.push(
            t0 + Duration::from_millis(20),
            &out(SoundOutput::StopAll { adapter: true }),
        );
        assert!(q.due(t0 + Duration::from_millis(99)).is_empty());
        assert_eq!(q.due(t0 + Duration::from_millis(100)).len(), 1);
        assert_eq!(q.due(t0 + Duration::from_millis(500)).len(), 1);
        assert_eq!(
            Queue::new(9999).offset,
            Duration::from_millis(u64::from(MAX_OFFSET_MS))
        );
    }

    #[test]
    fn super_sound_volume_is_decibels_and_the_rest_linear() {
        assert!((linear(SoundTarget::Super(1), 0.0) - 1.0).abs() < 1e-6);
        assert!((linear(SoundTarget::Super(1), -2000.0) - 0.1).abs() < 1e-6);
        assert_eq!(linear(SoundTarget::Builtin(SoundId(0)), 0.5), 0.5);
        assert_eq!(linear(SoundTarget::FmodGroup(1), -1.0), 0.0);
    }

    const RATE: f64 = 50.0;

    #[test]
    fn a_tune_on_the_beat_is_left_alone() {
        // A 10 s loop started at tick 100; at tick 400 (6 s in) the sink reads 6.01 s.
        let mut m = MusicSync::new(100.0, 1.0, 10.0);
        assert!((m.expected(400.0, RATE) - 6.0).abs() < 1e-9);
        assert_eq!(m.correction(400.0, RATE, 6.01), None);
        // One tick (20 ms) of drift is fine, and so are two ticks.
        assert_eq!(m.correction(400.0, RATE, 6.02), None);
        assert_eq!(m.correction(400.0, RATE, 6.039), None);
    }

    #[test]
    fn a_lag_spike_is_seeked_back_to_the_tick_clock() {
        // The sim stalled for 0.5 s while the sound device kept playing: the tick clock is at
        // 6.0 s, the sink at 6.5 s.
        let mut m = MusicSync::new(100.0, 1.0, 10.0);
        let to = m.correction(400.0, RATE, 6.5).expect("seeks");
        assert!((to - 6.0).abs() < 1e-9);
        // It is left alone right after (the seek takes a moment to show).
        assert_eq!(m.correction(401.0, RATE, 6.5), None);
        // And checked again once it has settled.
        assert!(m.correction(420.0, RATE, 9.0).is_some());
    }

    #[test]
    fn the_drift_is_measured_round_the_loop() {
        // Expected 0.01 s into the loop, the sink 0.01 s before the end of the last one: 20 ms off.
        let mut m = MusicSync::new(0.0, 1.0, 10.0);
        assert_eq!(m.correction(500.5, RATE, 9.99), None);
        // The other way round: expected 9.99, sink 0.01.
        let mut m = MusicSync::new(0.0, 1.0, 10.0);
        assert_eq!(m.correction(499.5, RATE, 0.0), None);
    }

    #[test]
    fn pitch_speeds_the_expected_position_and_a_pause_holds_it() {
        let mut m = MusicSync::new(0.0, 1.0, 100.0);
        // 2 s at normal speed, then double speed for 2 s of ticks: 2 + 4 = 6 s.
        m.set_speed(100.0, RATE, 2.0);
        assert!((m.expected(200.0, RATE) - 6.0).abs() < 1e-9);
        // Paused at tick 200: still 6 s at tick 300; resumed at 300, 1 s of ticks at 2x later.
        m.set_paused(200.0, RATE, true);
        assert!((m.expected(300.0, RATE) - 6.0).abs() < 1e-9);
        assert_eq!(m.correction(300.0, RATE, 99.0), None);
        m.set_paused(300.0, RATE, false);
        assert!((m.expected(350.0, RATE) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn a_shake_runs_its_ticks_and_then_stops() {
        let mut s = Shake::default();
        assert_eq!(s.offset(), Vec2::ZERO);
        s.tick(&[OutputEvent::Script(ScriptOutput::Shake {
            ticks: 2,
            strength: Real::new(4.0),
        })]);
        assert!(s.offset().length() <= 4.0 * 2f32.sqrt());
        assert_ne!(s.offset(), Vec2::ZERO);
        s.tick(&[]);
        s.tick(&[]);
        assert_eq!(s.offset(), Vec2::ZERO);
    }

    #[test]
    fn the_demo_package_starts_its_music_and_has_every_sound_file() {
        use iwbtg_sim::input::InputFrame;
        use iwbtg_sim::room::StartPoint;
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-audio.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let files = sound_bytes(&pkg.files);
        for s in &pkg.def.sounds {
            assert!(files.contains_key(&s.name), "no file for {}", s.name);
        }
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
        };
        let mut sim = Sim::new(pkg.def.clone(), options).unwrap();
        let mut mixer = Mixer::default();
        let mut cmds = Vec::new();
        // The room start of the first room happens in `Sim::new`, before the first step.
        for cue in cues(&sim.world().output) {
            cmds.extend(mixer.handle(&pkg.def, &cue));
        }
        for _ in 0..3 {
            sim.step(InputFrame::EMPTY);
            for cue in cues(&sim.world().output) {
                cmds.extend(mixer.handle(&pkg.def, &cue));
            }
        }
        let theme = pkg.def.sounds.iter().position(|s| s.name == "theme").unwrap();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, Cmd::Start { sound, looping: true, .. } if sound.0 as usize == theme))
        );
    }
}
