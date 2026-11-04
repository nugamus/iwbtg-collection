# `hidden_block`

A solid block that is invisible until a kid comes near (KOT blockNotVis, WAN blockInvisible, XIA objBlockInvis). Source: `crates/sim/src/builtins/hidden_block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `reveal_distance` | real | 2 | XIA uses 1. |
| `stops_bullets` | bool | false | A kid bullet that touches it is destroyed (WAN). |
| `destroy_when_no_kid` | bool | true | Destroys itself when no kid exists. |
| `destroyed_by_bullet` | bool | false | A kid bullet touching it destroys the block (XIA objBlockInvis). The bullet dies by the kid bullet's own block rule. |
| `destroyed_by_blood` | bool | false | Blood touching it while it is still invisible destroys the block (XIA). |
| `sound` | sound | none | Played on reveal; the client's. |

**Loading.** Every param loads on an object file or a placed instance. Tests `wp6b2_params_load` and `wp6b2_params_are_checked` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.

Create makes it invisible. While invisible, each Step: `distance_to_object` to either kid below `reveal_distance` makes it visible; no kid at all destroys it (when `destroy_when_no_kid`). Solidity and the mask are kind data (the loader).

XIA objBlockInvis (kid/C.md): set `destroyed_by_bullet` and `destroyed_by_blood`.

OPEN (nugamus/iwbtg-collection-research#48): TR-MECH-09 (the reveal tick, and whether the kid stops in the same tick).
