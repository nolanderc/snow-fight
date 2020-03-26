use cgmath::Point2;
use legion::prelude::*;
use std::collections::HashMap;

pub type TileCoord = Point2<i32>;

pub struct TileMap {
    tiles: HashMap<TileCoord, Tile>,
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub entity: Option<Entity>,
    pub kind: TileKind,
}

#[derive(Debug, Copy, Clone)]
pub enum TileKind {
    Water,
    Grass,
    Sand,
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
            entity: None,
            kind: TileKind::Water,
        }
    }
}

impl Tile {
    pub fn with_kind(self, kind: TileKind) -> Self {
        Tile { kind, ..self }
    }
}
