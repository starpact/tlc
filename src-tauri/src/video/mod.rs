mod decode;
mod filter;
mod pool;
mod progress_bar;

use std::{
    assert_matches::debug_assert_matches,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::context::Input,
};
use ffmpeg_next as ffmpeg;
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array2, Axis};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tokio::sync::oneshot;
use tracing::{debug, error, info_span, instrument};

use crate::setting::SettingStorage;
use decode::DecoderManager;
pub use filter::{FilterMetadata, FilterMethod};
use pool::SpawnHandle;
pub use progress_bar::Progress;
use progress_bar::ProgressBar;

pub struct VideoManager<S: SettingStorage> {
    inner: Arc<VideoManagerInner<S>>,
}

impl<S: SettingStorage> Clone for VideoManager<S> {
    fn clone(&self) -> Self {
        VideoManager {
            inner: self.inner.clone(),
        }
    }
}

struct VideoManagerInner<S: SettingStorage> {
    /// The db connection is needed in order to keep the in-memory data in sync with db.
    /// Generally db write operation happens when video_data lock is holden.
    setting_storage: Arc<Mutex<S>>,

    /// Video data including raw packets, decoder cache, `green2` and `filtered_green2` matrix.
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

    /// Manage thread-local decoders.
    decoder_manager: DecoderManager,

    /// Green value 2d matrix(cal_num, pix_num).
    green2: Option<ArcArray2<u8>>,

    /// Use `ArcArray2` for copy on write because `filtered_green2` can
    /// share the same data with `green2` if we choose not to filter.
    filtered_green2: Option<ArcArray2<u8>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct VideoMetadata {
    pub path: PathBuf,
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (usize, usize),
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Green2Metadata {
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (usize, usize, usize, usize),
    pub video_fingerprint: String,
}

impl<S: SettingStorage> VideoManager<S> {
    pub fn new(setting_storage: Arc<Mutex<S>>) -> Self {
        Self {
            inner: Arc::new(VideoManagerInner {
                setting_storage,
                video_data: Default::default(),
                build_progress_bar: Default::default(),
                filter_progress_bar: Default::default(),
                spawn_handle: Default::default(),
            }),
        }
    }

    /// Read video metadata and update setting if needed and continue loading all
    /// video packets into memory in the background.
    /// `video_path` is_some means updating video setting.
    /// `video_path` is_none means reading the path from current setting.
    #[instrument(skip(self), err)]
    pub async fn spawn_load_packets(&self, video_path: Option<PathBuf>) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();

        let join_handle = spawn_blocking(move || {
            let video_manager = video_manager.inner;
            // Stop current building and filtering process.
            video_manager.build_progress_bar.reset();
            video_manager.filter_progress_bar.reset();

            let _span1 = info_span!("read_video_metadata").entered();
            let video_path = match video_path {
                Some(video_path) => video_path,
                None => {
                    video_manager
                        .setting_storage
                        .lock()
                        .unwrap()
                        .video_metadata()?
                        .path
                }
            };
            let input = ffmpeg::format::input(&video_path)?;
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
                path: video_path,
                frame_rate,
                nframes,
                shape: (decoder.height() as usize, decoder.width() as usize),
                fingerprint: fingerprint.clone(),
            };
            debug!(?video_metadata);

            // Update db and video data.
            {
                let mut video_data = video_manager.video_data.write().unwrap();
                video_manager
                    .setting_storage
                    .lock()
                    .unwrap()
                    .set_video_metadata(video_metadata)?;
                // Even if video has not changed, we will still reset all video data.
                video_data.reset(parameters);
            }

            // The outer `spawn_load_packets` will return after this.
            tx.send(()).unwrap();
            drop(_span1);

            video_manager.load_packets(input, video_stream_index, fingerprint, nframes)
        });

        match rx.await {
            Ok(()) => Ok(()),
            Err(_) => Err(join_handle.await?.unwrap_err()),
        }
    }

    pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        let spawner = self.inner.spawn_handle.spawner(frame_index).await?;
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();
        spawner.spawn(move || {
            let ret = video_manager.inner.read_single_frame_base64(frame_index);
            tx.send(ret).unwrap();
        });

