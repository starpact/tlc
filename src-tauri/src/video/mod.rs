mod decode;
mod filter;
mod pg1;
mod pool;
mod progress_bar;

use std::{
    assert_matches::debug_assert_matches,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::Sender;
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{ArcArray2, Array2, Axis};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tokio::sync::oneshot;
use tracing::{info_span, instrument};

use crate::setting::SettingStorage;
use decode::DecoderManager;
pub use filter::{FilterMeta, FilterMethod};
use pool::SpawnHandle;
pub use progress_bar::Progress;
use progress_bar::ProgressBar;

pub struct VideoData {
    video_meta: VideoMeta,
    packets: Packets,
    decoder_manager: Arc<DecoderManager>,
    green2: Option<ArcArray2<u8>>,
    gmax_frame_indexes: Option<Arc<Vec<usize>>>,
}

pub enum Packets {
    /// Used when packets are being loaded gradually.
    InProgress(Vec<Packet>),
    /// After finished loading all packets, `Packets` becomes immutable and can be shared
    /// with other thread cheaply.
    Finished(Arc<Vec<Packet>>),
}

impl VideoData {
    pub fn new(video_meta: VideoMeta, parameters: Parameters) -> VideoData {
        let nframes = video_meta.nframes;
        let packets = Packets::InProgress(Vec::with_capacity(nframes));
        let decoder_manager = Arc::new(DecoderManager::new(parameters));

        VideoData {
            video_meta,
            packets,
            decoder_manager,
            green2: None,
            gmax_frame_indexes: None,
        }
    }

    pub fn video_meta(&self) -> &VideoMeta {
        &self.video_meta
    }

    pub fn push_packet(&mut self, video_path: &Path, packet: Packet) -> Result<()> {
        if self.video_meta.path != *video_path {
            bail!("video path changed");
        }

        //  meta1     p11  p12
        // ...|........|....|.....................
        // ...................|.......|.....|.......
        //                  meta2    p21   p22
        match self.packets {
            Packets::InProgress(ref mut packets) => {
                packets.push(packet);
                if packets.len() == self.video_meta.nframes {
                    self.packets = Packets::Finished(Arc::new(std::mem::take(packets)));
                }
            }
            Packets::Finished(_) => unreachable!(),
        }

        Ok(())
    }
}

#[instrument(skip(tx1, tx2), fields(video_path = video_path.as_ref().to_str().unwrap()), err)]
pub fn read_video<P: AsRef<Path>>(
    video_path: P,
    tx1: oneshot::Sender<(VideoMeta, Parameters)>,
    tx2: Sender<(Arc<PathBuf>, Packet)>,
) -> Result<()> {
    // Stop current building and peak detection process.
    // self.build_green2_progress_bar.reset();
    // self.detect_peak_progress_bar.reset();
    let video_path = video_path.as_ref();

    let _span1 = info_span!("read_video_meta").entered();
    let mut input = ffmpeg::format::input(&video_path)?;
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
    let shape = (decoder.height() as usize, decoder.width() as usize);

    let video_meta = VideoMeta {
        path: video_path.to_owned(),
        frame_rate,
        nframes,
        shape,
    };
    tx1.send((video_meta, parameters)).map_err(|_| ()).unwrap();
    drop(_span1);

    let video_path = Arc::new(video_path.to_owned());
    let _span2 = info_span!("load_packets", frame_rate, nframes).entered();
    input
        .packets()
        .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
        .try_for_each(|packet| tx2.send((video_path.clone(), packet)))?;

    Ok(())
}

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
    video_data: RwLock<VideoData1>,

    /// Progree bar for building green2.
    build_green2_progress_bar: ProgressBar,

    /// Progree bar for detecting peaks.
    detect_peak_progress_bar: ProgressBar,

    /// Frame graber controller.
    spawn_handle: SpawnHandle,
}

/// `VideoData` contains all video related data, built in the following order:
/// packets & decoder_manager -> green2 -> gmax_frame_indexes.
#[derive(Default)]
struct VideoData1 {
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
    green2: Option<Array2<u8>>,

