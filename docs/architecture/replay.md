# Input and replay format

Part of [Architecture](../architecture.md).


### Actions and input frames

- Actions: `left`, `right`, `up`, `down`, `jump`, `shoot`, `restart`, `suicide`, `skip`, `accept`, `back`, `menu`, `pause` (P), `mute` (M; wanna and xia also need Ctrl held), `reset` (F2), and `extra1` to `extra4`, generic actions for games that need keys beyond the standard set (a game maps each to its own meaning; the player binds them). One `u32` bit each (bits 0 to 18, `Action::MASK` = `0x7_ffff`; the higher bits are reserved). Pause, mute and reset change sim state in the originals (kaenbyou's jump ignores a press while P is pressed, mute changes `sound_stop_all` calls, F2 restarts the game), so they are sim input. Quitting (Escape, Alt+F4) stays client-only.
- `InputFrame { held: u32, pressed: u32, released: u32 }`, one per tick. The edge bits keep sub-tick taps: GM8 sees a press and a release inside one frame as both events in the same step, which gives a different jump than holding for one tick.
- The client collects raw OS input events between ticks into these bits. Games map actions, never raw keys.
- Each kid rule set defines how it reads the bits (for example YoYoYo versions differ on sub-frame taps).

### Replay file (`.iwbr`)

A postcard-encoded, versioned struct. Postcard is a small, documented, stable binary encoding, so we do not hand-write a codec.

- **Header:** magic `IWBR`, `replay_format: u16`, `sim_version: u32`, package id, `gameplay_hash: [u8; 32]`, `engine_rev: u16`, rng seed `u64`, start mode (`NewGame` or `FromSave(bytes)`; only `NewGame` is ranked), client build id.
- **Body:** run-length encoded input: a list of `(run_length: varint, InputFrame)`.
- **Trailer:** total ticks, splits as `(stage index, tick)`, checkpoints as `(tick, [u8; 16])` (BLAKE3 state hash truncated to 16 bytes) every 500 ticks, and the final state hash.
- **Canonical.** The decoder re-encodes what it read and rejects the file if the bytes differ (for example two adjacent runs of the same frame). Identical inputs therefore always give identical bytes, which helps exact-copy detection.
- **Size.** An hour at 50 ticks per second is 180,000 ticks. RLE keeps that to a few KB, far under the server's 1 MiB cap.
- **Text form.** `iwbtg replay dump` prints one run per line for humans and diffs.

### Text inputs for tests (`.inputs`)

Tests and traces use the same line format as the research trace recorder:

```
frames 10-29: hold jump     # first jump, released on frame 30
frames 32-41: hold jump     # double jump
frame 109: end
```

The research files name original keys (`hold Shift`). `iwbtg trace run` maps keys to actions with the engine's default key map (for example Shift to `jump`). Our own test files use action names. Both compile to the same input frames.

### Verification

The server decodes the replay, checks bounds (ticks at most the game's `max_ticks`), looks up the package by gameplay hash, and runs the sim headless in a child process (no `draw`, no audio). It requires the end trigger, compares splits and the final tick with what was claimed, and stores the verified tick count and its own checkpoint hashes. It never trusts hashes from the client; they only help find where a replay desynced.

### Versions

- `SIM_VERSION` changes whenever a sim change could alter any outcome, including any Rhai upgrade.
- Fixes to a built-in that change outcomes keep the old path behind `engine_rev < N`, the way osu! keeps old scoring code. A package declares the `engine_rev` it was verified against. Each branch is covered by the corpus.
- For Rhai upgrades and other changes that cannot be branched, the server keeps old verifier builds for replays made with an older `SIM_VERSION`.
- Before any sim release, CI re-verifies the whole stored replay corpus.
