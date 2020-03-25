use bitflags::bitflags;
use cgmath::{prelude::*, Vector3};
use legion::prelude::*;

use crate::components::Position;
use crate::resources::TimeStep;
use crate::System;

#[derive(Debug, Clone, Default)]
pub struct Input {
    pub direction: Direction,
}

bitflags! {
    #[derive(Default)]
    pub struct Direction: u8 {
        const NORTH = 1;
        const WEST = 2;
        const SOUTH = 4;
        const EAST = 8;
    }
}

pub fn system() -> System {
    let query = <(Read<Input>, Write<Position>)>::query();

    SystemBuilder::new("player_movement")
        .read_resource::<TimeStep>()
        .with_query(query)
        .build(move |_, world, dt, query| {
            for (input, mut position) in query.iter(world) {
                let mut movement = Vector3::zero();

                if input.direction.contains(Direction::NORTH) {
                    movement.y += 1.0;
                }
                if input.direction.contains(Direction::WEST) {
                    movement.x -= 1.0;
                }
                if input.direction.contains(Direction::SOUTH) {
                    movement.y -= 1.0;
                }
                if input.direction.contains(Direction::EAST) {
                    movement.x += 1.0;
                }

                if !movement.is_zero() {
                    position.0 += 5.0 * dt.secs_f32() * movement.normalize();
                }
            }
        })
}
