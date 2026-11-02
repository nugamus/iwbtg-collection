# `spring`

Showdown JumpBlockD and JumpBlockS. On touch the kid's motion is set to direction 90 at `speed`: `hspeed` 0 and `vspeed` minus the speed. Then the air jump is set (D). Destroyed outside the room. Source: `crates/sim/src/builtins/spring.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `speed` | real | 10 | Launch speed (upward). |
| `djump` | `keep`, `set`, `clear` | `keep` | `set` refreshes the air jump (D); `clear` (S) is refused at load. |

OPEN, not done: clearing `djump` (JumpBlockS) needs a setter in `kid::api` (kid lane, #309).
