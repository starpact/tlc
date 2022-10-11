mod decode;
mod filter;
mod spawn_handle;

use std::{
    assert_matches::debug_assert_matches,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{codec, codec::packet::Packet};
use ffmpeg_next as ffmpeg;
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array2, Axis};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tokio::sync::oneshot;
use tracing::{debug, error, instrument, trace_span};

use crate::util::progress_bar::{Progress, ProgressBar};
use decode::DecoderManager;
pub use filter::FilterMethod;

use self::spawn_handle::SpawnHandle;

#[derive(Clone, Default)]
pub struct VideoManager {
    inner: Arc<VideoManagerInner>,
}

#[derive(Default)]
struct VideoManagerInner {
    /// Video data including raw packets, `green2` matrix and `filtered_green2` matrix.
    video_data: RwLock<VideoData>,

    /// Progree bar for building green2.
    build_progress_bar: ProgressBar,

    /// Progree bar for filtering green2.
    filter_progress_bar: ProgressBar,

    /// Frame graber controller.
    spawn_handle: SpawnHandle,
}

/// `VideoData` contains all video related data, built in the following order:
/// video_cache -> green2 -> filtered_green2.
#[derive(Default)]
struct VideoData {
    /// Raw video data.
    video_cache: Option<VideoCache>,

    /// Green value 2d matrix(cal_num, pix_num).
    green2: Option<ArcArray2<u8>>,

    /// Use `ArcArray2` for copy on write because `filtered_green2` can
    /// share the same data with `green2` if we choose not to filter.
    filtered_green2: Option<ArcArray2<u8>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VideoMetadata {
    pub path: PathBuf,
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (usize, usize),
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct Green2Metadata {
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (usize, usize, usize, usize),
    pub video_fingerprint: String,
}

impl VideoManager {
    /// Spawn a thread to load all video packets into memory.
    pub async fn spawn_load_packets<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMetadata> {
        let video_manager = self.clone();
        let path = video_path.as_ref().to_owned();
        let (tx, rx) = oneshot::channel();
        let join_handle = spawn_blocking(move || {
            let video_manager = video_manager.inner;
            video_manager.load_packets(path.clone(), tx).map_err(|e| {
                // Error after send can not be returned, so log here.
                // `video_cache` is set to `None` to tell all waiters that `load_packets` failed
                // so should stop waiting.
                error!("Failed to load video from {:?}: {}", path, e);
                e
            })
        });

        match rx.await {
            Ok(video_metadata) => Ok(video_metadata),
            // `RecvError` only means sender has been dropped which means error occurred before
            // send, so it is ignored. Task should have already failed so we can get the error
            // from the `join_handle` immediately.
            Err(_) => Err(join_handle
                .await?
                .map_err(|e| anyhow!("failed to load video from {:?}: {}", video_path.as_ref(), e))
                .unwrap_err()),
        }
    }

    pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        let spawner = self.inner.spawn_handle.get_spwaner(frame_index).await?;
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();
        spawner.spawn(move || {
            let ret = video_manager.inner.read_single_frame_base64(frame_index);
            tx.send(ret).unwrap();
        });

        rx.await?
    }

    pub async fn spawn_build_green2(&self, green2_metadata: Green2Metadata) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();

        // Interrupt if needed the current build process and init the progress bar.
        video_manager
            .inner
            .build_progress_bar
            .start(green2_metadata.cal_num as u32);

        rayon::spawn(move || {
            let video_manager = video_manager.inner;
            let mut video_data = {
                let _span1 = trace_span!("spin_wait_for_loading_packets").entered();
                // Wait until all packets have been loaded.
                loop {
                    let video_data = video_manager.video_data.write().unwrap();
                    let Some(video_cache) = video_data.video_cache.as_ref() else {
                        tx.send(Err(anyhow!("video not loaded yet"))).unwrap();
                        return;
                    };
                    if video_cache.target_changed(&green2_metadata.video_fingerprint) {
                        tx.send(Err(anyhow!(
                            "video has been changed before start building green2, so aborted"
                        )))
                        .unwrap();
                        return;
                    }
                    if video_cache.finished() {
                        break video_data;
                    }
                }
            };

            // Tell outside that building actually started.
            tx.send(Ok(())).unwrap();

            match video_data
                .video_cache
                .as_ref()
                .unwrap() // cannot be none according to previous logic
                .build_green2(&green2_metadata, &video_manager.build_progress_bar)
            {
                Ok(green2) => video_data.green2 = Some(green2.into_shared()),
                Err(e) => {
                    video_manager.build_progress_bar.reset();
                    error!("Failed to build green2: {}", e);
                }
            }
        });

        rx.await?
    }

    pub async fn spawn_filter_green2(&self, filter_method: FilterMethod) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();
        rayon::spawn(move || {
            let video_manager = video_manager.inner;
            // Hold the read lock.
            let video_data = video_manager.video_data.read().unwrap();
            if let Some(green2) = video_data.green2.as_ref() {
                // Tell outside that filtering actually started.
                tx.send(Ok(())).unwrap();

                match filter::filter_all(
                    green2.clone(),
                    filter_method,
                    &video_manager.filter_progress_bar,
                ) {
                    Ok(filtered_green2) => {
                        video_manager.video_data.write().unwrap().filtered_green2 =
                            Some(filtered_green2);
                    }
                    Err(e) => {
                        video_manager.filter_progress_bar.reset();
                        error!("Failed to filter green2: {}", e);
                    }
                }
            }
            // tx dropped without any send. rx.await will be `RecvError`.
        });

