# `platform`

The one-way platform (mechanics `platform`). No events: it is not solid, usually invisible, and the kid's own "Collision with platform" event does the work. Source: `crates/sim/src/builtins/platform.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `width` | real | 32 | Mask width, applied at load. Must be positive. |
| `parent_kind` | `block`, `solidblock` | `block` | Classes `platform \| block` or `platform \| solidblock` (RORI, FT: the kid's solidblock event fires), applied at load. |
