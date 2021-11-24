mod daq;
mod filter;
mod interp;
mod plot;
mod solve;
mod video;

use std::path::Path;
use std::sync::Arc;

use ffmpeg_next as ffmpeg;

use anyhow::{anyhow, bail, Result};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array2};
use parking_lot::RwLock;
use tokio::sync::oneshot;
use tracing::debug;

use super::cfg::{DAQMeta, G2Param, VideoMeta};
use crate::util::{blocking, timing};
pub use filter::{filter, FilterMethod};
pub use interp::InterpMethod;
pub use solve::IterationMethod;
use video::{open_video, VideoCache};

#[derive(Default)]
pub struct TLCData {
    /// Blocking version of `RWLock` is used here because:
    /// 1. `frame_cache` directly works with blocking operation such as: reading videos from file(IO)
    /// and demuxing(CPU intensive). So `lock/unlock` mainly happens in synchronous context.
    /// 2. There is no need to keep it locked across an `.await` point. Can refer to
    /// [this](https://docs.rs/tokio/1.13.0/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use).
    video_cache: Arc<RwLock<VideoCache>>,
    /// DAQ data.
    daq: Array2<f64>,
    /// Green 2D matrix(frame_num, pix_num).
    ///
    /// frame 1: |X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ...... XnYn|
    ///
    /// frame 2: |X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ...... XnYn|
    ///
    /// ......
    pub g2: Arc<Array2<u8>>,

    pub filtered_g2: Arc<Array2<u8>>,
}

impl TLCData {
    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        let video_cache = self.video_cache.clone();

        // `get_frame` is regarded as synchronous blocking because:
        // 1. When the targeted frame is not loaded yet, it will block on the `RWLock`.
        // 2. Decoding will take some time(10~20ms) even for a single frame.
        // So this task should be executed in `tokio::task::spawn_blocking` or `rayon::spawn`,
        // here we must use `rayon::spawn` because the `thread-local` decoder is designed
        // to be kept in thread from rayon thread pool.
        blocking::compute(move || -> Result<String> {
            let frame = loop {
                let vc = video_cache.read();
                if frame_index >= vc.total_frames {
                    // This is an invalid `frame_index` from frontend and will never get the frame.
                    // So directly abort current thread. Then `rx` will be dropped and `tx` outside
                    // will stop pending(returning an `RecvError`).
                    bail!("frame_index({}) out of range", frame_index);
                }
                if let Some(packet) = vc.packets.get(frame_index) {
                    let mut decoder = vc.decoder_cache.get_decoder()?;
                    let (h, w) = decoder.shape();
                    // This pre-alloc size is just an empirical estimate.
                    let mut buf = Vec::with_capacity((h * w) as usize);
                    let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                    jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

                    break base64::encode(buf);
                }
            };

            Ok(frame)
        })
        .await?
    }

    pub async fn read_video<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMeta> {
        let path = video_path.as_ref().to_owned();
        let video_cache = self.video_cache.clone();
        let (tx, rx) = oneshot::channel();

        let worker = tokio::task::spawn_blocking(move || -> Result<()> {
            let _timing = timing::start("reading video");
            debug!("{:?}", &path);

            let mut input = ffmpeg::format::input(&path)?;
            let (frame_rate, total_frames, video_ctx, mut packet_iter) = open_video(&mut input)?;
            let decoder = video_ctx.clone().decoder().video()?;

            // Outer function can return at this point.
            let _ = tx.send(VideoMeta {
                path: path.clone(),
                frame_rate,
                total_frames,
                shape: (decoder.height(), decoder.width()),
            });

            // `packet_cache` is reset before start reading from file.
            video_cache.write().reset(&path, video_ctx, total_frames);

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
                        return Ok(());
                    }
                    vc.packets.push(packet);
                    cnt += 1;
                } else {
                    break;
                }
            }

            debug_assert!(cnt == total_frames);
            debug!("total_frames: {}", total_frames);

            Ok(())
        });

        match rx.await {
            Ok(t) => Ok(t),
            Err(_) => Err(worker
                .await?
                .map_err(|e| anyhow!("failed to read video from {:?}: {}", video_path.as_ref(), e))
                .unwrap_err()),
        }
    }

    pub async fn read_daq<P: AsRef<Path>>(&mut self, daq_path: P) -> Result<DAQMeta> {
        let path = daq_path.as_ref().to_owned();
        let daq = tokio::task::spawn_blocking(move || daq::read_daq(&path)).await??;
        let total_rows = daq.dim().0;
        self.daq = daq;

        Ok(DAQMeta {
            path: daq_path.as_ref().to_owned(),
            total_rows,
        })
    }

    pub fn get_daq(&self) -> ArcArray2<f64> {
        self.daq.to_shared()
    }

    pub async fn build_g2(&mut self, g2_parameter: G2Param) -> Result<Arc<Array2<u8>>> {
        let video_cache = self.video_cache.clone();
        let g2 = blocking::compute(move || loop {
            let vc = video_cache.read();
            if vc.packets.len() == vc.total_frames {
                break vc.build_g2(g2_parameter);
            }
        })
        .await??;
        self.g2 = Arc::new(g2);
        self.filtered_g2 = self.g2.clone();

        Ok(self.g2.clone())
    }
}
