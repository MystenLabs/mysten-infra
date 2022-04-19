// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(Copy, Clone, Debug, Eq)]
pub struct PeerId(pub ed25519_dalek::PublicKey);

impl std::hash::Hash for PeerId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl std::cmp::PartialEq for PeerId {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes() == other.0.as_bytes()
    }
}

impl std::cmp::PartialOrd for PeerId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_bytes().partial_cmp(other.0.as_bytes())
    }
}

impl std::cmp::Ord for PeerId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

/// Origin of how a Connection was established.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum ConnectionOrigin {
    /// `Inbound` indicates that we are the listener for this connection.
    Inbound,
    /// `Outbound` indicates that we are the dialer for this connection.
    Outbound,
}

impl ConnectionOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionOrigin::Inbound => "inbound",
            ConnectionOrigin::Outbound => "outbound",
        }
    }
}

impl std::fmt::Debug for ConnectionOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConnectionOrigin::")?;
        let direction = match self {
            ConnectionOrigin::Inbound => "Inbound",
            ConnectionOrigin::Outbound => "Outbound",
        };
        f.write_str(direction)
    }
}

impl std::fmt::Display for ConnectionOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod test {
    use super::ConnectionOrigin;

    #[test]
    fn connection_origin_debug() {
        let inbound = format!("{:?}", ConnectionOrigin::Inbound);
        let outbound = format!("{:?}", ConnectionOrigin::Outbound);

        assert_eq!(inbound, "ConnectionOrigin::Inbound");
        assert_eq!(outbound, "ConnectionOrigin::Outbound");
    }

    #[test]
    fn connection_origin_display() {
        let inbound = ConnectionOrigin::Inbound.to_string();
        let outbound = ConnectionOrigin::Outbound.to_string();

        assert_eq!(inbound, "inbound");
        assert_eq!(outbound, "outbound");
    }
}
