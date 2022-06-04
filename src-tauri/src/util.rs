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

pub mod blocking {
    use std::lazy::SyncOnceCell;

    use anyhow::Result;
    use rayon::ThreadPoolBuilder;
    use tokio::sync::oneshot;

    pub const NUM_THREADS: usize = 4;

    pub async fn compute<F, T>(f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        // `spawn` form `rayon`'s global thread pool will block when something like
        // `par_iter` is working as `rayon` uses `depth-first` strategy for highest
        // efficiency. So here we keep another thread pool for small tasks such as
        // decode a single frame or filter the green history of one point.
        static POOL: SyncOnceCell<rayon::ThreadPool> = SyncOnceCell::new();

        let (tx, rx) = oneshot::channel();
        POOL.get_or_try_init(|| ThreadPoolBuilder::new().num_threads(NUM_THREADS).build())?
            .spawn(move || {
                let _ = tx.send(f());
            });

        Ok(rx.await?)
    }
}
