use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex, RwLock, RwLockReadGuard,
    },
};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{
    codec,
    codec::packet::Packet,
    format::Pixel::RGB24,
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use ffmpeg_next as ffmpeg;
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{parallel::prelude::*, prelude::*};
use rayon::ThreadPoolBuilder;
use serde::{Deserialize, Serialize};
use thread_local::ThreadLocal;
use tokio::sync::oneshot;
use tracing::{debug, info};

use crate::util;

pub struct VideoDataManager {
    video_data_manager: Arc<VideoDataManagerInner>,
    frame_reader: FrameReader,
}

#[derive(Default)]
struct VideoDataManagerInner {
    video_data: RwLock<VideoData>,
    build_progress: BuildProgress,
    filter_progress: AtomicI64,
}

#[derive(Default)]
pub struct VideoData {
    video_cache: VideoCache,
    green2: Option<Array2<u8>>,
    filtered_green2: Option<Array2<u8>>,
}

pub struct FrameReader {
    inner: flume::Sender<(usize, oneshot::Sender<Result<String>>)>,
}

#[derive(Default)]
struct BuildProgress {
    inner: AtomicI64,
}

#[derive(Default)]
pub struct VideoCache {
    path: Option<PathBuf>,

    /// Total packet/frame number of the current video, which is used
    /// to validate the `frame_index` parameter of `read_single_frame`.
    nframes: usize,

    /// Cache thread-local decoder.
    decoder_cache: DecoderCache,

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

#[derive(Default)]
struct DecoderCache {
    parameters: Mutex<codec::Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,
}

struct Decoder {
    codec_ctx: ffmpeg::decoder::Video,
    sws_ctx: SendableSwsCtx,

    /// `src_frame` and `dst_frame` are used to avoid frequent allocation.
    /// This can speed up decoding by about 10%.
    src_frame: Video,
    dst_frame: Video,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VideoMetadata {
    /// Path of TLC video file.
    pub path: PathBuf,

    /// `frame_rate`, `total_frames`, `shape` is determined once the video(path)
    /// is determined, so we do not deserialize them from the config file but
    /// always directly get them from video stat. However, they are still recorded
    /// in the config file because they might be useful information for users.
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
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (usize, usize, usize, usize),
}

impl VideoDataManager {
    pub fn new() -> Self {
        let inner = Arc::new(VideoDataManagerInner::default());
        Self {
            frame_reader: FrameReader::new(inner.clone()),
            video_data_manager: inner,
        }
    }

    pub fn data(&self) -> RwLockReadGuard<VideoData> {
        self.video_data_manager.video_data.read().unwrap()
    }

    /// Spawn a thread to load all video packets into memory.
    pub async fn spawn_load_packets<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMetadata> {
        let video_data_manager = self.video_data_manager.clone();

        let path = video_path.as_ref().to_owned();
        info!("video_path: {:?}", path);
        let (tx, rx) = oneshot::channel();

        let join_handle = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut timer = util::timing::start("loading packets");

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

            tx.send(VideoMetadata {
                path: path.clone(),
                frame_rate,
                nframes,
                shape: (decoder.height() as usize, decoder.width() as usize),
            })
            .expect("The receiver has been dropped");

            video_data_manager
                .video_data
                .write()
                .unwrap()
                .video_cache
                .init(&path, nframes, parameters);

            let mut cnt = 0;
            for (stream, packet) in input.packets() {
                // `RwLockWriteGuard` is intentionally holden all the way during
                // reading *each* frame to avoid busy loop within `read_single_frame`.
                let video_cache = &mut video_data_manager.video_data.write().unwrap().video_cache;
                if stream.index() != video_stream_index {
                    continue;
                }
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
        });

        match rx.await {
            Ok(t) => Ok(t),
            Err(_) => Err(join_handle
                .await?
                .map_err(|e| anyhow!("failed to read video from {:?}: {}", video_path.as_ref(), e))
                .unwrap_err()),
        }
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        self.frame_reader.read_single_frame(frame_index).await
    }

    pub fn spawn_build_green2(&self, green2_param: Green2Param) {
        let video_data_manager = self.video_data_manager.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = video_data_manager.build_green2(green2_param) {
                debug!("{}", e);
            }
        });
    }
}

