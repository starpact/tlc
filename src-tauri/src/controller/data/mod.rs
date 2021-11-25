mod daq;
mod filter;
mod interp;
mod plot;
mod solve;
mod video;

use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Result};
use ffmpeg_next as ffmpeg;
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array1, Array2};
use parking_lot::{Mutex, RwLock};
use tokio::sync::oneshot;
use tracing::debug;

use super::cfg::{DAQMeta, G2Param, VideoMeta};
use crate::util::timing;
pub use filter::{filter, FilterMethod};
pub use interp::InterpMethod;
pub use solve::IterationMethod;
use video::{open_video, VideoCache};

#[derive(Default)]
pub struct TLCData {
    video_cache: Arc<RwLock<VideoCache>>,
    /// Green related data.
    /// Blocking version of `RWLock` is used here because:
    /// 1. `frame_cache` directly works with blocking operation such as: reading videos from file(IO)
    /// and demuxing(CPU intensive). So `lock/unlock` mainly happens in synchronous context.
    /// 2. There is no need to keep it locked across an `.await` point. Can refer to
    /// [this](https://docs.rs/tokio/1.13.0/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use).
    gr: Arc<Mutex<GreenRelated>>,
    /// Temperature related data.
    tr: Mutex<TemperatureRelated>,
}

#[derive(Default)]
struct GreenRelated {
    /// Green 2D matrix(frame_num, pix_num).
    ///
    /// frame 1: |X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ...... XnYn|
    ///
    /// frame 2: |X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ...... XnYn|
    ///
    /// ......
    g2: Arc<Array2<u8>>,
    /// If we do not filter `g2`, then `filtered_g2` and `g2` can shared the same data
    /// so `Arc` is used here to achieve this "copy-on-write" logic.
    filtered_g2: Arc<Array2<u8>>,
    /// Frame index of green value peak point.
    #[allow(dead_code)]
    peak_frames: Array1<usize>,
}

#[derive(Default)]
struct TemperatureRelated {
    #[allow(dead_code)]
    daq: Array2<f64>,
    #[allow(dead_code)]
    t2d: Option<Array2<f64>>,
}

impl TLCData {
    pub fn reset(&self) {
        *self.gr.lock() = GreenRelated::default();
        *self.tr.lock() = TemperatureRelated::default();
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        let video_cache = self.video_cache.clone();

        // `get_frame` is regarded as synchronous blocking because:
        // 1. When the targeted frame is not loaded yet, it will block on the `RWLock`.
        // 2. Decoding will take some time(10~20ms) even for a single frame.
        // So this task should be executed in `tokio::task::spawn_blocking` or `rayon::spawn`,
        // here we must use `rayon::spawn` because the `thread-local` decoder is designed
        // to be kept in thread from rayon thread pool.
        crate::util::blocking::compute(move || loop {
            let vc = video_cache.read();
            vc.worth_waiting(frame_index)?;
            if let Some(packet) = vc.packets.get(frame_index) {
                let mut decoder = vc.get_decoder()?;
                let (h, w) = decoder.shape();
                // This pre-alloc size is just an empirical estimate.
                let mut buf = Vec::with_capacity((h * w) as usize);
                let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

                break Ok(base64::encode(buf));
            }
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
                // reading *each* frame to avoid busy loop within `get_frame`.
                let mut vc = video_cache.write();
                if let Some(packet) = packet_iter.next() {
                    if vc.target_changed(&path) {
                        // Video path has been changed, which means user changed the path before
                        // previous reading finishes. So we should abort this reading at once.
                        // Other threads should be waiting for the lock to read from the latest path
                        // at this point.
                        return Ok(());
                    }
                    vc.packets.push(packet);
                    cnt += 1;
                } else {
                    vc.mark_finished();
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

    pub async fn read_daq<P: AsRef<Path>>(&self, daq_path: P) -> Result<DAQMeta> {
        let path = daq_path.as_ref().to_owned();
        let daq = tokio::task::spawn_blocking(move || daq::read_daq(&path)).await??;
        let total_rows = daq.dim().0;
        self.tr.lock().daq = daq;

        Ok(DAQMeta {
            path: daq_path.as_ref().to_owned(),
            total_rows,
        })
    }

    pub fn get_daq(&self) -> ArcArray2<f64> {
        self.tr.lock().daq.to_shared()
    }

    pub async fn build_g2(&self, g2_param: G2Param) -> &Self {
        let video_cache = self.video_cache.clone();
        let gr = self.gr.clone();
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut gr = gr.lock();
            let _ = tx.send(());

            let g2 = loop {
                let vc = &video_cache.read();
                if vc.finished() {
                    break vc.build_g2(g2_param)?;
                }
            };

            gr.g2 = Arc::new(g2);
            gr.filtered_g2 = gr.g2.clone();

            Ok(())
        });

        // We need to make sure the lock has been acquired so that
        // `filter` will not start until `build_g2` finished.
        let _ = rx.await;

        self
    }

    pub fn filter(&self, filter_method: FilterMethod) {
        let gr = self.gr.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut gr = gr.lock();

            let filtered_g2 = filter(gr.g2.clone(), filter_method)?;
            gr.filtered_g2 = filtered_g2;

            Ok(())
        });
    }
}
