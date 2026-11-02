# `boss_gate`

A solid block that destroys itself once its boss flag is set (KOT bossblock1, KAE bossBlock, LR blockFlagBoss). Source: `crates/sim/src/builtins/boss_gate.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `boss_index` | whole, 0 to 15 | required | A bit of `MechGlobals::bosses` (the save slot holds 16 flags). |
| `sprite` | | | The loader's. |

Create and every Step: when `global.boss[boss_index]` is set, destroy. Solidity is kind data.

**Loading.** Params load on an object file or a placed instance; a missing required param, an index above 15 and a wrong type are load errors (tests `wp6b4_params_*` in `crates/format/tests/wp7a_load.rs`). Converter request CR17.
