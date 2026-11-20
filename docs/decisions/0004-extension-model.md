# 0004: Extension model for unknown mechanics

Status: Proposed for the user's approval, 2026-11-20. Issue [#778](https://github.com/nugamus/iwbtg-collection/issues/778). The PR that closes #778 is the approval; until it merges this is a draft.

Related: [0001 script language](0001-script-language.md), [0002 data syntax](0002-data-syntax.md), [format principles](../format/README.md), [world model](../architecture/sim/world.md), [physics profile](../format/physics.md), [scripts](../architecture/sim/scripts.md), [collision](../architecture/sim/collision.md), [versioning](../architecture/versioning.md), [content: engines](../content.md#engines).

## Context

The collection is built to grow past the 20 original games. The extension points it has today all lead back into Rust:

- **The kid is closed.** A profile's events are ordered lists of named actions (`physics.toml` `[events]`, `crates/sim/src/def/physics.rs`; `KidAction` in `crates/sim/src/kid/mod.rs`), drawn from a fixed per-family vocabulary in `crates/sim/src/kid/actions/`. Scripts can kill the kid, and the generic `Inst` setters reach its `x`, `y`, speed and image fields on any instance; but a script cannot run at a kid slot or read/write kid-only state (`djump`, `curJumps`, ...), and the native Step actions overwrite generic motion each tick. The docs call the kid read-only ([0001](0001-script-language.md), How scripts call the API; `docs/architecture/sim/scripts.md`, Script API); the code has no such guard. Either way, a dash, a wall-jump variant, a grapple or custom gravity is new Rust and a client release.
- **Built-ins absorb one-game quirks.** Every difference between the original objects is a parameter ([format principle 2](../format/README.md)). That works while the parameter is general. It stops working when a switch exists for one game: the `save` built-in carries `requires_kid_in_room` (xia) and `alarm_save` (eat-the-lemon, xia), `gravity_flip` carries `transition = "yflag_with_view_rotation"` (kaenbyou), and the save position table carries `floor_nudge` (xia). `crates/sim/src/def/physics.rs` alone has 175 public items for seven families. At 50 or 200 games, a shared built-in becomes the union of every game's quirks, and every change risks every game that uses it.
- **Collision classes are capped.** `ClassMask` is a `u64`: 29 named class bits (0-28), three reserved bits (29-31), and 16 package bits (32-47) (`crates/sim/src/def/class.rs`). The loader assigns package classes in first-use order and rejects the 17th (`crates/format/src/load/kinds.rs`, "too many package classes ... at most 16"). GM games use parent objects freely, and a boss or avoidance game can collide many scripted kinds with each other.
- **Promotion has no rule.** Principle 6 says a script pattern that repeats across games becomes a built-in. There is no threshold, no reverse rule for a one-game quirk, and no statement of what must stay native.
- **Scripts are the only creator extension point, but they are incomplete for this job.** They are Rhai under the IWS sandbox, deterministic, budgeted and loaded as package content ([0001](0001-script-language.md)). A scripted object can already take a built-in `base` and add events (`docs/format/objects.md`; [world model](../architecture/sim/world.md), Package objects), and scripts can query collisions and spawn instances. What they cannot do is run at a kid slot, change kid movement state, or read a built-in's parameters.

The question this decision answers is the one a creator asks: *"I want a dash. What do I write, and does it need a client release?"* Today the answer is "Rust, and yes." The collection cannot scale to community mechanics on that answer.

## Options considered

### Where a mechanic goes

| Option | Verdict | Reason |
|---|---|---|
| Everything in Rust (today) | Rejected | The core grows without bound, creators wait on maintainers, and every ranked game runs every quirk. |
| Everything in scripts (a scripted kid and scripted built-ins) | Rejected | Per-tick kid physics and collision resolution are the fidelity and replay core; Rhai cannot meet the budget for thousands of live instances ([0001](0001-script-language.md), Costs we accept; #775). |
| Fixed extension tiers: data first, scripts for creators, built-ins promoted, runner rules maintainer-only | **Chosen** | Keeps the hot, fidelity-critical paths native, gives creators a no-release path, and makes every core addition pass a stated test. |
| WebAssembly or native plugins in packages | Rejected | Violates format principle 1 (readable text, never compiled) and the sandbox; no cross-platform determinism. |
| An arbitrary script hook on every engine point | Rejected | Every hook becomes API surface and a fidelity risk. Fixed slots only. |

### Kid hook

| Option | Verdict | Reason |
|---|---|---|
| No hook; new movement is always Rust | Rejected | A dash needs a client release, which is the problem. |
| A fully scripted kid (a script replaces the event lists) | Rejected | Loses the fidelity-pinned actions, makes the kid's state layout script-visible, and puts per-tick Rhai on the ranked critical path. |
| A script action at fixed slots, plus a documented field set | **Chosen** | New movement composes with the native actions; the hot path stays native unless a game chooses a slot. |
| A native DSL of new kid actions | Rejected | Still a client release per mechanic, and it is the same vocabulary with more syntax. |

### Built-in script hooks

| Option | Verdict | Reason |
|---|---|---|
| No script on a built-in base (scripts only on `scripted`) | Rejected | `base = "killer"` already exists and saves native roles and behavior; removing it pushes games to reimplement it. |
| Scripts read and write a built-in's native state | Rejected | Makes every built-in's private layout API, couples scripts to Rust, and breaks "documented once, behaves the same". |
| Scripts read params, write generic fields only; native state opaque | **Chosen** | One-game behavior can be layered on a native base without exposing its invariants. |

### Class capacity

| Option | Verdict | Reason |
|---|---|---|
| Keep 16 | Rejected | The issue's premise: boss and avoidance games collide many scripted kinds. |
| Widen `ClassMask` to `u128`, with 96 package bits | **Chosen** | O(1) intersections, `Copy`, no per-instance allocation; the existing first 16 bits and every existing hash stay byte-identical. |
| A sorted set of class ids per instance | Rejected | Allocates per instance (qoqoqo has 28,900 instances), slows the collision phase's hot path, and makes the hashed state larger and less regular. |
| Per-game class namespaces or run-time class creation | Rejected | Classes must be fixed at load for `object_order`, derived kinds, validation and `gameplay_hash`. |

## Decision

### The four extension tiers

A mechanic goes in the **lowest** tier that can express it. The tiers, from most to least creator-accessible:

| Tier | What lives there | Who writes it | How it ships | Versioned by |
|---|---|---|---|---|
| **Data** | Params, presets (`objects/`), physics profiles (`physics.toml`, `physics/`), rooms, timelines, paths, particles, engine packs and their assets | Creator/porter | Package | `gameplay_hash` |
| **Game scripts** | Rhai under IWS: scripted objects (with an optional native `base`), the shared script library (`lib:`, #797), and the new kid slots | Creator/porter | Package | `gameplay_hash` |
| **Shared built-ins** | `crates/sim/src/builtins/`, documented once in `docs/format/builtins/<kind>.md`; params and presets | Maintainer, by promotion | Client release | `engine_rev` for behavior fixes; `gameplay_hash` for params |
| **Runner rules** | `RunnerRules` (tick pipeline, motion, collision resolution, numbers, rooms, sound) and `engine_rev` branches | Maintainer only | Client release | `engine_rev`, `SIM_VERSION` |

These are the **extension tiers**. They are not the `Tier 1`/`Tier 2`/`Tier 3` work-package labels in `crates/sim/src/def/builtin.rs`, and not the fidelity tiers T1-T4 in [fidelity-tiers](../plans/fidelity-tiers.md). This record does not change either.

Three consequences of the table:

1. **Packages are never code.** Data and scripts are the only creator-supplied tiers. An engine pack bundles assets and a physics preset; a "custom engine" is a custom physics profile plus scripts and assets, not new runner code. A new GM semantic is a runner rule: a client release, a new `engine_rev`, and a golden case (see [versioning](../architecture/versioning.md), A fix).
2. **A game selects a runner revision, it does not supply one.** `game.toml` `engine_rev` picks an existing branch. A package cannot carry a `RunnerRules` value.
3. **Creator work does not need a client release.** A new mechanic in tiers 1 or 2 ships as a package edit. Only a promoted built-in or a new runner rule does.

### Promotion and demotion

**Promotion to a shared built-in** requires all of:

1. **Demand.** At least three shipped or committed ports need the same behavior. Two games are served by a versioned shared script (`lib:`, #797) until the third. The research census (research#172) may substitute projected ports for shipped ones, but the count is still three.
2. **Native need.** It needs one of: speed that the stress gate below shows a script cannot sustain; engine-internal state that is deliberately not exposed to scripts; or per-tick fidelity that a script slot cannot reach.
3. **Generality.** It is a concept, not a game. Its parameters are meaningful to at least three games; no parameter may name a game, a room, an original object id or a one-game quirk.
4. **Fidelity.** It comes with a spec or trace case and a golden/fidelity test, and it does not change any existing game's behavior. A behavior fix that does change one branches on `engine_rev` (old path kept), per [versioning](../architecture/versioning.md).

**A one-game quirk never becomes a shared built-in.** It is data if a parameter or preset can express it; otherwise it is a script. The single exception is behavior inside the kid's per-tick physics or collision resolution, which stays native: then it is a documented param or action, names the game in its page, and comes with a trace. New kid movement is a script slot instead (below).

**Demotion** works the other way:

- A built-in parameter used by exactly one game, and not a general concept, is moved to a preset or a script on `base = "<kind>"` at the next format major. It may stay as a documented part of the kind only if the page names the concept and the games that use it. The survey is follow-up #1076.
- A built-in kind used by one game and reproducible by a script is removed only at a format major, with a migration and re-verified traces.
- Demotion is never silent. It is a format change or a new `engine_rev`; a shipped `gameplay_hash` never starts playing differently.

The practical rule for a reviewer: a new built-in or param must name at least three games, or say why a preset/script cannot do it, in the issue and the page.

### Kid script slots

A profile may run a script function at a fixed slot in any kid event list. The action is a TOML table:

```toml
# physics.toml
preset = "A1"

[script]
file = "kid"                 # scripts/kid.rhai
vars = ["dash_dir"]          # the kid script's custom variables, interned at load

[events]
step = [
  "gravity", "walk", "air_sprite", "fall_cap", "shoot", "jump", "release", "vines",
  { do = "script", fn = "dash" },
]
keys = [
  { action = "extra1", on = "press", run = [{ do = "script", fn = "start_dash" }] },
]
```

- **Where it may appear.** Any `[events]` action list: `create`, `room_start`, `step`, `end_step`, alarm rows and key rows. A collision row takes a script run too: `{ with = "dash_zone", run = { do = "script", fn = "dash_touch" } }`. The native run and a script row can both be listed for the same class; rows run in order (`KidEvents.collisions`).
- **Which file.** `[script] file` names one script under `scripts/` (or later a `lib:` reference). A room profile may name its own file and its own slots; the kid kind's event set is unchanged, as [physics.md](../format/physics.md) requires for room profiles.
- **Signature and binding.** The function takes no arguments for the list events and one argument (`other`) for a collision slot. `this` is bound to the kid's `Inst`, exactly as for an object's script events. Helpers in the same file are callable. A function named by a profile that is missing, or has the wrong arity, is a load error.
- **Execution.** The slot runs in list order where the action sits. It uses the same `ScriptHost`, tick and call budget, spawn caps, error handling and world-lending rules as any script ([scripts](../architecture/sim/scripts.md)). A room change requested by a slot stops the rest of the event list, like a built-in's request. A slot cannot run before the kid exists; the kid's own Create is after the instance is in the world.
- **Custom state.** `[script] vars` interns the names the kid script may read and write as `this.<name>`. Names are package content and enter `gameplay_hash`. This is needed because a built-in has no object file to declare params; the script indexer only reaches `GameDef::var_names` (`crates/sim/src/script/inst.rs`), and today no syntax declares a kid variable. A name not in `[script] vars` remains an error, as for any undeclared variable.
- **Determinism and hashing.** Slots run at a fixed position, in a fixed order, with no closure state ([0001](0001-script-language.md), Determinism rules). Kid state and `Instance.vars` are hashed. The script text and the `[script]` table are in `gameplay_hash`, so editing the dash is a gameplay edit, exactly as editing an object or a room.
- **What a slot does not do.** It does not add or remove events, does not change the kid's classes, and does not replace the native actions listed around it. A profile that drops the family's core actions is a custom engine and must prove fidelity with traces. Validator support: a slot function is state-changing (it is in `STATE_FNS`), and its name is checked at load.

This decides what replaces the "kid is read-only" sentence in [0001](0001-script-language.md) (How scripts call the API). That sentence is already looser than the code: the generic `Inst` setters on a kid handle write `x`, `y`, `hspeed`, `image_xscale` and the other generic fields today, because the setters do not special-case the kid. The new contract is the documented field table and the slots, not an enforced read-only rule; the follow-up implementation issue updates 0001, `docs/format/physics.md`, `docs/architecture/sim/world.md` and `docs/architecture/sim/scripts.md`, and reconciles the generic setters with the text.

### Kid fields a script may read and write

On a kid `Inst` (as `this` in a slot, or through `kid()`), the generic GM fields are already available ([scripts](../architecture/sim/scripts.md), Script API): `x`, `y`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`, `sprite`, `image_index`, `image_speed`, `image_xscale`, `image_yscale`, `image_angle`, `image_alpha`, `visible`, `solid`, `depth`. The hook adds the kid-only fields, by their GM names:

| Field (GM name) | Rust field | Access | Notes |
|---|---|---|---|
| `djump` | `djump` | read/write | The double-jump flag. |
| `curJumps` | `cur_jumps` | read/write | Integer 0-255; a non-integer or out-of-range write is a script error. |
| `tjump` | `tjump` | read/write | The third-jump flag (showdown). |
| `onPlatform` | `on_platform` | read/write | The native `air_sprite` action may clear it on its next run. |
| `ladder` | `ladder` | read/write | The native `ladder` action owns it when listed. |
| `frozen` | `frozen` | read/write | Input block. |
| `frozen2` | `frozen2` | read/write | Movement-list skip. |
| `muteki` | `invincible` | read/write | Kills are ignored while set. |
| `tempHspeed` | `temp_hspeed` | read/write | Showdown's ice carry value. |
| `maxSpeed` | `run_speed` | read/write | The run speed; `kid_param_zone` writes it too. |
| `maxVspeed` | `max_vspeed` | read/write | The speed clamp. |
| `maxFallSpeed` | `max_fall_speed` | read/write | Read only by the A4 fall rule. |
| `scale` | `scale` | read/write | Machinary's layer scale, 1 elsewhere. |
| `ladderud` | `ladder_ud` | read only | Set each Step by the ladder action. |
| `ladderjump` | `ladder_jump` | read only | Set by the jump action. |
| `yflag` | `yflag` | read only | Use the gravity-flip knob; the mask and gravity must flip together. |
| `flipped` | `flipped` | read only | Part of the kind's identity (the swapped kid object). |
| `facing` | `facing` | read only | Derived from `image_xscale`; write `image_xscale` instead. |
| `jump`, `jump2`, `jump3` | `jump_speeds` | read only | Arrays in scripts are #997; use the profile or `kid_param_zone`. |
| `on_slope_last` | `on_slope_last` | read only | Set by the slope code. |
| `walk_h` | `ext.walk_h` | read only | The last `walk` direction, -1, 0 or 1. |
| `onVineL`, `onVineR` | `ext.on_vine_l`, `ext.on_vine_r` | read only | Family C's vine probes. |

`image_xscale` stays the one setter for facing. `yflag` and `flipped` stay behind `set_kid_gravity`/`flip_grav`/`player_reverse` (#776 and `crates/sim/src/kid/api.rs`) so no script can leave the mask and gravity disagreeing. The `b`/`ext` family sub-state is not exposed in v1; a family that needs a getter adds it with a spec reason.

Every field above is plain data in `KidState` (`crates/sim/src/kid/state.rs`), already part of the hashed state, so writes are deterministic and replay-visible. The field table is the documented set; a new kid field is not exposed by default.

### Built-in script hooks and base state

A scripted object with `base = "<kind>"` already keeps the base's native behavior, roles, classes and event set and adds the script's events, which run after the base's ([world model](../architecture/sim/world.md), Package objects; [scripts](../architecture/sim/scripts.md)). This decision fixes what that script may touch:

- **Read:** generic instance fields, custom variables declared by the object or instance `params`, and the kind's parameters through `param(name)` (read-only, #776).
- **Write:** generic instance fields and declared custom variables.
- **Opaque:** the base's `KindState`/`BuiltinState`, its event set, its classes, and its private alarms. No script getter or setter for them. A script cooperates through events, custom variables and the public query API.
- **Classes and events are load-time.** A script cannot add a class, change `counts_as`, or add an event function the loader did not see. `instance_create` already picks a derived kind the loader made (#659, #694, #853); nothing is created at run time.
- **Collision resolution stays native.** A script handler adds a handler; it does not replace the masks, 1 px stepping or pair order.

If a one-game quirk needs to change a base's private state, the answer is a documented param on the base (under the promotion rules) or a scripted object from scratch, not a getter into the base.

### Class capacity

**Decision: widen `ClassMask` from `u64` to `u128` and give packages 96 classes.**

- Named classes keep bits 0-28. Bits 29-31 stay reserved.
- Package classes take bits 32-127: `PACKAGE_FIRST_BIT = 32`, `PACKAGE_COUNT = 96`. The first 16 keep their current bit positions (32-47); bits 48-127 are newly available. This overrides the "no later appends" comment in `crates/sim/src/def/class.rs` for the package range.
- Assignment stays first-use order at load (`crates/format/src/load/kinds.rs`, `class_named`). The 97th distinct package class is the new load error; the 17th no longer is.

**Why 96 and not unbounded.** `ClassMask` is `Copy` and sits in every `Instance`, and every collision query and collision-phase pair test intersects it (`crates/sim/src/collide/`, #781). A sorted set would allocate per instance and turn an O(1) test into a merge. 96 is six times today's limit, enough for the scripted kinds a boss or avoidance game is expected to need; the census (research#172) may revise the number. If a real port exceeds it, the next step is a 256-bit mask or an interned class-id bitset, not a per-instance set.

**Hash and replay impact.**

- `gameplay_hash` is over the canonical parsed package files (`crates/format/src/load/hash.rs`), not over `ClassMask`. It does not change.
- `Instance.classes` is part of the hashed state (`crates/sim/src/hash.rs`, `HashView.instances`). postcard 1.1.3 writes `u64` and `u128` with the same LEB128 varint, so any value below 2^64 encodes to exactly the same bytes. Every instance in every existing package has only named bits and, at most, package bits 32-47 set. Its state hash, checkpoint hashes and `final_hash` are therefore unchanged: no `HASH_LAYOUT`, `SIM_VERSION`, `engine_rev` or `REPLAY_FORMAT` bump. A package that uses a 17th class sets a bit at 64 or above; only that new package's instances encode longer, and it has no old replays.
- A test pins the unchanged bytes for the first 16 package classes and accepts a 17th. `crates/sim/tests/hash_vectors.rs` and the golden pins should not move; if they do, the widening is wrong.

### What stays native

- **Runner rules and the tick pipeline.** Event order, auto-motion, collision masks and resolution, 1 px query rules, pair order, room changes, number rules, RNG and sound. Changes are `engine_rev` branches or `SIM_VERSION` bumps ([versioning](../architecture/versioning.md)).
- **Shared built-in behavior.** The 50 kinds and their params. A script adds events to a base; it does not rewrite the base.
- **The kid's existing actions and collision runs.** `walk`, `jump`, `fall_cap`, `vines`, `slope_*`, `block_standard` and the rest are fidelity-pinned and keep their behavior under `engine_rev`. Scripts compose with them at slots; they do not reimplement them.
- **Collision resolution and the load-time kind model.** Classes, event sets, derived kinds and `object_order` are fixed at load.
- **The world, hash, replay and server verification.** No script, data file or pack changes how a run is hashed or verified.

### The script budget and the stress gate

The hook does not change the budget. It uses the same numbers as [0001](0001-script-language.md#step-budget): 1,000,000 operation units per tick (a package may raise it to 2,000,000), 200,000 per call, 256 spawns per call, 4,096 per tick, 50,000 live instances, and the CI rule that a package's measured peak stays under 25% of its tick budget. A kid slot is one script call per event per kid; a per-bullet `step()` is the expensive case and is what the stress packages measure.

The gate the model must meet, before mass porting:

- The avoidance stress package (#990) at 500, 2,000 and 5,000 live bullets, in both variants (native motion, and a `step()` on every bullet), runs at 50 ticks per second on native release and wasm32 on the reference PC.
- The boss stress package (#991: phases, HP, alarms, a timeline) runs at the same rate.
- If the worst case misses the gate, the response is to promote the native pattern (for example the `projectile` decision in #775), not to raise the budget or move the kid's physics into scripts.

The measured milliseconds per tick and peak budget units belong in [0001](0001-script-language.md) (Revisited), recorded by #1002. They are not in the repository yet; this decision sets the gate, not the numbers.

### The user check: a dash

A creator writes a slot and a script. In `physics.toml`:

```toml
preset = "A1"

[script]
file = "kid"
vars = ["dash_dir"]

[events]
step = [
  "gravity", "walk", "air_sprite", "fall_cap", "shoot", "jump", "release", "vines",
  { do = "script", fn = "dash" },
]
keys = [
  { action = "extra1", on = "press", run = [{ do = "script", fn = "start_dash" }] },
]
```

In `scripts/kid.rhai`:

```rhai
const DASH_SPEED = 12.0;
const DASH_TICKS = 12;

fn start_dash() {
    this.dash_dir = this.image_xscale;   // -1 or 1; declared in [script] vars
    this.set_alarm(0, DASH_TICKS);
}

fn dash() {
    if this.get_alarm(0) > 0 {
        this.hspeed = DASH_SPEED * this.dash_dir;
        this.djump = false;
    }
}
```

No client release. The package's `gameplay_hash` changes, so it is a new leaderboard entry, and the trace/fidelity test proves the rest of the game is unchanged. If the dash needed a new GM collision rule that no query expresses, that would be a runner rule and a client release. Nothing in between requires Rust.

### Follow-ups

- #1074: kid script slots and the documented kid field API.
- #1075: raise the package class limit from 16 to 96 (`ClassMask` `u128`).
- #1076: survey and move one-game params out of shared built-ins.

## Consequences

- **Creators get a no-release path for new movement.** A dash, wall jump, grapple or custom gravity is a `[script]` table and Rhai, composing with the family's actions. The core stops absorbing one-game movement quirks.
- **The API surface grows in a bounded way.** The kid field table, the slot syntax and `[script] vars` are documented, validated and pinned. Every new field or slot is a deliberate decision, not an accident of a Rust layout.
- **Existing games do not move.** The class widening is byte-identical for every existing package; the slot syntax is opt-in; no behavior changes under an existing `gameplay_hash`. Format v0 is a draft, so the `physics.toml` additions need no migration; from format 1 they would.
- **Ranked integrity stays with the existing tools.** A script edit changes `gameplay_hash`; a built-in fix branches on `engine_rev` or bumps `SIM_VERSION`; the golden corpus and stored replays still verify.
- **Review cost shifts to the promotion gate.** A new built-in or param must name three games or justify itself against a script/preset. The built-in pages already name games; the survey follow-up makes the rule enforceable.
- **Risk: scripts can reimplement the kid and lose fidelity.** The model permits it as a custom engine. The defense is the trace/fidelity process, not a hard ban; a later validator warning on a profile that drops core actions can be added if it happens.
- **Two-game sharing is not promotion.** It is the shared script library (#797). This keeps the built-in set small and the shared code versioned per package.

## Open items

- **The census (research#172) may revise the numbers.** The 96-class bound and the three-game threshold are the defaults; revisit them when the census counts the one-game objects and the scripted kinds a port really needs. A lower or higher bound is an ordinary edit to this record and the follow-up issue.
- **The stress numbers are not measured yet.** #1002 records them in [0001](0001-script-language.md). The gate here is 50 ticks per second; the headroom is whatever the measurement issue reports.
- **Held-input mechanics wait on #776.** A press-triggered dash works through `events.keys` today. A held-dash, a menu or a quiz needs `pressed`/`held`/`released`, which #776 owns.
- **Kid script variables are declared, not inferred.** `[script] vars` is the v1 mechanism. If #997 makes array-valued variables script-visible, revisit whether a variable can be declared by writing it; the default stays explicit.
- **`jump_speeds` writes** wait for arrays (#997) or a dedicated `set_jump_speeds` function. The default is read-only.

## Evidence

Checked 2026-11-20 against the code at `origin/main` 6968bab:

- Kid: `crates/sim/src/kid/mod.rs` (`KidAction`, `KidCollisionRun`, `run_list`, `on_event`), `crates/sim/src/kid/actions/`, `crates/sim/src/kid/collisions/`, `crates/sim/src/kid/state.rs` (`KidState`, `KidGlobals`), `crates/sim/src/kid/api.rs`, `crates/sim/src/def/physics.rs` (175 `pub` items; `KidEvents`), `crates/sim/src/def/physics_ext.rs` (one-game switches).
- Built-ins: `crates/sim/src/def/builtin.rs` (50 package kinds plus `probe`/`inert`), `crates/sim/src/builtins/` (`save.rs` `requires_kid_in_room`/`alarm_save`/`floor_nudge`, `gravity_flip.rs` `yflag_with_view_rotation`, `water.rs`), `docs/format/builtins.md`.
- Scripts: `crates/sim/src/script/inst.rs` (generic properties; custom variables only from `GameDef::var_names`), `crates/sim/src/script/api.rs`, `crates/sim/src/script/validate.rs`, `docs/architecture/sim/scripts.md`, [0001](0001-script-language.md).
- Classes: `crates/sim/src/def/class.rs` (bit layout, `PACKAGE_COUNT = 16`), `crates/format/src/load/kinds.rs` (first-use assignment, "at most 16"), `crates/sim/src/world/instance.rs` (`Instance.classes`), `crates/sim/src/hash.rs` (`HashView.instances`), `crates/format/src/load/hash.rs` (`gameplay_hash` over package files), `crates/sim/src/replay.rs` (`final_hash`, checkpoints, header).
- postcard 1.1.3 `src/varint.rs` (`varint_u64`, `varint_u128`, the same LEB128 loop), `Cargo.lock`.
- Versioning: `crates/sim/src/rules/mod.rs` (`MAX_ENGINE_REV`), `docs/architecture/versioning.md`, `docs/format/versioning.md`.
- Related issues: #775 (stress packages), #990 (avoidance package), #991 (boss package), #1002 (measurements), #776 (script API v1, `param`, input, kid view), #797 (shared script library), research#172 (census), #86 (community uploads), #89 (GameMaker Studio runner).
