# `breakable_block`

A solid block child that breaks. Source: `crates/sim/src/builtins/breakable_block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `break_on` | `shot`, `touch_delay` | `shot` | `shot` (rainbow-feather break): a kid bullet touching it destroys it. `touch_delay` (showdown object831, object832, object833): the first kid touch arms Alarm 0; later touches change nothing. |
| `stages` | whole, at least 1 | 1 | `touch_delay`: Alarm 0 runs before the block is destroyed. Each run before the last shows the next frame (`image_index` = the stage). object833 has 1, object832 2, object831 3. |
| `stage_ticks` | whole, at least 1 | 50 | Ticks per stage. |
| `sound` | sound | none | The break sound: the client's. |
