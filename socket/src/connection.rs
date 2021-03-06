#![allow(unused_variables)]

use futures::stream::StreamExt;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::{self, delay_queue::Key, DelayQueue, Duration};

use self::serialize::{FromRawPacket, IntoRawPacket};
use crate::packet::{self, Flags, Header, PacketId, Sequence};

/// The number of sequences to buffer on in the receive buffer.
const SEQUENCE_BUFFER_SIZE: usize = 1024;

/// How long to wait before attempting to retransmit a packet.
const RETRANSMIT_DELAY: Duration = Duration::from_millis(100);

/// How long to wait for a response before closing the connection.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);

type RawPacket = Vec<u8>;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("connection closed")]
    Closed,

    #[error("the connection timed out")]
    Timeout,

    #[error("an error occured when closing connection")]
    Shutdown,

    #[error("failed to split payload")]
    SplitPayload(#[source] crate::packet::Error),

    #[error("failed to reconstruct payload")]
    ReconstructPayload(#[source] crate::packet::Error),

    #[error("failed to deserialize packet")]
    Deserialize(#[from] self::serialize::Error),

    #[error("client did not respond correctly to the challenge")]
    InvalidChallengeResponse,
}

pub(crate) struct ConnectionEnv {
    pub(crate) peer_addr: SocketAddr,
    pub(crate) packet_rx: mpsc::Receiver<RawPacket>,
    pub(crate) packet_tx: mpsc::Sender<RawPacket>,
}

pub struct Connection {
    peer_addr: SocketAddr,
    payload_rx: mpsc::Receiver<IncomingPayload>,
    payload_tx: mpsc::Sender<OutgoingPayload>,
    driver: task::JoinHandle<Result<()>>,
}

#[derive(Debug, Copy, Clone)]
pub enum Delivery {
    /// Guarantee that the data arrives in the same order as it was sent.
    Reliable,

    /// Send the packet once. Use when the payload should arrive as soon as possible, but dropping
    /// it has no consequence.
    BestEffort,
}

#[derive(Debug, Copy, Clone)]
struct Init {
    salt: u32,
}

#[derive(Debug, Copy, Clone)]
struct Challenge {
    pepper: u32,
}

#[derive(Debug, Copy, Clone)]
struct ChallengeResponse {
    seasoning: u32,
}

pub(crate) struct OutgoingPayload {
    bytes: Vec<u8>,
    needs_ack: bool,
}

pub(crate) struct IncomingPayload {
    bytes: Vec<u8>,
}

struct Responder {
    packet_tx: mpsc::Sender<RawPacket>,
    packet_rx: mpsc::Receiver<RawPacket>,
    payload_tx: mpsc::Sender<IncomingPayload>,
    payload_rx: mpsc::Receiver<OutgoingPayload>,

    sequences: SequenceBuilder,
    transmit: TransmitQueue,
}

struct SequenceBuilder {
    /// The sequence contained in each slot.
    slots: [Slot; SEQUENCE_BUFFER_SIZE],

    /// The first sequence that occupies as slot.
    start: u16,
}

#[derive(Clone, Default)]
struct Slot {
    /// The sequence that occupies this slot.
    sequence: Option<u16>,

    /// The actual data.
    entry: Box<Sequence>,

    /// Is the sequence complete?
    complete: bool,
}

struct TransmitQueue {
    packets: DelayQueue<(PacketId, RawPacket)>,
    keys: HashMap<PacketId, Key>,
    next_sequence: u16,
}

impl Connection {
    /// Accept a new connection.
    #[allow(dead_code)]
    pub(crate) async fn accept(mut env: ConnectionEnv) -> Result<Connection> {
        let init = env.recv::<Init>().await?;

        let challenge = Challenge::new();
        env.send(challenge).await?;

        let response = env.recv::<ChallengeResponse>().await?;

        if Self::valid_resposne(init, challenge, response) {
            Ok(Self::spawn(env))
        } else {
            Err(Error::InvalidChallengeResponse)
        }
    }

    /// Establish a new connection.
    #[allow(dead_code)]
    pub(crate) async fn establish(mut env: ConnectionEnv) -> Result<Connection> {
        let init = Init::new();
        env.send(init).await?;

        let challenge = env.recv::<Challenge>().await?;

        let response = ChallengeResponse::new(init, challenge);
        env.send(response).await?;

        Ok(Self::spawn(env))
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Send a payload.
    pub async fn send(&mut self, bytes: Vec<u8>, delivery: Delivery) -> Result<()> {
        let needs_ack = match delivery {
            Delivery::Reliable => true,
            Delivery::BestEffort => false,
        };

        let payload = OutgoingPayload { bytes, needs_ack };

        self.payload_tx
            .send(payload)
            .await
            .map_err(|_| Error::Closed)
    }

    /// Recv a payload
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        let payload = self.payload_rx.recv().await?;
        Some(payload.bytes)
    }

    /// Close the connection
    pub async fn shutdown(self) -> Result<()> {
        drop(self.payload_rx);
        drop(self.payload_tx);
        match self.driver.await {
            Err(_) => Err(Error::Shutdown),
            Ok(result) => result,
        }
    }

    fn valid_resposne(init: Init, challenge: Challenge, response: ChallengeResponse) -> bool {
        let expected = ChallengeResponse::new(init, challenge);
        expected.seasoning == response.seasoning
    }

    fn spawn(env: ConnectionEnv) -> Connection {
        let (outgoing_tx, outgoing_rx) = mpsc::channel(16);
        let (incoming_tx, incoming_rx) = mpsc::channel(16);

        let sequences = SequenceBuilder {
            slots: arr![Slot::default(); SEQUENCE_BUFFER_SIZE],
            start: 0,
        };

        let transmit = TransmitQueue {
            packets: DelayQueue::new(),
            keys: HashMap::new(),
            next_sequence: 0,
        };

        let responder = Responder {
            packet_tx: env.packet_tx,
            packet_rx: env.packet_rx,
            payload_tx: incoming_tx,
            payload_rx: outgoing_rx,
            sequences,
            transmit,
        };

        let driver = tokio::spawn(responder.handle_packets());

        Connection {
            peer_addr: env.peer_addr,
            payload_tx: outgoing_tx,
            payload_rx: incoming_rx,
            driver,
        }
    }
}

impl ConnectionEnv {
    async fn recv_packet(&mut self) -> Result<RawPacket> {
        self.packet_rx.recv().await.ok_or(Error::Closed)
    }

    async fn send_packet(&mut self, packet: RawPacket) -> Result<()> {
        self.packet_tx.send(packet).await.map_err(|_| Error::Closed)
    }

    async fn recv<T>(&mut self) -> Result<T>
    where
        T: FromRawPacket,
    {
        let packet = self.recv_packet().await?;
        T::deserialize(&packet).map_err(Into::into)
    }

    async fn send<T>(&mut self, value: T) -> Result<()>
    where
        T: IntoRawPacket,
    {
        let packet = value.serialize();
        self.send_packet(packet).await
    }
}

impl Init {
    pub fn new() -> Init {
        let mut rng = rand::thread_rng();
        let salt = rng.gen();
        Init { salt }
    }
}

impl Challenge {
    pub fn new() -> Challenge {
        let mut rng = rand::thread_rng();
        let pepper = rng.gen();
        Challenge { pepper }
    }
}

impl ChallengeResponse {
    pub fn new(init: Init, challenge: Challenge) -> ChallengeResponse {
        ChallengeResponse {
            seasoning: init.salt ^ challenge.pepper,
        }
    }
}

mod serialize {
    use super::*;
    use std::convert::TryInto;

    #[derive(Debug, Clone, Error)]
    pub enum Error {
        #[error("unexpected end of packet")]
        Eof,
    }

    pub type Result<T, E = Error> = std::result::Result<T, E>;

    pub trait FromRawPacket: Sized {
        fn deserialize(bytes: &[u8]) -> Result<Self>;
    }

    pub trait IntoRawPacket: Sized {
        fn serialize(&self) -> RawPacket;
    }

    fn read_u32(bytes: &[u8]) -> Result<(u32, &[u8])> {
        const SIZE: usize = std::mem::size_of::<u32>();
        if bytes.len() < SIZE {
            Err(Error::Eof)
        } else {
            let (prefix, suffix) = bytes.split_at(SIZE);
            let value = u32::from_be_bytes(prefix.try_into().unwrap());
            Ok((value, suffix))
        }
    }

    fn write_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    impl FromRawPacket for Init {
        fn deserialize(bytes: &[u8]) -> Result<Self> {
            let (salt, _) = read_u32(bytes)?;
            Ok(Init { salt })
        }
    }

    impl IntoRawPacket for Init {
        fn serialize(&self) -> RawPacket {
            let mut bytes = Vec::new();
            write_u32(&mut bytes, self.salt);
            bytes
        }
    }

    impl FromRawPacket for Challenge {
        fn deserialize(bytes: &[u8]) -> Result<Self> {
            let (pepper, _) = read_u32(bytes)?;
            Ok(Challenge { pepper })
        }
    }

    impl IntoRawPacket for Challenge {
        fn serialize(&self) -> RawPacket {
            let mut bytes = Vec::new();
            write_u32(&mut bytes, self.pepper);
            bytes
        }
    }

    impl FromRawPacket for ChallengeResponse {
        fn deserialize(bytes: &[u8]) -> Result<Self> {
            let (seasoning, _) = read_u32(bytes)?;
            Ok(ChallengeResponse { seasoning })
        }
    }

    impl IntoRawPacket for ChallengeResponse {
        fn serialize(&self) -> RawPacket {
            let mut bytes = Vec::new();
            write_u32(&mut bytes, self.seasoning);
            bytes
        }
    }
}

impl ConnectionEnv {
    pub fn pair(cap: usize, peer_addr: SocketAddr) -> (Self, Self) {
        let (a_tx, b_rx) = mpsc::channel(cap);
        let (b_tx, a_rx) = mpsc::channel(cap);

        let a = ConnectionEnv {
            peer_addr,
            packet_tx: a_tx,
            packet_rx: a_rx,
        };
        let b = ConnectionEnv {
            peer_addr,
            packet_tx: b_tx,
            packet_rx: b_rx,
        };

        (a, b)
    }
}

impl Responder {
    pub async fn handle_packets(mut self) -> Result<()> {
        let mut timeout = time::delay_for(CONNECTION_TIMEOUT);

        loop {
            tokio::select! {
                () = &mut timeout => {
                    log::warn!("connection timed out");
                    self.close_connection().await?;
                    break Err(Error::Timeout)
                },

                Some(packet) = self.packet_rx.recv() => {
                    if let Some((header, body)) = Header::extract(&packet) {
                        if header.is_close() {
                            break Ok(());
                        }

                        timeout = time::delay_for(CONNECTION_TIMEOUT);
                        self.handle_packet(header, body).await?;
                    }
                },

                payload = self.payload_rx.recv() => {
                    if let Some(payload) = payload {
                        self.transmit_payload(&payload).await?;
                    } else {
                        self.close_connection().await?;
                        break Ok(());
                    }
                },

                Some(packet) = &mut self.transmit.packets.next() => {
                    let (chunk, packet) = packet.unwrap().into_inner();
                    self.send_packet(packet.clone()).await?;
                    self.transmit.enqueue(chunk, packet);
                },

                else => {
                    self.close_connection().await?;
                    break Ok(());
                }
            }
        }
    }

    async fn handle_packet(&mut self, header: Header, body: &[u8]) -> Result<()> {
        self.acknowledge_packet(header).await?;

        if header.is_ack() {
            let chunk = header.chunk_id();
            self.transmit.acknowledge(header.chunk_id());
        } else if let Some(payload) = self.sequences.insert(header, body)? {
            self.send_payload(payload).await?;
        }

        Ok(())
    }

    async fn acknowledge_packet(&mut self, header: Header) -> Result<()> {
        if header.needs_ack() {
            let ack = Header::ack(header.seq, header.chunk);
            self.send_packet(ack.serialize().to_vec()).await?;
        }

        Ok(())
    }

    async fn close_connection(&mut self) -> Result<()> {
        log::debug!("closing connection");
        let close = Header::close();
        self.send_packet(close.serialize().to_vec()).await?;
        Ok(())
    }

    async fn transmit_payload(&mut self, payload: &OutgoingPayload) -> Result<()> {
        let sequence = self.transmit.allocate_sequence();
        let packets = packet::into_chunks(sequence, &payload.bytes).map_err(Error::SplitPayload)?;

        let mut buffer = Vec::new();
        for (mut header, body) in packets {
            if payload.needs_ack {
                header.flags.insert(Flags::NEEDS_ACK);
            }

            buffer.clear();
            buffer.extend_from_slice(&header.serialize());
            buffer.extend_from_slice(body);

            if payload.needs_ack {
                self.transmit.enqueue(header.chunk_id(), buffer.clone());
            }

            self.send_packet(buffer.clone()).await?;
        }

        Ok(())
    }

    async fn send_packet(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.packet_tx.send(bytes).await.is_err() {
            return Err(Error::Closed);
        }
        Ok(())
    }

    async fn send_payload(&mut self, payload: IncomingPayload) -> Result<()> {
        if self.payload_tx.send(payload).await.is_err() {
            return Err(Error::Closed);
        }
        Ok(())
    }
}

impl SequenceBuilder {
    pub fn insert(&mut self, header: Header, body: &[u8]) -> Result<Option<IncomingPayload>> {
        self.clear_complete(header.seq);

        let slot = self.entry(header.seq);

        if slot.complete {
            return Ok(None);
        }

        let sequence = &mut slot.entry;

        sequence
            .insert_chunk(header, body)
            .map_err(Error::ReconstructPayload)?;

        if sequence.is_complete() {
            slot.complete = true;
            let sequence = std::mem::take(sequence);
            let bytes = sequence.payload();
            Ok(Some(IncomingPayload { bytes }))
        } else {
            Ok(None)
        }
    }

    fn index(sequence: u16) -> usize {
        sequence as usize % SEQUENCE_BUFFER_SIZE
    }

    fn entry(&mut self, sequence: u16) -> &mut Slot {
        let index = Self::index(sequence);
        let slot = &mut self.slots[index];

        match slot.sequence {
            // found associated sequence
            Some(seq) if seq == sequence => slot,

            // insert new entry
            None | Some(_) => {
                *slot = Slot::default();
                slot.sequence = Some(sequence);
                slot
            }
        }
    }

    fn clear_complete(&mut self, current: u16) {
        while current.wrapping_sub(self.start) as usize >= SEQUENCE_BUFFER_SIZE {
            let index = Self::index(self.start);
            self.slots[index] = Slot::default();
            self.start = self.start.wrapping_add(1);
        }
    }
}

impl TransmitQueue {
    pub fn allocate_sequence(&mut self) -> u16 {
        let seq = self.next_sequence;
        self.next_sequence = seq.wrapping_add(1);
        seq
    }

    pub fn acknowledge(&mut self, chunk: PacketId) {
        if let Some(key) = self.keys.remove(&chunk) {
            self.packets.remove(&key);
        }
    }

    pub fn enqueue(&mut self, chunk: PacketId, packet: RawPacket) {
        let key = self.packets.insert((chunk, packet), RETRANSMIT_DELAY);
        self.keys.insert(chunk, key);
    }
}
