mod video;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::RwLock;
use tokio::sync::oneshot;
use tracing::debug;

use video::FrameCache;

#[derive(Debug, Default)]
pub struct TLCData {
    /// Blocking version of `RWLock` is used here because:
    /// 1. `frame_cache` directly works with blocking operation such as: reading videos from file(IO)
    /// and demuxing(CPU intensive). So `lock/unlock` mainly happens in synchronous context.
    /// 2. There is no need to keep it locked across an `.await` point. Can refer to
    /// [this](https://docs.rs/tokio/1.13.0/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use).
    frame_cache: Arc<RwLock<FrameCache>>,
}

impl TLCData {
    pub async fn get_frame(&self, frame_index: usize) -> Result<usize> {
        let frame_cache = self.frame_cache.clone();
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || {
            let frame = loop {
                let frames = &frame_cache.read().frames;
                if frame_index < frames.len() {
                    break frames[frame_index];
                }
            };
            let _ = tx.send(frame);
        });

        Ok(rx.await?)
    }

    pub fn read_video<P: AsRef<Path>>(&self, path: P) {
        if !self.frame_cache.read().path_changed(&path) {
            return;
        }

        let path = path.as_ref().to_owned();
        let frame_cache = self.frame_cache.clone();

        tokio::task::spawn_blocking(move || {
            // `frame_cache` is reset before start reading from file.
            frame_cache.write().reset(&path);
            debug!("start reading video from {:?}", &path);

            for i in 0..1000 {
                // `RwLockWriteGuard` is intentionally holden all the way during
                // reading **each** frame to avoid busy loop when `get_frame`.
                let mut frame_cache = frame_cache.write();

                if frame_cache.path_changed(&path) {
                    // Video path has been changed, which means user changed the path
                    // before previous reading finishes. We should abort this reading at once.
                    // Another thread should be waiting for the lock to read from the latest path
                    // at this point.
                    return;
                }

                std::thread::sleep(Duration::from_secs(1));

                debug!("read frame {}", i);
                frame_cache.frames.push(i);
            }
        });
    }
}
