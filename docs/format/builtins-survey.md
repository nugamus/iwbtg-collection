# Built-in parameter survey

Survey for [#1076](https://github.com/nugamus/iwbtg-collection/issues/1076) and the promotion/demotion rule of [decision 0004](../decisions/0004-extension-model.md). The shared built-ins keep only behavior that is general across the collection; a one-game quirk stays in the package's data or a script.

## Method

The parameter list is the `ParamSpec` inventory in `crates/sim/src/builtins/*.rs`, including the files' `real(...)`/`flag(...)` helpers (284 parameters across 45 kinds; the issue's 286 was the estimate before the survey). Each parameter was joined to its row in `docs/format/builtins/<kind>.md` (type, default, meaning and the games the page names) and to the research converter (`converter/games.toml`, `converter/kinds.toml`, 14 `[game.<slug>]` sections), which records the params each converted game emits.

The 20-port census (research#172) is not in this tree: the games repository holds no converted packages yet, so the converter's 14 games are the machine-checkable usage. A page's parenthetical game list is the second source. The verdict is therefore a recommendation with its evidence; a parameter with no usage and no named game is **keep** (there is no demotion evidence), not moved.

Verdicts:

- **keep**: a general concept, three or more games, or no one-game evidence.
- **keep/shared**: two games. The difference is served by a versioned shared script (`lib:`, [#797](https://github.com/nugamus/iwbtg-collection/issues/797)) until a third game promotes it.
- **move**: one game and not a general concept. Removal is a format-major change or a documented alias (the format-1 freeze waits on #795/#928); the target is a preset, a script on `base`, or data.

## Summary

- **284** parameters across **45** kinds.
- **209** keep, **19** keep/shared (two games), **56** move.
- The switches already modelled as data stay data and are not built-in params: the physics `[save] position` (e.g. `floor_nudge`), the room key `djump_grants_tjump`, the `[tricks]` switches and the physics profile's family values. `save.saved_position`, `fixed_x`/`fixed_y` and `game_clear.saved_position` duplicate `[save] position`; the duplicate is part of the move, not a second concept.

## Moves

Format 0 keeps every parameter as a documented alias, so existing packages and their `gameplay_hash` do not change; a removal lands at a format major or through the `[compat]`/script layer as it becomes available. The target column says what replaces each group. Package data and scripts cover most of them. Two groups are not pure package moves: `gravity_flip.transition` (kaenbyou's view rotation) and the camera/edge-warp view writes are runner/view work (Area A; the profile layer is #1160), and the save params need a save hook (#776/#797).

The loader knows the marker (#1375): every move param is a `ParamSpec` legacy alias carrying the game that may use it and the issue that replaces it. `iwbtg pkg check` warns when a package whose id is not that game uses one, in format 0 and in format 1 while the params are still accepted (#1510), so a new package cannot pick up a one-game param silently. The set is pinned against this table by `builtins_legacy_aliases` in `crates/sim/tests/`.

### `block` (4)

Target: a script on `base = "block"` (spawn the named collider; declare classes) or kind data.

| Param | Game | Meaning |
|---|---|---|
| `collider` | rely-the-needle-4-2 | Required with `visual_spawns_collider`: the plain block to create. |
| `kid_collides` | rely-the-needle-4-2 | Loader choice: false drops the `BLOCK` class so the kid has no collision event with it. On a placed instance it gives a derived kind (`block[kid_collides=false]`, objects.md, Params that pick a class or an event set). |
| `proximity_frame` | eat-the-lemon | Eat-the-lemon block02. Create sets `image_speed` 0; each Step shows frame 1 while the kid is in a box around the block, frame 0 otherwise or with no kid. It adds the kind's Step event, so a placed instance that sets it gets a derived kin... |
| `visual_spawns_collider` | rely-the-needle-4-2 | Create spawns the kind named by `collider` at the block's own position (rely objBLOCK, CR1). |

### `bullet_blocker` (1)

Target: a script on `base = "bullet_blocker"` (its `collide` handler ignores the kid).

| Param | Game | Meaning |
|---|---|---|
| `except_when_overlapping_kid` | wanna | Does nothing while it overlaps a kid (WAN bulletBlocker). |

### `camera` (3)

Target: the engine camera's view rule (Area A) or a script; not the shared camera kind.

| Param | Game | Meaning |
|---|---|---|
| `screen_h` | poronkusema | `screen_snap` screen size (poronkusema 960 x 540). |
| `screen_w` | poronkusema | `screen_snap` screen size (poronkusema 960 x 540). |
| `y_offset` | machinary | Added to the snapped y (machinary 4). |

### `edge_warp` (5)

Target: the package's per-edge table; keep only the common edge warp.

| Param | Game | Meaning |
|---|---|---|
| `down_room` | eat-the-new-year | The per-edge table (NY roomout h, m, u, s, kid/A4.md). `EdgeTable::target(edge)` answers for the crossed edge: left when `x < 0`, right when `x > room_width`, up when `y < 0`, down when `y > room_height`, in that order. An unset edge has... |
| `left_room` | eat-the-new-year | The per-edge table (NY roomout h, m, u, s, kid/A4.md). `EdgeTable::target(edge)` answers for the crossed edge: left when `x < 0`, right when `x > room_width`, up when `y < 0`, down when `y > room_height`, in that order. An unset edge has... |
| `mode` | go-the-spook-house | `room_assign` (SPK oRoomSwitch, K10): a kid touch queues the room change like `room_goto` and puts the kid at `warp_x`, and at `warp_y` when it is not 0, keeping his speeds (Confirmed (recorder), trace SH-roomswitch). With no `room_to` i... |
| `right_room` | eat-the-new-year | The per-edge table (NY roomout h, m, u, s, kid/A4.md). `EdgeTable::target(edge)` answers for the crossed edge: left when `x < 0`, right when `x > room_width`, up when `y < 0`, down when `y > room_height`, in that order. An unset edge has... |
| `up_room` | eat-the-new-year | The per-edge table (NY roomout h, m, u, s, kid/A4.md). `EdgeTable::target(edge)` answers for the crossed edge: left when `x < 0`, right when `x > room_width`, up when `y < 0`, down when `y > room_height`, in that order. An unset edge has... |

### `fake_block` (1)

Target: a script on `base = "fake_block"` or kind data.

| Param | Game | Meaning |
|---|---|---|
| `blood_solid_when_no_kid` | deride-kotaro-from-aslant | While no kid exists it is solid, so blood sticks (KOT niseblock, blockVis). |

### `gravity_flip` (1)

Target: the `[compat]` profile/view rule of 0005 (F1); keep the general flip.

| Param | Game | Meaning |
|---|---|---|
| `transition` | kaenbyou | `swap_object` and `grav_sign` are the same write; the kid swaps object or flips on his next Step (kid code). `yflag_with_view_rotation` (KAE): see below. |

### `hidden_block` (4)

Target: a script on `base = "hidden_block"`.

| Param | Game | Meaning |
|---|---|---|
| `destroyed_by_blood` | keep-celebrating-for-xia | Blood touching it while it is still invisible destroys the block (XIA). |
| `destroyed_by_bullet` | keep-celebrating-for-xia | A kid bullet touching it destroys the block (XIA objBlockInvis). The bullet dies by the kid bullet's own block rule. |
| `reveal_distance` | keep-celebrating-for-xia | XIA uses 1. |
| `stops_bullets` | wanna | A kid bullet that touches it is destroyed (WAN). |

### `jump_refresher` (1)

Target: a script on `base = "jump_refresher"`.

| Param | Game | Meaning |
|---|---|---|
| `hide_ticks` | showdown | Ticks it stays hidden (the room speed in showdown). |

### `kid_start` (2)

Target: the spawn rule or a script on `base = "kid_start"`.

| Param | Game | Meaning |
|---|---|---|
| `creates` | animus | `player_or_player2` (animus): `global.grav` 1 creates the flipped kid (engine role `kid_flipped`). Needs `shift_at = "room_start"`. |
| `shift_at` | rely-the-needle-4-2 | Where the offset is added: Create moves the start itself (rely), or Room Start adds it to the new kid's position. |

### `killer` (3)

Target: kind data (scale, speed) or a script on `base = "killer"`.

| Param | Game | Meaning |
|---|---|---|
| `bounce` | animus | Step runs the Bounce action, not precise, against solid objects (`move_bounce_solid(false)`; ANI boundwhite, research CLONE_SPEC "Bounce"): `hspeed` flips when (`x + hspeed`, `y`) is blocked, `vspeed` when (`x`, `y + vspeed`) is, both wh... |
| `image_speed` | animus | Cherry: 1/15. |
| `scale` | little-runner | Sets both image scales (LR object698: 9). |

### `moving_platform` (1)

Target: a script on `base = "moving_platform"`.

| Param | Game | Meaning |
|---|---|---|
| `marker_reverse` | animus | What a fake block marker touch does: reverse every direction, only the vertical, or nothing (animus: `vertical`). The platform handles Collision with the `fake_block` class (unless `none`) and calls `moving_platform::reverse_on_marker`. |

### `particle_system` (1)

Target: a script on `base = "particle_system"`.

| Param | Game | Meaning |
|---|---|---|
| `on_room_start_except` | go-the-spook-house | Rooms where Room Start does nothing at all: no call and no destroy (go-the-spook-house oRainSys). |

### `save` (4)

Target: the profile `[save]` section, or a script on `base = "save"` once a save hook exists (#776/#797).

| Param | Game | Meaning |
|---|---|---|
| `once` | little-trap | Destroyed after saving (autosave). |
| `requires_kid_depth0` | machinary | The kid's depth must be 0 (machinary). |
| `requires_kid_in_room` | keep-celebrating-for-xia | The kid's origin must be inside the room: no save when `x < 0`, `x > room_width`, `y < 0` or `y > room_height` (the edges count as inside; the same test as the edge death, on the origin, not the hitbox). Every save from the point checks ... |
| `self_destroy_alarm` | little-runner | Alarm 0 at Create destroys the point after this many ticks (LR object799). |

### `spike` (1)

Target: a script on `base = "spike"`.

| Param | Game | Meaning |
|---|---|---|
| `destroys_bullets` | eat-the-new-year | A collision with the kid's bullet destroys the bullet (eat-the-new-year). Read from the kind, since the handler is declared per kind. |

### `trap` (12)

Target: presets or scripts on `base = "trap"` (the model belongs to the game).

| Param | Game | Meaning |
|---|---|---|
| `endx` | little-runner | `move_to` (LR object749, object751-755): runs only while `endx` or `endy` is 0 and `global.stop[trg]` is set. Per axis with a target it moves by the step toward it, then within half a step snaps to the target, zeroes the step and replace... |
| `endy` | little-runner | `move_to` (LR object749, object751-755): runs only while `endx` or `endy` is 0 and `global.stop[trg]` is set. Per axis with a target it moves by the step toward it, then within half a step snaps to the target, zeroes the step and replace... |
| `hspd` | little-runner | `move_to` (LR object749, object751-755): runs only while `endx` or `endy` is 0 and `global.stop[trg]` is set. Per axis with a target it moves by the step toward it, then within half a step snaps to the target, zeroes the step and replace... |
| `hspeed` | showdown | `velocity` (the fixed-speed forms too). SHO spikeMoveUp: the loader folds `ue`, `sita`, `hidari`, `migi` into these. |
| `launch` | go-the-spook-house | The form. |
| `stop_on_stopper` | get-the-rainbow-feather | Touching a `trap_stopper` stops it; `flush` first snaps it to the stopper edge on the side it moves toward, horizontal first (RBF object1024). |
| `to_x` | go-the-spook-house | `step_toward` (go-the-spook-house playerKiller switch spikes, with `trigger_model = switch_array`): Create sets a target of 0 to the instance's own x (y). End Step, while the trigger fires, moves 1 px toward (`to_x`, `to_y`): x first, y ... |
| `to_y` | go-the-spook-house | `step_toward` (go-the-spook-house playerKiller switch spikes, with `trigger_model = switch_array`): Create sets a target of 0 to the instance's own x (y). End Step, while the trigger fires, moves 1 px toward (`to_x`, `to_y`): x first, y ... |
| `trigger_id` | go-the-spook-house | The id it waits for. With `shared_last` and an unset trigger (reads 0) an id of 0 launches at once. |
| `trigger_model` | go-the-spook-house | As `trigger`. |
| `vspd` | little-runner | `move_to` (LR object749, object751-755): runs only while `endx` or `endy` is 0 and `global.stop[trg]` is set. Per axis with a target it moves by the step toward it, then within half a step snaps to the target, zeroes the step and replace... |
| `vspeed` | showdown | `velocity` (the fixed-speed forms too). SHO spikeMoveUp: the loader folds `ue`, `sita`, `hidari`, `migi` into these. |

### `trigger` (2)

Target: a script on `base = "trigger"`.

| Param | Game | Meaning |
|---|---|---|
| `one_shot_destroy` | showdown | Destroy after touch; show frame 1 (SHO). |
| `sets_frame` | showdown | Destroy after touch; show frame 1 (SHO). |

### `warp` (1)

Target: a script on `base = "warp"` or kind data.

| Param | Game | Meaning |
|---|---|---|
| `gate_closed` | keep-celebrating-for-xia | A closed gate: `hide` is invisible and its touch does nothing; `destroy` destroys the warp. |

### `water` (6)

Target: presets or scripts on `base = "water"`.

| Param | Game | Meaning |
|---|---|---|
| `angle_step` | showdown | Added to `image_angle` every Step, with no wrap (showdown hidariSuityuu02 `1`, migiSuityuu02 `-1`). A non-zero value gives the kind a Step event; the turn comes before the `area` test. Their Create `image_angle = 1` is written as the ins... |
| `area_bottom` | eat-the-new-year | NY water: all four, or none. The water has no collision event; each Step, every kid whose origin is strictly inside (`left < x < right`, `top < y < bottom`) gets the same effects. |
| `area_left` | eat-the-new-year | NY water: all four, or none. The water has no collision event; each Step, every kid whose origin is strictly inside (`left < x < right`, `top < y < bottom`) gets the same effects. |
| `area_right` | eat-the-new-year | NY water: all four, or none. The water has no collision event; each Step, every kid whose origin is strictly inside (`left < x < right`, `top < y < bottom`) gets the same effects. |
| `area_top` | eat-the-new-year | NY water: all four, or none. The water has no collision event; each Step, every kid whose origin is strictly inside (`left < x < right`, `top < y < bottom`) gets the same effects. |
| `cur_jumps` | kaenbyou | Sets the kid's family B jump counter on touch (kaenbyou objWater2: 2). |

### `world` (3)

Target: the lifecycle/pause slice (#1149) or a script on the world kind.

| Param | Game | Meaning |
|---|---|---|
| `clear_switches_on_room_end` | go-the-spook-house | Room End clears the switch array (go-the-spook-house). |
| `clear_triggers_to` | poronkusema | Room Start sets `global.trigger[0]` to `[n]` false (poronkusema: 10). |
| `pause` | keep-celebrating-for-xia | `deactivate` is xia's pause (below). |

## Keep/shared (two games)

These stay in the kind until the shared script library (#797) can carry the difference, or a third game promotes them.

| Kind.param | Games |
|---|---|
| `blood_emitter.destroyed_by_restart` | eat-the-lemon, kaenbyou |
| `blood_emitter.mode` | eat-the-lemon, kaenbyou |
| `blood_emitter.outlives_stream` | eat-the-lemon, kaenbyou |
| `camera.on_room_end` | deride-kotaro-from-aslant, rely-the-needle-4-2 |
| `camera.on_room_start` | deride-kotaro-from-aslant, rely-the-needle-4-2 |
| `camera.on_step` | deride-kotaro-from-aslant, rely-the-needle-4-2 |
| `death_sound.sound` | eat-the-lemon, kaenbyou |
| `kid_param_zone.target` | rainbow, rainbow-feather |
| `killer.kill_outside_view` | eat-the-lemon, kaenbyou |
| `music.mode` | deride-kotaro-from-aslant, showdown |
| `save.alarm_save` | eat-the-lemon, keep-celebrating-for-xia |
| `save.requires_gravity` | fairy-tale, keep-celebrating-for-xia |
| `secret_item.sound` | kaenbyou, rainbow-feather |
| `secret_item.sprite` | kaenbyou, rainbow-feather |
| `sign.mode` | animus, fairy-tale |
| `trap.flag_guard` | fairy-tale, rainbow-feather |
| `warp.clear_rising` | eat-the-lemon, kaenbyou |
| `water.fall_cap` | little-runner, rainbow-feather |
| `world.mute` | keep-celebrating-for-xia, wanna |

## Full survey

| Kind.param | Type | Default | Games (converter) | Page names | Verdict |
|---|---|---|---|---|---|
| `kid_start.creates` | `player`, `player_or_player2` | `player` | - | animus | move |
| `kid_start.flipped_offset_x` | real | `offset_y`, `offset_x` (23, 17) | - | - | keep |
| `kid_start.flipped_offset_y` | real | `offset_y`, `offset_x` (23, 17) | - | - | keep |
| `kid_start.grav_gated` | bool | true | - | deride-kotaro-from-aslant, rori, showdown | keep |
| `kid_start.offset_x` | real | 17, 23 | - | machinary | keep |
| `kid_start.offset_y` | real | 17, 23 | - | machinary | keep |
| `kid_start.shift_at` | `create`, `room_start` | `create` | - | rely-the-needle-4-2 | move |
| `kid_bullet.block_check` | `before_move`, `collision_destroy`, `stop_and_die` | `before_move` | - | - | keep |
| `kid_bullet.life` | real | 40 | - | - | keep |
| `blood.flipped` | bool | false | - | - | keep |
| `blood.sticks` | bool | true | - | - | keep |
| `blood_emitter.chance` | real | 0.2 | - | - | keep |
| `blood_emitter.destroyed_by_restart` | bool | false | eat-the-lemon, kaenbyou | eat-the-lemon, kaenbyou | keep/shared |
| `blood_emitter.flipped` | bool | false | - | - | keep |
| `blood_emitter.life` | real | 20 | - | - | keep |
| `blood_emitter.mode` | `random_dir`, `angle_sweep`, `particles` | `random_dir` | eat-the-lemon, kaenbyou | - | keep/shared |
| `blood_emitter.outlives_stream` | bool | false | eat-the-lemon, kaenbyou | - | keep/shared |
| `blood_emitter.per_angle` | whole | 8 | - | - | keep |
| `blood_emitter.per_step` | whole | 40 (15 in `particles`) | - | - | keep |
| `blood_emitter.speed_max` | real | 6 | - | - | keep |
| `bow.destroy_in_select` | bool | true | - | - | keep |
| `bow.follow` | `step`, `begin_step`, `end_step` | `step` | - | eat-the-lemon, keep-celebrating-for-xia, wanna | keep |
| `bow.medium_only` | bool | true | - | - | keep |
| `game_over.delay` | real | 0 | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, kaenbyou, little-runner, machinary, wanna | deride-kotaro-from-aslant | keep |
| `game_over.offset_x` | real | 400, 304 | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, get-the-rainbow-feather, go-the-jun-g-world, kaenbyou, little-runner, little-trap, machinary, poronkusema, wanna | deride-kotaro-from-aslant, poronkusema, rely-the-needle-4-2 | keep |
| `game_over.offset_y` | real | 400, 304 | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, get-the-rainbow-feather, go-the-jun-g-world, kaenbyou, little-runner, little-trap, machinary, poronkusema, wanna | deride-kotaro-from-aslant, poronkusema, rely-the-needle-4-2 | keep |
| `game_over.view_relative` | bool | true | - | - | keep |
| `death_sound.music` | sound | the music playing at Create | - | - | keep |
| `death_sound.sound` | sound | - | eat-the-lemon, kaenbyou | - | keep/shared |
| `death_sound.start_volume` | real | 10000 | - | - | keep |
| `world.clear_switches_on_room_end` | bool | false | - | go-the-spook-house | move |
| `world.clear_triggers_to` | whole | - | - | poronkusema | move |
| `world.latch_cleared_by_world` | bool | false | - | eat-the-lemon, kaenbyou, keep-celebrating-for-xia | keep |
| `world.mute` | bool | false | - | keep-celebrating-for-xia, wanna | keep/shared |
| `world.pause` | `none`, `deactivate` | `none` | - | keep-celebrating-for-xia | move |
| `world.pause_delay` | whole | 0 | - | - | keep |
| `world.timer_needs_fps` | bool | false | - | - | keep |
| `world.timer_per_tick` | whole | 0 | - | - | keep |
| `world.timer_play_rooms_only` | bool | false | - | - | keep |
| `world.timer_stops_on_clear` | bool | false | - | - | keep |
| `difficulty_warp.dif` | whole number | required | - | - | keep |
| `difficulty_warp.room_to` | room | the game's new-game room | - | - | keep |
| `difficulty_warp.transition` | whole numbers | 0 | - | - | keep |
| `difficulty_warp.transition_steps` | whole numbers | 0 | - | - | keep |
| `camera.border_x` | real | 400, 304 | - | - | keep |
| `camera.border_y` | real | 400, 304 | - | - | keep |
| `camera.h_div` | real | 16, 12 | - | - | keep |
| `camera.mode` | `screen_snap`, `smooth_follow`, `end_step_clamp` | `screen_snap` | - | - | keep |
| `camera.on_room_end` | bool | true, false, false | - | deride-kotaro-from-aslant, rely-the-needle-4-2 | keep/shared |
| `camera.on_room_start` | bool | true, false, false | - | deride-kotaro-from-aslant, rely-the-needle-4-2 | keep/shared |
| `camera.on_step` | bool | true, false, false | - | deride-kotaro-from-aslant, rely-the-needle-4-2 | keep/shared |
| `camera.screen_h` | real | 800, 608 | - | poronkusema | move |
| `camera.screen_w` | real | 800, 608 | - | poronkusema | move |
| `camera.tolerance` | real | 2 | - | - | keep |
| `camera.v_div` | real | 16, 12 | - | - | keep |
| `camera.y_offset` | real | 0 | - | machinary | move |
| `music.loop` | bool | true | - | - | keep |
| `music.mode` | `on_create`, `every_step_if_not_playing` | `on_create` | - | deride-kotaro-from-aslant, showdown | keep/shared |
| `music.respects_music_off` | bool | false | - | - | keep |
| `music.track` | sound | required | - | - | keep |
| `particle_system.on_destroy` | same | `none` | - | - | keep |
| `particle_system.on_room_end` | same | `none` | - | - | keep |
| `particle_system.on_room_start` | `none`, `clear`, `destroy_emitters`, `destroy` | `none` | - | - | keep |
| `particle_system.on_room_start_destroys_instance` | bool | false | - | - | keep |
| `particle_system.on_room_start_except` | list of rooms | - | - | go-the-spook-house | move |
| `particle_system.system` | particles, or a list of them | required | - | - | keep |
| `block.blood_sticks` | bool | true | - | - | keep |
| `block.collider` | kind | - | rely-the-needle-4-2 | - | move |
| `block.depth` | real | kind default | - | - | keep |
| `block.kid_collides` | bool | true | rely-the-needle-4-2 | - | move |
| `block.mask` | `full32`, `mini16`, `large64`, `line_h`, `line_v`, `thin32x3`, `precise_sprite` | `full32` | - | - | keep |
| `block.proximity_frame` | bool | false | - | eat-the-lemon | move |
| `block.slip` | real | 0 | - | deride-kotaro-from-aslant, little-trap, rainbow | keep |
| `block.solid` | bool | kind default (true) | - | - | keep |
| `block.visible` | bool | kind default (false) | - | - | keep |
| `block.visual_spawns_collider` | bool | false | rely-the-needle-4-2 | rely-the-needle-4-2 | move |
| `platform.parent_kind` | `block`, `solidblock` | `block` | - | - | keep |
| `platform.width` | real | 32 | - | - | keep |
| `moving_platform.bounce` | bool | true | - | - | keep |
| `moving_platform.carry_rule` | `yuuutu`, `showdown_split`, `xia_grav`, `b_lines` | `yuuutu` | - | - | keep |
| `moving_platform.hspeed` | real | 0 | - | - | keep |
| `moving_platform.marker_reverse` | `both`, `vertical`, `none` | `both` | - | animus | move |
| `moving_platform.start_on_touch` | bool | false | - | - | keep |
| `moving_platform.vspeed` | real | 0 | - | - | keep |
| `drop_platform.mode` | `fall`, `rise`, `rise_kae` | `fall` | - | - | keep |
| `drop_platform.solid` | bool | kind data | - | - | keep |
| `drop_platform.speed` | real | 2 | - | little-runner | keep |
| `path_platform.keep_moving_when_kid_absent` | bool | false | - | - | keep |
| `path_platform.path` | path index | required | - | - | keep |
| `path_platform.path_end` | `stop`, `restart`, `continue`, `reverse` | `stop` | - | - | keep |
| `path_platform.scale` | real | 0 | - | - | keep |
| `path_platform.speed` | real | 0 | - | - | keep |
| `path_platform.start_on_touch` | bool | false | - | - | keep |
| `slope.scale_x` | real | 1 | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `slope.shape` | `rise_right`, `rise_left`, `precise_sprite` | `rise_right` | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `fake_block.approach_distance` | real | 2 | - | - | keep |
| `fake_block.blood_solid_when_no_kid` | bool | false | - | deride-kotaro-from-aslant | move |
| `fake_block.reverses_platforms` | bool | false | - | - | keep |
| `fake_block.solid` | bool | kind data | - | - | keep |
| `fake_block.sound` | sound | - | - | - | keep |
| `fake_block.stops_bullets` | bool | false | - | - | keep |
| `fake_block.vanish_on` | `touch`, `approach`, `group_fade`, `never`, `tile_delete` | `approach` | - | - | keep |
| `fake_block.visible` | bool | kind data | - | - | keep |
| `hidden_block.destroy_when_no_kid` | bool | true | - | - | keep |
| `hidden_block.destroyed_by_blood` | bool | false | - | keep-celebrating-for-xia | move |
| `hidden_block.destroyed_by_bullet` | bool | false | - | keep-celebrating-for-xia | move |
| `hidden_block.reveal_distance` | real | 2 | - | keep-celebrating-for-xia | move |
| `hidden_block.sound` | sound | - | - | - | keep |
| `hidden_block.stops_bullets` | bool | false | - | wanna | move |
| `breakable_block.break_on` | `shot`, `touch_delay` | `shot` | - | rainbow, rainbow-feather, showdown | keep |
| `breakable_block.sound` | sound | - | - | - | keep |
| `breakable_block.stage_ticks` | whole, at least 1 | 50 | - | - | keep |
| `breakable_block.stages` | whole, at least 1 | 1 | - | - | keep |
| `boss_gate.boss_index` | whole, 0 to 15 | required | - | - | keep |
| `boss_gate.sprite` | Real | - | - | - | keep |
| `conveyor.h` | real | required | - | - | keep |
| `conveyor.mode` | `add_each_tick`, `set_on_top_edge` | `add_each_tick` | - | rainbow, rainbow-feather, showdown | keep |
| `trap_stopper.mode` | `stop`, `destroy` | `stop` | - | fairy-tale, rainbow, rainbow-feather | keep |
| `trap_stopper.size` | real | - | - | - | keep |
| `spike.auto_rotate` | bool | false | - | - | keep |
| `spike.depth` | real | kind default | - | - | keep |
| `spike.destroys_bullets` | bool | false | - | eat-the-new-year | move |
| `spike.dir` | `up`, `down`, `left`, `right` | - | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `spike.size` | real (32 or 16) | 32 | - | - | keep |
| `spike.visible` | bool | kind default | - | - | keep |
| `killer.bounce` | bool | false | animus | animus | move |
| `killer.depth` | bool, real | kind default | - | - | keep |
| `killer.destroy_outside_room` | bool | false | - | - | keep |
| `killer.hspeed` | real | 0 | - | - | keep |
| `killer.image_speed` | real | image default | animus | - | move |
| `killer.kill_outside_view` | bool | false | eat-the-lemon, kaenbyou | kaenbyou | keep/shared |
| `killer.scale` | real | - | - | little-runner | move |
| `killer.visible` | bool, real | kind default | - | - | keep |
| `killer.vspeed` | real | 0 | - | - | keep |
| `orbit.angle` | real | the direction from the instance to the center at Create | - | - | keep |
| `orbit.center_x` | real | required | - | - | keep |
| `orbit.center_y` | real | required | - | - | keep |
| `orbit.child` | whole 1 to 4096; kind | 1; required | - | - | keep |
| `orbit.count` | whole 1 to 4096; kind | 1; required | - | - | keep |
| `orbit.destroy_outside_room` | bool | false | - | - | keep |
| `orbit.keep_spawner` | bool | false | - | - | keep |
| `orbit.mode` | `self`, `spawner` | `self` | - | kaenbyou, little-runner, poronkusema | keep |
| `orbit.radius` | real, not negative | the distance from the instance to the center at Create | - | - | keep |
| `orbit.speed` | real | 0 | - | - | keep |
| `orbit.unit` | `degrees`, `radians` | `degrees` | - | - | keep |
| `trigger.array` | `trigger`, `stop` | `trigger` | - | - | keep |
| `trigger.group` | kind | - | - | - | keep |
| `trigger.model` | the words above | `shared_last` | - | - | keep |
| `trigger.need_key` | real | - | - | - | keep |
| `trigger.one_shot_destroy` | bool | false | - | showdown | move |
| `trigger.reset` | bool | false | - | - | keep |
| `trigger.sets_frame` | bool | false | - | showdown | move |
| `trigger.size` | Real | - | - | - | keep |
| `trigger.sound` | Sound | - | - | - | keep |
| `trigger.trg` | real | 0 | - | - | keep |
| `trap._b` | kind | - | - | - | keep |
| `trap._c` | kind | - | - | - | keep |
| `trap.alm` | real | 0 | - | - | keep |
| `trap.alm2` | real | 0 | - | - | keep |
| `trap.alm3` | real | 0 | - | - | keep |
| `trap.alm4` | real | 0 | - | - | keep |
| `trap.depth_reveal` | bool | false | - | - | keep |
| `trap.destroy_outside_room` | bool | false | - | - | keep |
| `trap.dir` | real | 0 | - | eat-the-new-year | keep |
| `trap.endx` | real | 0 | - | little-runner | move |
| `trap.endy` | real | 0 | - | little-runner | move |
| `trap.flag_guard` | bool | false | - | fairy-tale, rainbow-feather | keep/shared |
| `trap.h2` | real | 0 | - | - | keep |
| `trap.h3` | real | 0 | - | - | keep |
| `trap.hspd` | real | 0 | - | little-runner | move |
| `trap.hspeed` | real | 0 | - | showdown | move |
| `trap.kind` | Enum | - | go-the-spook-house | - | keep |
| `trap.launch` | `velocity`, `angle`, `path`, `move_to`, `step_toward` | `velocity` | go-the-spook-house | - | move |
| `trap.path` | ? | - | - | - | keep |
| `trap.path_absolute` | ? | - | - | - | keep |
| `trap.path_end` | Enum | - | - | - | keep |
| `trap.path_scale` | ? | - | - | - | keep |
| `trap.path_speed` | ? | - | - | - | keep |
| `trap.sound` | Sound | - | - | - | keep |
| `trap.sound_if_trigger` | ? | - | - | - | keep |
| `trap.speed` | real | 0 | - | eat-the-new-year | keep |
| `trap.stop_on_stopper` | `none`, `zero_speed`, `flush` | `none` | - | get-the-rainbow-feather | move |
| `trap.to_x` | real | 0 | go-the-spook-house | go-the-spook-house | move |
| `trap.to_y` | real | 0 | go-the-spook-house | go-the-spook-house | move |
| `trap.trigger_id` | real | 0 | go-the-spook-house | - | move |
| `trap.trigger_kind` | kind | - | - | - | keep |
| `trap.trigger_kind_b` | Kind | - | - | - | keep |
| `trap.trigger_kind_c` | Kind | - | - | - | keep |
| `trap.trigger_model` | `shared_last`, `per_object`, `global_last`, `flag_array`, `switch_array` | `shared_last` | go-the-spook-house | - | move |
| `trap.v2` | real | 0 | - | - | keep |
| `trap.v3` | real | 0 | - | - | keep |
| `trap.vspd` | real | 0 | - | little-runner | move |
| `trap.vspeed` | real | 0 | - | showdown | move |
| `water.air_jump` | `none`, `press_if_no_djump`, `djump_ignoring_flag` | `none` | showdown | animus, little-runner, showdown | keep |
| `water.air_jump_speed` | `fixed_7`, `kid_jump2` | `fixed_7` | - | - | keep |
| `water.angle_step` | real | 0 | showdown | showdown | move |
| `water.area_bottom` | real | - | - | eat-the-new-year | move |
| `water.area_left` | real | - | - | eat-the-new-year | move |
| `water.area_right` | real | - | - | eat-the-new-year | move |
| `water.area_top` | real | - | - | eat-the-new-year | move |
| `water.cur_jumps` | 0 to 3 | - | - | kaenbyou | move |
| `water.destroy_outside_room` | bool | false | showdown | showdown | keep |
| `water.fall_cap` | real | 2 | - | little-runner, rainbow-feather | keep/shared |
| `water.ground_jump` | bool | false | - | - | keep |
| `water.refresh` | `none`, `on_touch` | `none` | - | - | keep |
| `water.release_same_tick` | bool | false | - | animus, rainbow-feather, wanna | keep |
| `water.variant` | 1, 2, 3 | 1 | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `vine.side` | `left`, `right` | required | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `gravity_flip.direction` | `up`, `down` | required | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | deride-kotaro-from-aslant, fairy-tale | keep |
| `gravity_flip.sound` | sound | - | - | fairy-tale | keep |
| `gravity_flip.transition` | `swap_object`, `grav_sign`, `yflag_with_view_rotation` | `swap_object` | kaenbyou | kaenbyou | move |
| `kid_param_zone.jump` | real | - | - | - | keep |
| `kid_param_zone.jump2` | real | - | - | - | keep |
| `kid_param_zone.jump3` | real | - | - | - | keep |
| `kid_param_zone.max_fall_speed` | real | - | - | - | keep |
| `kid_param_zone.max_vspeed` | real | - | - | - | keep |
| `kid_param_zone.run_speed` | real | - | - | - | keep |
| `kid_param_zone.target` | `toucher`, `player` | `toucher` | - | rainbow, rainbow-feather | keep/shared |
| `jump_refresher.hide_ticks` | real, at least 1 | 50 | - | showdown | move |
| `jump_refresher.tjump` | `none`, `set`, `clear` | `none` | - | - | keep |
| `spring.djump` | `keep`, `set`, `clear` | `keep` | - | - | keep |
| `spring.speed` | real | 10 | - | - | keep |
| `bullet_blocker.except_when_overlapping_kid` | bool | false | - | wanna | move |
| `save.alarm_save` | real | - | eat-the-lemon, keep-celebrating-for-xia | eat-the-lemon, keep-celebrating-for-xia | keep/shared |
| `save.blink` | bool | true | eat-the-lemon, keep-celebrating-for-xia, little-trap | - | keep |
| `save.cooldown` | real | 0 | eat-the-lemon, kaenbyou, keep-celebrating-for-xia | kaenbyou, rely-the-needle-4-2 | keep |
| `save.fixed_x` | real | - | - | - | keep |
| `save.fixed_y` | real | - | - | - | keep |
| `save.min_difficulty_removed` | real (whole) | - | animus, be-fed-up-with-path-w, clear-the-easy-miku-2, deride-kotaro-from-aslant, eat-the-lemon, eat-the-new-year, eat-the-rori-girl, get-the-rainbow-feather, go-the-jun-g-world, go-the-spook-house, help-the-fairy-tale, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, machinary, poronkusema, rely-the-needle-4-2, showdown, wanna | - | keep |
| `save.on_shot` | bool | false | eat-the-lemon | - | keep |
| `save.on_touch` | `never`, `press`, `shoot_press`, `skip_held`, `always` | `press` | eat-the-lemon, kaenbyou, keep-celebrating-for-xia, little-trap | eat-the-lemon, kaenbyou, keep-celebrating-for-xia, little-runner, little-trap, poronkusema, rely-the-needle-4-2, wanna | keep |
| `save.once` | bool | false | little-trap | - | move |
| `save.requires_gravity` | `any`, `normal`, `flipped` | `any` | keep-celebrating-for-xia | fairy-tale, keep-celebrating-for-xia | keep/shared |
| `save.requires_kid_depth0` | bool | false | - | machinary | move |
| `save.requires_kid_in_room` | bool | false | keep-celebrating-for-xia | keep-celebrating-for-xia | move |
| `save.saved_position` | `xy_base100`, `xy_base256`, `xy_text2dec`, `xy_previous`, `fixed_point`, `floor_nudge` | `xy_base100` | eat-the-lemon, kaenbyou, keep-celebrating-for-xia | - | keep |
| `save.self_destroy_alarm` | real | - | - | little-runner | move |
| `warp.clear_rising` | bool | false | eat-the-lemon, kaenbyou | - | keep/shared |
| `warp.gate_bosses` | list of whole numbers, 0 to 15 | - | - | - | keep |
| `warp.gate_closed` | `hide`, `destroy` | `hide` | keep-celebrating-for-xia | - | move |
| `warp.gate_items` | list of whole numbers, 0 to 15 | - | - | - | keep |
| `warp.gate_need` | whole number | all listed | - | - | keep |
| `warp.on_collide` | bool | true | - | - | keep |
| `warp.room_to` | room | required | - | - | keep |
| `warp.screen_shift` | real | 0 | - | - | keep |
| `warp.transition` | whole number | 0 | - | - | keep |
| `warp.transition_steps` | whole number | 0 | - | - | keep |
| `warp.visible` | bool | kind default | - | - | keep |
| `warp.warp_x` | real | 0, 0 | - | - | keep |
| `warp.warp_y` | real | 0, 0 | - | - | keep |
| `warp.zero_axis` | `set`, `keep` | `set` | eat-the-lemon, eat-the-new-year, kaenbyou | - | keep |
| `edge_warp.down_room` | room | - | - | eat-the-new-year | move |
| `edge_warp.left_room` | room | - | - | eat-the-new-year | move |
| `edge_warp.mode` | `boundary`, `room_assign` | `boundary` | - | go-the-spook-house | move |
| `edge_warp.right_room` | room | - | - | eat-the-new-year | move |
| `edge_warp.room_to` | room, real, real | - | - | - | keep |
| `edge_warp.up_room` | room | - | - | eat-the-new-year | move |
| `edge_warp.warp_x` | room, real, real | - | - | - | keep |
| `edge_warp.warp_y` | room, real, real | - | - | - | keep |
| `game_clear.fixed_x` | Real | as `save` | - | - | keep |
| `game_clear.fixed_y` | Real | as `save` | - | - | keep |
| `game_clear.save_with_position` | bool | true | - | - | keep |
| `game_clear.saved_position` | Enum | as `save` | - | - | keep |
| `boss_item.boss_index` | whole, 0 to 15 | required | - | - | keep |
| `boss_item.sound` | Sound | - | - | - | keep |
| `boss_item.sprite` | Real | - | - | - | keep |
| `boss_item.watched_object` | kind | - | - | - | keep |
| `secret_item.item_index` | whole, 0 to 15 | required | - | - | keep |
| `secret_item.sound` | Sound | - | - | kaenbyou, rainbow-feather | keep/shared |
| `secret_item.sprite` | Real | - | - | kaenbyou, rainbow-feather | keep/shared |
| `item_viewer.index` | whole, 0 to 15 | required | - | - | keep |
| `item_viewer.kind` | `boss`, `item` | required | - | - | keep |
| `item_viewer.sprite` | Real | - | - | - | keep |
| `time_limit.hours` | whole number, 0 or more | 0 | - | - | keep |
| `time_limit.minutes` | whole number, 0 or more | 0 | - | - | keep |
| `time_limit.on_expire` | `kill`, `shake`, `nothing`, `goto_room` | `kill` | - | - | keep |
| `time_limit.role` | `timer`, `start_zone` | `timer` | - | - | keep |
| `time_limit.room` | room | - | - | - | keep |
| `time_limit.seconds` | whole number, 0 or more | 0 | - | - | keep |
| `time_limit.shake` | kind | - | - | - | keep |
| `time_limit.warp_x` | real | 0 | - | - | keep |
| `time_limit.warp_y` | real | 0 | - | - | keep |
| `sign.mode` | `on_up_press`, `while_overlapping` | `on_up_press` | - | animus, fairy-tale | keep/shared |
| `sign.text` | text | required | - | - | keep |

## Follow-ups

The moves are grouped by their mechanism so each can land with its package and evidence:

- **Trap models** (12 params: `endx`, `endy`, `hspd`, `vspd`, `hspeed`, `vspeed`, `launch`, `to_x`, `to_y`, `trigger_id`, `trigger_model`, `stop_on_stopper`): presets/scripts on `base = "trap"`; tracked by [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267), blocked by a port package and #797.
- **Water and edge-warp models** (11: the four `water.area_*`, `water.angle_step`, `water.cur_jumps`, the four `edge_warp.*_room`, `edge_warp.mode`): presets/scripts on `base = "water"`/`edge_warp`; tracked by [#1268](https://github.com/nugamus/iwbtg-collection/issues/1268), blocked by a port package and #797.
- **Native-state one-game params** (the rest: `save`, `block`, `hidden_block`, `killer`, `fake_block`, `moving_platform`, `spike`, `trigger`, `particle_system`, `world`, `jump_refresher`, `kid_start`, `bullet_blocker`, `camera`, `warp`, `gravity_flip`): these need a script that reads/writes the built-in's state or a profile rule. `gravity_flip.transition` is F1 and belongs to the `[compat]` view-rotation rule (#1160); the save params need a save hook (#776/#797). Tracked by [#1269](https://github.com/nugamus/iwbtg-collection/issues/1269).
- **Duplicated position model**: `save.saved_position`/`fixed_x`/`fixed_y` and `game_clear.saved_position` repeat `[save] position`. The profile is the authority once the built-in can read it; the param stays an alias until then. Tracked by [#1270](https://github.com/nugamus/iwbtg-collection/issues/1270).
- **Format major**: every removal follows #795/#928 and keeps a documented alias for format 0, so no existing package or `gameplay_hash` moves.

