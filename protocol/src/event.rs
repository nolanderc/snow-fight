
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

impl Event {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            EventKind::Chat(_) => true,
        }
    }
}
