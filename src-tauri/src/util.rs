pub mod log {
    use std::sync::Once;

    static START: Once = Once::new();
    pub fn init() {
        START.call_once(|| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .pretty()
                .init();
        });
    }
}

pub mod timing {
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
}
