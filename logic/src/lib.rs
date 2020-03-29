pub extern crate legion;

use legion::entity::Entity;
use legion::schedule::Builder as ScheduleBuilder;
use legion::schedule::Schedulable;
use legion::world::World;

use rand::prelude::*;

use cgmath::Vector3;

pub mod components;
pub mod resources;
pub mod systems;
pub mod tags;

pub mod collision;
pub mod tile_map;

use crate::components::{Model, Position};
use crate::resources::TimeStep;
use crate::tags::{Moveable, Player};
use crate::tile_map::{Placed, Tile, TileKind, TileMap};

pub type System = Box<dyn Schedulable>;

const TREES: usize = 150;
const MUSHROOMS: usize = 50;
const SIZE: usize = 30;

const VOXEL_SIZE: f32 = 1.0 / 16.0;

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
        .add_system(systems::tile_interaction::system())
        .add_system(systems::collision::system())
}

pub fn add_player(world: &mut World) -> Entity {
    let tags = (Player, Moveable);

    let collision = collision::AlignedBox::centered(
        [0.0, 0.0, 8.0 * VOXEL_SIZE].into(),
        [14.0 * VOXEL_SIZE, 3.0 * VOXEL_SIZE, 16.0 * VOXEL_SIZE].into(),
    );

    let components = (
        Position([0.0; 3].into()),
        Model::Player,
        components::Movement::default(),
        components::WorldInteraction::default(),
        components::CollisionBox(collision),
    );

    let entities = world.insert(tags, Some(components));
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
    let mut spawn = |count, model, size: [u8; 3]| {
        for (coord, tile) in tiles.by_ref().take(count) {
            let w = (16 - size[0]) as f32 * 0.5 * VOXEL_SIZE;
            let h = (16 - size[1]) as f32 * 0.5 * VOXEL_SIZE;
            let offset_x = rng.gen_range(-w, w);
            let offset_y = rng.gen_range(-h, h);

            dbg!(offset_x);
            dbg!(offset_y);

            let position =
                Position([coord.x as f32 + offset_x, coord.y as f32 + offset_y, 0.0].into());
            let collision = components::CollisionBox(collision::AlignedBox::centered(
                [0.0, 0.0, VOXEL_SIZE * size[2] as f32 / 2.0].into(),
                Vector3::from(size).cast::<f32>().unwrap() * VOXEL_SIZE,
            ));

            let components = (position, model, collision);
            let entities = world.insert((), Some(components));

            tile.slot = Some(Placed {
                entity: entities[0],
                durability: 1.0,
            });
        }
    };

    spawn(TREES, Model::Tree, [14, 3, 30]);
    spawn(MUSHROOMS, Model::Mushroom, [9, 3, 7]);
}
