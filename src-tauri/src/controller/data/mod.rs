mod daq;
mod filter;
mod interp;
mod plot;
mod solve;
mod video;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Result};
use arc_swap::ArcSwap;
use ffmpeg_next as ffmpeg;
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array1, Array2};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::{oneshot, Semaphore};
use tracing::debug;

use super::cfg::{DAQMeta, G2Param, VideoMeta};
use crate::util::{blocking, timing};
pub use filter::{filter, filter_single_point, FilterMethod};
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
    tr: Arc<Mutex<TemperatureRelated>>,

    /// Build g2 progress watcher.
    bpw: Arc<ProgressWatcher>,

    /// Build g2 progress watcher.
    fpw: Arc<ProgressWatcher>,
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
    g2: Option<ArcArray2<u8>>,

    /// If we do not filter `g2`, then `filtered_g2` and `g2` can shared the same data
    /// so `Arc` is used here to achieve this "copy-on-write" logic.
    filtered_g2: Option<ArcArray2<u8>>,

    /// Frame index of green value peak point.
    #[allow(dead_code)]
    peak_frames: Option<Array1<usize>>,
}

#[derive(Default)]
struct TemperatureRelated {
    daq: ArcArray2<f64>,

    #[allow(dead_code)]
    t2d: Option<Array2<f64>>,
}

#[derive(Default)]
struct ProgressWatcher {
    /// We do need the interior mutability for task cancellation.
    /// * current progress: lower 32 bits
    /// * total progress: higher 32 bits
    inner: ArcSwap<AtomicI64>,
}

impl ProgressWatcher {
    fn get_progress(&self) -> Option<CalProgress> {
        let inner = self.inner.load().load(Ordering::Relaxed);
        if inner <= 0 {
            return None;
        }

        let current = (inner & (u32::MAX as i64)) as u32;
        let total = (inner >> 32) as u32;

        Some(CalProgress { current, total })
    }
}

#[derive(Serialize)]
pub struct CalProgress {
    current: u32,
    total: u32,
}

impl TLCData {
    pub async fn reset(&self) {
        let gr = self.gr.clone();
        let tr = self.tr.clone();
        let vc = self.video_cache.clone();

        let _ = tokio::task::spawn_blocking(move || {
            *gr.lock() = GreenRelated::default();
            *tr.lock() = TemperatureRelated::default();
            *vc.write() = VideoCache::default();
        })
        .await;
    }

    pub async fn read_frame(&self, frame_index: usize) -> Result<String> {
        static PENDING_COUNTER: Semaphore = Semaphore::const_new(3);
        let _counter = PENDING_COUNTER
            .try_acquire()
            .map_err(|e| anyhow!("busy: {}", e))?;

        let video_cache = self.video_cache.clone();

        // `get_frame` is regarded as synchronous blocking because:
        // 1. When the targeted frame is not loaded yet, it will block on the `RWLock`.
        // 2. Decoding will take some time(10~20ms) even for a single frame.
        // So this task should be executed in `tokio::task::spawn_blocking` or `rayon::spawn`,
        // here we must use `rayon::spawn` because the `thread-local` decoder is designed
        // to be kept in thread from rayon thread pool.
        blocking::compute(move || loop {
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
            video_cache.write().init(&path, video_ctx, total_frames);

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
        self.tr.lock().daq = daq.into_shared();

        Ok(DAQMeta {
            path: daq_path.as_ref().to_owned(),
            total_rows,
        })
    }

    pub fn get_daq(&self) -> ArcArray2<f64> {
        self.tr.lock().daq.clone()
    }

    pub fn build_g2(&self, g2_param: G2Param) -> &Self {
        let video_cache = self.video_cache.clone();
        let gr = self.gr.clone();
        let bpw = self.bpw.clone();
        let fpw = self.fpw.clone();
        let (tx, rx) = crossbeam::channel::bounded(0);

        tokio::task::spawn_blocking(move || -> Result<()> {
            fpw.inner.load().store(i64::MIN, Ordering::Relaxed);
            let new = Arc::new(AtomicI64::new((g2_param.frame_num as i64) << 32));
            let old = bpw.inner.swap(new.clone());
            old.store(i64::MIN, Ordering::Relaxed);

            let mut gr = gr.lock();
            let _ = tx.send(());

            let g2 = loop {
                let vc = &video_cache.read();
                if vc.finished() {
                    break vc.build_g2(g2_param, &new)?;
                }
            };

            let g2 = g2.into_shared();
            gr.g2 = Some(g2.clone());
            gr.filtered_g2 = Some(g2);

            fpw.inner.load().store(0, Ordering::Relaxed);

            Ok(())
        });

        // We need to make sure the lock has been acquired so that `filter` will not
        // start until `build_g2` has finished. It won't block for long here because
        // current calculation is terminated before waiting for the lock.
        let _ = rx.recv();

        self
    }

    pub fn filter(&self, filter_method: FilterMethod) {
        let gr = self.gr.clone();
        let fpw = self.fpw.clone();

        tokio::task::spawn_blocking(move || {
            let g2 = gr.lock().g2.as_ref()?.clone();
            if fpw.inner.load().load(Ordering::Relaxed) < 0 {
                return None;
            }

            // Store total progress in higher 32 bits and the current progress is zero.
            let new = Arc::new(AtomicI64::new((g2.dim().1 as i64) << 32));
            let old = fpw.inner.swap(new.clone());
            old.store(i64::MIN, Ordering::Relaxed);

            let filtered_g2 = filter(filter_method, g2, &new).ok()?;
            gr.lock().filtered_g2 = Some(filtered_g2);

            Some(())
        });
    }

    pub async fn filter_single_point(
        &self,
        filter_method: FilterMethod,
        pos: usize,
    ) -> Result<Vec<u8>> {
        let gr = self.gr.clone();

        tokio::task::spawn_blocking(move || {
            let g2 = gr
                .lock()
                .g2
                .as_ref()
                .ok_or_else(|| anyhow!("g2 not built"))?
                .clone();
            filter_single_point(filter_method, g2, pos)
        })
        .await?
    }

    pub fn get_build_progress(&self) -> Option<CalProgress> {
        self.bpw.get_progress()
    }

    pub fn get_filter_progress(&self) -> Option<CalProgress> {
        self.fpw.get_progress()
    }
}
