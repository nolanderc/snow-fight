use std::error::Error as StdError;
use std::fmt::Display;

pub trait Error: StdError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display;
}

pub trait WriteBits {
    type Error: Error;

    /// Write `count` bits, starting with the least significant bit (LSB).
    fn write(&mut self, bits: u32, count: u8) -> Result<(), Self::Error>;
}

pub struct BitWriter {
    bytes: Vec<u8>,
    buffer: u64,
    len: u8,
}

macro_rules! flush {
    ($writer:ident, $ty:ty) => {{
        const SIZE: u8 = 8 * std::mem::size_of::<$ty>() as u8;
        let lower = $writer.buffer as $ty;
        $writer.buffer >>= SIZE;
        $writer.bytes.extend_from_slice(&lower.to_le_bytes());
        $writer.len = $writer.len.saturating_sub(SIZE);
    }};
}

impl BitWriter {
    pub fn new() -> BitWriter {
        BitWriter {
            bytes: Vec::new(),
            buffer: 0,
            len: 0,
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.flush();

        while self.len > 0 {
            flush!(self, u8);
        }

        self.bytes
    }

    fn flush(&mut self) {
        if self.len >= 32 {
            flush!(self, u32);
        }
    }
}

impl Default for BitWriter {
    fn default() -> Self {
        BitWriter::new()
    }
}

impl WriteBits for BitWriter {
    type Error = crate::Error;

    fn write(&mut self, bits: u32, count: u8) -> Result<(), Self::Error> {
        let count = u8::min(count, 32);
        let mask = u32::max_value().checked_shr(32 - count as u32).unwrap_or(0);
        let masked_bits = (bits & mask) as u64;
        self.buffer |= masked_bits << self.len;
        self.len += count;

        self.flush();

        Ok(())
    }
}
