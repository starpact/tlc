use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::Arc,
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
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use thread_local::ThreadLocal;
use tokio::sync::{oneshot, Semaphore};
use tracing::{debug, error};

use crate::util;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VideoMetadata {
    /// Path of TLC video file.
    path: PathBuf,
    /// `frame_rate`, `total_frames`, `shape` is determined once the video(path)
    /// is determined, so we do not deserialize them from the config file but
    /// always directly get them from video stat. However, they are still recorded
    /// in the config file because they might be useful information for users.
    /// Frame rate of video.
    #[serde(skip_deserializing)]
    frame_rate: usize,
    /// Total frames of video.
    #[serde(skip_deserializing)]
    nframes: usize,
    /// (video_height, video_width)
    #[serde(skip_deserializing)]
    shape: (u32, u32),
}

pub async fn read_video<P: AsRef<Path>>(
    video_cache: Arc<RwLock<VideoCache>>,
    video_path: P,
) -> Result<VideoMetadata> {
    let path = video_path.as_ref().to_owned();
    let (tx, rx) = oneshot::channel();

    let join_handle = tokio::task::spawn_blocking(move || -> Result<()> {
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

        if let Err(_) = tx.send(VideoMetadata {
            path: path.clone(),
            frame_rate,
            nframes,
            shape: (decoder.height(), decoder.width()),
        }) {
            error!("The receiver has been dropped");
        }

        // `packet_cache` is reset before start reading from file.
        video_cache.write().reset(&path, parameters, nframes);

        let mut packet_iter = input.packets();
        let mut cnt = 0;
        loop {
            // `RwLockWriteGuard` is intentionally holden all the way during
            // reading *each* frame to avoid busy loop within `get_frame`.
            let mut vc = video_cache.write();

            if let Some((stream, packet)) = packet_iter.next() {
                if stream.index() != video_stream_index {
                    continue;
                }
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

async fn get_frame(video_cache: Arc<RwLock<VideoCache>>, frame_index: usize) -> Result<String> {
    static PENDING_COUNTER: Semaphore = Semaphore::const_new(3);
    let _counter = PENDING_COUNTER
        .try_acquire()
        .map_err(|e| anyhow!("busy: {}", e))?;

    // `get_frame` is regarded as synchronous blocking because:
    // 1. When the targeted frame is not loaded yet, it will block on the `RWLock`.
    // 2. Decoding will take some time(10~20ms) even for a single frame.
    // So this task should be executed in `tokio::task::spawn_blocking` or `rayon::spawn`,
    // here we must use `rayon::spawn` because the `thread-local` decoder is designed
    // to be kept in thread from rayon thread pool.
    util::blocking::compute(move || {
        let vc = video_cache.read();
        vc.worth_waiting(frame_index)?;
        if let Some(packet) = vc.packets.get(frame_index) {
            let mut decoder = vc.decoder_cache.get()?;
            let (h, w) = decoder.shape();
            // This pre-alloc size is just an empirical estimate.
            let mut buf = Vec::with_capacity((h * w) as usize);
            let jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
            jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

            Ok(base64::encode(buf));
        }
    })
    .await?
}

#[derive(Default)]
pub struct VideoCache {
    state: State,
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
    pub packets: Vec<Packet>,
}

#[derive(Debug)]
enum State {
    Uninitialized,
    Reading {
        /// Identifier of the current video.
        path: PathBuf,
        /// Total packet/frame number of the current video, which is
        /// used to validate the `frame_index` parameter of `get_frame`.
        nframes: usize,
    },
    Finished,
}

impl Default for State {
    fn default() -> Self {
        Self::Uninitialized
    }
}

#[derive(Default)]
struct DecoderCache {
    parameters: Mutex<codec::Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,
}

impl VideoCache {
    fn reset<P: AsRef<Path>>(
        &mut self,
        path: P,
        parameters: codec::Parameters,
        total_frames: usize,
    ) {
        self.state = State::Reading {
            path: path.as_ref().to_owned(),
            nframes: total_frames,
        };
        self.decoder_cache.reset(parameters);
        self.packets.clear();
    }

    fn target_changed<P: AsRef<Path>>(&self, original_path: P) -> bool {
        match self.state {
            State::Reading { ref path, .. } => original_path.as_ref() != path.as_path(),
            ref state @ _ => unreachable!("invalid state: {:?}", state),
        }
    }

    fn worth_waiting(&self, frame_index: usize) -> Result<()> {
        match self.state {
            State::Reading {
                nframes: total_frames,
                ..
            } => {
                if frame_index >= total_frames {
                    // This is an invalid `frame_index` from frontend and will never get the frame.
                    // So directly abort current thread.
                    bail!("frame_index({}) out of range", frame_index)
                }
                Ok(())
            }
            State::Finished => Ok(()),
            State::Uninitialized => bail!("video path unset"),
        }
    }

    fn finished(&self) -> bool {
        matches!(self.state, State::Finished)
    }

    fn mark_finished(&mut self) {
        self.state = State::Finished;
    }
}

impl DecoderCache {
    fn reset(&mut self, parameters: codec::Parameters) {
        self.parameters = Mutex::new(parameters);
        self.decoders.clear();
    }

    fn get(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.parameters.lock().clone())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }
}

/// wrap `Context` to pass between threads(because of the raw pointer)
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

pub struct Decoder {
    codec_ctx: ffmpeg::decoder::Video,
    sws_ctx: SendableSwsCtx,
    /// `src_frame` and `dst_frame` are used to avoid frequent allocation.
    /// This can speed up decoding by about 10%.
    src_frame: Video,
    dst_frame: Video,
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
