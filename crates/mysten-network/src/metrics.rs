// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use tonic::codegen::http::header::HeaderName;
use tonic::codegen::http::{HeaderValue, Request, Response};
use tower_http::trace::{OnRequest, OnResponse};
use tracing::Span;

pub(crate) static GRPC_ENDPOINT_PATH_HEADER: HeaderName = HeaderName::from_static("grpc-path-req");

/// The trait to be implemented when want to be notified about
/// a new request and related metrics around it. When a request
/// is performed (up to the point that a response is created) the
/// on_response method is called with the corresponding metrics
/// details. The on_request method will be called when the request
/// is received, but not further processing has happened at this
/// point.
pub trait MetricsCallbackProvider: Send + Sync + Clone + 'static {
    /// Method will be called when a request has been received.
    /// `path`: the endpoint uri path
    fn on_request(&self, path: String);

    /// Method to be called from the server when a request is performed.
    /// `path`: the endpoint uri path
    /// `latency`: the time when the request was received and when the response was created
    /// `status`: the http status code of the response
    fn on_response(&self, path: String, latency: Duration, status: u16);
}

impl MetricsCallbackProvider for () {
    fn on_request(&self, _path: String) {}

    fn on_response(&self, _path: String, _latency: Duration, _status: u16) {}
}

#[derive(Clone)]
pub(crate) struct MetricsHandler<M: MetricsCallbackProvider> {
    pub(crate) metrics_provider: Option<M>,
}

impl<B, M: MetricsCallbackProvider> OnResponse<B> for MetricsHandler<M> {
    fn on_response(self, response: &Response<B>, latency: Duration, _span: &Span) {
        if let Some(provider) = &self.metrics_provider {
            let path: HeaderValue = response
                .headers()
                .get(&GRPC_ENDPOINT_PATH_HEADER)
                .unwrap()
                .clone();

            provider.on_response(
                path.to_str().unwrap().to_string(),
                latency,
                response.status().as_u16(),
            );
        }
    }
}

impl<B, M: MetricsCallbackProvider> OnRequest<B> for MetricsHandler<M> {
    fn on_request(&mut self, request: &Request<B>, _span: &Span) {
        if let Some(provider) = &self.metrics_provider {
            provider.on_request(request.uri().to_string());
        }
    }
}
