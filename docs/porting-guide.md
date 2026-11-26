# Porting guide for community porters

A port rebuilds one fangame in our own package format so the client can play it without running the original. This page is for the person doing that work: what a port is, how the package is laid out, the steps that produce it, the permission and credit rules, and what a port pull request must contain.

The internal pipeline design — what the converter automates, who owns each step, how long each step takes and what is still in flight — is [porting](porting.md). This page does not repeat its time targets; it names the step and links to the detail. The principles that all ports follow are in [content](content.md#porting).

**Status.** Today the project owner and agents do the ports, and the games repository is private. Porting opens to other people as the project matures ([content](content.md#porting)). The process below is the one every existing port uses, so it is the one a new porter follows. The client rules apply to you exactly as they apply to us: [CLAUDE.md](../CLAUDE.md) and [CONTRIBUTING](../.github/CONTRIBUTING.md). ("The user" in the rest of the docs means the project owner.)

## What a port is

- **A clean-room rebuild, not a wrapper.** The client never runs the original or its engine. Behavior comes from the clean-room specs and recorded traces in the private research repository; nothing is copied from the game or from another runner.
- **Rebuilt clean, not mirrored.** A port does not keep the original's object or file layout. Near-duplicate objects collapse into one built-in with parameters, and only behavior the built-ins cannot express becomes a script, written fresh from its spec ([format principles](format/README.md)).
- **Feel over implementation.** Fidelity means what a player can feel: position, death, timing, RNG on a played path, input response and respawn control. An incidental implementation detail no player can observe — boot rooms, temp files, internal ids — is free to redesign. Every difference is graded F1 (feel-critical), F2 (visible or audible) or F3 (unobservable) under [fidelity tiers](plans/fidelity-tiers.md); F1 blocks Ranked, and a visible F2 difference blocks Beta.
- **Standard by default.** A port selects the named ruleset its family uses (`gm8@N`) and a `[compat]` profile, and each deliberate deviation is declared and logged. One-game behavior goes into data, a script or a profile — never into shared engine code ([0004](decisions/0004-extension-model.md), [0005](decisions/0005-standard-mechanics.md)).
- **A lifecycle, not a one-off.** A finished port enters Beta (playable, unranked, player clears count as proof), then Ranked when fidelity is proven and curation accepts it, then possibly Loved ([content](content.md#lifecycle)).

## The three repositories

| Repository | Visibility | Holds |
|---|---|---|
| `iwbtg-collection` (this one) | Public | Client, format, CLI, server and docs. No originals; game art is limited to two credited exceptions: the Kid sprites in the showcase package (games#34) and the client's UI delicious fruit in `crates/client/assets/ui/` (#1608). |
| `iwbtg-collection-games` | Private | Ported packages, one folder per game under `games/<game_id>/`. Packages only, never originals or extraction output. |
| `iwbtg-collection-research` | Private | Clean-room specs, traces, the converter, and the gitignored `archive/`, `extracted/` and `db/` that the converter reads. |

Extraction and conversion run in the research checkout, one agent at a time, because that is the only place `archive/`, `extracted/`, `db/` and `tools/` exist ([dev-setup](dev-setup.md)). The games repository's GitHub Actions do not run, so a package is checked locally with the commands below.

## Permission and credit

These rules decide whether a game may be ported and who is credited. Record them when the port opens, not at the end.

- **Pick the version.** The creator or the porter chooses which version to port; keeping the same version as before is preferred so records stay historical. Record the source URL, the creators and the permission status — the converter cannot supply any of them ([porting](porting.md#stage-0-intake)).
- **Permission is a package field.** `[source].permission` is required and is one of `granted`, `community-assumed`, `pending` or `revoked`, with `permission_date` and `permission_note` as the audit trail. A `revoked` package is never loaded, built or served ([game.toml](format/game.md)). Commercial music in fangames is kept and removed on request, like `community-assumed` games ([content](content.md#music)).
- **Credit the people.** Every `[[creators]]` entry names a person and a role: `author`, `co-author`, `music`, `art`, `tester` or `other`.
- **Credit every third-party file.** `[[third_party]]` is required once per file that is not the creator's own work or ours: `path`, `what`, `owner`, `license` (an SPDX id or `unknown`) and `status` (`cleared`, `to-clear` or `excluded-from-public`). Commercial music, Kayin's sprites and engine assets all go there. `CREDITS.md` is the human-readable view; `game.toml` is the machine source.
- **Asset reuse follows fangame culture.** Creators may reuse any asset from any ported game and the credit is added automatically; the origin is the game with the earliest release date. A creator can opt their game out of reuse, and a takedown removes only that game — assets reused elsewhere stay ([content](content.md#assets)).
- **Creators can claim their game.** A verified creator can edit the page, set the reuse opt-out, approve versions and request a takedown ([content](content.md#game-list-data)).

## The package layout

One port is one folder, `games/<game_id>/`, in the games repository. It ships as one `<id>.iwbz` file (a zip with its own extension) and is served by content hash. The full layout and the field rules are in [format v0](format/v0.md#package-layout); this is the shape:

```
games/<game_id>/
  game.toml                  manifest: source, permission, creators, third_party, timing, maps
  physics.toml               kid physics profile
  physics/<id>.toml          optional room profiles, for rooms that name one (medleys)
  objects/<id>.toml          package objects: presets of built-ins plus scripted objects
  scripts/<id>.rhai          scripts, only for behavior the built-ins cannot express
  rooms/<room_id>.toml       gameplay instances of one room
  rooms/<room_id>.tiles.toml optional decorative tiles
  sprites/<id>.png|.toml     sprite strip and its sidecar (frames, origin, collision mask)
  backgrounds/<id>.png|.toml backgrounds and tilesets
  sounds/<id>.ogg|.toml      sound effects
  music/<id>.ogg|.toml       music
  fonts/<id>.png|.toml       bitmap fonts
  timelines/<id>.toml        tick-indexed moments (avoidance, boss phases)
  paths/<id>.toml            movement paths
  particles/<id>.toml        particle systems
  engine/<pack>/             the bundled engine pack the game uses
  CREDITS.md                 human-readable credits
  tests/                     not shipped, not hashed
    replays/full_clear.iwbr  the verified full clear
    inputs/<case>.inputs     trace and test inputs
    trace-map.toml           original ids to ours
```

Ids are lowercase snake_case. A package object is referenced with `@` (`@boss1`); a built-in is bare (`spike`); an engine-pack asset is `engine:<pack>/<id>`. References resolve at load, and an unknown id is an error with file, line and column. Package paths may not differ only by ASCII case (Windows and macOS store them as one file). Format 0 is a draft until format 1, so expect the shape to change.

## The steps

The steps below are the porter's view. Each names the gate that says it is finished; the authoritative order, owners and time targets are in [porting](porting.md#the-pipeline-at-a-glance).

1. **Intake: version, source, people, permission.** Choose the version, record the source URL, the creators and `[source].permission` ([porting, Stage 0](porting.md#stage-0-intake)). Gate: the entry exists and the version is chosen.

2. **Download, clear and extract.** In the research checkout: `uv run iwc download --games <game id>` stores the original by SHA-256, `uv run iwc vt` clears it, `uv run iwc engines` detects the engine and `uv run iwc extract --games <game id>` writes `extracted/<slug>/` and an `extraction.json`. Extraction runs no game code. Gate: VirusTotal clean (a flagged file is a human check, because GM8 games often trigger false positives) and `extraction.json` says ok. The download queue is deliberately slow: 4 requests a minute and 500 a day.

3. **Convert.** `uv run iwc convert --game <slug>` writes `build/packages/<package id>/` and a report at `build/reports/<slug>.md`. Prefer `--all-shortlist` when you can: the run reads every shortlisted game and keeps all packages on one converter. Then `cargo run -p iwbtg-tools -- pkg check <package> --trace-mode`; `--trace-mode` accepts converter output and turns format gaps into warnings that name a converter request. Gate: the only errors are objects with `script =` and no script file.

4. **Triage the gaps before editing anything.** Read `build/reports/<slug>.md` and `summary.md`. Every gap that is not a missing script is either a converter request ([converter requests](format/converter-requests.md)) or a spec question, and is filed as an issue against the converter or the research repo — never fixed by hand in the package. This is the highest-leverage step: one mapping fix can remove hundreds of instances from the hand work. Gate: `pkg check` has no structural error.

5. **Keep the base and the hand work apart.** The converted base is regenerable and never hand-edited. Scripts, overrides, docs and tests are the overlay the converter never touches; until the overlay layout lands, keep the hand work outside the package folder and re-apply it after a re-convert (#779). Re-converting is normal, because converter fixes are shared by every game.

6. **Budget the scripts from the census.** The research census classifies every unmapped object: covered by a built-in, coverable by a new preset, menu-only, or a unique script. A cluster found in three or more games is a built-in candidate; two games share a versioned script instead of two copies ([content](content.md#engines)). Gate: the script count is known before writing scripts.

7. **Choose the engine pack.** A game whose physics match its family uses the shared pack for the Kid, blood, saves and sounds, so those assets are not copied into the package ([engine packs](format/engine.md)). Until the packs are filled, a game carries its own copies and their `[[third_party]]` entries.

8. **Ask the game's questions once.** Stripped-mode cuts, the ranked start and end, cutscenes and dialogue, secret items and mods are decisions only the project owner makes. Put them all in one batch in `docs/games/<id>.md` with a proposed default where the answer is not irreversible ([games](games/README.md#per-game-questions)). Do not guess them in the package.

9. **Spec the unique objects.** Clean room: a research session writes a short behavior spec per object from the GML, citing its evidence and a claim id. The default until #780 decides otherwise is one spec pass and one script pass ([porting, Stage 2](porting.md#stage-2-behavior)). Gate: every unique script has a spec with evidence.

10. **Write the scripts.** A sim session writes `scripts/<id>.rhai` from the spec, never from the original. Run `pkg check` after each script so a missing API function surfaces immediately. When a planned script API function blocks you, file it against Script API v1 (#776) instead of working around it inside the script ([scripts](format/scripts.md)).

11. **Prove each F1 mechanic.** Add one trace case and one feel contract per F1 mechanic, and use the recorder's case where one exists. A trace case is short scripted input, run in our port and diffed against the original's recorder trace; cases live in `crates/tools/cases/<package id>/` and enter `crates/tools/fidelity-baseline.txt`. A feel contract states the player-observable outcome and names the trace as evidence; it lives in `crates/tools/feel/<area>/<name>.toml`, and its `<area>/<name>` id is what the triage comment and the deviation log cite ([traces](architecture/traces.md), [feel contracts](architecture/feel-contracts.md)). F2 mechanics need a screenshot or audio check; F3 details need only the pinned hash. Gate: `trace check` passes and the case is in the baseline; `cargo xtask fidelity` (with `IWBTG_RESEARCH` set) and `cargo xtask feel` (with `IWBTG_GAMES` set) stay green.

12. **Get the human recording.** Agents never play whole games. The project owner plays a short segment of the original in the research recorder, with the exact command (room, variables, output name) prepared by the porter; the recording's `.inputs` and `.csv` files are committed to the research repo and imported as a case. Schedule this early: it is the first hard human dependency.

13. **Diff, fix, or file.** Run the case, read the first differing frame and field, and fix the sim, the package or the converter — never the trace. If the player-observable outcome still holds, the difference is F3 and is recorded; if it moved, it is F1 and the trace stays the evidence. A difference that reveals a new engine rule becomes a spec or trace issue and stays open; do not invent the answer. Do not clone an implementation detail to make a probe column match — file the redesign and mark its tier.

14. **Finish the paper trail.** Fill the deviation log's four sections — kept quirks, fixed bugs, known differences, open — with a tier, the feel-contract id and the evidence on every entry, in `docs/games/<id>.md` ([deviation log](games/README.md#deviation-log)). Stripped-mode tables come from the answers in step 8. The game page's known differences are generated from this log, so do not write a second list.

15. **Record a full clear.** Clearability is proven by a human recording a full clear in our port. `iwbtg replay verify <dir> <replay>` reproduces the run's splits and prints `verified`, or one `mismatch:` line per difference ([CLI](architecture/cli.md)). A clear that cannot be produced is a port problem, not a paperwork problem.

16. **Playtest and feel.** The project owner playtests the port. Every difference they see becomes an issue with an F tier, or is accepted in the deviation log with the contract, trace or dated playtest that proves it. Visible F2 differences block Beta; F1 blocks Ranked.

17. **Assets, credits and the index.** Review every non-ours asset the package carries, complete its `[[third_party]]` entry, and run `iwbtg assets index <games dir>` to refresh `assets-index.toml`; it prints duplicate groups and keeps the `tags` already in the file ([CLI](architecture/cli.md)). Origin credit is not a Beta gate, but every entry must exist.

18. **Open the package PR and mark Beta.** See the next section. Beta is reached when the fidelity proof, the verified clear, the deviation log and the playtest exist; Ranked waits on every F1 item ([porting](porting.md#stage-4-beta)).

## What a port pull request must contain

A port produces two kinds of change. Keep them in separate repositories and separate pull requests.

**The package PR** goes to the games repository with the package folder. Its body records:

- the local checks you ran, with their real output: `pkg check` clean and `pkg pack` producing the `.iwbz` with both hashes printed (`gameplay_hash` and `content_hash`);
- the trace result: every case OK, or the named deviation that covers it;
- the verified clear: `iwbtg replay verify` printing `verified`;
- the deviation log, or the link to the game's page in this repository;
- the project owner's playtest, and what it covered.

The games repository has no CI, so run the check locally as its rules require ([porting](porting.md#stage-4-beta)); `../client` is this checkout, so substitute your path if it is named differently:

```
cargo run -q --release --manifest-path ../client/Cargo.toml -p iwbtg-tools -- pkg check <package>
```

A port PR waits for the project owner's playtest before it merges. The package is not shipped or marked Beta until that review happens.

**A client-repository PR** is needed only when the port changes the client, format, sim or tools — a converter request, a new built-in, a feel-contract harness change. That is its own issue and its own PR against this repository, with the [pull request template](../.github/pull_request_template.md) and its User check, as [CONTRIBUTING](../.github/CONTRIBUTING.md) describes. Never fold an engine change into a package PR to avoid the client review.

A port PR is reviewed against the same checklist as any other: the F tier on every fidelity claim, the contract and evidence behind it, a declared `[compat]` deviation where the game keeps one, and the docs trail in the same change ([review checklist](agent-workflow.md#pr-review-checklist)).

## Where to ask

Open an issue: the games repository for package and port work, this repository for the client, format, sim or tools. A question only the game's creator can answer goes in the game's `## Open` section in `docs/games/<id>.md` with the options and a proposed default; do not guess the answer in the package. Never open a public issue for a security problem.
