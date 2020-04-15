pub extern crate legion;

pub mod components;
pub mod events;
pub mod resources;
pub mod snapshot;
pub mod systems;
pub mod tags;

pub mod collision;
pub mod tile_map;

mod templates;

use legion::entity::Entity;
use legion::schedule::{Builder as ScheduleBuilder, Schedulable, Schedule};
use legion::world::World;

use cgmath::Vector3;

use rand::prelude::*;

use std::time::{Duration, Instant};

use protocol::PlayerId;

use crate::components::{Model, Position};
use crate::resources::{DeadEntities, EntityAllocator, TimeStep};
use crate::tags::Player;
use crate::tile_map::{Tile, TileKind, TileMap};

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

pub enum WorldKind {
    Plain,
    WithObjects,
}

pub enum SystemSet {
    NonDestructive,
    Everything,
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
pub fn create_world(kind: WorldKind) -> World {
    let mut world = World::new();

    world.resources.insert(TimeStep::default());
    world.resources.insert(DeadEntities::default());

    let mut map = island_map(SIZE as i32);
    spawn_invisible_walls(&mut world, &map);
    spawn_floor(&mut world);

    if matches!(kind, WorldKind::WithObjects) {
        spawn_objects(&mut world, &mut map);
    }

    world.resources.insert(map);
    world.defrag(None);

    world
}

/// Schedule all game logic systems.
pub fn add_systems(builder: ScheduleBuilder, set: SystemSet) -> ScheduleBuilder {
    let base = builder
        .add_system(systems::movement::system())
        .add_system(systems::acceleration::system())
        .add_system(systems::tile_interaction::system())
        .add_system(systems::collision::continuous_system())
        .add_system(systems::collision::discrete_system());

    match set {
        SystemSet::NonDestructive => base,
        SystemSet::Everything => base.add_system(systems::attack::system()),
    }
}

pub fn add_player(world: &mut World, owner: PlayerId) -> Entity {
    let id = world
        .resources
        .get_or_insert_with(EntityAllocator::default)
        .unwrap()
        .allocate();

    let mut rng = thread_rng();

    let tags = (Player,);
    let template = templates::Player {
        id,
        position: Position([rng.gen_range(-0.5, 0.5), rng.gen_range(-0.5, 0.5), 0.0].into()),
        model: Model::Player,
        movement: components::Movement::default(),
        interaction: components::WorldInteraction::default(),
        collision: templates::collision(Model::Player),
        health: components::Health::with_max(3),
        owner: components::Owner(owner),
    };

    let entity = world.insert(tags, Some(()))[0];
    template.insert(world, entity);
    entity
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

    let entity_allocator = world
        .resources
        .get_or_insert_with(EntityAllocator::default)
        .unwrap()
        .clone();

    let mut tiles = tiles.into_iter();
    let mut spawn = |count, model| {
        for (coord, _) in tiles.by_ref().take(count) {
            let entity = world.insert((tags::Static,), Some(()))[0];
            let offset = Vector3::new(rng.gen_range(-0.5, 0.5), rng.gen_range(-0.5, 0.5), 0.0);
            let template = templates::Object {
                id: entity_allocator.allocate(),
                position: Position(coord.to_world() + offset),
                model,
                collision: templates::collision(model),
                health: components::Health::with_max(3),
                breakable: Some(components::Breakable::default()),
            };
            template.insert(world, entity);
        }
    };

    spawn(TREES, Model::Tree);
    spawn(MUSHROOMS, Model::Mushroom);
}

fn spawn_invisible_walls(world: &mut World, map: &TileMap) {
    let components = map
        .iter()
        .filter(|(_, tile)| matches!(tile.kind, TileKind::Water))
        .map(|(pos, _)| {
            (
                Position(pos.to_world()),
                components::Collision {
                    bounds: collision::AlignedBox::centered(
                        [0.0, 0.0, 1.0].into(),
                        [1.0, 1.0, 2.0].into(),
                    ),
                    ignored: None,
                },
            )
        });

    world.insert((tags::Static,), components);
}

fn spawn_floor(world: &mut World) {
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
