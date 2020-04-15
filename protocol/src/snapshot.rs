use cgmath::Point3;
use rabbit::{PackBits, UnpackBits};

use crate::{packers, PlayerId};

/// A snapshot of the entities within a world.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Snapshot {
    pub entities: Vec<Entity>,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PackBits, UnpackBits)]
pub struct EntityId(pub u32);

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum EntityKind {
    Object(Object),
    Player(Player),
    Dead,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Object {
    #[rabbit(with = "packers::point")]
    pub position: Point3<f32>,
    pub kind: ObjectKind,
    pub durability: Option<f32>,
    pub health: u32,
    pub max_health: u32,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum ObjectKind {
    Tree,
    Mushroom,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Player {
    #[rabbit(with = "packers::point")]
    pub position: Point3<f32>,
    pub movement: Direction,
    pub holding: Option<EntityId>,
    pub breaking: Option<EntityId>,
    pub owner: PlayerId,
    pub health: u32,
    pub max_health: u32,
}

bitflags::bitflags! {
    #[derive(Default, PackBits, UnpackBits)]
    pub struct Direction: u8 {
        const NORTH = 1;
        const WEST = 2;
        const SOUTH = 4;
        const EAST = 8;
    }
}
