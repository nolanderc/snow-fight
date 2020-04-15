
use super::*;
use std::convert::TryFrom;

pub trait IntoRequest {
    type Response: TryFrom<crate::ResponseKind>;
    fn into_request(self) -> RequestKind;
}

/// Sent from the client to the server.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Request {
    pub channel: Channel,
    pub kind: RequestKind,
}

/// Different kinds of requests.
#[derive(Debug, Clone, PackBits, UnpackBits, From)]
pub enum RequestKind {
    Ping(Ping),
    Init(Init),
    PlayerList,
    SendChat(String),
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Ping;

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Init {
    /// The requested nickname.
    pub nickname: String,
}

impl Request {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            RequestKind::Ping(_) => false,
            RequestKind::Init(_) => true,
            RequestKind::PlayerList => true,
            RequestKind::SendChat(_) => true,
        }
    }
}

impl RequestKind {
    pub fn name(&self) -> &'static str {
        match self {
            RequestKind::Ping(_) => "Ping",
            RequestKind::Init(_) => "Init",
            RequestKind::PlayerList => "PlayerList",
            RequestKind::SendChat(_) => "SendChat",
        }
    }
}

impl IntoRequest for Init {
    type Response = crate::Connect;
    fn into_request(self) -> RequestKind {
        self.into()
    }
}

impl IntoRequest for Ping {
    type Response = crate::Pong;
    fn into_request(self) -> RequestKind {
        self.into()
    }
}
