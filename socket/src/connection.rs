#![allow(unused_variables)]

use futures::stream::StreamExt;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::{self, delay_queue::Key, DelayQueue, Duration};

use crate::error::{Error, Result};
use crate::packet::{self, ChunkId, Flags, Header, Sequence};

/// How long to wait before attempting to retransmit a packet.
const RETRANSMIT_DELAY: Duration = Duration::from_millis(100);

/// How long to wait for a response before closing the connection.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);

type RawPacket = Vec<u8>;

pub(crate) struct ConnectionEnv {
    pub(crate) packet_rx: mpsc::Receiver<RawPacket>,
    pub(crate) packet_tx: mpsc::Sender<RawPacket>,
}

pub struct Connection {
    payload_rx: mpsc::Receiver<IncomingPayload>,
    payload_tx: mpsc::Sender<OutgoingPayload>,
    driver: task::JoinHandle<Result<()>>,
}

pub(crate) struct OutgoingPayload {
    bytes: Vec<u8>,
    needs_ack: bool,
}

pub(crate) struct IncomingPayload {
    bytes: Vec<u8>,
}

trait FromRawPacket: Sized {
    fn deserialize(bytes: &[u8]) -> Result<Self>;
}

trait IntoRawPacket: Sized {
    fn serialize(&self) -> RawPacket;
}

#[derive(Debug, Copy, Clone)]
struct Init;

#[derive(Debug, Copy, Clone)]
struct Challenge;

#[derive(Debug, Copy, Clone)]
struct ChallengeResponse;

#[derive(Debug, Copy, Clone)]
struct ConnectionToken;

impl Connection {
    /// Accept a new connection.
    #[allow(dead_code)]
    pub(crate) async fn accept(mut env: ConnectionEnv) -> Result<Connection> {
        let init = env.recv::<Init>().await?;

        let challenge = Challenge::new(init);
        env.send(challenge).await?;

        let response = env.recv::<ChallengeResponse>().await?;
        Self::verify(init, challenge, response)?;

        let token = ConnectionToken::new();
        env.send(token).await?;

        Ok(Self::spawn(env, token))
    }

    /// Establish a new connection.
    #[allow(dead_code)]
    pub(crate) async fn establish(mut env: ConnectionEnv) -> Result<Connection> {
        let init = Init;
        env.send(init).await?;

        let challenge = env.recv::<Challenge>().await?;

        let response = ChallengeResponse::new(init, challenge);
        env.send(response).await?;

        let token = env.recv::<ConnectionToken>().await?;

        Ok(Self::spawn(env, token))
    }

    /// Send a payload.
    pub async fn send(&mut self, bytes: Vec<u8>, needs_ack: bool) -> Result<()> {
        let payload = OutgoingPayload { bytes, needs_ack };

        self.payload_tx
            .send(payload)
            .await
            .map_err(|_| Error::ConnectionClosed)
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
            Err(_) => Err(Error::ConnectionShutdown),
            Ok(result) => result,
        }
    }

    fn verify(init: Init, challenge: Challenge, response: ChallengeResponse) -> Result<()> {
        Ok(())
    }

    fn spawn(env: ConnectionEnv, token: ConnectionToken) -> Connection {
        let (outgoing_tx, outgoing_rx) = mpsc::channel(16);
        let (incoming_tx, incoming_rx) = mpsc::channel(16);

        let sequences = SequenceBuilder {
            sequences: HashMap::new(),
            complete: HashSet::new(),
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
            payload_tx: outgoing_tx,
            payload_rx: incoming_rx,
            driver,
        }
    }
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
    sequences: HashMap<u16, Sequence>,
    complete: HashSet<u16>,
}

struct TransmitQueue {
    packets: DelayQueue<(ChunkId, RawPacket)>,
    keys: HashMap<ChunkId, Key>,
    next_sequence: u16,
}

