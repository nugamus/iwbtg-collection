# Sim core plan: 15. register, Input

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks M2; T2 and T3 move to the port (rule on the tier page).

### Input

| Id | Item | Impact | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|---|
| I1 | keyboard_check_direct latch (pressed since last direct read, shared per key); recorder's probe reads consume it | T1 | Inferred; the recorder has no direct-key latch, so its traces cannot settle it | `InputState::direct`, probe `KeyRead::Direct` | latch on; probe consumes | real runner (research#8); t4-input-direct-tap, t5-input-restart-latch, trick-restart-glitch-b (rec, cannot settle) |
| I2 | Held value at the end of a tick with press and release | T1 | UNKNOWN | `InputFrame::held` | as built | T3 |
| I3 | Second Shift press while one is held | T1 | UNKNOWN | `sim::input` per-key edges | press counts | T1, T2 |
| I4 | Key repeat sets pressed | T1 | Inferred no | client collector | no | T8 |
| I5 | Edges after a blocking pause | T1 | UNKNOWN | client pause, `InputState::clear` | drop | T9 |
| I6 | Game-side held flags go stale across death | T1 | Inferred | `KidGlobals` key flags | stale kept | T10 |
