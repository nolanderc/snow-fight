use protocol::snapshot::EntityId;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use std::sync::Arc;

#[derive(Debug, Copy, Clone)]
pub struct TimeStep(f32);

#[derive(Debug, Clone)]
pub struct EntityAllocator {
    next: Arc<AtomicU32>,
}

#[derive(Debug, Clone, Default)]
pub struct DeadEntities {
    pub entities: Vec<EntityId>,
}

impl Default for TimeStep {
    fn default() -> Self {
        TimeStep(0.0)
    }
}

impl TimeStep {
    pub(crate) fn from_duration(duration: Duration) -> Self {
        TimeStep(duration.as_secs_f32())
    }

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
    pub fn allocate(&self) -> EntityId {
        EntityId(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

