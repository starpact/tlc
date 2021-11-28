use std::lazy::SyncOnceCell;

use anyhow::Result;
use rayon::ThreadPoolBuilder;
use tokio::sync::oneshot;

pub async fn compute<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // `spawn` form `rayon`'s global thread pool will block when something like
    // `par_iter` is working as `rayon` uses `depth-first` strategy for highest
    // efficiency. So here we keep another thread pool for small tasks such as
    // decode a single frame or filter the green history of one point.
    static POOL: SyncOnceCell<rayon::ThreadPool> = SyncOnceCell::new();

    let (tx, rx) = oneshot::channel();
    POOL.get_or_try_init(|| ThreadPoolBuilder::new().num_threads(7).build())?
        .spawn(move || {
            let _ = tx.send(f());
        });

    Ok(rx.await?)
}
