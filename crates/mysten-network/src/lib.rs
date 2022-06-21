// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

pub mod client;
pub mod codec;
pub mod config;
mod metrics;
mod multiaddr;
pub mod server;

/// The trait to be implemented when want to be notified about
/// a new request and related metrics around it. When a request
/// is performed (up to the point that a response is created) the
/// on_request method is called with the corresponding metrics
/// details.
pub trait MetricsCallbackProvider: Send + Sync + Clone + 'static {
    /// Method to be called from the server when a request is performed.
    /// `path`: the endpoint uri path
    /// `start_time`: the time when the request was received
    /// `status`: the http status code of the response
    fn on_request(&self, path: String, start_time: Instant, status: u16);
}

impl MetricsCallbackProvider for () {
    fn on_request(&self, _path: String, _start_time: Instant, _status: u16) {}
}
