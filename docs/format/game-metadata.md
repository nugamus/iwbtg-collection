# Game page metadata

One model for the game-page data that [content](../content.md#game-list-data) (Game list data) and [product](../product.md#game-select-and-loading) (Game select) describe: where each field lives and who owns it. Game select (#148, #498), the catalog (#806) and the website (#85) read the same fields, so a field may never have two sources. Defined by #805.

This page is not part of the package format yet: fields marked **proposed** are additive candidates for a later format, outside `gameplay_hash`, and today's loader rejects unknown keys. Do not write one into a package until its issue lands. Existing fields link to their spec page.

## One field, one owner

| Owner | Lives in | Written by | Serves |
|---|---|---|---|
| **Package** | `game.toml` and package files inside the `.iwbz` | the porter (a new `version` for a change) | the source of truth for game-owned data |
| **Catalog** | `<catalog-dir>/catalog.json`, built by `iwbtg pkg publish` | `pkg publish` (package half) and the server (server half) | game select's list before a download, and every list surface |
| **Server** | the SQLite database and the served catalog | moderators, claimed creators, play data | status, curation, community tags, page text, measured values |
| **Deviation log** | `docs/games/<game-id>.md` in this repo | the porter, from a trace or spec claim | the Known differences the game page shows |

Rules:

- **One field, one writer.** No field is editable in two places. The package is authoritative over a stale catalog entry; a disagreement is a republish, not a merge.
- **The catalog is a projection with two halves.** `pkg publish` writes the package half from `game.toml`; the server overlays the server half when it answers `/catalog.json`. The file on disk never holds server-owned fields (a republish replaces the whole entry).
- **Absent is not empty.** A missing field means unknown (not measured, no log yet); an empty list means known none. The client shows nothing for unknown and "none known" for empty.
- **Display data never moves behavior.** Every field below is outside `gameplay_hash`, like `[key_art]` and `[controls]`. A field that could change a run belongs in the behavior docs, not here.

## Fields

### Package-owned

| Field | Source | Status |
|---|---|---|
| `id`, `title`, `version`, `source_version` | `game.toml` ([manifest](game.md)) | exists |
| Creators and roles | `[[creators]]` ([manifest](game.md)) | exists |
| Original page and permission | `[source]` | exists; `permission` is moderation data and is never displayed on the page |
| Release date | proposed `[source].release_date` (#1050) | proposed |
| Maps and stages (names, difficulty, category, version) | `[[maps]]`, `[timing].stages` ([manifest](game.md#maps)) | exists |
| Key art | `[key_art]` ([manifest](game.md#key-art)) | exists |
| Core tags | proposed `tags` | proposed |
| Trick list | `[tricks]` ([manifest](game.md)); the label is the trick id, the value its switch state | exists (#1160 may move the source) |
| Stage previews | proposed `[previews]` | proposed |
| Changelog per version | proposed `[[changelog]]` | proposed |
| Music preview | proposed `[preview]` | proposed |
| Custom physics | derived from the package: `LoadReport::physics.is_custom()` ([engine](engine.md#custom-physics), #802, #970) | computed by `pkg check`; not in the catalog yet |
| Fidelity evidence | `tests/inputs/`, `tests/trace-map.toml`, `tests/replays/*.iwbr` ([v0](v0.md#package-layout)) | exists |

#### Core tags

```toml
tags = ["needle", "trap"]        # 1 to 3, from the fixed set
```

- Fixed ids, snake_case: `needle`, `avoidance`, `adventure`, `boss_rush`, `gimmick`, `trap`, `puzzle`, `medley`. Display names: "Boss rush" for `boss_rush`.
- One to three tags per game, chosen by the porter at port time; the fixed set is closed, so a new genre is a product change, not a free tag.
- Free tags are **not** here: they are community data and belong to the server (below). The game page shows core tags first, then free tags.
- Display-only: outside `gameplay_hash`.

#### Release date

```toml
[source]
release_date = 2013-07-14        # the original's release
```

- A TOML date. The catalog copies it as an ISO `release_date`; game select derives the year for the Year group, sort and filter.
- It is also the asset origin credit's ordering key ([content](../content.md#assets); #1050 owns the field).

#### Changelog

```toml
[[changelog]]                    # newest first
version = "1.2.0"                # the package version this entry describes
date = 2026-11-20
notes = "Fixed the boss softlock on the Hard map."
```

- One entry per port version: `version` is a semver unique in the list, and the first entry is the package's current `version`.
- `notes` is one line of plain display text. The list covers the port's history, not the original's.
- Display-only. The website and the game page show it; game select's list does not.

#### Stage previews

```toml
[previews]                       # one image per stage, optional
"medium/stage1" = "art/previews/medium_stage1.png"
```

- The key is `<map id>/<stage id>`; the map is `main` when the package has no `[[maps]]`. Every key must resolve to a stage of that map, and a stage has at most one preview.
- The value is a PNG inside the package, with the [key art](game.md#key-art) image rules (at most 4096 px a side, not under `tests/`). Recommended 960x540 (16:9).
- A stage without a preview shows the game's key art. Previews are package files: a surface that has not downloaded the package shows key art only.

#### Music preview

```toml
[preview]                        # the clip on the game page
track = "stage1"                 # a music id of the package, or engine:<pack>/<id>
start_seconds = 42
```

- `track` resolves like every music reference; `start_seconds` is 0 to 3600.
- The porter picks the track and start point with the picker tool ([content](../content.md#game-list-data)). Audio only: it never touches the sim or the replay.

### Catalog

The entry the client sees at `/catalog.json` ([server](../server.md#game-downloads)). The package half is written at publish; the server half is overlaid when serving.

| Field | Half | Source | Status |
|---|---|---|---|
| `id`, `title`, `version`, `format`, `url`, `filename`, `size`, `content_hash`, `gameplay_hash` | package | `pkg publish` | exists |
| `creators` (name, role), `source` (url, permission) | package | `pkg publish` | exists |
| `performance` (class, peaks) | package, measured | `pkg publish` from `iwbtg profile --summary` | exists |
| `release_date` | package | `pkg publish` (#1050) | proposed |
| `tags` | package | `pkg publish` | proposed |
| `custom_physics` | package, derived | `pkg publish` from `LoadReport::physics` | proposed |
| `port_status` (Beta, Ranked, Loved) | server | the server, from the database | proposed (#62) |
| `fidelity` (state, counts, check date) | server | the server, from the ranking job | proposed (#62) |
| `free_tags` | server | the server, from the community tags | proposed |
| `known_differences` (`[{ text, tier }]`) | server | generated from the deviation log | proposed |
| `stars` (per map) | server | play data (#111) | proposed |
| `description` | server | page text (a claimed creator) | proposed |

- Only these fields. The changelog, stage previews, trick list, music preview, maps and key art stay in the package: game select's list needs none of them, and the details surface reads them from the downloaded package or from the website's own pipeline. Keeping them out keeps the catalog small and the list's 60-second cache cheap.
- The server half is never written to the file: `pkg publish` replaces an entry on a new version, so a written copy would go stale. A static catalog with no server in front shows the package half and the client treats the server fields as absent; the Status, Year and Tag controls stay faded until they arrive (#150).
- `stars` is keyed by map id (`{ "medium": 4.2 }`), because the rating is per map ([#111](https://github.com/nugamus/iwbtg-collection/issues/111)); a game entry without one is unrated, not zero.

### Server-owned

#### Port status

Values: `beta`, `ranked`, `loved` ([content](../content.md#lifecycle)).

- **Beta**: playable, unranked; player clears count as clearability proof.
- **Ranked**: fidelity proven and curation accepted; public leaderboards. A physics bug found later is fixed and every stored replay re-run; still-clearing runs stay ([content](../content.md#lifecycle)).
- **Loved**: fun to play, not suited to leaderboards; playable with no public boards.

Status is per game, never package data: a game does not change status by shipping a new version. The server records who set it and when, and the change of status is a moderation action (#62). The Status filter and the game list mark read only this field.

#### Fidelity proof

What Ranked requires, and what the page summarises:

- Every trace case in the package's `tests/` re-run against the port at its ranked revision, with the same result ([content](../content.md#proving-a-port)).
- A verified full clear (`tests/replays/*.iwbr`) through the headless CLI; during Beta, player clears also count.
- A human playtest of the feel.

The server stores the proof (state, trace-case count, verified-clear count, date checked) and the page shows the summary. The evidence files are package-owned; the verdict is server-owned. The fidelity record comes with the status field (#62).

#### Free tags

- Community tags, added by any signed-in player from the game page; a claimed creator can remove them. The server moderates them; the list is capped (proposed: 8 per game, 1 to 24 characters, lowercase, no duplicates of a core tag).
- Free tags never become core tags. Promoting a genre is a product change; correcting a game's core tags is a new package version.
- Filters match a tag against core and free tags together.

#### Page text

The description a claimed creator edits after claiming the game ([content](../content.md#game-list-data)). Server-owned because it changes without a package version, and moderated. The package's `[[creators]]` list stays the authoritative credit; a correction to it is a package change, not page text.

#### Known differences from the deviation log

Known differences are **generated** from the per-game deviation log (#784); there is no second list to edit.

- Source: the `### Known differences` section of `docs/games/<game-id>.md` ([games](../games/README.md#deviation-log)). Each entry already carries its F tier, its contract, its evidence and its date.
- The server copies only what the page shows: `known_differences = [{ text, tier }]`, in log order, with `tier` `F2` (visible/heard) or `F3` (not observable). The Kept quirks, Fixed bugs and Open sections are not game-page fields.
- A deviation accepted into Beta still appears here, with the date the user or lead accepted it ([games](../games/README.md#deviation-log)).
- A game with no file or no `### Known differences` section has no field (unknown), while an empty section is `[]` (none known).
- Generation needs this repo where the server can read it, or a generated artifact committed with the catalog; the mechanism is a follow-up issue.

#### Measured play data

- **Star difficulty** per map: deaths and time per stage across players, like osu!'s star rating ([content](../content.md#game-list-data)); computed and stored by the server (#111), copied to the catalog.
- **Clear stats** (average clear time, global clear rate) are live aggregates over the boards, served by the board API (#57), not the catalog: they change every run and the catalog is cached for a minute.
- **Performance class**: measured from the package's stored clear replays at publish time (`iwbtg profile --summary`), never hand-written; absent when no stored replay exists ([server](../server.md#game-downloads)).

## Edit rules

- A **porter** changes package-owned fields with a new package `version`, then republishes. Changelog, tags, previews and the release date change this way.
- A **claimed creator** edits the page text and free tags and can request a takedown; the package credit list is not theirs to rewrite.
- A **moderator or lead** sets port status, accepts curation, removes free tags and handles takedowns.
- A **known difference** is changed only in the deviation log; the page follows on the next generation.
- The client's rule: the list reads only the catalog; the details surface reads the catalog first and the downloaded package for package-only fields; on a disagreement the package wins.

## Follow-ups

| What | Issue |
|---|---|
| The proposed manifest fields (`tags`, `changelog`, `previews`, `preview`), the release date field (#1050) and their validation | #1631, #1050 |
| `tags`, `release_date` and the derived `custom_physics` in the catalog entry | #1631 |
| Status and fidelity in the catalog | #62 |
| The server half of the catalog (overlay, free tags, page text, known-differences generation) | #1632 |
| Star difficulty | #111 |
| The consumers: game select Details and filters, the website | #148, #498, #85 |
