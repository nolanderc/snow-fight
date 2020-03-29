use cgmath::{prelude::*, Vector3};
use legion::prelude::*;

use crate::components::{Direction, Movement, Position};
use crate::resources::TimeStep;
use crate::System;

pub fn system() -> System {
    let query = <(Read<Movement>, Write<Position>)>::query();

    SystemBuilder::new("player_direction")
        .read_resource::<TimeStep>()
        .with_query(query)
        .build(move |_, world, dt, query| {
            for (movement, mut position) in query.iter(world) {
                let mut direction = Vector3::zero();

                if movement.direction.contains(Direction::NORTH) {
                    direction.y += 1.0;
                }
                if movement.direction.contains(Direction::WEST) {
                    direction.x -= 1.0;
                }
                if movement.direction.contains(Direction::SOUTH) {
                    direction.y -= 1.0;
                }
                if movement.direction.contains(Direction::EAST) {
                    direction.x += 1.0;
                }

                if !direction.is_zero() {
                    position.0 += 5.0 * dt.secs_f32() * direction.normalize();
                }
            }
        })
}
