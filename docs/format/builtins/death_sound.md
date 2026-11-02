# `death_sound`

Family B's `objDeathSound` (kid/B.md "Death (killPlayer)", kaenbyou traces famb-death-timing-* and audio-supersound-deathfade-*). The kid's death step `death_sound` creates it (engine role `death_sound`, none unless the game declares it). Source: `crates/sim/src/builtins/death_sound.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `sound` | sound | none | The death sound effect. |
| `music` | sound | none | The music to fade. |
| `start_volume` | real | 10000 | The fade's start, on the original's SuperSound scale (10000 is full). |

- **Create**: arms Alarm 0 for the next tick.
- **Step**, while music is not muted (`global.music`): sends the music volume `now / start_volume` (a linear 0 to 1 stand-in for the original's dB scale; the client maps it), then `now` drops by 100 while it is at least 2000. The first Step sends full volume, the 82nd and later send 0.19 (kaenbyou: 2000 on frame 181, 1900 from 182). The original would pause a non-streamed music at the floor; kaenbyou streams, so that is not built.
- **Alarm 0**: plays the death sound unless it is already playing. It is first heard one tick after the death (the kid is gone from that tick, kaenbyou f13).
- **Retry** (`[engine.restart] kid_steps.release_death_sound`): stops the death sound, restores the music volume to full and destroys the object (kid/B.md Restart 2).

OPEN: the "sound is not muted" flag of the original's check is not modelled (there is no sound-mute global yet); the real SuperSound start volume (the DLL value) comes from the package's `start_volume`.
