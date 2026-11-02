# `slope`

Solid with a precise mask. No events: the climb and snap rules are the kid's (the `[slope]` model of the physics profile; KID-A1 POR and RBF, KID-A4 NY yaneH and yaneM, KID-B obj_wall). Source: `crates/sim/src/builtins/slope.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `shape` | `rise_right`, `rise_left`, `precise_sprite` | kind data | Picks the class and mask. **OPEN (#324):** the loader checks the word but does not apply it yet; a `slope` kind has class `slope` and its sprite's mask. |
| `scale_x` | real | 1 | Mirrors it. **OPEN (#324):** checked as a number, not applied yet. |

**Loading.** Both params load on an object file or a placed instance and are checked (`params::check`; tests `wp6b3_params_*` in `crates/format/tests/wp7a_load.rs`). Converter request CR17.
