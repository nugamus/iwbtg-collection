# The simulation: Determinism and purity

Part of [the simulation](../sim.md).

## Determinism and purity

The state hash itself is on [State hash](hash.md). This page holds the rules that keep the sim reproducible across targets. It is the live text of sections 10.2-10.6 of [the sim core plan](../../plans/sim-core.md), which stays only as history; this page is the authority.

### NaN and infinity

`Real`'s `Eq` compares raw bits, and NaN payloads and signs differ between x86_64, wasmtime and browser engines. Built-ins can make non-finite values (`sqrt` of a negative, `x/0` such as machinary's `range / Hand_speed` alarms, `atan2` of infinities). Rule (Design, N15): every `Motion`, `Image`, alarm, view and variable setter, called from a built-in or a script, rejects a non-finite value with `SimError::NonFinite`, which halts the sim ([tick pipeline](tick.md), Halting). Intermediate non-finite values inside one `num` function are allowed only if the function's result is finite. A golden test reaches this path on purpose (Cross-target checks).

### Integer overflow

- `rng.rs` uses `wrapping_mul`/`wrapping_add` (the LCG wraps by definition).
- Counters (`tick`, `room_tick`, `next_id`, deaths, time, grid math) use `checked_*` and return `SimError::Overflow`.
- `[profile.release] overflow-checks = true` in the root `Cargo.toml`, so debug tests and release servers behave the same.

### Features

Cargo resolver 3 still unifies features across members built in one command, so a server built next to tools would get `trace-setup`. Rules: the server binary refuses to start when `cfg!(feature = "trace-setup")`; CI builds the server with `-p`; `HashView` carries `setup_applied`, so a run that used setup can never match a clean verification.

### Lints (`crates/sim/clippy.toml`)

Beyond the existing purity bans:

- `disallowed-methods`: `f64::min`, `f64::max` (signed-zero result unspecified, lowered differently on x86 and WASM; use `num::gm_min/gm_max` or `num::min_bits`), `f64::clamp` (panics on NaN or lo > hi), `f64::to_degrees`, `f64::to_radians` (use `num::radtodeg/degtorad`), `std::io::stdin`, `std::fs::read`, `std::fs::write`, `std::fs::read_to_string`, `std::fs::File::open`, `std::fs::File::create`, `std::process::exit`.
- `disallowed-types`: `std::fs::File`, `std::net::TcpStream`, `std::net::TcpListener`, `std::net::UdpSocket`, `std::process::Command`, `std::hash::DefaultHasher`, `std::hash::RandomState`.
- `lib.rs`: `#![deny(clippy::print_stdout, clippy::print_stderr, clippy::float_cmp, clippy::cast_possible_truncation)]`; existing hits in `num`, `input`, `replay` and `trace` are fixed. Existing `#[cfg(test)]` uses of `std::fs` in `trace.rs` carry a scoped `#[allow]` with a reason.

### Cross-target checks

Golden engine rooms (`crates/sim/tests/golden/*.rs`, built with `def::testing::GameDefBuilder`) store a hash list per tick. They run on Windows x86_64, Linux x86_64, wasm32-wasip1 under wasmtime, **and** wasm32-unknown-unknown under node via `wasm-bindgen-test` (the shipped web target: different NaN bits, panics abort). One golden case reaches the NaN and infinity path above and expects `Halted(NonFinite)` on all four. The CI layout is in [Testing strategy](../testing.md).
