# Sim register: Input

Part of the [sim register](../sim.md#register).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../../plans/fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks phase 3; T2 and T3 move to the port (rule on the tier page).

### Input

| Id | Item | Impact | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|---|
| I1 | keyboard_check_direct latch (pressed since last direct read, shared per key); recorder's probe reads consume it | T1 | Inferred; the recorder has no direct-key latch, so its traces cannot settle it. The one named per-engine edge value (`TickRules::direct_latch`); every other engine uses the plain frame contract (0005, #1155) | `InputState::direct`, probe `KeyRead::Direct` | latch on; probe consumes | real runner (research#8); t4-input-direct-tap, t5-input-restart-latch, trick-restart-glitch-b (rec, cannot settle) |
| I2 | Held value at the end of a tick with press and release | T1 | UNKNOWN; the standard contract keeps both edge bits and `held` is the inputs' state at the sample (client collector and `.inputs` compiler agree) | `InputFrame::held` | as built | T3 |
| I3 | Second Shift press while one is held | T1 | Decided: press counts (edges are per input; client collector and `.inputs` compiler, #1155) | client `Collector`, `sim::input` per-key edges | press counts | T1, T2 |
| I4 | Key repeat sets pressed | T1 | Inferred no | client collector | no | T8 |
| I5 | Edges after a blocking pause | T1 | Decided: drop the edges, keep held (the pause menu takes no frame; on close it takes one, #1155) | client pause, `Collector::take_frame`, `InputState::clear` | drop | T9 |
| I6 | Game-side held flags go stale across death | T1 | Inferred | `KidGlobals` key flags | stale kept | T10 |
| I7 | Control latency: a press responds within `latency_max` ticks | T1 | Design; F1 feel contract `input/jump_on_press` (0005, #1155) | `CONTROL_LATENCY_MAX_TICKS`, client collector | 1 tick | playtest |
| I8 | Raw keys or reserved bits in the sim's input state | T1 | Built: the latch stores `InputFrame::masked`, so only action bits enter the hashed state (0005, #1155) | `InputState::latch`, `InputFrame::masked` | masked | unit test |
