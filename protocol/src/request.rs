
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

impl Request {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            RequestKind::Init(_) => true,
            RequestKind::PlayerList => true,
            RequestKind::SendChat(_) => true,
        }
    }
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
