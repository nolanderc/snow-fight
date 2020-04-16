
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
    Ping,
    Init,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Ping;

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Init;

impl Request {
    pub fn must_arrive(&self) -> bool {
        match self.kind {
            RequestKind::Ping => false,
            RequestKind::Init => true,
        }
    }
}

impl RequestKind {
    pub fn name(&self) -> &'static str {
        match self {
            RequestKind::Ping => "Ping",
            RequestKind::Init => "Init",
        }
    }
}

impl IntoRequest for Init {
    type Response = crate::Connect;
    fn into_request(self) -> RequestKind {
        RequestKind::Init
    }
}

impl IntoRequest for Ping {
    type Response = crate::Connect;
    fn into_request(self) -> RequestKind {
        RequestKind::Ping
    }
}
