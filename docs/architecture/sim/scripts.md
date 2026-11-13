# The simulation: Scripts and the script API

Part of [the simulation](../sim.md).

## Scripts

Scripts are Rhai under the IWS profile ([0001](../../decisions/0001-script-language.md)). The sim talks to them only through the `ScriptHost` trait, so the sim core never names Rhai types.

- A scripted object's events run in the same pipeline slots as built-in events, in the same (rank, id) order.
- During a script call the world is lent to the Rhai host: `on_event` swaps a placeholder into `ctx.w` and parks the live world in the host (`RefCell<Option<World>>`). Each API function borrows it briefly; a function that needs a whole `Ctx` (spawn, destroy, kill) takes the world out, builds a `Ctx` and puts it back, so a spawned instance's Create can run its own script. The world is restored also on errors. Single thread, no `unsafe`, no `sync` feature.
- Built-ins call scripts only through defined events: `collide`, `trigger`, `animation_end`, timeline moments, and the `call` trigger action.
- The budget counter, caps and error handling are in [0001](../../decisions/0001-script-language.md#step-budget).
- **The engine (WP9.1, `script/host.rs`, `script/api.rs`, `script/budget.rs`).** `script::host::Iws::new(NumRules)` builds the IWS engine: `Engine::new_raw()` with the core, arithmetic, logic, string, array and map packages only, `eval`, `**` and `**=` disabled, the 0001 limits, strict variables, the simple optimizer and fast operators off. The numeric API is `num` under the run's `NumRules` (`gm_eq`..`gm_ge`, `gm_bool`, `gm_repeat`, `div`, `mod`, rounding, libm transcendentals, `random`, `irandom`, `random_range`, `irandom_range`, `choose`, `tick`, `room_tick`); `/` on two integers is a FLOAT. **Rhai's built-in float comparisons (1.26.1 and 1.27.0) use a relative epsilon**, which is neither IEEE nor GM8's rule, so the engine overrides `== != < <= > >=` with exact IEEE (a test pins `0.1 + 0.2 == 0.3` false and `gm_eq` true). The RNG and the tick counters are lent per tick (`begin_tick`, `end_tick`), so the scripts' draws are the sim's draws. The tick budget (`budget.rs`: 1,000,000 units, 200,000 per call) counts one unit per Rhai operation and ends a runaway call with a deterministic error.
- **The host (WP9.2, `script/host.rs`, `script/inst.rs`).** `ScriptRuntime::new(def, rules)` compiles every script (a compile error is a load-time `SimError::Script` with the file and position) and keeps the functions each defines. `script::on_event` (called by `events::dispatch` after the base built-in) maps the event to a function (`create`, `begin_step`, `step`, `end_step`, `late`, `alarm(n)`, `user(n)`, `collide(other)`, `outside_room`, `intersect_boundary`, `animation_end`, `room_start`, `room_end`, `game_start`, `destroy`, `end_of_path`) and calls it with `this` bound to the instance; a function the script does not define is skipped. `ScriptRuntime::event_set(id)` gives the events a script handles (the loader merges it into the kind's `EventSet`; collision classes stay the kind's business). The instance API is `Inst` handles (ids are never reused, so a stale handle is a script error, and `exists(h)` tells) with the built-in fields as properties, the package's custom variables through the same syntax (`this.hp`, mapped by `GameDef::var_names`; an unknown name is an error, an unset one reads 0 when `uninit_reads_zero`), `set_alarm(n, steps)`, `get_alarm(n)`, `instance_create`, `destroy`, `exists`, `instances(kind)` (event order), `kid()`, `emit(name)` and `kill_kid()`. Scripts nest at most 8 deep. A script call may create at most 256 instances and all scripts 4,096 per tick (`budget::MAX_SPAWNS_PER_CALL`, `MAX_SPAWNS_PER_TICK`; a Create a spawn runs counts toward the spawning call); exceeding either halts the sim with a script error that names the cap, file, line and tick. No more than 50,000 instances are live at once (`world::store::MAX_LIVE_INSTANCES`, checked in `World::insert` for every spawn, script or built-in): the 50,001st halts the sim with `instance cap exceeded`. The tick loop calls `ScriptRuntime::begin_tick` at row 0 to reset the budget. An error becomes `SimError::Script` with `file: message (line, position) [kind, instance, event, tick]` and halts the sim.
- **The validator (WP9.3, `script/validate.rs`).** `script::validate::validate(&GameDef)` checks every script without running it and returns `Issue { script, line, column, severity, message }`; the format loader turns them into diagnostics (`scripts/boss.rhai:2:9: error: ...`), so the client, the server and `iwbtg pkg check` refuse the same scripts. Errors: the script does not compile (strict variables, `eval` and `**` disabled); a top-level item that is not `fn` or `const`; an event function (`step`, `alarm`, `moment_<name>`, ...) with the wrong argument count; a call of a function that is neither registered nor defined in the file, or with a wrong argument count; a string literal passed to `instance_create` or `instances`, or compared with `.kind`, that names no kind (`NAME_ARGS` also resolves sounds and rooms when an API function takes them); a call that draws from the RNG, changes state or calls a script function that does, in a right operand of `&&` or `||` (Rhai short-circuits, GML evaluates both; the left operand always runs, so it is fine; use `&` or `|` on bools, or evaluate into a variable first); `switch` on float case values (read from the token stream, because Rhai hashes case values away); a real-valued property or number function used as a condition (`if this.hspeed`; a number literal is already a Rhai compile error). Warning: raw `==` or `!=` where an operand is a float literal or a real property (use `gm_eq`/`gm_ne`). The walk runs on the AST the engine compiles with the simple optimizer, so code the optimizer removes (an empty `if`, an unused `let` of a pure call) is not checked. The function classification lists (`RNG_FNS`, `STATE_FNS`, `NUMBER_FNS`, `OTHER_PURE_FNS`, `REAL_PROPS`) are pinned to the registry by tests: a new API function must be added to one of them.
- **API descriptions (#364, `script/docs.rs`).** Each registered function, operator and property has a one-line description added next to its registration with `docs.add(name, text)` (overloads share one). `iwbtg script api-docs` prints them into `docs/format/script-api.md` and fails when a registered name has none, so a new API function cannot land undocumented.

### Script API (first version)

This is the starting surface. The generated reference is `docs/format/script-api.md`, and the code is the authority; a name marked *planned* below is not registered yet, and a script that calls it fails validation. Every number the API returns is a FLOAT. Every function that takes a kind, sound, room or trigger name is checked at load when the name is a string literal.

**Event functions** a script file may define: `create()`, `begin_step()`, `step()`, `end_step()`, `alarm(n)`, `collide(other)`, `trigger(name)`, `outside_room()`, `animation_end()`, `room_start()`, `room_end()`, `destroy()`, `late()`, `moment_<name>()`, and the client-only `draw(d)`.

**`this` and other instances** (`Inst` handles):

- Read and write: `x`, `y`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`, `sprite`, `image_index`, `image_speed`, `image_xscale`, `image_yscale`, `image_angle`, `image_alpha`, `visible`, `solid`, `depth`.
- Read only: `xstart`, `ystart`, `xprevious`, `yprevious`, `kind`, `name`, `group`, `id`.
- Custom variables: `this.hp = 20.0`. Unset reads are errors.
- Parameters: the map given to `instance_create` sets them. Reading a parameter set in the room file or the object file from a script is *planned* (there is no `param` function).

**Instances**

| Function | Notes |
|---|---|
| `instance_create(kind, x, y)` and `instance_create(kind, x, y, #{ ... })` (`spawn` is a reserved word in Rhai) | Returns the new `Inst`. The map sets built-in variables and parameters. Params that match a `[defaults.<kind>]` variant take the variant's kind first, with the loader's match (`GameDef::variant_kind`, `def::kind::variant_matches`; `spike` with `dir: "down"` is `spike[dir=down]`, #701); the loader makes a kind for every variant, placed or not (`GameDef::variants`). Then params that pick a class (`def::class::class_params`) take the kind the loader derived from `kind` with those values (`water[variant=2]`, #659), since classes come from the kind; a spawn that picks classes no such kind has is a script error, because no kind is made at run time (#694). |
| `destroy(i)` | |
| `exists(i)` | |
| `instances(kind)` | Array in event order, snapshotted at call time. Replaces GML `with`. Covers the kinds the loader derived from `kind` (`GameDef::kind_covers`, as probe kind targets do), as a GM object index covers its children (#694). Groups are *planned*. |
| `count(kind_or_group)` | *Planned.* |
| `nearest(kind, x, y)` | *Planned.* Ties broken by (rank, id). |
| `find(name)` | *Planned.* The instance with that `name` in the current room. |

**Collision** (*planned*): `place_meeting(i, x, y, role_or_kind)`, `place_free(i, x, y)`, `collision_point(x, y, role_or_kind)`, `collision_rect(x1, y1, x2, y2, role_or_kind)`, `collision_line(x1, y1, x2, y2, role_or_kind)`. Each returns a bool or the first `Inst` in (rank, id) order.

**Kid**: `kid()` returns the kid's `Inst` handle, or `()` if there is no kid (the view of `facing`, `alive` and `on_ground` is *planned*). `kill_kid()`. `set_kid_gravity(dir)` for gravity flips is *planned*. Other knobs are added only with a CLONE_SPEC reason.

**Rooms, triggers and timing**: `emit(name)`, `this.set_alarm(n, ticks)` (a method of the instance, so `other.set_alarm(...)` works too), `this.get_alarm(n)`, `tick()` and `room_tick()` are registered, and the room functions (#731): `room_goto(room)` (a room id; an unknown one halts the sim, and a literal is checked at load), `room_goto_next()`, `room_goto_previous()` (by room order; past either end the change at row 15 is a `SimError::Script`) and `room_restart()`. Each sets the world's one pending request (`Ctx::request_room`, last wins, R13); the rest of the script runs, then the tick stops as after a built-in's request (R8). The loader's reachability check reads them from the source (`script::validate::room_calls`), with the kind literals passed to `instance_create`, whose calls count in the creating kind's rooms (#751). *Planned*: `start_timeline(i, timeline)`, `stop_timeline(i)`, `room_speed()`.

**State** (*planned*): `global_get(name)`, `global_set(name, v)`, `flag(name)`, `set_flag(name, v)` (flags go into the save slot).

**Input** (*planned*; for menus and quizzes): `pressed(action)`, `held(action)`, `released(action)`.

**Audio and effects** (queued for the client, never read back): `play_sound(name)`, `play_music(name)` (a loop; another track stops or blocks it by the run's MULTIMEDIA rule), `stop_music()` and `shake(ticks, strength)` are registered; sound names are checked at load, and a pack sound is `engine:<pack>/<id>`. *Planned*: `play_music(name, #{ from_tick: n })`.

**Menus and text** (*planned*): `text_box(...)`, `menu(...)`. Exact shape **OPEN**, from the quiz, battle menu and difficulty menu in the shortlist games.

**Random**: `random(x)`, `irandom(n)`, `random_range(a, b)`, `irandom_range(a, b)`, `choose([...])`. All draw from the sim's GM8 generator.

**Math** (all in `sim::num`): `sin`, `cos`, `tan`, `arctan2`, `dsin`, `dcos`, `darctan2`, `sqrt`, `pow`, `exp`, `ln`, `abs`, `sign`, `floor`, `ceil`, `round` (banker's), `frac`, `min`, `max`, `clamp`, `lerp`, `div`, `mod`, `point_direction`, `point_distance`, `lengthdir_x`, `lengthdir_y`, `gm_eq`, `gm_ne`, `gm_lt`, `gm_le`, `gm_gt`, `gm_ge`, `gm_bool`, plus `sqr`, `degtorad`, `radtodeg`, `to_float`, `to_int` and `gm_repeat`.

**Draw** (*planned*; client only, inside `draw(d)`): `d.sprite(...)`, `d.rect(...)`, `d.text(...)`, `d.bar(...)`. No setters on anything.
