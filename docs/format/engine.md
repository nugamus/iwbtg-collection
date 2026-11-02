# Engine packs

An engine pack holds what every game of one engine family shares: the kid sprites, blood, the default save sprite and the sounds. It lives once in the games repo and is **bundled** into each package that uses it ([decision](v0.md#package-layout), #102).

## Where a pack lives

| Place | Layout |
|---|---|
| Games repo | `_engines/<pack>/engine.toml`, `_engines/<pack>/sprites/<id>.png` and `.toml`, `sounds/`, `music/` |
| A built package | `engine/<pack>/engine.toml` and a copy of the pack files the package uses, same layout |

A package opts in with `engine_pack = "<pack>"` in `game.toml` and refers to a pack asset as `engine:<pack>/<id>` (a sprite id, for example `sprite = "engine:a1/kid"` in `[defaults.kid]`). The id is the file stem, as in a package.

## engine.toml

```toml
#:schema ../../../client/docs/format/schema/v0/engine.schema.json
id = "a1"                        # the folder name
title = "Engine family A1"
family = "A1"                    # physics preset: A1 | A2 | A3 | A4 | A5 | B | C
games = ["rely_the_needle"]      # informational
assets = "pending"               # pending (no files yet) | complete

[[creators]]                     # at least one
name = "<creator>"
role = "other"                   # same roles as game.toml

[[third_party]]                  # one per pack file that is not ours; path relative to the pack
path = "sprites/kid_idle.png"
what = "Kid idle sprite"
owner = "<rights holder>"
license = "unknown"
status = "to-clear"
```

The generated schema is `docs/format/schema/v0/engine.schema.json`. Unknown keys are errors.

## Checks

- `engine_pack` must name a bundled pack: `engine/<pack>/engine.toml` must exist, its `id` must equal the folder name, `family` must be a preset, there must be a creator, every `third_party` path must be a file of the pack and listed once, and `assets = "complete"` needs asset files.
- Every `engine:<pack>/<id>` resolves to a file of the bundled pack, or `pkg check` reports `the engine pack ... has no sprite ...` or `... is not in the package`.
- A `third_party` path written `engine:<pack>/<id>` in `game.toml` must resolve too.

## Loading

A pack sprite loads like a package sprite, with its files under `engine/<pack>/sprites/`; the loader keys it as `engine:<pack>/<id>`. A pack sound (`engine/<pack>/sounds/<id>.ogg`) or music track (`engine/<pack>/music/<id>.ogg`, with the same optional `.toml` sidecar) loads like a package one and is named `engine:<pack>/<id>`, so it can be used wherever a sound name is read: the physics profile's `[sounds]` and sound params of built-ins. Sound and music ids stay unique per pack (a stem in both folders is the same error as in a package). Pack sprites, sounds and music are gameplay data and enter the gameplay hash; `engine.toml` (credits) does not. Scripts do not validate sound names yet, so an `engine:` string in a script is not checked.

## Bundling

`iwbtg pkg check <dir>` (and every other command that takes a package folder) bundles for you: when `game.toml` names an `engine_pack` and the folder has no `engine/<pack>/`, it reads `../_engines/<pack>/` and copies in `engine.toml` and the files named by `engine:<pack>/<id>` references in the package's `.toml` and `.rhai` files. Nothing is written to the package folder. `iwbtg pkg fmt` leaves bundled files alone. The package builder will do the same copy.
