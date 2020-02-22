//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//! Contains common data structures for the protocol implementation.

mod de;
mod error;
mod leb128;
mod ser;

pub use de::from_bytes;
pub use error::{Error, Result};
pub use ser::to_bytes;

use derive_more::From;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

pub use event::*;
pub use request::*;
pub use response::*;

/// A unique identifier for a player.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u32);

/// Top-level data that can be sent from the server to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Event(Event),
    Response(Response),
}

/// The id of a channel in which requests and responses are sent. 
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Channel(pub u32);

pub mod request {
    use super::*;

    /// Sent from the client to the server.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Request {
        pub channel: Channel,
        pub kind: RequestKind,
    }

    /// Different kinds of requests.
    #[derive(Debug, Clone, Serialize, Deserialize, From)]
    pub enum RequestKind {
        Init(Init),
        PlayerList,
        SendChat(String),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Init {
        /// The requested nickname.
        pub nickname: String,
    }

    impl RequestKind {
        pub fn name(&self) -> &'static str {
            match self {
                RequestKind::Init(_) => "Init",
                RequestKind::PlayerList => "PlayerList",
                RequestKind::SendChat(_) => "SendChat",
            }
        }
    }
}

pub mod response {
    use super::*;

    /// Sent from the server to the client in response to a request.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Response {
        pub channel: Channel,
        pub kind: ResponseKind,
    }

    /// Different kinds of responses.
    #[derive(Debug, Clone, Serialize, Deserialize, From)]
    pub enum ResponseKind {
        Error(String),
        Connect(Connect),
        Start,
        PlayerList(PlayerList),
        ChatSent,
    }

    /// Establish the connection.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Connect {
        /// The id assigned to the receiving client.
        pub player_id: PlayerId,
    }

    /// A list of the currently connected clients
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PlayerList {
        pub players: Vec<PlayerId>,
    }

    impl<T> From<(Channel, T)> for Response
    where
        T: Into<ResponseKind>,
    {
        fn from((channel, kind): (Channel, T)) -> Self {
            Response {
                channel,
                kind: kind.into(),
            }
        }
    }
}

pub mod event {
    use super::*;

    /// Sent from the server to the client when an event occurs.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Event {
        pub time: u32,
        pub kind: EventKind,
    }

    /// Different kind of events.
    #[derive(Debug, Clone, Serialize, Deserialize, From)]
    pub enum EventKind {
        Chat(Chat),
    }

    /// A chat message was sent by a player.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Chat {
        pub player: PlayerId,
        pub message: String,
    }
}

impl Into<u32> for PlayerId {
    fn into(self) -> u32 {
        self.0
    }
}

impl Display for PlayerId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "P{}", self.0)
    }
}
