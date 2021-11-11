pub mod video;

use std::path::Path;
use std::sync::Arc;

use ffmpeg_next as ffmpeg;

use anyhow::{bail, Context, Result};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::Array2;
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::oneshot;
use tracing::debug;

use super::cfg::G2DBuilder;
use video::{open_video, VideoCache};

#[derive(Debug, Default)]
pub struct TLCData {
    /// Blocking version of `RWLock` is used here because:
    /// 1. `frame_cache` directly works with blocking operation such as: reading videos from file(IO)
    /// and demuxing(CPU intensive). So `lock/unlock` mainly happens in synchronous context.
    /// 2. There is no need to keep it locked across an `.await` point. Can refer to
    /// [this](https://docs.rs/tokio/1.13.0/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use).
    video_cache: Arc<RwLock<VideoCache>>,

    g2d: Array2<u8>,
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

        let get_frame = move || -> Result<()> {
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
                    let (h, w) = decoder.shape();
                    let mut buf = Vec::with_capacity((h * w * 3) as usize);
                    let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                    jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

                    break base64::encode(buf);
                }
            };

            let _ = tx.send(frame);

            Ok(())
        };

        // `get_frame` is regarded as synchronous blocking because:
        // 1. When the targeted frame is not loaded yet, it will block on the `RWLock`.
        // 2. Decoding will take some time(10~20ms) even for a single frame.
        // So this task should be executed in `tokio::task::spawn_blocking` or `rayon::spawn`,
        // here we must use `rayon::spawn` because the `thread-local` decoder is designed
        // to be kept in thread from rayon thread pool.
        rayon::spawn(move || get_frame().unwrap_or_default());

        let frame = rx
            .await
            .with_context(|| format!("frame_index({}) out of range", frame_index))?;

        Ok(frame)
    }

    pub async fn read_video<P: AsRef<Path>>(&self, path: P) -> Result<VideoInfo> {
        let path = path.as_ref().to_owned();
        let video_cache = self.video_cache.clone();
        let (tx, rx) = oneshot::channel();

        let read_video = move || -> Result<()> {
            let t0 = std::time::Instant::now();

            let mut input = ffmpeg::format::input(&path)?;
            let (frame_rate, total_frames, video_ctx, mut packet_iter) = open_video(&mut input)?;

            // `packet_cache` is reset before start reading from file.
            let (h, w) = video_cache
                .write()
                .reset(&path, video_ctx, total_frames)
                .decoder_cache
                .get_decoder()?
                .shape();

            let _ = tx.send(VideoInfo {
                frame_rate,
                total_frames,
                shape: (h, w),
            });

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
        };

        rayon::spawn(move || read_video().unwrap_or_default());

        Ok(rx.await?)
    }

    pub async fn build_g2d(&mut self, g2d_builder: G2DBuilder) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let video_cache = self.video_cache.clone();

        rayon::spawn(move || match video_cache.read().build_g2d(g2d_builder) {
            Ok(g2d) => tx.send(Ok(g2d)).unwrap_or_default(),
            Err(e) => tx.send(Err(e)).unwrap_or_default(),
        });

        self.g2d = rx.await??;

        Ok(())
    }
}
