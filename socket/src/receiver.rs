use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use tokio::net::udp;
use tokio::sync::mpsc;

use super::{Event, IncomingPayload};
use crate::error::{Error, Result};
use crate::packet::{Header, Sequence};

/// The maximum
const MAX_UDP_PACKET_SIZE: usize = 1 << 16;

pub(crate) struct RecvState {
    pub receiver: udp::RecvHalf,
    pub payloads: mpsc::Sender<Result<IncomingPayload>>,
    pub events: mpsc::Sender<Event>,
}

#[derive(Debug, Copy, Clone)]
struct Packet<'a> {
    header: Header,
    bytes: &'a [u8],
}

struct Client {
    /// The payloads of every sequence.
    sequences: HashMap<u16, Sequence>,

    // TODO: mark sequences as uncomplete.
    /// Sequences that are complete.
    complete: HashSet<u16>,
}

impl RecvState {
    pub async fn handle_incoming(&mut self) -> Result<()> {
        let mut recv_buffer = vec![0; MAX_UDP_PACKET_SIZE];
        let mut clients = HashMap::new();

        loop {
            let (len, addr) = self.receiver.recv_from(&mut recv_buffer).await?;
            let bytes = &recv_buffer[..len];

            let client = clients.entry(addr).or_insert_with(Client::new);

            if let Some(packet) = Self::decode_packet(bytes) {
                self.acknowledge_packet(packet.header, addr).await;

                if !packet.header.is_ack() {
                    if client.complete.contains(&packet.header.seq) {
                        log::debug!(
                            "received packet from complete sequence {}",
                            packet.header.seq
                        );
                    } else {
                        let payload = client.reconstruct_payload(packet, addr).await;

                        if let Some(payload) = payload.transpose() {
                            if self.payloads.send(payload).await.is_err() {
                                self.send_event(Event::RequestDisconnect).await;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Notify the sender about an event.
    async fn send_event(&mut self, event: Event) {
        if self.events.send(event).await.is_err() {
            log::warn!("event channel was closed, dropping event");
        }
    }

    /// Extract the header and data from a packet.
    fn decode_packet(bytes: &[u8]) -> Option<Packet> {
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
                if rand::thread_rng().gen_bool(0.01) {
                    log::warn!("dropping packet {}:{}", header.seq, header.chunk);
                    return None;
                }

                Some(Packet { header, bytes })
            }
        }
    }

    /// If necessary, acknowledge the packet as being received.
    async fn acknowledge_packet(&mut self, header: Header, addr: SocketAddr) {
        let chunk = header.chunk_id();

        if header.needs_ack() {
            self.send_event(Event::ReceivedChunk { chunk, addr }).await;
        }

        if header.is_ack() {
            self.send_event(Event::ChunkAcknowledged { chunk }).await;
        }
    }
}

impl Client {
    /// Create new state for a new client.
    pub fn new() -> Client {
        Client {
            sequences: HashMap::new(),
            complete: HashSet::new(),
        }
    }

    /// Insert the packet into the sequence and return the complete payload if possible.
    async fn reconstruct_payload(
        &mut self,
        packet: Packet<'_>,
        addr: SocketAddr,
    ) -> Result<Option<IncomingPayload>> {
        let header = packet.header;
        let bytes = packet.bytes;

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
