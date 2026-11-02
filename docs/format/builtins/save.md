# `save`

A save point (mechanics `save`; kid specs "Saving"). The save is `save::save_now` with the game's position model, called from the point's own event, after the kid's block correction. Source: `crates/sim/src/builtins/save.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `on_touch` | `never`, `press`, `shoot_press`, `skip_held`, `always` | `press` | What the touching kid must do. `press`: S or Z (shoot or skip action) pressed this tick (rely, wanna, lemon, xia). `shoot_press`: Z only (little-trap). `skip_held`: S held while overlapping the kid (kaenbyou, eat-the-lemon). `always`: touch is enough (poronkusema, LR object738). |
| `on_shot` | bool | false | A bullet hit saves, from the live kid (most games). |
| `once` | bool | false | Destroyed after saving (autosave). |
| `cooldown` | real | 0 | `saveTimer` is set to it on a save and counts down every Step; a save needs it below 0. 0 never gates (rely sets the timer but never checks it). 30 elsewhere. |
| `min_difficulty_removed` | real (whole) | none | Destroyed in Create when the difficulty is above it (savePoint 1, Medium 0, VHard 2, Imp 3). |
| `requires_gravity` | `any`, `normal`, `flipped` | `any` | The kid must match. Flipped means `global.grav` or the flipped kid kind (FT savePoint2, xia). |
| `requires_kid_depth0` | bool | false | The kid's depth must be 0 (machinary). |
| `saved_position` | `xy_base100`, `xy_base256`, `xy_text2dec`, `xy_previous`, `fixed_point`, `floor_nudge` | `xy_base100` | The game's save format (`save::SavedPosition`; sim plan 4.4). |
| `fixed_x`, `fixed_y` | real | none | The point for `fixed_point`; both or neither. |
| `self_destroy_alarm` | real | none | Alarm 0 at Create destroys the point after this many ticks (LR object799). |
| `blink` | bool | true | A save shows frame 1 with `image_speed` 0.017; Animation End goes back to frame 0 and stops. |

Handlers are declared from the kind's params. The Step, Collision and Key held handlers all run the same save code: games that save in Step or End Step (WAN, LEM, SPK) are OPEN until a trace shows a difference. The save sound and the particles (RBF) are the client's and the sound model's.
