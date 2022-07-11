mod decode;
mod frame_reader;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{codec, codec::packet::Packet};
use ffmpeg_next as ffmpeg;
use ndarray::{parallel::prelude::*, prelude::*};
use serde::{Deserialize, Serialize};
use tauri::async_runtime;
use tokio::sync::oneshot;
use tracing::{debug, error, info};

use crate::util::{
    progress_bar::{Progress, ProgressBar},
    timing,
};
use decode::DecoderManager;
use frame_reader::FrameReader;

pub struct VideoDataManager {
    video_data: Arc<RwLock<VideoData>>,
    frame_reader: FrameReader,
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
            frame_reader: FrameReader::new(),
            video_data: Arc::new(RwLock::new(VideoData::default())),
        }
    }

    pub fn data(&self) -> Arc<RwLock<VideoData>> {
        self.video_data.clone()
    }

    pub fn progress(&self) -> Progress {
        self.video_data.read().unwrap().progress_bar.get()
    }

    /// Spawn a thread to load all video packets into memory.
    pub async fn spawn_load_packets<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMetadata> {
        let video_data = self.video_data.clone();
        let path = video_path.as_ref().to_owned();
        info!("video_path: {:?}", path);
        let (tx, rx) = oneshot::channel();
        let join_handle = async_runtime::spawn_blocking(move || load_packets(video_data, path, tx));

        match rx.await {
            Ok(t) => Ok(t),
            Err(_) => Err(join_handle
                .await?
                .map_err(|e| anyhow!("failed to read video from {:?}: {}", video_path.as_ref(), e))
                .unwrap_err()),
        }
    }

    pub fn spawn_build_green2(&self, green2_param: Green2Param) {
        let video_data = self.video_data.clone();
        rayon::spawn(
            move || match build_green2(video_data.clone(), green2_param) {
                Ok(green2) => video_data.write().unwrap().green2 = Some(green2),
                Err(e) => {
                    video_data.read().unwrap().progress_bar.reset();
                    error!("{}", e);
                }
            },
        );
    }

    pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        self.frame_reader
            .read_single_frame_base64(self.video_data.clone(), frame_index)
            .await
    }
}

#[derive(Default)]
pub struct VideoData {
    video_cache: Option<VideoCache>,
    green2: Option<Array2<u8>>,
    progress_bar: ProgressBar,
}

impl VideoData {
    pub fn green2(&self) -> Option<ArrayView2<u8>> {
        Some(self.green2.as_ref()?.view())
    }
}

struct VideoCache {
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

fn load_packets(
    video_data: Arc<RwLock<VideoData>>,
    path: PathBuf,
    tx: oneshot::Sender<VideoMetadata>,
) -> Result<()> {
    let mut timer = timing::start("loading packets");

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
    video_data.write().unwrap().video_cache =
        Some(VideoCache::new(video_metadata.clone(), parameters));
    tx.send(video_metadata)
        .expect("The receiver has been dropped");

    let mut cnt = 0;
    for (_, packet) in input
        .packets()
        .filter(|(stream, _)| stream.index() == video_stream_index)
    {
        // `RwLockWriteGuard` is intentionally holden all the way during
        // reading *each* frame to avoid busy loop within `read_single_frame`.
        let mut video_data = video_data.write().unwrap();
        let video_cache = video_data
            .video_cache
            .as_mut()
            .expect("should have already been initialized");
        if video_cache.target_changed(&path) {
            // Video path has been changed, which means user changed the path before
            // previous loading finishes. So we should abort current loading at once.
            // Other threads should be waiting for the lock to read from the latest path
            // at this point.
            return Ok(());
        }
        video_cache.packets.push(packet);
        cnt += 1;
    }

    timer.finish();
    debug_assert!(cnt == nframes);
    debug!("total_frames: {}", nframes);

    Ok(())
}

fn build_green2(
    video_data: Arc<RwLock<VideoData>>,
    green2_param: Green2Param,
) -> Result<Array2<u8>> {
    let mut timer = timing::start("building green2");

    let Green2Param {
        path,
        start_frame,
        cal_num,
        area,
    } = green2_param;

    // Wait until all packets have been loaded.
    let video_data = loop {
        let video_data = video_data.read().unwrap();
        let video_cache = video_data
            .video_cache
            .as_ref()
            .ok_or_else(|| anyhow!("uninitilized"))?;
        if video_cache.target_changed(&path) {
            bail!("video path has been changed before starting building green2");
        }
        if video_cache.finished() {
            break video_data;
        }
    };

    let video_cache = video_data.video_cache.as_ref().unwrap();
    let progress_bar = &video_data.progress_bar;
    progress_bar.start(cal_num as u32);

    let byte_w = video_cache.video_metadata.shape.1 as usize * 3;
    let (tl_y, tl_x, h, w) = area;
    let (tl_y, tl_x, h, w) = (tl_y as usize, tl_x as usize, h as usize, w as usize);
    let mut green2 = Array2::zeros((cal_num, h * w));

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
    timer.finish();

    debug_assert!(matches!(
        progress_bar.get(),
        Progress::Finished { total } if total == cal_num as u32,
    ));

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
            let progress = video_data_manager.progress();
            if matches!(progress, Progress::Finished { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        println!("{:?}", video_data_manager.video_data.read().unwrap().green2);
    }
}
