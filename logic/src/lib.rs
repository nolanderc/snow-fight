pub extern crate legion;

use legion::entity::Entity;
use legion::schedule::Builder as ScheduleBuilder;
use legion::schedule::Schedulable;
use legion::world::World;

use rand::prelude::*;

pub mod components;
pub mod resources;
pub mod systems;

pub mod tile_map;

use crate::components::{player::Player, Model, Position};
use crate::resources::TimeStep;
use crate::tile_map::{Tile, TileKind, TileMap};

pub type System = Box<dyn Schedulable>;

const TREES: usize = 15;
const MUSHROOMS: usize = 5;
const SIZE: usize = 10;

/// Creates all the required resources in the world.
pub fn create_world() -> World {
    let mut world = World::new();

    world.resources.insert(TimeStep::default());

    let mut map = island_map(SIZE as i32);
    spawn_objects(&mut world, &mut map);
    world.resources.insert(map);

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

fn island_map(size: i32) -> TileMap {
    let mut map = TileMap::new();

    let r = size - 2;

    for x in -size..=size {
        for y in -size..=size {
            let mag = x * x + y * y;
            let r2 = r * r;

            let kind = if mag <= r2 {
                if mag as f32 / r2 as f32 >= 0.7 {
                    TileKind::Sand
                } else {
                    TileKind::Grass
                }
            } else {
                TileKind::Water
            };

            map.insert([x, y].into(), Tile::default().with_kind(kind));
        }
    }

    map
}

fn spawn_objects(world: &mut World, map: &mut TileMap) {
    let mut tiles = map
        .iter_mut()
        .filter(|(pos, _)| (pos.x, pos.y) != (0, 0))
        .filter(|(_, tile)| matches!(tile.kind, TileKind::Grass))
        .collect::<Vec<_>>();

    let mut rng = rand::thread_rng();
    tiles.shuffle(&mut rng);

    let mut tiles = tiles.into_iter();
    let mut spawn = |count, model| {
        for (coord, tile) in tiles.by_ref().take(count) {
            let position = Position([coord.x as f32, coord.y as f32, 0.0].into());
            let entities = world.insert((), Some((position, model)));
            tile.entity = Some(entities[0]);
        }
    };

    spawn(TREES, Model::Tree);
    spawn(MUSHROOMS, Model::Mushroom);
}
