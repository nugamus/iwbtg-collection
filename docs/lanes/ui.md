# Lane: ui

**Owns:** `crates/client/src/ui/` (UI kit from #490: fonts, tokens, sheared shapes, widgets, motion, `--ui-gallery`), the screen modules, the HUD module directory `crates/client/src/hud/` and its sign/countdown rendering in `crates/client/src/view/hud.rs`, bundled fonts, `docs/decisions/0003-ui-toolkit.md`, `docs/architecture/client/ui.md` and `docs/architecture/client/hud.md`.
**Never edits:** `crates/sim`, `crates/format`, the settings and stats modules (data lane; you only read them).

## Build and test

- Rebuild after an edit: `cargo build -p iwbtg-client` (seconds when warm).
- Tests: `cargo test -p iwbtg-client <module>::`.
- See it: `cargo run -p iwbtg-client -- crates/client/showcase.pkg --mute --screenshot <out.png>` (or `--ui-gallery --mute`). Always pass `--mute` when launching the client (#712). `--window WxH` and `--ratio` pick the size.
- Screenshots come only from the client's `--screenshot` flag (`WGPU_BACKEND=dx12` when the display is locked). Never take OS or desktop screen captures or use a screen-capture tool: on 2026-11-15 two captures of the user's main screen were taken for #495 (#822). Key presses in a live window can't be done by an agent; tests cover them and the user checks on `main`.
- Attach screenshots to a PR: copy the PNGs to folder `<issue>/` on the `pr-assets` branch (`git worktree add <temp>/assets origin/pr-assets`, commit, `git push origin HEAD:pr-assets`, remove the worktree) and link `https://raw.githubusercontent.com/nugamus/iwbtg-collection/pr-assets/<issue>/<name>.png` in the PR body. Keep scratch files in your own temp folder, not in the worktree.
- Compare with the approved design: `python -I .claude/screenshot_diff.py <yours.png> <reference.png>` (#531).
- The approved artboards render locally (#628): `docs/design/mockups/m4/png/` holds a reference PNG per approved screen, drawn from the `.dc.html` sources by `python -I docs/design/mockups/render.py m4` (headless Chrome; our own renderer `dc-render.js`, not the canvas runtime). Render a state the PNGs lack with its prop in the query, e.g. `render.py m4 "Main?state=offline" --out <temp>`, and diff your `--screenshot` against it with the command above at the same window size (`--window 1920x1080`). The references draw a placeholder where the canvas shows the delicious fruit (fangame art is not committed), and the canvas stays the authority when the two disagree; `render.py --serve` opens the artboards in a browser.

## How a screen is built

1. Read `docs/design/mockups/m5/README.md` (approved 2026-11-26 except section W; its Implementation table lists the work in order) and `docs/design/mockups/m4/README.md`, then the `.dc.html` artboard the issue names: its markup holds sizes and colours, its script the sample data. Where m5 redraws an m4 screen, m5 wins. m5 has no committed PNGs: render a reference with `python -I docs/design/mockups/render.py m5 <Board> --out <temp>` (needs the fruit blob in `docs/design/mockups/blobs/` for the real sprite, else a placeholder).
2. Use the #490 kit and tokens, never one-off styles.
3. The PR attaches `--screenshot` images at the ratios the issue lists. The PR owner checks that they match the approved artboard, runs the User check and requests `gh pr merge <N> --squash --auto`; the user tests `main` in batches. A design or cross-area contract question still seeks the user or a domain review.

## Pitfalls seen before

- Testers play `crates/client/showcase.pkg` (#852); unit tests open the single-feature packages in `crates/client/tests/fixtures/`. A new client feature that a tester should see goes into the showcase (original art only; the Kid there is the one exception), its test into a fixture.
- Game art (key art, a game's own fruit) comes from the loaded package at run time; never commit a game's extracted art here. The one exception is the client's own delicious fruit, `crates/client/assets/ui/cherry.png` (credited in its `CREDITS.md`, user decision 2026-11-26, #1608).
- Game select screenshots now need `--select-preview time-attack` (or `classic`): plain `--select` opens on the main menu (`--menu-preview <initial|top|play|classic>` for its states).
- A new CLI flag or plugin goes in your own module (a `Registration`: its `Args` group and `Plugin`) plus one line in the module list in `main.rs`, not into `main.rs` itself or `view::run` arguments (#530, #948), so lanes don't conflict there.
- CJK text needs the fallback font (#502); check a Japanese title in a screenshot.
- Game select screenshots: copy `showcase.pkg` and the fixtures into a temp folder and pass it to `--select`; the first name in sort order is the one selected (`maps.pkg` has maps and key art, so copy it as `0-maps.pkg`). A `--screenshot` run keeps no stats, so no PB or Cleared mark shows (#149). In Git Bash give the client `cygpath -m` paths: a `cygpath -w` path with `\` joined on lost its separator and the screenshot did not save.
- Preserve feel, not implementation: a visible UI or rendering change is F2 and needs `--screenshot` evidence; a change that moves state, timing or input handling is F1 and needs a feel contract; an invisible refactor is F3 and is free to redesign ([fidelity tiers](../plans/fidelity-tiers.md), [0005](../decisions/0005-standard-mechanics.md)).
