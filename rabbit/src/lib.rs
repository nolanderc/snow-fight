//! Encoding raw bits faster than a rabbit can run.

mod impls;

pub mod read;
pub mod write;

use std::fmt::Display;
use thiserror::Error;

use read::BitReader;
use write::BitWriter;

pub use read::ReadBits;
pub use write::WriteBits;

#[cfg(feature = "derive")]
pub use rabbit_derive::{PackBits, UnpackBits};

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),

    #[error("unexpected eof")]
    Eof,
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub fn to_bytes<T: PackBits>(value: &T) -> Result<Vec<u8>> {
    let mut writer = BitWriter::new();
    value.pack(&mut writer)?;
    Ok(writer.finish())
}

pub fn from_bytes<T: UnpackBits>(bytes: &[u8]) -> Result<T> {
    let mut reader = BitReader::new(bytes);
    T::unpack(&mut reader)
}

pub trait PackBits {
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits;
}

pub trait UnpackBits: Sized {
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits;
}

impl write::Error for Error {
    fn custom<T: Display>(msg: T) -> Error {
        Error::Message(msg.to_string())
    }
}

impl read::Error for Error {
    fn custom<T: Display>(msg: T) -> Error {
        Error::Message(msg.to_string())
    }
}
