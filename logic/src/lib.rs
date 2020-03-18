pub extern crate legion;

use legion::entity::Entity;
use legion::schedule::Builder as ScheduleBuilder;
use legion::schedule::Schedulable;
use legion::world::World;

use rand::Rng;

pub mod components;
pub mod resources;
mod systems;

use crate::components::{player::Player, tile::Tile, Model, Position};
use crate::resources::TimeStep;

pub type System = Box<dyn Schedulable>;

/// Creates all the required resources in the world.
pub fn create_world() -> World {
    let mut world = World::new();
    world.resources.insert(TimeStep::default());

    let mut rng = rand::thread_rng();

    let trees = 10_000;
    let size = 80.0;
    for _ in 0..trees {
        let x = rng.gen_range(-size / 2.0, size / 2.0);
        let y = rng.gen_range(-size / 2.0, size / 2.0);
        let position = Position([x as f32, y as f32, 0.0].into());
        world.insert((Tile {},), Some((position, Model::Tree)));
    }

    world
}

/// Schedule all game logic systems.
pub fn add_systems(builder: ScheduleBuilder) -> ScheduleBuilder {
    builder.add_system(systems::measure_delta_time())
}

pub fn add_player(world: &mut World) -> Entity {
    let position = Position([0.0; 3].into());
    let model = Model::Circle;

    let tags = (Player,);

    let entities = world.insert(tags, Some((position, model)));

    entities[0]
}
