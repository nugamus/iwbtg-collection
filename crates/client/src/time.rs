//! The clock the client reads between ticks. `std::time::Instant::now` and `SystemTime::now`
//! panic on wasm32-unknown-unknown, which has no OS clock; the browser build reads
//! `performance.now` and `Date.now` instead (docs/architecture/client/web.md, #29). Natively this
//! is just `std::time`.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};