impl Responder {
    pub async fn handle_packets(mut self) -> Result<()> {
        let mut timeout = time::delay_for(CONNECTION_TIMEOUT);

        loop {
            tokio::select! {
                () = &mut timeout => {
                    log::warn!("connection timed out");
                    self.close_connection().await?;
                    break Err(Error::ConnectionTimeout)
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
            return Err(Error::ConnectionClosed);
        }
        Ok(())
    }

    async fn send_payload(&mut self, payload: IncomingPayload) -> Result<()> {
        if self.payload_tx.send(payload).await.is_err() {
            return Err(Error::ConnectionClosed);
        }
        Ok(())
    }
}

impl SequenceBuilder {
    pub fn insert(&mut self, header: Header, body: &[u8]) -> Result<Option<IncomingPayload>> {
        if self.complete.contains(&header.seq) {
            return Ok(None);
        }

        let sequence = self.sequences.entry(header.seq).or_default();

        sequence
            .insert_chunk(header, body)
            .map_err(Error::ReconstructPayload)?;

        if sequence.is_complete() {
            let sequence = self.sequences.remove(&header.seq).unwrap();

            self.complete.insert(header.seq);

            let payload = IncomingPayload {
                bytes: sequence.payload(),
            };

            Ok(Some(payload))
        } else {
            Ok(None)
        }
    }
}

impl TransmitQueue {
    pub fn allocate_sequence(&mut self) -> u16 {
        let seq = self.next_sequence;
        self.next_sequence = seq.wrapping_add(1);
        seq
    }

    pub fn acknowledge(&mut self, chunk: ChunkId) {
        if let Some(key) = self.keys.remove(&chunk) {
            self.packets.remove(&key);
        }
    }

    pub fn enqueue(&mut self, chunk: ChunkId, packet: RawPacket) {
        let key = self.packets.insert((chunk, packet), RETRANSMIT_DELAY);
        self.keys.insert(chunk, key);
    }
}

impl ConnectionEnv {
    async fn recv_packet(&mut self) -> Result<RawPacket> {
        self.packet_rx.recv().await.ok_or(Error::ConnectionClosed)
    }

    async fn send_packet(&mut self, packet: RawPacket) -> Result<()> {
        self.packet_tx
            .send(packet)
            .await
            .map_err(|_| Error::ConnectionClosed)
    }

    async fn recv<T>(&mut self) -> Result<T>
    where
        T: FromRawPacket,
    {
        let packet = self.recv_packet().await?;
        T::deserialize(&packet)
    }

    async fn send<T>(&mut self, value: T) -> Result<()>
    where
        T: IntoRawPacket,
    {
        let packet = value.serialize();
        self.send_packet(packet).await
    }
}

impl Challenge {
    pub fn new(init: Init) -> Challenge {
        Challenge
    }
}

impl ChallengeResponse {
    pub fn new(init: Init, challenge: Challenge) -> ChallengeResponse {
        ChallengeResponse
    }
}

impl ConnectionToken {
    pub fn new() -> ConnectionToken {
        ConnectionToken
    }
}

impl FromRawPacket for Init {
    fn deserialize(bytes: &[u8]) -> Result<Self> {
        Ok(Init)
    }
}

impl IntoRawPacket for Init {
    fn serialize(&self) -> RawPacket {
        vec![1]
    }
}

impl FromRawPacket for Challenge {
    fn deserialize(bytes: &[u8]) -> Result<Self> {
        Ok(Challenge)
    }
}

impl IntoRawPacket for Challenge {
    fn serialize(&self) -> RawPacket {
        vec![2]
    }
}

impl FromRawPacket for ChallengeResponse {
    fn deserialize(bytes: &[u8]) -> Result<Self> {
        Ok(ChallengeResponse)
    }
}

impl IntoRawPacket for ChallengeResponse {
    fn serialize(&self) -> RawPacket {
        vec![3]
    }
}

impl FromRawPacket for ConnectionToken {
    fn deserialize(bytes: &[u8]) -> Result<Self> {
        Ok(ConnectionToken)
    }
}

impl IntoRawPacket for ConnectionToken {
    fn serialize(&self) -> RawPacket {
        vec![4]
    }
}

impl ConnectionEnv {
    pub fn pair(cap: usize) -> (Self, Self) {
        let (a_tx, b_rx) = mpsc::channel(cap);
        let (b_tx, a_rx) = mpsc::channel(cap);

        let a = ConnectionEnv {
            packet_tx: a_tx,
            packet_rx: a_rx,
        };
        let b = ConnectionEnv {
            packet_tx: b_tx,
            packet_rx: b_rx,
        };

        (a, b)
    }
}
