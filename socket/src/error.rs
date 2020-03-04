use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("no target address specified, but the socket is not connected")]
    NoTarget,

    #[error("failed to establish connection")]
    Connect(#[source] crate::connection::Error),
}
