use legion::prelude::*;

use crate::components::{CollisionListener, Projectile, Health};
use crate::System;

pub fn system() -> System {
    let query = <(Read<CollisionListener>, Read<Projectile>)>::query();

    let mut damage = Vec::new();

    SystemBuilder::new("attack")
        .write_component::<Health>()
        .with_query(query)
        .build(move |cmd, world, _, query| {
            for (entity, (listener, projectile)) in query.iter_entities_immutable(world) {
                for collision in listener.collisions.iter() {
                    damage.push((collision.entity, projectile.damage));
                    cmd.delete(entity);
                }
            }

            for (entity, damage) in damage.drain(..) {
                if let Some(mut health) = world.get_component_mut::<Health>(entity) {
                    health.points = health.points.saturating_sub(damage);
                    if health.points == 0 {
                        cmd.delete(entity);
                    }
                }
            }
        })
}
