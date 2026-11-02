# `blood`

One blood drop. Source: `crates/sim/src/builtins/blood.rs`.

- **Create** draws from the gameplay RNG in the recorded order (Confirmed (recorder), rely `num-rng-b.csv`): the frame `choose(0, 1, 2)`, then the gravity `0.1 + random(0.2)` (`-0.1 + random(-0.2)` when `flipped`). `image_speed` is 0.
- The emitter sets direction and speed after Create.
- **Outside Room** destroys it.
- **Collision with a block** (with `sticks`): when the block has `blood_sticks`, the drop moves to contact and its speed and gravity become 0 (kid/A3.md). OPEN: the contact rule at fractional positions.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `flipped` | bool | false | Negative gravity. The emitter passes it on. |
| `sticks` | bool | true | Handle the block collision. |
