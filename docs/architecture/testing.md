# Testing strategy

Part of [Architecture](../architecture.md).


| Layer | What | Where | When |
|---|---|---|---|
| Unit tests | `num` (rounding, comparisons, trig table), `rng`, motion coupling, masks and bounding boxes, collision queries, input edges, replay round trip | `crates/sim`, `crates/format` | every `cargo test` |
| Script conformance | Every IWS math function over 10,000 fixed inputs, every operator and overload, operation-count fixtures | `crates/sim/tests/script_conformance/` (hashes pinned in `expected.txt`; `UPDATE_CONFORMANCE=1` rewrites) | every CI target |
| Engine test rooms | Small worlds we make ourselves (no creator content, built with `GameDefBuilder`), one pinned state hash per case in `expected.txt`. Probe cases, one kid course per kid family (A1 to C, `golden/courses.rs`: engine kinds, a floor, warps and a group of built-ins under a scripted input of runs, jumps, shots and retries) and a script API case whose scripts call every function and property of the script API. After the case lines, `expected.txt` names per built-in and per kid family the first case that had a live instance of it (`builtin <name> <case>`, `family <name> <case>`); `golden_corpus.rs` fails when one names no case (only `inert`, a trace-mode kind, is allow-listed) or when a script API name from `docs/format/script/iws.d.rhai` is missing from the golden scripts (#786). Coverage means the kind ran in the case, not that the kid touched every instance. A new built-in or family goes into a course in the same PR. A deliberate sim change re-pins it: `cargo xtask repin` | `crates/sim/tests/golden/` | every CI target |
| Validator fixtures | A set of broken packages, each expecting one specific error with file, line and column (and every `valid/` package must pass the full load, as `pkg check` does, with no warnings) | `crates/format/tests/invalid/` | every `cargo test` |
| Client package fixtures | Small original packages, one feature each (`input`, `audio`, `save`, `particles`, `clear`, `maps`: three maps and its own key art for game select), that the client's unit tests open; placeholder art, not for testers, who play `crates/client/showcase.pkg` (#852). Change a fixture only with the tests that read it | `crates/client/tests/fixtures/` | every `cargo test` |
| Trace tests | Every research trace case, run and diffed bit for bit | research path given by the `IWBTG_RESEARCH` environment variable; skipped when absent | locally, in a research CI job, before ranking a game |
| Replay corpus | Each ported game's full-clear replay, plus every stored leaderboard replay once Phase 8 exists | games repo, server | every sim change |
| Cross-platform equality | `iwbtg determinism report` on golden rooms, conformance suite and replay corpus on `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `wasm32-wasip1` under wasmtime and `wasm32-unknown-unknown` through wasm-bindgen-test in Node. A final job requires all hash lists to be byte-identical. | GitHub Actions matrix | every PR touching `sim` or `format` |
| Fuzzing | Replay decoder, package loader, script host | `cargo-fuzz`, native | nightly |
| Budget and speed | Peak script units under 25% of the budget; benchmarks of the Rori Girl and Easy Miku 2 avoidances and a 28,900-instance load, native and WASM | `iwbtg profile`, `cargo bench` | CI, early in Phase 3 |
| Lints | `cargo clippy -D warnings` with the sim bans, `cargo deny check`, `cargo fmt --check` | CI | every PR |

The roadmap Phase 3 check is the trace-test row plus the cross-platform row: the Phase 2 inputs reproduce the original traces exactly on Windows, WASM and Linux x86_64.