    /// Frame index of peak temperature.
    gmax_frame_indexes: Option<Arc<Vec<usize>>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct VideoMeta {
    pub path: PathBuf,
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Green2Meta {
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (usize, usize, usize, usize),
    pub video_path: PathBuf,
}

impl<S: SettingStorage> VideoManager<S> {
    pub fn new(setting_storage: Arc<Mutex<S>>) -> Self {
        Self {
            inner: Arc::new(VideoManagerInner {
                setting_storage,
                video_data: Default::default(),
                build_green2_progress_bar: Default::default(),
                detect_peak_progress_bar: Default::default(),
                spawn_handle: Default::default(),
            }),
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
        let join_handle = spawn_blocking(move || video_manager.inner.build_green2(tx));

        match rx.await {
            Ok(()) => Ok(()),
            Err(_) => Err(join_handle.await?.unwrap_err()),
        }
    }

    // Filter green2 if needed before start peak detection.
    pub async fn spawn_detect_peak(&self) -> Result<()> {
        let video_manager = self.clone();
        let (tx, rx) = oneshot::channel();
        let join_handle = spawn_blocking(move || video_manager.inner.detect_peak(tx));

        match rx.await {
            Ok(()) => Ok(()),
            Err(_) => Err(join_handle.await?.unwrap_err()),
        }
    }

    pub async fn filter_single_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
        let video_manager = self.clone();
        spawn_blocking(move || video_manager.inner.filter_single_point(position)).await?
    }

    pub fn build_green2_progress(&self) -> Progress {
        self.inner.build_green2_progress_bar.progress()
    }

    pub fn detect_peak_progress_bar(&self) -> Progress {
        self.inner.detect_peak_progress_bar.progress()
    }

    pub fn gmax_frame_indexes(&self) -> Option<Arc<Vec<usize>>> {
        self.inner
            .video_data
            .read()
            .unwrap()
            .gmax_frame_indexes
            .clone()
    }
}

impl<S: SettingStorage> VideoManagerInner<S> {
    #[instrument(skip(self), err)]
    fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        for spin_count in 0..20 {
            let video_data = self.video_data.read().unwrap();

            let VideoMeta {
                nframes,
                shape: (h, w),
                ..
            } = self.setting_storage.lock().unwrap().video_meta()?;
            if frame_index >= nframes {
                // This is an invalid `frame_index` from frontend and will never get the frame.
                // So directly abort it.
                bail!("frame_index({}) out of range({})", frame_index, nframes);
            }

            if let Some(packet) = video_data.packets.get(frame_index) {
                let _span = info_span!("decode_single_frame", spin_count).entered();

                let mut decoder = video_data.decoder_manager.decoder()?;
                let mut buf = Vec::new();
                let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                jpeg_encoder.encode(decoder.decode(packet)?.data(0), w as u32, h as u32, Rgb8)?;

                return Ok(base64::encode(buf));
            }
            let remaining_nframes = frame_index - video_data.packets.len();

            // Lock must be released here:
            // > The priority policy of the lock is dependent on the underlying operating
            // system’s implementation, and this type does not guarantee that any particular
            // policy will be used. In particular, a writer which is waiting to acquire the
            // lock in write might or might not block concurrent calls to read.
            drop(video_data);

            // Adaptive pause according to the remaining frame numbers.
            std::thread::sleep(Duration::from_millis((remaining_nframes as u64 >> 2) + 50));
        }

        bail!("run out of attempts")
    }

    #[instrument(skip_all, err)]
    fn build_green2(&self, tx: oneshot::Sender<()>) -> Result<()> {
        let (green2_meta, green2) = {
            let video_data = self.video_data.read().unwrap();

            let setting_storage = self.setting_storage.lock().unwrap();
            let video_meta = setting_storage.video_meta()?;
            let green2_meta = setting_storage.green2_meta()?;
            drop(setting_storage);

            if video_data.packets.len() < video_meta.nframes {
                bail!("video not loaded yet");
            }
            // Tell outside that building actually started.
            tx.send(()).unwrap();

            let green2 = video_data.decode_all(
                &video_meta,
                &green2_meta,
                &self.build_green2_progress_bar,
            )?;

            (green2_meta, green2)
        };

        let mut video_data = self.video_data.write().unwrap();
        let current_green2_meta = self.setting_storage.lock().unwrap().green2_meta()?;
        if current_green2_meta != green2_meta {
            bail!(
                "setting has been changed while building green2, old: {:?}, current: {:?}",
                green2_meta,
                current_green2_meta,
            );
        }
        video_data.green2 = Some(green2);

        Ok(())
    }

    #[instrument(skip_all, err)]
    fn detect_peak(&self, tx: oneshot::Sender<()>) -> Result<()> {
        let (filter_meta, gmax_frame_indexes) = {
            let video_data = self.video_data.read().unwrap();
            let green2 = video_data
                .green2
                .as_ref()
                .ok_or_else(|| anyhow!("green2 not built yet"))?
                .view();
            let filter_meta = self.setting_storage.lock().unwrap().filter_meta()?;
            // Tell outside that peak detection actually started.
            tx.send(()).unwrap();
            let filtered_green2 = filter::filter_detect_peak(
                green2,
                filter_meta.filter_method,
                &self.detect_peak_progress_bar,
            )?;

            (filter_meta, filtered_green2)
        };

        let mut video_data = self.video_data.write().unwrap();
        let current_filter_meta = self.setting_storage.lock().unwrap().filter_meta()?;
        if current_filter_meta != filter_meta {
            bail!(
                "setting has been changed while detecting peaks, old: {:?}, current: {:?}",
                filter_meta,
                current_filter_meta,
            );
        }
        video_data.gmax_frame_indexes = Some(Arc::new(gmax_frame_indexes));

        Ok(())
    }

    #[instrument(skip(self), err)]
    fn filter_single_point(&self, (y, x): (usize, usize)) -> Result<Vec<u8>> {
        let video_data = self.video_data.read().unwrap();
        let (_, _, h, w) = self.setting_storage.lock().unwrap().green2_meta()?.area;
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
            .column(position);
        let filter_method = self
            .setting_storage
            .lock()
            .unwrap()
            .filter_meta()?
            .filter_method;

        Ok(filter::filter_single_point(filter_method, green1))
    }
}

impl VideoData1 {
    fn reset(&mut self, parameters: Parameters) {
        self.packets.clear();
        // self.decoder_manager.reset(parameters);
        self.green2 = None;
        self.gmax_frame_indexes = None;
    }

