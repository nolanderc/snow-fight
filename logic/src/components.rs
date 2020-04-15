use cgmath::{Point3, Vector3};
use derive_more::{Deref, DerefMut};
use legion::prelude::*;
use std::collections::VecDeque;
use crate::collision;

pub use protocol::Direction;

#[derive(Debug, Copy, Clone)]
pub struct Owner(pub protocol::PlayerId);

#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Position(pub Point3<f32>);

#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Velocity(pub Vector3<f32>);

#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Acceleration(pub Vector3<f32>);

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

#[derive(Debug, Clone)]
pub struct WorldInteraction {
    /// The entity currently being broken by this entity.
    pub breaking: Option<Entity>,
    /// The maximum range of interacitons
    pub reach: f32,

    /// The entity currently held by a player.
    pub holding: Option<Entity>,
}

impl Default for WorldInteraction {
    fn default() -> Self {
        WorldInteraction {
            breaking: None,
            reach: 2.0,
            holding: None,
        }
    }
}

/// An entity that can be broken by the player.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Breakable {
    pub durability: f32,
}

impl Default for Breakable {
    fn default() -> Self {
        Breakable { durability: 1.0 }
    }
}

#[derive(Debug, Clone)]
pub struct Health {
    pub max_points: u32,
    pub points: u32,
}

impl Health {
    pub const fn with_max(points: u32) -> Health {
        Health {
            max_points: points,
            points,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub damage: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct Collision {
    pub bounds: collision::AlignedBox,
    pub ignored: Option<Entity>,
}

#[derive(Debug, Default)]
pub struct CollisionListener {
    pub collisions: VecDeque<CollisionEvent>,
}

#[derive(Debug, Clone)]
pub struct CollisionEvent {
    pub entity: Entity,
}

impl CollisionListener {
    pub fn new() -> CollisionListener {
        Self::default()
    }
}
