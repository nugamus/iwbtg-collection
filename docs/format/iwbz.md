# The `.iwbz` package file

Part of [format v0](v0.md). Decided in [content](../content.md#package-file): a game ships as one zip with its own extension, like osu!'s `.osz`.

## What is in it

An `.iwbz` is a zip of one built package: the files of the [package layout](v0.md#package-layout) at their package paths (`game.toml`, `rooms/stage1_01.toml`, `engine/a1/sprites/kid.png`), with the engine pack already bundled ([engine.md](engine.md#bundling)). It is named `<id>.iwbz` after the `id` in `game.toml`.

The `content_hash` ([versioning](versioning.md)) of an unpacked `.iwbz` is the `content_hash` of the package folder it was packed from. The hash is over the files, not the zip bytes.

## Only what the format allows

A package holds readable text and media, never code ([principle 1](README.md)). The packer and the reader both check every file against the layout (`iwbtg_format::iwbz::check_entry`) and refuse the whole file on the first one that fails:

| Where | Allowed |
| --- | --- |
| top | `<id>.toml` (`game.toml`, `physics.toml`) and Markdown (`CREDITS.md`) |
| `objects/`, `timelines/`, `paths/`, `rooms/`, `particles/`, `physics/` | `.toml` (also `rooms/<id>.tiles.toml`) |
| `scripts/` | `.rhai` |
| `sprites/`, `backgrounds/`, `fonts/`, `art/` | `.png` and `.toml` |
| `sounds/`, `music/` | `.ogg` and `.toml` |
| `engine/<pack>/` | `engine.toml`, Markdown, and the folders above |

- File names are ids (`[a-z][a-z0-9_]*`), never a Windows device name (`con`, `nul`, `com1` and so on). Folders have no subfolders.
- A `.png` starts with the PNG signature and an `.ogg` with `OggS`.
- `tests/` is never shipped: the packer leaves it out and the reader refuses it.
- Paths are package paths ([`check_path`](v0.md#package-layout)): relative, `/`-separated, no `..`, no backslash, no `:`.
- No links or special files: an entry made on Unix whose mode is not a plain file or folder is refused.

Folder entries (`rooms/`) are allowed and skipped.

## Limits

The reader is written for files from strangers. Before it inflates anything it checks, from the zip's central directory:

- the file is at most 528 MiB (`MAX_ARCHIVE_BYTES`);
- at most 20,000 files (`MAX_FILES`; the largest converted game has under 2,000);
- the files add up to at most 512 MiB unpacked, the [package size limit](assets.md) (`MAX_PACKAGE_BYTES`).

Then for each file: it inflates to exactly its stated size (reading stops one byte past it, so a file cannot claim to be small and fill memory), its CRC-32 matches, its data does not overlap another file's (the overlapping-files zip bomb) and ends before the directory, and its name appears once, also ignoring ASCII case (`CREDITS.md` and `Credits.md` would be one file on Windows and macOS).

`unpack_selected` reads only the entries a caller names: the directory, the limits and every local header and data range are still checked, but an entry it does not name is not inflated, CRC-checked or `check_entry`-checked. The client's game-select preview uses it for `game.toml`, the key art, the first ranked room and the background images its art uses, and the `cherry` sprite (#1022); opening the game still runs the full reader.

`Archive` (since #1591) is the same reader over any `Read + Seek` source, so a preview need not hold the whole compressed archive (up to 528 MiB) in memory. `Archive::open` reads the end record and the central directory once and checks the limits above and every local header and data range; nothing is inflated. `Archive::read_selected` then seeks to and reads only the named files, so peak memory is the directory, those files' bytes and one entry's inflate buffer rather than the archive. The common case (no archive comment) reads only the last 22 bytes to find the end record. The client's game-select preview and manifest pass seek a `File`; the browser, which fetches bytes, still uses `unpack_selected`.

## The zip subset

Written: zip 2.0, files in path order, deflate when that is smaller and stored otherwise, UTF-8 names (flag bit 11), the fixed date 1980-01-01, no extra fields, no comments. Text files (`.toml`, `.rhai`, `.md`) are written with LF line ends. So the same package always packs to the same bytes, from a Windows or a Unix checkout.

Read: stored and deflate only. Refused: other compression methods, encryption, zip64, split archives, and bytes after the end record. Any ordinary zip tool's output of a package folder reads (tested with Python's `zipfile`).

The code is our own (`crates/format/src/iwbz.rs`), on `flate2` and `crc32fast`, which `png` already uses: no new crate, and every check is in one short file.

## Tools

- `iwbtg pkg pack <dir> [-o <file>] [--trace-mode]` checks the package and writes the `.iwbz` ([CLI](../architecture/cli.md)).
- `iwbtg pkg unpack <file> <dir>` writes it back out into a new or empty folder.
- `iwbtg pkg publish <dir> --catalog-dir <folder> [--trace-mode]` packs and writes `<content_hash>.iwbz` plus a `catalog.json` entry for a static server ([server](../server.md#game-downloads), #806).
- Every command that takes a package folder also takes an `.iwbz`, and so does the client ([import](../architecture/client/import.md)).
