# `secret_item`

Source: `crates/sim/src/builtins/secret_item.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `item_index` | whole, 0 to 15 | required | A bit of `MechGlobals::items`. |
| `sprite`, `sound` | | | The loader's; the sound is the client's (KAE plays one, RBF does not). |

A kid touch sets `global.item[item_index]` and destroys it. Create removes it when the item is already taken.
