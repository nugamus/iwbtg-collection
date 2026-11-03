# `fake_block`

Looks like a block but is not solid, or vanishes. Source: `crates/sim/src/builtins/fake_block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `solid`, `visible` | bool | kind data | Applied in Create when given. |
| `vanish_on` | `touch`, `approach`, `group_fade`, `never`, `tile_delete` | `approach` | When it goes (below). |
| `approach_distance` | real | 2 | `approach` distance. |
| `blood_solid_when_no_kid` | bool | false | While no kid exists it is solid, so blood sticks (KOT niseblock, blockVis). |
| `stops_bullets` | bool | false | A kid bullet that touches it is destroyed. |
| `reverses_platforms` | bool | false | The loader gives it the `fake_block` class; `moving_platform` then handles the collision (`marker_reverse`). |
| `sound` | sound | none | The client's. |

**Loading.** Every param loads on an object file or a placed instance. On an object file `solid` and `visible` are the kind's own flags, not params; as instance params they apply in Create. Tests `wp6b2_params_load` and `wp6b2_params_are_checked` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.

- `touch`: a kid touching it destroys it (KOT blockVis).
- `approach`: each Step, a kid within `approach_distance` destroys it (WAN blockFake, LR blockNotInvis, SHO blockKieru01).
- `group_fade` (WAN oHiddenWall): a kid touch makes every instance of the kind fade: each Step `image_alpha -= 0.05`, destroyed at 0 or below.
- `tile_delete` (spook house oPatFakeBlock) is refused at load: it also makes 10 particles at random offsets and the draw order is not in the spec (OPEN, nugamus/iwbtg-collection-research#50).

OPEN: the WAN blockFake test that runs only while player2 does not exist.
