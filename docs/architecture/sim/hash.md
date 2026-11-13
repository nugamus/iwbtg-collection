# The simulation: State hash

Part of [the simulation](../sim.md).

## State hash

- `hash::state_hash(&World) -> [u8; 32]` is BLAKE3 over the **postcard** encoding of a canonical `HashView` struct borrowed from `World`: every field of `World` except `output` and `grid`, in declaration order: tick, room, `room_tick`, room speed, instances in id order with every field, `next_id`, run-time `var_names`, globals (both in `BTreeMap` order), engine globals, RNG state, `input` (this tick's frame and the direct-read latch, which changes outcomes per I1), views, pending request, persistent rooms, save slot, sound state, timing state, particle state (`World::particles`: types, systems, emitters, the live particles that still draw; #764), status, the play mode (`World::mode`, original or stripped, #561), the map (`World::map`, an index into `GameDef::maps`, #505), `setup_applied`. A field added to `World` is hashed unless it is a cache. `Real` serializes its canonical bits, so every NaN hashes the same.
- Postcard writes lengths as varints, so wasm32 and x86_64 give the same bytes. `std::hash::Hash` and its hashers are never used for state hashing: slices write their length with `write_usize` (4 bytes on wasm32, 8 on x86_64), and `DefaultHasher`/`RandomState` are unspecified across Rust versions (both are banned in the sim).
- No `usize` in hashed state, no pointers, no hash-map order. The output queue and the broadphase grid are not hashed.
- `setup_applied` is hashed, so a trace run that used setup actions can never match a clean verification.
- Checkpoints store the first 16 bytes.
- `hash::HASH_LAYOUT` versions the encoding. Adding fields to a state struct (for example `KidGlobals`) changes the hash but not the layout: re-pin the vectors in `tests/hash_vectors.rs` and the final hash in `tests/wp1_determinism.rs`. Bump `HASH_LAYOUT` only when the encoding itself changes.
