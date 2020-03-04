//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//! Contains common data structures for the protocol implementation.

mod de;
mod error;
mod leb128;
mod ser;

pub mod event;
pub mod request;
pub mod response;

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

impl Message {
    pub fn must_arrive(&self) -> bool {
        match self {
            Message::Event(event) => event.must_arrive(),
            Message::Response(response) => response.must_arrive(),
        }
    }
}
