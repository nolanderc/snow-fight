use cgmath::prelude::*;
use legion::prelude::*;

use crate::components::{CollisionBox, Position};
use crate::tags::Moveable;
use crate::System;

pub fn system() -> System {
    let colliders = <(Read<Position>, Read<CollisionBox>)>::query();
    let dynamic = <(Write<Position>, Read<CollisionBox>, Tagged<Moveable>)>::query();

    SystemBuilder::new("collision")
        .with_query(colliders)
        .with_query(dynamic)
        .build(move |_, world, (), queries| {
            let (colliders, dynamic) = queries;

            let bounding_boxes = colliders
                .iter_entities(world)
                .map(|(entity, (position, collider))| {
                    (entity, collider.0.translate(position.0.to_vec()))
                })
                .collect::<Vec<_>>();

            for (entity, (mut position, collider, _)) in dynamic.iter_entities(world) {
                let mut iterations = 0;

                loop {
                    let bounds = collider.0.translate(position.0.to_vec());

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

                            if iterations > 8 {
                                break;
                            }
                        }
                    }
                }
            }
        })
}
