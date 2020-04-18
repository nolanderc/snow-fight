use cgmath::Point3;
use rabbit::{PackBits, UnpackBits};

use crate::{packers, PlayerId};

/// A snapshot of the entities within a world.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Snapshot {
    pub entities: Vec<Entity>,
}

/// An entity within the world.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
}

/// The unique id of an entity.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PackBits, UnpackBits)]
pub struct EntityId(pub u32);

/// The kind of entity.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum EntityKind {
    Object(Object),
    Player(Player),
    Dead,
}

/// An object
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Object {
    /// The position within the world
    #[rabbit(with = "packers::point")]
    pub position: Point3<f32>,
    /// The kind of object.
    pub kind: ObjectKind,
    /// How much durability remains.
    pub durability: Option<f32>,
    /// Current health.
    pub health: u32,
    /// Maximum health.
    pub max_health: u32,
}

/// Different kinds of objcets.
#[derive(Debug, Clone, PackBits, UnpackBits)]
pub enum ObjectKind {
    Tree,
    Mushroom,
}

#[derive(Debug, Clone, PackBits, UnpackBits)]
pub struct Player {
    /// The current position.
    #[rabbit(with = "packers::point")]
    pub position: Point3<f32>,
    /// The direction it is currently moving
    pub movement: Direction,
    /// The entity this player is holding.
    pub holding: Option<EntityId>,
    /// The entity this player currently breaking.
    pub breaking: Option<EntityId>,
    /// The client controlling this player.
    pub owner: PlayerId,
    /// Current health
    pub health: u32,
    /// Maximum health
    pub max_health: u32,
}

bitflags::bitflags! {
    /// Different directions an entity can move.
    #[derive(Default, PackBits, UnpackBits)]
    pub struct Direction: u8 {
        const NORTH = 1;
        const WEST = 2;
        const SOUTH = 4;
        const EAST = 8;
    }
}
