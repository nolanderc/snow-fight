use serde::{de, ser};
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("failed to serialize: {0}")]
    SerializeMessage(String),
    #[error("failed to deserialize: {0}")]
    DeserializeMessage(String),

    #[error("only sequences of known length may be serialized")]
    MissingLength,
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
