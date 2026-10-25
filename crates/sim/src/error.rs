//! Why the sim halted (docs/plans/sim-core.md 2.2, 10.2, 10.3).
//!
//! The sim never panics on any input. A script error, a non-finite value, an overflow or an
//! unsupported case sets `SimStatus::Halted(SimError)` and freezes the world where it stopped.

use serde::{Deserialize, Serialize};

use crate::num::NumError;

/// A reason the sim stopped.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, thiserror::Error)]
pub enum SimError {
    /// A setter got NaN or an infinity (N15). `what` names the variable (`hspeed`, `alarm[0]`).
    #[error("non-finite value for {what}")]
    NonFinite {
        /// The variable or value that was not finite.
        what: String,
    },
    /// A checked counter or integer conversion overflowed (`tick`, `next_id`, deaths, grid
    /// cells). `what` names it.
    #[error("overflow in {what}")]
    Overflow {
        /// The counter or conversion.
        what: String,
    },
    /// A case this build does not implement, or that the spec leaves UNKNOWN and the plan
    /// refuses to guess (other mask angles, `DestroySelf`, an unknown engine_rev).
    #[error("unsupported: {what}")]
    Unsupported {
        /// What was asked for, with its register id or plan row where there is one.
        what: String,
    },
    /// A script stopped with an error (WP9), or GML-level misuse such as reading an unset
    /// variable in a game without "treat uninitialized as 0".
    #[error("script error: {message}")]
    Script {
        /// The script host's message.
        message: String,
    },
}

impl SimError {
    /// `NonFinite` for `what`.
    pub fn non_finite(what: impl Into<String>) -> SimError {
        SimError::NonFinite { what: what.into() }
    }

    /// `Overflow` for `what`.
    pub fn overflow(what: impl Into<String>) -> SimError {
        SimError::Overflow { what: what.into() }
    }

    /// `Unsupported` for `what`.
    pub fn unsupported(what: impl Into<String>) -> SimError {
        SimError::Unsupported { what: what.into() }
    }

    /// A `num` error in a GML operation, as a script-level error (GM8 stops the game on these).
    pub fn from_num(op: &str, e: NumError) -> SimError {
        SimError::Script {
            message: format!("{op}: {e}"),
        }
    }
}
