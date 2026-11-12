# `vine`

Wall-jump surface (WalljumpL, WalljumpR). **No events**: all the logic is in the kid, which reads the `vine_l` and `vine_r` classes. Source: `crates/sim/src/builtins/vine.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `side` | `left`, `right` | required | Which class the kind takes (`vine_l`, `vine_r`); the loader sets it (`crates/format/src/objects.rs`). On a placed instance it gives a derived kind (`vine[side=left]`, v0.md Params that pick a class). |

The mask strip (`mask_columns`) is also applied at load.
