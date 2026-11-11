# Sim core plan: 15. register, Kid and mechanics

Part of [the sim core plan](../sim-core.md), section 15 (the intro and the column meanings are there).

### Kid and mechanics

| Id | Item | Status | Isolation | Default | Settles it |
|---|---|---|---|---|---|
| K1 | Resting frac(y) .4 (depends on C11) | Inferred; consistent with t-col-03 and t-col-08 | emergent | — | famb-spawn-settle (567.0, 567.4, 567.4), k-spawn-idle (rec) |
| K2 | RORI/FT boundary roomTo lookup with no instance | UNKNOWN | kid action `edge_lookup_point_first` | treat as no changer, kill | rori/fairy-tale edge trace |
| K3 | niseblock second count test | UNKNOWN | `fake_block` param `second_test` | passes | — |
| K4 | Bullet runs its own Step on its creation tick | follows R9 (Confirmed (recorder)) | R9 | per R9 `LaterInOrder`: yes when the bullet's object comes later in the order than the kid's | t-evo-1 (rec), k-bullets (rec) |
| K5 | Water reads pressed/released during collisions | Inferred | input frame fixed for the tick | yes | a-water (rec) |
| K6 | Shared-last trigger semantics | Inferred / UNKNOWN U-MECH-2 | `trigger` model `shared_last` | all instances written, first read | TR-MECH-01 |
| K7 | Flag-array triggers across R | UNKNOWN U-MECH-3 | R16 | cleared on game_restart | TR-MECH-08 |
| K8 | Stop-chain timing | UNKNOWN U-MECH-8 | `trap::stop_chain` | alarms from launch tick | TR-MECH-03 |
| K9 | Orbit trig | UNKNOWN U-MECH-6 | `orbit` | as original | TR-MECH-05 |
| K10 | Room variable assignment (spook oRoomSwitch) | Inferred / UNKNOWN U-MECH-9 | `edge_warp` mode `room_assign` | acts like room_goto | sh-roomswitch (rec) |
| K11 | 5 fps rMenu first tick | UNKNOWN | R14 | 50 after Room Start | TR-MECH-12 |
| K12 | Showdown room creation code vs playerStart Room Start | UNKNOWN A3 U11 | 7-step order | creation actions first | A3-T20 |
