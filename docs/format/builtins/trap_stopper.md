# `trap_stopper`

Source: `crates/sim/src/builtins/trap_stopper.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `stop`, `destroy` | `stop` | `stop` (rainbow-feather stopneedleblock): an invisible 16 x 16 block with no events; a `trap` with `stop_on_stopper` reacts to touching it (the loader gives the kind the `trap_stopper` class). `destroy` (help-the-fairy-tale object817): each Step it destroys every `trap` instance it overlaps. |
| `size` | real | none | The mask size: the loader's. |

OPEN (nugamus/iwbtg-collection-research#47): object817 destroys its listed trap objects in a collision event (the collision phase); this runs in Step, and it destroys every `trap` base instance rather than the listed 18 kinds.
