use futures::future;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::{udp, ToSocketAddrs, UdpSocket};
use tokio::sync::mpsc;

pub mod error;
pub mod packet;

use crate::error::{Error, Result};
use crate::packet::{Flags, Header, SequenceBuilder};

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
    payloads: mpsc::Receiver<IncomingPayload>,
}

/// What kind of connection the socket has.
#[derive(Debug, Copy, Clone)]
enum Connection {
    /// The socket may only send and receive from a single address.
    Connected,
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

struct ChunkId {
    sequence: u16,
    chunk: u8,
}

enum Ack {
    Send { chunk: ChunkId, addr: SocketAddr },
    Receive { chunk: ChunkId },
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
    pub async fn connect<T>(remote_addr: T) -> Result<Socket>
    where
        T: ToSocketAddrs,
    {
        let local_addr = (Ipv4Addr::new(0, 0, 0, 0), 0u16);
        let socket = UdpSocket::bind(local_addr).await?;
        socket.connect(remote_addr).await?;
        Socket::from_udp(socket, Connection::Connected)
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
        let (payload_tx, receiver) = mpsc::channel(16);
        let (acks_tx, acks_rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let (recv, send) = socket.split();

            let result = future::try_join(
                Self::handle_incoming(recv, payload_tx, acks_tx),
                Self::handle_outgoing(send, payload_rx, acks_rx, connection),
            )
            .await;

            if let Err(e) = result {
                log::error!("connection aborted: {:#}", e);
            }
        });

        let send = SendHalf { payloads: sender };
        let recv = RecvHalf { payloads: receiver };

        Ok((send, recv))
    }

    async fn handle_incoming(
        mut receiver: udp::RecvHalf,
        mut payloads: mpsc::Sender<IncomingPayload>,
        mut acks: mpsc::Sender<Ack>,
    ) -> Result<()> {
        const MAX_UDP_PACKET_SIZE: usize = 1 << 16;

        let mut recv_buffer = vec![0; MAX_UDP_PACKET_SIZE];

        let mut sequences = SequenceBuilder::new();

        loop {
            let (len, addr) = receiver.recv_from(&mut recv_buffer).await?;
            let bytes = &recv_buffer[..len];

            match Header::extract(&bytes) {
                Err(e) => log::warn!("failed to extract header: {:#}", e),
                Ok((header, chunk)) => {
                    log::debug!(
                        "received packet {}:{} ({:?})",
                        header.seq,
                        header.chunk,
                        header.flags
                    );

                    // Introduce some artificial packet loss. For testing purposes only.
                    use rand::Rng;
                    if rand::thread_rng().gen_ratio(1, 10) {
                        log::debug!("dropping packet {}:{}", header.seq, header.chunk);
                        continue;
                    }

                    if header.needs_ack() {
                        let chunk = ChunkId {
                            sequence: header.seq,
                            chunk: header.chunk,
                        };
                        acks.send(Ack::Send { chunk, addr }).await;
                    }

                    if header.is_ack() {
                        let chunk = ChunkId {
                            sequence: header.seq,
                            chunk: header.chunk,
                        };
                        acks.send(Ack::Receive { chunk }).await;
                    } else {
                        match sequences.try_reconstruct_payload(header, chunk) {
                            Err(e) => log::warn!("failed to reconstruct sequence: {:#}", e),
                            Ok(None) => {}
                            Ok(Some(payload)) => {
                                let payload = IncomingPayload {
                                    bytes: payload,
                                    source: addr,
                                };

                                payloads.send(payload).await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_outgoing(
        mut sender: udp::SendHalf,
        mut payloads: mpsc::Receiver<OutgoingPayload>,
        mut acks: mpsc::Receiver<Ack>,
        connection: Connection,
    ) -> Result<()> {
        let mut sequence = 0;

        loop {
            tokio::select! {
                Some(payload) = payloads.recv() => {
                    if let Err(e) = Self::send_payload(&mut sender, sequence, payload).await? {
                        log::warn!("failed to send payload: {:#}", e)
                    }
                    sequence = sequence.wrapping_add(1);
                },
                Some(ack) = acks.recv() => {
                    match ack {
                        Ack::Send { chunk, addr } => {
                            Self::send_ack(&mut sender, chunk, addr, connection).await?;
                        }

                        Ack::Receive { chunk } => {
                            log::warn!("unimplemented: stop sending {}:{}", chunk.sequence, chunk.chunk);
                        }
                    }
                },
                else => break Ok(()),
            };
        }
    }

    async fn send_ack(
        sender: &mut udp::SendHalf,
        chunk: ChunkId,
        addr: SocketAddr,
        connection: Connection,
    ) -> Result<()> {
        let header = Header::ack(chunk.sequence, chunk.chunk);

        log::debug!("acking {}:{}", header.seq, header.chunk);

        let write_buffer = header.serialize();

        match connection {
            Connection::Connected => sender.send(&write_buffer).await?,
            Connection::Free => sender.send_to(&write_buffer, &addr).await?,
        };

        Ok(())
    }

    async fn send_payload(
        sender: &mut udp::SendHalf,
        sequence: u16,
        payload: OutgoingPayload,
    ) -> Result<Result<()>> {
        let chunks = match packet::into_chunks(sequence, &payload.bytes) {
            Err(e) => {
                return Ok(Err(Error::SplitPayload(e)));
            }
            Ok(chunks) => chunks,
        };

        log::debug!("sending {} packets...", chunks.len());

        let mut write_buffer = Vec::new();
        for (mut header, chunk) in chunks {
            header.flags.set(Flags::NEEDS_ACK, payload.needs_ack);

            log::debug!(
                "sending packet {}:{} ({:?})",
                header.seq,
                header.chunk,
                header.flags
            );

            write_buffer.extend_from_slice(&header.serialize());
            write_buffer.extend_from_slice(chunk);

            match payload.target {
                None => sender.send(&write_buffer).await?,
                Some(addr) => sender.send_to(&write_buffer, &addr).await?,
            };
        }

        Ok(Ok(()))
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
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        let payload = self.payloads.recv().await?;
        Some(payload.bytes)
    }

    /// Recv the next payload.
    pub async fn recv_from(&mut self) -> Option<(Vec<u8>, SocketAddr)> {
        let payload = self.payloads.recv().await?;
        Some((payload.bytes, payload.source))
    }
}
