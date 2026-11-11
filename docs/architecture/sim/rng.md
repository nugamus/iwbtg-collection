# The simulation: Random numbers

Part of [the simulation](../sim.md).

## Random numbers

- One gameplay generator in `sim::rng`, compatible with GM8's Delphi-style 32-bit LCG, so rooms that use `random()` can match the original's traces. Constants and the float derivation come from the CLONE_SPEC (OpenGMK uses multiplier 0x08088405, increment 1, step 2^-32; **OPEN** until confirmed).
- The seed is in the replay header. It is the only randomness in the sim.
- Each built-in documents how many random draws it consumes, so the stream stays aligned with the original.
- **Hidden consumers draw in the sim.** In the originals, blood picks its frame and gravity with `choose` and `random`, and playerface draws `irandom(3)`, from the gameplay generator (CLONE_SPEC numbers, Confirmed). So the blood emitter, blood and playerface built-ins make those draws in the sim, in the original's order (blood Create draws subimage then gravity inside `spawn`, then the emitter draws direction and speed), and send the drawn values to the client as output events. Only drawing stays in the client.
- Particle systems (N10, `NumRules::particles_draw_rng`). The spec now records (recorder) that the runner draws from the gameplay generator for particles: 2 draws on most rows of rely room1, where its particle emitters run, and more when particles spawn (CLONE_SPEC numbers, Confirmed (recorder), num-rng-a.csv; the UNKNOWN entry is "partly answered"). Whether the real runner draws the same number is open. The engine_rev 0 default `false` predates that result, so rooms with particle emitters will not match the traces' RNG stream until WP11 changes the default and the particle built-ins make those draws.
- Ranked runs use a random seed per run, as in the originals; the server flags reused seeds (user decision, 2026-11-02, [product](../../product.md)).
