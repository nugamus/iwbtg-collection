# `platform`

The one-way platform (mechanics `platform`). No events: it is not solid, usually invisible, and the kid's own "Collision with platform" event does the work. Source: `crates/sim/src/builtins/platform.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `width` | real | 32 | Mask width, applied at load. Must be positive. |
| `parent_kind` | `block`, `solidblock` | `block` | Classes `platform \| block` or `platform \| solidblock` (RORI, FT), applied at load. The kid runs only his platform event for a platform; his block or solidblock event runs for it only when he has no platform event (sim register C8). |
