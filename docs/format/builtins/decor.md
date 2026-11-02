# `decor`

Sprite-only objects with no behavior (SHO, LR, SPK, FT and others: about 826 instances in 15 games). **No events, no params, no state**: the sprite, depth and animation are kind data. Source: `crates/sim/src/builtins/decor.rs`.

An object that only sets its animation speed in Create is still a `decor` kind: put the speed in the kind's data.
