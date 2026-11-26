//! The client's log setup (docs/architecture/client/ui.md, Japanese text).
//!
//! `icu_segmenter` reports the missing Chinese/Japanese segmentation model by asking
//! `icu_provider` for it and logging the resulting `DataError`. With `icu_provider`'s `logging`
//! feature on (Cargo.toml) that report is a `log` record whose target is `icu_provider::error`,
//! which Bevy's `LogPlugin` forwards to tracing; the directive below drops it (#734,
//! [0003](../../docs/decisions/0003-ui-toolkit.md) decided to keep the dictionaries out).
//!
//! The directive is target-wide: it hides every ICU4X data error, not just this message. All
//! ICU4X data the client uses is baked in at compile time, so no data error can happen at run
//! time, and a message-scoped filter would need a custom `Layer` for one line that debug builds
//! only. Builds with `debug_assertions` off never made the report at all before the feature was on.

use bevy::log::LogPlugin;

/// `icu_provider`'s module for `DataError`'s log calls: every ICU4X data error has this target.
const ICU_ERROR_TARGET: &str = "icu_provider::error";

/// The client's `LogPlugin`: Bevy's defaults plus a directive that drops ICU4X data errors.
///
/// `RUST_LOG` still wins for whatever it names: Bevy appends the environment's directives after
/// these, so `RUST_LOG=icu_provider=warn` shows the notice again.
pub(crate) fn log_plugin() -> LogPlugin {
    let default = LogPlugin::default();
    LogPlugin {
        filter: format!("{},{ICU_ERROR_TARGET}=off", default.filter),
        ..default
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use bevy::log::{
        tracing,
        tracing_subscriber::{Layer, filter::EnvFilter, layer::Context, layer::SubscriberExt, registry},
    };

    use super::*;

    /// Counts the events that reach the sink; the filter in front of it decides which do.
    struct Count(Arc<AtomicUsize>);

    impl<S: tracing::Subscriber> Layer<S> for Count {
        fn on_event(&self, _event: &tracing::Event<'_>, _context: Context<'_, S>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// An ICU4X data error is dropped and an ordinary warning still reaches the sink.
    #[test]
    fn the_filter_drops_icu4x_data_errors_only() {
        let plugin = log_plugin();
        // `LogPlugin::build_filter_layer` parses `<level>,<filter>`; mirror it.
        let filter = EnvFilter::builder().parse_lossy(format!("{},{}", plugin.level, plugin.filter));
        let seen = Arc::new(AtomicUsize::new(0));
        let subscriber = registry().with(filter).with(Count(seen.clone()));

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "icu_provider::error", "ICU4X data error");
            tracing::warn!(target: "bevy_render", "kept");
        });

        assert_eq!(
            seen.load(Ordering::Relaxed),
            1,
            "only bevy_render's warning got through"
        );
    }
}
