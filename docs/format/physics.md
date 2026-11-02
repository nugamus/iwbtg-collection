# `physics.toml`: the kid's physics profile

One file per game, next to `game.toml`. It says how the kid moves, jumps, shoots, dies and saves. The kid is one built-in (`kid`, [builtins/kid.md](builtins/kid.md)); every difference between games is a value in this file or a named action in an event list. Nothing in the sim branches on the family. Schema: [schema/v0/physics.schema.json](schema/v0/physics.schema.json). Complete examples: rely-the-needle-4-2 [examples/rely-physics.toml](examples/rely-physics.toml), kaenbyou (family B) [examples/kaenbyou-physics.toml](examples/kaenbyou-physics.toml), and eat-the-lemon as the B preset plus its differences [examples/eat-the-lemon-physics.toml](examples/eat-the-lemon-physics.toml).

Source: [sim core plan](../plans/sim-core.md) sections 5.1 to 5.3; `crates/sim/src/def/physics.rs`; `crates/sim/src/kid/presets.rs`.

## Presets and overrides

`preset = "A1"` picks the family whose defaults fill every key the file leaves out. Any key can be overridden, one at a time: a written value (including `false` and `0`) replaces the preset's, and a missing one keeps it. Lists (`jumps`, `step`, `collisions`, ...) are replaced whole.

