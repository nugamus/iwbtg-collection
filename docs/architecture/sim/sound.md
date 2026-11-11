# The simulation: Sound model

Part of [the simulation](../sim.md).

## Sound model

Audio output is the client's job, but whether a sound **is playing** is sim state. 15 of the 20 games call `sound_isplaying` (CLONE_SPEC sound, Confirmed), and its result can change game state, not only audio: machinary's playMusic2 sets an instance flag from it, and Step-based music objects call `sound_stop_all` when it returns false (Inferred). The DLL getters matter too: the GMFMODSimple getters give the start value, and so the lifetime, of the fader objects in go-the-spook-house and keep-celebrating-for-xia, and the SuperSound volume getter gives the start of kaenbyou's and eat-the-lemon's death fade. So `sim::sound` keeps a model ([plan](../../plans/sim-core.md) section 7):

- Playing copies start at the call's tick and end after the sound's `length_ticks` (song length times tick rate, from the package). At most one MULTIMEDIA copy. `sound_isplaying` latency and the second MULTIMEDIA start are UNKNOWN switches in `SoundRules` (S1-S3); game_restart keeps sounds (Inferred, S4).
- SuperSound (kaenbyou, eat-the-lemon) and GMFMODSimple (go-the-spook-house, xia) are adapters whose getters return the last value set (S5). `SimOptions::dll_mode = Stub` reproduces the recorder's `--stub-dlls` traces; `Model` is for play and replays.
- The `mute` action flips the game's music flag through the world built-in; music objects then call `stop_all`, which is sim state.
- Built-in copies play the ticks `start .. start + length_ticks` (looping: until stopped). `sound::is_playing` computes that itself; tick row 16 (`sound::tick`) only drops ended copies, SuperSound sounds and FMOD instances. S1 (latency) delays only a start; S2 and S3 govern a second MULTIMEDIA start; S4 (`keep_on_restart`) is read by `game_restart`. The built-in model runs in `Stub` mode too: the recorder stubs only the DLLs.
- DLL adapters (`sound::{supersound, gmfmod, cleanmem}`) are called by the script host with the arguments of the GML `external_call`. SuperSound returns strings (`"1"`, `"0"`, `"-1"` for is-playing); FMOD returns numbers (0 is an error); in `Stub` mode every call returns `""` or 0 and changes nothing. The DLLs do not know a sound length, so the host passes `length_ticks` at load (0 = unknown, plays until stopped). OPEN: where the package supplies those lengths (follow-up).
- The state is hashed. The client receives `SoundOutput` events to play, stop and fade; nothing is read back from audio.
