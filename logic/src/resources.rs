use protocol::snapshot::EntityId;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use std::sync::Arc;

/// The amount of time stepped through in this tick.
#[derive(Debug, Copy, Clone)]
pub struct TimeStep(f32);

/// Manages the creation of new `EntityId`s.
#[derive(Debug, Clone)]
pub struct EntityAllocator {
    /// The id of the next entity that may be created.
    next: Arc<AtomicU32>,
}

/// A list of all entities that have been destroyed.
#[derive(Debug, Clone, Default)]
pub struct DeadEntities {
    /// A list of all entities that have been destroyed.
    pub entities: Vec<EntityId>,
}

impl Default for TimeStep {
    fn default() -> Self {
        TimeStep(0.0)
    }
}

impl TimeStep {
    /// Create a time step with the given duration
    pub(crate) fn from_duration(duration: Duration) -> Self {
        TimeStep(duration.as_secs_f32())
    }

    /// Get the number of seconds represented by this time step. 
    pub fn secs_f32(self) -> f32 {
        self.0
    }
}

impl Default for EntityAllocator {
    fn default() -> Self {
        EntityAllocator { next: Arc::new(AtomicU32::new(1)) }
    }
}

impl EntityAllocator {
    /// Get a new `EntityId`
    pub fn allocate(&self) -> EntityId {
        EntityId(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

