//! Built-in `world`: the engine controller (docs/plans/sim-core.md 6.1). Runs `RestartModel` and
//! `ResetModel`, world-Step direct reads (`latch_cleared_by_world`), B timer, pause, mute flag,
//! fixed real-time reads (R28). Tier 1. WP0 creates the file; WP6a owns it.
