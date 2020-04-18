use crate::collision::AlignedBox;
use crate::components::*;
use crate::VOXEL_SIZE;

use protocol::snapshot;

use legion::prelude::{Entity, World};

/// The default components of a player.
pub struct Player {
    pub id: snapshot::EntityId,
    pub position: Position,
    pub model: Model,
    pub movement: Movement,
    pub interaction: WorldInteraction,
    pub collision: Collision,
    pub health: Health,
    pub owner: Owner,
}

/// The default components of an object.
pub struct Object {
    pub id: snapshot::EntityId,
    pub position: Position,
    pub model: Model,
    pub collision: Collision,
    pub health: Health,
    pub breakable: Option<Breakable>,
}

impl Player {
    /// Insert the player's components into an entity.
    pub fn insert(self, world: &mut World, entity: Entity) {
        let Player {
            id,
            position,
            model,
            movement,
            interaction,
            collision,
            health,
            owner,
        } = self;

        world.add_component(entity, id);
        world.add_component(entity, position);
        world.add_component(entity, model);
        world.add_component(entity, movement);
        world.add_component(entity, interaction);
        world.add_component(entity, collision);
        world.add_component(entity, health);
        world.add_component(entity, owner);
    }
}

impl Object {
    /// Insert the ojbect's components into an entity.
    pub fn insert(self, world: &mut World, entity: Entity) {
        let Object {
            id,
            position,
            model,
            collision,
            health,
            breakable,
        } = self;

        world.add_component(entity, id);
        world.add_component(entity, position);
        world.add_component(entity, model);
        world.add_component(entity, collision);
        world.add_component(entity, health);
        if let Some(breakable) = breakable {
            world.add_component(entity, breakable);
        }
    }
}

/// Get the collision component for a specific model.
pub fn collision(model: Model) -> Collision {
    let (width, height) = match model {
        Model::Player => (14, 21),
        Model::Tree => (14, 30),
        Model::Mushroom => (9, 7),
        _ => unimplemented!(),
    };

    let width = width as f32;
    let height = height as f32;

    let bounds = AlignedBox::centered(
        [0.0, 0.0, 0.5 * height * VOXEL_SIZE].into(),
        [width * VOXEL_SIZE, 3.0 * VOXEL_SIZE, height * VOXEL_SIZE].into(),
    );

    Collision {
        bounds,
        ignored: None,
    }
}
