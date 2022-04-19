// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ConnectionOrigin, Result};
use quinn::{ConnectionError, RecvStream, SendStream};
use std::{fmt, net::SocketAddr, time::Duration};
use tracing::trace;

#[derive(Clone)]
pub struct Connection {
    inner: quinn::Connection,
    peer_identity: ed25519_dalek::PublicKey,
    origin: ConnectionOrigin,
}

impl Connection {
    pub fn new(inner: quinn::Connection, origin: ConnectionOrigin) -> Result<Self> {
        let peer_identity = Self::try_peer_identity(&inner)?;
        Ok(Self {
            inner,
            peer_identity,
            origin,
        })
    }

    /// Try to query Cryptographic identity of the peer
    fn try_peer_identity(connection: &quinn::Connection) -> Result<ed25519_dalek::PublicKey> {
        use x509_parser::{certificate::X509Certificate, traits::FromDer};

        // Query the certificate chain provided by a [TLS
        // Connection](https://docs.rs/rustls/0.20.4/rustls/enum.Connection.html#method.peer_certificates).
        // The first cert in the chain is gaurenteed to be the peer
        let peer_cert = &connection
            .peer_identity()
            .unwrap()
            .downcast::<Vec<rustls::Certificate>>()
            .unwrap()[0];

        let cert = X509Certificate::from_der(peer_cert.0.as_ref())
            .map_err(|_| rustls::Error::InvalidCertificateEncoding)?;
        let spki = cert.1.public_key();
        let key = ed25519_dalek::PublicKey::from_bytes(spki.subject_public_key.data).unwrap();
        Ok(key)
    }

    /// Cryptographic identity of the peer
    pub fn peer_identity(&self) -> ed25519_dalek::PublicKey {
        self.peer_identity
    }

    /// Origin of the Connection
    pub fn origin(&self) -> ConnectionOrigin {
        self.origin
    }

    /// A stable identifier for this connection
    ///
    /// Peer addresses and connection IDs can change, but this value will remain
    /// fixed for the lifetime of the connection.
    pub fn stable_id(&self) -> usize {
        self.inner.stable_id()
    }

    /// Current best estimate of this connection's latency (round-trip-time)
    pub fn rtt(&self) -> Duration {
        self.inner.rtt()
    }

    /// The peer's UDP address
    ///
    /// If `ServerConfig::migration` is `true`, clients may change addresses at will, e.g. when
    /// switching to a cellular internet connection.
    pub fn remote_address(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    /// Open a unidirection stream to the peer.
    ///
    /// Messages sent over the stream will arrive at the peer in the order they were sent.
    pub async fn open_uni(&self) -> Result<SendStream, ConnectionError> {
        self.inner.open_uni().await
    }

    /// Open a bidirectional stream to the peer.
    ///
    /// Bidirectional streams allow messages to be sent in both directions. This can be useful to
    /// automatically correlate response messages, for example.
    ///
    /// Messages sent over the stream will arrive at the peer in the order they were sent.
    pub async fn open_bi(&self) -> Result<(SendStream, RecvStream), ConnectionError> {
        self.inner.open_bi().await
    }

    /// Close the connection immediately.
    ///
    ///
    /// This is not a graceful close - pending operations will fail immediately and data on
    /// unfinished streams is not guaranteed to be delivered.
    pub fn close(&self) {
        trace!("Closing Connection");
        self.inner.close(0_u32.into(), b"connection closed")
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("origin", &self.origin())
            .field("id", &self.stable_id())
            .field("remote_address", &self.remote_address())
            .field("peer_identity", &self.peer_identity())
            .finish_non_exhaustive()
    }
}
