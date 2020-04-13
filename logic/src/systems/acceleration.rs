use legion::prelude::*;

use crate::components::{Acceleration, Velocity};
use crate::resources::TimeStep;
use crate::System;

pub fn system() -> System {
    let query = <(Write<Velocity>, Read<Acceleration>)>::query();
    SystemBuilder::new("gravity")
        .read_resource::<TimeStep>()
        .with_query(query)
        .build(move |_, world, dt, query| {
            for (mut velocity, acceleration) in query.iter(world) {
                velocity.0 += dt.secs_f32() * acceleration.0;
            }
        })
}
