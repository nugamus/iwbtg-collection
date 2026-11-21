# Versioning

Part of [format v0](v0.md).

- `format` is an integer major version. Format 0 is this draft.
- From format 1 on, `crates/format` keeps a loader or an in-memory migrator for every older version (principle 7). `iwbtg pkg upgrade` will rewrite files on disk with `toml_edit` (not built yet, #795). The freeze criteria and the migration rule are in [versioning](../architecture/versioning.md#format-1-freeze).
- Because unknown keys are errors, an older client refuses a newer package with a clear message.
- `engine_rev` pins built-in behavior ([architecture](../architecture/replay.md#versions)).
- `ruleset` and `[compat]` (decision 0005) are declarations in format 0. Their values must resolve to what the package already does (the profile to the physics preset's family, a deviation to the physics value), so they are removed before `gameplay_hash` is computed and writing them keeps the hash of the package without them. Format 1 will hash them as behavior; the format-1 freeze waits on #795/#928.
- Two hashes, computed by `iwbtg pkg hash` and stored in the catalog, never inside the package:
  - `gameplay_hash`: BLAKE3 over the canonical parsed form of everything that can change the sim: `game.toml` gameplay fields, physics (`physics.toml` and `physics/`), objects, rooms, timelines, scripts, sprite sidecars, mask bits and `engine_rev`. Comments, formatting, art pixels outside masks, music, backgrounds, tiles, key art, `[controls]` labels and credits are excluded. Leaderboards key on (package id, `gameplay_hash`), so cosmetic fixes keep the leaderboard.
  - `content_hash`: SHA-256 over all shipped file bytes (`tests/` excluded), for downloads (`/files/<sha256>` in [server](../server.md)). The input is the prefix `iwbtg-content-hash v0` and a zero byte, then for each file in path order the path and the bytes, each as an 8-byte little-endian length and the content. Text files (`.toml`, `.rhai`, `.md`, `.txt`, `.json`) have CRLF turned into LF first; PNG and Ogg are hashed as they are (`PackageFiles::content_hash`).
  - `iwbtg pkg hash <dir>` prints both (`gameplay_hash: <hex>`, `content_hash: <hex>`); a package with errors has no gameplay hash, so the errors are printed and the exit code is 1.
