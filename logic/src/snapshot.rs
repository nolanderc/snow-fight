use legion::prelude::*;

use crate::components::*;
use crate::resources::DeadEntities;
use crate::tags;
use crate::templates;

use std::collections::{hash_map::Entry, HashMap};

use protocol::{Entity as PEntity, EntityId, EntityKind, Object, ObjectKind, Player, Snapshot};

#[derive(Debug, Default)]
pub struct SnapshotEncoder {
    pub mapping: HashMap<EntityId, Entity>,
}

pub struct RestoreConfig {
    pub active_player: Option<Entity>,
}

impl SnapshotEncoder {
    pub fn new() -> Self {
        SnapshotEncoder {
            mapping: HashMap::new(),
        }
    }

    /// Update the network -> ECS entity mapping to match the current state.
    pub fn update_mapping(&mut self, world: &World) {
        <Read<EntityId>>::query()
            .iter_entities_immutable(world)
            .for_each(|(entity, id)| {
                self.mapping.insert(*id, entity);
            })
    }

    /// Make a snapshot of the current world state.
    pub fn make_snapshot(&self, world: &World) -> Snapshot {
        let mut entities = Vec::new();
        entities.extend(players(world));
        entities.extend(objects(world));
        entities.extend(dead(world));
        Snapshot { entities }
    }

    /// Update the world to match a previous snapshot.
    pub fn restore_snapshot(
        &mut self,
        world: &mut World,
        snapshot: &Snapshot,
        config: &RestoreConfig,
    ) {
        for entity in &snapshot.entities {
            match self.mapping.entry(entity.id) {
                Entry::Occupied(entry) => {
                    let target = *entry.get();
                    self.update_entity(world, target, entity, config);
                }
                Entry::Vacant(entry) => {
                    let target = world.insert((), Some(()))[0];
                    entry.insert(target);
                    self.update_entity(world, target, entity, config);
                }
            };
        }
    }

    /// Get the ECS entity index from a network entity
    pub fn lookup(&self, entity: EntityId) -> Option<Entity> {
        self.mapping.get(&entity).copied()
    }

    fn update_entity(
        &self,
        world: &mut World,
        target: Entity,
        data: &PEntity,
        config: &RestoreConfig,
    ) {
        match &data.kind {
            EntityKind::Player(player) => {
                self.update_player(world, target, data.id, player, config);
            }
            EntityKind::Object(object) => {
                self.update_object(world, target, data.id, object);
            }
            EntityKind::Dead => {
                world.delete(target);
            }
        }
    }

    fn update_player(
        &self,
        world: &mut World,
        target: Entity,
        id: EntityId,
        player: &Player,
        config: &RestoreConfig,
    ) {
        let lookup_entity = |entity: EntityId| self.lookup(entity);

        let movement = if Some(target) == config.active_player {
            let movement = world.get_component::<Movement>(target).unwrap();
            (*movement).clone()
        } else {
            Movement {
                direction: player.movement,
                ..Movement::default()
            }
        };

        let template = templates::Player {
            id,
            position: Position(player.position),
            model: Model::Player,
            movement,
            interaction: WorldInteraction {
                breaking: player.breaking.and_then(lookup_entity),
                holding: player.holding.and_then(lookup_entity),
                ..WorldInteraction::default()
            },
            collision: templates::collision(Model::Player),
            health: Health {
                points: player.health,
                max_points: player.max_health,
            },
            owner: Owner(player.owner),
        };

        template.insert(world, target);
        world.add_tag(target, tags::Player);
    }

    fn update_object(&self, world: &mut World, target: Entity, id: EntityId, object: &Object) {
        let model = match object.kind {
            ObjectKind::Tree => Model::Tree,
            ObjectKind::Mushroom => Model::Mushroom,
        };
        let breakable = object.durability.map(|durability| Breakable { durability });
        templates::Object {
            id,
            position: Position(object.position),
            model,
            collision: templates::collision(model),
            health: Health {
                points: object.health,
                max_points: object.max_health,
            },
            breakable,
        }
        .insert(world, target);
        world.add_tag(target, tags::Static);
    }
}

/// Attempt to get the network id of an entity.
fn entity_id<'a>(world: &'a World) -> impl Fn(Entity) -> Option<EntityId> + 'a {
    move |entity| match world.get_component::<EntityId>(entity) {
        Some(id) => Some(*id),
        None => {
            log::warn!("could not find network entity id for entity: {}", entity);
            None
        }
    }
}

fn players(world: &World) -> Vec<PEntity> {
    <(
        Read<EntityId>,
        Read<Position>,
        Read<Movement>,
        Read<WorldInteraction>,
        Read<Health>,
        Read<Owner>,
    )>::query()
    .iter_immutable(world)
    .map(
        move |(id, position, movement, interaction, health, owner)| {
            let player = Player {
                holding: dbg!(interaction.holding).and_then(entity_id(world)),
                breaking: dbg!(interaction.breaking).and_then(entity_id(world)),
                movement: movement.direction,
                position: position.0,
                owner: dbg!(owner.0),
                health: health.points,
                max_health: health.max_points,
            };
            PEntity {
                id: *id,
                kind: EntityKind::Player(player),
            }
        },
    )
    .collect()
}

fn objects(world: &World) -> Vec<PEntity> {
    <(
        Read<EntityId>,
        Read<Position>,
        Read<Model>,
        Read<Health>,
        TryRead<Breakable>,
    )>::query()
    .iter_immutable(world)
    .filter_map(move |(id, position, model, health, breakable)| {
        let kind = match *model {
            Model::Tree => ObjectKind::Tree,
            Model::Mushroom => ObjectKind::Mushroom,
            _ => return None,
        };
        let object = Object {
            position: position.0,
            kind,
            durability: breakable.map(|b| b.durability),
            health: health.points,
            max_health: health.max_points,
        };
        let entity = PEntity {
            id: *id,
            kind: EntityKind::Object(object),
        };
        Some(entity)
    })
    .collect()
}

fn dead(world: &World) -> Vec<PEntity> {
    world
        .resources
        .get::<DeadEntities>()
        .unwrap()
        .entities
        .iter()
        .map(|&id| PEntity {
            id,
            kind: EntityKind::Dead,
        })
        .collect()
}
