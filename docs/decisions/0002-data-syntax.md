# 0002: Data syntax

Status: Accepted by Claude on the user's behalf, 2026-10-24 (user may revisit).

Related: [0001 script language](0001-script-language.md), [architecture](../architecture.md), [format v0](../format/v0.md), [format principles](../format/README.md).

## Context

A game package is readable text plus media (format principle 1). The data files hold the manifest, the physics profile, object presets, rooms, timelines and sprite metadata. They must:

- Be readable and reviewable by people and agents, with comments.
- Diff well in git. A one-instance edit should be a one-line diff.
- Be read and written by the level editor without reformatting the whole file (principle 8).
- Scale to rooms with thousands of instances: 1,288 water tiles in Fairy tale, 28,900 instances across 116 rooms in the qoqoqo alternate.
- Give readable errors with file, line and column before anything runs (principle 4).
- Parse floats exactly the same on every platform.
- Keep instance order, because creation order is semantic in GM8: events and collisions run in instance order within an object.

## Options considered

| Option | Verdict | Reason |
|---|---|---|
| TOML 1.1 (`toml` 1.1.x, `toml_edit` 0.25.x) | **Chosen by all three proposals** | Known to people and agents from Cargo. Comments. No indentation rules. Mature serde support with spans. A format-preserving editor library. JSON Schema tooling through Taplo or Tombi. TOML 1.1 allows trailing commas and newlines in inline tables. |
| KDL v2 (`kdl` 6.7.1) | Fallback for rooms | Tersest room syntax (`spike 352 544 dir="up"`) and format-preserving, but a young v2, weak schema and serde tooling, and low agent familiarity. |
| RON (`ron` 0.12.2) | Rejected | Rust-centric, little tooling, unfamiliar outside Rust. |
| JSON | Rejected | No comments, noisy diffs. |
| YAML | Rejected | Indentation traps, implicit typing, `serde_yaml` is archived. |

### Judges' scores per proposal

Each judge scored the three proposals as a whole (determinism / sandbox safety / WASM / readability / porting speed / maturity). The data syntax shows up mainly in readability and porting speed.

| Proposal | Readability (J1, J2, J3) | Porting speed (J1, J2, J3) | Total over all six criteria and judges |
|---|---|---|---|
| conservative | 8, 8, 8 | 8, 7, 8 | 147 |
| determinism | 7, 6, 7 | 6, 6, 6 | 142 |
| authoring | 9, 9, 9 | 9, 9, 9 | 144 |

All three chose TOML. The authoring proposal's file conventions scored best on readability and porting speed and are the base for this decision. Grafted in:

- From determinism: `deny_unknown_fields`, exact-bit float literals, the `@` prefix for package objects, a formatter that never reorders instances.
- From conservative: `toml_edit` for every editor write, `iwbtg room show` ASCII maps, snake_case ids.

Judges' corrections applied: ids are snake_case so they can double as Rhai identifiers (judge 2). Room instance keys are spelled out (`kind`, not `k`) for readability (judge 2).

## Decision

**TOML 1.1 for every data file.** Scripts are `.rhai` files ([0001](0001-script-language.md)). Images are PNG. Audio is Ogg Vorbis.

### Libraries

- Parse with `toml` 1.1.x (TOML spec 1.1.0) and serde with `#[serde(deny_unknown_fields)]` on every struct. An unknown key is an error, so a typo or a field from a newer format version fails clearly.
- `serde_spanned` keeps spans, so every error names file, line and column.
- `schemars` 1.2 generates JSON Schemas from the same Rust structs into `client/docs/format/schema/v<N>/*.schema.json`, where `<N>` is the format major version (`v0` for the current draft). Each data file starts with a `#:schema` comment (a path relative to the file, assuming the `client/` and `games/` repos sit side by side), so VS Code with Even Better TOML (Taplo) or Tombi gives completion and red squiggles. `iwbtg pkg check` stays the authority.
- The level editor and `iwbtg pkg upgrade` write through `toml_edit` 0.25.x, which keeps comments, key order and layout. Saves produce minimal diffs.
- Exact versions are pinned when `crates/format` is created, after checking each is at least 14 days old.

### Conventions

