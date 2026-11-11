# Sim core plan: 15. register, Sound

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

### Sound

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| S1 | sound_isplaying latency | UNKNOWN | `SoundRules::isplaying_latency` | 0 ticks | audio-isplaying-latency |
| S2 | Second MULTIMEDIA start while one plays | UNKNOWN | `SoundRules::multimedia_second` | stop the first | audio-multimedia-overlap |
| S3 | sound_loop on an already playing MULTIMEDIA sound | UNKNOWN | `SoundRules::loop_playing` | ignore | room creation code loops |
| S4 | game_restart keeps sounds | Inferred | `sound` on `GameRestart` | keeps | audio-restart-carryover |
| S5 | DLL getters return the last value set exactly | UNKNOWN | adapters | exact | audio-supersound-deathfade (rec), audio-fmod-pitchfade (rec) |
