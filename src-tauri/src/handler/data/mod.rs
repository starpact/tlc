pub mod video;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use parking_lot::RwLock;
use tokio::sync::oneshot;
use tracing::debug;

use video::{get_video_info, FrameCache, VideoInfo};

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
                let FrameCache {
                    ref frames,
                    total_frames,
                    ..
                } = *frame_cache.read();

                if frame_index < frames.len() {
                    break frames[frame_index];
                }
                if frame_index >= total_frames {
                    // This is an invalid `frame_index` from frontend and will never get the frame.
                    // So directly abort current thread. Then `rx` will be dropped and `tx` outside
                    // will stop pending(returning an `RecvError`).
                    return;
                }
            };
            let _ = tx.send(frame);
        });

        let frame = rx
            .await
            .with_context(|| format!("frame_index({}) out of range", frame_index))?;

        Ok(frame)
    }

    pub async fn read_video<P: AsRef<Path>>(&self, path: P) -> Result<VideoInfo> {
        let path = path.as_ref().to_owned();
        let frame_cache = self.frame_cache.clone();
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || {
            let video_info = match get_video_info(&path) {
                Ok(video_info) => {
                    let _ = tx.send(Ok(video_info));
                    video_info
                }
                Err(e) => {
                    let _ = tx.send(Err(anyhow!("failed to get video info: {}", e)));
                    return;
                }
            };

            // `frame_cache` is reset before start reading from file.
            frame_cache.write().reset(&path, video_info.total_frames);
            debug!("start reading video from {:?}", &path);

            for i in 0..1000 {
                // `RwLockWriteGuard` is intentionally holden all the way during
                // reading **each** frame to avoid busy loop within `get_frame`.
                let mut frame_cache = frame_cache.write();

                if frame_cache.path_changed(&path) {
                    // Video path has been changed, which means user changed the path before
                    // previous reading finishes. So we should abort this reading at once.
                    // Another thread should be waiting for the lock to read from the latest path
                    // at this point.
                    return;
                }

                // TODO:mock slow reading.
                std::thread::sleep(Duration::from_secs(1));

                debug!("read frame {}", i);
                frame_cache.frames.push(i);
            }
        });

        Ok(rx.await??)
    }
}
