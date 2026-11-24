# Porting a game

The steps every port follows, from download to Beta, and who owns each one. The principles are in [content](content.md#porting) (Porting); the fidelity rule and the impact tiers are in [fidelity tiers](plans/fidelity-tiers.md). This page is the pipeline design (#23): it says what the converter automates, what an agent does, where the user records or playtests, and how long each step should take.

The targets below are planning estimates, not measured averages. They are written to be checked after each port and revised here. "Agent time" is work in one lane; "human time" is the user at the keyboard and is counted separately.

This page does not wait for the first finished port. `rely_the_needle` is the sample that exposed the bottlenecks ([below](#bottlenecks-found-porting-rely_the_needle)); the census (nugamus/iwbtg-collection-research#172) generalizes them to the other 19 games. What is still in flight is listed in [Dependencies](#dependencies-and-what-is-still-in-flight).

## Who does what

| Owner | Means | Where |
|---|---|---|
| **Converter** | Automated: `uv run iwc convert` reads extracted game data and writes a format v0 package plus a report. It runs nothing from the game. | Research checkout, main `iwbtg-collection-research/` |
| **Agent** | One agent session in its lane: research, sim, data, docs or ui. Runs the commands, reads the reports, writes specs, scripts, traces and docs. | Research checkout or a lane worktree ([agent workflow](agent-workflow.md)) |
| **Human** | The user only: answers game questions, records inputs on the original, records a full clear in our port, and playtests. Agents never play whole games ([content](content.md#proving-a-port)). | The research recorder, then the client |

Three repo rules shape every step:

- The client repo is public and never holds game art. The games repo holds only packages in our format, never originals or extraction output. Originals stay in research `archive/` and `extracted/` ([dev setup](dev-setup.md#why-iwbtg-collection-games-has-no-packages-yet)).
- Research data work (crawl, download, extraction, traces, converter) needs `archive/`, `extracted/`, `db/` and `tools/`, which exist only in the main research checkout, so it runs there one agent at a time ([dev setup](dev-setup.md), [research lane](lanes/research.md)).
- The games repo is private and its GitHub Actions do not run; a package PR is checked locally, as its `CLAUDE.md` requires. Port PRs wait for the user's playtest.

## The pipeline at a glance

| # | Step | Owner | Time target | Gate |
|---|---|---|---|---|
| 0.1 | Fix the version, source URL, creators and permission status | Agent | 15 min | Entry in the catalog; version chosen ([content](content.md#porting)) |
| 0.2 | Download and VirusTotal check | Agent, human only on a flag | 10 min active; queue waits (4/min, 500/day) | VirusTotal clean |
| 0.3 | Detect the engine and extract | Agent | 15 min | `extraction.json` says ok |
| 1.1 | Convert the package and read its report | Converter, agent reads | 15 min | Converter checks OK; only missing-script gaps |
| 1.2 | Triage converter gaps, file CRs and research issues | Agent | 30-60 min per game; a converter fix is 0.5-2 days, shared by all games | `iwbtg pkg check` has no structural error |
| 1.3 | Put the converted base under an overlay | Agent | 15 min once the layout lands | Base never hand-edited (nugamus/iwbtg-collection#779) |
| 1.4 | Census slice: which objects need behavior | Agent | 30-60 min per game once the census exists | Script count known (research#172) |
| 1.5 | Choose the engine pack and start credits | Agent | 15 min | Pack skeleton resolves, credits listed (games#27) |
| 2.1 | Per-game questions for the user | Agent writes, human answers | 20 min agent, 15-30 min human | Answers in `docs/games/<id>.md` ([games](games/README.md)) |
| 2.2 | Behavior specs for the unique objects | Agent (research, clean room) | 30-60 min per object | Spec cites evidence and claim ids |
| 2.3 | Scripts written from the specs | Agent (sim lane) | 1-2 h per object | `pkg check` OK for each script |
| 2.4 | One feel contract per F1 mechanic, plus a trace case where the recorder has one | Agent | 30-45 min per case | Contract passes; `trace check` OK and in the fidelity baseline where a trace exists |
| 2.5 | Human recordings on the original | Human records, agent prepares | 10-20 min per recording, 1-3 per game | `.inputs` and `.csv` committed; replay check passed |
| 2.6 | Diff our run and fix the difference | Agent | 2-4 h on a proven family; 1-3 days on a new rule | Every F1 column matches within the declared tolerance, or a filed deviation with a feel test |
| 3.1 | Stripped-mode tables and the deviation log | Agent, with the answers | 1 h | Four sections filled ([games](games/README.md#deviation-log)) |
| 3.2 | Full clear recorded in our port | Human plays, agent verifies | 30-90 min human, 15 min agent | `iwbtg replay verify` prints `verified` |
| 3.3 | Playtest and feel | Human | 45-90 min human, 30-60 min agent triage | Every difference fixed, filed or accepted (#30) |
| 4.1 | Assets, credits, index and tags | Agent | 1-2 h per game | `third_party` complete; index current |
| 4.2 | Package PR and Beta | Agent | 30 min | Local `pkg check`/`pkg pack`; user playtest done |
| 4.3 | Mark Beta | Agent, human confirms | 5 min | Status in `docs/games/<id>.md` and the game page |

The path is deliberately ordered so that the scarce human steps (2.5, 3.2, 3.3) are scheduled early and can overlap with agent work on another game. An agent must never sit idle waiting for the user; the rest of the package can be prepared around an open question ([agent workflow](agent-workflow.md)).

## Stage 0: intake

Start from the shortlist entry, not from a download. The shortlist already carries the name, engine family, Delicious-fruit id and the built-in share (`shortlist.md` in the research repo).

1. **Fix the version and the paper trail.** The porter or creator picks the version; keeping the same version is preferred so records stay historical ([content](content.md#porting)). Record the source URL, creators and permission status. The converter cannot: `source_version` is not in the extracted data, `gameInfo/author` is a placeholder in all 20 projects, and the credits come from readmes and the game page.
2. **Download and clear it.** `uv run iwc download --games <game id>` stores the original by SHA-256 in `archive/`; `uv run iwc vt` clears it. The queue is slow by design: 4 requests per minute and 500 per day ([roadmap](roadmap.md#research-pipeline-rules)). A flagged file is the only human step here, because GM8 games often trigger false positives.
3. **Detect and extract.** `uv run iwc engines` then `uv run iwc extract --games <game id>`. Extraction runs no game code and writes `extracted/<slug>/` with an `extraction.json` log.

**Target:** under an hour of agent time per game, plus the VirusTotal queue. This step repeats for a game that is already in the 20-game shortlist only when the version changes.

## Stage 1: the converter base

`uv run iwc convert --game <slug>` writes `build/packages/<package_id>/` and a report at `build/reports/<slug>.md`; `--all-shortlist` converts all 20 and merges the summary (research `scraper/convert/__init__.py`). Convert all 20 together when you can: the run reads every shortlisted game's sprites and objects to build the mask-default table, and a full run keeps every package on the same converter. Both folders are gitignored.

**What the converter does automatically** (details in the converter's `converter/FORMAT_NOTES.md`):

- Resolves resource names through `_resources.list.xml`, including escaped file names, non-ASCII names, case-only pairs and unnamed `object724` resources; writes a per-game original-name-to-id map into `tests/trace-map.toml`.
- Writes `game.toml` fields the data carries: title, source URL, runner (`gm80`/`gm81`), tick rate, object order and background colour.
- Writes rooms: size, persistence, background layers, views, instances in creation order, tiles, tileset sidecars, creation-code constants as instance params or `on_create` rows, room roles and the restart flags.
- Writes sprites and backgrounds: PNGs, frame count and size, origin, collision kind, bounding box and alpha tolerance; splits nothing, but reports strips that do not fit.
- Writes sounds and music: Ogg files, the sound/music split, volume and `length_ticks`, including files a game loads through a DLL.
- Classifies most objects by parent chain and name against the hand-maintained `converter/kinds.toml`, writes presets and params, and leaves anything it cannot map as `script = "<id>"` with no script file.
- Writes the family physics profile and the `tests/trace-map.toml` skeleton.
- Verifies its own output by reading the package back and comparing it with the source; any difference fails the run. It also counts the v0 gaps that `iwbtg pkg check` will refuse ([converter requests](format/converter-requests.md)).

**What it does not do:** choose a start room beyond its heuristic, choose the version, write credits, review `third_party` entries, decide stripped-mode cuts or ranked start/end, write behavior specs or scripts, render font bitmaps, or create path data. Anything behavior-specific is the agent's work from here on.

**Step 1.1 Convert and read the report.** Run the converter, then `cargo run -p iwbtg-tools -- pkg check <package> --trace-mode`. `--trace-mode` accepts converter output: v0 gaps are warnings that name converter requests CR1-CR15 ([cli](architecture/cli.md)). The target state after this step is exactly one class of error: objects with `script =` and no file.

**Step 1.2 Triage the gaps before touching the package.** Read `build/reports/<slug>.md` and `summary.md`: they list unmapped objects and placed instances, order conflicts, dropped assets and the v0 gap counts. Every gap that is not a missing script is either a converter request or a spec question. File it in [converter requests](format/converter-requests.md) and as a research issue; fix it once in the converter, not per game. This is the highest-leverage step in the pipeline: a single mapping fix can remove hundreds of instances from the hand work.

**Step 1.3 Keep the base and the hand work apart.** The base is regenerable and never hand-edited; scripts, overrides, docs and tests are the overlay the converter never touches. The layout, override form and location are decided in nugamus/iwbtg-collection#779; until it lands, keep hand work outside the package folder and re-apply it after a re-convert. The package the client loads is base plus overlay.

**Step 1.4 Take the census slice.** The census (research#172) classifies every unmapped object across the 20 games: covered by a built-in, coverable by a new preset or built-in, menu-only, or a unique script. Use its slice for this game to set the script budget before writing anything. A cluster that appears in three or more games is a built-in candidate under 0004; two games use a versioned shared script, not a per-game copy ([content](content.md#engines), [0004](decisions/0004-extension-model.md)).

**Step 1.5 Choose the engine pack.** A game whose physics match its family uses the shared pack for the Kid, blood, saves and sounds, so those assets are not copied into the package. The skeletons exist but their assets are still pending (games#27); until they are filled, the game carries its own copies and its `[[third_party]]` entries. Choosing the pack is 15 minutes; filling the pack is a one-time job outside the per-game path.

**Target for Stage 1:** 2.5-3 agent-hours per game, excluding converter changes (a shared 0.5-2 day cost) and the census (which this stage consumes).

## Stage 2: behavior

**Step 2.1 Ask the game's questions once.** Every game has decisions only the user can make: what stripped mode cuts, where the ranked run starts and ends, how cutscenes and dialogue are handled, which items are secrets, which mods apply ([games](games/README.md)). Ask them all in one batch in `docs/games/<id>.md`, with a proposed default where the answer is not irreversible. These answers unblock stripped mode, the deviation log and the ranked start. Do not guess them in the package.

**Step 2.2 Spec the unique objects.** Clean-room rules apply: behavior comes only from `research/specs/` and the traces, never from another runner's source or a decompiled game (client `CLAUDE.md`). The current default until nugamus/iwbtg-collection#780 decides otherwise is option 1: a research session writes a short behavior spec per object from the GML, and a sim session writes the script from that spec. The spec cites its evidence and a claim id.

**Step 2.3 Write the scripts.** The script cites the spec, not the original. Run `pkg check` after each script so a missing API function surfaces immediately. When a planned script API function blocks a script, file it against Script API v1 (#776) instead of working around it inside the script; a workaround is a rule the sim later has to undo ([scripts](architecture/sim/scripts.md)).

**Step 2.4 Add one trace case and one feel contract per F1 mechanic.** A trace case is short scripted input, run in our port and diffed against the original's recorder trace. Rely's four cases (`stand`, `djump`, `right`, `num-rng-a`) are the baseline-needle set; a game with a boss, avoidance, vines, water or a moving platform needs more. Cases live in `crates/tools/cases/<package id>/` and enter `crates/tools/fidelity-baseline.txt`; `cargo xtask fidelity` runs them all after a sim or converter change, with `IWBTG_RESEARCH` set ([traces](architecture/traces.md), [testing](architecture/testing.md)). The feel contract states the player-observable outcome for the same mechanic (a jump envelope, retry timing, reset scope, collision outcome) and names the trace as evidence; it lives in `crates/tools/feel/<area>/<name>.toml`, names the package by its id and enters `crates/tools/feel-baseline.txt` ([feel contracts](architecture/feel-contracts.md)). Its id (`<area>/<name>`) is what the triage comment and the deviation log cite. **One command checks every port's contracts:** `cargo xtask feel` with `IWBTG_GAMES` and/or `IWBTG_RESEARCH` set (for example `IWBTG_GAMES=../iwbtg-collection-games IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask feel`). It runs the public set, then every contract in `crates/tools/feel/` against `crates/tools/feel-baseline.txt`. A contract's package id is looked up in this order: the games repo (`IWBTG_GAMES/<id>`, the hand-worked port, loaded strictly), then the converter output (`$IWBTG_RESEARCH/build/packages/<id>`, the same ids, loaded like the trace sweep with unmapped kinds inert). Each result line names the source it ran against (`[games: <id>]` or `[converter: <id>]`). A package in neither is SKIP with the folders searched: not a result, not a regression, and `--bless` keeps its last baseline entry. A package that is found but does not load is FAIL and never falls through to the next source. So a contract written before its port lands runs against the converter package; when the port moves into the games repo it runs there instead, and a changed status is blessed in that port PR with `cargo xtask feel --bless`. With neither variable set the command says that it did not run the port set. F2 mechanics need a screenshot or audio check instead; F3 details need only the pinned hash.

**Step 2.5 Get the human recording.** Agents never play whole games. The user plays a short segment of the original in the research recorder, with the exact command prepared by the agent (room, variables, output name); the agent commits the `.inputs` and `.csv` and imports the case. The recorder checks its own replay before writing anything. The guide is `docs/recording-inputs.md` in the research repo. Schedule this early: it is the first hard human dependency, and one 15-minute recording can unblock a day of diff work.

**Step 2.6 Diff, contract and fix.** Run the trace case, read the first differing frame and field, and fix the sim, the package or the converter — never the trace. Then run the feel contract: if the player-observable outcome still holds, the difference is F3 and is recorded; if the outcome moved, it is F1 and the trace stays the evidence. If the difference is a new engine rule, it becomes a spec/trace issue and stays OPEN; do not invent the answer. On a proven family this is usually a few hours. When a game exposes a rule the family never needed, it costs 1-3 days and benefits every later game on that family. Do not clone an implementation detail to make a probe column match: file the redesign (data, script, profile or standard ruleset) and mark its F tier.

**Target for Stage 2:** 4-5 agent-days for a baseline needle game like rely (10 objects: about 2.5-3 days of specs and scripts, half a day of trace cases, up to a day of diffing), and about 2 weeks for a game with a boss or avoidance and 25 or more unique objects. One 20-40 minute human recording session is part of this stage.

## Stage 3: proof

**Step 3.1 Finish the paper trail.** Fill the deviation log's four sections — kept quirks, fixed bugs, known differences, open — with a tier and evidence on every entry ([games](games/README.md#deviation-log), [fidelity tiers](plans/fidelity-tiers.md)). Stripped-mode tables come from the answers in 2.1. Write the per-game page while the evidence is fresh; the game page's "known differences" is generated from this log, so there is no second list.

**Step 3.2 Record a full clear.** Clearability is proven by a human recording a full clear in our port, as a replay the headless CLI verifies; during Beta, player clears count as proof too ([content](content.md#proving-a-port)). `iwbtg replay verify <dir> <replay>` reproduces the seven `replay run` lines and prints `verified` or one `mismatch:` line per difference ([cli](architecture/cli.md)). A clear the user cannot produce is a port problem, not a paperwork problem.

**Step 3.3 Playtest and feel.** A human playtest checks the feel, and for rely also compares rooms side by side with the original: backgrounds, tiles and depth order (#30). Native was the first target; the web playtest follows the web build (#29). Every difference the user sees becomes an issue with an F tier or is accepted in the deviation log. Visible F2 differences block Beta; F1 blocks Ranked ([fidelity tiers](plans/fidelity-tiers.md)). A playtest can add a feel contract or narrow a tolerance; a deliberate deviation is recorded in the game's deviation log with the contract that proves its impact. A kept difference that cannot name a contract, a trace or a dated playtest is not accepted.

**Target for Stage 3:** 2-3 agent-hours plus 45-90 minutes of human play, and the full clear.

## Stage 4: Beta

**Step 4.1 Assets, credits and the index.** Review every non-ours asset the package carries: sounds, music, sprites, fonts and the Kid sprite. The converter writes a `[[third_party]]` skeleton from sound file names, but the owner, license and status are manual, and 92 original sound names are non-ASCII; four games load audio through a DLL. Add each entry to `game.toml` or the engine pack ([engine packs](format/engine.md)). Then run `iwbtg assets index <games dir>`; it lists duplicates and leaves a `tags = []` per asset for the porting checklist (#104). Origin credit is not a Beta gate; the package does not carry a release date yet, so the index lists every place and the command prints the first game in folder order ([content](content.md#assets)).

**Step 4.2 Open the package PR.** From the games repo, run the local check the repo requires for every touched package: `cargo run -q --release --manifest-path ../client/Cargo.toml -p iwbtg-tools -- pkg check <package>`. `iwbtg pkg pack` must produce the `.iwbz` with a `gameplay_hash` and a `content_hash` ([cli](architecture/cli.md)). The PR body records the trace result, the verified replay, the deviation log and the playtest. A port PR waits for the user's playtest before it merges (games repo rules).

**Step 4.3 Mark Beta.** Status is Beta when the fidelity proof, the verified clear, the deviation log and the playtest exist. Ranked is a later gate: every F1 item answered and proven ([fidelity tiers](plans/fidelity-tiers.md)).

**Target for Stage 4:** 2-3 agent-hours plus the merge wait.

## Time budget

An agent-day is 6-8 hours in one lane. Human time is listed separately because it overlaps with agent work on another game.

| Port | Agent time | Human time | Calendar in one lane |
|---|---|---|---|
| First port on a new engine family | 4-8 weeks | 4-6 h | 5-8 weeks |
| Repeat port, proven family, ~10 unique scripts (rely-like) | 4-5 agent-days (30-40 h) | 2-3 h | about 1.5 weeks, with human steps overlapped |
| Repeat port, 25+ unique scripts (a boss or avoidance) | 9-11 agent-days (65-85 h) | 2-4 h | about 2-2.5 weeks |
| If the census leaves ~150 scripts per game | 30+ agent-days | - | 6+ weeks |

The 19 remaining games at 1.5-2 lane-weeks each is about 29-38 lane-weeks: roughly 7-9 months with one port lane, 3.5-5 months with two, and 2.5-3 months with three. The target is to move the average toward the rely baseline by promoting every repeated cluster to a built-in, because each promotion removes its script cost from every game that uses it. If the census leaves about 150 unique scripts per game instead of the ~10 rely needed, the script step alone is 225-300 agent-hours per game and the schedule is dominated by it. That is why the census and the built-in promotion rule come before mass porting.

## Bottlenecks found porting rely_the_needle

### Converter gaps

Rely exposed the converter's structural gaps one at a time, each found after a package had already been built:

- The first `pkg check` on the rely package failed on a warp without its target room (research#93, found 2026-11-04). The same class of error existed across most games: 20 warp errors, 14 music objects without a track and a missing `game.toml` (research#120).
- Rely's `objEffect` turned out to be a particle system that the original creates from the world's Room Start in every room; the converted package never spawned it, so its RNG draws were missing (research#186). That is an F1 difference hidden in what looked like visual effects. It took a converter change (CR23/CR24) and re-conversion.
- The converter keeps improving: about 40 converter PRs landed between 2026-11-03 and the audit, so every improvement means re-converting (nugamus/iwbtg-collection#779).

**What the pipeline does about it:** convert all 20 in one run, read the report before editing, and file every non-script gap as a CR plus a research issue before starting per-game work. Fix the converter once. Keep the base and hand work apart (#779) so a re-convert does not overwrite scripts, and run `cargo xtask fidelity` after the converter changes.

### Script work

Rely's package passed `pkg check` except for 10 objects with `script =` and no file (games#31). Three of those are title/menu objects that Play original needs and ranked mode does not (#100). Across the 20 packages the same check reported 3,042 missing scripts (research#120), from 967 source scripts; rely is the easy end of the range (built-in share 0.996).

The clean-room rule makes each object two passes today: a research session writes the spec, a sim session writes the script. The census (#172) is meant to show how many of the 3,042 are near-duplicates, preset variants, shared built-ins or menu-only, and Script API v1 (#776) must cover what the remainder uses. Until those land, the 3,042 is the only known number and it dominates the schedule.

**What the pipeline does about it:** census first, built-in or preset before script, spec and script batched per object, and short trace cases instead of hand play. Do not write menu behavior for ranked mode; it is Play original work (#100) and F2 by scope.

### Assets and credits

The converter copies assets, but the credit work is manual: creators, `source_version`, permissions, `[[third_party]]` owners and licenses. The 20-game survey found 92 non-ASCII original sound file names, four games that load audio through DLLs, and a track with an HTML prefix before the MP3 data. The other extraction quirks are reported rather than fixed: seven sprites without frames, seven sprites whose strip is over 4096 px, escaped and non-ASCII resource names, and fonts with no glyph bitmaps.

The engine packs are the intended dedupe: 260 sprites appear in two or more games and 68 in ten or more, and 14 sound files are identical in ten or more games. The pack skeletons exist but their assets are pending (games#27), so rely still carries its own copies.

**What the pipeline does about it:** one asset and credit pass per game at Beta, one shared pack fill outside the per-game path, and tags while crediting (#104). Do not block the port on origin credit; the index prints a provisional origin until packages carry a release date ([content](content.md#assets)).

### Playtest and the human path

Three human steps sit on the critical path and cannot be automated:

1. **Recordings on the original** (2.5): short segments, but only the user can play them. The recorder runs the original only through the research gate and only when VirusTotal-clean (the recording guide is `docs/recording-inputs.md` in the research repo).
2. **The full clear** (3.2): the user must clear our port; a verified replay is the Beta proof.
3. **The playtest and feel** (3.3): native now, web after #29 (#30). For rely it also checks backgrounds, tiles and depth order against the original.

**What the pipeline does about it:** prepare the exact command and room for every recording, batch all recordings and the playtest into one session per game where possible, and let agents continue on other packages while a human step is pending. No agent may sit idle waiting for the user.

## Dependencies and what is still in flight

State on 2026-11-20. Each row names what it changes in this page.

| Dependency | State | What it changes |
|---|---|---|
| games#1, the rely port | Open. Package exists only in `research/build/packages/`; scripts are games#31; playtest is #30. | The sample, not a prerequisite. Its unfinished parts are the four bottlenecks above. |
| nugamus/iwbtg-collection#779, port layering | Open | Steps 1.3 and 4.2: where a port lives and how re-conversion keeps hand work. Until it lands, keep hand work out of the converted base. |
| nugamus/iwbtg-collection#780, clean-room authoring | Open; default option 1 (spec then script) | Steps 2.2 and 2.3. If it changes, the two steps merge into one pass. |
| research#172, census | Open; no census file yet | Step 1.4 and the whole script budget. Until it lands, use the report's unmapped count and the 3,042 total. |
| nugamus/iwbtg-collection#776, Script API v1 | Open | Step 2.3: a planned function can block a script. File it, do not work around it. |
| nugamus/iwbtg-collection#775 and #1002, Rhai at scale | Open | Confirms scripts stay cheap enough before mass porting; may change the script target. |
| nugamus/iwbtg-collection#29, web build | Open | Step 3.3: playtest native now, web when it lands. |
| games#27, engine pack assets | Open | Step 1.5 and the asset pass: rely carries its own copies until the packs are filled. |
| games#30, games repo CI | Open; Actions do not run there | Step 4.2: check the package locally, as the games repo requires. |
| client#960, rely blood destruction | Open | Step 2.6: the `num-rng-a` case ignores its blood-count column until this closes. Settle or accept it in the deviation log before Beta if the difference is visible. |
| #104, asset tagging | Open | Step 4.1: tags may be filled after Beta; they are not a gate. |

## Checklist for a port issue

Each phase 9 port issue copies this list and splits it into sub-issues; games#1 carries the same list from the milestone review. Every item names its owner and its gate so an agent can start without re-reading this page.

- [ ] **Version and source recorded** (agent, 15 min): version chosen, URL, creators, permission status.
- [ ] **Downloaded and VirusTotal-clean** (agent; human only on a flag).
- [ ] **Extracted** (agent, 15 min): `extraction.json` ok.
- [ ] **Converted and checked** (agent, 15 min): converter checks OK, only missing-script gaps.
- [ ] **Gaps triaged** (agent, 30-60 min): every non-script gap is a CR or a spec issue, not a hand edit.
- [ ] **Base under overlay** (agent, 15 min; #779): the base is regenerable, the hand work is separate.
- [ ] **Census slice read** (agent, 30-60 min; research#172): unique scripts, built-in candidates, menu-only objects.
- [ ] **Questions answered** (agent writes, human answers, 15-30 min): `docs/games/<id>.md` open section.
- [ ] **Specs written** (agent, research, 30-60 min per object): clean room, each cites evidence.
- [ ] **Scripts written** (agent, 1-2 h per object): `pkg check` OK; every blocker filed against #776.
- [ ] **One trace case per mechanic** (agent, 30-45 min each): `trace check` OK, baseline updated.
- [ ] **Human recordings on the original** (human 10-20 min each; agent prepares and imports).
- [ ] **Differences fixed or filed** (agent, 2-4 h, more on a new rule): no unexplained trace column.
- [ ] **Stripped tables and deviation log** (agent, 1 h): tiers and evidence on every entry.
- [ ] **Full clear verified** (human 30-90 min; agent verifies): `iwbtg replay verify` says `verified`.
- [ ] **Playtest done** (human 45-90 min): every difference fixed, filed or accepted (#30).
- [ ] **Assets and credits complete** (agent, 1-2 h): `third_party` reviewed; `iwbtg assets index` current.
- [ ] **Package PR** (agent, 30 min): local `pkg check` and `pkg pack`; user playtest recorded.
- [ ] **Beta** (agent, 5 min): the status is written and the milestone's done-when can be ticked.
