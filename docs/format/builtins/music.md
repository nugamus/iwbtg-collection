# `music`

Plays a track through the sound model (`sound::play`). Source: `crates/sim/src/builtins/music.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `track` | sound | required | |
| `loop` | bool | true | |
| `mode` | `on_create`, `every_step_if_not_playing` | `on_create` | `on_create` (KOT playMusic, whose creation code plays the track): Create plays it. `every_step_if_not_playing` (SHO playMusic001-005): each Step, while music is on and the track is not playing, stop all sounds and play it. |
| `respects_music_off` | bool | false | Silent while music is off (`global.musicD`). |

The music-off flag is `MechGlobals::music_muted`, which the `world` built-in flips on a Mute press. OPEN: the sound model is WP10 (#20), so the events cannot run in tests yet; the params and the event set are tested.