        rx.await.map_err(|_| anyhow!("green2 not built yet"))?
    }

    pub async fn filter_single_point(
        &self,
        filter_method: FilterMethod,
        (y, x): (usize, usize),
    ) -> Result<Vec<u8>> {
        let video_manager = self.clone();
        spawn_blocking(move || {
            // Hold the read lock.
            let video_data = video_manager.inner.video_data.read().unwrap();
            let (h, w) = video_data
                .video_cache
                .as_ref()
                .ok_or_else(|| anyhow!("video not loaded yet"))?
                .video_metadata
                .shape;
            if y >= h {
                bail!("y({y}) out of range({h})");
            }
            if x >= w {
                bail!("x({x}) out of range({w})");
            }
            let position = y * w + x;
            let green1 = video_data
                .green2
                .as_ref()
                .ok_or_else(|| anyhow!("green2 not built yet"))?
                .row(position);

            Ok(filter::filter_single_point(filter_method, green1))
        })
        .await?
    }

    pub fn build_progress(&self) -> Progress {
        self.inner.build_progress_bar.get()
    }

    pub fn filter_progress(&self) -> Progress {
        self.inner.filter_progress_bar.get()
    }

    pub fn filtered_green2(&self) -> Option<ArcArray2<u8>> {
        Some(
            self.inner
                .video_data
                .read()
                .unwrap()
                .filtered_green2
                .as_ref()?
                .clone(),
        )
    }
}

impl VideoData {
    fn new(video_metadata: VideoMetadata, parameters: codec::Parameters) -> Self {
        Self {
            video_cache: Some(VideoCache::new(video_metadata, parameters)),
            green2: None,
            filtered_green2: None,
        }
    }
}

struct VideoCache {
    /// As identifier with some basic information of current cached video.
    video_metadata: VideoMetadata,

    /// Manage thread-local decoders.
    decoder_manager: DecoderManager,

    /// > [For video, one packet should typically contain one compressed frame](
    /// https://libav.org/documentation/doxygen/master/structAVPacket.html).
    ///
    /// There are two key points:
    /// 1. Will *one* packet contain more than *one* frame? As videos used
    /// in TLC experiments are lossless and have high-resolution, we can assert
    /// that one packet only contains one frame, which make multi-threaded
    /// decoding [much easier](https://www.cnblogs.com/TaigaCon/p/10220356.html).
    /// 2. Why not cache the frame data, which should be more straight forward?
    /// This is because packet is *compressed*. Specifically, a typical video
    /// in our experiments of 1.9GB will expend to 9.1GB if decoded to rgb byte
    /// array, which may cause some trouble on PC.
    packets: Vec<Packet>,
}

impl VideoCache {
    fn new(video_metadata: VideoMetadata, parameters: codec::Parameters) -> Self {
        Self {
            video_metadata,
            decoder_manager: DecoderManager::new(parameters),
            packets: Vec::new(),
        }
    }

    fn target_changed(&self, fingerprint: &str) -> bool {
        self.video_metadata.fingerprint != fingerprint
    }

    fn finished(&self) -> bool {
        self.video_metadata.nframes == self.packets.len()
    }

    #[instrument(skip(self))]
    fn build_green2(
        &self,
        green2_metadata: &Green2Metadata,
        progress_bar: &ProgressBar,
    ) -> Result<Array2<u8>> {
        let cal_num = green2_metadata.cal_num;

        let byte_w = self.video_metadata.shape.1 as usize * 3;
        let (tl_y, tl_x, cal_h, cal_w) = green2_metadata.area;
        let (tl_y, tl_x, cal_h, cal_w) =
            (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));

        let _span2 = trace_span!("decode_in_parallel").entered();
        self.packets
            .par_iter()
            .skip(green2_metadata.start_frame)
            .zip(green2.axis_iter_mut(Axis(0)).into_iter())
            .try_for_each(|(packet, mut row)| -> Result<()> {
                let mut decoder = self.decoder_manager.get()?;
                let dst_frame = decoder.decode(packet)?;

                // each frame is stored in a u8 array:
                // |r g b r g b...r g b|r g b r g b...r g b|......|r g b r g b...r g b|
                // |.......row_0.......|.......row_1.......|......|.......row_n.......|
                let rgb = dst_frame.data(0);
                let mut row_iter = row.iter_mut();

                for i in (0..).step_by(byte_w).skip(tl_y).take(cal_h) {
                    for j in (i..).skip(1).step_by(3).skip(tl_x).take(cal_w) {
                        // Bounds check can be removed by optimization so no need to use unsafe.
                        // Same performance as `unwrap_unchecked` + `get_unchecked`.
                        if let Some(b) = row_iter.next() {
                            *b = rgb[j];
                        }
                    }
                }

                progress_bar.add(1)?;

                Ok(())
            })?;

