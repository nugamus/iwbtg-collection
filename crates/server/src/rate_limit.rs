//! Per-client request limits (docs/server.md, Security rules, rule 1; #161).
//!
//! The client key is `CF-Connecting-IP`, the header Cloudflare sets on every proxied request.
//! `X-Forwarded-For` is never read: a client can forge it, and behind the tunnel it carries no
//! information we trust. If the header is absent (local development, tests) the peer address from
//! [`ConnectInfo`] is used, and with neither the key is `local`. Production publishes no port, so
//! only cloudflared can reach the server; the fallback never sees real traffic.
//!
//! This caps *requests per time window*. It is deliberately separate from the Argon2 permit
//! semaphore (#1505), which caps the memory in flight; neither substitutes for the other.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

/// Stop growing the map once it holds this many windows; expired ones are dropped first.
const MAX_BUCKETS: usize = 10_000;

/// The client address as the limiter sees it.
pub struct ClientIp(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        const CF_CONNECTING_IP: &str = "cf-connecting-ip";
        let forwarded = parts
            .headers
            .get(CF_CONNECTING_IP)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|connect_info| connect_info.0.ip().to_string());
        Ok(Self(forwarded.or(peer).unwrap_or_else(|| "local".to_owned())))
    }
}

/// One fixed window for one key.
struct Window {
    started: Instant,
    count: u32,
    span: Duration,
}

/// In-memory fixed-window counters. One per server process; a restart starts the windows over,
/// which is acceptable for limits measured in minutes and hours.
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Window>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one request against `key`.
    ///
    /// `Ok(())` while the request is inside the limit, otherwise the number of seconds until the
    /// window resets (at least one) for `Retry-After`.
    pub fn check(&self, key: &str, limit: u32, span: Duration) -> Result<(), u64> {
        let now = Instant::now();
        // A poisoned lock must not take the endpoint down; the counters are still usable.
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if buckets.len() >= MAX_BUCKETS {
            buckets.retain(|_, window| now.duration_since(window.started) < window.span);
        }

        let window = buckets.entry(key.to_owned()).or_insert(Window {
            started: now,
            count: 0,
            span,
        });
        if now.duration_since(window.started) >= window.span {
            window.started = now;
            window.count = 0;
            window.span = span;
        }
        window.count += 1;

        if window.count > limit {
            let elapsed = now.duration_since(window.started);
            Err(window.span.saturating_sub(elapsed).as_secs().max(1))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RateLimiter;

    #[test]
    fn a_window_allows_the_limit_and_then_answers_with_a_retry() {
        let limiter = RateLimiter::new();
        let span = Duration::from_secs(60);

        for _ in 0..3 {
            assert!(limiter.check("ip", 3, span).is_ok());
        }
        let retry = limiter
            .check("ip", 3, span)
            .expect_err("the fourth request must be limited");
        assert!((1..=60).contains(&retry), "implausible Retry-After: {retry}");
    }

    #[test]
    fn keys_do_not_share_a_window() {
        let limiter = RateLimiter::new();
        let span = Duration::from_secs(60);
        for _ in 0..2 {
            assert!(limiter.check("one", 2, span).is_ok());
        }
        assert!(limiter.check("one", 2, span).is_err());
        assert!(
            limiter.check("two", 2, span).is_ok(),
            "another key inherited the exhausted window"
        );
    }
}
