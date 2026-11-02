# `moving_platform`

Non-solid; moves by hand in Step plus the runner's automatic motion. Upward motion goes through `yspeed` in Step, downward through `vspeed` by automatic motion (step 5 below). "Free" means `place_free` (solids only). Source: `crates/sim/src/builtins/moving_platform.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `hspeed`, `vspeed` | real | 0 | Start speeds, applied in Create. |
| `bounce` | bool | true | `false` skips the reversals (objMovingPlatformNoBounce). |
| `start_on_touch` | bool | false | Still until a kid touches it, then takes the start speeds. |
| `marker_reverse` | `both`, `vertical`, `none` | `both` | What a fake block marker touch does: reverse every direction, only the vertical, or nothing (animus: `vertical`). The platform handles Collision with the `fake_block` class (unless `none`) and calls `moving_platform::reverse_on_marker`. |
| `carry_rule` | `yuuutu`, `showdown_split`, `xia_grav`, `b_lines` | `yuuutu` | How a kid is carried. |

## Step (`yuuutu`, deride-kotaro, A1 games, A4)

1. If `(x + hspeed, y)` is not free, `hspeed = -hspeed`.
2. If `(x, y + vspeed + yspeed)` is not free: if `vspeed != 0` then `yspeed = -vspeed`, `vspeed = 0`; else `vspeed = -yspeed`, `yspeed = 0`.
3. `y += yspeed`.
4. A kid touching the platform mask moved 2 px up gets `y += vspeed + yspeed`, then `x += hspeed` if that spot is free for him. The flipped kid is probed 17 px below.
5. `vspeed < 0` moves into `yspeed` (`vspeed = 0`); `yspeed > 0` moves into `vspeed`.

`showdown_split` (showdown, kid/A3.md): the same Step, but the carry is two parts: while `hspeed != 0` a kid 2 px above gets `y += vspeed + yspeed` and `x += hspeed` if free; separately, while `vspeed != 0` he gets `y += vspeed + yspeed`, then `y -= vspeed` again if that spot is free.

`xia_grav` (keep-celebrating-for-xia): runs only while `hspeed`, `vspeed` or `yspeed` is not 0; carries a kid found at `y - 2 * grav` before adding `yspeed`. OPEN: the sign of `grav` is taken from the kid's gravity flag.

`b_lines` (kaenbyou, eat-the-lemon; kid/B.md): everything runs in End Step instead of Step. The same reversals and `y += yspeed`; then two `collision_line` probes from `x - 2` to `x + 33` at `y + 8 - 25 * yflag` and `y + 8 - 10 * yflag` (the first kid's `yflag`, 1 when it has none). When both lines touch a kid, the first kid moves by `hspeed` if that spot is free of solids and by `vspeed + yspeed` if that spot is free; then step 5. OPEN: the probes use the bounding box (`prec` false; the spec does not say), the order of the two moves (x, then y), and "first kid" is the lowest id.
