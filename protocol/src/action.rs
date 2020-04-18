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

/// The specified entity is being broken.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Break {
    pub entity: Option<EntityId>,
}

/// Attempt to throw the currently held entity.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Throw {
    #[rabbit(with = "packers::point")]
    pub target: Point3<f32>,
}

/// Attempt to move in the given direction.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Move {
    pub direction: Direction,
}

impl Action {
    pub fn must_arrive(&self) -> bool {
        true
    }
}
