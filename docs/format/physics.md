# `physics.toml`: the kid's physics profile

One file per game, next to `game.toml`. It says how the kid moves, jumps, shoots, dies and saves. The kid is one built-in (`kid`, [builtins/kid.md](builtins/kid.md)); every difference between games is a value in this file or a named action in an event list. Nothing in the sim branches on the family. Schema: [schema/v0/physics.schema.json](schema/v0/physics.schema.json). A complete example (rely-the-needle-4-2): [examples/rely-physics.toml](examples/rely-physics.toml).

Source: [sim core plan](../plans/sim-core.md) sections 5.1 to 5.3; `crates/sim/src/def/physics.rs`; `crates/sim/src/kid/presets.rs`.

## Presets and overrides

`preset = "A1"` picks the family whose defaults fill every key the file leaves out. Any key can be overridden, one at a time: a written value (including `false` and `0`) replaces the preset's, and a missing one keeps it. Lists (`jumps`, `step`, `collisions`, ...) are replaced whole.

| Preset | Defaults |
|---|---|
| `A1` | rely-the-needle-4-2 (kid/A1.md): rely's numbers, event lists, collisions, death order and save model. |
| `A2` to `A5`, `B`, `C` | Record the family name and use the `A1` defaults until their work packages (WP5b, WP5c) add theirs. |

An empty file is the same as `preset = "A1"`. The loader rejects unknown keys, sections, action names, run names, class names and enum values, so a typo never falls back silently.

## Event lists

An event is an ordered list of action names. The kid runs them top to bottom; a list stops early when an action asks to abort the tick (a room change). An action that belongs to a family that has not landed returns `Unsupported` and halts the sim with its name, so a profile never silently skips one.

`[events]` keys:

| Key | Runs |
|---|---|
| `create` | the kid's Create |
| `room_start` | Room Start |
| `step` | Step |
| `end_step` | End Step |
| `alarms` | `[{ n = 0, run = [...] }]`: Alarm `n` |
| `keys` | `[{ action = "suicide", on = "press", run = [...] }]`: `on` is `held`, `press` or `release`; `action` is a client action name |
| `collisions` | `[{ with = "block", run = "block_standard" }]`: the kid collides with an instance of that class. Every row whose `with` matched runs, in row order |

| `boundary` | the room edge: `none`, `intersect_origin` (A1), `outside_room_kill` (family B: Outside Room kills unless a `warp` or `edge_warp` overlaps the kid) or `end_step_origin` (WP5c, `Unsupported` for now) |

`intersect_origin` (kid/A1.md "Intersect Boundary"): the kid's Intersect Boundary event tests the origin (strictly: `x = room_width` is inside). Outside the room with no `edge_warp` overlapping the kid, it dies (`kill`). With one, the room it names is requested and the kid moves to its `warp_x`, `warp_y`, or, when both are 0, the kid and the bow are destroyed. Then, if the kid is alive, it wraps: `x < 0` gives `x = room_width - 16` and undoes this step's `vspeed` on `y`; `x > room_width` gives `x = 16` and the same; else `y < 0` gives `y = room_height - 16` and `x = xprevious`; else `y > room_height` gives `y = 16` and `x = xprevious`. The warp target is read from the `edge_warp`'s room and warp point (a per-edge table is `Unsupported`, #250).

Not yet in `[events]`: the outside-room events of the other models.

### Actions (event lists)

