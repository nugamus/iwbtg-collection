//! The per-tick step budget (decision 0001 "Step budget"). WP9.1 owns it.
//!
//! One budget unit is one Rhai operation (one `on_progress` callback). Host functions that do
//! real work charge extra units to the same counter (WP9.2 fixes the cost table). The tick-wide
//! counter resets at the start of every tick. Exceeding the tick budget or a single call's
//! budget ends the call with `ErrorTerminated`, which is a deterministic script error: a runaway
//! script halts the sim the same way on every target.

use std::cell::Cell;

/// Default units across all script calls in one tick.
pub const DEFAULT_TICK_BUDGET: u64 = 1_000_000;
/// The largest tick budget a package may ask for.
pub const MAX_TICK_BUDGET: u64 = 2_000_000;
/// Default units for one event call.
pub const DEFAULT_CALL_BUDGET: u64 = 200_000;

/// Budget units a host function charges on top of the one unit every operation costs
/// (decision 0001 "Step budget"; the generated `docs/format/script-api.md` lists them).
/// Rows are (function, units per call, units per item it returns). A function not listed costs
/// only its operation.
pub const COSTS: &[(&str, u64, u64)] = &[("instance_create", 50, 0), ("instances", 0, 1)];

/// The extra units `name` charges: (per call, per item). `(0, 0)` for a function not listed.
#[must_use]
pub fn cost(name: &str) -> (u64, u64) {
    COSTS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map_or((0, 0), |(_, base, item)| (*base, *item))
}

/// The budget counters. Interior mutability is allowed here only (architecture "Scripts"): the
/// Rhai callbacks hold a shared reference.
#[derive(Debug)]
pub struct Budget {
    tick_limit: Cell<u64>,
    call_limit: u64,
    used: Cell<u64>,
    peak: Cell<u64>,
}

impl Budget {
    /// A budget with the default limits.
    pub fn new() -> Budget {
        Budget::with_limits(DEFAULT_TICK_BUDGET, DEFAULT_CALL_BUDGET)
    }

    /// A budget with explicit limits (the tick limit is capped at [`MAX_TICK_BUDGET`]).
    pub fn with_limits(tick: u64, call: u64) -> Budget {
        Budget {
            tick_limit: Cell::new(tick.min(MAX_TICK_BUDGET)),
            call_limit: call,
            used: Cell::new(0),
            peak: Cell::new(0),
        }
    }

    /// Sets the tick limit (a package may raise it in `game.toml`, up to the cap).
    pub fn set_tick_limit(&self, tick: u64) {
        self.tick_limit.set(tick.min(MAX_TICK_BUDGET));
    }

    /// The tick limit.
    pub fn tick_limit(&self) -> u64 {
        self.tick_limit.get()
    }

    /// The per-call limit.
    pub fn call_limit(&self) -> u64 {
        self.call_limit
    }

    /// Starts a tick: the counter goes back to 0 (the peak is kept for `iwbtg profile`).
    pub fn begin_tick(&self) {
        self.used.set(0);
    }

    /// Units used so far in this tick.
    pub fn used(&self) -> u64 {
        self.used.get()
    }

    /// The most units any tick has used since creation.
    pub fn peak(&self) -> u64 {
        self.peak.get()
    }

    /// Adds `units` and reports whether the tick budget still holds. `false` means exceeded.
    pub fn charge(&self, units: u64) -> bool {
        let used = self.used.get().saturating_add(units);
        self.used.set(used);
        if used > self.peak.get() {
            self.peak.set(used);
        }
        used <= self.tick_limit.get()
    }

    /// The `on_progress` rule: one unit for the operation, and `true` when the call must end
    /// (`call_ops` is Rhai's count of operations in this call so far).
    pub fn exceeded(&self, call_ops: u64) -> bool {
        !self.charge(1) || call_ops > self.call_limit
    }
}

impl Default for Budget {
    fn default() -> Budget {
        Budget::new()
    }
}
