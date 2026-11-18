# `particle_system`

An object whose code builds a particle system: rely objEffect, keep-celebrating objcpart, go-the-spook-house oRainSys, eat-the-lemon docobj2, docobj3, object131, object132. **Create builds** the `particles/<id>.toml` system it names ([particles.md](../particles.md)) in the sim's particle state (`World::particles`, `Particles::build`): the types, the system, each emitter with its region and stream, each emitter's burst, then the `[[create]]` calls. `system` may list several files (rely objEffect builds two systems, at depths -100000 and 0): Create then builds each file in list order, all of one before the next, so the RNG draw order is defined, and each system draws at its own file's `depth`. Bursts and creates draw from the RNG in Create; streams draw in each tick's particle update after End Step (CLONE_SPEC numbers "Particle draw rules", N10). Source: `crates/sim/src/builtins/particle_system.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `system` | particles, or a list of them | required | The `particles/<id>` file, or the files, built in list order. |
| `on_room_start` | `none`, `clear`, `destroy_emitters`, `destroy` | `none` | The call the instance's Room Start makes. |
| `on_room_start_except` | list of rooms | none | Rooms where Room Start does nothing at all: no call and no destroy (go-the-spook-house oRainSys). |
| `on_room_start_destroys_instance` | bool | false | Room Start destroys the instance after its call; its Destroy runs, so `on_destroy`'s call too, and it gets no later Room Start. |
| `on_room_end` | same | `none` | The call the instance's Room End makes. |
| `on_destroy` | same | `none` | The call the instance's Destroy makes. |

In GM a system outlives the instance that made it and the room, until code clears or destroys it, so the params say what the original's code does later. Each call reaches every system of the `system` list, in list order. None of the calls draws (CLONE_SPEC numbers "Particle draw rules" 8, Confirmed (recorder), research#167):

- `clear` (`part_particles_clear`): the live particles go, with no death particles; the emitters keep streaming.
- `destroy_emitters` (`part_emitter_destroy_all`): the streams stop; the live particles stay, age and still fire their death particles.
- `destroy` (`part_system_destroy`): the system is gone and draws nothing again.

A call after `destroy` does nothing. The client draws each system from its own copy, which follows these calls ([particles](../../architecture/client/particles.md)). Each new instance builds a new system with new types, as the original's Create does each time it runs.

**Switch.** `NumRules::particles_draw_rng` is on in engine_rev 0 (#766); without it the built-in builds nothing and draws nothing, and the client draws nothing either. Whether a system's update runs on a tick a room request ends early, and whether systems survive the room change, are Confirmed (recorder), CLONE_SPEC "Particle draw rules" item 9: the update is skipped and the sim keeps systems across rooms and game_restart.

**Look only.** A type's `alpha = [{ random = [min, max] }]` (go-the-spook-house's rain) is drawing only: the client picks one value per built system from its own generator. It never reaches the sim or the state hash; the original's RNG draw for it stays with the code that makes it (the world's Create).

**Known difference after F2.** go-the-spook-house's oRainSys calls `part_particles_clear(global.rain)` with the rain *type* number, which equals the system number only before the first game restart. The original clears the rain on a fresh start only; after F2 the call clears nothing and the falling rain lives out its life (up to 100 ticks) in the new room. The port's `on_room_start = "clear"` always clears. The difference is drawing only, with no RNG or state-hash effect, because the rain type has no step or death particles. No format key models it (#901); reopen it if a game needs the original behaviour for gameplay. Source: research#216, CLONE_SPEC "Particle draw rules" item 8.

**State.** The definitions, the three calls, the excepted rooms, `on_room_start_destroys_instance` and the systems Create built (hashed with the instance). Tests: `particles_rng::particle_system::*` (sim), `load_particles::*particle*` (loading). Converter requests CR23 (research#183) and CR24 (client#826).
