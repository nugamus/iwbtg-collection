//! Optional startup phase timing for the browser build (#1489). The page enables it with
//! `?measure`; with it off every call is one relaxed atomic load and no clock read, so the normal
//! page pays nothing. `crates/client/web/measure_startup.py` drives the page headlessly and reads
//! the lines this logs. The native client never enables it (the native measurement is
//! `shell::tests::startup_phases`).
//!
//! Each phase logs `startup phase <name>: <ms> ms`; [`total`] logs the time since [`enable`], and
//! the sprite queue logs its own CPU and wall time when it drains (`view::upload_textures`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::time::{Duration, Instant};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// When the page started measuring (before its fetch), for the final total.
static START: Mutex<Option<Instant>> = Mutex::new(None);

/// Starts measuring and records the page's start time: `true` the first time, `false` if it was
/// already on. Only the browser entry asks to measure, so natively this is unused.
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), allow(dead_code))]
pub fn enable() -> bool {
    if ENABLED.swap(true, Ordering::Relaxed) {
        return false;
    }
    *START.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
    true
}

/// Whether the page asked to measure.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A clock reading to pass to [`mark`], or `None` when measuring is off (then no clock is read).
pub fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

/// Logs how long `since` took under `name`; a no-op when `since` is `None`.
pub fn mark(name: &str, since: Option<Instant>) {
    if let Some(since) = since {
        log(&format!(
            "startup phase {name}: {:.3} ms",
            millis(since.elapsed())
        ));
    }
}

/// Logs the time from [`enable`] to now under `name`.
pub fn total(name: &str) {
    let since = *START.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(since) = since {
        log(&format!(
            "startup total {name}: {:.3} ms",
            millis(since.elapsed())
        ));
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The browser console (wasm) or stderr (native; the harness only runs in a browser).
pub fn log(message: &str) {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    web_sys::console::log_1(&web_sys::wasm_bindgen::JsValue::from_str(message));
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    eprintln!("{message}");
}
