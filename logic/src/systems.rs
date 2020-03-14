use legion::system::SystemBuilder;
use std::time::Instant;

use crate::resources::TimeStep;
use crate::System;

pub fn measure_delta_time() -> System {
    let mut previous_tick = Instant::now();
    SystemBuilder::new("measure_delta_time")
        .write_resource::<TimeStep>()
        .build(move |_, _, delta_time, _| {
            **delta_time = TimeStep::from_duration(previous_tick.elapsed());
            previous_tick = Instant::now();
        })
}
