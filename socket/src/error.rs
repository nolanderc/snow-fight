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
}

/// Encode the severity of an error.
#[derive(Debug, Error)]
pub(crate) enum Severity<E>
where
    E: std::fmt::Debug + std::error::Error + 'static,
{
    #[error("a fatal error occured")]
    Fatal(#[source] E),
    #[error("an error occured")]
    Soft(#[source] E),
}

impl<E> Severity<E>
where
    E: std::fmt::Debug + std::error::Error + 'static,
{
    pub fn fatal<T>(t: T) -> Self
    where
        T: Into<E>,
    {
        Self::Fatal(t.into())
    }

    pub fn soft<T>(t: T) -> Self
    where
        T: Into<E>,
    {
        Self::Soft(t.into())
    }
}
