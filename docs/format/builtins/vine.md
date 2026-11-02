# `vine`

Wall-jump surface (WalljumpL, WalljumpR). **No events**: all the logic is in the kid, which reads the `vine_l` and `vine_r` classes. Source: `crates/sim/src/builtins/vine.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `side` | `L`, `R` | required | Which class the kind takes; the loader sets it. |

The mask strip (`mask_columns`) is also applied at load.
