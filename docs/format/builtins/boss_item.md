# `boss_item`

The reward of a boss (KOT bossitem1, MIKU, XIA objBossItem). Source: `crates/sim/src/builtins/boss_item.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `boss_index` | whole, 0 to 15 | required | The boss flag it sets. |
| `watched_object` | kind | none | The boss kind. With none, it appears on its first Step. |
| `sprite`, `sound` | | | The loader's and the client's. |

Create: invisible. Each Step, in order: visible when no instance of `watched_object` exists; destroy when `global.boss[boss_index]` is already set. A kid touch, only while visible, sets the flag and destroys it.