| Action | What it does (kid/A1.md) |
|---|---|
| `create_defaults` | Create 1-2: copies `[numbers]` into the kid, clears `frozen`, `frozen2` and `muteki`, sets `djump`, gravity and `image_speed`; alpha 0 when `misc.fade_in`; faces left only when `global.image_xscale` is -1. |
| `ensure_world` | Create 3: spawns the game's `world` kind unless a live one exists. |
| `first_play_save` | Create 4: with no save, deaths and time become 0 and the position is saved by `[save]`. |
| `arm_play_timer` | Create 5: `alarm[0] = room_speed`. |
| `create_bow` | Create 6: a bow at the kid's position. |
| `gravity` | Step action 2.4: gravity back to `numbers.gravity` unless on a ladder (`gravity.reapply = "step"`). |
| `walk` | Step action 3.1-3.4: direction from Right then Left (right wins; both are read), 0 when frozen; facing and `hspeed` unless a vine blocks; running and idle sprites. |
| `air_sprite` | Step action 3.5: jump and fall sprites past `sprite_threshold`; on a platform, clears `onPlatform` when none is `platform.hold_probe` px below. |
| `fall_cap` | Step action 3.6: `abs(vspeed) > max_vspeed` becomes `sign * max_vspeed` (`fall_cap.rule = "clamp_abs"`). |
| `shoot` | `playerShoot` on a Shoot press: a bullet at the kid when fewer than `bullet.max` exist, with `hspeed = bullet.speed * facing`. |
| `jump` | `playerJump` on a Jump press (`jump.air = "cur_jumps"`: first jump `jump[1]` sets the counter to 2, walking off a ledge spends it, an air press uses `jump[curJumps]` and adds 1, water or `infJump` repeat `jump[maxJumps]`; `jump[n]` is 1-based): ground jump (`-jumps[0]`, sets `djump`) when a `jump.ground_classes` instance is 1 px below, or a `ground_also` condition holds; else air jump (`-jumps[1]`, `djump` false) when `djump` is set or a water 2 is below. |
| `stop_if_frozen2` | Step action 1 of family B: with `global.frozen2`, gravity, `image_speed` and speed become 0 and the rest of the Step list is skipped. |
| `sync_yflag` | Step, `playerMove` 1-2 (`flip.model = "yflag"`): `image_yscale` takes the current `yflag`, then `yflag` follows `global.reverse`. The mask flips one tick after the global does. |
| `player_reverse` | `playerReverse` (the gravity arrows' Alarm 1): `global.reverse` toggles, `yflag` negates, `y` moves 4 times the new `yflag`, `curJumps` 1, `vspeed` 0. Also `kid::api::player_reverse`. OPEN: with several kids the original script's per-kid toggle of the global. |
| `set_view_angle` | Room Start: with `global.reverse` view 0 is enabled at angle 180, else angle 0. |
| `killer_check` | `playerMisc` 1, in the Step: the kid dies when it touches a `killer` instance. It runs before motion, so it sees the position the last tick ended at. |
| `ensure_bow` | Room Start and `playerMisc`: a bow at the kid unless one exists. |
| `cancel_poll` | Alarm 0 when `jump.cancel = "poll_alarm"`: Jump up and moving against gravity cuts `vspeed` by `release_mult` and stops; else re-arms for 1 tick. |
| `release` | `playerVJump` on a Jump release: upward `vspeed` times `release_mult`, every release. |
| `vines` | Step action 4 (`vine.model = "distance_asym"`): left vine nearer than `left_below` or right vine at exactly `right_equal`, with free space below: slide, face away, kick or drop on a press. |
| `platform_gravity_zero` | Step action 6 (rely): gravity 0 while `onPlatform`. |
| `fade_in` | Step action 7 (rely): while alpha is below 1 add `misc.fade_step`; no clamp. |
| `play_timer` | Alarm 0: `global.time` + 1 unless the room is a Boot, Title, Menu or Select room, then `alarm[0] = room_speed`. |
| `kill` | The death sequence (`[death]`), as a key or alarm action. |

All real comparisons use the GM epsilon (`NumRules::compare`), so for example the rely fade-in takes 10 steps.

### Collision runs (`collisions.run`)

| Run | What it does |
|---|---|
| `block_standard` | kid/A1.md "Collision with block": horizontal contact, vertical contact (a landing refreshes `djump`; `gravity.block_landing_zeroes` zeroes gravity), then the diagonal case. With `flip.model = "yflag"` a reversed kid refreshes the jump counter on the ceiling side (`vspeed <= 0`) instead of the floor side. Another flipped kid is `Unsupported` (WP5c). |
| `platform_standard` | kid/A1.md "Collision with platform": lands when `y - vspeed / 2 <= platform.y`; one-way from below. A reversed `yflag` kid lands when `y - vspeed / 2 >= platform.y + reversed_above` and is put at `platform.y + reversed_snap`. |
| `kill` | The other instance kills the kid (`[death]`). |

## Sections

Every key below is optional. Defaults are the `A1` preset's.

`[numbers]`: `jumps` (list, ground jump then air jumps, `[8.5, 7.0]`), `gravity` (0.4), `run_speed` (3), `max_vspeed` (9), `max_fall_speed` (8, read only by `shift_then_clamp`), `release_mult` (0.45), `sprite_threshold` (0.05), `image_speed_idle` (0.2), `image_speed_run` (0.5).

`[input]`: `walk` = `direct_latch` (`keyboard_check_direct`, sees a tap inside one tick) or `sampled`; `vine_keys` = `edges` (this tick's press) or `latched_flags` (WP5c).

`[jump]`: `ground_classes` (class names), `ground_also` (`platform_flag`, `ladder_jump`), `ground_sets_djump`, `air` (`djump` or `cur_jumps`; `djump_then_tjump` is WP5c), `water_air` (`none`, `water2_without_flag`, `water_unlimited`; `water3_keeps` is WP5c), `max_jumps` (family B, 2), `inf_jump` (B cheat flag, false), `blocked_by_pause_press` (a tick with a Pause press does nothing), `cancel` (`edge`: the `release` action cuts the jump; `poll_alarm`: `release` does nothing and the `cancel_poll` alarm action cuts it).

`[fall_cap]`: `rule` = `clamp_abs` or `clamp_against_gravity` (clamp only when `vspeed * yflag` exceeds `max_vspeed`; the others are WP5c).

`[gravity]`: `reapply` = `step`, or `end_step_slope` (Create leaves gravity 0; the `slope_b` action, WP5b.4, sets it; `create_only` is WP5c), `block_landing_zeroes`.

`[platform]`: `hold_probe` (4), `rule` = `half_vspeed`, `snap` (9), `take` = `vspeed` or `delta_y` (the platform's `y - yprevious`, family B; `yspeed_flipped` is WP5c), `reversed_above` (family B 15) and `reversed_snap` (24).

`[vine]`: `model` = `distance_asym` or `none` (`distance_sym_early` is WP5c), `left_below` (2), `right_equal` (1), `slide` (2), `kick` (`[15, -9]`: horizontal, vertical), `drop` (3).

`[water]`, `[ice]`, `[slope]`, `[ladder]`: empty until their work packages. `[flip]`: `model` (`none`, `swap_object`, `yflag`, `grav_sign`). With `yflag` (family B) a reversed kid (`yflag` -1): `walk` swaps Left and Right; `vines` slide at `slide * yflag`, kick at `kick[1] * yflag` and take the other key as the away key; `air_sprite` swaps the jump and fall sprites; `shoot` places the bullet at `y - offset[1] * yflag`; `create_defaults` reads `global.reverse` (y - 4, `curJumps` 1). `swap_object` and `grav_sign` are WP5c.

`[death]`: `order`, the steps in the order they run: `sound` (not played yet, #231), `blood_emitter` (spawned with a `flipped` param), `destroy_kid`, `game_over` (it places itself at view 0 plus its own offset params), `deaths_plus_one` (checked), `save_counters` (death-only save). `select_room_reset`: in a Select room a kill requests the same room, destroys the kid and does nothing else. `stop_music`: not built (`true` is `Unsupported`). Nothing happens while `muteki` is set.

`[bullet]`: `max` (4), `speed` (16), `offset` (`[0, 0]`; family B `[2, 2]`: the bullet appears at `x + offset[0] * facing`, `y - offset[1] * yflag`).

`[save]`: `position` (`xy_base100_bytes`, `xy_base256_bytes`, `xy_text2_dec`, `xy_previous`, `fixed_point`, `floor_nudge`), `fixed` (`[x, y]` for `fixed_point`).

`[misc]`: `fade_in`, `fade_step` (0.1).

`[sprites]`: sprite names from the package's sprite table, by role: `idle`, `running`, `jump`, `fall`, `sliding`. A role left out leaves the sprite alone.

## Converter note (CR8)

The converter writes `physics.toml` in this form for every game (plan 8.4, CR8). Per game:

1. Set `preset` to the kid's family from the research spec and write only what differs from the preset. rely itself needs `preset = "A1"` and `[sprites]`.
2. Write the Step list from the spec's Step actions in order, using the names above. Where the spec's first Step action is the ladder (rely), list `gravity` until the ladder action exists (WP5c, #232); `ad_nudge` and `flip_swap` are dormant in rely and left out.
3. Collapse the kid's `Collision with ...` events into `events.collisions` rows: block and solidblock to `block_standard`, platform to `platform_standard`, the killer parent to `kill`. A game that names a parent object maps it to the class that object has (`counts_as`).
4. Key events become `events.keys` rows (`Key Q pressed` is `suicide` on `press` running `kill`), Alarm 0 an `events.alarms` row.
5. Use sprite names from the package, never the original's ids.
6. A spec rule with no action here yet is not guessed: the converter lists it as a gap for the work package that owns the family (5.3), and the game's trace check stays red until it lands.
