# Particles: `particles/<id>.toml`

Part of [format v0](v0.md).

One particle system as the original's code builds it in one go (rely objEffect Create, keep-celebrating objcpart, go-the-spook-house oRainSys, eat-the-lemon docobj2): GM's `part_type_*`, `part_emitter_*` and `part_particles_create` arguments. A [`particle_system`](builtins/particle_system.md) object names it by id (the file stem), or names several in a list, and builds it in its Create.

```toml
#:schema ../../../client/docs/format/schema/v0/particles.schema.json
depth = 10                                  # part_system_depth (drawing only)

[types.spark]                               # part_type_create; types are created in name order
shape = "flare"                             # pt_shape_*; or sprite = { id = "spark", animate = true, stretch = false, random = false }
size = [0.5, 1, -0.01, 0]                   # [min, max] or [min, max, increment, wiggle]
scale = [1, 1]
speed = [1, 2]                              # same form as size; so are direction and orientation
direction = [0, 360]
orientation = [0, 0]
orientation_relative = false
gravity = [0.1, 270]                        # amount, direction
color = ["#ffffff", "#ff0000"]              # color1/2/3; or color_mix = [a, b], color_rgb or color_hsv = [[min, max], [min, max], [min, max]]
alpha = [1, 0]                              # alpha1/2/3; a value may be { random = [min, max] }
additive = true                             # part_type_blend
life = [20, 20]                             # ticks
death = { type = "dust", count = 2 }        # part_type_death (count < 0: one with chance 1 / (|count| + 1)); step = {...} is part_type_step

[types.dust]
life = [10, 10]

[[emitters]]                                # part_emitter_create, in this order
region = [0, 800, 0, 608]                   # xmin, xmax, ymin, ymax in room coordinates
shape = "ellipse"                           # rectangle (default), ellipse, diamond, line
distr = "invgaussian"                       # linear (default), gaussian, invgaussian
stream = { type = "dust", number = -20 }    # n > 0: n a tick; n < 0: one with chance 1 / (|n| + 1)
burst = { type = "spark", number = 5 }      # once, when the system is built; n < 0: one with chance 1 / (|n| + 1)

[[create]]                                  # part_particles_create, after every burst
at = [400, 304]
type = "spark"
number = 3
```

Defaults are GM's: shape `pixel`, size and scale 1, speed, direction and orientation 0, gravity `[0, 270]`, color white, alpha 1, life `[100, 100]`. Building runs the calls in this order: the types, the system, each emitter with its region and stream, each emitter's burst, then the creates. Bursts and creates draw from the RNG at once; streams and step and death particles draw in each tick's particle update (after End Step, CLONE_SPEC numbers "Particle draw rules"). An `alpha` value written `{ random = [min, max] }` is a value the original's code draws (go-the-spook-house's rain, `random_range(0.12, 0.32)`): it is drawing only, so the client picks one value in the range per built system from its own generator, and the original's RNG draw stays with the code that makes it (a world's Create draws it there). Only the settings that decide draws reach the sim (`PartType`: size, speed, direction, orientation and life ranges, the color form, a random start frame, step and death); the rest is for the client's drawing. Positions never reach the sim either: it draws region points in unit coordinates.

Load errors, so a run never stops on them: a type name that is not in the file; `shape` together with `sprite`; more than one of `color`, `color_mix`, `color_rgb`, `color_hsv`; a stream, burst or death count of 0 and a step count below 1 (not traced; a negative step count is only Inferred, research#168); step or death particles on a type with a random life, or on a step or death type that has its own (not in the spec, N10). Ids are numbered in file path order.
