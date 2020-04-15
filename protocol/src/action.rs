use super::*;
use cgmath::Point3;
use snapshot::{Direction, EntityId};

/// Sent from the client to the server when an action is performed.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Action {
    pub kind: ActionKind,
}

/// Different kind of actions.
#[derive(Debug, Clone, PackBits, UnpackBits, From)]
pub enum ActionKind {
    Break(Break),
    Throw(Throw),
    Move(Move),
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Break {
    pub entity: Option<EntityId>,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Throw {
    #[rabbit(with = "packers::point")]
    pub target: Point3<f32>,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Move {
    pub direction: Direction,
}

impl Action {
    pub fn must_arrive(&self) -> bool {
        true
    }
}
