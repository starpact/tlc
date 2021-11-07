pub mod video;

use std::path::Path;
use std::sync::Arc;

use ffmpeg_next as ffmpeg;

use anyhow::{bail, Context, Result};
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::oneshot;
use tracing::debug;

use video::{open_video, VideoCache};

#[derive(Debug, Default)]
pub struct TLCData {
    /// Blocking version of `RWLock` is used here because:
    /// 1. `frame_cache` directly works with blocking operation such as: reading videos from file(IO)
    /// and demuxing(CPU intensive). So `lock/unlock` mainly happens in synchronous context.
    /// 2. There is no need to keep it locked across an `.await` point. Can refer to
    /// [this](https://docs.rs/tokio/1.13.0/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use).
    video_cache: Arc<RwLock<VideoCache>>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct VideoInfo {
    pub frame_rate: usize,
    pub total_frames: usize,
    pub shape: (u32, u32),
}

impl TLCData {
    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        let video_cache = self.video_cache.clone();
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let frame = loop {
                let vc = video_cache.read();
                if frame_index >= vc.total {
                    // This is an invalid `frame_index` from frontend and will never get the frame.
                    // So directly abort current thread. Then `rx` will be dropped and `tx` outside
                    // will stop pending(returning an `RecvError`).
                    bail!("never read");
                }
                if let Some(packet) = vc.packets.get(frame_index) {
                    let decoder = vc.decoder_cache.get_decoder()?;
                    let buf = decoder.decode(packet)?.data(0).to_vec();
                    break String::from_utf8(buf)?;
                }
            };

            let _ = tx.send(frame);

            Ok(())
        });

        let frame = rx
            .await
            .with_context(|| format!("frame_index({}) out of range", frame_index))?;

        Ok(frame)
    }

    pub async fn read_video<P: AsRef<Path>>(&self, path: P) -> Result<VideoInfo> {
        let path = path.as_ref().to_owned();
        let video_cache = self.video_cache.clone();
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let t0 = std::time::Instant::now();

            let mut input = ffmpeg::format::input(&path)?;
            let (frame_rate, total_frames, video_ctx, mut packet_iter) = open_video(&mut input)?;

            let decoder = video_ctx.clone().decoder().video()?;
            let (h, w) = (decoder.height(), decoder.width());

            let _ = tx.send(VideoInfo {
                frame_rate,
                total_frames,
                shape: (h, w),
            });

            // `packet_cache` is reset before start reading from file.
            video_cache.write().reset(&path, video_ctx, total_frames);

            debug!("start reading video from {:?} ......", &path);
            let mut cnt = 0;
            loop {
                // `RwLockWriteGuard` is intentionally holden all the way during
                // reading **each** frame to avoid busy loop within `get_frame`.
                let mut vc = video_cache.write();
                if let Some(packet) = packet_iter.next() {
                    if vc.path_changed(&path) {
                        // Video path has been changed, which means user changed the path before
                        // previous reading finishes. So we should abort this reading at once.
                        // Other threads should be waiting for the lock to read from the latest path
                        // at this point.
                        bail!("never read");
                    }
                    vc.packets.push(packet);
                    cnt += 1;
                } else {
                    break;
                }
            }
            debug_assert!(cnt == total_frames);
            debug!("[TIMING] read all packets in {:?}", t0.elapsed());

            Ok(())
        });

        Ok(rx.await?)
    }

    pub async fn decode_all(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let video_cache = self.video_cache.clone();

        rayon::spawn(move || match video_cache.read().decode_all() {
            Ok(_) => tx.send(Ok(())).unwrap_or_default(),
            Err(e) => tx.send(Err(e)).unwrap_or_default(),
        });

        Ok(rx.await??)
    }
}
