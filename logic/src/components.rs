use bitflags::bitflags;
use cgmath::Point3;
use derive_more::{Deref, DerefMut};

use crate::collision;
use crate::tile_map::TileCoord;

#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Position(pub Point3<f32>);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Model {
    Rect,
    Circle,
    Tree,
    Player,
    Mushroom,
    Cube,
}

impl Model {
    pub const KINDS: &'static [Model] = &[
        Model::Rect,
        Model::Circle,
        Model::Tree,
        Model::Player,
        Model::Mushroom,
        Model::Cube,
    ];
}

#[derive(Debug, Clone, Default)]
pub struct Movement {
    /// The directions the entity is moving
    pub direction: Direction,
    /// The maximum speed of the entity.
    pub speed: f32,
}

bitflags! {
    #[derive(Default)]
    pub struct Direction: u8 {
        const NORTH = 1;
        const WEST = 2;
        const SOUTH = 4;
        const EAST = 8;
    }
}

#[derive(Debug, Clone)]
pub struct WorldInteraction {
    /// The tile currently being broken by the entity.
    pub breaking: Option<TileCoord>,
    /// The maximum range of interacitons
    pub reach: f32,
}

impl Default for WorldInteraction {
    fn default() -> Self {
        WorldInteraction {
            breaking: None,
            reach: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollisionBox(pub collision::AlignedBox);
