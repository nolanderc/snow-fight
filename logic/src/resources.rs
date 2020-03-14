use std::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct TimeStep(f32);

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
