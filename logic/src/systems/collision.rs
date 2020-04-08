use cgmath::{prelude::*, Vector3};
use legion::prelude::*;

use crate::components::{Collision, CollisionListener, Position, Velocity, CollisionEvent};
use crate::resources::TimeStep;
use crate::tags::Moveable;
use crate::System;

pub fn continuous_system() -> System {
    let colliders = <(Read<Position>, Read<Collision>)>::query();
    let dynamic = <(
        Write<Position>,
        Write<Velocity>,
        Read<Collision>,
        TryWrite<CollisionListener>,
        Tagged<Moveable>,
    )>::query();

    SystemBuilder::new("continuous_collision")
        .read_resource::<TimeStep>()
        .with_query(colliders)
        .with_query(dynamic)
        .build(move |_, world, dt, queries| {
            let (colliders, dynamic) = queries;

            let bounding_boxes = colliders
                .iter_entities(world)
                .map(|(entity, (position, collider))| {
                    (
                        entity,
                        Collision {
                            bounds: collider.bounds.translate(position.0.to_vec()),
                            ..*collider
                        },
                    )
                })
                .collect::<Vec<_>>();

            for (entity, (mut position, mut velocity, collider, mut listener, _)) in
                dynamic.iter_entities(world)
            {
                let bounds = collider.bounds.translate(position.0.to_vec());
                let delta = velocity.0 * dt.secs_f32();

                let collision = bounding_boxes
                    .iter()
                    .filter(|(other, _)| *other != entity)
                    .filter(|(other, _)| collider.ignored != Some(*other))
                    .filter(|(_, collider)| collider.ignored != Some(entity))
                    .filter_map(|(other, collider)| {
                        let hit = bounds.sweep(delta, collider.bounds)?;
                        Some((*other, hit))
                    })
                    .min_by(|(_, a_hit), (_, b_hit)| {
                        a_hit.entry.partial_cmp(&b_hit.entry).unwrap()
                    });

                if let Some((other, collision)) = collision {
                    position.0 += delta * collision.entry;
                    velocity.0 = Vector3::zero();

                    if let Some(listener) = &mut listener {
                        listener.collisions.push_back(CollisionEvent { entity: other })
                    }
                } else {
                    position.0 += delta;
                }
            }
        })
}

/// Move entities out collisions
pub fn discrete_system() -> System {
    let colliders = <(Read<Position>, Read<Collision>)>::query();
    let dynamic = <(Write<Position>, Read<Collision>, Tagged<Moveable>)>::query();

    SystemBuilder::new("discrete_collision")
        .with_query(colliders)
        .with_query(dynamic)
        .build(move |_, world, (), queries| {
            let (colliders, dynamic) = queries;

            let bounding_boxes = colliders
                .iter_entities(world)
                .map(|(entity, (position, collider))| {
                    (entity, collider.bounds.translate(position.0.to_vec()))
                })
                .collect::<Vec<_>>();

            for (entity, (mut position, collider, _)) in dynamic.iter_entities(world) {
                let mut iterations = 0;

                loop {
                    let bounds = collider.bounds.translate(position.0.to_vec());

                    let overlap = bounding_boxes
                        .iter()
                        .filter(|(other, _)| *other != entity)
                        .filter_map(|&(_, other_bounds)| bounds.overlap(other_bounds))
                        .max_by(|a, b| a.volume.partial_cmp(&b.volume).unwrap());

                    match overlap {
                        None => break,
                        Some(overlap) => {
                            position.0 += overlap.resolution;
                            iterations += 1;

                            if iterations > 11 {
                                break;
                            }
                        }
                    }
                }
            }
        })
}
