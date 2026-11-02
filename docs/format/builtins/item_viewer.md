# `item_viewer`

Shows a collected boss reward or item (KAE bossItemViewer, JUN itemview5, LR, RBF). Source: `crates/sim/src/builtins/item_viewer.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `kind` | `boss`, `item` | required | Which flag array. |
| `index` | whole, 0 to 15 | required | The flag. |
| `sprite` | | | The loader's. |

Create and every Step: visible while the flag is set, invisible otherwise.

**Loading.** `kind` is an object-file param only: on a placed instance `kind` is the key that names the object, so a placed viewer cannot override it. Tests `wp6b4_params_*` and `an_item_viewer_kind_is_a_preset_param...` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.
