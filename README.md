# IWBTG Collection

One client that plays I Wanna Be The Guy fangames, the way osu! plays beatmaps. Every supported game is rebuilt in our own documented format; the client never runs original games or their engines. Written in Rust with Bevy, for native and web (WASM).

Status: pre-release, in active development. There are no release builds yet; the public alpha is planned for phase 7 (Online). See `docs/roadmap.md` for the phases, the [progress dashboard](https://nugamus.github.io/iwbtg-collection/) for porting coverage, and `docs/dev-setup.md` for building it yourself.

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
docs/
  format/    Game package format spec.
  games/     Per-game design questions for the user.
  product.md Player-facing decisions: ranked mode, mods, leaderboards, UI.
  content.md Porting, assets, engines, package file, level editor.
  server.md  Server stack and security rules.
  roadmap.md Order of work (the phase gates).
  dev-setup.md Machine layout, toolchain versions, setup check.
progress/    Generated porting-progress dashboard and its data.
Potential Features.md   Feature ideas, one line each.
```

Ported games live in the separate games repo (`iwbtg-collection-games`). Original binaries, extraction output and clean-room specs live in the research repo. Neither belongs here.

## Games and assets

Each ported game belongs to its creator. This project is a fan project: it is not affiliated with or endorsed by the creators of I Wanna Be The Guy or of any fangame. Game content, including characters, sprites, music and level design, belongs to its creators and is not covered by this repository's licenses.

The project is non-commercial and will never be monetized. Creators can have their game removed from the project, its builds and its download server (the process is not published yet; it is tracked in [#68](https://github.com/nugamus/iwbtg-collection/issues/68)).

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the licenses, the clean-room rule (no decompiled code or original game files) and what a PR must contain. Report security problems privately, never in a public issue; see [SECURITY.md](SECURITY.md).

## License

Code is licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Documentation in `docs/` is licensed under [CC BY 4.0](LICENSES/CC-BY-4.0.txt). The bundled fonts in `crates/client/assets/fonts/` (Barlow, Barlow Condensed, Pixelify Sans, a subset of Noto Sans Symbols 2, a subset of Inter, a subset of Noto Sans JP) are under the SIL Open Font License 1.1; their licence files sit next to them. Game content is not covered by these licenses. That includes the Kid sprites in `crates/client/showcase.pkg`, which come from a fangame and are credited in its `CREDITS.md`.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
