use super::*;
use crate::Snapshot;
use std::sync::Arc;

/// Sent from the server to the client when an event occurs.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Event {
    pub time: u32,
    pub kind: EventKind,
}

/// Different kind of events.
#[derive(Debug, Clone, PackBits, UnpackBits, From)]
pub enum EventKind {
    Chat(Chat),
    Snapshot(Arc<Snapshot>),
    GameOver(GameOver),
}

/// A chat message was sent by a player.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Chat {
    pub player: PlayerId,
    pub message: String,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum GameOver {
    /// The player receiving this won.
    Winner,
    /// The player receiving this lost.
    Loser,
}

impl Event {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            EventKind::Chat(_) => true,
            EventKind::Snapshot(_) => false,
            EventKind::GameOver(_) => true,
        }
    }
}
