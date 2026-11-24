# Sim register: Kid and mechanics

Part of the [sim register](../sim.md#register).

The **Impact** column is the fidelity tier ([fidelity impact tiers](../../plans/fidelity-tiers.md)): T1 can change a ranked run, T2 only off-ranked paths or visuals and sound, T3 only a probe column, T? is not judged. T1 blocks phase 3; T2 and T3 move to the port (rule on the tier page). The **Claim** column names the stable research id the row implements, from `research/specs/engines/gm8/kid/` and `research/specs/mechanics/` (claims `A1-NNN`..`C-NNN` and `MECH-NNN`), indexed by `research/specs/INDEX.md`. The register id stays the client-side key. — marks a row with no matching GM8 claim, with the reason. `scripts/test_spec_claims.py` fails on a cited id that is not in [`claim-ids.txt`](claim-ids.txt).

### Kid and mechanics

| Id | Item | Impact | Status | Isolation | Default | Settles it | Claim |
|---|---|---|---|---|---|---|---|
| K1 | Resting frac(y) .4 (depends on C11) | T1 | Inferred; consistent with t-col-03 and t-col-08 | emergent | — | famb-spawn-settle (567.0, 567.4, 567.4), k-spawn-idle (rec) | COL-088 |
| K2 | RORI/FT boundary roomTo lookup with no instance | T1 | UNKNOWN | kid action `edge_lookup_point_first` | treat as no changer, kill | rori/fairy-tale edge trace | A1-131 |
| K3 | niseblock second count test | T1 | UNKNOWN | `fake_block` param `second_test` | passes | — | A1-082 |
| K4 | Bullet runs its own Step on its creation tick | T1 | follows R9 (Confirmed (recorder)) | R9 | per R9 `LaterInOrder`: yes when the bullet's object comes later in the order than the kid's | t-evo-1 (rec), k-bullets (rec) | TICK-112 |
| K5 | Water reads pressed/released during collisions | T1 | Inferred | input frame fixed for the tick | yes | a-water (rec) | A3-163 |
| K6 | Shared-last trigger semantics | T1 | Inferred / UNKNOWN U-MECH-2 | `trigger` model `shared_last` | all instances written, first read | TR-MECH-01 | MECH-085 |
| K7 | Flag-array triggers across R | T1 | UNKNOWN U-MECH-3 | R16 | cleared on game_restart | TR-MECH-08 | MECH-203 |
| K8 | Stop-chain timing | T1 | UNKNOWN U-MECH-8 | `trap::stop_chain` | alarms from launch tick | TR-MECH-03 | MECH-208 |
| K9 | Orbit trig | T1 | UNKNOWN U-MECH-6 | `orbit` | as original | TR-MECH-05 | MECH-206 |
| K10 | Room variable assignment (spook oRoomSwitch) | T1 | Inferred / UNKNOWN U-MECH-9 | `edge_warp` mode `room_assign` | acts like room_goto | sh-roomswitch (rec) | MECH-118 |
| K11 | 5 fps rMenu first tick | T2 | UNKNOWN | R14 | 50 after Room Start | TR-MECH-12 | TICK-004 |
| K12 | Showdown room creation code vs playerStart Room Start | T1 | UNKNOWN A3 U11 | 7-step order | creation actions first | A3-T20 | A3-167 |