- **Canonical formatter.** `iwbtg pkg fmt` writes one room instance per line as a single-line inline table (valid TOML 1.0 and 1.1), a fixed key order per kind, shortest round-trip floats, and LF line endings. It never reorders instances. The editor writes through the same rules.
- **Line endings.** `.gitattributes` in the games repo sets `* text eol=lf` and marks media as binary, so Windows checkouts do not change hashes.
- **Instance order is creation order.** The order of the `instances` list is the GM creation order. Tools never sort it. The gameplay hash changes on any reorder, so a reorder is visible in review.
- **Ids.** Lowercase snake_case ASCII, `[a-z][a-z0-9_]*`, at most 48 characters, unique within their namespace. An asset id equals its file stem.
- **Package objects carry `@`.** A kind defined by the package is written `@boss1`; built-in kinds are bare (`spike`). A future built-in can therefore never collide with an existing package object.
- **Shared engine assets.** References into a shared engine pack are written `engine:<pack>/<id>`, for example `engine:yuuutu/player_idle`.
- **Numbers.** Positions and physics values are plain TOML integers or decimals. Rust parses decimals to f64 with correct rounding on every platform. When a trace shows that a constant must match a GM8 literal bit for bit (Delphi may parse literals through extended precision and round differently), any number may be written as `{ bits = "3FDCCCCCCCCCCCCD" }` (16 hex digits of the IEEE double).
- **Large rooms.** A `run = [columns, rows]` key on an instance expands into a grid of identical instances in row-major order, with spacing `step = [dx, dy]` (default `[32, 32]`). The converter writes a `run` only when the original instances were consecutive in creation order and aligned. It expands into individual instances at load and never merges into one rectangle, because GM8 tests collisions per instance. Decorative tiles go in a separate `rooms/<id>.tiles.toml`, so gameplay diffs stay small.

### Example

```toml
#:schema ../../../client/docs/format/schema/v0/room.schema.json
size = [800, 608]
speed = 50
music = "stage1"
instances = [
  { kind = "kid_start", x = 48, y = 544 },
  { kind = "block", x = 0, y = 576, run = [25, 1] },
  { kind = "spike", x = 352, y = 544, dir = "up" },
  { kind = "spike", x = 384, y = 544, dir = "up", group = "trap1", visible = false },
  { kind = "water", x = 416, y = 320, variant = 2, run = [4, 6] },
  { kind = "save", x = 96, y = 512, name = "save_a" },
  { kind = "warp", x = 768, y = 512, to = { room = "stage1_b", at = "spawn_left" } },
  { kind = "@boss1", x = 400, y = 200, hp = 30 },
]
```

Full examples for every file type are in [format v0](../format/v0.md).

## Consequences

- Agents and people read packages with no special tooling. Schemas give editor feedback before the game runs.
- Large rooms stay reviewable: 1,288 water tiles become a few dozen `run` lines. A room without runs is still only a few thousand short lines and parses in milliseconds. `iwbtg room show <room>` prints an ASCII map, so a reviewer can see the layout without the client.
- Original GM ids never appear in packages (principle 9). The mapping from original object and instance ids to ours lives outside the shipped package in `tests/trace-map.toml`.
- Because unknown keys are errors, an old client refuses a newer package clearly instead of ignoring fields.
- TOML is more verbose than KDL for rooms. If rooms prove too verbose even with `run` and tiles files, rooms can move to KDL in a later format major version without changing the sim.
- Data syntax and the in-memory model are separate: `crates/format` turns files into `sim::def::GameDef`, and the sim never parses text.

## Open items

- **OPEN:** whether `run` spacing should default to the sprite's size instead of `[32, 32]`. Decide when the converter handles the first game.
- **OPEN:** how GM instance creation code maps into rooms (instance parameters, or a named script call). Decide from `research/specs/mechanics.md`.
- **OPEN:** whether the `{ bits = ... }` escape is ever needed. It stays in the format either way, and the converter compares every physics constant's bits with trace values.

## Evidence

- toml 1.1.8+spec-1.1.0 and toml_edit 0.25.17 (2026-10-09): https://github.com/toml-rs/toml
- schemars 1.2.3: https://crates.io/crates/schemars. serde_spanned 1.1: https://crates.io/crates/serde_spanned
- kdl 6.7.1: https://crates.io/crates/kdl. ron 0.12.2: https://crates.io/crates/ron
- Instance-order semantics: research reference OpenGMK `gm8emulator/src/game/events.rs` (object, then instance order); `research/knowledge/physics-core.md` (object order affects collision and event order).
- Room sizes: `research/shortlist.md` (Fairy tale water 1288, qoqoqo 28,900 instances).
