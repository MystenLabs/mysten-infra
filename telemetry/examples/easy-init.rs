// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{debug, info, warn};

fn main() {
    let config = telemetry::TelemetryConfig {
        service_name: "my_app".into(),
        ..Default::default()
    };
    let guard = telemetry::init(config);

    info!(a = 1, "This will be INFO.");
    debug!(a = 2, "This will be DEBUG.");
    warn!(a = 3, "This will be WARNING.");
    panic!("This should cause error logs to be printed out!");
}