    #[instrument(skip(self, video_meta, progress_bar))]
    fn decode_all(
        &self,
        video_meta: &VideoMeta,
        green2_meta: &Green2Meta,
        progress_bar: &ProgressBar,
    ) -> Result<Array2<u8>> {
        let cal_num = green2_meta.cal_num;
        let byte_w = video_meta.shape.1 * 3;
        let (tl_y, tl_x, cal_h, cal_w) = green2_meta.area;

        let _reset_guard = progress_bar.start(cal_num as u32);

        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));
        self.packets
            .par_iter()
            .skip(green2_meta.start_frame)
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

                // This does not add any noticeable overhead.
                progress_bar.add(1)?;

                Ok(())
            })?;

        debug_assert_matches!(
            progress_bar.progress(),
            Progress::Finished { total } if total == cal_num as u32,
        );

        Ok(green2)
    }
}

#[cfg(test)]
mod tests {
    use std::thread::spawn;

    use crossbeam::channel::bounded;

    use crate::util;

    use super::*;

    const VIDEO_PATH_SAMPLE: &str = "./tests/almost_empty.avi";
    const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";
    fn _video_meta_sample() -> VideoMeta {
        VideoMeta {
            path: PathBuf::from(VIDEO_PATH_SAMPLE),
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
        }
    }
    fn _video_meta_real() -> VideoMeta {
        VideoMeta {
            path: PathBuf::from(VIDEO_PATH_REAL),
            frame_rate: 25,
            nframes: 2444,
            shape: (1024, 1280),
        }
    }

    #[test]
    fn test_read_video_sample() {
        util::log::init();

        let (tx1, rx1) = oneshot::channel();
        let (tx2, rx2) = bounded(3);
        spawn(move || read_video(VIDEO_PATH_SAMPLE, tx1, tx2).unwrap());

        let (video_meta, _) = rx1.blocking_recv().unwrap();
        let video_meta_sample = _video_meta_sample();
        assert_eq!(video_meta, video_meta_sample,);
        let mut cnt = 0;
        for (_, packet) in rx2 {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, video_meta_sample.nframes);
    }

    #[ignore]
    #[test]
    fn test_read_video_real() {
        util::log::init();

        let (tx1, rx1) = oneshot::channel();
        let (tx2, rx2) = bounded(3);
        spawn(move || read_video(VIDEO_PATH_REAL, tx1, tx2).unwrap());

        let (video_meta, _) = rx1.blocking_recv().unwrap();
        let video_meta_real = _video_meta_real();
        assert_eq!(video_meta, video_meta_real);
        let mut cnt = 0;
        for (_, packet) in rx2 {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, video_meta_real.nframes);
    }
}

