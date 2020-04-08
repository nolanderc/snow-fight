use cgmath::{prelude::*, Vector3};
use legion::prelude::*;

use crate::components::{Breakable, Position, WorldInteraction};
use crate::resources::TimeStep;
use crate::System;

pub fn system() -> System {
    let query = <(Write<WorldInteraction>, Read<Position>)>::query();

    SystemBuilder::new("tile_interaction")
        .read_resource::<TimeStep>()
        .read_component::<Position>()
        .write_component::<Breakable>()
        .write_component::<Position>()
        .with_query(query)
        .build(move |cmd, world, resources, query| {
            let dt = resources;

            for (mut interaction, position) in query.iter(world) {
                if let Some(held) = interaction.holding {
                    if let Some(mut float_pos) = world.get_component_mut::<Position>(held) {
                        float_pos.0 = position.0 + Vector3::new(0.0, 0.0, 1.0);
                    }

                    continue;
                } else {
                    || -> Option<_> {
                        let target = interaction.breaking?;
                        let distance = world
                            .get_component::<Position>(target)?
                            .distance(position.0);

                        if distance <= interaction.reach {
                            let breakable = world.get_component_mut::<Breakable>(target);
                            if let Some(mut breakable) = breakable {
                                breakable.durability -= dt.secs_f32();

                                if breakable.durability <= 0.0 {
                                    interaction.holding = interaction.breaking.take();
                                    cmd.remove_component::<Breakable>(target);
                                }
                            }
                        }

                        Some(())
                    }();
                }
            }
        })
}
