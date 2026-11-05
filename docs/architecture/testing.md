# Testing strategy

Part of [Architecture](../architecture.md).


| Layer | What | Where | When |
|---|---|---|---|
| Unit tests | `num` (rounding, comparisons, trig table), `rng`, motion coupling, masks and bounding boxes, collision queries, input edges, replay round trip | `crates/sim`, `crates/format` | every `cargo test` |
| Script conformance | Every IWS math function over 10,000 fixed inputs, every operator and overload, operation-count fixtures | `crates/sim/tests/script_conformance/` (hashes pinned in `expected.txt`; `UPDATE_CONFORMANCE=1` rewrites) | every CI target |
| Validator fixtures | A set of broken packages, each expecting one specific error with file, line and column | `crates/format/tests/invalid/` | every `cargo test` |
| Engine test rooms | Small worlds we make ourselves (no creator content, built with `GameDefBuilder`), one pinned state hash per case in `expected.txt`. A deliberate sim change re-pins it: `UPDATE_GOLDEN=1 cargo test -p iwbtg-sim --test golden_corpus`. More cases per built-in come with their work packages | `crates/sim/tests/golden/` | every CI target |
| Trace tests | Every research trace case, run and diffed bit for bit | research path given by the `IWBTG_RESEARCH` environment variable; skipped when absent | locally, in a research CI job, before ranking a game |
| Replay corpus | Each ported game's full-clear replay, plus every stored leaderboard replay once Phase 8 exists | games repo, server | every sim change |
| Cross-platform equality | `iwbtg determinism report` on golden rooms, conformance suite and replay corpus on `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `wasm32-wasip1` under wasmtime and `wasm32-unknown-unknown` through wasm-bindgen-test in Node. A final job requires all hash lists to be byte-identical. | GitHub Actions matrix | every PR touching `sim` or `format` |
| Fuzzing | Replay decoder, package loader, script host | `cargo-fuzz`, native | nightly |
| Budget and speed | Peak script units under 25% of the budget; benchmarks of the Rori Girl and Easy Miku 2 avoidances and a 28,900-instance load, native and WASM | `iwbtg profile`, `cargo bench` | CI, early in Phase 3 |
| Lints | `cargo clippy -D warnings` with the sim bans, `cargo deny check`, `cargo fmt --check` | CI | every PR |

The roadmap Phase 3 check is the trace-test row plus the cross-platform row: the Phase 2 inputs reproduce the original traces exactly on Windows, WASM and Linux x86_64.
