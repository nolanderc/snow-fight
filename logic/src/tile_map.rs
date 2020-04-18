use cgmath::{Point2, Point3, Vector3};
use derive_more::{Deref, DerefMut, From};
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, From, Deref, DerefMut)]
pub struct TileCoord(pub Point2<i32>);

pub struct TileMap {
    tiles: HashMap<TileCoord, Tile>,
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub slot: Option<Slot>,
    pub kind: TileKind,
}

#[derive(Debug, Copy, Clone)]
pub enum TileKind {
    Water,
    Grass,
    Sand,
}

#[derive(Debug, Clone)]
pub struct Slot {}

impl Default for TileMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TileMap {
    /// Create an empty map.
    pub fn new() -> Self {
        TileMap {
            tiles: HashMap::new(),
        }
    }

    /// Crate a new world in the shape of an island with radius size.
    pub fn island(size: i32) -> TileMap {
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

    /// Insert a new tile at the given position
    pub fn insert(&mut self, position: TileCoord, tile: Tile) {
        self.tiles.insert(position, tile);
    }

    /// Get the tile at the specified position.
    pub fn get(&self, position: TileCoord) -> Option<&Tile> {
        self.tiles.get(&position)
    }

    /// Get the tile at the specified position.
    pub fn get_mut(&mut self, position: TileCoord) -> Option<&mut Tile> {
        self.tiles.get_mut(&position)
    }

    /// Iterator through every tile
    pub fn iter(&self) -> impl Iterator<Item = (TileCoord, &Tile)> {
        self.tiles.iter().map(|(pos, tile)| (*pos, tile))
    }

    /// Iterator through every tile
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
    /// Create a specific kind of tile.
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
    /// Get the world coordinates of a tile.
    pub fn to_world(self) -> Point3<f32> {
        Point3 {
            x: self.x as f32,
            y: self.y as f32,
            z: 0.0,
        }
    }

    /// Get the tile at the specified world coordinates.
    pub fn from_world(world: Point3<f32>) -> TileCoord {
        let x = world.x.round() as i32;
        let y = world.y.round() as i32;
        [x, y].into()
    }

    /// Get the tile that intersects the given ray.
    pub fn from_ray(origin: Point3<f32>, direction: Vector3<f32>) -> TileCoord {
        let intersection_time = -origin.z / direction.z;
        let intersection = origin + intersection_time * direction;
        TileCoord::from_world(intersection)
    }
}
