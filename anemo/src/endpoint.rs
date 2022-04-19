// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{config::EndpointConfig, Connection, ConnectionOrigin, Result};
use futures::{FutureExt, StreamExt};
use quinn::{IncomingBiStreams, IncomingUniStreams};
use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tracing::trace;

#[derive(Debug)]
pub struct Endpoint {
    inner: quinn::Endpoint,
    local_addr: SocketAddr,
    config: EndpointConfig,
}

impl Endpoint {
    pub fn new<A: std::net::ToSocketAddrs>(
        config: EndpointConfig,
        addr: A,
    ) -> Result<(Self, Incoming)> {
        let socket = std::net::UdpSocket::bind(addr)?;
        let local_addr = socket.local_addr()?;
        let server_config = config.server_config().clone();
        let client_config = config.client_config().clone();
        let (mut endpoint, incoming) =
            quinn::Endpoint::new(Default::default(), Some(server_config), socket)?;
        endpoint.set_default_client_config(client_config);

        let endpoint = Self {
            inner: endpoint,
            local_addr,
            config,
        };
        let incoming = Incoming::new(incoming);
        Ok((endpoint, incoming))
    }

    pub fn connect<A: std::net::ToSocketAddrs>(&self, addr: A) -> Result<Connecting> {
        let addr = addr.to_socket_addrs()?.next().unwrap();

        self.inner
            .connect(addr, self.config.server_name())
            .map_err(Into::into)
            .map(Connecting::new_outbound)
    }

    /// Returns the socket address that this Endpoint is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn config(&self) -> &EndpointConfig {
        &self.config
    }

    /// Close all of this endpoint's connections immediately and cease accepting new connections.
    pub fn close(&self) {
        trace!("Closing endpoint");
        // let _ = self.termination_tx.send(());
        self.inner.close(0_u32.into(), b"endpoint closed")
    }
}

/// Stream of incoming connections.
#[derive(Debug)]
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Incoming(quinn::Incoming);

impl Incoming {
    pub(crate) fn new(inner: quinn::Incoming) -> Self {
        Self(inner)
    }
}

impl futures::stream::Stream for Incoming {
    type Item = Connecting;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0
            .poll_next_unpin(cx)
            .map(|maybe_next| maybe_next.map(Connecting::new_inbound))
    }
}

#[derive(Debug)]
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Connecting {
    inner: quinn::Connecting,
    origin: ConnectionOrigin,
}

impl Connecting {
    pub(crate) fn new(inner: quinn::Connecting, origin: ConnectionOrigin) -> Self {
        Self { inner, origin }
    }

    pub(crate) fn new_inbound(inner: quinn::Connecting) -> Self {
        Self::new(inner, ConnectionOrigin::Inbound)
    }

    pub(crate) fn new_outbound(inner: quinn::Connecting) -> Self {
        Self::new(inner, ConnectionOrigin::Outbound)
    }
}

impl Future for Connecting {
    type Output = Result<(Connection, IncomingUniStreams, IncomingBiStreams)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner.poll_unpin(cx).map(|result| {
            result.map_err(Into::into).and_then(
                |quinn::NewConnection {
                     connection,
                     uni_streams,
                     bi_streams,
                     ..
                 }| {
                    Ok((
                        Connection::new(connection, self.origin)?,
                        uni_streams,
                        bi_streams,
                    ))
                },
            )
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{future::join, io::AsyncReadExt, stream::StreamExt};
    use std::time::Duration;

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn basic_endpoint() -> Result<()> {
        let msg = b"hello";
        let config_1 = EndpointConfig::random("test");
        let (endpoint_1, _incoming_1) = Endpoint::new(config_1, "localhost:0")?;
        let pubkey_1 = endpoint_1.config.keypair().public;

        println!("1: {}", endpoint_1.local_addr());

        let config_2 = EndpointConfig::random("test");
        let (endpoint_2, mut incoming_2) = Endpoint::new(config_2, "localhost:0")?;
        let pubkey_2 = endpoint_2.config.keypair().public;
        let addr_2 = endpoint_2.local_addr();
        println!("2: {}", endpoint_2.local_addr());

        let peer_1 = async move {
            let (connection, _, _) = endpoint_1.connect(addr_2).unwrap().await.unwrap();
            assert_eq!(connection.peer_identity(), pubkey_2);
            {
                let mut send_stream = connection.open_uni().await.unwrap();
                send_stream.write_all(msg).await.unwrap();
                send_stream.finish().await.unwrap();
            }
            endpoint_1.close();
            endpoint_1.inner.wait_idle().await;
            // Result::<()>::Ok(())
        };

        let peer_2 = async move {
            let (connection, mut uni_streams, _bi_streams) =
                incoming_2.next().await.unwrap().await.unwrap();
            assert_eq!(connection.peer_identity(), pubkey_1);

            let mut recv = uni_streams.next().await.unwrap().unwrap();
            let mut buf = Vec::new();
            AsyncReadExt::read_to_end(&mut recv, &mut buf)
                .await
                .unwrap();
            println!("from remote: {}", buf.escape_ascii());
            assert_eq!(buf, msg);
            endpoint_2.close();
            endpoint_2.inner.wait_idle().await;
            // Result::<()>::Ok(())
        };

        timeout(join(peer_1, peer_2)).await?;
        Ok(())
    }

    async fn timeout<F: std::future::Future>(
        f: F,
    ) -> Result<F::Output, tokio::time::error::Elapsed> {
        tokio::time::timeout(Duration::from_millis(500), f).await
    }
}
