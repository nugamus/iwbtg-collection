# `kid_param_zone`

While a kid overlaps it, it writes kid numbers every tick (`kid::api::apply_param_zone`). The values persist until another zone, or a new kid's Create, changes them (Inferred). Source: `crates/sim/src/builtins/kid_param_zone.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `run_speed`, `max_vspeed`, `max_fall_speed`, `jump`, `jump2`, `jump3` | real | none | The kid numbers it sets. At least one is required. |
| `target` | `toucher`, `player` | `toucher` | `player` acts on the first kid even when the second kid touched it (rainbow-feather object867 and object869). |

Examples: rainbow-feather object867 sets `run_speed` 3, `max_fall_speed` 1, `max_vspeed` 30, `jump` 16, `jump2` 13; showdown speed3 sets `run_speed` 3, `jump` 8.5, `jump2` 7, `jump3` 6, and speedup01 sets `run_speed` 5.

OPEN: when exactly the constants change in the original (TR-MECH-07, not run).
