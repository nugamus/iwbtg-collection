# 0005: Standard mechanics and feel-based fidelity

Status: Accepted, 2026-11-21. Umbrella #1146; the proposal issue is #1147, closed by [PR #1148](https://github.com/nugamus/iwbtg-collection/pull/1148). The user answered all seven open decisions on 2026-11-21 (see [Decision log](#decision-log)); this record is normative from that date, and no behavior changes until each child ships behind a ruleset/profile with evidence.

Related: [0004 extension model](0004-extension-model.md), [product](../product.md) (Look and screen, Restart keys), [content](../content.md#porting) (Porting, Fidelity), [fidelity tiers](../plans/fidelity-tiers.md), [versioning](../architecture/versioning.md), [replay](../architecture/replay.md), [traces](../architecture/traces.md), [sim core](../architecture/sim.md).

## Context

The collection ports games from several GameMaker engines. The engine has reproduced their behavior by modelling each engine's implementation where the spec could observe it: a boot room and a temp marker for the retry, one save-position table per family, one action list per family, one dispatch/pair-order variant per engine, one comparison mode per engine, `room_speed` as a clock, and the original's shared RNG stream for visual effects.

That was the right default while the goal was "reproduce the trace". It stops being the right default as the collection grows:

- **The implementation leaks into places players and creators pay for.** #1141 added an empty boot room to the showcase because A1's retry is `GameRestartViaBoot`. The player-visible behavior is "R returns to the save with the room reset"; the boot **room**, the temp file and the package plumbing are implementation, but the N/N+1/N+2 tick sequence on the R tick is a feel contract (CLONE_SPEC 1174). The defect is that a **package** room encodes the delay: adding it shifted numeric room ids and broke user saves (#1144/#1145).
- **The code has paths no package uses.** The collision pair walk was optimized twice for instance density (#1139/#1143); the alternate pair, dispatch and room orders are test-only, yet they also carry grid support and maintenance cost. Production runs one order, so the target is to freeze it and delete the dead switches. `def/physics.rs` has ~175 public items across seven families; `physics_ext.rs` carries one-game switches.
- **The extension model already rejects the endpoint.** [0004](0004-extension-model.md) says a shared built-in must be general, and a one-game quirk is data or a script. The same argument applies to mechanics themselves: shared code should be one model with data, not the union of every engine.
- **The ports are not all equal witnesses.** A trace proves positions on the traced path. It does not prove that every probe column, boot tick, internal id or visual particle draw is a rule a player relies on. Treating all of it as sacred blocks redesigns that no player would notice.

The user's direction (2026-11-20):

> "We should take choices that are more modern and better for our engine. It seems really stupid to match original engine behavior as from a gameplay area it doesn't seem to change anything and just makes our code more complicated. Our engine should be able to match gameplay feel while modernizing and standardizing a way for people to port games into our engine and make new games. We should rethink all the engine mechanics, even ones that impact gameplay, so long as they feel the same, to design them way better."

## Options considered

### The fidelity contract

| Option | Verdict | Reason |
|---|---|---|
| Trace equality is the contract for every mechanic. | Rejected as the only contract | It cannot distinguish a player-relied outcome from a probe column, an internal id or a boot room. It makes every redesign a trace fight and every new engine a clone. |
| Feel is the contract; traces are one kind of evidence. | **Chosen** | A timing window, position envelope, collision outcome, control latency or playtest is what a player can observe. Traces remain the strongest evidence for F1 mechanics. |
| Only playtests decide. | Rejected | Slow, unrepeatable, and cannot gate CI. Playtests validate contracts; they do not replace them. |

### The standardization model

| Option | Verdict | Reason |
|---|---|---|
| Keep per-engine code branches and presets. | Rejected | The union grows with every port; #1076 already has to demote one-game params. |
| One ruleset only; every game uses modern defaults. | Rejected | It would silently change ranked outcomes for existing ports. |
| One standard ruleset for new games plus data-only compatibility profiles for ports. | **Chosen** | New work is simple; existing feel is preserved without code forks; every deviation is declared and testable. |
| A code fork per engine pack. | Rejected | Unverifiable and unmaintainable; violates format principle 1 (no compiled per-game code). |

### The lifecycle

| Option | Verdict | Reason |
|---|---|---|
| Keep the package-owned boot room and temp file. | Rejected | The room is F3 implementation that leaks into room ids, saves and content authoring (#1141/#1144). |
| An engine-owned lifecycle with the same N-tick init phase; no package boot room. | **Chosen** | The player sees the same "R → death counted, N-tick pause, spawn at the checkpoint, music as before". The N/N+1/N+2 timing is F1 evidence; the room is F3 and disappears from content. |
| A scripted lifecycle per game. | Rejected | Per-tick lifecycle code is ranked-critical and repeats across 15 games. |

## Decision

### The feel taxonomy

Every mechanic gets one tier, recorded on its issue, deviation log and register row:

| Tier | Meaning | Gate |
|---|---|---|
| **F1 feel-critical** | Can change what a skilled player can do: position, death, timing, RNG on a played path, input response, respawn control. | Needs a feel contract and, for a redesign, evidence and the user where irreversible. Blocks Ranked and the milestone's F1 done-when. |
| **F2 feedback** | Only visual or audio, but affects learning or clarity. | Needs a contract and screenshot/audio evidence. A visible difference blocks Beta. |
| **F3 invisible** | A player cannot observe it: a package-authored boot room, a temp file, an instance id, numeric quantization, an internal event order. | Free to redesign. Determinism, hashes and existing replays still apply. |

This is the same question the fidelity tiers ask, asked of every mechanic rather than only of an open spec question. The mapping and legacy aliases are in [fidelity tiers](../plans/fidelity-tiers.md).

### The ruleset and profile

- **Ruleset** is the named, versioned semantic contract: tick pipeline, lifecycle, motion, collision, numbers, input, scheduling, RNG and sound, **including the built-in behavior revision**. `gm8@1` is the compatibility ruleset; `standard@1` is the modern default for new games. The code implements each ruleset; a package selects one, it does not supply one (0004).
  - Through format 0, `engine_rev` remains the field name for the revision and `ruleset = "gm8@<rev>"` is its explicit form; `iwbtg engine check` moves the revision. At format 1, `ruleset@N` supersedes `engine_rev`.
  - `standard` is the target for ports that have proved each move: an engine-owned retry without a package boot room, exact checkpoint positions, separate effects RNG, tick-based scheduling, one dispatch/collision order.
- **Compatibility profile** (`[compat] profile = "<ruleset>/<family>@<n>"`, for example `gm8/a1@1`) is package data that names one of the engine's **pre-defined** compatibility bundles, plus the declared deviations. It cannot supply runner-rule values: numeric tolerances, dispatch order, stream layout and the other `RunnerRules` fields belong to the ruleset, not to package data. A value the engine does not implement is a ruleset change and a client release.
- **`physics.toml`** holds the per-game tuning values and event lists, as overrides on the profile.
- **`engine_pack`** is the shared asset bundle and an authoring suggestion; today it has no runtime physics influence. The package copies the profile it wants.

The proposed package declaration is in [Package shape](#package-shape). An absent `ruleset`/`[compat]` in a format-0 package means `gm8` at its `engine_rev`, with the compatibility profile inferred from the physics preset. That keeps every existing `gameplay_hash` and replay unchanged. Format 1 makes the fields explicit and requires every deviation to name its catalogue id.

### One lifecycle

The sim has one lifecycle with explicit events:

| Event | Meaning | Policies |
|---|---|---|
| `new_game` | Start a run from the package's `new_game_room` and `[stripped].start`. | Reset scope `run`; spawn `room_start`; checkpoint write policy. |
| `enter_room` | Change room and run the room sequence. | Persistence scopes; per-room physics; music/effects. |
| `death` | The kid dies. | Counters; save policy; death sound; auto-retry; GAME OVER. |
| `retry` | The player presses R (or F2/reset). | Reset scope (`room`/`save`/`run`); spawn `checkpoint`; control delay in ticks; music/effects; checkpoint write. |
| `quit` | The client leaves the run. | Client-only; no sim state. |

The standard retry contract: **on retry the player controls a Kid at the checkpoint after `retry_delay_ticks` ticks; the room's placed instances are rebuilt; the declared counters persist.** The delay is an engine-owned init phase, not a package-authored room: there is no package-authored boot room and no temp marker, and the checkpoint does not load through an init room. The N/N+1/N+2 tick sequence is preserved because it is observed on the R tick (CLONE_SPEC 1174); the room is F3 and disappears from content. The recommended implementation is an engine-owned `@init` room (role Boot) appended by the loader, so `game_restart` never targets `RoomId(0)`; the alternative is a lifecycle phase with the same tick schedule. Existing games keep their felt sequence through a named profile: the death counter, save write, delay, spawn position and music behavior are the same.

### One checkpoint

One versioned checkpoint replaces `SaveSlot` on disk and in `StartPoint::FromSave`:

- stable **room name** and optional checkpoint name, never a numeric `RoomId`;
- position as exact `f64` plus a declared `save_precision` policy (`exact` for `standard`; the family quantization modes are selected by the named `gm8` compatibility profile where they change the respawn alignment);
- facing, flip state, difficulty, deaths, time, bosses/items/clear flags and the ruleset plus compatibility-profile id;
- atomic write (temp + rename) and a format version; old files migrate or are refused with one warning and a new game;
- the replay header stores the same checkpoint bytes; the server parses them for verification and never trusts client hashes.

### One movement model

The standard model composes a small set of movement concerns — walk/run, jump/double/variable jump, gravity/flip, vines, ladders, ice, slopes, water, platforms — with standard parameter names and per-game tuning. The existing family presets become tuning data. Unique movement keeps a script slot at a fixed event position ([0004](0004-extension-model.md), #1074), not a new cloned action list. The native hot path stays native.

A family may keep a different **step order** when the order is a player-visible feel difference; that order is physics-profile data over the standard actions. A family that reimplements an action keeps the native action; the profile chooses parameters, not code.

### One dispatch and collision order

The standard order is one deterministic walk:

- `object_order` rank, then **instance id**, for event dispatch. For a placed instance the id may be the GM8 `gm_id`, not the room-file creation ordinal; whether the two differ is OPEN (#1079), and the current behavior is kept until a trace settles it.
- the same order for the collision phase: outer instance by rank then id, inner by rank then id, both directions per pair, one most-specific handler (today `ClassMask::generality()`, to become data whose defaults reproduce it);
- draw order is depth descending, then id, in its own explicit layer order;
- classes are collision data; layers are draw data; both are declared, not inferred from numeric ids.

The production walk is one order. The alternate `PairOrder`, `RoomStartOrder`, `RoomEndOrder`, `JoinPhase` and dispatch values are test-only today; they are deleted after their evidence moves into `tests/pairs.rs`/`tests/world.rs`, or become named compatibility-profile values only if a feel contract shows a port needs one. The #1139/#1143 gather is kept.

### One numeric policy

IEEE binary64 everywhere. Exact operations stay exact. A tolerance exists only where a feel contract needs it (collision edges, triggers, save alignment). The standard ruleset uses one explicit tolerance mechanism, with each value declared at its use; `gm8` keeps `NumRules` as it is. A package selects the ruleset; it does not supply a tolerance. Serialization uses shortest round-trip; save precision is a checkpoint policy, not a comparison mode. No engine-specific comparison mode is added to shared code.

### One input contract

`InputFrame { held, pressed, released }` over the action bit set remains the contract. The client collector derives the frame; the sim latches it, and the client maps devices and owns pause, quick-retry, auto-retry and quit. Per-engine sub-frame tap and direct-latch behavior is named compatibility-profile data where a trace proves it is feel-visible, and disappears otherwise.

### One scheduler

Ticks are the truth. `room_speed` becomes a legacy per-room data field for `gm8`; `standard` runs at the package tick rate (50 Hz by default) and derives seconds only at the UI and audio boundary. Alarms, timelines, paths and animation advance from one scheduler with explicit tick units. Existing felt timings are preserved by the profile.

### Named RNG streams

The standard ruleset has named streams: `gameplay`, `effects` (blood, particles, playerface), `draw`, and `script` where a script asks for randomness. Effects never draw from `gameplay` in `standard`; a player-visible outcome that depends on an effect draw is either moved to `gameplay` deliberately or the effect is reworked. `gm8` keeps the single shared stream and its exact draw order, because splitting it can change later enemy behavior (F1). The replay header records the seed; the stream layout and each stream's state are hashed.

### One sound model

The sim emits deterministic sound events (`play`, `stop`, `stop_all`, `volume`, `pitch`) with sound ids and tick timings; the client mixer owns latency, fades and output. `sound_isplaying` remains sim state. `DllMode`/SuperSound/GMFMODSimple adapters become named `gm8` compatibility-profile values; `standard` uses the event model directly. Music sync stays tick-authoritative.

### A small standard built-in set

The shared kinds reduce to standard behaviours — spawn, checkpoint, portal, trigger, hazard, pickup, overlay, emitter, mover — with presets and scripts for the rest. One-game parameters do not enter the shared set; the promotion/demotion rule is [0004](0004-extension-model.md)'s. Structural changes that do not alter a package's behavior are F3; any behavior change is F1 and needs a contract.

### Feel contracts

A **feel contract** is data: a scenario plus assertions over what the player can observe. It complements, and does not replace, determinism tests, golden hashes and trace diffs.

- **Scenario:** package or fixture, start point, inputs, mods, seed, room changes.
- **Assertions:** position envelope, timing window, death/time/counter outcome, reset scope, control latency, sound/room event, screenshot.
- **Tolerance:** exact for state hashes; a declared window or envelope for feel. Every tolerance has a reason.
- **Where it lives:** `crates/tools/feel/<area>/*.toml` (names, no creator content) beside the trace cases; fixtures the public CI can run live in `crates/sim/tests/feel/`.
- **How it runs:** `cargo xtask feel` runs the public set; with `IWBTG_RESEARCH` it runs the port contracts too. CI runs the public set on every sim/format change. A contract that changes must say so in the PR and the deviation log.
- **Relation to traces:** a trace diff is evidence for an F1 mechanic. A feel contract states the outcome that must hold even when a probe column or an F3 detail differs.

The concrete format and runner are specified in the harness child issue and [feel contracts](../architecture/feel-contracts.md).

### Package shape

Proposed additions for format 1 (not implemented by this record):

```toml
# A new game
format = 1
ruleset = "standard@1"
start_room = "room_1"
physics = "physics.toml"
# no runner, engine_rev, object_order or room_order for a new game

# A port
format = 1
ruleset = "gm8@1"
engine_pack = "a1"
physics = "physics.toml"

[compat]
profile = "gm8/a1@1"

[[compat.deviation]]
id = "save.position"          # from the ruleset's closed catalogue
value = "floor_nudge"
why = "the original's save format"
impact = "F1"
evidence = "trace:xia/tr-room-07-a"
issue = 1234
```

The ruleset ships a closed deviation catalogue; `pkg check` rejects an unknown id, a wrong type or missing provenance. A deviation selects an existing enum/bool and cannot add native code. The loader validates the profile against the ruleset's known bundles; an unknown name is an error. `ruleset`, `[compat]` and every field that changes behavior enter `gameplay_hash`, so a behavior change is a new board entry, never a silent edit. A package that only names existing behavior is normalized to the same hash as the absent field during format 0.

### Workstreams

The umbrella is [#1146](https://github.com/nugamus/iwbtg-collection/issues/1146); the proposal issue is [#1147](https://github.com/nugamus/iwbtg-collection/issues/1147).

| Area | Issue | First move |
|---|---|---|
| A lifecycle | [#1149](https://github.com/nugamus/iwbtg-collection/issues/1149) | Engine-owned init phase; no package boot room; the N-tick delay stays. |
| B checkpoint | [#1150](https://github.com/nugamus/iwbtg-collection/issues/1150) | Versioned checkpoint with a stable room name. |
| C physics | [#1151](https://github.com/nugamus/iwbtg-collection/issues/1151) | Standard parameter schema over the family presets. |
| D collision | [#1152](https://github.com/nugamus/iwbtg-collection/issues/1152) | One walk with data-driven order keys. |
| E identity | [#1153](https://github.com/nugamus/iwbtg-collection/issues/1153) | Stable room/entity identity; runtime ids stay runtime. |
| F numbers | [#1154](https://github.com/nugamus/iwbtg-collection/issues/1154) | One tolerance policy as a ruleset value. |
| G input | [#1155](https://github.com/nugamus/iwbtg-collection/issues/1155) | One edge contract; per-engine rules as named profiles. |
| H scheduling | [#1156](https://github.com/nugamus/iwbtg-collection/issues/1156) | Ticks are the clock; `room_speed` is legacy data. |
| I RNG | [#1157](https://github.com/nugamus/iwbtg-collection/issues/1157) | Named streams; effects separate in `standard`. |
| J sound | [#1158](https://github.com/nugamus/iwbtg-collection/issues/1158) | Event model; DLL adapters as profiles. |
| K built-ins | [#1159](https://github.com/nugamus/iwbtg-collection/issues/1159) | Standard behaviours plus presets/scripts. |
| L package | [#1160](https://github.com/nugamus/iwbtg-collection/issues/1160) | `ruleset@N`, `[compat]`, deviations. |
| M harness | [#1161](https://github.com/nugamus/iwbtg-collection/issues/1161) | Feel-contract runner and feel tiers. |
| N guidance | [#1162](https://github.com/nugamus/iwbtg-collection/issues/1162) | Deviation log, templates and review checklist. |

## Migration phases

1. **Phase 0 — decision and evidence (this proposal).** Decision record, guidance, feel-tier mapping, harness specification, issues. No behavior change.
2. **Phase 1 — F3 quick wins.** Engine-owned init phase without a package-authored boot room (keep the N-tick delay and spawn outcome), stable room/entity identity in saves, internal ordering, dead-code and duplicate removal. Every step is proven by unchanged golden pins and a feel contract.
3. **Phase 2 — feel-neutral standardization.** Standard parameter names and a compositional movement model; one dispatch/collision core with data-driven order; `ruleset@N` + `[compat]` parsing; checkpoint v2; input/tick/RNG/sound models behind named profiles. Existing packages keep their exact behavior and hashes.
4. **Phase 3 — F1 redesigns.** Only with evidence and a new ruleset/compatibility profile. Each ships with a board/replay migration: old rulesets remain verifiable, old boards remain, new runs get a new `gameplay_hash`.

Each phase has child issues with size, dependencies and a feel contract.

## Consequences

- **New games get the simple model.** No package-authored boot room, no per-engine action list, no numeric ids in saves.
- **Ports keep their feel without code forks.** A profile is data, hashed and reviewable; a deviation names its reason, impact and test.
- **Ranked integrity is unchanged.** A behavior change is a new ruleset/compatibility profile and a new `gameplay_hash`; old replays verify with the build/ruleset they were made on.
- **The evidence standard becomes clearer.** A trace column that no player can observe is no longer a design mandate; an F1 outcome still needs the strongest evidence available.
- **Short-term cost.** Two rulesets and a profile layer exist until the ports have proved their moves; the harness grows a contract runner; docs and lanes need one more rule.
- **Risk: "feel" becomes an excuse.** The defense is the taxonomy, the contract, and the user for anything irreversible.
- **Risk: profiles become the new code forks.** A profile may only set values the ruleset exposes. A new value is a ruleset change, reviewed as such.

## Decision log

**2026-11-21 — the user answered all seven open items and accepted every proposed default.** The answers are binding for the program; a later change to any of them is a new record, not an edit here.

1. **Principle and taxonomy.** Answered: approve the principle and the F1/F2/F3 taxonomy. Accepted: the feel-first principle and the three tiers above are normative, with T1–T3 kept as aliases ([fidelity tiers](../plans/fidelity-tiers.md)).
2. **Board key.** Answered: include the ruleset and profile in the board identity. Accepted: `ruleset` and `[compat]`/`profile` (and every behavior-changing field) enter `gameplay_hash`, so a behavior change is a new board entry; `SIM_VERSION` stays for build-level changes (#933).
3. **Format 1 shape.** Answered: accept the format-1 package shape. Accepted: `ruleset@N` + `[compat] profile` + declared deviations as in [Package shape](#package-shape), required at format 1; `engine_rev` remains the accepted format-0 alias, and absent means `gm8` at its `engine_rev` in format 0. Irreversible once format 1 freezes.
4. **Checkpoint v2.** Answered: accept checkpoint v2. Accepted: versioned and room-name based, exact `f64` position for `standard`, profile quantization for `gm8`, atomic write, and old saves migrate (or are refused with one warning and a new game).
5. **Which F1 redesigns may change feel.** Answered: evidence first. Accepted: no F1 redesign changes feel without a feel contract; every port keeps its `gm8` profile until each modern move is proved.
6. **Modern defaults for new games.** Answered: yes. Accepted: exact `f64` checkpoints, a separate effects RNG stream, no package-authored boot room, and tick-based scheduling are the `standard` defaults.
7. **Promotion threshold interaction.** Answered: keep [0004](0004-extension-model.md). Accepted: the three-game promotion rule stands; a standard behaviour promoted out of a profile needs three ports or a committed creator demand.

## Evidence

Checked 2026-11-20 against `origin/main`:

- Lifecycle: `crates/sim/src/restart.rs`, `crates/sim/src/room.rs` (`start_game`, `perform_change`, `game_restart`, `create_boot`, `load_slot`), `crates/sim/src/def/game.rs` (`RestartModel`, `RestartKind`, `boot_ticks`), `crates/client/showcase.pkg/rooms/boot.toml`, #1141, #1144, #1145.
- Save: `crates/sim/src/save.rs` (`SaveSlot`, `SavedPosition`, `quantize`), `crates/client/src/savefile.rs`, `docs/architecture/client/storage.md`.
- Physics: `crates/sim/src/def/physics.rs`, `crates/sim/src/def/physics_ext.rs`, `crates/sim/src/kid/presets.rs`, `crates/sim/src/kid/actions/`, #1074, #1076.
- Dispatch/collision: `crates/sim/src/events.rs`, `crates/sim/src/rules/collide.rs`, `crates/sim/src/collide/pairs.rs`, #1139, #1143.
- Numbers: `crates/sim/src/num/`, `crates/sim/src/rules/num.rs`, `docs/architecture/sim/numbers.md`.
- Tick/scheduling: `crates/sim/src/tick.rs`, `crates/sim/src/alarm.rs`, `crates/sim/src/timeline.rs`, `crates/sim/src/path.rs`, `crates/sim/src/anim.rs`, `crates/sim/src/room.rs`.
- RNG/sound: `crates/sim/src/rng.rs`, `crates/sim/src/particles.rs`, `crates/sim/src/sound.rs`, `docs/architecture/sim/rng.md`, `docs/architecture/sim/sound.md`.
- Harness: `crates/xtask/src/main.rs` (`fidelity`, `repin`), `crates/sim/tests/golden_corpus.rs`, `crates/sim/tests/determinism.rs`, `crates/tools/fidelity-baseline.txt`, `docs/plans/fidelity-tiers.md`.
- [0004 extension model](0004-extension-model.md), #778; versioning #783/#933; save bug #1144/#1145.
- Issues: umbrella #1146, proposal #1147, workstreams #1149–#1162. Prior art: #1076 (built-in survey), #1074 (kid script slots), #1093 (script retry prep), #1024 (client perf budgets), #1142 (debug/release tester build), #1140/#1141/#1144/#1145 (boot room and saves).
