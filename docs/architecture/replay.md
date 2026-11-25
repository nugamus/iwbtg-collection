# Input and replay format

Part of [Architecture](../architecture.md).


### Actions and input frames

- Actions: `left`, `right`, `up`, `down`, `jump`, `shoot`, `restart`, `suicide`, `skip`, `accept`, `back`, `menu`, `pause` (P), `mute` (M; wanna and xia also need Ctrl held), `reset` (F2), and `extra1` to `extra4`, generic actions for games that need keys beyond the standard set (a game maps each to its own meaning; the player binds them). One `u32` bit each (bits 0 to 18, `Action::MASK` = `0x7_ffff`). Adding an action is a new bit, never a renumber; the higher bits are reserved. One shared action-name table (`Action::ALL`) is read by the sim, the client device map and the `.inputs` compiler. Pause, mute and reset change sim state in the originals (kaenbyou's jump ignores a press while P is pressed, mute changes `sound_stop_all` calls, F2 restarts the game), so they are sim input. Quitting (Escape, Alt+F4) stays client-only.
- `InputFrame { held: u32, pressed: u32, released: u32 }`, one per tick, is the **only** input wire contract between the client, a replay and the sim. The **client collector** derives it from device events between ticks; the **sim latches** it (`InputState::latch`) and never re-derives an edge from held state. A replay and `replay verify` store and check frames, not keys; the `.inputs` text form compiles to the same frames.
- The edge bits keep sub-tick taps: GM8 sees a press and a release inside one frame as both events in the same step, which gives a different jump than holding for one tick. `held` is the OR of the inputs down at the sample; edges are per input, so a second key bound to an already-held action sets the action's press bit again, and releasing one of two keys sets the release bit while the action stays held (`input/second_key_edge`).
- The frame is action bits only. `InputFrame::masked` clears anything outside `Action::MASK` at the sim's latch boundary, so a device, a replay or a caller cannot pack a raw key code or a reserved bit into the sim. Games read actions, never raw keys.
- The plain contract above is the `standard` ruleset and every existing package. Per-engine edge behavior is a **named** ruleset/profile value only where a trace proves a player feels it: `TickRules::direct_latch` (register I1) is the one such value today, and the recorder's traces cannot settle it; xia's `tap_fix` remains a documented research gap ([kid](sim/kid.md#tricks-switches)), not a promised profile value. A port without that evidence uses the plain contract; a package selects a ruleset/profile, it does not supply an edge rule (#1160).
- **Control latency (F1):** a press the player makes between two ticks is in the next frame, so the sim responds within `CONTROL_LATENCY_MAX_TICKS` (one) tick. The same tick still sees the same press, and the same release cuts the jump (feel contracts `input/jump_on_press`, `input/release_height`, `input/sub_frame_tap`).
- **Client-only inputs.** The client maps devices to actions and owns its own pause menu, quick-retry, auto-retry, skip-cutscene, frame advance, slow motion and quit; none enters `InputFrame` or a replay. The pause menu takes no frame while open; on close it drops the edges collected inside it and keeps keys that are still held (`input/pause_edges`). The reset/auto-retry policy: R (`restart`) and F2 (`reset`) are sim actions and are in the replay; quick-retry and auto-retry start a new run as a new replay and never enter the current one ([product](../product.md), Restart keys and Pause). Escape is not bound to `menu` by default (#758).

### Replay file (`.iwbr`)

A postcard-encoded, versioned struct. Postcard is a small, documented, stable binary encoding, so we do not hand-write a codec.

- **Header:** magic `IWBR`, `replay_format: u16`, `sim_version: u32`, package id, `gameplay_hash: [u8; 32]`, `engine_rev: u16`, rng seed `u64` (the seed of every named stream: the ruleset picks the layout and `rng.md` derives the other streams from it, #1157), start mode (`NewGame` or `FromSave(bytes)`, the postcard bytes of the sim's `save::Checkpoint`, the same versioned encoding the client's save file holds; format 7, #1150. Only `NewGame` is ranked), play mode (`Original` or `Stripped`, `iwbtg_sim::stripped::PlayMode`; only `Stripped` is ranked; format 3, #561), map id (the map the run played, `Sim::map_id`: its timing chain and, in a stripped run, its ranked start; `main` for a package without `[[maps]]`; leaderboards key on it; format 5, #505), mods (a `u32` bit set, see Mods below; format 2), client build id.
- **Ruleset and compatibility profile** are not separate header fields: they enter `gameplay_hash`, so every replay is already bound to the rules it was played under. A replay on `gm8` verifies on a build that still implements `gm8`; a replay on `standard` verifies on a build that implements `standard` ([versioning](versioning.md)). A checkpoint in `FromSave` ships with format 7 ([0005](../decisions/0005-standard-mechanics.md)); the consumer keeps reading the raw `SaveSlot` bytes of formats up to 6, whose replay's `gameplay_hash` binds the numeric room to that package. The checkpoint's room is a stable name, so a format-7 `FromSave` survives a room reorder; the payload travels to `StartPoint::FromSave` unresolved and the sim looks the name up when it starts (#1238). The checkpoint also names its save object (`point`: a declared name or exact spawn position); a replay whose point no longer resolves is refused rather than started at a different instance. The payload is decoded exactly (`Checkpoint::decode`, or `SaveSlot::decode` below format 7), so bytes appended after it are refused, and the checkpoint runs the same `Checkpoint::validate` the client's save path uses: a foreign checkpoint format is rejected identically by both (#1325).
- **Body:** run-length encoded input: a list of `(run_length: varint, InputFrame)`. Frames are what the player gave; a dialogue's timeout press is derived by the sim on replay and never stored (#34).
- **Trailer:** total ticks, splits as `(stage index, tick)`, ranked ticks (ranked time: the timer's ticks outside every cutscene and dialogue region, `TimingState::ranked_ticks`, at most total ticks; format 4, #34), pauses as `(tick, ticks paused)` (the client's pause menu while the run's timer ran, #479: the sim tick it opened after and the fixed-clock ticks the timer counted while it was open; ticks strictly increasing, none past total ticks, each at least 1 tick long; format 6, #623), checkpoints as `(tick, [u8; 16])` (BLAKE3 state hash truncated to 16 bytes, taken right after the step that brings the tick count to a multiple of 500), and the final state hash (after the last input's step).
- **Time.** The sim never sees the pause menu (it does not tick while the menu is open), so ranked ticks are the sim's alone. The run's time, which the HUD showed and the stats record, is ranked ticks plus the ticks paused (`Trailer::ranked_time`). Pausing can only make a time longer, so the verifier takes the pauses as claimed.
- **Older files.** `decode` also reads format 5 (`LEGACY_FORMAT`, the layout before `pauses`): its pauses are empty and its header keeps `replay_format` 5. `encode` writes only the current format. Formats before 5 are refused.
- **Canonical.** The decoder re-encodes what it read and rejects the file if the bytes differ (for example two adjacent runs of the same frame). Identical inputs therefore always give identical bytes, which helps exact-copy detection.
- **Size.** An hour at 50 ticks per second is 180,000 ticks. RLE keeps that to a few KB, far under the server's 1 MiB cap.
- **Text form.** `iwbtg replay dump` prints the header and trailer as `name: value` lines, then one input run per line, for humans and diffs (docs/architecture/cli.md).
- **Recording.** The client records every run (docs/architecture/client/replays.md; #476); `iwbtg replay verify` checks a recorded file the way the server will.

### Mods

`iwbtg_sim::mods` holds the mod table (`MODS`: code, name, one-line description, kind for the mod select sheet, ranked flag) that the client and the UI (#497, #495, #37) read, and `ModSet`, the header's `mods` field: bit `n` is the mod whose `Mod` discriminant is `n`. A bit, once given, keeps its meaning forever; `encode` and `decode` reject a bit no mod owns (`UnknownMods`).

| Bit | Code | Mod | Kind | Ranked |
|---|---|---|---|---|
| 0 | `SD` | Sudden Death | Challenge | yes |
| 1 | `HB` | Hitboxes | Visual | yes |
| 2 | `NB` | No background | Visual | yes |
| 3 | `SS` | Save states | Practice | never |
| 4 | `SL` | Slow motion | Practice | never |
| 5 | `FA` | Frame advance | Practice | never |

- A run is ranked only when it starts a new game and every mod in it is ranked. `ModSet::unranked_by` gives the first mod that is not, for the Unranked pill's reason. Each combination has its own board, named by `ModSet::codes` (`SD+NB`, `NM` for none).
- No mod changes what a tick computes, so replays need no mod-specific sim path. Sudden Death only fixes where a run ends: the tick that raises the death counter is the last one (`ModSet::ends_run`). The client stops there, so its replay ends on that tick, and the verifier (`iwbtg replay run`) rejects a Sudden Death replay with inputs after it.
- Slow motion and frame advance change only the client's clock; save states break the input-only replay and are practice anyway.

### Text inputs for tests (`.inputs`)

Tests and traces use the same line format as the research trace recorder:

```
frames 10-29: hold jump     # first jump, released on frame 30
frames 32-41: hold jump     # double jump
frame 109: end
```

The research files name original keys (`hold Shift`). `iwbtg trace run` maps keys to actions with the engine's default key map (for example Shift to `jump`). Our own test files use action names. Both compile to the same input frames.

### Verification

The server decodes the replay, checks bounds (ticks at most the game's `max_ticks`), looks up the package by gameplay hash, and runs the sim headless in a child process (no `draw`, no audio). It requires the end trigger, compares splits and the final tick with what was claimed, and stores the verified tick count, the ranked time (its own ranked ticks plus the replay's pauses) and its own checkpoint hashes. It never trusts hashes from the client; they only help find where a replay desynced.

The header's `sim_version` is checked before the run: a replay recorded on another build is reported as a `sim_version` mismatch and verification stops there. Its recorded hashes are that build's, so comparing them here would print a hash difference that hides the real cause (`iwbtg replay verify` does the same offline).

### Versions

The full contract (what a board keys on, the freeze point, the bump check, old verifier builds) is [versioning](versioning.md).

- `SIM_VERSION` changes whenever a sim change could alter any outcome, including any Rhai upgrade and any state-hash encoding change (`HASH_LAYOUT`; layout 3 is `SIM_VERSION` 2).
- A ruleset or profile move is content-level: a new `gameplay_hash` and a new board, with the old board and its replays kept on the old build. It is not a `SIM_VERSION` bump and it never silently changes a ranked outcome ([0005](../decisions/0005-standard-mechanics.md)).
- Fixes to a built-in that change outcomes keep the old path behind `engine_rev < N`, the way osu! keeps old scoring code. A package declares the `engine_rev` it was verified against. Each branch is covered by the corpus.
- For Rhai upgrades and other changes that cannot be branched, the server keeps old verifier builds for replays made with an older `SIM_VERSION`.
- Before any sim release, CI re-verifies the whole stored replay corpus.
