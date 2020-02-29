use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("failed to split payload")]
    SplitPayload(#[source] crate::packet::Error),

    #[error("failed to reconstruct payload")]
    ReconstructPayload(#[source] crate::packet::Error),

    #[error("no target address specified, but the socket is not connected")]
    NoTarget,

    #[error("the connection timed out")]
    ConnectionTimeout,
}