// #[cfg(test)]
// mod tests {
//     use std::time::Duration;
//
//     use mockall::predicate::eq;
//
//     use super::*;
//     use crate::{setting::MockSettingStorage, util};
//
//     const SAMPLE_VIDEO_PATH: &str = "./tests/almost_empty.avi";
//     const VIDEO_PATH: &str =
//         "/home/yhj/Downloads/2021_YanHongjie/EXP/imp/videos/imp_20000_1_up.avi";
//     const VIDEO_PATH1: &str =
//         "/home/yhj/Downloads/2021_YanHongjie/EXP/imp/videos/imp_20000_2_up.avi";
//
//     #[tokio::test]
//     async fn test_full_fake() {
//         let video_meta = VideoMeta {
//             path: PathBuf::from(SAMPLE_VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 3,
//             shape: (1024, 1280),
//         };
//         let green2_meta = Green2Meta {
//             start_frame: 1,
//             cal_num: 2,
//             area: (10, 10, 600, 800),
//             video_path: video_meta.path.to_owned(),
//         };
//
//         full(video_meta, green2_meta).await;
//     }
//
//     #[tokio::test]
//     #[ignore]
//     async fn test_full_real() {
//         let video_meta = VideoMeta {
//             path: PathBuf::from(VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 2444,
//             shape: (1024, 1280),
//         };
//         let green2_meta = Green2Meta {
//             start_frame: 10,
//             cal_num: 2000,
//             area: (10, 10, 600, 800),
//             video_path: video_meta.path.to_owned(),
//         };
//
//         full(video_meta, green2_meta).await;
//     }
//
//     async fn full(video_meta: VideoMeta, green2_meta: Green2Meta) {
//         util::log::init();
//
//         let video_path = video_meta.path.clone();
//         let nframes = video_meta.nframes;
//         let filter_meta = FilterMeta {
//             filter_method: FilterMethod::Median { window_size: 20 },
//             green2_meta: green2_meta.clone(),
//         };
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_set_video_meta()
//             .with(eq(video_meta.clone()))
//             .return_once(|_| Ok(()));
//         mock.expect_video_meta()
//             .returning(move || Ok(video_meta.clone()));
//         mock.expect_green2_meta()
//             .returning(move || Ok(green2_meta.clone()));
//         mock.expect_filter_meta()
//             .returning(move || Ok(filter_meta.clone()));
//
//         let video_manager = VideoManager::new(Arc::new(Mutex::new(mock)));
//         video_manager
//             .spawn_read_video(Some(video_path))
//             .await
//             .unwrap();
//
//         tokio::try_join!(
//             video_manager.read_single_frame_base64(0),
//             video_manager.read_single_frame_base64(1),
//             video_manager.read_single_frame_base64(2),
//         )
//         .unwrap();
//
//         // Wait until all frames has been loaded.
//         video_manager
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//
//         video_manager.spawn_build_green2().await.unwrap();
//         loop {
//             match video_manager.build_green2_progress() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("building green2...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//
//         while video_manager
//             .inner
//             .video_data
//             .read()
//             .unwrap()
//             .green2
//             .is_none()
//         {
//             tokio::time::sleep(Duration::from_millis(10)).await;
//         }
//
//         tokio::try_join!(
//             video_manager.filter_single_point((100, 100)),
//             video_manager.filter_single_point((500, 500)),
//         )
//         .unwrap();
//
//         video_manager.spawn_detect_peak().await.unwrap();
//         loop {
//             match video_manager.detect_peak_progress_bar() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("detecting peaks...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//
//         while video_manager
//             .inner
//             .video_data
//             .read()
//             .unwrap()
//             .gmax_frame_indexes
//             .is_none()
//         {
//             tokio::time::sleep(Duration::from_millis(10)).await;
//         }
//     }
//
//     #[tokio::test]
//     #[ignore]
//     async fn test_interrupt_build_green2_by_video_change() {
//         util::log::init();
//
//         let video_meta = Arc::new(Mutex::new(VideoMeta {
//             path: PathBuf::from(VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 2444,
//             shape: (1024, 1280),
//         }));
//         let green2_meta = Green2Meta {
//             start_frame: 1,
//             cal_num: 2000,
//             area: (10, 10, 600, 800),
//             video_path: video_meta.lock().unwrap().path.to_owned(),
//         };
//
//         let mut mock = MockSettingStorage::new();
//
//         {
//             let video_meta = video_meta.clone();
//             mock.expect_set_video_meta()
//                 .returning(move |new_video_meta| {
//                     *video_meta.lock().unwrap() = new_video_meta.clone();
//                     Ok(())
//                 });
//         }
//         {
//             let video_meta = video_meta.clone();
//             mock.expect_video_meta()
//                 .returning(move || Ok(video_meta.lock().unwrap().clone()));
//         }
//         mock.expect_green2_meta()
//             .return_once(move || Ok(green2_meta));
//
//         let video_manager = VideoManager::new(Arc::new(Mutex::new(mock)));
//         video_manager.spawn_read_video(None).await.unwrap();
//         let nframes = video_meta.lock().unwrap().nframes;
//         video_manager
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//
//         video_manager.spawn_build_green2().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         match video_manager.build_green2_progress() {
//             Progress::Uninitialized => unreachable!(),
//             Progress::InProgress { total, count } => {
//                 println!("building green2...... {count}/{total}");
//             }
//             Progress::Finished { .. } => unreachable!(),
//         }
//
//         // Update video path, interrupt building green2.
//         video_manager
//             .spawn_read_video(Some(PathBuf::from(VIDEO_PATH1)))
//             .await
//             .unwrap();
//         let nframes = video_meta.lock().unwrap().nframes;
//         video_manager
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//     }
//
//     #[tokio::test]
//     #[ignore]
//     async fn test_interrupt_build_green2_by_parameter_change() {
//         util::log::init();
//
//         let video_meta = VideoMeta {
//             path: PathBuf::from(VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 2444,
//             shape: (1024, 1280),
//         };
//         let nframes = video_meta.nframes;
//         let green2_meta = Arc::new(Mutex::new(Green2Meta {
//             start_frame: 1,
//             cal_num: 2000,
//             area: (10, 10, 600, 800),
//             video_path: video_meta.path.to_owned(),
//         }));
//
//         let mut mock = MockSettingStorage::new();
//
//         mock.expect_set_video_meta().returning(move |_| Ok(()));
//         let video_meta = video_meta.clone();
//         mock.expect_video_meta()
//             .returning(move || Ok(video_meta.clone()));
//         {
//             let green2_meta = green2_meta.clone();
//             mock.expect_green2_meta()
//                 .returning(move || Ok(green2_meta.lock().unwrap().clone()));
//         }
//
//         let video_manager = VideoManager::new(Arc::new(Mutex::new(mock)));
//         video_manager.spawn_read_video(None).await.unwrap();
//         video_manager
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//
//         video_manager.spawn_build_green2().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         match video_manager.build_green2_progress() {
//             Progress::Uninitialized => unreachable!(),
//             Progress::InProgress { total, count } => {
//                 println!("building green2 old...... {count}/{total}");
//             }
//             Progress::Finished { .. } => unreachable!(),
//         }
//
//         green2_meta.lock().unwrap().start_frame = 10;
//         video_manager.spawn_build_green2().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         loop {
//             match video_manager.build_green2_progress() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("building green2 new...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//     }
//
//     #[tokio::test]
//     #[ignore]
//     async fn test_interrupt_detect_peak_by_parameter_change() {
//         util::log::init();
//
//         let video_meta = VideoMeta {
//             path: PathBuf::from(VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 2444,
//             shape: (1024, 1280),
//         };
//         let nframes = video_meta.nframes;
//         let green2_meta = Green2Meta {
//             start_frame: 1,
//             cal_num: 2000,
//             area: (10, 10, 600, 800),
//             video_path: video_meta.path.clone(),
//         };
//         let filter_meta = Arc::new(Mutex::new(FilterMeta {
//             filter_method: FilterMethod::Wavelet {
//                 threshold_ratio: 0.8,
//             },
//             green2_meta: green2_meta.clone(),
//         }));
//
//         let mut mock = MockSettingStorage::new();
//
//         mock.expect_set_video_meta().returning(move |_| Ok(()));
//         let video_meta = video_meta.clone();
//         mock.expect_video_meta()
//             .returning(move || Ok(video_meta.clone()));
//         mock.expect_green2_meta()
//             .returning(move || Ok(green2_meta.clone()));
//         {
//             let filter_meta = filter_meta.clone();
//             mock.expect_filter_meta()
//                 .returning(move || Ok(filter_meta.lock().unwrap().clone()));
//         }
//
//         let video_manager = VideoManager::new(Arc::new(Mutex::new(mock)));
//         video_manager.spawn_read_video(None).await.unwrap();
//         video_manager
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//
//         video_manager.spawn_build_green2().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         loop {
//             match video_manager.build_green2_progress() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("building green2...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         video_manager.spawn_detect_peak().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(500)).await;
//
//         filter_meta.lock().unwrap().filter_method = FilterMethod::Median { window_size: 10 };
//
//         video_manager.spawn_detect_peak().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         loop {
//             match video_manager.detect_peak_progress_bar() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("detecting peaks...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//     }
// }
