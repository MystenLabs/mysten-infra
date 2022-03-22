//! This is a library for common Tokio Tracing subscribers, such as for Jaeger.
//!
//! The subscribers are configured using TelemetryConfig passed into the `init()` method.
//!
//! Getting started is easy:
//! ```
//! let config = telemetry::TelemetryConfig {
//!   service_name: "my_app".into(),
//!   ..Default::default()
//! };
//! telemetry::init(config);
//! ```

use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use tracing::info;
use tracing::subscriber::set_global_default;
use tracing_appender::non_blocking::NonBlocking;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

/// Configuration for different logging/tracing options
/// ===
/// - json_log_output: Output JSON logs to stdout only.  No other options will work.
/// - log_file: If defined, write output to a file starting with this name, ex app.log
/// - service_name:
#[derive(Default, Clone, Debug)]
pub struct TelemetryConfig {
    pub enable_tracing: bool,
    /// The name of the service for Jaeger and Bunyan
    pub service_name: String,
    pub tokio_console: bool,
    /// Output JSON logs.  Tracing and Tokio Console are not available if this is enabled.
    pub json_log_output: bool,
    /// If defined, write output to a file starting with this name, ex app.log
    pub log_file: Option<String>,
}

fn get_output(config: &TelemetryConfig) -> NonBlocking {
    if let Some(logfile_prefix) = &config.log_file {
        let file_appender = tracing_appender::rolling::daily("", logfile_prefix);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        non_blocking
    } else {
        let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
        non_blocking
    }
}

// NOTE: this function is copied from tracing's panic_hook example
fn set_panic_hook() {
    // Set a panic hook that records the panic as a `tracing` event at the
    // `ERROR` verbosity level.
    //
    // If we are currently in a span when the panic occurred, the logged event
    // will include the current span, allowing the context in which the panic
    // occurred to be recorded.
    std::panic::set_hook(Box::new(|panic| {
        // If the panic has a source location, record it as structured fields.
        if let Some(location) = panic.location() {
            // On nightly Rust, where the `PanicInfo` type also exposes a
            // `message()` method returning just the message, we could record
            // just the message instead of the entire `fmt::Display`
            // implementation, avoiding the duplicated location
            tracing::error!(
                message = %panic,
                panic.file = location.file(),
                panic.line = location.line(),
                panic.column = location.column(),
            );
        } else {
            tracing::error!(message = %panic);
        }
    }));
}

/// Initialize telemetry subscribers based on TelemetryConfig
pub fn init(config: TelemetryConfig) {
    // TODO: reorganize different telemetry options so they can use the same registry
    // Code to add logging/tracing config from environment, including RUST_LOG
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let nb_output = get_output(&config);

    if config.json_log_output {
        // See https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/#5-7-tracing-bunyan-formatter
        // Also Bunyan layer addes JSON logging for tracing spans with duration information
        let formatting_layer = BunyanFormattingLayer::new(config.service_name, nb_output);
        // The `with` method is provided by `SubscriberExt`, an extension
        // trait for `Subscriber` exposed by `tracing_subscriber`
        let subscriber = Registry::default()
            .with(env_filter)
            .with(JsonStorageLayer)
            .with(formatting_layer);
        // `set_global_default` can be used by applications to specify
        // what subscriber should be used to process spans.
        set_global_default(subscriber).expect("Failed to set subscriber");

        info!("Enabling JSON and span logging");
    } else if config.tokio_console {
        console_subscriber::init();
    } else {
        // Output to file or to stdout with ANSI colors
        let fmt_layer = fmt::layer()
            .with_ansi(config.log_file.is_none())
            .with_writer(nb_output);

        // Standard env filter (RUST_LOG) with standard formatter
        let subscriber = Registry::default().with(env_filter).with(fmt_layer);

        if config.enable_tracing {
            // Install a tracer to send traces to Jaeger.  Batching for better performance.
            let tracer = opentelemetry_jaeger::new_pipeline()
                .with_service_name(&config.service_name)
                .with_max_packet_size(9216) // Default max UDP packet size on OSX
                .with_auto_split_batch(true) // Auto split batches so they fit under packet size
                .install_batch(opentelemetry::runtime::Tokio)
                .expect("Could not create async Tracer");

            // Create a tracing subscriber with the configured tracer
            let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

            // Enable Trace Contexts for tying spans together
            global::set_text_map_propagator(TraceContextPropagator::new());

            set_global_default(subscriber.with(telemetry)).expect("Failed to set subscriber");
            info!("Jaeger tracing initialized");
        } else {
            set_global_default(subscriber).expect("Failed to set subscriber");
        }
    }

    set_panic_hook();
}

mod tests {
    use super::*;
    use tracing::{debug, info, warn};

    #[test]
    #[should_panic]
    fn test_telemetry_init() {
        let config = TelemetryConfig {
            service_name: "my_app".into(),
            ..Default::default()
        };
        init(config);

        info!(a = 1, "This will be INFO.");
        debug!(a = 2, "This will be DEBUG.");
        warn!(a = 3, "This will be WARNING.");
        panic!("This should cause error logs to be printed out!");
    }
}
