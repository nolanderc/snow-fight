use std::error::Error as StdError;
use std::fmt::Display;

pub trait Error: StdError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display;
}

pub trait ReadBits {
    type Error: Error;

    fn read(&mut self, count: u8) -> Result<u32, Self::Error>;
}

pub struct BitReader<'a> {
    bytes: &'a [u8],
    buffer: u64,
    len: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> BitReader<'a> {
        BitReader {
            bytes,
            buffer: 0,
            len: 0,
        }
    }

    fn refill_buffer(&mut self) {
        let space = (64 - self.len) / 8;
        let available = usize::min(self.bytes.len(), space as usize);

        let (prefix, rest) = self.bytes.split_at(available);

        for &byte in prefix {
            self.buffer |= (byte as u64) << self.len;
            self.len += 8;
        }

        self.bytes = rest;
    }
}

impl<'a> ReadBits for BitReader<'a> {
    type Error = crate::Error;

    fn read(&mut self, count: u8) -> Result<u32, Self::Error> {
        let count = u8::min(count, 32);

        if count > self.len {
            self.refill_buffer();
        }

        if count > self.len {
            Err(crate::Error::Eof)
        } else {
            let mask = u32::max_value().checked_shr(32 - count as u32).unwrap_or(0);
            let bits = self.buffer as u32 & mask;
            self.buffer >>= count;
            self.len -= count;
            Ok(bits)
        }
    }
}
