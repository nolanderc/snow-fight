//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//! Contains common data structures for the protocol implementation.

pub mod event;
pub mod request;
pub mod response;

pub use event::*;
pub use request::*;
pub use response::*;

pub use rabbit::{to_bytes, from_bytes};

use rabbit::{PackBits, UnpackBits};
use derive_more::From;
use std::fmt::{self, Display, Formatter};

/// A unique identifier for a player.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, PackBits, UnpackBits)]
pub struct PlayerId(pub u32);

/// Top-level data that can be sent from the server to the client.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum Message {
    Event(Event),
    Response(Response),
}

/// The id of a channel in which requests and responses are sent.
#[derive(Debug, Copy, Clone, PackBits, UnpackBits, PartialEq, Eq, Hash)]
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