impl VideoDataManagerInner {
    pub fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        loop {
            let video_cache = &self.video_data.read().unwrap().video_cache;
            if !video_cache.initialized() {
                bail!("uninitialized");
            }
            if frame_index >= video_cache.nframes {
                // This is an invalid `frame_index` from frontend and will never get the frame.
                // So directly abort it.
                bail!(
                    "frame_index({}) out of range({})",
                    frame_index,
                    video_cache.nframes
                );
            }
            if let Some(packet) = video_cache.packets.get(frame_index) {
                let mut decoder = video_cache.decoder_cache.get()?;
                let (h, w) = decoder.shape();
                let mut buf = Vec::new();
                let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
                jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

                break Ok(base64::encode(buf));
            }
        }
    }

    pub fn build_green2(&self, green2_param: Green2Param) -> Result<()> {
        let cal_num = green2_param.cal_num as u32;
        self.build_progress.start(cal_num);
        let ret = self._build_green2(green2_param);
        match ret {
            Ok(_) => {
                let x = self.build_progress.inner.load(Ordering::Relaxed);
                if x > 0 {
                    let (total, count) = split_i64(x);
                    debug_assert_eq!(total, cal_num);
                    debug_assert_eq!(total, count);
                }
            }
            Err(_) => self.build_progress.reset(),
        }

        ret
    }

    fn _build_green2(&self, green2_param: Green2Param) -> Result<()> {
        let mut timer = util::timing::start("building green2");

        // Wait until all packets have been loaded.
        let video_data = loop {
            let video_data = self.video_data.read().unwrap();
            if video_data.video_cache.finished() {
                break video_data;
            }
        };
        let video_cache = &video_data.video_cache;

        let byte_w = video_cache.decoder_cache.get()?.shape().1 as usize * 3;
        let (tl_y, tl_x, h, w) = green2_param.area;
        let (tl_y, tl_x, h, w) = (tl_y as usize, tl_x as usize, h as usize, w as usize);
        let mut green2 = Array2::zeros((green2_param.cal_num, h * w));

        video_cache
            .packets
            .par_iter()
            .skip(green2_param.start_frame)
            .zip(green2.axis_iter_mut(Axis(0)).into_iter())
            .try_for_each(|(packet, mut row)| -> Result<()> {
                let mut decoder = video_cache.decoder_cache.get()?;
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

                self.build_progress.add(1)?;

                Ok(())
            })?;
        timer.finish();

        drop(video_data);
        self.video_data.write().unwrap().green2 = Some(green2);

        Ok(())
    }
}

impl VideoData {
    pub fn green2(&self) -> Option<ArrayView2<u8>> {
        Some(self.green2.as_ref()?.view())
    }

    pub fn filtered_green2(&self) -> Option<ArrayView2<u8>> {
        Some(self.filtered_green2.as_ref()?.view())
    }
}

impl FrameReader {
    fn new(video_data_manager: Arc<VideoDataManagerInner>) -> Self {
        let (tx, rx) = flume::unbounded();
        let frame_reader = Self { inner: tx };

        std::thread::spawn(move || {
            // As thread-local decoders are designed to be kept in just a few threads,
            // so a standalone `rayon` thread pool is used.
            // `spawn` form `rayon`'s global thread pool will block when something like
            // `par_iter` is working as `rayon` uses `depth-first` strategy for highest
            // efficiency. So another dedicated thread pool is used.
            const NUM_THREADS: usize = 4;
            let thread_pool = ThreadPoolBuilder::new()
                .num_threads(NUM_THREADS)
                .build()
                .expect("Failed to init rayon thread pool");

            // When user drags the progress bar quickly, the decoding can not keep up
            // and there will be significant lag. Actually, we do not have to decode
            // every frames, and the key is how to give up decoding some frames properly.
            // The naive solution to avoid too much backlog is maintaining the number of
            // pending tasks and directly abort current decoding if it already exceeds the
            // limit. But it's not perfect for this use case because it can not guarantee
            // decoding the frame where the progress bar **stops**.
            // To solve this, we introduce an unbounded channel to accept all frame indexes
            // but only **the latest few** will be actually decoded.
            while let Ok((frame_index, tx)) = rx.recv() {
                let video_data_manager = video_data_manager.clone();
                thread_pool.spawn(move || {
                    let ret = video_data_manager.read_single_frame(frame_index);
                    let _ = tx.send(ret);
                });

                loop {
                    let len = rx.len();
                    if len <= NUM_THREADS {
                        break;
                    }
                    for _ in 0..len - NUM_THREADS {
                        let _ = rx.recv();
                    }
                }
            }
        });

        frame_reader
    }

    async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .send((frame_index, tx))
            .expect("Frame grab daemon exited unexptedly");
        rx.await.map_err(|_| anyhow!("no idle worker thread"))?
    }
}

/// Higher 32 bits: total, lower 32 bits: count.
fn split_i64(x: i64) -> (u32, u32) {
    ((x >> 32) as u32, x as u32)
}

