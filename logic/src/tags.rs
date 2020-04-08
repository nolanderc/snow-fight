#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Player;

/// An entity that will never move/change.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Static;

/// An entity that responds to collisions.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Moveable;
