pub extern crate legion;

use legion::entity::Entity;
use legion::schedule::Builder as ScheduleBuilder;
use legion::schedule::Schedulable;
use legion::world::World;

use rand::prelude::*;

pub mod components;
pub mod resources;
pub mod systems;

use crate::components::{player::Player, tile::Tile, Model, Position};
use crate::resources::TimeStep;

pub type System = Box<dyn Schedulable>;

/// Creates all the required resources in the world.
pub fn create_world() -> World {
    let mut world = World::new();
    world.resources.insert(TimeStep::default());

    let mut rng = rand::thread_rng();

    let trees = 1_000;
    let mushrooms = 100;
    let size = 40;

    let mut tiles = (-size..=size)
        .flat_map(|i| (-size..=size).map(move |j| (i, j)))
        .filter(|pos| *pos != (0, 0))
        .collect::<Vec<_>>();

    tiles.shuffle(&mut rng);

    let mut tiles = tiles.into_iter();

    for (x, y) in tiles.by_ref().take(trees) {
        let position = Position([x as f32, y as f32, 0.0].into());
        world.insert((Tile {},), Some((position, Model::Tree)));
    }

    for (x, y) in tiles.by_ref().take(mushrooms) {
        let position = Position([x as f32, y as f32, 0.0].into());
        world.insert((Tile {},), Some((position, Model::Mushroom)));
    }

    world
}

/// Schedule all game logic systems.
pub fn add_systems(builder: ScheduleBuilder) -> ScheduleBuilder {
    builder
        .add_system(systems::measure_delta_time())
        .add_system(systems::movement::system())
}

pub fn add_player(world: &mut World) -> Entity {
    let position = Position([0.0; 3].into());
    let model = Model::Player;
    let input = systems::movement::Input::default();

    let tags = (Player,);

    let entities = world.insert(tags, Some((position, model, input)));
    entities[0]
}
