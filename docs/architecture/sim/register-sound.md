# Sim register: Sound

Part of the [sim register](../sim.md#register).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../../plans/fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks M2; T2 and T3 move to the port (rule on the tier page).

### Sound

| Id | Item | Impact | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|---|
| S1 | sound_isplaying latency | T1 | UNKNOWN | `SoundRules::isplaying_latency` | 0 ticks | audio-isplaying-latency |
| S2 | Second MULTIMEDIA start while one plays | T1 | UNKNOWN | `SoundRules::multimedia_second` | stop the first | audio-multimedia-overlap |
| S3 | sound_loop on an already playing MULTIMEDIA sound | T1 | UNKNOWN | `SoundRules::loop_playing` | ignore | room creation code loops |
| S4 | game_restart keeps sounds | T1 | Inferred | `sound` on `GameRestart` | keeps | audio-restart-carryover |
| S5 | DLL getters return the last value set exactly | T2 | UNKNOWN | adapters | exact | audio-supersound-deathfade (rec), audio-fmod-pitchfade (rec) |
