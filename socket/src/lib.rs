use futures::future;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::{ToSocketAddrs, UdpSocket};
use tokio::sync::mpsc;

mod receiver;
mod sender;

pub mod error;
pub mod packet;

use crate::error::{Error, Result};
use crate::receiver::RecvState;
use crate::sender::SendState;
use crate::packet::ChunkId;

#[derive(Debug)]
pub struct Socket {
    send: SendHalf,
    recv: RecvHalf,
    addr: Option<SocketAddr>,
}

#[derive(Debug)]
pub struct SendHalf {
    payloads: mpsc::Sender<OutgoingPayload>,
}

#[derive(Debug)]
pub struct RecvHalf {
    payloads: mpsc::Receiver<Result<IncomingPayload>>,
}

/// What kind of connection the socket has.
#[derive(Debug, Copy, Clone)]
enum Connection {
    /// The socket may only send and receive from a single address.
    Connected { remote: SocketAddr },
    /// The socket is free to send and receivee from any address.
    Free,
}

struct OutgoingPayload {
    bytes: Vec<u8>,
    target: Option<SocketAddr>,

    /// Determines if this payload needs to be acked
    needs_ack: bool,
}

struct IncomingPayload {
    bytes: Vec<u8>,
    source: SocketAddr,
}

#[derive(Debug)]
enum Event {
    /// A packet was received.
    ReceivedChunk { chunk: ChunkId, addr: SocketAddr },

    /// A previously sent chunk was acknowledged.
    ChunkAcknowledged { chunk: ChunkId },

    /// A disconnect was requested.
    RequestDisconnect,
}

impl Socket {
    /// Bind to a local address.
    pub async fn bind<T>(local_addr: T) -> Result<Socket>
    where
        T: ToSocketAddrs,
    {
        let socket = UdpSocket::bind(local_addr).await?;
        Socket::from_udp(socket, Connection::Free)
    }

    /// Connect to a remote address and bind to a random local one.
    pub async fn connect(remote_addr: SocketAddr) -> Result<Socket>
    {
        let local_addr = (Ipv4Addr::new(0, 0, 0, 0), 0u16);
        let socket = UdpSocket::bind(local_addr).await?;
        socket.connect(remote_addr).await?;

        let connection = Connection::Connected {
            remote: remote_addr,
        };

        Socket::from_udp(socket, connection)
    }

    /// Wrap a UDP socket.
    fn from_udp(socket: UdpSocket, connection: Connection) -> Result<Socket> {
        let addr = socket.local_addr().ok();

        let (send, recv) = Self::init_socket(socket, connection)?;

        Ok(Socket { send, recv, addr })
    }

    /// Get the local address that this socket is bound to.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    /// Split the socket into a send and recv half.
    pub fn split(self) -> (SendHalf, RecvHalf) {
        (self.send, self.recv)
    }

    fn init_socket(socket: UdpSocket, connection: Connection) -> Result<(SendHalf, RecvHalf)> {
        let (sender, payload_rx) = mpsc::channel(16);
        let (mut payload_tx, receiver) = mpsc::channel(16);
        let (events_tx, events_rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let (recv, send) = socket.split();

            let mut send_state = SendState {
                socket: send,
                connection,
                payloads: payload_rx,
                events: events_rx,
            };

            let mut recv_state = RecvState {
                receiver: recv,
                events: events_tx,
                payloads: payload_tx.clone(),
            };

            let result =
                future::try_join(recv_state.handle_incoming(), send_state.handle_outgoing()).await;

            if let Err(e) = result {
                log::error!("connection aborted: {:#}", e);

                let _ = payload_tx.send(Err(e)).await;
            }
        });

        let send = SendHalf { payloads: sender };
        let recv = RecvHalf { payloads: receiver };

        Ok((send, recv))
    }
}

impl SendHalf {
    /// Send a payload.
    pub async fn send(&mut self, bytes: Vec<u8>, needs_ack: bool) -> Result<()> {
        let payload = OutgoingPayload {
            bytes,
            target: None,
            needs_ack,
        };

        self.payloads
            .send(payload)
            .await
            .map_err(|_| Error::ConnectionClosed)
    }

    /// Send a payload.
    pub async fn send_to(
        &mut self,
        bytes: Vec<u8>,
        addr: SocketAddr,
        needs_ack: bool,
    ) -> Result<()> {
        let payload = OutgoingPayload {
            bytes,
            target: Some(addr),
            needs_ack,
        };

        self.payloads
            .send(payload)
            .await
            .map_err(|_| Error::ConnectionClosed)
    }
}

impl RecvHalf {
    /// Recv the next payload.
    pub async fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        match self.payloads.recv().await.transpose()? {
            None => Ok(None),
            Some(payload) => Ok(Some(payload.bytes)),
        }
    }

    /// Recv the next payload.
    pub async fn recv_from(&mut self) -> Result<Option<(Vec<u8>, SocketAddr)>> {
        match self.payloads.recv().await.transpose()? {
            None => Ok(None),
            Some(payload) => Ok(Some((payload.bytes, payload.source))),
        }
    }
}
