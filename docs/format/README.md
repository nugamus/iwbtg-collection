# Game package format: principles

The full spec is written in roadmap Phase 3. These rules come first and every version of the format must keep them.

1. **Readable text, never compiled.** A package is plain text files plus media assets (images, audio). Rooms, objects, physics profiles, triggers and scripts are all text a person or an AI can read, diff and review. No per-game compiled code, no native libraries, no WebAssembly, no bytecode.
2. **Data first.** Common mechanics are built-in object types with parameters: spikes, platforms, saves, warps, vines, water, killers, triggers and so on. Each built-in is documented once in the spec and behaves the same in every game that uses it. Per-game differences are parameters or physics profiles, not new code.
3. **Scripts only as the escape hatch.** Behavior the built-ins can't express (bosses, avoidance patterns, unusual traps) goes in scripts written in our own sandboxed script language:
   - Scripts can only use the API we expose. They have no file, network, OS, clock or random-source access beyond what the simulation provides.
   - A budget of steps per tick, so a script can't hang the game or the server.
   - Deterministic: the same inputs give the same result on Windows, WASM and the Linux server. Replay verification depends on this.
   - A script error stops that game cleanly. It never crashes the client or the server.
4. **Validated before anything runs.** On load, the client and the server check the package against the schema and parse and check every script. An invalid package is rejected with a readable error.
5. **Safe on the server too.** The leaderboard server runs the same scripts to verify replays, so the sandbox must hold there as well. The isolation in `docs/server.md` is a second layer, not the only one.
6. **Grows from research.** Every mechanic found in a CLONE_SPEC either maps to an existing built-in or becomes a new one. Script patterns that repeat across games get promoted to built-ins.
7. **Versioned.** Each package declares its format version, and the client keeps supporting older versions.
8. **One format for ports and new games.** The level editor reads and writes these same files.
9. **Ports are rebuilt clean, not copied.** A port never mirrors the original's file or object layout. Near-duplicate objects collapse into one built-in with parameters (e.g. `spikeUp`, `spikeDown`, `spikeLeftSS` become one spike with a direction). Behavior shared by several games goes into the shared built-in library, not into each game. Only behavior unique to one game becomes a script, written fresh from its spec. Gameplay stays identical, and traces of the original prove it.
