use std::time::Instant;

use tracing::debug;

pub struct Timer {
    t0: Instant,
    succeeded: bool,
    description: String,
}

pub fn start<S: ToString>(description: S) -> Timer {
    let description = description.to_string();
    debug!("[TIMING] start {} ......", description);
    Timer {
        t0: Instant::now(),
        succeeded: false,
        description,
    }
}

impl Timer {
    pub fn finish(&mut self) {
        self.succeeded = true;
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if self.succeeded {
            debug!(
                "[TIMING] finish {} in {:?}",
                self.description,
                self.t0.elapsed()
            );
        }
    }
}
