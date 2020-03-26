use cgmath::Point3;
use derive_more::{Deref, DerefMut};

pub mod player;

#[derive(Debug, Copy, Clone, Deref, DerefMut)]
pub struct Position(pub Point3<f32>);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Model {
    Rect,
    Circle,
    Tree,
    Player,
    Mushroom,
}

impl Model {
    pub const KINDS: &'static [Model] = &[
        Model::Rect,
        Model::Circle,
        Model::Tree,
        Model::Player,
        Model::Mushroom,
    ];
}
