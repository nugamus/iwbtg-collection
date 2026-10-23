# IWBTG Collection

One client that plays I Wanna Be The Guy fangames, the way osu! plays beatmaps. Every supported game is rebuilt in our own documented format; the client never runs original games or their engines. Written in Rust with Bevy, for native and web (WASM).

Status: planning. See `docs/roadmap.md`.

## Layout

Folders appear as work reaches them. This is the agreed shape:

```
crates/
  sim/       Deterministic simulation: fixed tick, physics profiles, collision, triggers, replays.
             No Bevy dependency, so the server can re-simulate submitted runs.
  format/    Reads, writes and validates game packages.
  client/    The Bevy app: game select, rendering, input, audio, settings.
  server/    Leaderboard backend: submissions, replay verification, review queue.
  tools/     Small editors (first: music preview picker). Grows into the level editor.
games/<game-id>/   Ported games in our format.
docs/
  format/    Game package format spec.
  roadmap.md Order of work.
Potential Features.md   Feature ideas, one line each.
```

Original binaries, extraction output and clean-room specs live in the separate research repo, never here.

## Games and assets

Each ported game belongs to its creator. This project is non-commercial and will never be monetized. Creators can have their game removed from the project, its builds and its download server (process to be published before going public).
