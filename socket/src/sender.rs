use super::{Connection, Event, OutgoingPayload};
use crate::error::{Error, Result, Severity};
use crate::packet::{self, ChunkId, Flags, Header};

use futures::stream::StreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::udp;
use tokio::sync::mpsc;
use tokio::time::{delay_queue::Key, DelayQueue, Duration};

const RETRANSMIT_DELAY: u64 = 100;

pub(crate) struct SendState {
    pub socket: udp::SendHalf,
    pub connection: Connection,
    pub payloads: mpsc::Receiver<OutgoingPayload>,
    pub events: mpsc::Receiver<Event>,
}

struct Sender<'a> {
    env: &'a mut SendState,
    clients: HashMap<SocketAddr, Client>,
    write_buffer: Vec<u8>,

    packet_queue: DelayQueue<Packet>,
    packet_keys: HashMap<ChunkId, Key>,
}

#[derive(Debug, Default)]
struct Client {
    sequence: u16,
}

#[derive(Debug, Clone)]
struct Packet {
    header: Header,
    bytes: Vec<u8>,
    target: Option<SocketAddr>,
}

impl SendState {
    pub async fn handle_outgoing(&mut self) -> Result<()> {
        let mut sender = Sender {
            env: self,
            clients: HashMap::new(),
            write_buffer: Vec::new(),

            packet_queue: DelayQueue::new(),
            packet_keys: HashMap::new(),
        };

        sender.handle_outgoing().await
    }
}

impl<'a> Sender<'a> {
    /// Receives incoming events and packets and relays the appropriate messages to the receiver.
    pub async fn handle_outgoing(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(payload) = self.env.payloads.recv() => {
                    let res = self.handle_payload(payload).await;
                    match res {
                        Err(Severity::Soft(e)) => log::warn!("failed to send payload: {:#}", e),
                        Err(Severity::Fatal(e)) => return Err(e),
                        Ok(()) => {}
                    }
                },
                Some(packet) = self.packet_queue.next() => {
                    match packet {
                        Err(e) => {
                            log::error!("failed to retransmit packet: timer error: {:#}", e);
                        }
                        Ok(packet) => {
                            let packet = packet.into_inner();
                            self.retransmit(packet).await?;
                        }
                    }
                },
                Some(event) = self.env.events.recv() => {
                    match event {
                        Event::ReceivedChunk { chunk, addr } => {
                            self.send_ack(chunk, addr).await?;
                        }

                        Event::ChunkAcknowledged { chunk } => {
                            self.stop_retransmit(chunk);
                        }

                        Event::RequestDisconnect => {
                            log::warn!("unimplemented: clean disconnect");
                        }
                    }
                },
                else => break Ok(()),
            };
        }
    }

    /// Send a payload to the intended recipient.
    async fn handle_payload(&mut self, payload: OutgoingPayload) -> Result<(), Severity<Error>> {
        let target = self
            .target_addr()
            .or(payload.target)
            .ok_or(Error::NoTarget)
            .map_err(Severity::Soft)?;

        let client = self.clients.entry(target).or_insert_with(Client::default);
        let sequence = client.reserve_sequence();

        self.send_payload(sequence, payload).await
    }

    /// Get the address that the socket is connected to.
    fn target_addr(&self) -> Option<SocketAddr> {
        match self.env.connection {
            Connection::Connected { remote } => Some(remote),
            Connection::Free => None,
        }
    }

    /// Acknowledge a received packet.
    async fn send_ack(&mut self, chunk: ChunkId, addr: SocketAddr) -> Result<()> {
        let header = Header::ack(chunk.seq, chunk.chunk);

        log::debug!("acking {}:{}", header.seq, header.chunk);

        let bytes = header.serialize();
        self.send(&bytes, Some(addr)).await?;

        Ok(())
    }

    /// Send the payload to the recipient.
    async fn send_payload(
        &mut self,
        sequence: u16,
        payload: OutgoingPayload,
    ) -> Result<(), Severity<Error>> {
        let target_addr = self.target_addr().or(payload.target);

        let chunks = packet::into_chunks(sequence, &payload.bytes)
            .map_err(Error::SplitPayload)
            .map_err(Severity::soft)?;

        log::debug!("sending {} packets...", chunks.len());

        for (mut header, chunk) in chunks {
            if payload.needs_ack {
                header.flags.insert(Flags::NEEDS_ACK);
                let packet = Packet {
                    header,
                    bytes: chunk.to_vec(),
                    target: target_addr,
                };
                self.enqueue_retransmit(packet);
            }

            self.send_packet(header, chunk, target_addr)
                .await
                .map_err(Severity::fatal)?;
        }

        Ok(())
    }

    /// Send a packet to the specified address.
    async fn send_packet(
        &mut self,
        header: Header,
        bytes: &[u8],
        addr: Option<SocketAddr>,
    ) -> Result<()> {
        log::debug!(
            "sending packet {}:{} ({:?})",
            header.seq,
            header.chunk,
            header.flags
        );

        self.write_buffer.clear();
        self.write_buffer.extend_from_slice(&header.serialize());
        self.write_buffer.extend_from_slice(bytes);

        let buffer = std::mem::take(&mut self.write_buffer);
        self.send(&buffer, addr).await?;
        self.write_buffer = buffer;

        Ok(())
    }

    /// Send the bytes to the specified address.
    async fn send(&mut self, bytes: &[u8], addr: Option<SocketAddr>) -> Result<()> {
        match self.env.connection {
            Connection::Connected { .. } => {
                self.env.socket.send(bytes).await?;
            }
            Connection::Free => match addr {
                Some(addr) => {
                    self.env.socket.send_to(bytes, &addr).await?;
                }
                None => return Err(Error::NoTarget),
            },
        }

        Ok(())
    }

    /// Place a packet into the retransmission queue. Packets in the queue will be sent to the
    /// receiver once again unless the packet was acknowledged.
    fn enqueue_retransmit(&mut self, packet: Packet) {
        let chunk = packet.header.chunk_id();
        let delay = Duration::from_millis(RETRANSMIT_DELAY);
        let key = self.packet_queue.insert(packet, delay);
        self.packet_keys.insert(chunk, key);
    }

    /// Resend a packet to the receiver.
    async fn retransmit(&mut self, packet: Packet) -> Result<()> {
        log::debug!(
            "retransmitting packet {}:{}",
            packet.header.seq,
            packet.header.chunk
        );

        self.send_packet(packet.header, &packet.bytes, packet.target)
            .await?;

        self.enqueue_retransmit(packet);

        Ok(())
    }

    /// Remove a packet from the retransmission queue.
    fn stop_retransmit(&mut self, chunk: ChunkId) {
        match self.packet_keys.remove(&chunk) {
            None => log::debug!("packet {}:{} already acked", chunk.seq, chunk.chunk),
            Some(key) => {
                log::debug!("packet {}:{} was acked!", chunk.seq, chunk.chunk);
                self.packet_queue.remove(&key);
            }
        }
    }
}

impl Client {
    /// Get the next sequence number.
    pub fn reserve_sequence(&mut self) -> u16 {
        let sequence = self.sequence;
        self.sequence = sequence.wrapping_add(1);
        sequence
    }
}
