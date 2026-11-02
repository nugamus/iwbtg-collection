# `time_limit`

A countdown that does something when it runs out (KOT, LT, MIKU, ANI, RBF). Two objects in the original, so two roles. Source: `crates/sim/src/builtins/time_limit.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `role` | `timer`, `start_zone` | `timer` | The countdown, or the zone that starts it. |
| `hours`, `minutes`, `seconds` | whole number, 0 or more | 0 | Time left (timer). |
| `on_expire` | `kill`, `shake`, `nothing`, `goto_room` | `kill` | What expiry does (timer). |
| `room` | room | none | Where `goto_room` goes; required for it. |
| `warp_x`, `warp_y` | real | 0 | Where the kid is placed there; (0, 0) destroys the kid, like a `warp`. |
| `shake` | kind | none | The kind `shake` spawns at the timer. |

**Timer.** It starts idle. Alarm 0 takes one second off when a kid exists, then always re-arms for `room_speed` ticks. Step: seconds below 0 borrow from minutes, then hours; seconds below 0 with minutes and hours at 0 is the expiry, one alarm period after the display reads 0:00:00. Every expiry destroys the timer. Confirmed by a recorder trace (MIKU roommiku: 0:00 on frame 8803, timer gone on 8853).

**Start zone.** A kid touching it sets every timer's alarm 0 to `room_speed` and destroys the zone.

The H:MM:SS display is the client's.

OPEN (#356): the original kills player2 instead of player when `global.grav` is set; the sim kills the first kid whichever kind it is.

**Loading.** Tests in `crates/sim/tests/wp6c_misc_objects.rs`.
