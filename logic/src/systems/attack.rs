use legion::prelude::*;

use protocol::EntityId;

use crate::components::{CollisionListener, Projectile, Health};
use crate::resources::DeadEntities;
use crate::System;

/// Apply damage when a projectile hits another entity.
pub fn system() -> System {
    let query = <(Read<CollisionListener>, Read<Projectile>)>::query();

    let mut damage = Vec::new();

    SystemBuilder::new("attack")
        .read_component::<EntityId>()
        .write_component::<Health>()
        .write_resource::<DeadEntities>()
        .with_query(query)
        .build(move |cmd, world, dead, query| {
            let mut deleted = Vec::new();

            for (entity, (listener, projectile)) in query.iter_entities_immutable(world) {
                for collision in listener.collisions.iter() {
                    damage.push((collision.entity, projectile.damage));
                    cmd.delete(entity);
                    deleted.push(entity);
                }
            }

            for (entity, damage) in damage.drain(..) {
                if let Some(mut health) = world.get_component_mut::<Health>(entity) {
                    health.points = health.points.saturating_sub(damage);
                    if health.points == 0 {
                        cmd.delete(entity);
                    deleted.push(entity);
                    }
                }
            }

            for entity in deleted {
                if let Some(id) = world.get_component::<EntityId>(entity) {
                    dead.entities.push(*id);
                }
            }
        })
}
