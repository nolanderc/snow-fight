use cgmath::{prelude::*, Vector3};

use legion::prelude::*;
use legion::system::SubWorld;

use crate::components::{Breakable, Collision, Position, WorldInteraction};
use crate::resources::TimeStep;
use crate::System;

pub fn system() -> System {
    let query = <(Write<WorldInteraction>, Read<Position>)>::query();

    SystemBuilder::new("tile_interaction")
        .read_resource::<TimeStep>()
        .read_component::<Position>()
        .write_component::<Position>()
        .write_component::<Breakable>()
        .read_component::<Collision>()
        .write_component::<Collision>()
        .write_component::<WorldInteraction>()
        .with_query(query)
        .build(move |cmd, world, resources, query| {
            let dt = resources;
            let dt = dt.secs_f32();

            for (entity, (mut interaction, position)) in query.iter_entities(world) {
                if let Some(held) = interaction.holding {
                    let height = world
                        .get_component::<Collision>(entity)
                        .map(|coll| coll.bounds.high.z)
                        .unwrap_or(1.0);

                    if let Some(mut float_pos) = world.get_component_mut::<Position>(held) {
                        float_pos.0 = position.0 + Vector3::new(0.0, 0.0, height);
                    }
                } else if let Some(broken) = mine(world, &mut interaction, *position, dt) {
                    cmd.remove_component::<Breakable>(broken);
                    if let Some(mut collision) = world.get_component_mut::<Collision>(broken) {
                        collision.ignored = Some(entity);
                    }
                }
            }
        })
}

fn mine(
    world: &mut SubWorld,
    interaction: &mut WorldInteraction,
    position: Position,
    dt: f32,
) -> Option<Entity> {
    let target = interaction.breaking?;

    let distance = world
        .get_component::<Position>(target)?
        .distance(position.0);

    if distance > interaction.reach {
        return None;
    }

    let durability = break_entity(world, target, dt)?;
    if durability > 0.0 {
        return None;
    }

    interaction.holding = interaction.breaking.take();

    Some(target)
}

fn break_entity(world: &mut SubWorld, target: Entity, amount: f32) -> Option<f32> {
    let mut breakable = world.get_component_mut::<Breakable>(target)?;
    breakable.durability = f32::max(0.0, breakable.durability - amount);
    Some(breakable.durability)
}
