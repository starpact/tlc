mod decode;
mod filter;
mod frame_reader;

use std::{
    assert_matches::debug_assert_matches,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{codec, codec::packet::Packet};
use ffmpeg_next as ffmpeg;
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tauri::async_runtime;
use tokio::sync::oneshot;
use tracing::{debug, error, instrument, trace_span};

use crate::util::progress_bar::{Progress, ProgressBar};
use decode::DecoderManager;
pub use filter::FilterMethod;
use frame_reader::FrameReader;

pub struct VideoDataManager {
    video_data: Arc<RwLock<VideoData>>,
    frame_reader: FrameReader,
    build_progress_bar: ProgressBar,
    filter_progress_bar: ProgressBar,
}

/// `frame_rate`, `total_frames`, `shape` is determined once the video(path)
/// is determined, so we do not deserialize them from the config file but
/// always directly get them from video stat. However, they are still recorded
/// in the config file because they might be useful information for users.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VideoMetadata {
    /// Path of TLC video file.
    pub path: PathBuf,

    /// Frame rate of video.
    #[serde(skip_deserializing)]
    pub frame_rate: usize,

    /// Total frames of video.
    #[serde(skip_deserializing)]
    pub nframes: usize,

    /// (video_height, video_width)
    #[serde(skip_deserializing)]
    pub shape: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct Green2Param {
    pub path: PathBuf,
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (usize, usize, usize, usize),
}

impl VideoDataManager {
    pub fn new() -> Self {
        Self {
            video_data: Arc::new(RwLock::new(VideoData::default())),
            frame_reader: FrameReader::new(),
            build_progress_bar: ProgressBar::default(),
            filter_progress_bar: ProgressBar::default(),
        }
    }

    pub fn get_build_progress(&self) -> Progress {
        self.build_progress_bar.get()
    }

    pub fn get_filter_progress(&self) -> Progress {
        self.filter_progress_bar.get()
    }

    #[allow(dead_code)]
    pub fn green2(&self) -> Option<ArcArray2<u8>> {
        Some(self.video_data.read().unwrap().green2.as_ref()?.clone())
    }

    pub fn filtered_green2(&self) -> Option<ArcArray2<u8>> {
        Some(
            self.video_data
                .read()
                .unwrap()
                .filtered_green2
                .as_ref()?
                .clone(),
        )
    }

    /// Spawn a thread to load all video packets into memory.
    pub async fn spawn_load_packets<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMetadata> {
        let video_data = self.video_data.clone();
        let path = video_path.as_ref().to_owned();
        let (tx, rx) = oneshot::channel();
        let join_handle = async_runtime::spawn_blocking(move || {
            load_packets(video_data.clone(), path.clone(), tx).map_err(|e| {
                // Error after send can not be returned, so log here.
                // `video_cache` is set to `None` to tell all waiters that `load_packets` failed
                // so should stop waiting.
                error!("Failed to load video from {:?}: {}", path, e);
                video_data.write().unwrap().reset();
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
        self.frame_reader
            .read_single_frame_base64(self.video_data.clone(), frame_index)
            .await
    }

    pub fn spawn_build_green2(&self, green2_param: Green2Param) {
        let video_data = self.video_data.clone();
        let progress_bar = self.build_progress_bar.clone();
        rayon::spawn(move || {
            match build_green2(video_data.clone(), progress_bar.clone(), green2_param) {
                Ok(green2) => video_data.write().unwrap().green2 = Some(green2.into_shared()),
                Err(e) => {
                    progress_bar.reset();
                    error!("Failed to build green2: {}", e);
                }
            }
        });
    }

    pub fn spawn_filter_green2(&self, filter_method: FilterMethod) -> Result<()> {
        let video_data = self.video_data.clone();
        if video_data.read().unwrap().green2.is_none() {
            bail!("green2 not built yet");
        }

        let progress_bar = self.filter_progress_bar.clone();
        rayon::spawn(move || {
            if let Some(green2) = video_data.read().unwrap().green2.clone() {
                match filter::filter_all(green2, filter_method, progress_bar.clone()) {
                    Ok(filtered_green2) => {
                        video_data.write().unwrap().filtered_green2 = Some(filtered_green2);
                    }
                    Err(e) => {
                        progress_bar.reset();
                        error!("Failed to filter green2: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn filter_single_point(
        &self,
        filter_method: FilterMethod,
        (y, x): (usize, usize),
    ) -> Result<Vec<u8>> {
        let video_data = self.video_data.clone();
        async_runtime::spawn_blocking(move || {
            let video_data = video_data.read().unwrap();
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

impl VideoData {
    fn new(video_metadata: VideoMetadata, parameters: codec::Parameters) -> Self {
        Self {
            video_cache: Some(VideoCache::new(video_metadata, parameters)),
            green2: None,
            filtered_green2: None,
        }
    }

    fn reset(&mut self) {
        self.video_cache = None;
        self.green2 = None;
        self.filtered_green2 = None;
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

    fn target_changed<P: AsRef<Path>>(&self, original_path: P) -> bool {
        self.video_metadata.path != original_path.as_ref()
    }

    fn finished(&self) -> bool {
        self.video_metadata.nframes == self.packets.len()
    }
}

#[instrument(skip(video_data, tx))]
fn load_packets(
    video_data: Arc<RwLock<VideoData>>,
    path: PathBuf,
    tx: oneshot::Sender<VideoMetadata>,
) -> Result<()> {
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

    let video_metadata = VideoMetadata {
        path: path.clone(),
        frame_rate,
        nframes,
        shape: (decoder.height() as usize, decoder.width() as usize),
    };

    *video_data.write().unwrap() = VideoData::new(video_metadata.clone(), parameters);

    // Caller `spawn_load_packets` can return after this point.
    tx.send(video_metadata)
        .expect("The receiver has been dropped");
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
        let mut video_data = video_data.write().unwrap();
        let video_cache = video_data
            .video_cache
            .as_mut()
            .ok_or_else(|| anyhow!("video has not been loaded"))?;
        if video_cache.target_changed(&path) {
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

#[instrument(skip(video_data))]
fn build_green2(
    video_data: Arc<RwLock<VideoData>>,
    progress_bar: ProgressBar,
    green2_param: Green2Param,
) -> Result<Array2<u8>> {
    let Green2Param {
        path,
        start_frame,
        cal_num,
        area,
    } = green2_param;

    // Wait until all packets have been loaded.
    let _span1 = trace_span!("spin_wait_for_loading_packets").entered();
    let video_data = loop {
        let video_data = video_data.read().unwrap();
        let video_cache = video_data
            .video_cache
            .as_ref()
            .ok_or_else(|| anyhow!("video has not been loaded"))?;
        if video_cache.target_changed(&path) {
            bail!("video path has been changed before starting building green2");
        }
        if video_cache.finished() {
            break video_data;
        }
    };
    drop(_span1);

    let video_cache = video_data.video_cache.as_ref().unwrap();
    progress_bar.start(cal_num as u32);

    let _span2 = trace_span!("alloc_green2_matrix").entered();
    let byte_w = video_cache.video_metadata.shape.1 as usize * 3;
    let (tl_y, tl_x, h, w) = area;
    let (tl_y, tl_x, h, w) = (tl_y as usize, tl_x as usize, h as usize, w as usize);
    let mut green2 = Array2::zeros((cal_num, h * w));
    drop(_span2);

    let _span3 = trace_span!("decode_in_parallel").entered();
    video_cache
        .packets
        .par_iter()
        .skip(start_frame)
        .zip(green2.axis_iter_mut(Axis(0)).into_iter())
        .try_for_each(|(packet, mut row)| -> Result<()> {
            let mut decoder = video_cache.decoder_manager.get()?;
            let dst_frame = decoder.decode(packet)?;

            // each frame is stored in a u8 array:
            // |r g b r g b...r g b|r g b r g b...r g b|......|r g b r g b...r g b|
            // |.......row_0.......|.......row_1.......|......|.......row_n.......|
            let rgb = dst_frame.data(0);
            let mut row_iter = row.iter_mut();

            for i in (0..).step_by(byte_w).skip(tl_y).take(h) {
                for j in (i..).skip(1).step_by(3).skip(tl_x).take(w) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::log;

    #[tokio::test]
    async fn test_load_packets() {
        log::init();
        let video_data_manager = VideoDataManager::new();
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
        let video_data_manager = VideoDataManager::new();
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
        let video_data_manager = VideoDataManager::new();
        let video_path =
            PathBuf::from("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi");
        video_data_manager
            .spawn_load_packets(video_path.clone())
            .await
            .unwrap();
        video_data_manager.spawn_build_green2(Green2Param {
            path: video_path,
            start_frame: 0,
            cal_num: 2000,
            area: (0, 0, 1000, 800),
        });

        loop {
            let progress = video_data_manager.get_build_progress();
            if matches!(progress, Progress::Finished { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        println!("{:?}", video_data_manager.video_data.read().unwrap().green2);
    }
}
