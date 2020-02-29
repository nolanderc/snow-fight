use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::{udp, ToSocketAddrs, UdpSocket};
use tokio::sync::mpsc;

mod connection;

pub mod error;
mod packet;

pub use crate::connection::*;

use crate::error::{Error, Result};

/// The percentage of artificial packet loss to add (for testing purposes).
const PACKET_LOSS: f64 = 0.00;

type RawPacket = Vec<u8>;

#[derive(Debug)]
pub struct Listener {
    connections: mpsc::Receiver<Connection>,
    addr: Option<SocketAddr>,
}

struct ConnectionStore {
    connections: HashMap<SocketAddr, mpsc::Sender<RawPacket>>,
    listener: mpsc::Sender<Connection>,
    packets: mpsc::Sender<(RawPacket, SocketAddr)>,
}

impl Connection {
    /// Connect to a remote address and bind to a random local one.
    pub async fn connect<T>(remote_addr: T) -> Result<Connection>
    where
        T: ToSocketAddrs,
    {
        let local_addr = (Ipv4Addr::new(0, 0, 0, 0), 0);
        let socket = UdpSocket::bind(local_addr).await?;
        socket.connect(remote_addr).await?;
        let (receiver, sender) = socket.split();

        let (packet_tx, outgoing) = mpsc::channel(16);
        let (incoming, packet_rx) = mpsc::channel(16);

        tokio::spawn(Self::send_packets(sender, outgoing));
        tokio::spawn(Self::recv_packets(receiver, incoming));

        let env = ConnectionEnv {
            packet_rx,
            packet_tx,
        };

        Connection::establish(env).await
    }

    /// Receive packets from a channel and send them to the adressee.
    async fn send_packets(mut socket: udp::SendHalf, mut packets: mpsc::Receiver<RawPacket>) {
        while let Some(packet) = packets.recv().await {
            log::trace!("sending {} bytes", packet.len());
            if let Err(e) = socket.send(&packet).await {
                log::error!("failed to send packet: {:#}", e);
            }
        }
    }

    async fn recv_packets(mut socket: udp::RecvHalf, mut packets: mpsc::Sender<RawPacket>) {
        const MAX_UDP_PACKET_SIZE: usize = 1 << 16;
        let mut buffer = vec![0; MAX_UDP_PACKET_SIZE];
        loop {
            match socket.recv(&mut buffer).await {
                Err(e) => {
                    log::error!("failed to receive packet: {:#}", e);
                    break;
                }
                Ok(len) => {
                    log::trace!("receiveing {} bytes...", len);

                    use rand::Rng;
                    if rand::thread_rng().gen_bool(PACKET_LOSS) {
                        log::warn!("dropping packet");
                        continue;
                    }

                    let bytes = buffer[..len].to_vec();
                    if packets.send(bytes).await.is_err() {
                        log::warn!("failde to dispatch packet: channel closed");
                        break;
                    }
                }
            };
        }
    }
}

impl Listener {
    /// Bind to a local address.
    pub async fn bind<T>(local_addr: T) -> Result<Listener>
    where
        T: ToSocketAddrs,
    {
        let socket = UdpSocket::bind(local_addr).await?;
        let addr = socket.local_addr().ok();
        let (receiver, sender) = socket.split();

        let (packet_tx, packet_rx) = mpsc::channel::<(Vec<_>, _)>(16);
        let (connection_tx, connection_rx) = mpsc::channel(16);

        let connections = ConnectionStore {
            connections: HashMap::new(),
            listener: connection_tx,
            packets: packet_tx,
        };

        tokio::spawn(Self::send_packets(sender, packet_rx));
        tokio::spawn(Self::recv_packets(receiver, connections));

        Ok(Listener {
            connections: connection_rx,
            addr,
        })
    }

    /// Get the local address this socket is bound to
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    /// Accept an incoming connection.
    pub async fn accept(&mut self) -> Result<Connection> {
        self.connections.recv().await.ok_or(Error::ConnectionClosed)
    }

    /// Receive packets from a channel and send them to the adressee
    async fn send_packets(
        mut socket: udp::SendHalf,
        mut packets: mpsc::Receiver<(RawPacket, SocketAddr)>,
    ) {
        while let Some((packet, addr)) = packets.recv().await {
            log::trace!("sending {} bytes to [{}]", packet.len(), addr);
            if let Err(e) = socket.send_to(&packet, &addr).await {
                log::error!("failed to send packet: {:#}", e);
            }
        }
    }

    /// Receive packets from a socket and send any new connections to the listener.
    async fn recv_packets(mut socket: udp::RecvHalf, mut connections: ConnectionStore) {
        const MAX_UDP_PACKET_SIZE: usize = 1 << 16;
        let mut buffer = vec![0; MAX_UDP_PACKET_SIZE];

        loop {
            match socket.recv_from(&mut buffer).await {
                Err(e) => log::error!("failed to receive packet: {:#}", e),
                Ok((len, addr)) => {
                    log::trace!("receiving {} bytes from [{}]", len, addr);
                    let bytes = buffer[..len].to_vec();

                    use rand::Rng;
                    if rand::thread_rng().gen_bool(PACKET_LOSS) {
                        log::warn!("dropping packet");
                        continue;
                    }

                    connections.send(bytes, addr).await;
                }
            };
        }
    }
}

impl ConnectionStore {
    /// Send a packet to a client. If the client does not have an active connection, send a new
    /// connection to the listener.
    pub async fn send(&mut self, packet: RawPacket, addr: SocketAddr) {
        let ConnectionStore {
            ref mut connections,
            ref mut listener,
            ref packets,
        } = self;

        let conn = connections.entry(addr).or_insert_with(|| {
            let (a, b) = ConnectionEnv::pair(16);

            let mut listener = listener.clone();
            tokio::spawn(async move {
                match Connection::accept(b).await {
                    Err(e) => log::error!("failed to accept connection: {:#}", e),
                    Ok(conn) => {
                        if listener.send(conn).await.is_err() {
                            log::warn!("failed to accept incoming connection: listener closed");
                        }
                    }
                }
            });

            let mut packet_rx = a.packet_rx;
            let mut packet_tx = packets.clone();
            tokio::spawn(async move {
                while let Some(packet) = packet_rx.recv().await {
                    if packet_tx.send((packet, addr)).await.is_err() {
                        break;
                    }
                }
            });

            a.packet_tx
        });

        if conn.send(packet).await.is_err() {
            log::warn!("dropping connection to [{}]", addr);
            self.connections.remove(&addr);
        }
    }
}
