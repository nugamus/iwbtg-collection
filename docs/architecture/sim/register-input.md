# Sim register: Input

Part of the [sim register](../sim.md#register).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../../plans/fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks phase 3; T2 and T3 move to the port (rule on the tier page). The **Claim** column names the stable research id the row implements, from `research/specs/engines/gm8/runner/input.md` (claims `INP-NNN`), indexed by `research/specs/INDEX.md`. The register id stays the client-side key. — marks a row with no matching GM8 claim, with the reason. `scripts/test_spec_claims.py` fails on a cited id that is not in [`claim-ids.txt`](claim-ids.txt).

### Input

| Id | Item | Impact | Status | Isolation | Default | Settles it | Claim |
|---|---|---|---|---|---|---|---|
| I1 | keyboard_check_direct latch (pressed since last direct read, shared per key); recorder's probe reads consume it | T1 | Inferred; the recorder has no direct-key latch, so its traces cannot settle it. The one named per-engine edge value (`TickRules::direct_latch`); every other engine uses the plain frame contract (0005, #1155) | `InputState::direct`, probe `KeyRead::Direct` | latch on; probe consumes | real runner (research#8); t4-input-direct-tap, t5-input-restart-latch, trick-restart-glitch-b (rec, cannot settle) | INP-052 |
| I2 | Held value at the end of a tick with press and release | T1 | UNKNOWN; the standard contract keeps both edge bits and `held` is the inputs' state at the sample (client collector and `.inputs` compiler agree) | `InputFrame::held` | as built | T3 | INP-076 |
| I3 | Second Shift press while one is held | T1 | Decided: press counts (edges are per input; client collector and `.inputs` compiler, #1155) | client `Collector`, `sim::input` per-key edges | press counts | T1, T2 | INP-044 |
| I4 | Key repeat sets pressed | T1 | Inferred no | client collector | no | T8 | INP-078 |
| I5 | Edges after a blocking pause | T1 | Decided: drop the edges, keep held (the pause menu takes no frame; on close it takes one, #1155) | client pause, `Collector::take_frame`, `InputState::clear` | drop | T9 | INP-081 |
| I6 | Game-side held flags go stale across death | T1 | Inferred | `KidGlobals` key flags | stale kept | T10 | INP-060 |
| I7 | Control latency: a press responds within `latency_max` ticks | T1 | Design; F1 feel contract `input/jump_on_press` (0005, #1155) | `CONTROL_LATENCY_MAX_TICKS`, client collector | 1 tick | playtest | — (client design; no GM8 claim) |
| I8 | Raw keys or reserved bits in the sim's input state | T1 | Built: the latch stores `InputFrame::masked`, so only action bits enter the hashed state (0005, #1155) | `InputState::latch`, `InputFrame::masked` | masked | unit test | — (client design; no GM8 claim) |
