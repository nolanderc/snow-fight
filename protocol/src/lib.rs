//! Author(s):
//! - Christofer Nolander (cnol@kth.se)
//!
//! Contains common data structures for the protocol implementation.

use derive_more::From;
use serde::{Deserialize, Serialize};

/// A message sent from the server to the client.
#[derive(Debug, Clone, Serialize, Deserialize, From)]
pub enum Message {
    Data(Response),
    Error(String),
}

/// Sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize, From)]
pub enum Request {
    Init(Init),
    PlayerList,
}

/// Sent from the server to the client.
#[derive(Debug, Clone, Serialize, Deserialize, From)]
pub enum Response {
    Connect(Connect),
    Start,
    PlayerList(PlayerList),
}

/// Establish the connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connect {
    /// The id assigned to the receiving client.
    pub player_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Init {
    /// The requested nickname.
    pub nickname: String,
}

/// A list of the currently connected clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerList {
    pub players: Vec<Player>,
}

/// A list of the currently connected clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: u32,
    pub nickname: String,
}
