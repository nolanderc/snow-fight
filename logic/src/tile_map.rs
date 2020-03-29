use cgmath::{Point2, Point3, Vector3};
use derive_more::{Deref, DerefMut, From};
use legion::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, From, Deref, DerefMut)]
pub struct TileCoord(pub Point2<i32>);

pub struct TileMap {
    tiles: HashMap<TileCoord, Tile>,
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub slot: Option<Placed>,
    pub kind: TileKind,
}

#[derive(Debug, Copy, Clone)]
pub enum TileKind {
    Water,
    Grass,
    Sand,
}

#[derive(Debug, Clone)]
pub struct Placed {
    pub entity: Entity,
    pub durability: f32,
}

impl Default for TileMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TileMap {
    pub fn new() -> Self {
        TileMap {
            tiles: HashMap::new(),
        }
    }

    pub fn insert(&mut self, position: TileCoord, tile: Tile) {
        self.tiles.insert(position, tile);
    }

    pub fn get(&self, position: TileCoord) -> Option<&Tile> {
        self.tiles.get(&position)
    }

    pub fn get_mut(&mut self, position: TileCoord) -> Option<&mut Tile> {
        self.tiles.get_mut(&position)
    }

    pub fn iter(&self) -> impl Iterator<Item = (TileCoord, &Tile)> {
        self.tiles.iter().map(|(pos, tile)| (*pos, tile))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (TileCoord, &mut Tile)> {
        self.tiles.iter_mut().map(|(pos, tile)| (*pos, tile))
    }
}

impl Default for Tile {
    fn default() -> Self {
        Tile {
            slot: None,
            kind: TileKind::Water,
        }
    }
}

impl Tile {
    pub fn with_kind(self, kind: TileKind) -> Self {
        Tile { kind, ..self }
    }
}

impl From<[i32; 2]> for TileCoord {
    fn from(point: [i32; 2]) -> Self {
        TileCoord(point.into())
    }
}

impl TileCoord {
    pub fn from_world(world: Point3<f32>) -> TileCoord {
        let x = world.x.round() as i32;
        let y = world.y.round() as i32;
        [x, y].into()
    }

    pub fn from_ray(origin: Point3<f32>, direction: Vector3<f32>) -> TileCoord {
        let intersection_time = -origin.z / direction.z;
        let intersection = origin + intersection_time * direction;
        TileCoord::from_world(intersection)
    }
}
