use futures::stream::StreamExt;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use tokio::net::udp;
use tokio::sync::mpsc;
use tokio::time::{delay_queue::Key, DelayQueue, Duration};

use super::{Event, IncomingPayload};
use crate::error::{Error, Result};
use crate::packet::{Header, Sequence};

/// The maximum size of a UDP packet.
const MAX_UDP_PACKET_SIZE: usize = 1 << 16;

/// The percentage of artificial packet loss to add (for testing purposes).
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

/// The percentage of artificial packet loss to add (for testing purposes).
const PACKET_LOSS: f64 = 0.00;

pub(crate) struct RecvState {
    pub socket: udp::RecvHalf,
    pub payloads: mpsc::Sender<Result<IncomingPayload>>,
    pub events: mpsc::Sender<Event>,
}

struct Receiver<'a> {
    payloads: &'a mut mpsc::Sender<Result<IncomingPayload>>,
    events: &'a mut mpsc::Sender<Event>,
    timeouts: DelayQueue<SocketAddr>,
}

#[derive(Debug, Clone)]
struct Packet {
    header: Header,
    bytes: Vec<u8>,
    source: SocketAddr,
}

type ClientStore = HashMap<SocketAddr, Client>;

struct Client {
    key: Key,

    /// The payloads of every sequence.
    sequences: HashMap<u16, Sequence>,

    // TODO: mark sequences as uncomplete.
    /// Sequences that are complete.
    complete: HashSet<u16>,
}

impl RecvState {
    pub async fn handle_incoming(&mut self) -> Result<()> {
        let mut receiver = Receiver {
            payloads: &mut self.payloads,
            events: &mut self.events,
            timeouts: DelayQueue::new(),
        };

        let (packets_tx, packets_rx) = mpsc::channel(16);

        let read = Self::read_packets(&mut self.socket, packets_tx);
        let dispatch = receiver.handle_incoming(packets_rx);

        tokio::select! {
            result = read => result,
            result = dispatch => result,
        }
    }

    async fn read_packets(
        socket: &mut udp::RecvHalf,
        mut packets: mpsc::Sender<Packet>,
    ) -> Result<()> {
        let mut buffer = vec![0; MAX_UDP_PACKET_SIZE];

        loop {
            let (len, source) = socket.recv_from(&mut buffer).await?;
            let bytes = &buffer[..len];

            if let Some((header, bytes)) = Self::decode_packet(bytes) {
                let packet = Packet {
                    source,
                    header,
                    bytes: bytes.to_vec(),
                };

                if packets.send(packet).await.is_err() {
                    break Ok(());
                }
            }
        }
    }

    /// Extract the header and data from a packet.
    fn decode_packet(bytes: &[u8]) -> Option<(Header, &[u8])> {
        match Header::extract(bytes) {
            Err(e) => {
                log::warn!("failed to extract header from packet: {:#}", e);
                None
            }
            Ok((header, bytes)) => {
                log::debug!(
                    "received packet {}:{} ({:?})",
                    header.seq,
                    header.chunk,
                    header.flags
                );

                // Introduce some artificial packet loss. For testing purposes only.
                use rand::Rng;
                if rand::thread_rng().gen_bool(PACKET_LOSS) {
                    log::warn!("dropping packet {}:{}", header.seq, header.chunk);
                    return None;
                }

                Some((header, bytes))
            }
        }
    }
}

impl<'a> Receiver<'a> {
    pub async fn handle_incoming(&mut self, mut packets: mpsc::Receiver<Packet>) -> Result<()> {
        let mut clients = ClientStore::new();

        loop {
            tokio::select! {
                Some(packet) = packets.recv() => {
                    let addr = packet.source;
                    if packet.header.is_close() {
                        self.send_event(Event::ConnectionClosed { addr }).await;
                        clients.remove(&addr);
                    }

                    let client = self.get_or_insert_client(&mut clients, addr);
                    self.handle_packet(client, packet).await;
                },
                Some(addr) = self.timeouts.next() => {
                    match addr {
                        Err(e) => log::error!("client timed out: time error: {:#}", e),
                        Ok(addr) => {
                            let addr = addr.into_inner();
                            log::debug!("client [{}] timed out", addr);
                            clients.remove(&addr);
                            self.send_event(Event::RequestDisconnect { addr }).await;
                        }
                    }
                },
                else => break Ok(()),
            }
        }
    }

    fn get_or_insert_client<'c>(
        &mut self,
        clients: &'c mut ClientStore,
        addr: SocketAddr,
    ) -> &'c mut Client {
        clients
            .entry(addr)
            .or_insert_with(|| self.allocate_client(addr))
    }

    async fn handle_packet(&mut self, client: &mut Client, packet: Packet) {
        let addr = packet.source;
        log::debug!("received {} bytes from [{}]", packet.bytes.len(), addr);

        self.acknowledge_packet(&packet).await;

        self.timeouts.reset(&client.key, CLIENT_TIMEOUT);

        if packet.contains_payload() {
            if client.complete.contains(&packet.header.seq) {
                log::debug!(
                    "received packet from complete sequence {}",
                    packet.header.seq
                );
            } else {
                let payload = client.reconstruct_payload(&packet).await;

                if let Some(payload) = payload.transpose() {
                    if self.payloads.send(payload).await.is_err() {
                        self.send_event(Event::RequestDisconnect { addr }).await;
                    }
                }
            }
        }
    }

    fn allocate_client(&mut self, addr: SocketAddr) -> Client {
        let key = self.timeouts.insert(addr, CLIENT_TIMEOUT);
        Client::new(key)
    }

    /// Notify the sender about an event.
    async fn send_event(&mut self, event: Event) {
        if self.events.send(event).await.is_err() {
            log::warn!("event channel was closed, dropping event");
        }
    }

    /// If necessary, acknowledge the packet as being received.
    async fn acknowledge_packet(&mut self, packet: &Packet) {
        let chunk = packet.header.chunk_id();
        let addr = packet.source;

        if packet.header.needs_ack() {
            self.send_event(Event::ReceivedChunk { chunk, addr }).await;
        }

        if packet.header.is_ack() {
            self.send_event(Event::ChunkAcknowledged { chunk }).await;
        }
    }
}

impl Packet {
    pub fn contains_payload(&self) -> bool {
        !self.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.header.is_ack() || self.header.is_close()
    }
}

impl Client {
    /// Create new state for a new client.
    pub fn new(key: Key) -> Client {
        Client {
            key,
            sequences: HashMap::new(),
            complete: HashSet::new(),
        }
    }

    /// Insert the packet into the sequence and return the complete payload if possible.
    async fn reconstruct_payload(&mut self, packet: &Packet) -> Result<Option<IncomingPayload>> {
        let addr = packet.source;
        let header = packet.header;
        let bytes = &packet.bytes;

        let sequence = self.sequences.entry(header.seq).or_default();

        sequence
            .insert_chunk(header, bytes)
            .map_err(Error::ReconstructPayload)?;

        if sequence.is_complete() {
            let sequence = self.sequences.remove(&header.seq).unwrap();

            self.complete.insert(header.seq);

            let payload = IncomingPayload {
                bytes: sequence.payload(),
                source: addr,
            };

            Ok(Some(payload))
        } else {
            Ok(None)
        }
    }
}
