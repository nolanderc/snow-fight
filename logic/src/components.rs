use cgmath::{Point3, Vector3};
use derive_more::{Deref, DerefMut};
use legion::prelude::*;
use std::collections::VecDeque;
use crate::collision;

pub use protocol::Direction;

/// The player that controls the entity.
#[derive(Debug, Copy, Clone)]
pub struct Owner(pub protocol::PlayerId);

/// The position of an entity within the world.
#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Position(pub Point3<f32>);

/// The velocity of an entity.
#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Velocity(pub Vector3<f32>);

/// The acceleration currently being applied to the inty.
#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Acceleration(pub Vector3<f32>);

/// The model to render the entity with.
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
    /// All kinds of models.
    pub const KINDS: &'static [Model] = &[
        Model::Rect,
        Model::Circle,
        Model::Tree,
        Model::Player,
        Model::Mushroom,
        Model::Cube,
    ];
}

/// This entity can control its movement within the world.
#[derive(Debug, Clone, Default)]
pub struct Movement {
    /// The directions the entity is moving
    pub direction: Direction,
    /// The maximum speed of the entity.
    pub speed: f32,
}

/// This entity can interact with the world.
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

/// The current healhth of an entity
#[derive(Debug, Clone)]
pub struct Health {
    pub max_points: u32,
    pub points: u32,
}

impl Health {
    /// Create a full health bar with a maximum amount of health points. 
    pub const fn with_max(points: u32) -> Health {
        Health {
            max_points: points,
            points,
        }
    }
}

/// This entity is an entity that deals damage.
#[derive(Debug, Clone)]
pub struct Projectile {
    /// The amount of damage dealt upon impact.
    pub damage: u32,
}

/// This entity can collide with other entities.
#[derive(Debug, Copy, Clone)]
pub struct Collision {
    /// The bounding box of the collider.
    pub bounds: collision::AlignedBox,
    /// This entity ignores collisions with this entity.
    pub ignored: Option<Entity>,
}

/// A list of all collisions that happened during the last tick.
#[derive(Debug, Default)]
pub struct CollisionListener {
    /// The collisions accumulated during the previous tick.
    pub collisions: VecDeque<CollisionEvent>,
}

/// A collision.
#[derive(Debug, Clone)]
pub struct CollisionEvent {
    /// The entity that was collided with.
    pub entity: Entity,
}

impl CollisionListener {
    pub fn new() -> CollisionListener {
        Self::default()
    }
}
