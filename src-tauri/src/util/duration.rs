use std::time::Instant;

use tracing::debug;

pub struct DurationGuard {
    t0: Instant,
    description: String,
}

pub fn measure<S: ToString>(description: S) -> DurationGuard {
    let description = description.to_string();
    debug!("[TIMING] start {} ......", description);
    DurationGuard {
        t0: Instant::now(),
        description,
    }
}

impl Drop for DurationGuard {
    fn drop(&mut self) {
        debug!(
            "[TIMING] finish {} in {:?}",
            self.description,
            self.t0.elapsed()
        );
    }
}
