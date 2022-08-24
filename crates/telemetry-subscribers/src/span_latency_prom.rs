// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This is a module that records Tokio-tracing [span](https://docs.rs/tracing/latest/tracing/span/index.html)
//! latencies into Prometheus histograms directly.
//! There is also the tracing-timing crate, from which this differs significantly:
//! - tracing-timing records latencies between events (logs).  We just want to record the latencies of spans.
//! - tracing-timing does not output to Prometheus, and extracting data from its histograms takes extra CPU
//! - tracing-timing records latencies using HDRHistogram, which is great, but uses extra memory when one
//!   is already using Prometheus
//! Thus this is a much smaller and more focused module.
//!

use chrono::offset::Utc;
use prometheus::{exponential_buckets, register_histogram_vec_with_registry, Registry};
use tracing::{span, Subscriber};

/// A tokio_tracing Layer that records span latencies into Prometheus histograms
pub struct PrometheusSpanLatencyLayer {
    span_latencies: prometheus::HistogramVec,
}

pub enum PrometheusSpanError {
    /// num_buckets must be positive >= 1
    ZeroOrNegativeNumBuckets,
    PromError(prometheus::Error),
}

impl From<prometheus::Error> for PrometheusSpanError {
    fn from(err: prometheus::Error) -> Self {
        Self::PromError(err)
    }
}

const TOP_LATENCY_IN_NS: f64 = 300.0 * 1.0e9;
const LOWEST_LATENCY_IN_NS: f64 = 500.0;

impl PrometheusSpanLatencyLayer {
    /// Create a new layer, injecting latencies into the given registry.
    /// The num_buckets controls how many buckets thus how much memory and time series one
    /// uses up in Prometheus (and in the application).  10 is probably a minimum.
    pub fn try_new(registry: &Registry, num_buckets: usize) -> Result<Self, PrometheusSpanError> {
        if num_buckets < 1 {
            return Err(PrometheusSpanError::ZeroOrNegativeNumBuckets);
        }

        // Histogram for span latencies must accommodate a wide range of possible latencies, so
        // don't use the default Prometheus buckets
        // The latencies are in nanoseconds
        let factor = (TOP_LATENCY_IN_NS / LOWEST_LATENCY_IN_NS).powf(1.0 / (num_buckets as f64));
        let buckets = exponential_buckets(LOWEST_LATENCY_IN_NS, factor, num_buckets)?;
        let span_latencies = register_histogram_vec_with_registry!(
            "tracing_span_latencies",
            "Latencies from tokio-tracing spans",
            &["span_name"],
            buckets,
            registry
        )?;
        Ok(Self { span_latencies })
    }
}

struct PromSpanTimestamp(i64);

impl<S> tracing_subscriber::Layer<S> for PrometheusSpanLatencyLayer
where
    S: Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    fn on_new_span(
        &self,
        _attrs: &span::Attributes,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<S>,
    ) {
        let span = ctx.span(id).unwrap();
        span.extensions_mut()
            .insert(PromSpanTimestamp(Utc::now().timestamp()));
    }

    fn on_close(&self, id: span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(&id).unwrap();
        let start_time = span
            .extensions()
            .get::<PromSpanTimestamp>()
            .expect("Could not find saved timestamp on span")
            .0;
        let elapsed_ns = Utc::now().timestamp() - start_time;
        self.span_latencies
            .with_label_values(&[span.name()])
            .observe(elapsed_ns as f64);
    }
}
