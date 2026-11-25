//! One sim tick's output, drained into the client's presentation state (#1463).
//!
//! The live path drains `world.output` in [`crate::view::tick`]: the sign box
//! (`hud::note_tick`), the audio queue, the screen shake and the room transition. The replay
//! viewer re-simulates ticks itself ([`crate::viewer`]) and drains the same events through
//! [`drain`], so a viewed run shows its signs, plays its sounds and shakes like the run did.
//!
//! A seek steps many ticks at once: the presentation state still follows every tick (the last
//! sign, the last shake), but only lasting sound events (music starting, stopping or changing)
//! are queued, so a scrub across a long replay does not fire hundreds of one-shot sounds at once.
//! That is the same filter `skip.rs` uses when it skips a cutscene.

use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::sound::SoundOutput;

use crate::audio::{Queue, Shake};
use crate::time::Instant;
use crate::view::transition::Transition;

/// Feeds one tick's `output` to the audio queue, the screen shake and the room transition.
/// `audible` is false while a seek re-simulates skipped ticks: then only music changes are
/// queued, not one-shot sounds. The sign box is drained by the caller until `view::hud` is
/// reachable from here (#1463); it is the one part of `view::tick` still duplicated.
pub(crate) fn drain(
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

/// Whether a sound event still matters after its tick: every event but a one-shot play, so music
/// that starts, stops or changes during the skipped ticks still reaches the mixer.
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
