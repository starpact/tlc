use std::time::Instant;

use tracing::debug;

pub struct TimingGuard {
    t0: Instant,
    description: String,
}

pub fn start<S: ToString>(description: S) -> TimingGuard {
    let description = description.to_string();
    debug!("[TIMING] start {} ......", description);
    TimingGuard {
        t0: Instant::now(),
        description,
    }
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        debug!(
            "[TIMING] finish {} in {:?}",
            self.description,
            self.t0.elapsed(),
        );
    }
}
