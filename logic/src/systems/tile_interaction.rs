use cgmath::{prelude::*, Point3};
use legion::prelude::*;

use crate::components::{Position, WorldInteraction};
use crate::resources::TimeStep;
use crate::tile_map::TileMap;
use crate::System;

pub fn system() -> System {
    let query = <(Read<WorldInteraction>, Read<Position>)>::query();

    SystemBuilder::new("tile_interaction")
        .read_resource::<TimeStep>()
        .write_resource::<TileMap>()
        .with_query(query)
        .build(move |cmd, world, resources, query| {
            let (dt, map) = resources;

            for (interaction, position) in query.iter(world) {
                || -> Option<_> {
                    let tile_coord = interaction.breaking?;
                    let tile_world = Point3::new(tile_coord.x as f32, tile_coord.y as f32, 0.0);

                    if tile_world.distance(position.0) <= interaction.reach {
                        let tile = map.get_mut(tile_coord)?;
                        let slot = tile.slot.as_mut()?;

                        slot.durability -= dt.secs_f32();

                        if slot.durability <= 0.0 {
                            cmd.delete(slot.entity);
                            tile.slot = None;
                        }
                    }

                    Some(())
                }();
            }
        })
}