fn idle(x: i64) -> bool {
    let (total, count) = split_i64(x);
    // Initial state: total == count == 0
    // Already finished: total == count
    // So we just check if total == count
    total == count
}

impl BuildProgress {
    fn get_progress(&self) -> (u32, u32) {
        split_i64(self.inner.load(Ordering::Relaxed))
    }

    fn reset(&self) {
        self.inner.store(0, Ordering::Relaxed);
    }

    fn start(&self, new_total: u32) {
        if self
            .inner
            .fetch_update(Ordering::SeqCst, Ordering::Acquire, |x| {
                idle(x).then_some((new_total as i64) << 32)
            })
            .is_err()
        {
            self.interrupt();
        }
    }

    fn add(&self, n: i64) -> Result<()> {
        if self.inner.fetch_add(n, Ordering::Relaxed) < 0 {
            bail!("interrupted");
        }
        Ok(())
    }

    fn interrupt(&self) {
        self.inner.store(i64::MIN, Ordering::Relaxed);
        for i in 0.. {
            std::thread::sleep(std::time::Duration::from_millis(1));
            if idle(self.inner.load(Ordering::Relaxed)) {
                debug!("Interrupt after {} checks", i);
                break;
            }
        }
    }
}

impl VideoCache {
    fn init<P: AsRef<Path>>(&mut self, path: P, nframes: usize, parameters: codec::Parameters) {
        self.path = Some(path.as_ref().to_owned());
        self.nframes = nframes;
        self.decoder_cache.init(parameters);
        self.packets.clear();
    }

    fn target_changed<P: AsRef<Path>>(&self, original_path: P) -> bool {
        !matches!(&self.path, Some(path) if path == original_path.as_ref())
    }

    fn initialized(&self) -> bool {
        self.path.is_some()
    }

    fn finished(&self) -> bool {
        self.nframes == self.packets.len()
    }
}

impl DecoderCache {
    fn init(&mut self, parameters: codec::Parameters) {
        self.parameters = Mutex::new(parameters);
        self.decoders.clear();
    }

    fn get(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.parameters.lock().unwrap().clone())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }
}

/// Wrap `Context` to pass between threads(because of the raw pointer).
struct SendableSwsCtx(scaling::Context);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for SendableSwsCtx {}

impl Deref for SendableSwsCtx {
    type Target = scaling::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendableSwsCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Decoder {
    fn new(parameters: codec::Parameters) -> Result<Self> {
        let codec_ctx = codec::Context::from_parameters(parameters)?
            .decoder()
            .video()?;
        let (h, w) = (codec_ctx.height(), codec_ctx.width());
        let sws_ctx =
            scaling::Context::get(codec_ctx.format(), w, h, RGB24, w, h, Flags::BILINEAR)?;

        Ok(Self {
            codec_ctx,
            sws_ctx: SendableSwsCtx(sws_ctx),
            src_frame: Video::empty(),
            dst_frame: Video::empty(),
        })
    }

    fn decode(&mut self, packet: &Packet) -> Result<&Video> {
        self.codec_ctx.send_packet(packet)?;
        self.codec_ctx.receive_frame(&mut self.src_frame)?;
        self.sws_ctx.run(&self.src_frame, &mut self.dst_frame)?;

        Ok(&self.dst_frame)
    }

    fn shape(&self) -> (u32, u32) {
        (self.codec_ctx.height(), self.codec_ctx.width())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_packets() {
        util::log::init();
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
        util::log::init();
        let video_data_manager = VideoDataManager::new();
        let video_metadata = video_data_manager
            .spawn_load_packets("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();

        let last_frame_index = video_metadata.nframes - 1;
        println!("waiting for frame {}", last_frame_index);
        let frame_str = video_data_manager
            .read_single_frame(last_frame_index)
            .await
            .unwrap();
        println!("{}", frame_str.len());

        assert_eq!(
            format!(
                "frame_index({nframes}) out of range({nframes})",
                nframes = video_metadata.nframes
            ),
            video_data_manager
                .read_single_frame(video_metadata.nframes)
                .await
                .unwrap_err()
                .to_string(),
        );
    }

    #[tokio::test]
    async fn test_build_green2() {
        util::log::init();
        let video_data_manager = VideoDataManager::new();
        video_data_manager
            .spawn_load_packets("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();
        video_data_manager.spawn_build_green2(Green2Param {
            start_frame: 0,
            cal_num: 2000,
            area: (0, 0, 1000, 800),
        });
        println!(
            "{:?}",
            video_data_manager
                .video_data_manager
                .video_data
                .read()
                .unwrap()
                .green2
        );
    }
}
