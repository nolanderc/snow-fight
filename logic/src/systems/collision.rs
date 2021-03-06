use cgmath::{prelude::*, Vector3};
use legion::prelude::*;

use crate::collision::{Overlap, SweepCollision};
use crate::components::{Collision, CollisionEvent, CollisionListener, Position, Velocity};
use crate::resources::TimeStep;
use crate::tags::Static;
use crate::System;

/// Find all collisions of objects that move continously, ie. have a velocity.
pub fn continuous_system() -> System {
    let colliders = <(Read<Position>, Read<Collision>)>::query();
    let dynamic = <(
        Write<Position>,
        Write<Velocity>,
        Read<Collision>,
        TryWrite<CollisionListener>,
    )>::query();

    SystemBuilder::new("continuous_collision")
        .read_resource::<TimeStep>()
        .with_query(colliders)
        .with_query(dynamic)
        .build(move |_, world, dt, queries| {
            let (colliders, dynamic) = queries;

            let bounding_boxes = colliders
                .iter_entities(world)
                .map(|(entity, (position, collider))| (entity, bounding_box(*position, *collider)))
                .collect::<Vec<_>>();

            for (entity, components) in dynamic.iter_entities(world) {
                let (mut position, mut velocity, collider, mut listener) = components;

                let delta = velocity.0 * dt.secs_f32();
                let bounds = bounding_box(*position, *collider);

                match first_collision(entity, bounds, delta, &bounding_boxes) {
                    Some((other, collision)) => {
                        position.0 += delta * collision.entry;
                        velocity.0 = Vector3::zero();

                        if let Some(listener) = &mut listener {
                            listener
                                .collisions
                                .push_back(CollisionEvent { entity: other })
                        }
                    }
                    None => position.0 += delta,
                }
            }
        })
}

/// Move entities that move in discrete steps out collisions.
pub fn discrete_system() -> System {
    let obstacles = <(Read<Position>, Read<Collision>)>::query();
    let dynamic = <(Write<Position>, Read<Collision>)>::query().filter(!tag::<Static>());

    SystemBuilder::new("discrete_collision")
        .with_query(obstacles)
        .with_query(dynamic)
        .build(move |_, world, (), queries| {
            let (obstacles, dynamic) = queries;

            let collision_boxes = obstacles
                .iter_entities(world)
                .map(|(entity, (position, collider))| (entity, bounding_box(*position, *collider)))
                .collect::<Vec<_>>();

            let dynamic = dynamic.iter_entities(world).collect::<Vec<_>>();
            let dynamic_entities = dynamic
                .iter()
                .map(|(entity, _)| *entity)
                .collect::<Vec<_>>();

            for (entity, (mut position, collider)) in dynamic {
                let bounds = bounding_box(*position, *collider);
                let mut count = 0;
                let mut sum = Vector3::zero();

                for (other, overlap) in overlaps(entity, bounds, &collision_boxes) {
                    count += 1;
                    if dynamic_entities.contains(&other) {
                        sum += 0.5 * overlap.resolution;
                    } else {
                        sum += overlap.resolution;
                    }
                }

                if count > 0 {
                    let average = sum / count as f32;
                    position.0 += average;
                }
            }
        })
}

/// Find the first collisions of an entity.
fn first_collision(
    entity: Entity,
    collision: Collision,
    delta: Vector3<f32>,
    colliders: &[(Entity, Collision)],
) -> Option<(Entity, SweepCollision)> {
    colliders
        .iter()
        .filter(may_collide_with(entity, collision))
        .filter_map(|(other, collider)| {
            let hit = collision.bounds.sweep(delta, collider.bounds)?;
            Some((*other, hit))
        })
        .min_by(|(_, a_hit), (_, b_hit)| a_hit.entry.partial_cmp(&b_hit.entry).unwrap())
}

/// Find all overlaps between an entity.
fn overlaps<'a>(
    entity: Entity,
    collision: Collision,
    colliders: &'a [(Entity, Collision)],
) -> impl Iterator<Item = (Entity, Overlap)> + 'a {
    colliders
        .iter()
        .filter(may_collide_with(entity, collision))
        .filter_map(move |&(other, collider)| {
            let overlap = collision.bounds.overlap(collider.bounds)?;
            Some((other, overlap))
        })
}

/// Creates a new predicate that is true if a collider may collide with a specific entity.
fn may_collide_with(entity: Entity, collider: Collision) -> impl Fn(&&(Entity, Collision)) -> bool {
    move |(other, other_collider)| {
        entity != *other
            && collider.ignored != Some(*other)
            && other_collider.ignored != Some(entity)
    }
}

/// Get the bounding box of an entity. The collision component's bounding box is centered around
/// origio, so we have to translate it to the current position of the entity.
fn bounding_box(position: Position, collision: Collision) -> Collision {
    Collision {
        bounds: collision.bounds.translate(position.0.to_vec()),
        ..collision
    }
}
