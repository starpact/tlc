use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{bail, Result};
use rayon::{ThreadPool, ThreadPoolBuilder};
use tokio::sync::{Semaphore, SemaphorePermit};

const DEFAULT_NUM_THREADS: usize = 4;

/// When user drags the progress bar quickly, the decoding can not keep up
/// and there will be a significant lag. Actually, we do not have to decode
/// every frames, and the key is how to give up decoding some frames properly.
/// The naive solution to avoid too much backlog is maintaining the number of
/// pending tasks and directly abort current decoding if it already exceeds the
/// limit. But FIFO is not perfect for this use case because it's better to give
/// priority to newer frames, e.g. we should at least guarantee decoding the frame
/// where the progress bar **stops**.
/// An asynchronous semaphore is used to ensure it will be not synchronously
/// blocked at `spawn` when there is no idle worker threads. The number of
/// permits must be the same as the number of threads of the pool.
pub struct SpawnHandle {
    /// Thread pool for decoding single frame, which makes use of thread-local decoders.
    thread_pool: ThreadPool,

    /// Track the number of idle workers in the thread pool.
    semaphore: Semaphore,

    /// See [get_spawner].
    last_target_frame_index: AtomicUsize,
}

/// `Spawner` will return its permit to semaphore on drop.
pub struct Spawner<'a> {
    thread_pool: &'a ThreadPool,
    _permit: SemaphorePermit<'a>,
}

impl<'a> Spawner<'a> {
    /// `spawn` will never block because a semaphore permit is holden whilch ensure
    /// that there is idle thread in the pool.
    pub fn spawn<OP>(&self, op: OP)
    where
        OP: FnOnce() + Send + 'static,
    {
        self.thread_pool.spawn(op)
    }
}

impl SpawnHandle {
    pub fn new(num_threads: usize) -> Self {
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("Failed to init rayon thread pool");
        let semaphore = Semaphore::new(num_threads);
        let last_target_frame_index = AtomicUsize::new(0);

        Self {
            thread_pool,
            semaphore,
            last_target_frame_index,
        }
    }

    pub async fn spawner(&self, frame_index: usize) -> Result<Spawner> {
        if let Ok(_permit) = self.semaphore.try_acquire() {
            return Ok(Spawner {
                thread_pool: &self.thread_pool,
                _permit,
            });
        }

        self.last_target_frame_index
            .store(frame_index, Ordering::Relaxed);
        let _permit = self.semaphore.acquire().await?;

        // While awaiting for permit, the `last_target_frame_index` may be modified by subsequent
        // requests. So we need to check if this is still the last one, otherwise we should abort
        // it to make sure the last one is processed.
        if self.last_target_frame_index.load(Ordering::Relaxed) != frame_index {
            bail!("no idle worker thread");
        }

        Ok(Spawner {
            thread_pool: &self.thread_pool,
            _permit,
        })
    }
}

impl Default for SpawnHandle {
    fn default() -> Self {
        Self::new(DEFAULT_NUM_THREADS)
    }
}
