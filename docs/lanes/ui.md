# Lane: ui

**Owns:** `crates/client/src/ui/` (UI kit from #490: fonts, tokens, sheared shapes, widgets, motion, `--ui-gallery`), the screen modules, `crates/client/src/view/hud.rs`, bundled fonts, `docs/decisions/0003-ui-toolkit.md`, `docs/architecture/client/ui.md` and `client/hud.md`.
**Never edits:** `crates/sim`, `crates/format`, the settings and stats modules (data lane; you only read them).

## Build and test

- Rebuild after an edit: `cargo build -p iwbtg-client` (seconds when warm).
- Tests: `cargo test -p iwbtg-client <module>::`.
- See it: `cargo run -p iwbtg-client -- crates/client/demo-art.pkg --mute --screenshot <out.png>` (or `--ui-gallery --mute`). Always pass `--mute` when launching the client (#712). `--window WxH` and `--ratio` pick the size.
- Compare with the approved design: `python -I .claude/screenshot_diff.py <yours.png> <reference.png>` (#531).

## How a screen is built

1. Read `docs/design/mockups/m4/README.md`, then the `.dc.html` artboard the issue names: its markup holds sizes and colours, its script the sample data.
2. Use the #490 kit and tokens, never one-off styles.
3. The PR attaches `--screenshot` images at the ratios the issue lists. The lead merges when they match; the user tests `main` in batches.

## Pitfalls seen before

- Game art (delicious fruit, key art) comes from the loaded package at run time; never commit extracted art here.
- A new CLI flag or plugin goes in your own module (an `Args` group and a `Plugin`), not into `main.rs` and `view::run` arguments (#530), so lanes don't conflict there.
- CJK text needs the fallback font (#502); check a Japanese title in a screenshot.
