//! One sim tick's output, drained into the client's presentation state (#1463).
//!
//! The live path drains `world.output` in [`super::tick`]: the sign box ([`hud::note_tick`]),
//! the audio queue, the screen shake and the room transition. The replay viewer re-simulates
//! ticks itself and drains the same events through [`Outputs::drain`], so a viewed run shows its
//! signs, plays its sounds and shakes like the run did.
//!
//! A seek steps many ticks at once: the presentation state still follows every tick (the last
//! sign, the last shake), but only lasting sound events (music starting, stopping or changing)
//! are queued, so a scrub across a long replay does not fire hundreds of one-shot sounds at once.
//! That is the same filter `skip.rs` uses when it skips a cutscene.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::sound::SoundOutput;

use super::hud::{self, SignText};
use super::transition::Transition;
use crate::audio::{Queue, Shake};
use crate::time::Instant;

/// Feeds one live tick's `output` to the sign box, the audio queue, the screen shake and the room
/// transition, as [`super::tick`] does. The viewer uses [`Outputs`] so it never names the private
/// `SignText`.
pub(crate) fn drain(
    output: &[OutputEvent],
    sign: &mut SignText,
    audio: &mut Queue,
    shake: &mut Shake,
    transition: Option<&mut Transition>,
    audible: bool,
) {
    hud::note_tick(sign, output);
    drain_effects(output, audio, shake, transition, audible);
}

/// Everything but the sign: the audio, the shake and the transition.
fn drain_effects(
    output: &[OutputEvent],
    audio: &mut Queue,
    shake: &mut Shake,
    transition: Option<&mut Transition>,
    audible: bool,
) {
    if audible {
        audio.push(Instant::now(), output);
    } else {
        let lasting: Vec<OutputEvent> = output.iter().filter(|e| lasts(e)).cloned().collect();
        if !lasting.is_empty() {
            audio.push(Instant::now(), &lasting);
        }
    }
    shake.tick(output);
    if let Some(transition) = transition {
        transition.observe(output, Instant::now());
    }
}

/// The presentation resources a re-simulated tick drains into, as one system parameter so the
/// viewer can hold them without naming the private [`SignText`]. `sign` is optional because the
/// viewer's unit tests have no HUD; a run always has one.
#[derive(SystemParam)]
pub(crate) struct Outputs<'w> {
    sign: Option<ResMut<'w, SignText>>,
    audio: ResMut<'w, Queue>,
    shake: ResMut<'w, Shake>,
    transition: Option<ResMut<'w, Transition>>,
}

impl Outputs<'_> {
    /// Feeds one re-simulated tick's `output` to the presentation state. `audible` is false while
    /// a seek skips ticks: then only music changes are queued, not one-shot sounds.
    pub(crate) fn drain(&mut self, output: &[OutputEvent], audible: bool) {
        if let Some(sign) = self.sign.as_deref_mut() {
            hud::note_tick(sign, output);
        }
        drain_effects(
            output,
            &mut self.audio,
            &mut self.shake,
            self.transition.as_deref_mut(),
            audible,
        );
    }

    /// Drops a sign the live run raised, so a viewer opened or scrubbed backward does not show
    /// text from a tick it is no longer at.
    pub(crate) fn clear_sign(&mut self) {
        if let Some(sign) = self.sign.as_deref_mut() {
            sign.clear();
        }
    }
}

/// Whether a sound event still matters after its tick: every event but a one-shot play, so music
/// that starts, stops or changes during skipped ticks still reaches the mixer.
fn lasts(event: &OutputEvent) -> bool {
    matches!(event, OutputEvent::Sound(s) if !matches!(s, SoundOutput::Play { looping: false, .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::def::{RoomId, SoundId};
    use iwbtg_sim::room::RoomOutput;
    use iwbtg_sim::sound::{SoundOutput, SoundTarget};

    fn play(looping: bool) -> OutputEvent {
        OutputEvent::Sound(SoundOutput::Play {
            target: SoundTarget::Builtin(SoundId(0)),
            looping,
            length_ticks: 0,
            tick: 0,
        })
    }

    #[test]
    fn a_seek_keeps_music_but_drops_one_shot_sounds() {
        assert!(!lasts(&play(false)));
        assert!(lasts(&play(true)));
        assert!(!lasts(&OutputEvent::Room(RoomOutput::Entered {
            from: None,
            to: RoomId(0),
        })));
    }
}
