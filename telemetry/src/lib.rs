//! This is a library for common Tokio Tracing subscribers, such as for Jaeger.
//!
//! The subscribers are configured using TelemetryConfig passed into the `init()` method.

use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use tracing::info;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

/// Configuration for different logging/tracing options
pub struct TelemetryConfig {
    enable_tracing: bool,
    service_name: String,   /// The name of the service for Jaeger and Bunyan
    tokio_console: bool,
    json_log_output: bool,
}

/// Initialize telemetry subscribers based on TelemetryConfig
pub fn init(config: TelemetryConfig) {
    // TODO: reorganize different telemetry options so they can use the same registry
    // Code to add logging/tracing config from environment, including RUST_LOG
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // See [[dev-docs/observability.md]] for more information on span logging.
    if config.json_log_output {
        // See https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/#5-7-tracing-bunyan-formatter
        // Also Bunyan layer addes JSON logging for tracing spans with duration information
        let formatting_layer = BunyanFormattingLayer::new(
            config.service_name,
            // Output the formatted spans to stdout.
            std::io::stdout,
        );
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
        // Standard env filter (RUST_LOG) with standard formatter
        let subscriber = Registry::default()
            .with(env_filter)
            .with(fmt::layer().with_ansi(true).with_writer(std::io::stdout));

        // We assume you would not enable both SUI_JSON_SPAN_LOGS and open telemetry at same time, but who knows?
        if config.enable_tracing {
            // Install a tracer to send traces to Jaeger.  Batching for better performance.
            let tracer = opentelemetry_jaeger::new_pipeline()
                .with_service_name(config.service_name)
                .with_max_packet_size(9216) // Default max UDP packet size on OSX
                .with_auto_split_batch(true) // Auto split batches so they fit under packet size
                .install_batch(opentelemetry::runtime::Tokio)
                .expect("Could not create async Tracer");

            // Create a tracing subscriber with the configured tracer
            let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

            // Enable Trace Contexts for tying spans together
            global::set_text_map_propagator(TraceContextPropagator::new());

            set_global_default(subscriber.with(telemetry)).expect("Failed to set subscriber");
        } else {
            set_global_default(subscriber).expect("Failed to set subscriber");
        }
    }
}