use serde::{de, ser};
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    #[error("failed to serialize: {0}")]
    SerializeMessage(String),
    #[error("failed to deserialize: {0}")]
    DeserializeMessage(String),

    #[error("only sequences of known length may be serialized")]
    MissingLength,

    #[error("unexpected EOF")]
    Eof,

    #[error("trailing bytes when deserializing")]
    TrailingBytes,

    #[error("no field with the name {0}")]
    UnknownField(&'static str),

    #[error("the type of the value must be known beforehand")]
    UnknownType,

    #[error("a bool must be either 0 or 1, found {0}")]
    InvalidBool(u8),

    #[error("integer overflow when decoding integer")]
    Leb128Overflow,

    #[error("expected char, found empty string")]
    EmptyString,

    #[error("expected char, found string with multiple characters")]
    MultiCharString,

    #[error(transparent)]
    Utf(#[from] std::str::Utf8Error),

    #[error("deserializing a identifier is not supported")]
    IdentifierNotSupported,

    #[error("ignoring parts of the deserialization not supported")]
    IgnoredNotSupported,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Error {
        Error::SerializeMessage(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Error {
        Error::DeserializeMessage(msg.to_string())
    }
}