        rx.await?
    }

    pub async fn spawn_build_green2(&self) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();

        let join_handle = spawn_blocking(move || {
            let video_manager = video_manager.inner;

            let (green2_metadata, green2) = {
                let video_data = video_manager.video_data.read().unwrap();

                let setting_storage = video_manager.setting_storage.lock().unwrap();
                let video_metadata = setting_storage.video_metadata()?;
                let green2_metadata = setting_storage.green2_metadata()?;
                drop(setting_storage);

                if video_data.packets.len() < video_metadata.nframes {
                    bail!("video not loaded yet");
                }
                // Tell outside that building actually started.
                tx.send(()).unwrap();

                let green2 = video_data.build_green2(
                    &video_metadata,
                    &green2_metadata,
                    &video_manager.build_progress_bar,
                )?;

                (green2_metadata, green2)
            };

            // Acquire write lock.
            let mut video_data = video_manager.video_data.write().unwrap();
            // Check metadata before write.
            if video_manager
                .setting_storage
                .lock()
                .unwrap()
                .green2_metadata()?
                != green2_metadata
            {
                bail!("setting has been changed while building green2");
            }
            video_data.green2 = Some(green2.into_shared());

            Ok(())
        });

        match rx.await {
            Ok(()) => Ok(()),
            Err(_) => Err(join_handle.await?.unwrap_err()),
        }
    }

    pub async fn spawn_filter_green2(&self) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();

        let f = move || -> Result<()> {
            let video_manager = video_manager.inner;

            let (filter_metadata, filtered_green2) = {
                let video_data = video_manager.video_data.read().unwrap();
                let green2 = video_data
                    .green2
                    .clone()
                    .ok_or_else(|| anyhow!("green2 not built yet"))?;
                let filter_metadata = video_manager
                    .setting_storage
                    .lock()
                    .unwrap()
                    .filter_metadata()?;
                // Tell outside that filtering actually started.
                tx.send(()).unwrap();
                let filtered_green2 = filter::filter_all(
                    green2,
                    filter_metadata.filter_method,
                    &video_manager.filter_progress_bar,
                )?;

                (filter_metadata, filtered_green2)
            };

            let mut video_data = video_manager.video_data.write().unwrap();
            if video_manager
                .setting_storage
                .lock()
                .unwrap()
                .filter_metadata()?
                != filter_metadata
            {
                bail!("setting has been changed while filtering green2");
            }
            video_data.filtered_green2 = Some(filtered_green2);

            Ok(())
        };

        let join_handle = spawn_blocking(|| match f() {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Failed to filter green2: {e}");
                Err(e)
            }
        });

        match rx.await {
            Ok(()) => Ok(()),
            Err(_) => Err(join_handle.await?.unwrap_err()),
        }
    }

    pub async fn filter_single_point(&self, (y, x): (usize, usize)) -> Result<Vec<u8>> {
        let video_manager = self.clone();
        spawn_blocking(move || {
            // Hold the read lock.
            let video_manager = video_manager.inner;
            let video_data = video_manager.video_data.read().unwrap();
            let (h, w) = video_manager
                .setting_storage
                .lock()
                .unwrap()
                .video_metadata()?
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
            let filter_method = video_manager
                .setting_storage
                .lock()
                .unwrap()
                .filter_metadata()?
                .filter_method;

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

impl<S: SettingStorage> VideoManagerInner<S> {
    #[instrument(skip_all, err)]
    fn load_packets(
        &self,
        mut input: Input,
        video_stream_index: usize,
        fingerprint: String,
        nframes: usize,
    ) -> Result<()> {
        const LOCAL_BUFFER_LENGTH: usize = 50;
        let mut buf = Vec::with_capacity(LOCAL_BUFFER_LENGTH);
        let mut cnt = 0;
        for (_, packet) in input
            .packets()
            .filter(|(stream, _)| stream.index() == video_stream_index)
        {
            // The first frame should be available as soon as possible.
            if cnt == 1 || buf.len() == LOCAL_BUFFER_LENGTH {
                self.batch_push_packets(&fingerprint, &mut buf)?;
            }
            buf.push(packet);
            cnt += 1;
        }

        if !buf.is_empty() {
            self.batch_push_packets(&fingerprint, &mut buf)?;
        }

        debug_assert!(cnt == nframes);
        debug!(nframes);

        Ok(())
    }

    fn batch_push_packets(&self, fingerprint: &str, buf: &mut Vec<Packet>) -> Result<()> {
        let mut video_data = self.video_data.write().unwrap();
        if fingerprint
            != self
                .setting_storage
                .lock()
                .unwrap()
                .video_metadata()?
                .fingerprint
        {
            // Video has been changed, which means user changed the video before previous
            // loading finishes. So we should abort current loading at once. Other threads
            // should be waiting for the lock to read from the latest path at this point.
            bail!("video has been changed before finishing loading packets");
        }
        video_data.packets.append(buf);

        Ok(())
    }

    #[instrument(skip(self), err)]
    fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        loop {
            let video_data = self.video_data.read().unwrap();

            let VideoMetadata {
                nframes,
                shape: (h, w),
                ..
            } = self.setting_storage.lock().unwrap().video_metadata()?;
            if frame_index >= nframes {
                // This is an invalid `frame_index` from frontend and will never get the frame.
                // So directly abort it.
                bail!("frame_index({}) out of range({})", frame_index, nframes);
            }

            if let Some(packet) = video_data.packets.get(frame_index) {
                let _span = info_span!("decode_single_frame").entered();

                let mut decoder = video_data.decoder_manager.decoder()?;
                let mut buf = Vec::new();
                let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                jpeg_encoder.encode(decoder.decode(packet)?.data(0), w as u32, h as u32, Rgb8)?;

                break Ok(base64::encode(buf));
            }

            if self.spawn_handle.last_target_frame_index() != frame_index {
                bail!("aborted, to give priority to newer target frame index");
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl VideoData {
    fn reset(&mut self, parameters: Parameters) {
        self.packets.clear();
        self.decoder_manager.reset(parameters);
        self.green2 = None;
        self.filtered_green2 = None;
    }

    #[instrument(skip(self), err)]
    fn build_green2(
        &self,
        video_metadata: &VideoMetadata,
        green2_metadata: &Green2Metadata,
        progress_bar: &ProgressBar,
    ) -> Result<Array2<u8>> {
        let cal_num = green2_metadata.cal_num;
        let byte_w = video_metadata.shape.1 * 3;
        let (tl_y, tl_x, cal_h, cal_w) = green2_metadata.area;

        let _reset_guard = progress_bar.start(cal_num as u32);

        let _span2 = info_span!("decode_in_parallel").entered();
        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));
        self.packets
            .par_iter()
            .skip(green2_metadata.start_frame)
            .zip(green2.axis_iter_mut(Axis(0)).into_iter())
            .try_for_each(|(packet, mut row)| -> Result<()> {
                let mut decoder = self.decoder_manager.decoder()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{setting::SqliteSettingStorage, util::log};

    #[tokio::test]
    #[ignore]
    async fn test_load_packets() {
        log::init();
        let video_manager = VideoManager::new(Arc::new(Mutex::new(SqliteSettingStorage::new())));
        video_manager
            .spawn_load_packets(Some(PathBuf::from(
                "/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi",
            )))
            .await
            .unwrap();
        println!(
            "{:#?}",
            video_manager
                .inner
                .setting_storage
                .lock()
                .unwrap()
                .video_metadata()
                .unwrap()
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_read_single_frame_pending_until_available() {
        log::init();
        let video_manager = VideoManager::new(Arc::new(Mutex::new(SqliteSettingStorage::new())));
        video_manager
            .spawn_load_packets(Some(PathBuf::from(
                "/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi",
            )))
            .await
            .unwrap();

        let nframes = video_manager
            .inner
            .setting_storage
            .lock()
            .unwrap()
            .video_metadata()
            .unwrap()
            .nframes;
        let last_frame_index = nframes - 1;
        println!("waiting for frame {}", last_frame_index);
        let frame_str = video_manager
            .read_single_frame_base64(last_frame_index)
            .await
            .unwrap();
        println!("{}", frame_str.len());

        assert_eq!(
            format!(
                "frame_index({nframes}) out of range({nframes})",
                nframes = nframes
            ),
            video_manager
                .read_single_frame_base64(nframes)
                .await
                .unwrap_err()
                .to_string(),
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_build_green2() {
        log::init();
        let video_manager = VideoManager::new(Arc::new(Mutex::new(SqliteSettingStorage::new())));
        let video_path =
            PathBuf::from("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi");
        video_manager
            .spawn_load_packets(Some(video_path))
            .await
            .unwrap();
        video_manager.spawn_build_green2().await.unwrap();

        loop {
            let progress = video_manager.build_progress();
            if matches!(progress, Progress::Finished { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        println!(
            "{:?}",
            video_manager.inner.video_data.read().unwrap().green2
        );
    }
}
