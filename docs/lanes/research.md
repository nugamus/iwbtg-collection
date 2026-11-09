# Lane: research

Works in the main `research/` checkout, one agent at a time: crawls, downloads, extraction, traces and the converter need `archive/`, `extracted/`, `db/` and `tools/`, which exist only there. Read `research/CLAUDE.md` first; its safety rules override everything here.

**Owns:** `research/` (specs, scraper, converter, traces, knowledge).

## Build and test

- `uv run pytest -q tests` and `python -I .claude/hooks/guard_archive.py --test`.
- The repo is private and has no CI: paste the local results into the PR and merge (decided 2026-11-05, #527).

## Pitfalls seen before

- Never open or run anything from `archive/` or `extracted/` except through `scraper/gate.py`.
- The OpenGMK source in `tools/opengmk/src/` is only for rebuilding the tracer; never read it while writing specs.
- A spec answer that client issues wait for: comment on those issues when it lands, so the sim lane picks it up.
