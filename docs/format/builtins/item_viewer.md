# `item_viewer`

Shows a collected boss reward or item (KAE bossItemViewer, JUN itemview5, LR, RBF). Source: `crates/sim/src/builtins/item_viewer.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `kind` | `boss`, `item` | required | Which flag array. |
| `index` | whole, 0 to 15 | required | The flag. |
| `sprite` | | | The loader's. |

Create and every Step: visible while the flag is set, invisible otherwise.
