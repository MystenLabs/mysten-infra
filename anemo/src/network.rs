// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Connection, ConnectionOrigin, Endpoint, Incoming, PeerId, Request, Response, Result};
use bytes::Bytes;
use futures::{
    stream::{Fuse, FuturesUnordered},
    SinkExt, StreamExt,
};
use parking_lot::{Mutex, RwLock};
use quinn::{IncomingBiStreams, IncomingUniStreams, RecvStream, SendStream};
use std::{
    collections::{hash_map::Entry, HashMap},
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tower::{util::BoxCloneService, ServiceExt};
use tracing::{error, info, trace};

#[derive(Clone)]
pub struct Network(Arc<NetworkInner>);

impl Network {
    /// Start a network and return a handle to it
    ///
    /// Requires that this is called from within the context of a tokio runtime
    pub fn start(endpoint: Endpoint, incoming: Incoming) -> Self {
        let network = Self(Arc::new(NetworkInner {
            endpoint,
            connections: Default::default(),
            on_uni: Mutex::new(noop_service()),
            on_bi: Mutex::new(echo_service()),
        }));

        info!("Starting network");

        let inbound_connection_handler = InboundConnectionHandler::new(network.clone(), incoming);

        tokio::spawn(inbound_connection_handler.start());

        network
    }

    pub fn peers(&self) -> Vec<PeerId> {
        self.0.peers()
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<PeerId> {
        self.0.connect(addr).await
    }

    pub fn disconnect(&self, peer: PeerId) -> Result<()> {
        self.0.disconnect(peer)
    }

    pub async fn rpc(&self, peer: PeerId, request: Bytes) -> Result<Bytes> {
        self.0.rpc(peer, request).await
    }

    pub async fn send_message(&self, peer: PeerId, request: Bytes) -> Result<()> {
        self.0.send_message(peer, request).await
    }

    /// Returns the socket address that this Network is listening on
    pub fn local_addr(&self) -> SocketAddr {
        self.0.local_addr()
    }

    pub fn peer_id(&self) -> PeerId {
        self.0.peer_id()
    }
}

struct NetworkInner {
    endpoint: Endpoint,
    connections: RwLock<HashMap<PeerId, Connection>>,
    //TODO these shouldn't need to be wrapped in mutexes
    on_uni: Mutex<BoxCloneService<Request, (), Infallible>>,
    on_bi: Mutex<BoxCloneService<Request, Response, Infallible>>,
}

impl NetworkInner {
    fn peers(&self) -> Vec<PeerId> {
        self.connections.read().keys().copied().collect()
    }

    /// Returns the socket address that this Network is listening on
    fn local_addr(&self) -> SocketAddr {
        self.endpoint.local_addr()
    }

    fn peer_id(&self) -> PeerId {
        PeerId(self.endpoint.config().keypair().public)
    }

    async fn connect(&self, addr: SocketAddr) -> Result<PeerId> {
        let (connection, incoming_uni, incoming_bi) = self.endpoint.connect(addr)?.await?;
        let peer_id = PeerId(connection.peer_identity());
        self.add_peer(connection, incoming_uni, incoming_bi);
        Ok(peer_id)
    }

    fn disconnect(&self, peer: PeerId) -> Result<()> {
        if let Some(connection) = self.connections.write().remove(&peer) {
            connection.close();
        }
        Ok(())
    }

    async fn rpc(&self, peer: PeerId, request: Bytes) -> Result<Bytes> {
        let connection = self.connections.read().get(&peer).unwrap().clone();
        let (send_stream, recv_stream) = connection.open_bi().await?;
        let mut send_stream = FramedWrite::new(send_stream, network_message_frame_codec());
        let mut recv_stream = FramedRead::new(recv_stream, network_message_frame_codec());
        send_stream.send(request).await?;
        send_stream.get_mut().finish().await.unwrap();
        let response = recv_stream.next().await.unwrap()?;
        Ok(response.into())
    }

    async fn send_message(&self, peer: PeerId, message: Bytes) -> Result<()> {
        let connection = self.connections.read().get(&peer).unwrap().clone();
        let send_stream = connection.open_uni().await?;
        let mut send_stream = FramedWrite::new(send_stream, network_message_frame_codec());
        send_stream.send(message).await?;
        send_stream.get_mut().finish().await.unwrap();
        Ok(())
    }

    fn add_peer(
        &self,
        connection: Connection,
        incoming_uni: IncomingUniStreams,
        incoming_bi: IncomingBiStreams,
    ) {
        // TODO drop Connection if you've somehow connected out ourself

        let peer_id = PeerId(connection.peer_identity());
        match self.connections.write().entry(peer_id) {
            Entry::Occupied(mut entry) => {
                if Self::simultaneous_dial_tie_breaking(
                    self.peer_id(),
                    peer_id,
                    entry.get().origin(),
                    connection.origin(),
                ) {
                    info!("closing old connection with {peer_id:?} to mitigate simultaneous dial");
                    let old_connection = entry.insert(connection);
                    old_connection.close();
                } else {
                    info!("closing new connection with {peer_id:?} to mitigate simultaneous dial");
                    connection.close();
                    // Early return to avoid standing up Incoming Request handlers
                    return;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(connection);
            }
        }
        let request_handler = InboundRequestHandler::new(
            self.on_bi.lock().clone(),
            self.on_uni.lock().clone(),
            incoming_uni,
            incoming_bi,
        );

        tokio::spawn(request_handler.start());
    }

    /// In the event two peers simultaneously dial each other we need to be able to do
    /// tie-breaking to determine which connection to keep and which to drop in a deterministic
    /// way. One simple way is to compare our local PeerId with that of the remote's PeerId and
    /// keep the connection where the peer with the greater PeerId is the dialer.
    ///
    /// Returns `true` if the existing connection should be dropped and `false` if the new
    /// connection should be dropped.
    fn simultaneous_dial_tie_breaking(
        own_peer_id: PeerId,
        remote_peer_id: PeerId,
        existing_origin: ConnectionOrigin,
        new_origin: ConnectionOrigin,
    ) -> bool {
        match (existing_origin, new_origin) {
            // If the remote dials while an existing connection is open, the older connection is
            // dropped.
            (ConnectionOrigin::Inbound, ConnectionOrigin::Inbound) => true,
            // We should never dial the same peer twice, but if we do drop the old connection
            (ConnectionOrigin::Outbound, ConnectionOrigin::Outbound) => true,
            (ConnectionOrigin::Inbound, ConnectionOrigin::Outbound) => remote_peer_id < own_peer_id,
            (ConnectionOrigin::Outbound, ConnectionOrigin::Inbound) => own_peer_id < remote_peer_id,
        }
    }
}

impl Drop for NetworkInner {
    fn drop(&mut self) {
        self.endpoint.close()
    }
}

struct InboundConnectionHandler {
    // TODO we probably don't want this to be Network but some other internal type that doesn't keep
    // the network alive after the application layer drops the handle
    network: Network,
    incoming: Fuse<Incoming>,
}

impl InboundConnectionHandler {
    fn new(network: Network, incoming: Incoming) -> Self {
        Self {
            network,
            incoming: incoming.fuse(),
        }
    }

    async fn start(mut self) {
        info!("InboundConnectionHandler started");

        let mut pending_connections = FuturesUnordered::new();

        loop {
            futures::select! {
                connecting = self.incoming.select_next_some() => {
                    info!("recieved new incoming connection");
                    pending_connections.push(connecting);
                },
                maybe_connection = pending_connections.select_next_some() => {
                    match maybe_connection {
                        Ok((connection, incoming_uni, incoming_bi)) => {
                            info!("new connection complete");
                            self.network.0.add_peer(connection, incoming_uni, incoming_bi);
                        }
                        Err(e) => {
                            error!("inbound connection failed: {e}");
                        }
                    }
                },
                complete => break,
            }
        }

        info!("InboundConnectionHandler ended");
    }
}

struct InboundRequestHandler {
    incoming_uni: Fuse<IncomingUniStreams>,
    incoming_bi: Fuse<IncomingBiStreams>,
    bi_service: BoxCloneService<Request, Response, Infallible>,
    uni_service: BoxCloneService<Request, (), Infallible>,
}

impl InboundRequestHandler {
    fn new(
        service: BoxCloneService<Request, Response, Infallible>,
        uni_service: BoxCloneService<Request, (), Infallible>,
        incoming_uni: IncomingUniStreams,
        incoming_bi: IncomingBiStreams,
    ) -> Self {
        Self {
            incoming_uni: incoming_uni.fuse(),
            incoming_bi: incoming_bi.fuse(),
            bi_service: service,
            uni_service,
        }
    }

    async fn start(mut self) {
        info!("InboundRequestHandler started");

        loop {
            futures::select! {
                uni = self.incoming_uni.select_next_some() => {
                    if let Ok(recv_stream) = uni {
                        info!("incoming uni stream! {}", recv_stream.id());
                        let request_handler =
                        UniStreamRequestHandler::new(self.uni_service.clone(), recv_stream);
                        tokio::spawn(request_handler.handle());
                    }
                },
                bi = self.incoming_bi.select_next_some() => {
                    if let Ok((bi_tx, bi_rx)) = bi {
                        info!("incoming bi stream! {}", bi_tx.id());
                        // Maybe handle via FuturesUnordered
                        let request_handler =
                        BiStreamRequestHandler::new(self.bi_service.clone(), bi_tx, bi_rx);
                        tokio::spawn(request_handler.handle());
                    }
                },
                complete => break,
            }
        }

        info!("InboundRequestHandler ended");
    }
}

/// Returns a fully configured length-delimited codec for writing/reading
/// serialized frames to/from a socket.
pub(crate) fn network_message_frame_codec() -> LengthDelimitedCodec {
    const MAX_FRAME_SIZE: usize = 1 << 23; // 8 MiB

    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_SIZE)
        .length_field_length(4)
        .big_endian()
        .new_codec()
}

struct BiStreamRequestHandler {
    service: BoxCloneService<Request, Response, Infallible>,
    send_stream: FramedWrite<SendStream, LengthDelimitedCodec>,
    recv_stream: FramedRead<RecvStream, LengthDelimitedCodec>,
}

impl BiStreamRequestHandler {
    fn new(
        service: BoxCloneService<Request, Response, Infallible>,
        send_stream: SendStream,
        recv_stream: RecvStream,
    ) -> Self {
        Self {
            service,
            send_stream: FramedWrite::new(send_stream, network_message_frame_codec()),
            recv_stream: FramedRead::new(recv_stream, network_message_frame_codec()),
        }
    }

    async fn handle(mut self) {
        //TODO define wire format
        let request = self.recv_stream.next().await.unwrap().unwrap();
        let request = Request {
            body: request.into(),
        };
        let response = self.service.oneshot(request).await.expect("Infallible");
        self.send_stream.send(response.body).await.unwrap();
        self.send_stream.get_mut().finish().await.unwrap();
    }
}

struct UniStreamRequestHandler {
    service: BoxCloneService<Request, (), Infallible>,
    recv_stream: FramedRead<RecvStream, LengthDelimitedCodec>,
}

impl UniStreamRequestHandler {
    fn new(service: BoxCloneService<Request, (), Infallible>, recv_stream: RecvStream) -> Self {
        Self {
            service,
            recv_stream: FramedRead::new(recv_stream, network_message_frame_codec()),
        }
    }

    async fn handle(mut self) {
        //TODO define wire format
        let request = self.recv_stream.next().await.unwrap().unwrap();
        let request = Request {
            body: request.into(),
        };
        let _ = self.service.oneshot(request).await.expect("Infallible");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{config::EndpointConfig, Result};
    use futures::{future::join, stream::StreamExt};
    use tracing::trace;

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn uni_stream_handler() -> Result<()> {
        let msg = b"hello";
        let config_1 = EndpointConfig::random("test");
        let (endpoint_1, _incoming_1) = Endpoint::new(config_1, "localhost:0")?;
        let pubkey_1 = endpoint_1.config().keypair().public;

        println!("1: {}", endpoint_1.local_addr());

        let config_2 = EndpointConfig::random("test");
        let (endpoint_2, mut incoming_2) = Endpoint::new(config_2, "localhost:0")?;
        let pubkey_2 = endpoint_2.config().keypair().public;
        let addr_2 = endpoint_2.local_addr();
        println!("2: {}", endpoint_2.local_addr());

        let peer_1 = async move {
            let (connection, _, _) = endpoint_1.connect(addr_2).unwrap().await.unwrap();
            assert_eq!(connection.peer_identity(), pubkey_2);
            {
                let mut send_stream = FramedWrite::new(
                    connection.open_uni().await.unwrap(),
                    network_message_frame_codec(),
                );
                send_stream.send(msg.as_ref().into()).await.unwrap();
                send_stream.get_mut().finish().await.unwrap();
            }
            endpoint_1.close();
            // Result::<()>::Ok(())
        };

        let service = expect_uni_service(msg);

        let peer_2 = async move {
            let (connection, mut uni_streams, _bi_streams) =
                incoming_2.next().await.unwrap().await.unwrap();
            assert_eq!(connection.peer_identity(), pubkey_1);

            let recv = uni_streams.next().await.unwrap().unwrap();
            let request_handler = UniStreamRequestHandler::new(service, recv);
            request_handler.handle().await;
            endpoint_2.close();
            // Result::<()>::Ok(())
        };

        join(peer_1, peer_2).await;

        Ok(())
    }

    fn expect_uni_service(expected: &'static [u8]) -> BoxCloneService<Request, (), Infallible> {
        let handle = move |request: Request| async move {
            trace!("recieved: {}", request.body.escape_ascii());
            assert_eq!(request.body.as_ref(), expected);
            Result::<(), Infallible>::Ok(())
        };

        tower::service_fn(handle).boxed_clone()
    }

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn basic_network() -> Result<()> {
        let msg = b"The Way of Kings";
        let config_1 = EndpointConfig::random("test");
        let (endpoint_1, incoming_1) = Endpoint::new(config_1, "localhost:0")?;
        let pubkey_1 = endpoint_1.config().keypair().public;

        trace!("1: {}", endpoint_1.local_addr());

        let config_2 = EndpointConfig::random("test");
        let (endpoint_2, incoming_2) = Endpoint::new(config_2, "localhost:0")?;
        let _pubkey_2 = endpoint_2.config().keypair().public;
        let addr_2 = endpoint_2.local_addr();
        trace!("2: {}", endpoint_2.local_addr());

        let network_1 = Network::start(endpoint_1, incoming_1);
        let network_2 = Network::start(endpoint_2, incoming_2);

        let peer = network_1.connect(addr_2).await?;
        let response = network_1.rpc(peer, msg.as_ref().into()).await?;
        assert_eq!(response, msg.as_ref());

        let msg = b"Words of Radiance";
        let peer_id_1 = PeerId(pubkey_1);
        let response = network_2.rpc(peer_id_1, msg.as_ref().into()).await?;
        assert_eq!(response, msg.as_ref());
        Ok(())
    }
}

fn echo_service() -> BoxCloneService<Request, Response, Infallible> {
    let handle = move |request: Request| async move {
        trace!("recieved: {}", request.body.escape_ascii());
        let response = Response { body: request.body };
        Result::<Response, Infallible>::Ok(response)
    };

    tower::service_fn(handle).boxed_clone()
}

fn noop_service() -> BoxCloneService<Request, (), Infallible> {
    let handle = move |request: Request| async move {
        trace!("recieved: {}", request.body.escape_ascii());
        Result::<(), Infallible>::Ok(())
    };

    tower::service_fn(handle).boxed_clone()
}
