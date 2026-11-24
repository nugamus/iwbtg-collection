//! healthchecks.io pings (docs/server.md, Logs and monitoring; #155).
//!
//! `HEALTHCHECKS_URL` names the check's ping URL; [`ping_forever`] GETs it at startup and every
//! [`INTERVAL`]. Unset, the binary never spawns it. Every failure is a log line, never a return:
//! healthchecks.io alerts when the pings stop, which is exactly the outage the server cannot
//! report about itself.

use std::time::Duration;

/// How often healthchecks.io expects a ping (docs/server.md, Logs and monitoring).
pub const INTERVAL: Duration = Duration::from_secs(5 * 60);

/// A stalled connect must not hold a blocking worker indefinitely.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Pings `url` at startup, then every `interval`, forever. Spawn it as a task; it never returns.
///
/// The first tick fires immediately, so a restart is visible to healthchecks.io at once. A missed
/// tick is not replayed as a burst: one ping per period is what the check wants.
pub async fn ping_forever(url: String, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        match ping_once(&url).await {
            Ok(status) if (200..300).contains(&status) => {
                tracing::debug!(status, "healthchecks ping");
            }
            Ok(status) => tracing::warn!(status, "healthchecks ping returned an error status"),
            Err(err) => tracing::warn!(err = %err, "healthchecks ping failed"),
        }
    }
}

/// One GET, reported as the HTTP status.
///
/// `ureq` is blocking, so it runs on a blocking worker: a slow DNS lookup or connect never stalls
/// the async runtime. An unreachable host, a timeout and a malformed URL are all `Err`, which the
/// caller logs; none of them panics.
pub async fn ping_once(url: &str) -> Result<u16, String> {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || {
        // The OS trust store, not a bundled CA list: the deployment image needs
        // `ca-certificates` (a dependency of that is libssl3 on Debian).
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .tls_config(tls)
            .build()
            .new_agent();
        agent
            .get(url.as_str())
            .call()
            .map(|response| response.status().as_u16())
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("ping task failed: {err}"))?
}
