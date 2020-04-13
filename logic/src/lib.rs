pub extern crate legion;

pub mod components;
pub mod events;
pub mod resources;
pub mod systems;
pub mod tags;

pub mod collision;
pub mod tile_map;

use legion::entity::Entity;
use legion::schedule::{Builder as ScheduleBuilder, Schedulable, Schedule};
use legion::world::World;

use rand::prelude::*;

use cgmath::Vector3;

use std::time::{Duration, Instant};

use crate::components::{Model, Position};
use crate::resources::TimeStep;
use crate::tags::Player;
use crate::tile_map::{Tile, TileCoord, TileKind, TileMap};

pub type System = Box<dyn Schedulable>;

const TREES: usize = 150;
const MUSHROOMS: usize = 150;
const SIZE: usize = 30;

const VOXEL_SIZE: f32 = 1.0 / 16.0;

const TARGET_TICK_RATE: u32 = 120;

pub struct Executor {
    schedule: Schedule,
    previous_tick: Instant,
}

impl Executor {
    pub fn new(schedule: ScheduleBuilder) -> Executor {
        Executor {
            schedule: schedule.build(),
            previous_tick: Instant::now(),
        }
    }

    pub fn tick(&mut self, world: &mut World) {
        let now = Instant::now();
        if let Some(elapsed) = now.checked_duration_since(self.previous_tick) {
            let target_delay = Duration::from_secs(1) / TARGET_TICK_RATE;

            let mut single_tick = |dt| {
                let time_step = TimeStep::from_duration(dt);
                world.resources.insert(time_step);
                self.schedule.execute(world);
            };

            let mut remaining = elapsed;
            while let Some(rest) = remaining.checked_sub(target_delay) {
                single_tick(target_delay);
                // fast forward if we are too far behind
                remaining = if rest.as_secs() >= 1 {
                    Duration::from_secs(0)
                } else {
                    rest
                };
            }
            single_tick(remaining);

            world.resources.insert(TimeStep::from_duration(elapsed));
            self.previous_tick = now;
        }
    }
}

/// Creates all the required resources in the world.
pub fn create_world() -> World {
    let mut world = World::new();

    world.resources.insert(TimeStep::default());

    let mut map = island_map(SIZE as i32);
    spawn_objects(&mut world, &mut map);
    world.resources.insert(map);

    world.defrag(None);

    world
}

/// Schedule all game logic systems.
pub fn add_systems(builder: ScheduleBuilder) -> ScheduleBuilder {
    builder
        .add_system(systems::movement::system())
        .add_system(systems::acceleration::system())
        .add_system(systems::tile_interaction::system())
        .add_system(systems::collision::continuous_system())
        .add_system(systems::collision::discrete_system())
        .add_system(systems::attack::system())
}

pub fn add_player(world: &mut World) -> Entity {
    let tags = (Player,);

    let width = 14.0;
    let height = 21.0;

    let bounds = collision::AlignedBox::centered(
        [0.0, 0.0, 0.5 * height * VOXEL_SIZE].into(),
        [width * VOXEL_SIZE, 3.0 * VOXEL_SIZE, height * VOXEL_SIZE].into(),
    );

    let components = (
        Position([0.0; 3].into()),
        Model::Player,
        components::Movement::default(),
        components::WorldInteraction::default(),
        components::Collision {
            bounds,
            ignored: None,
        },
        components::Health::with_max(10),
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
        .iter()
        .filter(|(pos, _)| (pos.x, pos.y) != (0, 0))
        .filter(|(_, tile)| matches!(tile.kind, TileKind::Grass))
        .collect::<Vec<_>>();

    let mut rng = rand::thread_rng();
    tiles.shuffle(&mut rng);

    let mut tiles = tiles.into_iter();
    let mut spawn = |count, model, size: [u8; 3]| {
        let mut components = Vec::with_capacity(count);

        for (coord, _) in tiles.by_ref().take(count) {
            let w = (16 - size[0]) as f32 * 0.5 * VOXEL_SIZE;
            let h = (16 - size[1]) as f32 * 0.5 * VOXEL_SIZE;
            let offset_x = rng.gen_range(-w, w);
            let offset_y = rng.gen_range(-h, h);

            let center = [coord.x as f32 + offset_x, coord.y as f32 + offset_y, 0.0];
            let collision = collision::AlignedBox::centered(
                [0.0, 0.0, VOXEL_SIZE * size[2] as f32 / 2.0].into(),
                Vector3::from(size).cast::<f32>().unwrap() * VOXEL_SIZE,
            );

            components.push((
                Position(center.into()),
                model,
                components::Collision {
                    bounds: collision,
                    ignored: None,
                },
                components::Breakable { durability: 1.0 },
                components::Health::with_max(5),
            ));
        }

        world.insert((tags::Static,), components);
    };

    spawn(TREES, Model::Tree, [14, 3, 30]);
    spawn(MUSHROOMS, Model::Mushroom, [9, 3, 7]);

    for (pos, tile) in map.iter() {
        if matches!(tile.kind, TileKind::Water) {
            spawn_invisible_wall(world, pos);
        }
    }

    let size = SIZE as f32;
    let floor = (
        Position([0.0; 3].into()),
        components::Collision {
            bounds: collision::AlignedBox::centered(
                [0.0, 0.0, -size].into(),
                [2.0 * size; 3].into(),
            ),
            ignored: None,
        },
    );

    world.insert((), Some(floor));
}

fn spawn_invisible_wall(world: &mut World, tile: TileCoord) {
    let position = Position(tile.to_world());
    let collision = components::Collision {
        bounds: collision::AlignedBox::centered([0.0, 0.0, 1.0].into(), [1.0, 1.0, 2.0].into()),
        ignored: None,
    };

    let components = (position, collision);

    world.insert((tags::Static,), Some(components));
}
