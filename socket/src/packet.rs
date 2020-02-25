use bitflags::bitflags;
use std::collections::HashMap;
use std::convert::TryInto;
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Copy, Clone, Error)]
pub enum Error {
    #[error("the payload limit of {MAX_PAYLOAD_SIZE} bytes was exceeded")]
    PayloadLimitExceeded,

    #[error("the chunk exceeded it's maximum size: found {actual} expected {MAX_CHUNK_COUNT}")]
    ChunkSizeExceeded { actual: usize },

    #[error("the chunk did not fill up the packet: found {actual} expected {MAX_CHUNK_SIZE}")]
    ChunkNotFull { actual: usize },

    #[error("invalid packet size, needs at least {HEADER_SIZE} bytes")]
    MissingHeader,
}

/// The maximum number of chunks in a sequence.
pub const MAX_CHUNK_COUNT: usize = u8::max_value() as usize;

/// The maximum size (in bytes) of a chunk's payload.
// The MTU is 576 bytes minimum. Subtract the largest IP header (60 bytes) and UDP header (8 bytes)
// and you are left with 508 bytes for the packet.
pub const MAX_CHUNK_SIZE: usize = 508 - HEADER_SIZE;

/// The maximum size of a payload. A payload with more bytes can not be split into chunks.
pub const MAX_PAYLOAD_SIZE: usize = MAX_CHUNK_COUNT * MAX_CHUNK_SIZE;

/// The size of the packet header, in bytes.
pub const HEADER_SIZE: usize = 4;

bitflags! {
    pub struct Flags: u8 {
        /// This packet needs to be acknowledged by the receiver.
        const NEEDS_ACK = 1;

        /// This packet acknowledges another.
        const ACK = 1 << 1;

        /// This is the last chunk of the message.
        const LAST_CHUNK = 1 << 2;
    }
}

/// The header of every packet.
#[derive(Debug, Copy, Clone)]
pub struct Header {
    pub flags: Flags,
    pub chunk: u8,
    pub seq: u16,
}

/// Builds multiple sequences from an unordered stream.
#[derive(Clone, Default)]
pub struct SequenceBuilder {
    sequences: HashMap<u16, Sequence>,
}

/// A sequence if chunks that is being partially constructed by packets.
#[derive(Clone)]
pub struct Sequence {
    max_chunks: usize,
    payload: Vec<u8>,
    received: [bool; MAX_CHUNK_COUNT],
}

/// Split a payload into a sequence of chunks.
pub fn into_chunks(sequence: u16, payload: &[u8]) -> Result<Vec<(Header, &[u8])>> {
    let mut payloads = payload
        .chunks(MAX_CHUNK_SIZE)
        .enumerate()
        .map(|(i, chunk)| -> Result<_> {
            let chunk_id = i.try_into().map_err(|_| Error::PayloadLimitExceeded)?;
            let header = Header::new(sequence, chunk_id);
            Ok((header, chunk))
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some((header, _)) = payloads.last_mut() {
        header.flags.insert(Flags::LAST_CHUNK);
    }

    Ok(payloads)
}

impl Header {
    /// Create a new packet with a specific sequence number and chunk id.
    pub fn new(seq: u16, chunk: u8) -> Self {
        Header {
            flags: Flags::empty(),
            seq,
            chunk,
        }
    }

    /// Acknowledge a previous packet.
    pub fn ack(seq: u16, chunk: u8) -> Self {
        Header {
            flags: Flags::ACK | Flags::LAST_CHUNK,
            seq,
            chunk,
        }
    }

    pub fn needs_ack(self) -> bool {
        self.flags.contains(Flags::NEEDS_ACK)
    }
    
    pub fn is_ack(self) -> bool {
        self.flags.contains(Flags::ACK)
    }

    /// Serialize the header into a stream of bytes
    pub fn serialize(self) -> [u8; HEADER_SIZE] {
        let [seq_lo, seq_hi] = self.seq.to_be_bytes();
        [self.flags.bits(), self.chunk, seq_lo, seq_hi]
    }

    /// Map the header in memory to the data structure.
    pub fn deserialize(bytes: [u8; HEADER_SIZE]) -> Header {
        let [flags, chunk, seq_lo, seq_hi] = bytes;
        Header {
            flags: Flags::from_bits_truncate(flags),
            chunk,
            seq: u16::from_be_bytes([seq_lo, seq_hi]),
        }
    }

    /// Extract the header from a stream of bytes, retruns the remaining bytes.
    pub fn extract(bytes: &[u8]) -> Result<(Header, &[u8])> {
        if bytes.len() < 4 {
            Err(Error::MissingHeader)
        } else {
            let (header, had) = bytes.split_at(4);
            let header = Header::deserialize(header.try_into().unwrap());
            Ok((header, had))
        }
    }
}

impl Default for Sequence {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempty to reconstruct a payload from a chunk.
    ///
    /// Returns `Ok(None)` in case the payload needs additional chunks to complete.
    pub fn try_reconstruct_payload(
        &mut self,
        header: Header,
        chunk: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let sequence = self
            .sequences
            .entry(header.seq)
            .or_insert_with(Sequence::new);
        sequence.insert_chunk(header, chunk)?;
        if sequence.is_complete() {
            let sequence = self.sequences.remove(&header.seq).unwrap();
            Ok(Some(sequence.payload()))
        } else {
            Ok(None)
        }
    }
}

impl Sequence {
    pub fn new() -> Self {
        Sequence {
            max_chunks: MAX_CHUNK_COUNT,
            payload: Vec::new(),
            received: [false; MAX_CHUNK_COUNT],
        }
    }

    /// Get the current payload of the sequence
    pub fn payload(self) -> Vec<u8> {
        self.payload
    }

    /// Sets index of the last expected chunk. This is used to determine if the sequence is complete
    /// or not.
    pub fn set_last_packet(&mut self, chunk: u8) {
        self.max_chunks = 1 + chunk as usize;
    }

    /// Determines if the sequence is complete. 
    pub fn is_complete(&self) -> bool {
        self.received[0..self.max_chunks]
            .iter()
            .all(|received| *received)
    }

    /// Returns true if a specific chunk of this sequence has been returned.
    pub fn has_received(&self, chunk: u8) -> bool {
        self.received[chunk as usize]
    }

    /// Adds a chunk to the sequence.
    pub fn insert_chunk(&mut self, header: Header, chunk: &[u8]) -> Result<()> {
        if chunk.len() > MAX_CHUNK_SIZE {
            return Err(Error::ChunkSizeExceeded {
                actual: chunk.len(),
            });
        }

        if header.flags.contains(Flags::LAST_CHUNK) {
            self.set_last_packet(header.chunk);
        } else if chunk.len() != MAX_CHUNK_SIZE {
            return Err(Error::ChunkNotFull {
                actual: chunk.len(),
            });
        }

        let chunk_index = header.chunk as usize;

        self.received[chunk_index] = true;

        let insert_start = MAX_CHUNK_SIZE * chunk_index;
        let required_size = insert_start + chunk.len();

        if self.payload.len() < required_size {
            self.payload.resize(required_size, 0);
        }

        self.payload[insert_start..required_size].copy_from_slice(chunk);

        Ok(())
    }
}