# Fuzz targets

Fuzzing for the inputs a stranger supplies (#809): `.iwbz` archives, package files, replays and
scripts. This crate is workspace-excluded (`[workspace]` at the end of `Cargo.toml`), so a build in
the parent folder never compiles it.

| Target | Input | Owner |
|---|---|---|
| `iwbz` | `.iwbz` bytes through `check_entry` and `unpack` | #992 |
| `package_load` | unpacked files through `load_report` and the check | #993 |
| `replay` | `.iwbr` bytes through `Replay::decode` | #994 |
| `script` | Rhai source through compile and a bounded run | #995 |

`iwbz` has its body (#992); the other three are stubs until #993-#995 land.

## Run

```sh
cargo install cargo-fuzz --locked   # once
cd fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run iwbz -- -max_total_time=600
```

Nightly and a Linux or macOS host are needed for libFuzzer; the scheduled job
(`.github/workflows/fuzz.yml`) runs the same command on Linux.

`corpus/<target>/` is the seed corpus and is committed. The `iwbz` seeds are the packed valid
fixtures: `cargo run -p iwbtg-tools -- pkg pack crates/format/tests/valid/<fixture> -o
fuzz/corpus/iwbz/<fixture>.iwbz`. libFuzzer adds generated inputs to it: commit one only when it
reaches new code, and prune the rest. A crash lands in `artifacts/<target>/`, which is gitignored.