        debug_assert_matches!(
            progress_bar.get(),
            Progress::Finished { total } if total == cal_num as u32,
        );

        Ok(green2)
    }
}

impl VideoManagerInner {
    #[instrument(skip(self, tx))]
    fn load_packets(&self, path: PathBuf, tx: oneshot::Sender<VideoMetadata>) -> Result<()> {
        let _span1 = trace_span!("load_metadata").entered();
        let mut input = ffmpeg::format::input(&path)?;
        let video_stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| anyhow!("video stream not found"))?;

        let video_stream_index = video_stream.index();
        let parameters = video_stream.parameters();
        let codec_ctx = codec::Context::from_parameters(parameters.clone())?;
        let rational = video_stream.avg_frame_rate();
        let frame_rate = (rational.0 as f64 / rational.1 as f64).round() as usize;
        let nframes = video_stream.frames() as usize;
        let decoder = codec_ctx.decoder().video()?;

        let fingerprint = "TODO".to_owned();
        let video_metadata = VideoMetadata {
            path,
            frame_rate,
            nframes,
            shape: (decoder.height() as usize, decoder.width() as usize),
            fingerprint: fingerprint.clone(),
        };

        *self.video_data.write().unwrap() = VideoData::new(video_metadata.clone(), parameters);

        // Caller `spawn_load_packets` can return after this point.
        tx.send(video_metadata).unwrap();
        drop(_span1);

        let _span2 = trace_span!("load_packets_core").entered();
        let mut cnt = 0;
        for (_, packet) in input
            .packets()
            .filter(|(stream, _)| stream.index() == video_stream_index)
        {
            // `RwLockWriteGuard` is intentionally holden all the way during
            // reading *each* frame to avoid busy loop within `read_single_frame_base64`
            // and `build_green2`.
            let mut video_data = self.video_data.write().unwrap();
            let video_cache = video_data
                .video_cache
                .as_mut()
                .ok_or_else(|| anyhow!("video has not been loaded"))?;
            if video_cache.target_changed(&fingerprint) {
                // Video path has been changed, which means user changed the path before
                // previous loading finishes. So we should abort current loading at once.
                // Other threads should be waiting for the lock to read from the latest path
                // at this point.
                bail!("video path has been changed before finishing loading green2");
            }
            video_cache.packets.push(packet);
            cnt += 1;
        }

        debug_assert!(cnt == nframes);
        debug!(nframes);

        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        loop {
            let video_data = self.video_data.read().unwrap();
            let video_cache = video_data
                .video_cache
                .as_ref()
                .ok_or_else(|| anyhow!("uninitialized"))?;

            let nframes = video_cache.video_metadata.nframes;
            if frame_index >= nframes {
                // This is an invalid `frame_index` from frontend and will never get the frame.
                // So directly abort it.
                bail!("frame_index({}) out of range({})", frame_index, nframes);
            }

            if let Some(packet) = video_cache.packets.get(frame_index) {
                let _span = trace_span!("decode_single_frame").entered();

                let mut decoder = video_cache.decoder_manager.get()?;
                let (h, w) = video_cache.video_metadata.shape;
                let mut buf = Vec::new();
                let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                jpeg_encoder.encode(decoder.decode(packet)?.data(0), w as u32, h as u32, Rgb8)?;

                break Ok(base64::encode(buf));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::log;

    #[tokio::test]
    async fn test_load_packets() {
        log::init();
        let video_data_manager = VideoManager::default();
        let video_metadata = video_data_manager
            .spawn_load_packets("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();
        println!("{:#?}", video_metadata);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    #[tokio::test]
    async fn test_read_single_frame_pending_until_available() {
        log::init();
        let video_data_manager = VideoManager::default();
        let video_metadata = video_data_manager
            .spawn_load_packets("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();

        let last_frame_index = video_metadata.nframes - 1;
        println!("waiting for frame {}", last_frame_index);
        let frame_str = video_data_manager
            .read_single_frame_base64(last_frame_index)
            .await
            .unwrap();
        println!("{}", frame_str.len());

        assert_eq!(
            format!(
                "frame_index({nframes}) out of range({nframes})",
                nframes = video_metadata.nframes
            ),
            video_data_manager
                .read_single_frame_base64(video_metadata.nframes)
                .await
                .unwrap_err()
                .to_string(),
        );
    }

    #[tokio::test]
    async fn test_build_green2() {
        log::init();
        let video_data_manager = VideoManager::default();
        let video_path =
            PathBuf::from("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi");
        video_data_manager
            .spawn_load_packets(video_path.clone())
            .await
            .unwrap();
        video_data_manager
            .spawn_build_green2(Green2Metadata {
                start_frame: 0,
                cal_num: 2000,
                area: (0, 0, 1000, 800),
                video_fingerprint: todo!(),
            })
            .await
            .unwrap();

        loop {
            let progress = video_data_manager.build_progress();
            if matches!(progress, Progress::Finished { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        println!(
            "{:?}",
            video_data_manager.inner.video_data.read().unwrap().green2
        );
    }
}
