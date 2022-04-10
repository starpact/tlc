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
}

pub mod blocking {
    use std::lazy::SyncOnceCell;

    use anyhow::Result;
    use rayon::ThreadPoolBuilder;
    use tokio::sync::oneshot;

    pub const NUM_THREAD: usize = 4;

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
        POOL.get_or_try_init(|| ThreadPoolBuilder::new().num_threads(NUM_THREAD).build())?
            .spawn(move || {
                let _ = tx.send(f());
            });

        Ok(rx.await?)
    }
}
