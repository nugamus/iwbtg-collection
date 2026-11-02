# `playerface`

The face on the death screen. **Create** sets `image_index = irandom(3)`: one draw from the gameplay RNG, made even when the face is never shown (11 of the 20 games; CLONE_SPEC numbers, "Hidden RNG consumers"). Showing the frame is the client's. Source: `crates/sim/src/builtins/playerface.rs`.

No params. No events other than Create.
