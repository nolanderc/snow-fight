use super::*;
use crate::snapshot::Snapshot;
use std::convert::TryFrom;
use thiserror::Error;

/// Sent from the server to the client in response to a request.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Response {
    pub channel: Channel,
    pub kind: ResponseKind,
}

/// Different kinds of responses.
#[derive(Debug, Clone, PackBits, UnpackBits, From)]
pub enum ResponseKind {
    Error(String),
    Pong(Pong),
    Connect(Connect),
    PlayerList(PlayerList),
    ChatSent,
}

#[derive(Debug, Clone, Error)]
pub enum FromResponseError {
    #[error("request failed: {0}")]
    Error(String),
    #[error("invalid response, found {found} expected {expected}")]
    InvalidResponse {
        found: &'static str,
        expected: &'static str,
    },
}

/// A list of the currently connected clients
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct PlayerList {
    pub players: Vec<PlayerId>,
}

/// Response to a Ping.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Pong;

/// Establish the connection and initialize the world.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Connect {
    /// The id assigned to the receiving client.
    pub player_id: PlayerId,
    pub snapshot: Snapshot,
}

impl<R> From<(Channel, R)> for Response
where
    R: Into<ResponseKind>,
{
    fn from((channel, kind): (Channel, R)) -> Self {
        Response {
            channel,
            kind: kind.into(),
        }
    }
}

impl Response {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            ResponseKind::Error(_) => true,
            ResponseKind::Connect(_) => true,
            ResponseKind::PlayerList(_) => true,
            ResponseKind::ChatSent => true,
            ResponseKind::Pong(_) => false,
        }
    }
}

impl ResponseKind {
    pub fn name(&self) -> &'static str {
        match self {
            ResponseKind::Error(_) => "Error",
            ResponseKind::Connect(_) => "Connect",
            ResponseKind::PlayerList(_) => "PlayerList",
            ResponseKind::ChatSent => "ChatSent",
            ResponseKind::Pong(_) => "Pong",
        }
    }
}

macro_rules! try_extract {
    ($value:expr, $variant:ident $(( $($bindings:tt),* ))? => $expr:expr) => {
        match $value {
            ResponseKind::$variant $(( $($bindings),* ))? => $expr,
            ResponseKind::Error(err) => Err(FromResponseError::Error(err)),
            value => Err(FromResponseError::InvalidResponse {
                found: value.name(),
                expected: stringify!($variant),
            }),
        }
    }
}

impl TryFrom<ResponseKind> for Connect {
    type Error = FromResponseError;
    fn try_from(value: ResponseKind) -> Result<Self, Self::Error> {
        try_extract!(value, Connect(connect) => Ok(connect))
    }
}

impl TryFrom<ResponseKind> for Pong {
    type Error = FromResponseError;
    fn try_from(value: ResponseKind) -> Result<Self, Self::Error> {
        try_extract!(value, Pong(pong) => Ok(pong))
    }
}
