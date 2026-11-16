# `particle_system`

An object whose code builds a particle system: rely objEffect, keep-celebrating objcpart, go-the-spook-house oRainSys, eat-the-lemon docobj2, docobj3, object131, object132. **Create builds** the `particles/<id>.toml` system it names ([v0.md, Particles](../v0.md#particles-particlesidtoml)) in the sim's particle state (`World::particles`, `Particles::build`): the types, the system, each emitter with its region and stream, each emitter's burst, then the `[[create]]` calls. Bursts and creates draw from the RNG in Create; streams draw in each tick's particle update after End Step (CLONE_SPEC numbers "Particle draw rules", N10). Source: `crates/sim/src/builtins/particle_system.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `system` | particles | required | The `particles/<id>` file. |
| `on_room_start` | `none`, `clear`, `destroy_emitters`, `destroy` | `none` | The call the instance's Room Start makes. |
| `on_room_end` | same | `none` | The call the instance's Room End makes. |
| `on_destroy` | same | `none` | The call the instance's Destroy makes. |

In GM a system outlives the instance that made it and the room, until code clears or destroys it, so the params say what the original's code does later. None of the calls draws (CLONE_SPEC numbers "Particle draw rules" 8, Confirmed (recorder), research#167):

- `clear` (`part_particles_clear`): the live particles go, with no death particles; the emitters keep streaming.
- `destroy_emitters` (`part_emitter_destroy_all`): the streams stop; the live particles stay, age and still fire their death particles.
- `destroy` (`part_system_destroy`): the system is gone and draws nothing again.

A call after `destroy` does nothing. The client draws each system from its own copy, which follows these calls ([particles](../../architecture/client/particles.md)). Each new instance builds a new system with new types, as the original's Create does each time it runs.

**Switch.** `NumRules::particles_draw_rng` is on in engine_rev 0 (#766); without it the built-in builds nothing and draws nothing, and the client draws nothing either. Whether a system's update runs on a tick a room request ends early, and whether systems survive the room change, are Confirmed (recorder), CLONE_SPEC "Particle draw rules" item 9: the update is skipped and the sim keeps systems across rooms and game_restart.

**State.** The definition, the three calls and the system Create built (hashed with the instance). Tests: `particles_rng::particle_system::*` (sim), `wp7b_loader::*particle*` (loading). Converter request CR23 (research#183).
