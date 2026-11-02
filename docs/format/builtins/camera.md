# `camera`

A camera object writes view 0 (mechanics `camera`). It targets the first kid of either kind. Its Step placement gives the 1-tick lag the spec infers (it reads the kid before this tick's motion; Confirmed (recorder) little-runner `tr-room-11.csv`). Source: `crates/sim/src/builtins/camera.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `screen_snap`, `smooth_follow`, `end_step_clamp` | `screen_snap` | See below. |
| `screen_w`, `screen_h` | real | 800, 608 | `screen_snap` screen size (poronkusema 960 x 540). |
| `y_offset` | real | 0 | Added to the snapped y (machinary 4). |
| `on_step`, `on_room_start`, `on_room_end` | bool | true, false, false | `screen_snap` events (A2 and rely also snap in Room Start; A2 in Room End). |
| `h_div`, `v_div` | real | 16, 12 | `smooth_follow`: speed is distance over this. |
| `border_x`, `border_y` | real | 400, 304 | `smooth_follow`: view 0 borders. |
| `tolerance` | real | 2 | `smooth_follow`: no speed within this distance (inclusive). |

- `screen_snap`: `view_xview = floor(kid x / screen_w) * screen_w`, `view_yview = floor(kid y / screen_h) * screen_h + y_offset`. No kid: the view stays.
- `smooth_follow` (kaenbyou objSmoothView): Create makes the camera view 0's followed object and moves it to the kid. Each Step its hspeed is the x distance over `h_div` and its vspeed the y distance over `v_div`, each 0 within `tolerance`, both 0 with no kid. Automatic motion and the runner view follow (`view::follow_update`) do the rest.
- `end_step_clamp` (xia objCamera): screen snap with the view's own size after clamping the kid's point to 0 .. size - 1, in Create and End Step.

OPEN: xia's `leaveRoom` switch (no clamp).
