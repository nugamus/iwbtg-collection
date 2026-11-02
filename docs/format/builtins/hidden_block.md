# `hidden_block`

A solid block that is invisible until a kid comes near (KOT blockNotVis, WAN blockInvisible, XIA objBlockInvis). Source: `crates/sim/src/builtins/hidden_block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `reveal_distance` | real | 2 | XIA uses 1. |
| `stops_bullets` | bool | false | A kid bullet that touches it is destroyed (WAN). |
| `destroy_when_no_kid` | bool | true | Destroys itself when no kid exists. |
| `sound` | sound | none | Played on reveal; the client's. |

**Loading.** Every param loads on an object file or a placed instance. Tests `wp6b2_params_load` and `wp6b2_params_are_checked` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.

Create makes it invisible. While invisible, each Step: `distance_to_object` to either kid below `reveal_distance` makes it visible; no kid at all destroys it (when `destroy_when_no_kid`). Solidity and the mask are kind data (the loader).

OPEN: XIA objBlockInvis is also destroyed when shot, or touched by blood while invisible (KID-C); TR-MECH-09 (the reveal tick, and whether the kid stops in the same tick).
