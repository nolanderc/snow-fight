//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//! Contains common data structures for the protocol implementation.

mod packers;

pub mod action;
pub mod event;
pub mod request;
pub mod response;
pub mod snapshot;

pub use action::*;
pub use event::*;
pub use request::*;
pub use response::*;
pub use snapshot::*;

pub use rabbit::{from_bytes, to_bytes};

use derive_more::From;
use rabbit::{PackBits, UnpackBits};
use std::fmt::{self, Display, Formatter};

/// A unique identifier for a player.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, PackBits, UnpackBits)]
pub struct PlayerId(pub u32);

/// Top-level data that can be sent from the server to the client.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum ServerMessage {
    Event(Event),
    Response(Response),
}

/// Top-level data that can be sent from the client to the server
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum ClientMessage {
    Request(Request),
    Action(Action),
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

impl ServerMessage {
    pub fn must_arrive(&self) -> bool {
        match self {
            ServerMessage::Event(event) => event.must_arrive(),
            ServerMessage::Response(response) => response.must_arrive(),
        }
    }
}

impl ClientMessage {
    pub fn must_arrive(&self) -> bool {
        match self {
            ClientMessage::Request(request) => request.must_arrive(),
            ClientMessage::Action(action) => action.must_arrive(),
        }
    }
}
