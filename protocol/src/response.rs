
use super::*;

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
    Connect(Connect),
    Start,
    PlayerList(PlayerList),
    ChatSent,
}

/// Establish the connection.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Connect {
    /// The id assigned to the receiving client.
    pub player_id: PlayerId,
}

/// A list of the currently connected clients
#[derive(Debug, Clone, PackBits, UnpackBits)]
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

impl Response {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            ResponseKind::Error(_) => true,
            ResponseKind::Connect(_) => true,
            ResponseKind::Start => true,
            ResponseKind::PlayerList(_) => true,
            ResponseKind::ChatSent => true,
        }
    }
}