| Preset | Defaults |
|---|---|
| `A1` | rely-the-needle-4-2 (kid/A1.md): rely's numbers, event lists, collisions, death order and save model. |
| `B` | kaenbyou (kid/B.md): the `yflag` flip, `cur_jumps` jumps with water, End Step slope code and gravity, polling killer check, Outside Room kill, B's death order with the first kid. eat-the-lemon is `B` with `jump.blocked_by_pause_press = false` and no Alarm 0 (`events.alarms = []`). |
| `A2` | deride-kotaro-from-aslant (kid/A2.md): rely without the fade-in and `platform_gravity_zero`. Animus and easy-miku-2 add `ad_nudge`. Group A's flip by object swap is `Unsupported` (WP5c.4, #131). |
| `A3` | wanna (kid/A3.md): latched vine-key flags, walking that ignores vines, `gravity` after the jump code, `killer_check` in the End Step, held `suicide_check`, A3's death order. showdown overrides the jumps (`djump_then_tjump`), its collisions and keys; its ice is #131. |
| `A4` | machinary (kid/A4.md): no vines, release 0.5, `shift_then_clamp`, a ground jump that leaves `djump` alone, gravity set at Create, fixed room edges, text saves. eat-the-new-year differs (own timer in the kid, bow, roomout edges, slopes: #131). |
| `A5` | little-runner (kid/A5.md): no vines, `gravity_and_unstuck` in the End Step, base-256 saves. go-the-spook-house is `A5` with a different Step list, `clamp_down_only`, 0.5 sprite thresholds, `killer_check` in the End Step and no boundary events. |
| `C` | keep-celebrating-for-xia (kid/C.md): `grav_sign` flip, symmetric early vines, suicide press, End Step edge and killer tests, water 3, the flipped platform rule. Slide blocks, slip and slopes are not in xia's rooms (tier 3, `Unsupported`); the C key teleport and the restart are not built. |

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

| `boundary` | the room edge: `none`, `intersect_origin` (A1), `outside_room_kill` (family B: Outside Room kills unless a `warp` or `edge_warp` overlaps the kid) `intersect_origin_fixed` (A4 machinary: `intersect_origin` with edges 800 by 608 whatever the room, wrapping by adding or subtracting 800 sideways, adding 608 at the top and subtracting 600, not 608, at the bottom) or `end_step_origin` (C: in the End Step the kid dies when its origin is outside the room, strictly, then the End Step list runs) |

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
| `slope_b` | End Step, `playerSlope` (kid/B.md "End Step"): the `slope_wall` slope code, then gravity back to `numbers.gravity * yflag` (`gravity.reapply = "end_step_slope"`), which also runs in rooms without slopes. |
| `ad_nudge` | A2 (animus, easy-miku-2), end of the movement action: with a `block` or `solidblock` 1 px below, a press of A (`extra1`) subtracts 1 from `hspeed` and a press of D (`extra2`) adds 1. Nothing while frozen (OPEN). |
| `key_flags` | A3 Step action 4.1: the game's Left, Right and Jump flags (`KidGlobalsExt`) become 1 on the key's press tick and 0 on its release tick; `input.vine_keys = "latched_flags"` makes `vines` read them. A press and a release in one tick end 0 (OPEN). |
| `suicide_check` | A3 Step action 5: the Suicide key held (not a press) outside Boot, Title, Menu and Select rooms runs `kill` every tick. |
| `gravity_and_unstuck` | A5 (little-runner) End Step: gravity is `numbers.gravity` every tick, frozen or not; a kid overlapping a solid goes back to `yprevious` and moves to contact toward where it was (no limit), keeping `vspeed`. |
| `stop_if_frozen` | A5 (spook house) Step: with `frozen` the rest of the Step list is skipped and nothing is written. |
| `ladder` | A1 Step action 2 (little-trap, rainbow, rori, fairy-tale), listed instead of `gravity`: `ladderud` clears; Up or Down (`[ladder] entry`) held, off a ladder and overlapping a `ladder` instance: on the ladder with `djump`; with no ladder overlapping any more: off; otherwise Up moves `y` up by the run speed, Down down, each with gravity and `vspeed` 0 and `ladderud` 1; Jump held leaves with `ladderjump`. Off the ladder gravity returns to `numbers.gravity`. The keys are direct reads. |
| `ladder_sprites` | End Step: on a ladder the `back` sprite (`image_speed` 0.1), moving on it the `ladder_up_down` sprite (0.2). |
| `ladder_jump_reset` | End Step: `ladderjump` clears (rori has only this). |
| `ice_temp_hspeed` | Showdown Step action 3: with a slip block within `ice.temp_probe` px below, `tempHspeed` stores `hspeed` when it is 0 or the signs match and `hspeed` is not 0; otherwise `hspeed` gains it and it is multiplied by `ice.temp_decay`, zero below `ice.temp_zero_below`. Off ice it is 0. |
| `vine_probe` | C Step 3-4: `onVineL` / `onVineR` (`KidStateExt`): the nearest left or right vine is at most 1 px away and the kid is free 1 px in the gravity direction. Computed before the jump code; `vines` reads them. |
| `suicide_press` | C Step 10: the Suicide key pressed (not held) runs `kill`, when not frozen. |
| `slope_jump` | End of `playerJump` (rainbow, rori, fairy-tale; kid/A1.md): on a Jump press with a `slope` or `slope_mirror` within `[slope] jump_probe` px below, `vspeed` is `-jumps[0]` and `djump` is set (it can override a double jump of the same press). List it right after `jump`. |
| `downhill_snap` | End Step, `[slope] downhill`: `por` (vspeed >= 0, free below, on a slope last step and a slope within 12 px: down to contact by at most 12, running or idle sprite; then remembers a slope 1 px below), `rainbow` (a slope, then a mirrored slope, within 5 px below: down to contact by at most 10), `fairy_tale` (no block 1 px below and a slope 3 px below at `xprevious`: down by at most 10). |
| `flip_swap` | Group A Step (`flip.model = "swap_object"`; A1 Step action 5, A2 action 4, A3 action 3): when `global.grav` no longer matches this kid object, the facing goes to `global.image_xscale`, the other kid object (engine role `kid_flipped`, or `kid`) is created at `y - 4` (to the flipped one) or `y + 4` and this instance is destroyed. The new kid starts from its Create. |
| `flip_grav` | `scrFlipGrav` (`flip.model = "grav_sign"`): `global.grav` negates, `djump` 1, `vspeed` 0, gravity and `image_yscale` take the new sign, `y` moves 4 times it. Also `kid::api::flip_grav`. |
| `killer_check` | `playerMisc` 1 (B Step) or the A3 End Step: the kid dies when it touches a `killer` instance. In the Step it runs before motion, so it sees the position the last tick ended at. With `death.killer_check_menu_rooms` it does nothing in Boot, Title and Menu rooms and moves the kid to `xstart`, `ystart` in a Select room. |
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
| `block_slope_step` | `block_standard` with the climb of `[slope] block_step` in the horizontal part: `free_spot` (rainbow, kid/A1.md) moves the kid `step` sideways and up after the contact when the matching slope is `step` px ahead and the spot is free; `replace` (rori, fairy-tale) does that instead of the contact when the shifted kid overlaps the matching slope, with no free-spot test. Rows: `block`, `slope`, `slope_mirror`. OPEN: rori's and fairy-tale's per-slope events differ in the right-hit contact angle (rori 180, fairy-tale 0), not modelled (#312). |
| `slope_por_rise` | Poronkusema `nanameblock` (kid/A1.md): the vertical block part, a rise found by testing 0 to 9 px up with the kid shifted by `hspeed` (0 when still in a slope at 9 or when the spot overlaps a block), then `x` moves `step` toward `hspeed` (even with rise 0), `y` up by the rise, `hspeed` 0, then the diagonal check. |
| `slope_yane` | Eat-the-new-year `yaneH` (`slope`) and `yaneM` (`slope_mirror`, kid/A4.md): with `place_free(x + hspeed, y)` false, `hspeed >= 0` (M: `<= 0`, OPEN: the spec gives only H) and `place_free(x, y + vspeed)` false, `step` up and sideways; `hspeed` 0; then the vertical and diagonal block parts. |
| `moving_block_por` | Poronkusema, Collision with movingBlock (class `moving_block`, 64 x 64, not solid; kid/A1.md), in order: ride from above (`y <= other.y + vspeed + 2`, `vspeed > other.vspeed`, `other.y - yprevious >= 8`: `y = other.y - 9`, `vspeed` the block's, `onPlatform` and `djump` set); bump from below (`yprevious - other.y >= height + 10`: `y = other.y + height + 12`, `vspeed` the block's if it is 0 or more, else 0); else pushed out sideways to `other.x - 6` or `other.x + width + 5` by which half the kid is in. OPEN: the spec gives the numbers but not each test; the relative-motion test, the "within vspeed + 2" reading and the side choice are guesses (#314). The game places no moving block. |
| `platform_standard` | kid/A1.md "Collision with platform": lands when `y - vspeed / 2 <= platform.y`; one-way from below. A reversed `yflag` kid lands when `y - vspeed / 2 >= platform.y + reversed_above` and is put at `platform.y + reversed_snap`. |
| `kill` | The other instance kills the kid (`[death]`). |

## Sections

Every key below is optional. Defaults are the `A1` preset's.

`[numbers]`: `jumps` (list, ground jump then air jumps, `[8.5, 7.0]`), `gravity` (0.4), `run_speed` (3), `max_vspeed` (9), `max_fall_speed` (8, read only by `shift_then_clamp`), `release_mult` (0.45), `sprite_threshold` (0.05), `image_speed_idle` (0.2), `image_speed_run` (0.5), `air_wall_probe` (0; family B 2: `air_sprite` shows the fall sprite only with no `slope_wall` that many pixels in the gravity direction).

`[input]`: `walk` = `direct_latch` (`keyboard_check_direct`, sees a tap inside one tick) or `sampled`; `vine_keys` = `edges` (this tick's press) or `latched_flags` (the `key_flags` flags).

`[jump]`: `ground_classes` (class names), `ground_also` (`platform_flag`, `ladder_jump`), `ground_sets_djump`, `air` (`djump`, `cur_jumps` or `djump_then_tjump`: showdown's third jump `jumps[2]`, granted by Create, spent by a landing on a block and cut twice by `release` with `release_twice_while_tjump`; the double jump granting it in room028 is game code, OPEN), `water_air` (`none`, `water2_without_flag`, `water_unlimited` or `water3_keeps`: C, water 2 and 3 allow the air jump and water 3 keeps `djump`), `max_jumps` (family B, 2), `inf_jump` (B cheat flag, false), `release_twice_while_tjump` (false), `blocked_by_pause_press` (a tick with a Pause press does nothing), `cancel` (`edge`: the `release` action cuts the jump; `poll_alarm`: `release` does nothing and the `cancel_poll` alarm action cuts it).

`[fall_cap]`: `rule` = `clamp_abs`, `clamp_against_gravity` (B) or `clamp_down_only` (spook house): clamp only when `vspeed * yflag` exceeds `max_vspeed`; or `shift_then_clamp` (A4): beyond `max_fall_speed` the kid's `y` first moves back by `abs(vspeed) - max_fall_speed` against the motion, then both directions clamp to `max_vspeed`.

`[gravity]`: `reapply` = `step`, or `end_step_slope` (Create leaves gravity 0; the `slope_b` action, WP5b.4, sets it; `create_only` is WP5c), `block_landing_zeroes`.

`[platform]`: `hold_probe` (4), `rule` = `half_vspeed`, `snap` (9), `take` = `vspeed` or `delta_y` (the platform's `y - yprevious`, family B; `yspeed_flipped`, C: the A1 rule for normal gravity; the flipped kid lands when `y - vspeed / 2 >= platform.y + height - 1`, is put at `platform.y + height + 8` and takes the platform's `yspeed` if that is 0 or less), `reversed_above` (family B 15) and `reversed_snap` (24).

`[vine]`: `walk_ignores_vines` (A3: `walk` has no vine rule), `model` = `distance_asym`, `distance_sym_early` (C: both sides grab at a distance of at most 1, `vine_probe` before the jump code, `vines` after it) or `none` (`distance_sym_early` is WP5c), `left_below` (2), `right_equal` (1), `slide` (2), `kick` (`[15, -9]`: horizontal, vertical), `drop` (3). `probe_down_fixed` (false; the vine tests look at `y + 1` whatever the gravity, Kotaro player2).

`[ice]`: `model` = `none` or `slip` (A1, A2, B: `walk` adds `slip * h` to `hspeed` while below the run speed when a `slip` block is within `probe` px (4) below, reading the slip at `read` px (10), no clamp, and standing on ice subtracts `slip * sign(hspeed)` with overshoot; off ice the speed is set outright), `clamp` (C: slip clamped to the run speed), `temp_probe` (2), `temp_decay` (0.95), `temp_zero_below` (0.2) for `ice_temp_hspeed`. `[ladder]`: `entry` = `up_or_down` or `up_only`. `[slope]`: `block_step` (`none`, `free_spot`, `replace`), `step` (3), `downhill` (`none`, `por`, `rainbow`, `fairy_tale`), `jump_probe` (0 = off, 2), `air_sprite_probe` (0 = off, 2: `air_sprite` shows no fall sprite with a slope that close below), `keep_hspeed` (`walk` keeps `hspeed` with a `slope` 1 px below and Left or no key, or a `slope_mirror` and Right or no key, kid/A4.md). `slope` rises to the right, `slope_mirror` to the left; a game's slope kinds carry only those classes, so the kid's `slope` and `slope_mirror` rows run alone like the original's child-object events (add `block` too only if the block row should also fire). Tier 3 `slope_c` (family C) is not built (#295). `[water]`: empty until its work package. `[flip]`: `model` (`none`, `swap_object`, `yflag`, `grav_sign`: family C reads `global.grav`, signs gravity and `image_yscale` at Create, has no key swap, and its flipped kid reads `hspeed` 0 as right in `block_standard`). With `yflag` (family B) a reversed kid (`yflag` -1): `walk` swaps Left and Right; `vines` slide at `slide * yflag`, kick at `kick[1] * yflag` and take the other key as the away key; `air_sprite` swaps the jump and fall sprites; `shoot` places the bullet at `y - offset[1] * yflag`; `create_defaults` reads `global.reverse` (y - 4, `curJumps` 1). `swap_object` (group A): the flipped kid is its own object (class `kid_flipped`); its Create sets gravity and `image_yscale` negative, and it reads like `yflag` -1 in `gravity`, `jump`, `release`, `vines`, `block_standard` and the platform rule (`platform.reversed_above`, `reversed_snap`), with no key swap. The `flip_swap` action switches objects. Kotaro's player2 quirks (kid/A2.md) are on in the A2 preset: `[vine] probe_down_fixed` (the vine tests look at `y + 1` whatever the gravity) and `[flip] solidblock_no_refresh` (a landing on a `solidblock`-only object does not restore `djump`; `block` still does). OPEN: other A2 games (animus, fed-up, easy-miku-2) may differ per game (animus snaps at +25, fed-up swaps its sprite thresholds: research#42).

`[sounds]`: sound names by role, resolved against the package's sounds (an unknown name is a load error; a role left out plays nothing): `jump` (a ground jump), `double_jump` (an air jump, also the third jump), `shoot` (a bullet appeared), `vine_kick` (a kick off a vine, not a drop) and `death` (played by the death step `sound`). They play through `sound::play`, so `sound_isplaying` and the state hash see them. Family B's death sound is a delayed `objDeathSound` with a music fade (kid/B.md), not this immediate sound: leave `death` out for B until that is built.

`[death]`: `order`, the steps in the order they run: `sound` (plays `[sounds] death`), `blood_emitter` (spawned with a `flipped` param), `destroy_kid`, `destroy_all_kids` (every kid instance, family B), `game_over` (it places itself at view 0 plus its own offset params), `deaths_plus_one` (checked), `save_counters` (death-only save). `select_room_reset`: in a Select room a kill requests the same room, destroys the kid and does nothing else. `stop_music`: not built (`true` is `Unsupported`). `first_kid`: the blood emitter, `flipped` param and GAMEOVER use the first kid's position and gravity (family B with several kids). Nothing happens while `muteki` is set.

`[bullet]`: `max` (4), `speed` (16), `offset` (`[0, 0]`; family B `[2, 2]`: the bullet appears at `x + offset[0] * facing`, `y - offset[1] * yflag`).

`[save]`: `position` (`xy_base100_bytes`, `xy_base256_bytes`, `xy_text2_dec`, `xy_previous`, `fixed_point`, `floor_nudge`), `fixed` (`[x, y]` for `fixed_point`), `first_kid` (a save stores the first kid whichever kid touched the save point; family B).

`[misc]`: `fade_in`, `fade_step` (0.1), `idle_needs_block` (spook house: `walk` shows the idle sprite only with a ground-class instance 1 px below).

`[sprites]`: sprite names from the package's sprite table, by role: `idle`, `running`, `jump`, `fall`, `sliding`, `back`, `ladder_up_down`. A role left out leaves the sprite alone.

## Converter note (CR8)

The converter writes `physics.toml` in this form for every game (plan 8.4, CR8). Per game:

1. Set `preset` to the kid's family from the research spec and write only what differs from the preset. rely itself needs `preset = "A1"` and `[sprites]`.
2. Write the Step list from the spec's Step actions in order, using the names above. Where the spec's first Step action is the ladder (rely), list `gravity` until the ladder action exists (WP5c, #232); `ad_nudge` and `flip_swap` are dormant in rely and left out.
3. Collapse the kid's `Collision with ...` events into `events.collisions` rows: block and solidblock to `block_standard`, platform to `platform_standard`, the killer parent to `kill`. A game that names a parent object maps it to the class that object has (`counts_as`).
4. Key events become `events.keys` rows (`Key Q pressed` is `suicide` on `press` running `kill`), Alarm 0 an `events.alarms` row.
5. Use sprite names from the package, never the original's ids.
6. A spec rule with no action here yet is not guessed: the converter lists it as a gap for the work package that owns the family (5.3), and the game's trace check stays red until it lands.

## Legacy drafts

The converter first wrote a draft form (`rules = "classic"`, `jumps`, `gravity = 0.4`, `step_sets`, `air_jump_restore`). The loader does not read its values: it maps the file to the family preset `rules` names (`classic` is `A1`) and warns, listing the ignored keys. A `rules` word that names no family is an error, and a warning for converter output (`--trace-mode`), which falls back to `A1`. The draft's `mask = { sprite = ... }` still gives the kid its mask until `[defaults.kid]` does (CR8).
