# `save`

A save point (mechanics `save`; kid specs "Saving"). The save is `save::save_now` with the game's position model, called from the point's own event, after the kid's block correction. Source: `crates/sim/src/builtins/save.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `on_touch` | `never`, `press`, `shoot_press`, `skip_held`, `always` | `press` | What the touching kid must do. `press`: S or Z (shoot or skip action) pressed this tick (rely, wanna, xia). `shoot_press`: Z only (little-trap, eat-the-lemon; eat-the-lemon saves in End Step with `on_shot` and `cooldown = 30`, no Key S event, kid/B.md lemon difference 6). `skip_held`: S held while overlapping the kid (kaenbyou). `always`: touch is enough (poronkusema, LR object738). |
| `on_shot` | bool | false | A bullet hit saves, from the live kid (most games). |
| `once` | bool | false | Destroyed after saving (autosave). |
| `cooldown` | real | 0 | `saveTimer` is set to it on a save and counts down every Step; a save needs it below 0. 0 never gates (rely sets the timer but never checks it). 30 elsewhere. With `on_touch = "skip_held"` (kaenbyou) the timer starts at -1, Step does not count it, and it counts down by 1 only on ticks S is held and only while above 0; a save needs it at most 0, then sets it to `cooldown`. |
| `min_difficulty_removed` | real (whole) | none | Destroyed in Create when the difficulty is above it (savePoint 1, Medium 0, VHard 2, Imp 3). |
| `requires_gravity` | `any`, `normal`, `flipped` | `any` | The kid must match. Flipped means `global.grav` or the flipped kid kind (FT savePoint2, xia). |
| `requires_kid_depth0` | bool | false | The kid's depth must be 0 (machinary). |
| `saved_position` | `xy_base100`, `xy_base256`, `xy_text2dec`, `xy_previous`, `fixed_point`, `floor_nudge` | `xy_base100` | The game's save format (`save::SavedPosition`; sim plan 4.4). |
| `fixed_x`, `fixed_y` | real | none | The point for `fixed_point`; both or neither. |
| `self_destroy_alarm` | real | none | Alarm 0 at Create destroys the point after this many ticks (LR object799). |
| `alarm_save` | real | none | The autosave (eat-the-lemon `autosave`, xia `objautosave`; kid/B.md lemon item 7, kid/C.md objautosave): Create arms Alarm 0 for this many ticks (1 in both games), and Alarm 0 saves from the first kid in instance order (the lowest id; traces/eat-the-lemon/famb-multikid-restart-b.csv f25), if a kid exists. The point stays: `alarm[0]` reads 0 on the save tick and -1 after. Use with `on_touch = "never"` and `blink = false`. Not with `self_destroy_alarm` (both use Alarm 0). |
| `blink` | bool | true | A save shows frame 1 with `image_speed` 0.017; Animation End goes back to frame 0 and stops. |

**What a save stores as `flipped`.** `SaveSlot.flipped` is the game's own flip state. Games that flip by kind or by a kid flag store whether the kid was flipped; the load spawns the flipped kid kind when the game has one and puts the kid `restart.flipped_load_dy` lower. Games that flip by the `yflag` model (`[flip] model = "yflag"`, family B) store `global.reverse` there (kid/B.md Restart); the load writes it back to `global.reverse`, and never takes the flipped-kind and offset path, because family B has one kid kind. The two meanings never meet in one game, so there is one field. (`save::flip_by_kind` is the test; #267.)

With `blink` on, Create sets `image_speed` to 0 first, so an idle save rests on frame 0 (kid/B.md Saving; traces/kaenbyou/famb-save-restart-b.csv). OPEN: kaenbyou counts `saveTimer` only while S is held (#410).

Handlers are declared from the kind's params. The Step, Collision and Key held handlers all run the same save code: games that save in Step or End Step (WAN, LEM, SPK) are OPEN until a trace shows a difference. The save sound and the particles (RBF) are the client's and the sound model's.
