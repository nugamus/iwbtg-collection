# Sim core plan: 8.4 Converter requests

Part of [the sim core plan](../sim-core.md), section 8.4.


Checked against `research/build/packages/rely_the_needle/` on 2026-10-24. The client never edits converter output; it accepts the current output only through `LoadMode::Trace` and the legacy physics mapping.

| Id | Request | Evidence | Gate |
|---|---|---|---|
| CR1 | Map rely objBLOCK (`@block_2`, `script = "block_2"`, ~600 instances, the floor under the room1 spawn) to the `block` preset with `visual_spawns_collider` and `collider` (the plain block kind it spawns; `kid_collides = false` on the visual). CR16 to CR18 (tier 1 and tier 2 built-in params, kind references): `research/specs/converter-requests.md` | `objects/block_2.toml`; only 19 plain `block` instances in the package | M1 |
| CR2 | Map `warp_start` to `difficulty_warp` with `dif` from creation code, or mark it inert-safe | `objects/warp_start.toml`, placed in rSelectStage | M1b |
| CR3 | `status = "infra"` objects (world, init, blood, bow, GAMEOVER, bloodEmitter, playerface, menus) become built-ins or are dropped, never `script =` | `objects/world.toml`, `init.toml`; trace-map `kind = ""` rows | M1 |
| CR4 | `object_order` lists every spawnable engine kind with its original index (bow 25, GAMEOVER 51, blood 11, bloodEmitter 175, playerface 608 in rely) | trace-map rows with `kind = ""` | M1 |
| CR5 | Instance creation code to params for warps (`roomTo`, `warpX`, `warpY`) | room_1 "1 instance with creation code, not converted" | M3a |
| CR6 | Room `role` (boot, title, menu, select, play) | 4.1 | M1 (defaults to play) |
| CR7 | `start_room` stays the title for game select art, plus `[engine] new_game_room = "room_1"` | `start_room = "title"` | M3b |
| CR8 | physics.toml in the 5.2 form | 5.2 | after WP5a |
| CR9 | Sound kind and length in ticks (ffprobe duration × 50); also for files a game loads through SuperSound or GMFMODSimple, which sit in `sounds/` or `music/` and are named by stem (`GameDef::dll_sound_length`, #288) | section 7 | WP10 tests on packages |
| CR10 | Kind names and object bases that are not built-ins of 6.1 (they load inert in Trace mode) | loader warning CR10 | M3a |
| CR11 | Write `counts_as` instead of the legacy object key `roles` | loader warning CR11 | M3a |
| CR12 | `[defaults.<kind>]` sprites for bare built-ins in game.toml (not only in tests/trace-map.toml) | loader warning CR12 | M3a |
| CR13 | Room `file_speed` in each room file (not only in tests/trace-map.toml) | loader warning CR13 | M3a |
| CR14 | `room_order` in game.toml (not only in tests/trace-map.toml) | loader warning CR14 | M3a |
| CR15 | Sprite strips at most 4096 px on a side (split or re-pack wider strips) | loader warning CR15 | M3a |
| CR16 | Tier 1 params per built-in: `block` `kid_collides`, `slip`, `mask`; `kid_start` `shift_at`, `grav_gated`, `creates`, `offset_x`, `offset_y`; `kid_bullet` `block_check`, `life`; `spike` `dir`, `size`, `auto_rotate`, `destroys_bullets`; and add the built-ins that exist to `known_kinds` | research/specs/converter-requests.md CR16; `docs/format/builtins/{block,kid_start,kid_bullet,spike}.md`; kid/A1.md to A4.md, B.md | M3a (a game's own trace check shows no unknown param for these) |
| CR17 | Tier 2 params: `vine` `side`; `water` `variant`, `fall_cap`, `refresh`, `air_jump`; `moving_platform` `hspeed`, `vspeed`, `bounce`, `carry_rule`; `fake_block`, `hidden_block`, `trigger`, `trap`, `edge_warp`, `drop_platform`, `gravity_flip`, `bullet_blocker`, `slope`, `boss_gate`, `boss_item`, `secret_item`, `item_viewer`, `game_clear`, `music` params; objects a built-in cannot express yet stay unmapped | research/specs/converter-requests.md CR17 (client PRs #253, #257, #266, #271); `docs/format/builtins/<kind>.md` | M3a |
| CR18 | A kind-typed param (`collider`, `watched_object`, `trigger_kind`, `group`) is the id of a preset written `"@id"`, or a bare built-in name; an unknown bare name is inert in Trace mode (CR10) and an error in Strict mode | research/specs/converter-requests.md CR18; loader error for a missing target | M3a |
| CR19 | Write the params the built-ins take: `moving_platform` kind instead of `platform[moving]`; `save` `min_difficulty_removed` (Medium 0, Hard 1, in A3 games 2, VHard 2) instead of `difficulty`; bare `slope` instead of `dir`; `gravity_flip` `direction = "up"` or `"down"` instead of `up`. Sounds and music share one id space: a sound and a music track with the same name become `death` and `death_2` | loader warning `unknown param` (research#57); `docs/format/builtins/{moving_platform,platform,save,slope,gravity_flip}.md`; research/specs/converter-requests.md CR19 | M3a (`pkg check --trace-mode` on the 20 packages shows no `unknown param` and no duplicate sound or music id; #142) |
